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
| DL-023 | `EngineAdapter` produces artifacts; connectors execute | `EngineAdapter::adapt()` produces `PlanArtifact` (SQL or Substrait). `ComputeConnector::execute()` runs the artifact. Old `ComputeEmitter`/`ComputeAdapter` removed in V2. |
| DL-024 | `SafeDivide` → `Divide` after Substrait round-trip | Substrait has no safe-divide. Null-guard is SQL-emission only. |
| DL-030 | Polyglot SQL: AST builder approach | `PlanBuilder` → polyglot-sql `Expression` AST → `generate()` to target dialect. Requires `opt-level = 1` in dev profile. |
| DL-031 | DuckDB: pin `>=1.3.0,<1.4.0`, `Arc<Mutex<Connection>>` | Arrow 55 alignment. `Connection` is `Send` but `!Sync`; all DB ops via `spawn_blocking`. |
| DL-032 | Trino connector via `reqwest` (not client crate) | Direct REST v1/statement API. Pagination via nextUri polling. Basic/JWT auth. |
| DL-033 | Spark connector: structural impl only | Full trait interface, `execute()` returns NotImplemented. gRPC client deferred. |
| DL-035 | Grainset horizontal join: greedy set-cover + FULL OUTER JOIN | Multi-dataset join when no single dataset covers all fields. COALESCE for null handling. |
| DL-037 | Schema drift covers `manifest.datasets` only | Kind-bound datasets lack `compiled_schema`. Extending deferred. |
| DL-038 | `column_mapping: auto` = identity mapping | Expanded to 1:1 identity (semantic name → same name). No catalog lookup needed. |
| DL-039 | `semstrait-connectors` is one crate | Traits + feature-gated engine impls. `ConsumerProfile` moved to core breaks compute ↔ planner cycle. |
| DL-040 | `semstrait-api` is one crate with feature-gated transports | `grpc`, `rest`, `cli` share `RequestParser`, error types, `SemstraitEngine`. |
| DL-041 | `Optimizer` lives inside `semstrait-planner` | Internal quality pass at end of `plan()`. Not a public pipeline stage. |
| DL-042 | `Optimizer` is empty by default | Zero passes = identity. `OptimizerPass` trait exists for extensibility. |
| DL-043 | Manifest compilation was stateless-only in v1 | `FileSystemRepository` added later. `InMemoryRepository` remains for testing. |

---

## Archived

Entries DL-001–003, 005–007, 010–012, 014–019, 025–029, 034, 036 recorded historical
implementation choices now baked into the codebase. Consult git history for original narratives.

### Completed Items (from CONTEXT.md §8)

| Item | Status |
|------|--------|
| `column_mapping: auto` identity expansion (DL-038) | Done |
| Domain filter step (planner step 3, `domain_hint`) | Done |
| Aggregation constraints (`check_aggregation_constraints`) | Done |
| REST `/schema` and `/compile` endpoints | Done |
| gRPC transport (tonic, 4 RPCs) | Done (DL-034) |
| Polyglot SQL transpilation (DL-030) | Done |
| DuckDB connector (DL-031) | Done |
| Trino connector (DL-032) | Done |
| Spark connector — structural (DL-033) | Done |
| `FileSystemRepository` (JSON-backed, atomic write) | Done |
| UNION DISTINCT (`UnionMode`) | Done |
| Kind-level filter block | Done |
| Schema drift detection (DL-037) | Done |
| EngineAdapter pipeline (adapter/connector split) | Done |
| Unity catalog provider | Done |
| Unified Expr migration (DL-020, Phases 1-6) | Done |
| SafeDivide Substrait anchor (DL-024) | By design |
| Glob namespace field (`SemanticModel.namespace`) | Done |
| V2.0 Phase 1: Extras alignment, default categorical dim | Done |
| V2.0 Phase 2: Multi-path storage, compilation preconditions | Done |
| V2.0 Phase 3: Declarative aggregation, horizontal-only expr | Done |
| V2.0 Phase 4: Metadata dimension type | Done |
