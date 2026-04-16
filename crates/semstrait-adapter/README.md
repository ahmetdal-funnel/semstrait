# semstrait-adapter

Engine adapter layer — produces engine-appropriate artifacts (SQL or Substrait) from logical plans. Contains SQL emission (SqlEmitter, dialects) and per-engine adapters.

---

## Core Trait

```rust
pub trait EngineAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn plan_builder(&self) -> Option<Box<dyn PlanBuilder>> { None }
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError>;
    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError> { /* ANSI default */ }
}
```

- `plan_builder()` — optional engine-specific node construction. The facade extracts this and passes it to the planner.
- `adapt()` — converts LogicalPlan to `PlanArtifact::Substrait` or `PlanArtifact::Sql`.
- `debug_sql()` — always available ANSI SQL for debugging. Default impl uses `AnsiSqlEmitter`.

---

## Adapters

| Adapter | Output | Feature | Status |
|---------|--------|---------|--------|
| `DataFusionAdapter` | `PlanArtifact::Substrait` | `datafusion` | Implemented — primary supported engine |
| `DuckDbAdapter` | `AdaptError::UnsupportedFeature` | `duckdb` | Structural stub — dialect infra exists, `adapt()` returns `UnsupportedFeature` |
| `SparkAdapter` | `AdaptError::UnsupportedFeature` | `spark` | Structural stub — dialect infra exists, `adapt()` returns `UnsupportedFeature` |

---

## SQL Emission

SQL is emitted by walking the `PlanNode` tree via `SqlEmitter` trait. Lives in `src/sql/`.

| Type | Description |
|------|-------------|
| `SqlEmitter` trait | `emit(plan) -> Result<String>` |
| `SqlDialect` trait | Quoting, DATE_TRUNC, LIMIT/FETCH, window functions |
| `AnsiSqlEmitter<D>` | Parameterized emitter — one per dialect |
| `AnsiDialect` | ANSI standard (FETCH FIRST, DATE_TRUNC) |
| `DataFusionDialect` | LIMIT, ILIKE native, `now()`, `regexp_match` |
| `DuckDbDialect` | LIMIT, lowercase date_trunc |
| `SparkDialect` | Spark SQL conventions |
| `ExprSqlRenderer` | `Expr -> SQL string` rendering |
| `PolyglotEmitter` | Dialect transpilation via `polyglot-sql` (feature-gated) |

---

## Module Structure

```
src/
├── lib.rs                  re-exports
├── error.rs                AdaptError
├── traits.rs               EngineAdapter trait
├── sql/
│   ├── mod.rs              SqlEmitter, SqlDialect, AnsiSqlEmitter
│   ├── dialect.rs          Dialect impls (Ansi, DataFusion, DuckDb, Spark)
│   ├── expr_renderer.rs    Expr → SQL string
│   ├── polyglot_emitter.rs PolyglotSqlEmitter (feature-gated)
│   ├── polyglot/           polyglot sub-module (expr_builder, plan_builder)
│   └── tests.rs            SQL emission tests
└── engines/
    ├── mod.rs              engine re-exports
    ├── ansi.rs             AnsiAdapter (always available)
    ├── datafusion/         DataFusionAdapter + DataFusionPlanBuilder
    ├── duckdb/             DuckDbAdapter (stub — returns UnsupportedFeature)
    └── spark/              SparkAdapter (stub — returns UnsupportedFeature)
```

---

## Dependencies

- `semstrait-core` — `DataType`, `Expr`
- `semstrait-ir` — `LogicalPlan`, `PlanArtifact`, `PlanBuilder`, `SubstraitSerializer`, `FunctionRegistry`
- `polyglot-sql` (optional, behind `duckdb`/`spark` features) — dialect transpilation
