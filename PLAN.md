# Semstrait Implementation Plan

**Version:** 5.0 | **Status:** Active | **Target:** V2 connectors, catalogs, API expansion
**Authoritative source:** CONTEXT.md (v3.6)

---

## Completed (V1 + V1.1 + V2-A + V2-B)

All 10 crates compiling and tested — **311 tests** with `datafusion,duckdb,polyglot` features, 0 clippy warnings.

- Full E2E pipeline: YAML → compile → plan → SQL → DataFusion/DuckDB execute → JSON
- Binary: `cargo build -p semstrait-api --features cli,rest,datafusion,duckdb`
- CLI: compile, explain, validate, query (DataFusion), query-duckdb (DuckDB), serve (REST)
- Iceberg REST catalog with OAuth2 token lifecycle
- Substrait round-trip for all DslExpr variants
- PolyglotEmitter — 34+ SQL dialects via polyglot-sql transpilation
- DuckDB connector — embedded DuckDB 1.3.x, Arc<Mutex<Connection>> + spawn_blocking
- V1.1 complete: Unionset/Joinset planners, aggregation constraints, domain filter, kind-level filters, UNION DISTINCT, REST /schema + /compile

---

## Remaining Work

### V1.1-C: Low Priority — Cleanup

| Task | Description | Crate | Ref |
|------|-------------|-------|-----|
| C.1 | Rename IR `DslExpr` to `IrExpr` (eliminate name collision) | semstrait-ir + dependents | DL-020 |
| C.2 | `ComputeEmitter` integration into engine pipeline | semstrait-api | DL-023 |
| C.3 | Schema drift detection (warn PLAN_W003) | semstrait-planner | CONTEXT.md §8 |

---

### V2-C: Trino Connector

Implement `TrinoConnector` using `trino-rust-client` v0.9.3 (DL-027).

| Task | Description | Crate |
|------|-------------|-------|
| C.1 | Add `trino-rust-client = "0.9"` to workspace + connectors | semstrait-connectors |
| C.2 | Implement `TrinoConnector` struct (client, config, profile) | semstrait-connectors |
| C.3 | Implement `ComputeConnector` — submit SQL via REST, receive JSON rows | semstrait-connectors |
| C.4 | JSON → Arrow RecordBatch conversion layer | semstrait-connectors |
| C.5 | Authentication: Basic Auth + JWT (reuse OAuth2 patterns from Iceberg) | semstrait-connectors |
| C.6 | health_check via system query | semstrait-connectors |
| C.7 | Tests with mock server or integration test fixtures | semstrait-connectors |
| C.8 | Wire into SemstraitEngine and CLI (feature = "trino") | semstrait-api |

**Fallback:** If trino-rust-client proves insufficient, implement directly with reqwest (~800 lines).

---

### V2-D: Spark Connector

Implement `SparkConnector` using Apache `spark-connect-rs` (forked for dep alignment, DL-028).

| Task | Description | Crate |
|------|-------------|-------|
| D.1 | Fork apache/spark-connect-rust, bump prost 0.12→0.14, tonic 0.11→0.12 | external |
| D.2 | Add forked spark-connect-rs to workspace (git dep or vendored) | semstrait-connectors |
| D.3 | Implement `SparkConnector` struct (SparkSession, profile) | semstrait-connectors |
| D.4 | Implement `ComputeConnector` — SQL execution path (spark.sql()) | semstrait-connectors |
| D.5 | Arrow RecordBatch results (native from spark-connect-rs) | semstrait-connectors |
| D.6 | Authentication: bearer token via connection string | semstrait-connectors |
| D.7 | health_check via spark.version() or SELECT 1 | semstrait-connectors |
| D.8 | Tests with Spark Connect mock or docker-compose | semstrait-connectors |
| D.9 | Wire into SemstraitEngine and CLI (feature = "spark") | semstrait-api |

---

### V2-E: Catalog Expansion

| Task | Description | Crate |
|------|-------------|-------|
| E.1 | UnityCatalog (Databricks) — REST API client | semstrait-catalog |
| E.2 | GlueCatalog (AWS) — aws-sdk-glue integration | semstrait-catalog |
| E.3 | HiveCatalog — Thrift/HTTP metastore client | semstrait-catalog |
| E.4 | FileSystemRepository for persistent manifest storage | semstrait-manifest |
| E.5 | ObjectStoreRepository (S3/GCS/Azure) | semstrait-manifest |

---

### V2-F: API Expansion

| Task | Description | Crate |
|------|-------------|-------|
| F.1 | gRPC transport implementation (tonic server) | semstrait-api |
| F.2 | Arrow Flight SQL server for client connectivity | semstrait-api |
| F.3 | Multi-engine query fan-out (one query, multiple connectors) | semstrait-api |

---

## Execution Order

```
V1.1-C (cleanup/rename)     ──── LOW PRIORITY (fits with next refactor)
V2-C (Trino connector)      ──┐
V2-D (Spark connector)      ──┤─ NEXT (independent, parallelizable after fork prep)
V2-E (catalogs)              ──┘
V2-F (API expansion)         ──── LAST (depends on connector maturity)
```

---

## Dependencies for Remaining V2

| Dependency | Version | Feature | Purpose |
|-----------|---------|---------|---------|
| trino-rust-client | 0.9.3 | spooling | Trino REST connector |
| spark-connect-rs | fork | tls | Spark Connect gRPC connector |
| arrow-flight | 58.0.0 | (deferred) | Databricks Flight SQL |
