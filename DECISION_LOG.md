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

## DL-002: V1 focuses on plan generation, with DataFusion as reference executor

**Date:** 2026-03-17 (updated 2026-03-18)
**Status:** Amended
**Context:** Full engine execution requires DuckDB, Trino, Spark connectors with complex wire protocols.
**Decision:** V1 generates LogicalPlan + ANSI SQL as primary output. DataFusion is the reference execution connector (feature-gated). DuckDB/Trino/Spark connectors are stubs — full execution for those engines is v2.
**Rationale:** Validating the semantic model → plan → SQL pipeline end-to-end is the critical path. DataFusion is embedded (in-process Rust), making it the lowest-friction execution target for E2E validation. External engine connectors remain v2.

## DL-003: Use sqlparser-rs AST for SQL generation

**Date:** 2026-03-17
**Status:** Superseded by DL-015
**Context:** CONTEXT.md specifies using `sqlparser-rs` as intermediate form for syntactically correct SQL output.
**Decision:** PlanNode tree → sqlparser AST → String. No Jinja templates. Programmatic SQL construction only.
**Rationale:** sqlparser-rs guarantees syntactic correctness. Template-based approaches are fragile and hard to test.
**Superseded:** Direct string building via `SqlDialect` trait proved simpler and sufficient. See DL-015.

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
**Status:** Superseded by DL-010
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

## DL-020: Dual DslExpr types — core vs IR

**Date:** 2026-03-18
**Status:** Acknowledged — future rename
**Context:** `semstrait_core::DslExpr` (used by manifest compiler) and `semstrait_ir::DslExpr` (used by plan nodes/SQL emission) are structurally different. Core uses typed variants per operator (Sum, Add, Eq, Guard). IR uses `BinaryOp { op }` enum and `FunctionCall`. They do NOT share the same variant set despite CONTEXT.md's note.
**Decision:** Both types remain as-is for v1. The IR variant is the one documented in CONTEXT.md §5.1 and used by `ExprConverter` and `DslExprSqlRenderer`. Future: rename IR type to `IrExpr` to eliminate name collision.
**Rationale:** Renaming mid-v1 would touch 40+ files. The current setup works — core DslExpr is internal to the compiler; IR DslExpr is the public-facing expression type.

## DL-021: Domain filter step deferred to v1.1

**Date:** 2026-03-18
**Status:** Accepted
**Context:** CONTEXT.md §5.6 Step 2 documents a "domain filter" step that narrows candidate datasets by `domain_hint`. The `domain_hint` field exists on `ResolvedQueryRequest` but the planner never reads it.
**Decision:** Domain filtering is deferred to v1.1. The planner looks up the kind by name and dispatches directly. The `domain_hint` field is preserved for future use.
**Rationale:** V1 models have a single kind per query. Domain filtering becomes necessary with multi-kind or multi-tenant deployments. Adding it now without a use case would be speculative.

## DL-022: Aggregation constraint checking deferred to v1.1

**Date:** 2026-03-18
**Status:** Accepted
**Context:** `ConstraintEvaluator::check_aggregation_constraints()` is a stub returning `Ok(())`. Dimensional constraints (one_of, none_of, all) are fully implemented.
**Decision:** Aggregation constraints (allowed/prohibited function lists) are deferred to v1.1. Dimension constraints are the priority for v1.
**Rationale:** V1 measures use simple aggregates (SUM, COUNT, AVG). Aggregation constraints become relevant when custom aggregation functions are supported.

## DL-023: ComputeEmitter trait — available but not in engine hot path

**Date:** 2026-03-18
**Status:** Accepted
**Context:** `ComputeEmitter` trait is defined in `semstrait-connectors` and implemented by connectors, but `SemstraitEngine` constructs `ComputePayload::Sql(sql)` directly via `SqlEmitter`, bypassing `ComputeEmitter`. D6 diagram shows it in the pipeline.
**Decision:** `ComputeEmitter` remains as an optional connector capability for engines that want custom payload creation. The engine's hot path uses `SqlEmitter` + `SubstraitSerializer` directly. Connectors that need Substrait or native plan formats can use `ComputeEmitter` in their own orchestration.
**Rationale:** The engine knows best which formats to produce (SQL + Substrait JSON for explain). Delegating to `ComputeEmitter` would add indirection without benefit for the common SQL-execution path.

## DL-024: SafeDivide maps to Divide in Substrait (null-guard at SQL level)

**Date:** 2026-03-18
**Status:** Accepted
**Context:** `BinaryOp::SafeDivide` encodes as `FUNC_DIVIDE` (anchor 303) in Substrait. On round-trip decode, it becomes `BinaryOp::Divide`. The null-guard semantics (`CASE WHEN b = 0 THEN NULL ELSE a / b END`) are only preserved in SQL emission.
**Decision:** This is by design. Substrait does not have a "safe divide" function. The null-guard is a SQL-emission concern, not a plan-level concern.
**Rationale:** Substrait consumers that receive the plan will get a standard divide. The SafeDivide semantics are a presentation-layer feature specific to SQL output.

## DL-025: Adopt polyglot-sql as SQL emission core abstraction

**Date:** 2026-03-18
**Status:** Accepted
**Context:** semstrait-sql uses custom `SqlDialect` trait + `AnsiSqlEmitter` (~580 lines) for SQL emission via direct string building. Adding dialects (Spark, Snowflake, Databricks) requires ~200 lines per dialect with risk of missing edge cases. `polyglot-sql` (v0.1.15, MIT, pure Rust) provides a builder API + dialect-aware `generate()` for 34 dialects with 18,745 test cases.
**Decision:** Adopt `polyglot-sql` as the SQL generation backend. Build queries programmatically via polyglot's builder API (`select()`, `col()`, `sum()`, `case()`, etc.), then call `generate(&expr, dialect)` for target-specific SQL. Keep `SqlEmitter` trait as our public API — polyglot is an implementation detail. Deprecate `AnsiSqlEmitter`, `DslExprSqlRenderer`, and per-dialect `SqlDialect` trait impls.
**Rationale:** Eliminates per-dialect reimplementation. polyglot's 18K tests catch function mapping, quoting, and syntax edge cases. Fork-friendly (MIT, pure Rust, zero C deps). Builder gaps (SUM DISTINCT, window OVER, DATE_TRUNC) are solvable with thin wrappers or PRs upstream. `sqlparser` kept in workspace for future SQL expr parsing (separate concern).
**Risks:** Pre-1.0 (0.1.x), 5 weeks old, 222K SLoC. Mitigated by: pin exact version, fork-ready, feature-gate only needed dialects, keep SqlEmitter trait as abstraction boundary.

## DL-026: DuckDB connector via official duckdb crate v1.3.2

**Date:** 2026-03-18
**Status:** Accepted
**Context:** DuckDB connector was a stub (feature flag exists, no implementation). `duckdb` crate v1.3.2 is the official Rust binding (1.58M downloads, arrow 55 compatible).
**Decision:** Implement `DuckDbConnector` using `duckdb = "1.3"` with `bundled` feature. Execute SQL via `stmt.query_arrow()` -> `Vec<RecordBatch>`. Wrap blocking calls in `tokio::task::spawn_blocking` since `Connection` is `Send` but `!Sync`.
**Rationale:** Arrow 55 matches our workspace — no version bump. Official bindings, production maturity. Embedded (in-process) execution with native Parquet/CSV/Iceberg/S3 reading via DuckDB extensions.
**Trade-off:** `bundled` feature compiles DuckDB C++ from source (several minutes first build). Acceptable for feature-gated dependency.

## DL-027: Trino connector via trino-rust-client v0.9.3

**Date:** 2026-03-18
**Status:** Accepted
**Context:** Trino connector was a stub. `trino-rust-client` v0.9.3 (MIT, 36K downloads, Feb 2026) is an actively maintained fork of prusto with Basic Auth, JWT, spooling protocol, and dynamic Row type.
**Decision:** Implement `TrinoConnector` using `trino-rust-client`. Submit SQL via REST v1/statement, receive JSON rows. Add JSON -> Arrow conversion layer. Fallback to raw reqwest implementation if crate proves insufficient (reuse IcebergRestCatalog patterns).
**Rationale:** Handles pagination (nextUri), spooling protocol, compression. ~4K SLoC — small enough to audit/fork. No Arrow Flight SQL option (Trino does not support it).

## DL-028: Spark connector via Apache spark-connect-rs (forked)

**Date:** 2026-03-18
**Status:** Accepted
**Context:** Apache `spark-connect-rust` (github.com/apache/spark-connect-rust) is the official Apache Spark Connect Rust client. Same codebase as community `spark-connect-rs` (donated by sjrusso8). Uses arrow 55 (matches), but tonic 0.11 (we have 0.12) and prost 0.12 (we have 0.14).
**Decision:** Fork the Apache repo, bump tonic 0.11->0.12 and prost 0.12->0.14 to match workspace. Primary path: SQL string execution via `spark.sql()` -> `RecordBatch`. UnresolvedLogicalPlan submission via DataFrame API available for future v3 optimization. Contribute dep bumps upstream.
**Rationale:** Only Apache-blessed Rust client for Spark. Arrow 55 compatible. Supports SQL + catalog + streaming. Fork necessary due to dep version mismatch. Small codebase (~10K SLoC) makes fork maintenance feasible.
**Risks:** Experimental status, low commit velocity (2 commits in 6 months), 3-4 contributors. protoc build requirement. Mitigated by: fork under our control, pin to specific commit, SQL path is simple and stable.

## DL-029: Arrow Flight SQL deferred to Databricks-specific connector

**Date:** 2026-03-18
**Status:** Accepted
**Context:** `arrow-flight` crate (v58.0.0, 9.4M downloads) provides production-grade `FlightSqlServiceClient`. However, neither Spark nor Trino natively expose Flight SQL endpoints. Only Databricks and purpose-built Flight SQL servers (Dremio, Ballista) support it.
**Decision:** Defer Flight SQL connector to v2+ as a Databricks-specific or generic Flight-compatible connector. Not part of the Spark or Trino connector implementations.
**Rationale:** Flight SQL is the right long-term protocol for zero-copy Arrow data transfer, but the server-side ecosystem is not there yet for our target engines. When Databricks support is needed, `arrow-flight` is the clear choice.
