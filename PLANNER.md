# Planner Module

Transforms a `QueryRequest` into a logical `PlanNode` tree that can be emitted
as Substrait or SQL.

## Module Map

```
planner/
├── plan.rs      Router — classifies query, dispatches to builders
├── table.rs     Single-dataset plans (Scan → Join → Filter → Aggregate → Project → Sort)
├── cross.rs     Cross-grain-set metrics (UNION branches → re-aggregate)
├── union.rs     Conformed / qualified / partitioned / virtual-only UNIONs
├── join.rs      Multi-table JOIN within one grain set
├── expr.rs      Semantic model expressions → plan expressions
├── util.rs      Shared helpers (column builders, dimension parsing, virtual values)
└── error.rs     PlanError
```

## Decision Tree

`plan_semantic_query` (in `plan.rs`) is the single entry point. It inspects the
query and routes to exactly one planning path:

```
plan_semantic_query
│
├─ cross-grain-set metric (1)?
│  └─► cross::plan_cross_grain_set_query
│
├─ cross-grain-set metrics (>1)?
│  └─► cross::plan_multi_cross_grain_set_query
│
├─ qualified groups > 1?  (e.g. "adwords.dates.year" + "facebookads.dates.year")
│  └─► union::plan_multi_grain_set_query          UNION (tier 1/2/3 per group, NULL for missing)
│
├─ qualified group == 1?  (e.g. "adwords.dates.year")
│  └─► union::plan_single_grain_set_query          constrain to that group
│
└─ unqualified (normal path)
   │
   ├─ select_datasets OK
   │  ├─ partitioned (multiple datasets with partition)?
   │  │  └─► union::plan_partitioned_union           UNION ALL per partition
   │  ├─ conformed dimensions + multiple groups?
   │  │  └─► union::plan_conformed_query             UNION across groups (tier 1/2/3 per group)
   │  └─ single dataset
   │     └─► resolve → table::plan_query             standard path
   │
   └─ select_datasets FAIL
      ├─ conformed + multiple groups?
      │  └─► union::plan_conformed_query             (tries tier 2 join, then tier 3 partial)
      └─ try multi-table JOIN
         ├─ only 1 table needed after all?
         │  └─► resolve → table::plan_query
         └─ multiple tables
            └─► join::plan_same_grain_set_join      FULL OUTER JOIN
```

**Conformed query (per grain set):** `plan_conformed_query` builds one branch per grain set using a three-tier policy. Required measures are taken from **all** requested metrics (including structured e.g. CASE) via `resolver::collect_required_measure_names`.

- **Tier 1** — One feasible table (all dimensions + all measures) → `resolve` + `table::plan_query`.
- **Tier 2** — No single table → `selector::select_datasets_for_join` for that grain set; if OK, one branch (resolve + plan_query) or multiple tables → `join::plan_same_grain_set_join`.
- **Tier 3** — No join feasible → `selector::select_partial_for_grain_set` picks the best partial dataset; `build_partial_union_branch` produces a branch with the same output schema but **NULL** for missing dimensions and measures (typed; no data dropped).

**Qualified multi-grain-set query** (`plan_multi_grain_set_query`): Uses the same three-tier policy per qualified grain set. If a grain set has no table with a requested measure (e.g. adwords without `clicks`), tier 3 adds a branch with that measure as NULL instead of erroring or dropping the slice.

## Plan Shapes

Each path produces a different plan tree. The leaves are always `Scan` nodes;
the roots are usually `Sort` or `Project`.

**Standard (single table)**
```
Sort
└── Project (dimensions + metrics)
    └── Aggregate (GROUP BY dimensions, agg measures)
        └── Join* (LEFT JOIN dimension tables)
            └── Scan (fact table)
```

**Cross-grain-set metric**
```
Sort
└── Aggregate (re-aggregate: SUM metric by dimensions)
    └── Union
        ├── Project (dims + metric, NULLs for other groups)
        │   └── Aggregate → Join → Scan   [group A]
        └── Project
            └── Aggregate → Join → Scan   [group B]
```

**Conformed dimension UNION**
```
Union
├── Sort → Project → Aggregate → Join → Scan   [group A — full or partial]
├── Sort → Project → Aggregate → Join → Scan   [group B]
└── Project (dims + metrics, NULL for missing) → Aggregate → Join → Scan   [group C — tier 3 partial]
```
Branches may be full (tier 1/2) or partial (tier 3): same output columns, with NULL for missing dimensions/measures on partial branches.

**Partitioned UNION ALL**
```
Union
├── Sort → Project → Aggregate → Join → Scan   [partition 1]
└── Sort → Project → Aggregate → Join → Scan   [partition 2]
```

**Multi-table JOIN (same group)**
```
Sort
└── Project (COALESCE dims, pick measures from owning table)
    └── Join (FULL OUTER on dimensions)
        ├── Project → Aggregate → Scan   [table 1]
        └── Project → Aggregate → Scan   [table 2]
```

**Virtual-only (no table scan)**
```
VirtualTable (literal rows, one per group/partition)
```

## Key Concepts

| Term | Meaning |
|------|---------|
| **Grain set** | A logical group of related datasets sharing dimensions and measures |
| **Conformed dimension** | A dimension defined at the model level, queryable across all groups |
| **Qualified dimension** | A 3-part path like `adwords.dates.year` scoping a dimension to one group |
| **Cross-grain-set metric** | A metric referencing measures from different groups (e.g. `adwords.cost + facebook.spend`) |
| **Partitioned dataset** | A dataset split into physical partitions, each served by a separate Scan |
| **Degenerate dimension** | A dimension whose attributes live directly on the fact table (no JOIN) |
| **Virtual dimension** | `_dataset.*` metadata attributes projected as literals, not columns |
| **Required measures** | Set of measure names needed by the query; from **all** requested metrics (including structured e.g. CASE) via `resolver::collect_required_measure_names` |
| **Partial selection (tier 3)** | When no single table or join can serve a grain set, selector picks the best dataset; planner builds a branch with NULL for missing dimensions and measures so the union has a single projection and no data is dropped |

## Module Responsibilities

**`plan.rs`** — Query classification and dispatch. Builds `required_measures` via
`resolver::collect_required_measure_names` (all metrics, including structured)
for selector and conformed path; no plan-building logic of its own beyond that.

**`table.rs`** — The workhorse. `plan_query` handles the resolver-based path
(single resolved dataset). `build_tablegroup_branch` is the unified builder used
by `cross.rs` when it needs a per-group aggregate sub-plan.

**`cross.rs`** — Builds UNION plans for metrics that span multiple grain sets.
Each branch aggregates its own group, projects to a common schema with NULLs for
missing columns, then a final re-aggregation (SUM) combines everything.

**`union.rs`** — Handles four UNION-flavored scenarios: conformed dimensions
(three-tier: single table → join → partial + NULL fill), multi-group qualified
dimensions, partitioned datasets, and virtual-only queries. Uses
`resolver::collect_required_measure_names` for conformed and qualified paths.
`build_union_branch` and `build_partial_union_branch` produce per-group branches
with a common output schema (NULL for missing on partial).

**`join.rs`** — When measures are spread across multiple tables in the same group,
builds sub-queries per table and joins them with FULL OUTER JOIN + COALESCE on
dimension columns.

**`expr.rs`** — Pure conversion from semantic model expression trees (`MeasureExpr`,
`MetricExpr`, `ConditionExpr`) to plan `Expr` nodes. Also handles filter and
JSON literal conversion.

**`util.rs`** — Reusable helpers: `needs_join_for_dimension`, `build_column`,
`ParsedDimensionAttr`, virtual attribute value resolution, column collection for
scan schemas.

## Dependencies

- **Resolver** — `collect_required_measure_names(model, metric_names)` returns all measure names referenced by the given metrics (including structured). Used by `plan.rs` and `union.rs` to build `required_measures` so selection and conformed union consider every required measure (e.g. from CASE metrics).
- **Selector** — `select_datasets` (tier 1), `select_datasets_for_join` (tier 2), `select_partial_for_grain_set` (tier 3). `PartialSelection` carries present/missing dimensions and measures for NULL-fill branches.
