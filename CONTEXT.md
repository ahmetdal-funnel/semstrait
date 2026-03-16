
# CONTEXT.md — Semstrait Project

> Authoritative reference for agentic work on the Semstrait codebase.
> Schema version: v1.1 | Architecture: as-designed (not yet implemented)

---

## What Is Semstrait

A **manifest compiler + semantic query engine** written in Rust. It sits between a business query ("revenue by region") and a physical query engine (DuckDB, DataFusion, Velox). It resolves semantic models defined in YAML into Substrait protobuf query plans.

**Core value:** Hides physical data architecture complexity. A user queries semantic names; Semstrait decides which tables to scan, which joins to build, how to handle SCD rows, how to route between grain levels — then emits a portable, engine-agnostic query plan.

---

## Design Principles

1. **Kind-as-contract.** A kind (grainset/unionset/joinset) defines a semantic interface. Datasets are physical bindings underneath. Users query the contract, never the physical layout.

2. **Substrait is the output, not the IR.** All semantic resolution (routing, additivity, temporal filters) happens inside Semstrait. Substrait is emitted only *after* all decisions are made. It knows nothing about grainset logic or SCD.

3. **LogicalPlan is a public artifact.** Serializable, versioned, returned on the Explain endpoint. Enables debugging and client-side introspection.

4. **Schema ordinals are the single source of truth.** Every IR node carries its output `Schema`. Parent nodes derive field references by name-to-ordinal lookup via `schema.ordinal()` — never by position guessing.

5. **Constraints are gates, properties are strategies.** Additivity tells the planner *how* to structure a query. Constraints tell it *whether* to allow it. Evaluated in strict order: constraints → additivity → filters.

6. **Planner evaluation order is strict:**
   ```
   1. constraints check (reject or pass)
   2. additivity resolution (restructure query)
   3. dataset filter (base WHERE)
   4. measure filter (CASE WHEN or WHERE)
   5. metric filter (outer WHERE)
   6. user filter (request WHERE)
   ```

7. **Keys and dimensions are identifiers, not values.** Allowed aggregations: `COUNT(DISTINCT)`, `MIN`, `MAX`, windowed `FIRST/LAST/NTH_VALUE`, `COUNT`. Prohibited: `SUM`, `AVG`, `MEDIAN`. Enforced by planner, not user-configured.

8. **Grain: resolution vs. historization are independent concerns.**
   - `dimension.temporal.grains` = query resolution (DAY → MONTH rollup).
   - `extras.temporal` = row historization (SCD type 2, snapshot). Both can coexist on the same dataset.

9. **Column mapping direction:** always `semantic_name → physical_column`. Left = what users see, right = what lives in the table.

10. **Ref syntax for reuse.** Top-level `semantic_model.dimensions/measures/metrics` are reusable definitions. Referenced via `- ref: name`. No local overrides — for variations, define inline.

---

## System Pipeline

```
User Request (gRPC proto)
  → RequestParser          proto → QueryRequest, grain coercion, ref resolution
  → SemanticPlanner        constraint check, coverage, kind dispatch, additivity
  → LogicalPlan  ◄──────── PUBLIC ARTIFACT
  → LogicalPlanOptimizer   predicate pushdown, projection pruning,
                           null-branch elimination, constant folding
  → SubstraitEmitter       ordinal resolution, extension registry, ConsumerProfile
  → substrait::Plan        (protobuf bytes) → target engine
    + LogicalPlan JSON     → Explain endpoint
```

---

## Crate Map

| Crate | Role |
|---|---|
| `semstrait-core` | Shared primitives: `Schema`, `DataType`, `DslExpr`, `Literal` |
| `semstrait-model` | Parsed + resolved YAML model types |
| `semstrait-manifest` | Compiler: parse → validate → compile → `manifest.json` |
| `semstrait-planner` | `SemanticPlanner`, kind planners, `AdditivityResolver` |
| `semstrait-optimizer` | `LogicalPlan → LogicalPlan` passes |
| `semstrait-substrait` | `SubstraitEmitter`, `ConsumerProfile`, `ExtensionRegistry` |
| `semstrait-api` | gRPC service, proto codegen, `RequestParser` |
| `semstrait-cli` | `compile`, `explain`, `validate` commands |

---

## Schema Structure

```
semantic_model
├── name, description, ai_context, labels
├── datasets[]            ← top-level physical datasets
│   ├── name, description, tags
│   ├── keys              ← planner hinting (not integrity enforcement)
│   ├── dimensions[]      ← GROUP BY / filter axes
│   ├── measures[]        ← aggregated numeric values
│   ├── metrics[]         ← business-named derived from measures
│   ├── filter[]          ← always-on base predicate (dataset scope)
│   └── extras
│       ├── catalog
│       ├── temporal      ← historization: snapshot | scd | time_series
│       ├── storage       ← path, partition_def
│       └── column_mapping ← semantic → physical binding
│
├── kinds[]               ← virtual containers with resolution strategies
│   ├── name, description
│   ├── type              ← grainset | unionset | joinset
│   ├── keys, dimensions, measures, metrics  ← queryable contract (kind level)
│   ├── datasets[]        ← shortened form: extras only, no semantic blocks
│   └── relationships[]   ← joinset only: physical join paths
│
├── relationships[]       ← cross-dataset/kind joins (top-level)
├── dimensions[]          ← reusable definitions (ref: syntax)
├── measures[]            ← reusable definitions
└── metrics[]             ← reusable definitions
```

---

## Semantic Primitives

### Keys
Logical quality gates and join hints for the planner. Types: `primary`, `unique`, `foreign`.
- `keys.foreign` = semantic meaning ("this IS a FK").
- `relationships[]` = physical join path ("JOIN on these columns").
- Relationships takes precedence when both exist.

### Dimensions
Physical columns or logical expressions used for GROUP BY and filtering.
- **temporal** — time-based; `grains` list controls available rollup levels. Finest grain = default.
- **categorical** — discrete values; optional `enum` list.
- **binary** — boolean/bit/string two-state.
- **geo** — composite from `lat` + `lon` columns.
- **bucketed** — dynamic numeric buckets; requires `column` (source physical column).

### Measures
Stored or computed numeric values. Three orthogonal blocks:
- **`additivity`** — PROPERTY: shapes query structure.
  - `full` → SUM of SUMs is correct. No special handling.
  - `semi` → blocked on specific dimensions. Planner injects pre-resolution sub-query to collapse the non-additive dimension *before* aggregation.
  - `non` → never re-aggregatable from pre-computed values. Must use exact pre-computed grain OR recompute from source.
- **`constraints`** — CONSTRAINT: query validity gate. Violation = reject.
  - `dimensions.one_of/none_of/all` — dimensional scope requirements.
  - `aggregation.allowed/prohibited` — function whitelist/blacklist (antagonistic; define only one).
- **`filter`** — PREDICATE: per-measure row scope. Single-measure → `WHERE`. Multi-measure same dataset → `CASE WHEN` inside aggregation.

### Metrics
Business-meaningful names derived from measures/dimensions/other metrics.
- Inherit additivity and constraints from referenced measures. Own blocks add further restrictions. Stricter wins on conflict.
- Max chaining depth: 3. No circular references. Compiler validates via petgraph.
- Metric-level `filter` = outer WHERE on full computation scope (vs. measure filter which is per-aggregation).
- Cross-kind metric references: **prohibited in v1**. Emits `COMP_E006`.

---

## Kind Types and Resolution

### Grainset
**Problem:** Same data at different grain levels (daily transactions, monthly rollups).
**Operation:** Planner routes to the cheapest covering dataset.

Resolution algorithm:
1. Collect required coverage = requested dims + dims in filters (filter-referenced dims expand coverage because a pre-aggregated dataset can't evaluate them).
2. For each dataset, check `column_mapping` covers all required dims and measures.
3. Temporal: dataset grain must be ≤ requested grain (can roll UP, cannot split DOWN).
4. Non-additive measures: must be exact grain match or source data — disqualify pre-aggregated.
5. **Case A** — single dataset covers all → pick coarsest grain. Multiple same-grain → UNION ALL (treat as partitions).
6. **Case B** — no single covers all → horizontal join: aggregate each subset to user grain, FULL OUTER JOIN on shared dims. Falls back to PartialResult if consumer lacks FULL OUTER.
7. **Case C** — nothing covers → reject.

### Unionset
**Problem:** Same type of data split across sources (web orders, POS, wholesale).
**Operation:** UNION ALL into one virtual table.

Resolution algorithm:
1. Build one branch per dataset; map physical columns via `column_mapping`.
2. Missing mappings → `SELECT NULL AS semantic_name`.
3. Filter pruning: if filter targets unmapped dim and would exclude NULLs → prune entire branch.
4. UNION ALL surviving branches.
5. GROUP BY + aggregation on combined result. Additivity rules apply to combined result.
6. Emit `PLAN_W004` if AVG is used on a partially-mapped union (skew risk).

### Joinset
**Problem:** One entity spread across multiple tables (Data Vault hub+satellites, 3NF).
**Operation:** Dynamic denormalization via JOIN pruning.

Resolution algorithm:
1. Map each requested dim/measure/filter-col → providing dataset.
2. Always include anchor (inferred: dataset appearing only in `from`, never in `to`).
3. Trace relationship paths from anchor to each needed dataset; include intermediates.
4. Prune datasets not needed and not on any path to needed dataset.
5. Inject temporal filters per dataset: SCD type_2 → `valid_to IS NULL`; snapshot → `validity_col = MAX(validity_col)`.
6. Build left-deep JOIN tree; schema-tracked at each step.

Validation (compile-time, petgraph):
- Exactly one root (anchor).
- No cycles.
- All datasets reachable from anchor.

| | Grainset | Unionset | Joinset |
|---|---|---|---|
| Problem | Same data, different grains | Same data, multiple sources | One entity, multiple tables |
| Operation | Route (pick best) | Stack (UNION ALL) | Combine (JOIN) |
| Optimization | Grain routing, horizontal join | Branch pruning | Join pruning |
| Relationships | Not needed | Not needed | Required |
| Missing dimension | Dataset skipped | NULL substituted | Dataset not joined |
| Anchor | No | No | Yes (inferred) |

---

## Key Rust Types

### Schema (ordinal contract)
```rust
pub struct Schema { pub columns: Vec<SchemaColumn> }
impl Schema {
    pub fn ordinal(&self, name: &str) -> Result<u32, SchemaError>
    pub fn join(left: &Schema, right: &Schema) -> Schema  // [left | right]
    pub fn project(&self, keep: &[&str]) -> Schema
    pub fn emit_mapping(&self, target: &Schema) -> Vec<u32>
}
```
`Schema::join` produces `[left columns | right columns]` in order. Ordinals for the right side = `left.len() + right_local_ordinal`. JoinRel deduplication via `RelCommon.emit` remap (no extra ProjectRel).

### ConsumerProfile
```rust
pub struct ConsumerProfile {
    pub supports_window_functions: bool,
    pub supports_full_outer_join: bool,
    pub supports_fetch_rel: bool,
    pub max_join_depth: Option<usize>,
    pub function_uris: HashSet<String>,
}
// semi_additive_strategy() → WindowFunction | DoubleAggregate
```
Semi-additive: WindowFunction if supported, else DoubleAggregate (sub-query fallback).

### LogicalPlan nodes
All carry: `node_id` (UUID), `output_schema`, semantic annotations.

Key annotation enums:
- `AggregateRole`: `Final | SemiAdditiveInner | HorizontalSubResult | FanoutDedup`
- `JoinSource`: `Relationship{name} | HorizontalGrain{shared_dims} | SemiAdditiveReJoin`
- `FilterSource`: `DatasetFilter | MeasureFilter | MetricFilter | UserFilter | ScdCurrentRow | SnapshotValidity | RowLevelSecurity`

### DslExpr (no raw SQL strings in v1)
- Aggregate (measure context): `Sum, Count, CountDistinct, Avg, Min, Max, Median, Percentile`
- Scalar: `Column, Literal, Add, Subtract, Multiply, Divide, Negate`
- Comparison: `Eq, Neq, Lt, Lte, Gt, Gte, IsNull, IsNotNull, InList, Like`
- Logical: `And, Or, Not`
- Conditional: `Case, Coalesce`
- Temporal: `DateTrunc{grain, column}, DateDiff{grain, start, end}`
- Special: `Guard{condition, column}` — emits as `CASE WHEN` in measure filter context

### SubstraitEmitter pattern
```rust
// Every emit_rel() returns:
struct EmittedRel { rel: proto::Rel, schema: Schema }
// Schema is the ordinal map for the parent node.
// All field references: structField { field: ordinal as i32 }
```

---

## Optimizer Passes
Applied to `LogicalPlan → LogicalPlan` before Substrait emission.

1. **PredicatePushdown** — moves Filter nodes toward Scan; pushes through INNER/LEFT joins and Union if all branches have the column; blocked by FULL OUTER.
2. **ProjectionPruning** — trims `ScanNode.column_mapping` and `ProjectNode.projections` via top-down needed-set propagation.
3. **NullBranchElimination** — prunes Union branches where filter predicate excludes NULLs on unmapped column; collapses single-branch unions.
4. **ConstantFolding** — evaluates constant sub-expressions at plan time.

---

## gRPC API Surface
```protobuf
service SemstraitService {
  rpc Query    (QueryRequest)  returns (QueryResponse);
  rpc Explain  (QueryRequest)  returns (ExplainResponse);
  rpc Validate (QueryRequest)  returns (ValidationResponse);
  rpc Schema   (SchemaRequest) returns (SchemaResponse);
}
```
`QueryResponse`: `substrait_plan (bytes)`, `logical_plan (proto)`, `diagnostics[]`, `complete (bool)`, `stats`.
`ExplainResponse`: human-readable + JSON `LogicalPlan`.

---

## Known Open Gaps (v1)

| Gap | Status | Resolution |
|---|---|---|
| Metric chaining depth | Partial | Compiler enforces max depth 3, no cycles via petgraph. Error `COMP_E005`. |
| Cross-kind metrics | Unresolved | Prohibited in v1 (`COMP_E006`). Cross-kind composition deferred to v2. |
| Top-level relationship join strategy | Schema exists | Defaults to INNER JOIN at shared grain. Additivity per-entity. |
| Row-level security (`user_attribute`) | Schema exists | Session parameter required; missing → `PLAN_E005` reject. |
| Many-to-many junction tables | Deferred v2 | Workaround: model bridge as explicit dataset in joinset. |
| Grainset horizontal join type | Documented | Always FULL OUTER. Not configurable. No consumer support → PartialResult. |
| Unionset NULL for AVG | Documented | Emits `PLAN_W004`. No schema change. |
| SCD Type 3 dual-column | Documented | Model as two separate dimensions (`country_current`, `country_previous`). |

---

## Key Dependencies
```toml
substrait   = "0.58"   # proto types via prost
prost       = "0.13"
tonic       = "0.12"   # gRPC
serde       = { features = ["derive"] }
serde_json  = "1"
indexmap    = "2"      # insertion-order preservation (ordinal correctness)
uuid        = { features = ["v4"] }
petgraph    = "0.6"    # manifest compiler graph validation only
thiserror   = "1"
tracing     = "0.1"
```

---

## Self-Validation Questions & Answers

**Q1: Why does `filter` on a grainset measure expand coverage requirements?**
Because a dataset that has already aggregated away a dimension cannot evaluate a filter on that dimension. If `country = 'US'` is in the filter and a dataset pre-aggregated country, applying the filter would give wrong results — the dimension no longer exists at row level. The planner therefore treats filter-referenced dimensions as required coverage, causing that dataset to be skipped.

**Q2: What is the difference between `semi` and `non` additivity?**
- `semi`: blocked only on *specific* dimensions. If those dimensions are in the GROUP BY, the measure is safe to SUM. If not, the planner injects a pre-resolution sub-query that collapses the non-additive dimension first, *then* aggregates. The aggregation function itself is unchanged.
- `non`: never re-aggregatable from any pre-computed value at any grain. Must use exact pre-computed grain or recompute entirely from source. Example: `COUNT(DISTINCT customer_id)` pre-aggregated monthly cannot be re-summed to a yearly count.

**Q3: Why is the joinset anchor inferred, not declared?**
The anchor is structurally determined by the relationship graph — it's the dataset with in-degree 0 (appears only as `from`, never as `to`). Inferring it at compile time via petgraph means the schema is self-consistent: if you accidentally create two roots or a cycle, the compiler catches it. Declaring it explicitly would create a second place to define the same structural truth.

**Q4: What does `column_mapping` omission mean in each kind type?**
- **grainset**: the dataset is skipped for any query needing that field.
- **unionset**: `NULL` is substituted for that field in the UNION branch.
- **joinset**: the dataset is not joined if no needed fields map to it.

**Q5: Why is `extras.temporal` separate from `dimension.temporal.grains`?**
They govern different concerns. `dimension.temporal.grains` defines *resolution* — which grain levels are available for GROUP BY (DAY, MONTH, YEAR). `extras.temporal` defines *historization* — how rows change over time (SCD type 2 tracks validity periods; snapshot records periodic full loads). A monthly snapshot table has both: `temporal.grains: [month]` (resolution) and `extras.temporal.snapshot` (historization). Conflating them would make it impossible to model a monthly SCD type 2 table correctly.

**Q6: Why is raw SQL prohibited in `expr` fields (v1)?**
DSL-only expressions can be parsed, type-checked, and lowered to Substrait at compile time. Raw SQL strings are engine-specific, cannot be validated for type correctness, and cannot be optimized by the LogicalPlanOptimizer. Admitting raw SQL would break the portability guarantee — the same semantic model must emit correct plans for DuckDB, DataFusion, and Velox.

**Q7: What happens when a consumer does not support FULL OUTER JOIN in a grainset horizontal join?**
The planner falls back to picking the single best-covering dataset (highest coverage score) and emits a `PartialResult` diagnostic listing which fields could not be served. The query completes with `complete: false` in the response. This prioritizes correctness over silent wrong results.
