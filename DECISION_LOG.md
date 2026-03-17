# Decision Log

All architectural and implementation decisions are recorded here.
Module-level decisions live in `crates/<module>/DECISION_LOG.md`.

---

## DL-001: Restructure from monolithic to 10-crate workspace

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Current `semstrait-core` contains all logic (schema, parser, planner, DSL, compiler, output, diagnostics). CONTEXT.md specifies 10 crates with strict dependency DAG.
**Decision:** Restructure to match CONTEXT.md's 10-crate architecture. Move existing working code into appropriate new crates. Preserve all 203 existing tests.
**Rationale:** The 10-crate structure enforces dependency rules at the Cargo level, enables parallel compilation, and matches the design document that is the authoritative reference.

## DL-002: V1 focuses on plan generation, not execution

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Full engine execution requires DuckDB, Trino, Spark connectors with complex wire protocols.
**Decision:** V1 generates LogicalPlan + ANSI SQL. Connector traits exist but implementations are stubs. Execution is v2.
**Rationale:** Validating the semantic model → plan → SQL pipeline end-to-end is the critical path. Execution adds integration complexity that can be layered on once the plan generation is proven correct.

## DL-003: Use sqlparser-rs AST for SQL generation

**Date:** 2026-03-17
**Status:** Accepted
**Context:** CONTEXT.md specifies using `sqlparser-rs` as intermediate form for syntactically correct SQL output.
**Decision:** PlanNode tree → sqlparser AST → String. No Jinja templates. Programmatic SQL construction only.
**Rationale:** sqlparser-rs guarantees syntactic correctness. Template-based approaches are fragile and hard to test.

## DL-004: Substrait is always produced, SQL is on-demand

**Date:** 2026-03-17
**Status:** Accepted (from CONTEXT.md design principle)
**Context:** Substrait bytes are the canonical IR.
**Decision:** Every plan compilation produces Substrait internally. SQL emission is an additional step, dialect-specific, derived from the same PlanNode tree.
**Rationale:** Substrait is the stable semantic contract. It's engine-agnostic, inspectable, and versionable.

## DL-005: Existing semstrait-core code forms the seed implementation

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Current core has 203 passing tests covering schema types, DSL lexer/parser/lowering, planner IR, SQL emission, Substrait conversion, and constraint checking.
**Decision:** Extract and refactor existing code into new crates rather than rewriting from scratch. Preserve test coverage.
**Rationale:** The existing code is tested and correct for its scope. Rewriting introduces unnecessary risk and discards validated logic.

## DL-006: Optimizer is empty in v1 (identity function)

**Date:** 2026-03-17
**Status:** Accepted (from CONTEXT.md D4)
**Context:** Optimizer passes (predicate pushdown, projection pruning) add complexity.
**Decision:** `Optimizer` struct and `OptimizerPass` trait exist on day one. Zero passes registered by default. Passes are opt-in.
**Rationale:** The infrastructure cost is minimal. Adding passes later requires no API changes.

## DL-007: IcebergRestCatalog is stub-only in v1

**Date:** 2026-03-17
**Status:** Superseded by DL-009
**Context:** Full Iceberg REST catalog requires Polaris/Gravitino integration.
**Decision:** `IcebergRestCatalog` struct exists behind `iceberg` feature flag. V1 implementation is minimal.
**Rationale:** Catalog integration is secondary to plan generation correctness.

## DL-008: Measure filters as conditional aggregation

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Measures can have dataset-scoped filters. Two approaches: (a) pre-aggregate FilterNodes per measure, (b) conditional aggregation via `CASE WHEN filter THEN expr ELSE NULL END`.
**Decision:** Use conditional aggregation. `expr_lower::lower_measure_with_filters()` wraps aggregate inner expressions with CASE WHEN for each measure's filters.
**Rationale:** Different measures can have different filters while sharing the same GROUP BY. Pre-aggregate FilterNodes would require separate scan branches per measure. Conditional aggregation is the standard semantic layer approach (MetricFlow, Cube.js pattern).

## DL-009: DataFusion connector — feature-gated, uses re-exported Arrow

**Date:** 2026-03-17
**Status:** Accepted
**Context:** DataFusion bundles its own arrow crate. Adding a separate `arrow` workspace dependency causes type mismatches.
**Decision:** The `datafusion` feature on `semstrait-connectors` pulls in `datafusion` v52. All Arrow types (RecordBatch, etc.) use DataFusion's re-exported `datafusion::arrow::*`, not a separate arrow dependency.
**Rationale:** Avoids duplicate arrow types. DataFusion owns its arrow version; consumers use its re-exports.

## DL-010: Iceberg REST catalog — feature-gated with reqwest

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Iceberg REST catalog requires HTTP calls to a catalog server (Polaris, Gravitino, etc.).
**Decision:** `IcebergRestCatalog` behind `iceberg` feature flag on `semstrait-catalog`. Uses `reqwest` for HTTP. Supports OAuth2 client credentials and bearer token auth. Implements `CatalogProvider` trait for `list_tables` and `get_table_schema`.
**Rationale:** Keeps the default build free of HTTP dependencies. Feature flag isolates the network dependency.

## DL-011: SemstraitEngine wires full pipeline end-to-end

**Date:** 2026-03-17
**Status:** Accepted
**Context:** The API layer needs a single orchestrator that ties together manifest compilation, request parsing, planning, and SQL emission.
**Decision:** `SemstraitEngine` in `semstrait-api` orchestrates: `with_manifest_yaml()` → `validate()` / `explain()`. `RequestParser::to_resolved()` converts raw API requests to planner `ResolvedQueryRequest` with full manifest validation and filter/order conversion.
**Rationale:** Single entry point for all API transports (REST, CLI, gRPC). Keeps transport code thin.

## DL-012: Test fixtures as external YAML files

**Date:** 2026-03-17
**Status:** Accepted
**Context:** E2E tests across workspace root, facade crate, and API crate all embed large inline YAML model strings, making test files bloated and hard to maintain.
**Decision:** Extract all inline YAML test models to `tests/fixtures/models/*.yaml`. Tests load fixtures via `load_model(name)` helper that reads from disk using `CARGO_MANIFEST_DIR`.
**Rationale:** Single source of truth for test models. Easier to add/modify models. Test files focus on logic, not YAML formatting. Fixtures are reusable across crate boundaries.

## DL-013: SemanticPlanner::plan() is synchronous

**Date:** 2026-03-17
**Status:** Accepted
**Context:** `SemanticPlanner::plan()` was originally `async fn` but contained zero `.await` points. The planner does no I/O — it reads from in-memory `CompiledManifest` and `ConsumerProfile`.
**Decision:** Change `plan()` from `async fn` to `fn`. Remove all `.await` at call sites.
**Rationale:** Unnecessary async adds runtime overhead (future state machine), complicates call sites, and violates the principle of least surprise. Plan generation is pure computation.

## DL-014: EngineError preserves typed errors via #[from]

**Date:** 2026-03-17
**Status:** Accepted
**Context:** `EngineError` variants wrapped inner errors as `String` (e.g., `Plan(String)`), losing type information for programmatic error handling.
**Decision:** Use `#[error("...")] #[from]` for `PlannerError`, `EmitError`, and `CompileError`. Keep `String` wrappers only for `Parse`, `Execution`, `NotConfigured`, and `Internal` which cross crate boundaries.
**Rationale:** Typed errors enable pattern matching on specific error variants. Error context is preserved for debugging.

## DL-015: SQL uses direct string building, not sqlparser-rs AST

**Date:** 2026-03-17
**Status:** Accepted (supersedes DL-003)
**Context:** CONTEXT.md v3.0 specified sqlparser-rs as an intermediate form. In practice, `semstrait-sql` generates SQL by direct string building through `SqlDialect` trait methods and `DslExprSqlRenderer`.
**Decision:** No sqlparser-rs AST intermediate. `AnsiSqlEmitter<D: SqlDialect>` walks `PlanNode` tree and builds SQL strings directly. `DslExprSqlRenderer` converts `DslExpr` trees to SQL fragments per dialect.
**Rationale:** Direct string building is simpler, faster, and gives full control over dialect-specific output. sqlparser-rs adds a dependency for round-trip correctness that isn't needed when generating SQL from a well-typed IR.

## DL-016: ANSI FETCH FIRST instead of LIMIT for AnsiDialect

**Date:** 2026-03-17
**Status:** Accepted
**Context:** The `FetchNode` SQL emission used `LIMIT N OFFSET M` (MySQL/PostgreSQL syntax) for all dialects.
**Decision:** Add `limit_clause()` to `SqlDialect` trait. `AnsiDialect` and `TrinoDialect` emit `FETCH FIRST N ROWS ONLY` (SQL:2008 standard). `DuckDbDialect` emits `LIMIT N`.
**Rationale:** ANSI SQL standard uses FETCH FIRST. Dialect-specific emission ensures correct syntax per target engine.

## DL-017: ConsumerProfile wired from connector through engine to planner

**Date:** 2026-03-17
**Status:** Accepted
**Context:** `ConsumerProfile` existed on `ComputeAdapter` but was never passed to the planner. The planner always used `ConsumerProfile::default()`.
**Decision:** `SemstraitEngine::with_connector()` extracts `connector.consumer_profile().clone()` and passes it to `SemanticPlannerBuilder::with_profile()`. `set_connector()` rebuilds the planner with the new profile.
**Rationale:** The planner uses `ConsumerProfile` for strategy decisions (e.g., window functions vs double-aggregate for semi-additive measures). Without wiring, all queries use default capabilities regardless of the actual engine.

## DL-018: Schema uses HashMap index for O(1) ordinal lookups

**Date:** 2026-03-17
**Status:** Accepted
**Context:** `Schema::ordinal()` performed linear scan (`iter().position()`) for each column reference. During Substrait conversion, this is called per column reference, making it O(n×m) for n columns and m references.
**Decision:** Add `HashMap<String, usize>` index field to `Schema`, built at construction. `ordinal()` becomes `self.index.get(name).copied()`. HashMap is `#[serde(skip)]` and excluded from `PartialEq`.
**Rationale:** O(1) lookups for wide tables (100+ columns). Minimal memory overhead (one HashMap per Schema instance). Clone propagates the index automatically.

## DL-019: Substrait round-trip for all DslExpr variants

**Date:** 2026-03-17
**Status:** Accepted
**Context:** `ExprConverter::from_substrait()` reconstructed many expression types as generic `FunctionCall` nodes instead of their native `DslExpr` variants (Not, IsNull, InList, Between, Like, NullIf, DateTrunc, Coalesce).
**Decision:** Add function anchor constants (205-210) for all missing functions. Update `from_scalar_function()` to reconstruct native DslExpr variants. Register all functions in Substrait extension declarations.
**Rationale:** Faithful round-trip (DslExpr → Substrait → DslExpr) is required for plan inspection, optimization passes, and Substrait-based plan exchange with external consumers.
