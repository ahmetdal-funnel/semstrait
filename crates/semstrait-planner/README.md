# semstrait-planner

Semantic query planner with kind-specific planning strategies.

Builds a `LogicalPlan` from a `ResolvedQueryRequest` + `CompiledManifest` by dispatching to the appropriate kind planner, resolving additivity, injecting filters, and applying the optimizer.

---

## Planning Pipeline

The planner follows a 12-step pipeline (synchronous, not async):

```
ResolvedQueryRequest + CompiledManifest
       |
  1. ConstraintValidator::check()     pre-resolution validity gate
  2. Entity resolution                 manifest.resolve(name) -> &DataKind
  3. Binding pruning                   metadata + literal filter pruning
  4. DataKind dispatch                 route to planner by variant
  5. PlannerContext                    manifest + plan_builder + catalog + session
  6. KindPlanner::resolve()           build PlanFragment
  7. AdditivityResolver               semi/non-additive measure handling
  8. Filter injection                 kind-level -> user filters
  9. ORDER BY                         SortNode from request.order_by
 10. LIMIT                            FetchNode from request.limit
 11. Build LogicalPlan                root + output_names
 12. Optimizer::apply()               identity in v1 (zero passes)
       |
       v
  LogicalPlan
```

### Diagram: Planner Evaluation Order

![Planner Evaluation Order](docs/D3_planner_evaluation_order.svg)

Shows the step-by-step evaluation within `SemanticPlanner::plan()` -- constraint checks, kind dispatch, additivity resolution, filter stacking, and optimizer application.

---

## DataKind Dispatch

The planner resolves entities via `manifest.resolve(name)`, which returns a `&DataKind` from the pre-computed `data_kinds` map. Dispatch is variant-based:

```
DataKind::Dataset   -->  DatasetPlanner::resolve()    (via KindPlannerRegistry)
DataKind::Grainset  -->  GrainsetPlanner::resolve()  (via KindPlannerRegistry)
DataKind::Unionset  -->  UnionsetPlanner::resolve()  (via KindPlannerRegistry)
DataKind::Joinset   -->  JoinsetPlanner::resolve()   (via KindPlannerRegistry)
```

All kind planners receive `&DataKind` and extract the variant-specific struct (`GrainsetKind`, etc.) which embeds:
- **`KindInterface`** -- shared semantic fields (dimensions, measures, metrics, filters, keys, domain)
- **`DatasetBinding`** -- per-dataset physical mapping (`ResolvedColumnMapping`, `resolved_sources`)
- **Acceleration indices** -- `CoverageIndex`, `DimensionIndex`, `GrainMap`, etc.

### Binding Pruning

Before dispatch, the planner narrows bindings via two pruning passes:

1. **Metadata pruning** -- if a user filter matches a metadata dimension with `Eq`, bindings whose extracted metadata value doesn't match are excluded
2. **Literal pruning** -- if a user filter matches a field with a literal column mapping value, bindings whose literal doesn't match are excluded

---

## Kind Planners

Each `DataKind` variant dispatches to a dedicated planner that builds the initial `PlanFragment`:

| DataKind Variant | Strategy | Planner |
|-----------------|----------|---------|
| `Dataset` | Single-dataset fast path (Scan → Agg → Project) | `DatasetPlanner` (dataset.rs) |
| `Grainset` | Route to cheapest covering dataset by grain | `GrainsetPlanner` |
| `Unionset` | UNION ALL with NULL-fill for missing columns | `UnionsetPlanner` |
| `Joinset` | BFS join chain from anchor dataset | `JoinsetPlanner` |

### Computed Dimension Handling

All kind planners partition requested dimensions into three tiers:

1. **Metadata** — `DimensionType::Metadata` — extracted from source paths/partitions (not scanned)
2. **Computed** — `dim.expr.is_some()` — derived from expressions over other columns (post-aggregation)
3. **Physical** — regular columns scanned from datasets and used in GROUP BY

Functions in `expr/mod.rs`:
- `partition_dimensions_iface()` — separates metadata from regular dims
- `split_computed_dims()` — separates computed from physical dims
- `collect_column_refs()` — extracts column references from expression trees
- `extract_metadata_value_binding()` — resolves metadata dimension values from bindings
- `resolve_native_grain_binding()` — finds best-match grain binding for temporal dims
- `grain_to_temporal()` — converts grain enum to temporal truncation expression

Expression resolution in `resolver.rs`:
- `ExprResolver` trait with `PhysicalResolver` and `MappingResolver` implementations
- Resolves semantic column names to physical, expands Guard → Case

Measure decomposition in `decomposer.rs`:
- `decompose_measure()` — declarative path (agg tag + horizontal expr)
- `decompose_metric()` — recursive metric decomposition via KindInterface

Computed dimension flow:
1. Expression resolved via `PhysicalResolver::new().resolve_expr()` (semantic → physical column names)
2. Physical columns referenced by the expression are collected for ScanNode
3. Computed dim NOT added to GROUP BY (AggNode groups only physical dims)
4. Computed dim emitted as ProjectNode expression (post-aggregation, alongside measure/metric aliases)

### Diagram: Kind Interface Binding

![Kind Interface Binding](docs/D5_kind_interface_binding.svg)

Shows the three layers of a Kind: the **interface** (`KindInterface` -- dimensions, measures, metrics, constraints) that users query; the **strategy** (enum variant) that determines plan structure; and the **binding** (`DatasetBinding` -- column mappings, resolved sources) that connects to physical data.

---

## Module Structure

```
src/
├── lib.rs                      re-exports, public API
├── planner.rs                  SemanticPlanner orchestrator
├── request.rs                  ResolvedQueryRequest, QueryFilter, OrderByClause
├── error.rs                    PlannerError enum
│
├── resolver.rs                 ExprResolver trait + PhysicalResolver + MappingResolver
├── decomposer.rs               DecomposedMeasure, decompose_measure, decompose_metric
├── validator.rs                ConstraintValidator (pre-resolution validity gate)
├── optimizer.rs                OptimizerPass trait + Optimizer
├── additivity.rs               AdditivityResolver
│
├── expr/
│   └── mod.rs                  dimension partitioning, column ref collection, grain utils
│
├── kind/
│   ├── mod.rs                  KindPlanner trait, Registry, PlanFragment, PlannerContext
│   ├── plan_builder.rs         shared plan-building utilities (Scan, Agg, Project construction)
│   ├── dataset.rs              DatasetPlanner (single-dataset fast path)
│   ├── grainset.rs             GrainsetPlanner (grain-aware routing + UNION ALL)
│   ├── unionset.rs             UnionsetPlanner (UNION ALL with NULL-fill)
│   └── joinset.rs              JoinsetPlanner (BFS join chain + field resolution)
│
└── tests/
    ├── mod.rs
    ├── helpers.rs              shared test fixtures (manifests, requests, DataKind builders)
    └── integration.rs          end-to-end planning pipeline tests
```

---

## Key Types

```rust
// The main entry point.
pub struct SemanticPlanner { .. }

impl SemanticPlanner {
    pub fn builder() -> SemanticPlannerBuilder;
    pub fn plan(&self, request: &ResolvedQueryRequest, manifest: &CompiledManifest)
        -> Result<LogicalPlan, PlannerError>;
}

// Resolved query request (produced by RequestParser).
pub struct ResolvedQueryRequest {
    pub entity_name: String,
    pub dimensions: Vec<String>,
    pub measures: Vec<String>,
    pub filters: Vec<QueryFilter>,
    pub order_by: Vec<OrderByClause>,
    pub limit: Option<u64>,
    pub grain: Option<String>,
    pub domain_hint: Option<String>,
    pub session_variables: SessionVariables,
}
```

---

## Filter Injection Order

Filters are layered in a specific order (inner to outer):

1. **Measure filters** -- conditional aggregation (`CASE WHEN filter THEN expr ELSE NULL END`), applied inside KindPlanner
2. **Metric filters** -- same conditional aggregation pattern, applied during expression lowering
3. **Kind-level filters** -- injected before user filters, apply to all queries against the kind
4. **User filters** -- from the request, outermost `FilterNode`s

---

## Dependencies

- `semstrait-core` -- `ConsumerProfile`, `Expr`, `DataType`
- `semstrait-ir` -- `PlanNode`, `LogicalPlan`, `NodeMeta`
- `semstrait-manifest` -- `CompiledManifest`, `DataKind`, `KindInterface`, `DatasetBinding`
- `semstrait-catalog` -- `CatalogProvider` (optional, for schema checks)
