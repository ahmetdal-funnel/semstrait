# Semstrait Implementation Plan

**Version:** 4.0 | **Status:** Active | **Target:** V1.1 polish + V2 connectors & SQL layer
**Authoritative source:** CONTEXT.md (v3.1)

---

## V1 Status: Complete

All 10 crates compiling and tested (261 tests with datafusion feature).
Full E2E pipeline: YAML -> compile -> plan -> SQL -> DataFusion execute -> JSON.
Binary: `cargo build -p semstrait-api --features cli,rest,datafusion`

---

## V1.1 Scope — Polish & Hardening

Fix known issues, add missing tests, complete deferred v1 items.

### V1.1-A: Critical — Test Coverage Gaps

| Task | Description | Crate |
|------|-------------|-------|
| A.1 | Unionset planner tests (multi-dataset UNION ALL, NULL-fill) | semstrait-planner |
| A.2 | Joinset planner tests (BFS anchor, multi-hop join chain) | semstrait-planner |
| A.3 | Clippy clean — all crates, deny warnings | workspace |

### V1.1-B: Important — Deferred V1 Items

| Task | Description | Crate | Ref |
|------|-------------|-------|-----|
| B.1 | Aggregation constraint checking (allowed/prohibited functions) | semstrait-planner | DL-022 |
| B.2 | Domain filter step in planner (domain_hint routing) | semstrait-planner | DL-021 |
| B.3 | Glob namespace from model (not hardcoded "default") | semstrait-manifest | CONTEXT.md §8 |
| B.4 | REST /schema and /compile endpoints | semstrait-api | CONTEXT.md §8 |
| B.5 | Kind-level filter block (applies to all queries against a kind) | semstrait-planner | CONTEXT.md §8 |
| B.6 | UNION DISTINCT support (alongside UNION ALL) | semstrait-planner | CONTEXT.md §8 |

### V1.1-C: Low Priority — Cleanup

| Task | Description | Crate | Ref |
|------|-------------|-------|-----|
| C.1 | Rename IR DslExpr to IrExpr (eliminate name collision) | semstrait-ir + dependents | DL-020 |
| C.2 | ComputeEmitter integration into engine pipeline | semstrait-api | DL-023 |
| C.3 | Schema drift detection (warn PLAN_W003) | semstrait-planner | CONTEXT.md §8 |

---

## V2 Scope — Connectors, SQL Layer, Catalogs

### Research Results (2026-03-18)

Library evaluation for v2 connector and SQL implementations:

| Component | Library | Version | Arrow Compat | Maturity | Decision |
|-----------|---------|---------|-------------|----------|----------|
| SQL transpilation | `polyglot-sql` | 0.1.15 | N/A | Early (726 stars, 18K tests) | Adopt as SQL core abstraction (DL-025) |
| SQL parsing (future) | `sqlparser-rs` | 0.53 (workspace) | N/A | Production (4.3M/mo) | Keep for future expr parsing |
| DuckDB connector | `duckdb` | 1.3.2 | arrow 55 | Production (1.58M DL) | Adopt with `bundled` feature (DL-026) |
| Trino connector | `trino-rust-client` | 0.9.3 | N/A (JSON) | Medium-High (36K DL) | Adopt, add JSON->Arrow layer (DL-027) |
| Spark connector | Apache `spark-connect-rs` | 0.0.2 | arrow 55 | Experimental (Apache) | Fork, bump deps (DL-028) |
| Flight SQL | `arrow-flight` | 58.0.0 | arrow 58 | Production (9.4M DL) | Deferred — for Databricks (DL-029) |

### V2-A: SQL Layer — polyglot-sql Integration (High Priority)

Replace custom `SqlDialect` + `AnsiSqlEmitter` + `DslExprSqlRenderer` with polyglot-sql's
builder API + dialect-aware `generate()`. Eliminates per-dialect reimplementation risk.

| Task | Description | Crate |
|------|-------------|-------|
| A.1 | Add `polyglot-sql` to semstrait-sql with feature-gated dialects | semstrait-sql |
| A.2 | Implement `PolyglotEmitter`: PlanNode -> polyglot Expression via builder API | semstrait-sql |
| A.3 | Map DslExpr -> polyglot Expr (column, literal, binary, case, aggregate, etc.) | semstrait-sql |
| A.4 | Thin wrappers for builder gaps (sum_distinct, date_trunc, window_over) | semstrait-sql |
| A.5 | Add SparkDialect via polyglot's DialectType::Spark | semstrait-sql |
| A.6 | Validate output equivalence — all 261 tests must pass with polyglot emitter | semstrait-sql |
| A.7 | Deprecate AnsiSqlEmitter, DslExprSqlRenderer, SqlDialect trait impls | semstrait-sql |
| A.8 | Keep SqlEmitter trait as public API — polyglot is implementation detail | semstrait-sql |

**Architecture:**
```
BEFORE: PlanNode -> AnsiSqlEmitter -> SqlDialect (string building)
AFTER:  PlanNode -> PolyglotEmitter -> polyglot::builder -> generate(dialect)
```

**Feature flags (semstrait-sql/Cargo.toml):**
```toml
polyglot-sql = { version = "0.1", default-features = false, features = [
    "dialect-duckdb", "dialect-trino", "dialect-spark",
    "dialect-snowflake", "dialect-databricks", "dialect-postgresql",
    "dialect-datafusion"
] }
```

**Risk mitigation:** Pin exact version. Fork-ready (MIT, pure Rust, zero C deps).

---

### V2-B: DuckDB Connector (High Priority)

Implement `DuckDbConnector` using official `duckdb` crate v1.3.2 (arrow 55 compatible).

| Task | Description | Crate |
|------|-------------|-------|
| B.1 | Add `duckdb = "1.3"` with `bundled` feature to workspace + connectors | semstrait-connectors |
| B.2 | Implement `DuckDbConnector` struct (Connection, profile) | semstrait-connectors |
| B.3 | Implement `ComputeEmitter` — emit_sql wraps SQL string | semstrait-connectors |
| B.4 | Implement `ComputeAdapter` — consumer_profile for DuckDB capabilities | semstrait-connectors |
| B.5 | Implement `ComputeConnector` — execute via query_arrow(), spawn_blocking | semstrait-connectors |
| B.6 | Register CSV/Parquet/file helpers (like DataFusion connector) | semstrait-connectors |
| B.7 | health_check via SELECT 1 | semstrait-connectors |
| B.8 | Tests: query execution, file registration, error handling | semstrait-connectors |
| B.9 | Wire into SemstraitEngine and CLI (feature = "duckdb") | semstrait-api |

**Key pattern:** `Connection` is `Send` but `!Sync` — wrap all calls in `tokio::task::spawn_blocking`.

---

### V2-C: Trino Connector (Medium Priority)

Implement `TrinoConnector` using `trino-rust-client` v0.9.3.

| Task | Description | Crate |
|------|-------------|-------|
| C.1 | Add `trino-rust-client = "0.9"` to workspace + connectors | semstrait-connectors |
| C.2 | Implement `TrinoConnector` struct (client, config, profile) | semstrait-connectors |
| C.3 | Implement `ComputeConnector` — submit SQL via REST, receive JSON rows | semstrait-connectors |
| C.4 | JSON -> Arrow RecordBatch conversion layer | semstrait-connectors |
| C.5 | Authentication: Basic Auth + JWT (reuse OAuth2 patterns from Iceberg) | semstrait-connectors |
| C.6 | health_check via system query | semstrait-connectors |
| C.7 | Tests with mock server or integration test fixtures | semstrait-connectors |
| C.8 | Wire into SemstraitEngine and CLI (feature = "trino") | semstrait-api |

**Fallback:** If trino-rust-client proves insufficient, implement directly with reqwest (~800 lines).
We already have the patterns from IcebergRestCatalog.

---

### V2-D: Spark Connector (Medium Priority)

Implement `SparkConnector` using Apache `spark-connect-rs` (forked for dep alignment).

| Task | Description | Crate |
|------|-------------|-------|
| D.1 | Fork apache/spark-connect-rust, bump prost 0.12->0.14, tonic 0.11->0.12 | external |
| D.2 | Add forked spark-connect-rs to workspace (git dep or vendored) | semstrait-connectors |
| D.3 | Implement `SparkConnector` struct (SparkSession, profile) | semstrait-connectors |
| D.4 | Implement `ComputeConnector` — SQL execution path (spark.sql()) | semstrait-connectors |
| D.5 | Arrow RecordBatch results (native from spark-connect-rs) | semstrait-connectors |
| D.6 | Authentication: bearer token via connection string | semstrait-connectors |
| D.7 | health_check via spark.version() or SELECT 1 | semstrait-connectors |
| D.8 | Tests with Spark Connect mock or docker-compose | semstrait-connectors |
| D.9 | Wire into SemstraitEngine and CLI (feature = "spark") | semstrait-api |

**Future (v3):** UnresolvedLogicalPlan submission — convert PlanNode IR -> Spark Relation proto.
This bypasses SQL entirely but requires tight coupling to Spark Connect protos.

---

### V2-E: Catalog Expansion (Lower Priority)

| Task | Description | Crate |
|------|-------------|-------|
| E.1 | UnityCatalog (Databricks) — REST API client | semstrait-catalog |
| E.2 | GlueCatalog (AWS) — aws-sdk-glue integration | semstrait-catalog |
| E.3 | HiveCatalog — Thrift/HTTP metastore client | semstrait-catalog |
| E.4 | FileSystemRepository for persistent manifest storage | semstrait-manifest |
| E.5 | ObjectStoreRepository (S3/GCS/Azure) | semstrait-manifest |

---

### V2-F: API Expansion (Lower Priority)

| Task | Description | Crate |
|------|-------------|-------|
| F.1 | gRPC transport implementation (tonic server) | semstrait-api |
| F.2 | Arrow Flight SQL server for client connectivity | semstrait-api |
| F.3 | Multi-engine query fan-out (one query, multiple connectors) | semstrait-api |

---

## Execution Order

```
V1.1-A (test gaps, clippy)        ──── IMMEDIATE (critical for quality gate)
V1.1-B (deferred v1 items)        ──── NEXT (important but not blocking)
V2-A (polyglot SQL layer)         ──┐
V2-B (DuckDB connector)           ──┤─ PARALLEL (independent crates)
V2-C (Trino connector)            ──┘
V2-D (Spark connector)            ──── AFTER fork prep (dep alignment needed)
V1.1-C (cleanup/rename)           ──── AFTER V2-A (rename fits with SQL refactor)
V2-E (catalogs)                   ──── INDEPENDENT (any time)
V2-F (API expansion)              ──── LAST (depends on connector maturity)
```

---

## Progress

| Phase | Status |
|-------|--------|
| 1-6 — V1 Implementation | Complete |
| 7 — Documentation reconciliation | Complete |
| V1.1-A — Test coverage gaps | **Complete** |
| V1.1-B — Deferred v1 items | **Partial** (B.1-B.4 complete, B.5-B.6 pending) |
| V2-A — polyglot SQL layer | Pending |
| V2-B — DuckDB connector | Pending |
| V2-C — Trino connector | Pending |
| V2-D — Spark connector | Pending |
| V2-E — Catalog expansion | Pending |
| V2-F — API expansion | Pending |

### Completed milestones (2026-03-17 — 2026-03-18)
- All 10 crates compiling and tested (278 tests with datafusion feature, 0 clippy warnings)
- Full E2E pipeline: YAML -> compile -> plan -> SQL -> DataFusion execute -> JSON
- Binary: `cargo build -p semstrait-api --features cli,rest,datafusion`
- CLI commands: compile, explain, validate, query (datafusion), serve (rest)
- Iceberg REST catalog with OAuth2 token lifecycle
- Substrait round-trip for all DslExpr variants
- ANSI FETCH FIRST / dialect-aware limit_clause()
- ConsumerProfile wired from connector through engine to planner
- Schema HashMap ordinal index for O(1) lookups (IR)
- Documentation reconciliation complete (CONTEXT.md v3.1, DECISION_LOG DL-001..024)
- Library research complete: polyglot-sql, duckdb, trino-rust-client, spark-connect-rs
- V1.1-A complete: Unionset planner (4 tests), Joinset planner (5 tests), Clippy clean (0 warnings)
- V1.1-B partial: Aggregation constraints (B.1, 6 tests), Domain filter (B.2, 3 tests), Glob namespace (B.3), REST /schema + /compile (B.4)

### Dependencies for V2

| Dependency | Version | Feature | Purpose |
|-----------|---------|---------|---------|
| polyglot-sql | 0.1.15 | dialect-duckdb, dialect-trino, dialect-spark, etc. | SQL transpilation |
| duckdb | 1.3.2 | bundled | DuckDB embedded connector |
| trino-rust-client | 0.9.3 | spooling | Trino REST connector |
| spark-connect-rs | fork | tls | Spark Connect gRPC connector |
| sqlparser | 0.53 | (kept) | Future SQL expr parsing |
