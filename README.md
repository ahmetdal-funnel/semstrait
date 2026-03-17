# semstrait

A **manifest compiler + semantic query engine** written in Rust.

semstrait resolves semantic models (defined in YAML) into engine-executable plans by:
1. Compiling YAML model files into a validated `CompiledManifest` artifact (offline)
2. Planning `QueryRequest`s against that manifest into a `LogicalPlan` IR (online)
3. Emitting the plan as Substrait bytes or dialect-specific SQL (online)

The canonical output is Substrait — a portable, engine-agnostic binary representation of relational algebra. SQL is derived from the same internal plan, for engines that prefer SQL over Substrait.

---

## Architecture

The system is organized as a layered crate workspace. Each layer depends only on the layers below it.

### Diagram: Crate Layer Architecture
![Crate Layer Architecture](docs/D1_crate_layer_architecture.svg)

### Diagram: System Pipeline
![System Pipeline](docs/D2_system_pipeline.svg)

### Crate Map

```
semstrait/                       Cargo workspace root
├── crates/
│   ├── semstrait-core/          Foundation — shared primitives, zero internal deps
│   ├── semstrait-model/         YAML model parsing and ref resolution
│   ├── semstrait-catalog/       CatalogProvider trait + implementations
│   ├── semstrait-manifest/      ManifestCompiler + Repository (InMemory v1)
│   ├── semstrait-ir/            PlanNode IR + Substrait bridge
│   ├── semstrait-planner/       SemanticPlanner + KindPlanners + Optimizer
│   ├── semstrait-sql/           SqlEmitter trait + dialect implementations
│   ├── semstrait-connectors/    Compute traits + feature-gated engine impls
│   ├── semstrait-api/           gRPC + REST + CLI (submodules, feature-gated)
│   └── semstrait/               Facade — builder, public API, feature flags
├── test_data/                   Shared YAML model fixtures
└── docs/                        Architecture diagrams and design documents
```

### Dependency Graph

```
semstrait-core                    (zero internal deps — foundation)
    ├── semstrait-model           (YAML types, ref resolution)
    ├── semstrait-catalog         (CatalogProvider trait)
    └── semstrait-ir              (PlanNode, Substrait bridge)
            │
semstrait-manifest                (core + model + catalog)
            │
    ├── semstrait-planner         (core + ir + manifest + catalog)
    └── semstrait-sql             (core + ir)
            │
semstrait-connectors              (core + ir + sql)
            │
    ├── semstrait-api             (core + planner + manifest + connectors)
    └── semstrait (facade)        (planner + manifest + connectors + catalog)
```

Dependencies flow strictly downward. No cycles.

---

## Core Concepts

### Semantic Model

A semantic model is a YAML file that declares a queryable interface over physical data:

```yaml
semantic_model:
  name: sales
  kinds:
    - name: orders
      type:
        grainset:
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: [day, month, year]
        - name: region
          data_type: string
          type:
            categorical:
      measures:
        - name: revenue
          data_type: "decimal(18,2)"
          expr: "SUM(amount)"
          additivity:
            type:
              full:
      metrics:
        - name: avg_order_value
          data_type: "decimal(18,2)"
          expr: "revenue / COUNT(order_id)"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              region: region_code
              revenue: total_amount
            storage:
              path: warehouse.fact_orders_daily
```

### Kind Types

Kinds define how datasets relate to each other:

| Kind | Strategy | Use Case |
|------|----------|----------|
| **Grainset** | Route to cheapest covering dataset | Multiple aggregation levels of the same data |
| **Unionset** | UNION ALL with NULL-fill | Same schema across multiple sources |
| **Joinset** | BFS join chain from anchor | Related datasets with different schemas |

### Additional Architecture Diagrams

| Diagram | Description |
|---------|-------------|
| [D3 - Planner Evaluation Order](docs/D3_planner_evaluation_order.svg) | Steps within SemanticPlanner.plan() |
| [D4 - PlanNode Substrait Map](docs/D4_plannode_substrait_map.svg) | PlanNode variant to Substrait Rel correspondence |
| [D5 - Kind Interface Binding](docs/D5_kind_interface_binding.svg) | Three layers of Kind: interface, strategy, binding |
| [D6 - Connector Architecture](docs/D6_connector_architecture.svg) | Compute emit/adapt/execute pipeline |

---

## Compilation Pipeline

```
QueryRequest + CompiledManifest
       │  ConstraintEvaluator    step 0: pre-resolution validity gate
       ▼
 SemanticPlanner
       │  KindPlanner dispatch   Grainset | Unionset | Joinset
       │  AdditivityResolver     semi/non-additive measure handling
       │  Filter injection       dataset → measure → metric → user
       ▼
 LogicalPlan (PlanNode IR)
       │
       ├─ SubstraitSerializer  → substrait::proto::Plan → Vec<u8>  (always produced)
       └─ SqlEmitter           → String                             (on demand)
       ▼
 CompiledPlan                  ← the public output
```

---

## DSL Expressions

All computations are expressed via a typed DSL — raw SQL strings are rejected at compile time:

```yaml
# Aggregations
expr: "SUM(amount)"
expr: "COUNT(DISTINCT customer_id)"
expr: "AVG(price)"

# Arithmetic
expr: "revenue / order_count"

# Safe division (NULL when divisor is 0)
expr: "SAFE_DIVIDE(revenue, order_count)"

# Conditional
expr: "CASE WHEN status = 'active' THEN amount END"

# Date truncation
expr: "DATE_TRUNC('month', order_date)"

# Guards (measure-scoped filters)
expr: "SUM(CASE WHEN channel = 'online' THEN amount END)"
```

---

## Design Principles

**Substrait is the contract, not SQL.** The canonical representation of a compiled query is always Substrait bytes. SQL is a convenience output.

**Proto is an implementation detail.** `substrait::proto::*` types are used only inside the IR crate's serializer. The public API surface contains only `Vec<u8>`.

**PlanNode is internal.** The relational algebra IR is never exposed. Engine integrations work from Substrait bytes or SQL strings only.

**Strict dependency direction.** `semstrait-core` has no I/O, no engine deps, no network. Layers above add capability without modifying layers below.

**DSL only, no raw SQL.** Expressions are parsed from a typed DSL. This enables validation, optimization, and cross-dialect emission.

---

## Quick Start

### Library Usage

```rust
use semstrait::{Semstrait, QueryRequest};

let sem = Semstrait::builder()
    .with_manifest_yaml(yaml_str)
    .build()
    .await?;

let result = sem.explain(QueryRequest {
    kind: "orders".into(),
    dimensions: vec!["region".into(), "order_date".into()],
    measures: vec!["revenue".into()],
    ..Default::default()
}).await?;

println!("SQL: {}", result.sql.unwrap());
println!("Substrait: {} bytes", result.substrait.len());
```

### CLI

```bash
# Compile a model and explain a query
semstrait explain --model models/sales.yaml \
  --dimensions region,order_date \
  --measures revenue

# Validate a model
semstrait validate --model models/sales.yaml

# Start REST API server
semstrait serve --model models/ --port 3000
```

### REST API

```bash
curl -X POST http://localhost:3000/query \
  -H 'Content-Type: application/json' \
  -d '{
    "kind": "orders",
    "dimensions": ["region", "order_date"],
    "measures": ["revenue"],
    "filters": [{"dimension": "order_date", "op": "gte", "value": "2024-01-01"}]
  }'
```

---

## Development

```bash
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Run with specific features
cargo build -p semstrait --features "api-rest,api-cli"
```

---

## License

Apache 2.0. See [LICENSE](LICENSE).
