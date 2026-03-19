# Decision Log — Active Constraints

Architectural decisions that code must respect. Historical entries archived — consult git history.

---

## Active Constraints

| ID | Constraint | Rationale |
|----|-----------|-----------|
| DL-004 | Substrait is always produced; SQL is on-demand | Substrait is the canonical IR — engine-agnostic, inspectable, versionable. |
| DL-008 | Measure filters use conditional aggregation (`CASE WHEN`) | Measures share GROUP BY. Standard semantic layer pattern (MetricFlow, Cube.js). |
| DL-009 | DataFusion uses `datafusion::arrow::*` re-exports only | Separate `arrow` dep causes type mismatch. Never add standalone arrow to DataFusion connectors. |
| DL-013 | `SemanticPlanner::plan()` is synchronous | Pure computation over in-memory data. Async adds overhead for zero benefit. |
| DL-020 | Unified `Expr` in `semstrait-core::expr` | IR, planner, SQL all use unified `Expr` with typed `Aggregation`, `BinaryOp`, preserved `i64` precision. |
| DL-023 | `ComputeEmitter` is NOT in engine hot path | Engine uses `SqlEmitter` + `SubstraitSerializer` directly. `ComputeEmitter` is optional connector capability. |
| DL-024 | `SafeDivide` → `Divide` after Substrait round-trip | Substrait has no safe-divide. Null-guard is SQL-emission only. |
| DL-030 | Polyglot SQL: AST builder approach | `PlanBuilder` → polyglot-sql `Expression` AST → `generate()` to target dialect. Requires `opt-level = 1` in dev profile. |
| DL-031 | DuckDB: pin `>=1.3.0,<1.4.0`, `Arc<Mutex<Connection>>` | Arrow 55 alignment. `Connection` is `Send` but `!Sync`; all DB ops via `spawn_blocking`. |
| DL-032 | Trino connector via `reqwest` (not client crate) | Direct REST v1/statement API. Pagination via nextUri polling. Basic/JWT auth. |
| DL-033 | Spark connector: structural impl only | Full trait interface, `execute()` returns NotImplemented. gRPC client deferred. |
| DL-035 | Grainset horizontal join: greedy set-cover + FULL OUTER JOIN | Multi-dataset join when no single dataset covers all fields. COALESCE for null handling. |
| DL-037 | Schema drift covers `manifest.datasets` only | Kind-bound datasets lack `compiled_schema`. Extending deferred. |
| DL-038 | `column_mapping: auto` = identity mapping | Expanded to 1:1 identity (semantic name → same name). No catalog lookup needed. |

---

## Archived

Entries DL-001–003, 005–007, 010–012, 014–019, 025–029, 034, 036 recorded historical
implementation choices now baked into the codebase. Consult git history for original narratives.
