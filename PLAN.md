# Semstrait Implementation Plan

**Version:** 2.0 | **Status:** Active | **Target:** V1 — plan generation + ANSI SQL
**Authoritative source:** CONTEXT.md (v3.0)

---

## V1 Scope

Generate valid logical plans and ANSI-standard SQL from semantic YAML models.
Human-reviewable output. DataFusion compatible Substrait plans. CLI + REST API.
Iceberg/Polaris catalog (stub for v1). Connectors execution is v2.

### In Scope
- 10-crate workspace per CONTEXT.md section 3
- DSL expression parser (no raw SQL — rejected at compile time)
- ManifestCompiler: YAML -> CompiledManifest (9-step pipeline)
- SemanticPlanner: QueryRequest -> LogicalPlan (kind dispatch)
- PlanNode IR with Substrait serialization + SemAnnotation
- SQL emission (ANSI baseline via sqlparser-rs AST)
- CatalogProvider trait + NullCatalogProvider
- CLI: compile, explain, validate, query commands
- REST API: /query, /explain, /schema, /compile endpoints
- Connector traits (stubs; no execution in v1)
- Optimizer skeleton (empty passes, OptimizerPass trait)
- Test suite: unit + integration + E2E

### Out of Scope (v2)
- Engine execution (DuckDB, Trino, Spark connectors)
- FileSystemRepository / ObjectStoreRepository
- Cross-kind metric refs (COMP_E006)
- Many-to-many junction tables
- UNION DISTINCT (UNION ALL only in v1)
- Multi-engine query fan-out
- column_mapping: auto
- Kind-level filter block

---

## Phase 1: Foundation — semstrait-core

**Goal:** Shared primitives. Zero internal workspace deps.

| Task | Description |
|------|-------------|
| 1.1 | Refactor `DataType` to Arrow-aligned enum per CONTEXT.md 5.1 |
| 1.2 | Implement `Schema`, `SchemaColumn` with ordinal-based lookup |
| 1.3 | Implement `ConsumerProfile` + `SemiAdditiveStrategy` |
| 1.4 | Implement `Grain` enum (Minute..Year) |
| 1.5 | Implement `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints` |
| 1.6 | Implement full `DslExpr` enum per CONTEXT.md 5.1 |
| 1.7 | Define error type hierarchy (`thiserror`) |

**Deps:** none | **External:** serde, thiserror, arrow-schema (type compat)

---

## Phase 2: Definition Layer — semstrait-model + semstrait-catalog

**Goal:** Parse YAML models and abstract catalog access. These two crates are independent.

### semstrait-model (depends on: core)
| Task | Description |
|------|-------------|
| 2.1 | SemanticModel, Dataset, Kind, Dimension, Measure, Metric types |
| 2.2 | DimensionEntry/MeasureEntry/MetricEntry (Inline or Ref) |
| 2.3 | GlobPattern, DatasetName types |
| 2.4 | KindDataset with ColumnMapping + ColumnMappingValue |
| 2.5 | `parse(yaml)` and `resolve_refs(model)` functions |
| 2.6 | KindRelationship, Relationship types |

### semstrait-catalog (depends on: core)
| Task | Description |
|------|-------------|
| 2.7 | `CatalogProvider` async trait (list_tables, get_schema, table_exists) |
| 2.8 | `TableRef`, `CatalogColumn` types |
| 2.9 | `NullCatalogProvider` (no-op for testing) |
| 2.10 | Stub `IcebergRestCatalog` (feature = "iceberg") |

---

## Phase 3: IR + Manifest — semstrait-ir + semstrait-manifest

**Goal:** Plan IR and manifest compilation.

### semstrait-ir (depends on: core)
| Task | Description |
|------|-------------|
| 3.1 | `PlanNode` enum (Scan, Filter, Project, Aggregate, Join, Union, Sort, Fetch) |
| 3.2 | `NodeMeta` with output_schema + annotations |
| 3.3 | `SemAnnotation` enum (AggregateRole, FilterSource, Additivity, KindRef, DomainHint) |
| 3.4 | `ExprConverter` (DslExpr <-> Substrait Expression) |
| 3.5 | `SubstraitSerializer` (LogicalPlan <-> substrait::proto::Plan) |
| 3.6 | Proto definition for SemstraitAnnotation |

### semstrait-manifest (depends on: core, model, catalog)
| Task | Description |
|------|-------------|
| 3.7 | `ManifestCompiler` with 9-step pipeline |
| 3.8 | `CompiledManifest`, `CompiledKind`, `CompiledMeasure` types |
| 3.9 | `Repository` trait + `InMemoryRepository` |
| 3.10 | Metric graph (petgraph) cycle detection, depth <= 3 |
| 3.11 | Relationship graph for joinset anchor inference |
| 3.12 | Expression compilation (DSL parse + reject raw SQL) |
| 3.13 | Glob expansion via CatalogProvider |

---

## Phase 4: Planning + SQL — semstrait-planner + semstrait-sql

**Goal:** Build logical plans and emit SQL.

### semstrait-planner (depends on: core, ir, manifest, catalog)
| Task | Description |
|------|-------------|
| 4.1 | `SemanticPlanner` + `SemanticPlannerBuilder` |
| 4.2 | `ConstraintEvaluator` (step 0 pre-resolution gate) |
| 4.3 | `KindPlanner` trait + `KindPlannerRegistry` |
| 4.4 | `GrainsetPlanner` — coverage-based dataset selection |
| 4.5 | `UnionsetPlanner` — UNION ALL with NULL-fill |
| 4.6 | `JoinsetPlanner` — BFS from anchor, join chain |
| 4.7 | `AdditivityResolver` (window function / double aggregate) |
| 4.8 | Filter injection pipeline (dataset, measure, metric, user) |
| 4.9 | `Optimizer` skeleton + `OptimizerPass` trait |
| 4.10 | `ResolvedQueryRequest` type |

### semstrait-sql (depends on: core, ir)
| Task | Description |
|------|-------------|
| 4.11 | `SqlEmitter` + `SqlDialect` traits |
| 4.12 | `AnsiSqlEmitter` — PlanNode -> sqlparser AST -> String |
| 4.13 | `DslExprSqlRenderer` |
| 4.14 | Dialect stubs: Trino, DuckDB, Spark |

---

## Phase 5: Connectors + API — semstrait-connectors + semstrait-api

**Goal:** Compute interface and entry points.

### semstrait-connectors (depends on: core, ir, sql)
| Task | Description |
|------|-------------|
| 5.1 | `ComputeEmitter`, `ComputeAdapter`, `ComputeConnector` traits |
| 5.2 | `ComputePayload`, `ComputeRequest`, `ComputeResult` types |
| 5.3 | Stub implementations (v1 returns unimplemented) |

### semstrait-api (depends on: core, planner, manifest, connectors)
| Task | Description |
|------|-------------|
| 5.4 | `RequestParser` — RawQueryRequest -> ResolvedQueryRequest |
| 5.5 | `SemstraitEngine` orchestrator |
| 5.6 | CLI submodule (clap): compile, explain, validate, query |
| 5.7 | REST submodule (axum): /query, /explain, /schema, /compile |
| 5.8 | Feature flags: cli, rest, grpc |

---

## Phase 6: Facade + E2E — semstrait

**Goal:** Public API and integration tests.

| Task | Description |
|------|-------------|
| 6.1 | `Semstrait` struct + `SemstraitBuilder` |
| 6.2 | Feature flag coordination |
| 6.3 | Public type re-exports |
| 6.4 | E2E: YAML -> compile -> plan -> SQL -> validate |
| 6.5 | DataFusion roundtrip test (Substrait decode) |
| 6.6 | Update README.md per crate |

---

## Phase 7: Polish + Review

| Task | Description |
|------|-------------|
| 7.1 | Clippy clean (all crates) |
| 7.2 | Documentation pass |
| 7.3 | Review + brainstorm session |

---

## Progress

| Phase | Status |
|-------|--------|
| 1 — Foundation | In Progress |
| 2 — Definition Layer | Pending |
| 3 — IR + Manifest | Pending |
| 4 — Planning + SQL | Pending |
| 5 — Connectors + API | Pending |
| 6 — Facade + E2E | Pending |
| 7 — Polish | Pending |
