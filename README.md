# semstrait

> Compile semantic models to executable compute plans.

semstrait transforms declarative YAML semantic model definitions into [Substrait](https://substrait.io) compute plans and SQL. It acts as a semantic layer between the way analysts define metrics and dimensions and the way engines execute queries — without coupling either side to the other.

---

## What it does

A semantic model describes *what* data means: which tables exist, how they join, what "revenue" means as an aggregated measure, which dimensions can slice it, and what grain each physical dataset lives at. semstrait takes that definition and a query request (give me `revenue` sliced by `region` and `month`, filtered to `year = 2024`) and produces:

- A **Substrait plan** — a portable, engine-agnostic binary representation of the relational algebra required to answer the query. Hand this to DataFusion, DuckDB, Velox, or any Substrait-capable engine.
- **SQL** — a dialect-specific SQL string derived from the same internal plan, for engines that prefer SQL over Substrait.

The Substrait plan is the canonical internal representation. SQL is a projection from it, not a parallel path.

---

## Crate map

```
semstrait/                          workspace root
├── crates/
│   ├── semstrait-core/             Core compilation pipeline
│   ├── semstrait-sql/              SQL dialect emission layer
│   ├── semstrait-connectors/       Engine adapter abstractions
│   ├── semstrait-http/             HTTP API server (axum)
│   ├── semstrait-cli/              CLI binary (clap)
│   └── semstrait/                  Facade crate — public re-exports + feature gates
├── test_data/                      Shared YAML model fixtures
└── docs/                           Design documents
```

### Dependency graph

```
semstrait-core
    └── semstrait-sql         (adds dialect SQL from core's plans)
            └── semstrait-connectors   (adds engine execution)
                    ├── semstrait-http
                    └── semstrait-cli

semstrait  (facade, depends on all, re-exports with feature gates)
```

Dependencies flow strictly downward. `semstrait-core` has no knowledge of SQL dialects, engines, HTTP, or CLI. Each layer adds capability without modifying the layer below.

---

## Core concepts

### Semantic model

A semantic model is a YAML file that declares a queryable interface over physical data:

```yaml
semantic_models:
  - name: sales
    datasets:
      - name: orders_daily
        source: warehouse.fact_orders_daily
        grain: [date, region_id]
      - name: orders_raw
        source: warehouse.fact_orders
        grain: [order_id]
    dimensions:
      - name: region
        column: region_id
        join_path: dim_region
      - name: date.year
        column: order_date
        derivable_levels: [year, month, week]
    measures:
      - name: revenue
        expr: sum(amount)
        additive: true
```

### QueryRequest

A `QueryRequest` names the model, the measures to retrieve, the dimensions to group by, and any filters:

```rust
QueryRequest {
    model: "sales".into(),
    measures: vec!["revenue".into()],
    dimensions: vec!["date.year".into(), "region".into()],
    filters: vec![DataFilter::equals("date.year", "2024")],
    ..Default::default()
}
```

### Compilation pipeline

```
QueryRequest + Schema
       ↓  selector     picks the optimal dataset (aggregate-aware)
SelectedDataset
       ↓  resolver     resolves dimension paths, join graph, filter predicates
ResolvedQuery
       ↓  planner      builds a logical relational plan
PlanNode               ← private; never leaves the planner boundary
       ↓  always runs both paths in one pass:
       ├─ substrait_conv  → proto::Plan → Vec<u8>  (canonical IR, always produced)
       └─ sql_emitter     → String                  (derived from PlanNode, on demand)
CompiledPlan           ← the public output
```

`PlanNode` is a private implementation type. No code outside `semstrait-core`'s planner module ever constructs or inspects it. The public boundary is `CompiledPlan`.

### Substrait as canonical IR

Substrait bytes (`Vec<u8>`) are always produced during compilation — regardless of whether the caller asked for them. They are the canonical representation of the query intent. When a caller requests only SQL, the Substrait plan is still built internally but the bytes are not included in the output.

The reason: Substrait is the only format that can be handed directly to a compute engine's physical execution layer. SQL requires the engine to parse and re-plan it. Substrait is also inspectable, versionable, and diffable across engine configurations. Making it optional would undermine its role as the stable semantic contract.

`substrait::proto::Plan` is used only inside the `substrait_conv` module as an in-memory intermediate. Serialization to bytes (`encode_to_vec()`) happens exactly once, at the boundary of `CompiledPlan` construction. No proto types are ever exposed in the public API.

---

## Public API (via `semstrait` facade)

```rust
use semstrait::{SemanticCompiler, StatelessCompiler, FileSystemRegistry,
                QueryRequest, CompiledPlan, CompileOpts, ModelRef};

// Build a compiler backed by YAML files on disk
let registry = FileSystemRegistry::new("./models");
let compiler = StatelessCompiler::new(registry);

// Compile a query
let plan: CompiledPlan = compiler.compile(
    &ModelRef::file("sales.yaml"),
    &QueryRequest::new("sales").measure("revenue").dimension("date.year"),
    &CompileOpts::default().with_sql(Dialect::DuckDb),
)?;

// Use outputs
let substrait_bytes: &[u8] = plan.substrait();   // always present
let sql: &str = plan.sql().unwrap();              // present when requested
```

The `SemanticCompiler` trait is the only type CLI and HTTP depend on. Swapping the compiler implementation (e.g., from `StatelessCompiler` to a future `ManifestCompiler`) requires no changes anywhere else.

---

## Integration

### With DataFusion

```rust
use datafusion::prelude::*;
use datafusion_substrait::logical_plan::consumer::from_substrait_plan;
use prost::Message;
use substrait::proto::Plan;

let compiled = compiler.compile(&model_ref, &request, &opts)?;

let proto_plan = Plan::decode(compiled.substrait())?;
let ctx = SessionContext::new();
let logical = from_substrait_plan(&ctx.state(), &proto_plan).await?;
let df = ctx.execute_logical_plan(logical).await?;
```

### With DuckDB (SQL path)

```rust
let opts = CompileOpts::default().with_sql(Dialect::DuckDb);
let compiled = compiler.compile(&model_ref, &request, &opts)?;
conn.execute(compiled.sql().unwrap(), [])?;
```

### Via HTTP

```bash
curl -X POST http://localhost:3000/query \
  -H 'Content-Type: application/json' \
  -d '{"model": "sales.yaml", "measures": ["revenue"], "dimensions": ["date.year"]}'
```

Returns `{"substrait": "<base64>", "sql": "<dialect sql>"}`.

---

## Design principles

**Substrait is the contract, not SQL.** SQL is a convenience output for engines that need it. The canonical representation of a compiled query is always Substrait bytes.

**Proto is an implementation detail.** `substrait::proto::*` types are used only inside the conversion module. The public API surface contains only `Vec<u8>` for Substrait output.

**PlanNode is private.** The internal relational algebra representation is a named-column, domain-aware tree that's ergonomic to build and traverse. It is never exposed. Engine integrations work from Substrait bytes or SQL strings only.

**Strict dependency direction.** `semstrait-core` has no I/O, no engine deps, no network. Layers above it add capability. Nothing in a lower layer ever imports from a higher one.

**Decomposition-ready module boundaries.** The single `semstrait-core` crate is structured so that `schema/`, `parser/`, and `planner/` can each be extracted into independent crates when interface stability warrants it. Module visibility (`pub(crate)`) is the today-boundary; crate visibility is the tomorrow-boundary.

---

## Workspace structure (future decomposition target)

The current single-crate `semstrait-core` is structured to eventually split into:

| Module today | Crate tomorrow | Rationale |
|---|---|---|
| `schema/` | `semstrait-schema` | Pure types — no parser dep, usable by programmatic model builders, LLM integrations |
| `parser/` | `semstrait-parser` | Format-specific — enables LookML, Cube.js JSON parsers without changing schema types |
| `planner/` | stays in core | Selector + resolver + planner are tightly coupled; external benefit of splitting is low |

This split happens when APIs stabilize, not at v1.

---

## License

Apache 2.0. See [LICENSE](LICENSE).
