# Decision Log — Active Constraints

Architectural decisions that code must respect. Historical entries archived — consult git history.

---

## Active Constraints

| ID | Constraint | Rationale |
|----|-----------|-----------|
| DL-004 | Substrait is always produced; SQL is on-demand | Substrait is the canonical IR — engine-agnostic, inspectable, versionable. Do not bypass. |
| DL-008 | Measure filters use conditional aggregation (`CASE WHEN`) | Different measures share GROUP BY. Pre-aggregate filter nodes would fork scan branches. Standard semantic layer pattern (MetricFlow, Cube.js). |
| DL-009 | DataFusion connector uses `datafusion::arrow::*` re-exports only | Separate `arrow` dep causes type mismatch. Never add standalone arrow to connectors that use DataFusion's arrow. |
| DL-013 | `SemanticPlanner::plan()` is synchronous | No I/O in planning — pure computation over in-memory `CompiledManifest` and `ConsumerProfile`. Async would add overhead for zero benefit. |
| DL-020 | `core::DslExpr` and `ir::DslExpr` are different types | Known name collision. Core uses typed variants (Sum, Add, Eq, Guard). IR uses `BinaryOp { op }` + `FunctionCall`. Future: rename IR type to `IrExpr`. |
| DL-023 | `ComputeEmitter` is NOT in engine hot path | Engine uses `SqlEmitter` + `SubstraitSerializer` directly. `ComputeEmitter` is optional connector capability for custom payload creation. Intentional divergence from D6 diagram. |
| DL-024 | `SafeDivide` → `Divide` after Substrait round-trip | Intentional. Substrait has no safe-divide function. Null-guard (`CASE WHEN b=0 THEN NULL`) is SQL-emission only. Not a bug. |
| DL-030 | Polyglot SQL: transpilation layer, not builder replacement | ANSI SQL generated first via `AnsiSqlEmitter`, then transpiled via `polyglot_sql::transpile()`. Requires `profile.dev.package.polyglot-sql.opt-level = 1` (1000x slower unoptimized). `dialect-presto` feature required with `dialect-trino` (upstream bug). |
| DL-031 | DuckDB: pin `duckdb >=1.3.0, <1.4.0`, use `Arc<Mutex<Connection>>` | Arrow 55 alignment — v1.4+ uses arrow 56+. `Connection` is `Send` but `!Sync`; all DB ops via `spawn_blocking`. Collect `Vec<RecordBatch>` inside blocking closure (borrow lifetime). Package aliased as `duckdb-engine` in Cargo.toml. |

---

## Archived Entries

Entries DL-001, 002, 003, 005, 006, 007, 010–012, 014–019, 025–026 recorded historical
implementation choices now baked into the codebase. Key context absorbed into CONTEXT.md v3.6.
Consult git history for original narratives.

### Connector Roadmap Decisions (not yet implemented)

| ID | Decision | Library | Notes |
|----|----------|---------|-------|
| DL-027 | Trino connector | `trino-rust-client` v0.9.3 | REST v1/statement, JSON→Arrow conversion. Fallback: raw reqwest. |
| DL-028 | Spark connector | Apache `spark-connect-rs` (forked) | Fork needed: bump tonic 0.11→0.12, prost 0.12→0.14. SQL path via `spark.sql()`. |
| DL-029 | Arrow Flight SQL | `arrow-flight` v58 | Deferred to v2+. Databricks-specific, not Spark/Trino. |
