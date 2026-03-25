# semstrait-adapter

Engine adapter layer — produces engine-appropriate artifacts from logical plans.

Each adapter implements both `EngineProfile` (capability flags) and `EngineAdapter` (plan conversion). This is the core value layer of semstrait's engine integration.

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
