# semstrait-adapter

Engine adapter layer — produces engine-appropriate artifacts from logical plans.

Each adapter holds an `EngineProfile` internally (HAS-A, not IS-A — see DL-055) and implements `EngineAdapter` for plan conversion. This is the core value layer of semstrait's engine integration.

---

## Core Trait

```rust
pub trait EngineAdapter: Send + Sync {
    fn profile(&self) -> &dyn EngineProfile;
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError>;
    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError>;
}
```

The facade extracts `adapter.profile()` and passes it to the planner (DL-059). Connectors never reference adapters (DL-056).

---

## Adapters

| Adapter | `supports_substrait` | Output | Feature |
|---------|---------------------|--------|---------|
| `DataFusionAdapter` | `true` | `PlanArtifact::Substrait` | `datafusion` |
| `DuckDbAdapter` | `false` | `PlanArtifact::Sql` (DuckDB dialect) | `duckdb` |
| `TrinoAdapter` | `false` | `PlanArtifact::Sql` (Trino dialect) | `trino` |
| `SparkAdapter` | `false` | `PlanArtifact::Sql` (Spark dialect) | `spark` |

---

## Dependencies

- `semstrait-core` -- `EngineProfile` trait, `DataType`, `Expr`
- `semstrait-ir` -- `LogicalPlan`, `PlanArtifact`, `SubstraitSerializer`
- `semstrait-sql` -- `SqlEmitter` trait, dialect implementations
