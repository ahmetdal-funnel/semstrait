# semstrait

A **manifest compiler + semantic plan-generation library** written in Rust.

semstrait resolves semantic models (defined in YAML) into engine-executable artifacts:
1. Compiles YAML model files into a validated `CompiledManifest` (offline)
2. Plans `QueryRequest`s against that manifest into a `LogicalPlan` IR (online)
3. Adapts the plan into engine-specific artifacts — Substrait plans or dialect-specific SQL (online)

Primary output is `PlanArtifact` (SQL string or `substrait::proto::Plan`). Execution is the consumer's responsibility.

---

## Architecture

The system is organized as a layered crate workspace. Each layer depends only on the layers below it. See `docs/ARCHITECTURE.md` for the full architectural reference including constraints, design decisions, and the crate dependency DAG.

### Crate Map

```
semstrait/                       Cargo workspace root
├── crates/
│   ├── semstrait-core/          Foundation — DataType, Expr, Schema, Grain, constraints
│   ├── semstrait-model/         YAML model parsing, ref resolution, 69 expression keys
│   ├── semstrait-catalog/       CatalogProvider trait + Iceberg/Unity catalogs
│   ├── semstrait-manifest/      ManifestCompiler pipeline (parse -> validate -> compile)
│   ├── semstrait-ir/            PlanNode IR + PlanArtifact + Substrait bridge
│   ├── semstrait-planner/       SemanticPlanner + DataKind planners + Optimizer
│   ├── semstrait-adapter/       EngineAdapter trait + SqlEmitter + dialect impls
│   ├── semstrait-api/           REST + CLI + gRPC transports (feature-gated)
│   │   └── proto/               gRPC service proto definitions
│   └── semstrait/               Facade — builder, public API, re-exports
├── tests/
│   └── fixtures/models/         YAML model fixtures for testing
├── test_data/                   Larger test models (paid_media, ecommerce, etc.)
└── docs/                        Design documents (architecture, strategies, functions)
```

### Dependency Graph

```
semstrait-core                    (zero internal deps — foundation)
    ├── semstrait-model           (YAML types, ref resolution)
    ├── semstrait-catalog         (CatalogProvider trait)
    └── semstrait-ir              (PlanNode, PlanArtifact, Substrait bridge)
            │
semstrait-manifest                (core + model + catalog)
            │
    ├── semstrait-planner         (core + ir + manifest + catalog)
    └── semstrait-adapter         (core + ir)
            │
    ├── semstrait-api             (planner + manifest + adapter + catalog)
    └── semstrait (facade)        (planner + manifest + adapter + catalog)
```

Dependencies flow strictly downward. No cycles. Enforced by Cargo.

---

## Core Concepts

### Semantic Model

A semantic model is a YAML file that declares a queryable interface over physical data:

```yaml
semantic_model:
  name: sales
  grainsets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: [day, month, year]
        - name: region
          data_type: string
          type:
            categorical: {}
      measures:
        - name: revenue
          data_type: float64
          agg: sum
        - name: order_count
          data_type: int64
          agg: count
      metrics:
        - name: avg_order_value
          data_type: float64
          expr: "revenue / order_count"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              region: region_name
              revenue: amount
              order_count: order_id
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
```

### Kind Types

Kinds define how datasets relate to each other within a semantic entity:

| Kind | Strategy | Use Case |
|------|----------|----------|
| **Simple** | Single-dataset fast path (Scan -> Agg -> Project) | One dataset, direct query |
| **Grainset** | Route to cheapest covering dataset by grain | Multiple aggregation levels of the same data |
| **Unionset** | UNION ALL with NULL-fill | Same schema across multiple sources |
| **Joinset** | BFS join chain from anchor | Related datasets with different schemas |

YAML top-level keys: `datasets:` (Simple), `grainsets:`, `unionsets:`, `joinsets:`.

### Engine Adapters

Adapters produce engine-appropriate artifacts from the logical plan:

| Engine | Adapter Output | V1 Status |
|--------|---------------|-----------|
| **DataFusion** | `PlanArtifact::Substrait` | Primary path |
| **DuckDB** | `PlanArtifact::Sql` (DuckDB dialect) | Dialect exists, adapter returns unsupported |
| **Spark** | `PlanArtifact::Sql` (Spark dialect) | Dialect exists, adapter returns unsupported |

All adapters expose `debug_sql()` — ANSI SQL for debugging, always available regardless of primary artifact type.

---

## Planning Pipeline

```
QueryRequest + CompiledManifest
       │  ConstraintValidator      step 0: pre-resolution validity gate
       v
 SemanticPlanner (synchronous)
       │  DataKind dispatch        Simple | Grainset | Unionset | Joinset
       │  Binding pruning          metadata + literal filter pruning
       │  AdditivityResolver       semi/non-additive measure handling (v1 stub)
       │  Filter injection         kind-level -> measure (conditional agg) -> user
       v
 LogicalPlan (PlanNode IR)
       │
 Optimizer.apply()                 identity in v1 (zero passes)
       │
 EngineAdapter.adapt()             selects output based on engine
       │
       ├─ PlanArtifact::Substrait  (DataFusion — native Substrait plan)
       └─ PlanArtifact::Sql        (DuckDB / Spark — dialect SQL)
```

---

## DSL Expressions

All computations are expressed via a typed DSL — raw SQL strings are rejected at compile time:

```yaml
# Declarative aggregation (preferred)
agg: sum
expr: amount

# Metric arithmetic
expr: "revenue / order_count"

# Safe division (NULL when divisor is 0)
expr: "SAFE_DIVIDE(revenue, order_count)"

# Conditional
expr: "CASE WHEN status = 'active' THEN amount END"

# Date truncation
expr: "DATE_TRUNC('month', order_date)"

# Computed dimensions
expr:
  regexp_extract:
    column: campaign
    pattern: "^([A-Z]{2})_"
    group: 1
```

69 YAML expression keys supported. See `docs/FUNCTION_CATALOG.md` for the full function mapping.

---

## Quick Start

### Library Usage (Facade)

```rust
use semstrait::SemstraitBuilder;

// Fast path — file paths + engine name, same simplicity as CLI
let sem = SemstraitBuilder::new()
    .with_model_file("path/to/model.yaml")
    .with_catalogs_file("path/to/catalogs.yaml")
    .with_engine("datafusion")
    .build()
    .await?;

let sql = sem.explain(&request)?;
let artifact = sem.plan(&request)?;  // PlanArtifact::Substrait
```

#### S3 + Polaris on EC2 (IAM Role)

```rust
// On EC2, AWS credentials resolve via IAM instance role automatically.
// catalogs.yaml defines Polaris connections (OAuth2, Secrets Manager, etc.)
let sem = SemstraitBuilder::new()
    .with_model_file("s3://my-bucket/models/paid_media.yaml")
    .with_catalogs_file("s3://my-bucket/config/catalogs.yaml")
    .with_engine("datafusion")
    .build()
    .await?;
```

See `crates/semstrait/README.md` for the full set of examples (explicit provider construction, manual S3 loading, etc.).

### API Layer (SemstraitEngine)

```rust
use semstrait_api::{SemstraitEngine, RawQueryRequest};

let engine = SemstraitEngine::with_model(yaml).await?;

let result = engine.explain(&RawQueryRequest {
    from: "orders".into(),
    select: vec!["region".into(), "revenue".into()],
    engine: Some("datafusion".into()),
    ..Default::default()
})?;

println!("{}", result.plan_text);
```

### CLI

```bash
# Local model
semstrait explain --model path/to/model.yaml --from orders --select date,revenue

# S3 model + S3 catalogs
semstrait explain --model s3://bucket/model.yaml --catalogs s3://bucket/catalogs.yaml --from orders --select date,revenue

# Compile and output manifest
semstrait compile --input s3://bucket/model.yaml --catalogs s3://bucket/catalogs.yaml
```

---

## Feature Flags

| Crate | Feature | Adds |
|-------|---------|------|
| `semstrait-adapter` | `datafusion` | DataFusion adapter (Substrait output) |
| `semstrait-adapter` | `duckdb` | DuckDB adapter + polyglot-sql transpilation |
| `semstrait-adapter` | `spark` | Spark adapter + polyglot-sql transpilation |
| `semstrait-catalog` | `iceberg` | Iceberg REST catalog client (OAuth2, Polaris) |
| `semstrait-catalog` | `unity` | Databricks Unity catalog client |
| `semstrait-catalog` | `local` | Local filesystem storage provider (glob) |
| `semstrait-catalog` | `aws` | AWS Secrets Manager for Iceberg OAuth2 |
| `semstrait-manifest` | `aws` | S3 model loading (`s3://` URIs in `load_text`) |
| `semstrait` (facade) | `aws` | Pass-through: S3 loading + AWS Secrets Manager |
| `semstrait` (facade) | `catalog-iceberg` | Pass-through: Iceberg REST catalog |
| `semstrait-api` | `cli` | CLI transport via clap |
| `semstrait-api` | `rest` | REST transport via axum |
| `semstrait-api` | `grpc` | gRPC transport via tonic |
| `semstrait-api` | `datafusion` | Pass-through to adapter |
| `semstrait-api` | `duckdb` | Pass-through to adapter |
| `semstrait-api` | `spark` | Pass-through to adapter |
| `semstrait-api` | `iceberg` | Pass-through to catalog |
| `semstrait-api` | `unity` | Pass-through to catalog |
| `semstrait-api` | `aws` | Pass-through to catalog |

---

## Development

```bash
# Build all crates
cargo build --workspace

# Run all tests (791 tests)
cargo test --workspace

# Build CLI binary with DataFusion + Iceberg
cargo build -p semstrait-api --features cli,datafusion,iceberg,aws --release

# Run with specific features
cargo test --workspace --features datafusion
```

---

## Test Fixtures

Test models in `tests/fixtures/models/`:

| Fixture | Description |
|---------|-------------|
| `orders_basic` | 3 dims, 2 measures, 1 metric — full-featured grainset |
| `orders_simple` | Minimal: 1 dim, 1 measure |
| `orders_3dim` | 3 dims (date/region/customer), 1 measure |
| `orders_constrained` | Measure with `one_of` dimension constraint |
| `orders_computed_dim` | Computed dimension via expression |
| `orders_datafusion` | DataFusion-specific adapter tests |
| `orders_with_metrics` | 2 dims, 1 measure — API engine tests |
| `products` | 2 dims, 1 measure — filter/order tests |
| `transactions_multi_measure` | 1 dim, 3 measures |
| `sales_constrained` | Kind with dimension constraint |
| `comprehensive_ecommerce` | Multi-kind ecommerce model |
| `declarative_expressions` | Declarative expression block coverage |
| `e2e_full_coverage` | Full pipeline coverage model |
| `raw_sql_invalid` | Raw SQL in expr — compile rejection test |

Larger models in `test_data/`:

| Fixture | Description |
|---------|-------------|
| `alpinestars_eu_ad_platform_v2` | Real-world paid media model (4 ad platforms, grainset + unionset) |
| `paid_media_kind` | Paid media grainset with 4 datasets |
| `catalogs` | Catalog configuration fixtures |
| `comprehensive_ecommerce` | Multi-kind ecommerce model |
| `e2e_full_coverage` | Full pipeline coverage |

---

## Features — Status & Roadmap

### Implemented

| Feature | Description | Coverage |
|---------|-------------|----------|
| **Manifest Compilation** | YAML -> validated CompiledManifest with acceleration structures | 9-step pipeline, 104+ tests |
| **4 Planning Strategies** | Simple, Grainset, Unionset, Joinset — each with dedicated planner | Full pipeline, 142+ planner tests |
| **Metric Decomposition** | Simple, Ratio, Derived metrics with recursive decomposition | Topological sort, depth <= 3 |
| **Computed Dimensions** | Expressions over columns, post-aggregation projection | regexp_extract, CASE, arithmetic |
| **Constraint Validation** | Pre-resolution validity gates (one_of, none_of, all, aggregation) | 13 constraint tests |
| **Binding Pruning** | Metadata + literal filter pruning before dataset routing | Eliminates non-matching bindings |
| **Re-aggregation Skip** | Unionset skips re-agg when literal dims distinguish branches | Known-values optimization |
| **Ad-hoc Join Resolution** | FROM-less queries resolved via FieldIndex + RelationshipGraph | 16+ tests |
| **Entity Resolution** | Dimension/measure names resolved to providing entities | Field-based routing |
| **Function Registry** | 28 ANSI SQL functions with compile-time arity + return type validation | String, math, date, conditional |
| **Expression DSL** | 69 YAML expression keys, 22 Expr variants, no raw SQL | Declarative blocks + inline |
| **Type System** | 8 logical types, 30+ parse aliases, type predicates | Full serde round-trip |
| **Substrait Round-trip** | LogicalPlan ↔ Substrait proto serialization with semantic annotations | All PlanNode variants |
| **SQL Emission** | SqlEmitter with dialect system (ANSI, DataFusion, DuckDB, Spark) | Per-engine quoting, functions, types |
| **Catalog Integration** | Iceberg REST (OAuth2/Polaris) + Unity catalog clients | Async source resolution |
| **Schema Propagation** | Output schema computed per PlanNode, ordinal-based field references | Full pipeline |
| **Temporal Grain Routing** | GrainMap enables grain-aware dataset selection (prefer coarser = cheaper) | Grainset planner |
| **Expression Rewriting** | PlanBuilder trait for engine-specific expr rewrites (DF: regexp_match -> regexp_like) | DataFusionPlanBuilder |
| **Literal Dimension/Measure Handling** | Literal column mappings injected as constants, not scanned | Typed literal injection |

### Stub / Partial (v1 pass-through, framework exists)

| Feature | Current State | What's Needed | Comparable To |
|---------|--------------|---------------|---------------|
| **Additivity Resolution** | Pass-through stub — semi/non-additive measures produce incorrect results at coarser grains | Window function strategy (LAST_VALUE for semi-additive), double-aggregate strategy (pre-agg at native grain, re-agg at query grain) | cube.js `rolling_window`, dbt `semi_additive_over_time` |
| **Optimizer** | Framework exists (OptimizerPass trait, chain), zero passes registered | Predicate pushdown, projection pruning, join reordering, common sub-expression elimination | cube.js pre-aggregation routing, dbt materialization selection |
| **Temporal Historization Planning** | Types parsed (Timeseries, Events, Snapshot, SCD 1-6), validated for type consistency across datasets, but not used in plan generation | SCD Type 2 → filter on `valid_from <= @date AND valid_to > @date`; Snapshot → latest-snapshot selection; Events → window dedup | dbt `snapshot` strategy, cube.js `refreshKey` with time dimension |
| **Schema Drift Detection** | PlannerWarning defined, compile-time detection in engine.rs | Query-time schema validation — warn when catalog schema diverges from compiled manifest | dbt `--warn-error`, cube.js schema validation |

### Not Implemented (planned)

| Feature | Description | Priority | Comparable To |
|---------|-------------|----------|---------------|
| **Two-Stage Metric Aggregation** | Metric-level `agg:` with inner/outer grain for pre-aggregation | High | cube.js pre-aggregations, dbt `metrics` with `grain` |
| **Window Functions** | `window:` YAML tag for ROW_NUMBER, LAG, LEAD, running totals | High | cube.js `rolling_window` measures |
| **Ratio Structured Aggregation** | `ratio:` YAML tag for numerator/denominator with independent filters | Medium | cube.js ratio measures |
| **Cross-Kind Metric Refs** | Metrics referencing measures from different kinds | Medium | dbt cross-project refs |
| **Static Pushdown (SR-10)** | CASE/IF pruned on metadata dims and literals at planning time | Medium | — |
| **Caching / Content Hash** | Model hash as manifest cache key for incremental compilation | Low | dbt `state:modified`, cube.js `refreshKey` |
| **Execution Layer** | Direct query execution (DataFusion, DuckDB embedded) | Deferred | cube.js query orchestration, dbt `run` |

### Comparison with cube.js and dbt Metrics

| Concept | semstrait | cube.js | dbt metrics/MetricFlow |
|---------|-----------|---------|----------------------|
| **Model definition** | YAML semantic model | JavaScript/YAML cube schema | YAML semantic manifest |
| **Dimensions** | Categorical, Temporal, Metadata, Computed | dimensions, time dimensions | dimensions, entities |
| **Measures** | Declarative `agg:` + horizontal expr | measures with `type: sum/count/...` | measures with `agg: sum/count/...` |
| **Metrics** | Ratio/Derived expressions over measures | calculated measures | derived/ratio/cumulative metrics |
| **Multi-dataset** | 4 strategies (Simple, Grainset, Unionset, Joinset) | joins between cubes | join paths between semantic models |
| **Pre-aggregation** | Grainset routing (grain-aware) | pre-aggregations (materialized) | materializations |
| **Additivity** | Type system exists, planning is stub | implicit via pre-agg granularity | `non_additive_dimension` tag |
| **Output** | PlanArtifact (Plan or SQL) | SQL string | SQL string |
| **Execution** | Library only (consumer executes) | Built-in query orchestrator | Built-in `mf query` |
| **Caching** | Not yet | Redis/in-memory query cache | Relies on warehouse caching |
| **Access control** | Out of scope | `queryRewrite` middleware | `grants` + external policies |

**semstrait's differentiators:**
- Substrait as canonical IR (not just SQL output) — enables engine-native consumption
- 4 explicit data kinds composition strategies vs implicit joins
- Typed expression DSL with compile-time validation (no raw SQL)
- Separation of plan generation from execution — embeddable library, not a service
- Bitmap-based coverage index + GrainMap for O(1) dataset routing

---

## Design Documents

| Topic | Document |
|-------|----------|
| Architecture, constraints, crate DAG | `docs/ARCHITECTURE.md` |
| Catalog, storage, source resolution | `docs/CATALOG_RESOLUTION.md` |
| Function mapping between IR and engines | `docs/FUNCTION_CATALOG.md` |
| Grainset planning | `docs/GRAINSET.md` |
| Unionset planning | `docs/UNIONSET.md` |
| Joinset planning | `docs/JOINSET.md` |
| Dataset (Simple) planning | `docs/DATASET.md` |
| Semantic model scoping rules | `docs/SEMANTIC_RESOLUTION.md` |
| Computed dimensions and expressions | `docs/COMPUTED_EXPRESSIONS.md` |
| Data type catalog | `docs/DATATYPE_CATALOG.md` |
| Known technical debt | `docs/TECH_DEBT.md` |

Each crate also has its own `README.md` with module maps, key types, and control flows.
