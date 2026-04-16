# Dataset — Single-Dataset Kind (Simple)

**Status:** Implemented
**Taxonomy:** Simple kind (single-dataset fast path). Compiles to `CompiledDataKind::Simple(CompiledSimpleKind)`. Planned by `SimplePlanner`. For multi-dataset strategies see `GRAINSET.md`, `UNIONSET.md`, `JOINSET.md`.
**Scope:** Planner (`data_kind/simple.rs`, `data_kind/plan_layers.rs`), Manifest (`CompiledSimpleKind`)

---

## 1. Definition

A **dataset** is the Simple kind — a single `DatasetBinding` mapped to one `CompiledInterface`. No grain routing, no union coverage, no join chains.

`SimplePlanner` dispatches to `build_dataset_kind_plan()`, which calls the shared `build_layered_plan()` used by all kind planners. Complex kinds compose this same builder for per-binding sub-plans.

---

## 2. Layered Plan Architecture

The plan builder produces a 5-layer tree. After L2 (Rename), all names are semantic.

```
L1  Scan          Physical columns from resolved sources
L2  Rename        Physical → semantic + literal/metadata injection + CAST
L3  Expression    Computed dimension evaluation (Guard → Case, SR-10 simplification)
L4  Aggregate     GROUP BY semantic dimensions + declarative measure decomposition
L5  Project       Final output projection (skipped when identity with L4 output)
```

### 2.1 L1 — Scan

Collects the minimal set of physical columns:
- Physical dimension columns (from `column_mapping.physical`)
- Dependency columns referenced by computed-dimension expressions
- Measure expression columns (including measure-filter columns)
- Metric constituent measure columns (recursive expansion)

Table name resolution priority: `table_fqn` > `reference` > `dataset_name`.

### 2.2 L2 — Rename (ProjectNode)

Maps physical column names to semantic names. Four column categories:

| Category | Expression |
|----------|-----------|
| Physical dimension | `Column(physical_col)` with optional `CAST` when catalog type differs from semantic type |
| Literal dimension | `Literal(value)` from `column_mapping.literals` |
| Metadata dimension | `Literal(extracted_value)` from source path/partition extraction |
| Measure dependency | `Column(physical_col)` for columns referenced by measure expressions |

### 2.3 L3 — Expression (ProjectNode)

Only emitted when computed dimensions exist. Passes through all L2 columns and appends computed expressions:

1. `resolve_guards()` — expands Guard sugar to Case
2. `substitute(known_values)` — replaces metadata/literal references with compile-time values (SR-10 static pushdown)
3. `simplify()` — constant-folds the substituted expression

Skipped entirely when there are no computed dimensions.

### 2.4 L4 — Aggregate (AggNode)

- **GROUP BY**: all requested dimensions (semantic names) including computed dims, with optional `DATE_TRUNC` for temporal grain rollup
- **Measures**: declaratively decomposed via `decompose_measure()` (agg tag + horizontal expression) or `decompose_metric()` (recursive metric expansion)
- **Measure filters**: applied as conditional aggregation (`CASE WHEN filter THEN expr ELSE NULL END`)

### 2.5 L5 — Project (ProjectNode)

Final output schema: dimensions followed by measures. Emits `post_agg_expr` for each measure (handles ratio metrics, derived expressions).

**Identity optimization**: skipped when every projection expression is `Column(name)` matching the L4 output schema field-for-field.

---

## 3. Dimension Partitioning

Dimensions are classified into three tiers before plan construction:

```
requested dimensions
       │
  partition_dimensions_iface()
       │
  ┌────┴────┐
  │Metadata │ Regular
  └─────────┴────┬────┐
                 │    │
         split_computed_dims()
                 │    │
            Physical  Computed
```

- **Metadata**: `DimensionType::Metadata` — not scanned, extracted from source paths/partitions
- **Computed**: `dim.expr.is_some()` — derived post-scan, emitted in L3
- **Physical**: regular columns scanned from datasets, used in GROUP BY

---

## 4. Multi-Source Handling

When a `DatasetBinding` has multiple `resolved_sources` (e.g., glob expansion produces multiple files), the planner builds per-source sub-plans:

```
Source A:  Scan_A → Rename_A → Expr_A → Agg_A
Source B:  Scan_B → Rename_B → Expr_B → Agg_B
                       │
                   UNION ALL
                       │
              Re-Aggregate (optional)
                       │
                   Project
```

Each source gets its own metadata dimension values extracted from its specific path/partition.

### 4.1 Re-Aggregation Skip

Re-aggregation after UNION ALL is skipped when a metadata dimension in GROUP BY has **distinct values across all sources** — no rows from different sources share the same GROUP BY key. Checked by `has_source_distinguishing_metadata()`.

### 4.2 Re-Aggregation Functions

When re-aggregation is needed, functions are derived per-aggregate:
- `MIN` / `MAX` → `MIN` / `MAX` (idempotent)
- `SUM` / `COUNT` / `COUNT_DISTINCT` / `AVG` → `SUM` (partial sums merge)

**Known limitation**: `COUNT_DISTINCT` and `AVG` re-aggregated as `SUM` are lossy. A warning is emitted.

---

## 5. Type Resolution

Column types use a 3-tier priority:

1. **Catalog schema** (physical truth) — from `ResolvedSource.schema`
2. **Semantic type** (model declaration) — from `CompiledInterface` dimension/measure `data_type`
3. **Fallback** — `DataType::String`

When catalog type differs from semantic type, a `CAST` is emitted in L2 (Rename).

---

## 6. Key Functions

| Function | Location | Role |
|----------|----------|------|
| `SimplePlanner::resolve()` | `data_kind/simple.rs` | KindPlanner dispatch |
| `build_dataset_kind_plan()` | `data_kind/plan_layers.rs` | Entry for Simple variant |
| `build_layered_plan()` | `data_kind/plan_layers.rs` | Core 5-layer plan builder |
| `build_scan_node_binding()` | `data_kind/plan_layers.rs` | Single/multi-source scan construction |
| `build_rename_project()` | `data_kind/plan_layers.rs` | Physical → semantic rename with CAST |
| `build_expression_project()` | `data_kind/plan_layers.rs` | Computed dim L3 layer |
| `partition_dimensions_iface()` | `expr/mod.rs` | Metadata vs regular dimension split |
| `split_computed_dims()` | `expr/mod.rs` | Computed vs physical dimension split |
| `decompose_measure()` | `decomposer.rs` | Declarative measure decomposition |
| `decompose_metric()` | `decomposer.rs` | Recursive metric expansion |

---

## 7. Shared Infrastructure

`build_layered_plan()` is also called by Complex kinds:

- `build_binding_plan()` — used by grainset, unionset, and joinset planners for per-binding sub-plans
- `build_union_branch()` — used by unionset/grainset for UNION ALL branches with null-fill projection

The Simple kind is the **fast path** — all Complex kinds compose dataset-level plans into larger structures (UNION ALL, JOIN, grain routing).

---

## 8. Related Documentation

- `docs/GRAINSET.md`, `docs/UNIONSET.md`, `docs/JOINSET.md` — Complex kinds
- `crates/semstrait-planner/src/data_kind/simple.rs` — `SimplePlanner`
- `crates/semstrait-planner/src/data_kind/plan_layers.rs` — layered plan builder
