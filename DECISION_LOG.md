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
| DL-023 | `EngineAdapter` produces artifacts; connectors execute | `EngineAdapter::adapt()` produces `PlanArtifact` (SQL or Substrait). `ComputeConnector::execute()` runs the artifact. Old `ComputeEmitter`/`ComputeAdapter` removed. |
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
| DL-044 | Core `DataType` uses ANSI SQL logical types (8 variants) | Replaced 23 Arrow-aligned physical types with 8 logical: Integer, Number, Decimal, String, Boolean, Date, Timestamp, Binary. Adapter layer handles engine-specific physical mapping. Eliminates lossy Substrait type conversions. |
| DL-045 | DataFusion connector accepts both SQL and Substrait artifacts | `execute()` dual-path: `PlanArtifact::Sql` → `ctx.sql()`, `PlanArtifact::Substrait` → `from_substrait_plan()`. Engine no longer falls back to SQL. |
| DL-046 | Scan schema uses semantic type fallback when catalog schema unavailable | `resolve_scan_type_binding()` priority: catalog schema → semantic type (from KindInterface via column_mapping inverse) → DataType::String. Required for Substrait strict type matching. |
| DL-047 | Computed dimensions excluded from column_mapping completeness | Computed dims derive values from expressions over other columns; they have no physical storage. `collect_mappable_names()` excludes them (same as metadata dims). |
| DL-048 | Computed dimensions are post-aggregation ProjectNode expressions | Computed dim expressions may reference GROUP BY columns. They are injected into ProjectNode after AggNode, not as ScanNode columns. Physical columns referenced by the expression are collected for ScanNode. |
| DL-049 | Declarative YAML expr blocks limited in nested kinds | serde_yaml 0.9 nested untagged enum limitation: `ExprSource` (untagged: Inline\|Declarative) inside `DimensionEntry` (untagged: Ref\|Inline) fails parsing in grainsets/unionsets/joinsets. Workaround: use inline string expressions for kind-level computed dims. Declarative blocks work in top-level `datasets:`. |
| DL-050 | FunctionRegistry validates 28 ANSI SQL functions at compile time | Arity-checked during `compile_dimensions()`. Unknown functions pass with warning (extensibility). Categories: String(13), Math(7), Date(5), Conditional(3). Computed dim expressions must not contain aggregation. |
| DL-051 | `Events` is a fourth `TemporalHistorization` variant | Independent occurrences (transactions, clicks). Unlike `Timeseries` (periodic, semi-additive), `Events` are fully additive and use standard GROUP BY. `EventsConfig.occurred_at` names the physical timestamp column. |
| DL-052 | `TemporalConfig` gains `grain` and `dimension` fields | `grain: Option<TemporalGrain>` = data-level cadence. `dimension: Option<String>` = links to semantic dimension name. Both optional for backward compat. `grain` enables auto-propagation (DL-053). `dimension` replaces interface scan for finding temporal dimension. |
| DL-053 | Grain auto-propagation: `temporal.grain` → column_mapping | `temporal.grain` auto-sets `column_mapping[temporal.dimension].grain` ONLY when both reference the same physical column. Different physical columns require explicit grain. Explicit column_mapping grain always wins. SCD types (no single timestamp) gracefully skipped. |
| DL-054 | Dimension grain auto-derivation from dataset temporal configs | Empty `TemporalDimension.grains` auto-derived from dataset `temporal.grain` values. Derived set = all grains coarser-or-equal to finest dataset grain. Emits COMP_I001 diagnostic. Runs after `expand_auto_mappings` (step 4.5), before `validate_grain_compatibility` (step 5.5). |
| DL-055 | `EngineAdapter` HAS-A profile, not IS-A `EngineProfile` | `fn profile() -> &dyn EngineProfile` replaces supertrait. Each adapter struct holds its own profile internally. Enables facade to extract profile for planner without `profile_from_adapter()` lossy copy. |
| DL-056 | `ComputeConnector` decoupled from `EngineAdapter` | No `fn adapter()` method. Connector is pure execution: receives `PlanArtifact`, returns `ComputeResult`. Adapter and connector are independent — either can exist without the other. Removes `semstrait-adapter` as dependency of `semstrait-connectors`. |
| DL-057 | `ConsumerProfile` → `DefaultProfile` (testing fallback) | Renamed for clarity. `DefaultProfile` is a bag-of-booleans `EngineProfile` impl used when no adapter is provided. `profile_from_adapter()` removed — `adapter.profile()` is the canonical path. |
| DL-058 | `AdditivityResolver` is v1 stub — semi/non-additive measures produce incorrect results | All branches return `Ok(fragment)` unchanged. Semi-additive measures (Timeseries) need window-function deduplication; non-additive measures (CountDistinct in UNION) need re-aggregation. V1 explicitly does not restructure plans for additivity. |
| DL-059 | Facade `with_adapter()`; planner `with_profile()` (DAG constraint) | Planner depends on core (not adapter). Cannot accept `&dyn EngineAdapter`. Facade bridges: `builder.with_adapter(adapter)` → internally calls `adapter.profile()` → passes to `planner.with_profile()`. |
| DL-060 | `builder.rs` must not discard Substrait artifacts | Bug: current code discards `PlanArtifact::Substrait` and falls back to SQL via `debug_sql()`. Fix: pass artifact directly from `adapter.adapt()` to connector or return to consumer. |
| DL-061 | Domain removed from model, manifest, planner, IR | `DomainSpec`, `domain` field, `domain_hint`, `DomainHint` annotation all removed. Metadata dimensions provide richer classification. Phase J-0. |
| DL-062 | `Compiled` prefix for all resolved manifest types | `CompiledDataKind`, `CompiledInterface`, `CompiledDatasetKind`, `CompiledGrainsetKind`, `CompiledUnionsetKind`, `CompiledJoinsetKind`. Distinguishes model-layer types from compiled-layer types. Phase J-3. |
| DL-063 | Single `data_kinds: IndexMap` in `CompiledManifest` v3 | Replaces three separate maps (`datasets`, `kinds`, `data_kinds`). All entity types live in one map. `resolve()` is sole lookup. Phase J-3. |
| DL-064 | Single-dataset kinds compile as `CompiledDataKind::Dataset` | Fast path: kinds with 1 dataset are functionally datasets for query purposes — no grain routing, union, or join logic needed. The dataset planner handles computed dimensions and simpler plans correctly. Phase J-3. |
| DL-065 | `CatalogSnapshot` for schema drift (replaces per-dataset `compiled_schema`) | Schema drift detection uses `manifest.catalog_snapshot` with `ResolvedColumn` types instead of per-`CompiledDataset` schema. Phase J-3. |

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
| Extras alignment, default categorical dim | Done |
| Multi-path storage, compilation preconditions | Done |
| Declarative aggregation, horizontal-only expr | Done |
| Metadata dimension type | Done |
| Phase G: Computed expressions (G0-G6) | Done |
| Phase H: Planner restructuring | Done |
| Phase I: Temporal/DataKind schema redesign | Done |
| Phase J: Domain removal + compiled layer consolidation (v3) | Done |
