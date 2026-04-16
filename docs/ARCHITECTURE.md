# Semstrait Architecture

Manifest compiler + semantic plan-generation library in Rust.

Resolves semantic models (YAML) into engine-executable artifacts:
1. Compile YAML into validated `CompiledManifest` (offline)
2. Plan `QueryRequest`s against manifest into `LogicalPlan` IR (online)
3. Adapt plan into engine artifact -- SQL or Substrait (online)

Primary output: `PlanArtifact` (SQL string or `substrait::proto::Plan`).
Execution is the consumer's responsibility.

---

## Architectural Constraints

Active constraints that guide new code. Historical decisions (D1-D4, D7) archived in DECISION_LOG.md.

| # | Constraint | Rationale |
|---|---|---|
| D5 | Wildcard expansion requires a provider | `storage.paths` -> StorageProvider; `storage.tables` -> CatalogProvider. No silent pass-through. |
| D6 | `CompiledDataKind` has three layers: interface, strategy, binding | Interface = what users query. Strategy = plan structure: **Simple** (single-dataset fast path, wraps `Dataset`) vs **Complex** (`Grainset`, `Unionset`, `Joinset`). Binding = physical impl. See `docs/DATASET.md`, `docs/{GRAINSET,UNIONSET,JOINSET}.md`. |
| D8 | YAML field is `constraints`, not `requires` | Pre-resolution validity gates at step 0, before dataset routing. |
| E1 | Engine selection at request level | `engine` field in `RawQueryRequest`. Model is engine-agnostic. |
| E2 | Artifact driven by engine adapter | `EngineAdapter` trait determines output type. DataFusion -> Substrait. DuckDB/Spark adapters are structural stubs in the current workspace (dialect infrastructure exists, `adapt()` returns `UnsupportedFeature`). |
| E3 | `PlanBuilder` trait in IR, impls in adapter | Breaks planner <-> adapter coupling. Planner depends only on the trait. |
| E5 | Semstrait is a plan-generation library | Primary output is `PlanArtifact`. Consumers own execution. |
| E6 | Primary path: DataFusion + Polaris/Iceberg | Polaris as catalog, DataFusion as compute, Substrait as interchange. |
| E7 | Debug SQL always available | `EngineAdapter::debug_sql()` generates ANSI SQL regardless of primary artifact type. |

---

## Crate Workspace

```
semstrait/                       Cargo workspace root
+-- semstrait-core/              Foundation -- shared primitives (Expr, DataType, Schema)
+-- semstrait-model/             YAML model parsing and ref resolution
+-- semstrait-catalog/           CatalogProvider trait + implementations (Iceberg, Unity)
+-- semstrait-manifest/          ManifestCompiler + Repository (InMemory + FileSystem)
+-- semstrait-ir/                PlanNode IR + Substrait bridge + PlanBuilder trait
+-- semstrait-planner/           SemanticPlanner + DataKindPlanners + Optimizer
+-- semstrait-adapter/           EngineAdapter trait + SqlEmitter + dialect impls
+-- semstrait-api/               gRPC + REST + CLI (feature-gated)
+-- semstrait/                   Facade -- builder, public API, feature flags
```

### Dependency Rules

**Hard rules (enforced by Cargo):**
1. No cycles. Any change that creates a cycle is rejected.
2. `semstrait-core` has zero internal workspace dependencies.
3. Only `semstrait-api` and `semstrait` (facade) may depend on the full stack.

| Crate | Depends on |
|---|---|
| `semstrait-core` | *(nothing internal)* |
| `semstrait-model` | `core` |
| `semstrait-catalog` | `core` |
| `semstrait-manifest` | `core`, `model`, `catalog` |
| `semstrait-ir` | `core` |
| `semstrait-planner` | `core`, `ir`, `manifest`, `catalog` |
| `semstrait-adapter` | `core`, `ir`, `polyglot-sql` (optional) |
| `semstrait-api` | `core`, `ir`, `planner`, `manifest`, `adapter`, `catalog` |
| `semstrait` (facade) | `core`, `ir`, `planner`, `manifest`, `adapter`, `catalog` |

### Crate Layer Diagram

```
+---------------------------------------------------------------------+
|  Entry points                                                        |
|  semstrait (facade)  /  semstrait-api (grpc, rest, cli)             |
+--------------------------------+------------------------------------+
                                 | depends on
+--------------------------------v------------------------------------+
|  Adapter layer                                                       |
|  semstrait-adapter -- EngineAdapter + SqlEmitter + dialects          |
+--------------------------------+------------------------------------+
                                 |
+--------------------------------v------------------------------------+
|  Planning layer                                                      |
|  semstrait-planner -- kind planners, Optimizer (empty by default)    |
+--------------------------------+------------------------------------+
                                 |
+--------------------------------v------------------------------------+
|  IR + Manifest layer                                                 |
|  semstrait-ir -- PlanNode, Substrait, PlanBuilder trait              |
|  semstrait-manifest -- ManifestCompiler, Repository (InMem + FileSys) |
+--------------------------------+------------------------------------+
                                 |
+--------------------------------v------------------------------------+
|  Definition layer                                                    |
|  semstrait-model -- parsed YAML types, ref resolution                |
|  semstrait-catalog -- CatalogProvider, StorageProvider                |
+--------------------------------+------------------------------------+
                                 |
+--------------------------------v------------------------------------+
|  Foundation -- semstrait-core                                        |
|  Schema, DataType, Expr, Grain, errors                               |
+---------------------------------------------------------------------+
```

---

## System Pipeline

```
---------------------------------+--------------------------------------
COMPILE TIME (offline)           |  QUERY TIME (online)
---------------------------------+--------------------------------------
                                 |
YAML source files                |  QueryRequest { from, select, engine }
        |                        |          |
        v                        |          v
CatalogProvider --+              |  RequestParser
(optional; req    |              |    parse, resolve refs, grain coerce
 for globs)       |              |          |
                  v              |          v
ManifestCompiler.compile()  <----+  ConstraintValidator (step 0)
  parse -> expand globs          |    pre-resolution validity gate
  validate -> compile exprs      |          |
  build petgraph DAG             |          v
        |                        |  SemanticPlanner (uses &dyn PlanBuilder)
        v                        |    kind dispatch, additivity, filters
CompiledManifest --- loaded ---->|          |
        |                        |          v
        v                        |  LogicalPlan (PlanNode IR)
InMemoryRepository.save()        |          |
                                 |          v (internal)
                                 |  Optimizer.apply()  <- empty by default
                                 |          |
                                 |          v
                                 |  EngineAdapter.adapt(plan)
                                 |    +-----+-----+
                                 |    v           v
                                 |  Sql(String)  Substrait(Plan)
                                 |    +-----+-----+
                                 |          v
                                 |  PlanArtifact <- primary output
---------------------------------+--------------------------------------
```

---

## Connection Verification

Cross-crate type references -- confirms no cycles.

| Type | Defined in | Used in | OK |
|---|---|---|---|
| `Schema`, `DataType`, `Expr` | `core` | All crates | all depend on core |
| `PlanBuilder` trait | `ir` | `planner`, `adapter` | both depend on ir |
| `SemanticModel`, `DataKind` | `model` | `manifest` only | model -> core |
| `CatalogProvider` trait | `catalog` | `manifest`, `planner` | catalog -> core only |
| `CompiledManifest`, `CompiledDataKind` | `manifest` | `planner`, `api`, `facade` | no dep on planner |
| `PlanNode`, `LogicalPlan` | `ir` | `planner`, `adapter` | ir -> core only |
| `PlanArtifact` | `ir` | `adapter`, `api`, `facade` | ir -> core |
| `EngineAdapter` trait | `adapter` | `api`, `facade` | adapter -> core, ir |
| `DataKindPlanner` trait | `planner` | `planner` (internal) | internal |

**No cycles.** Strict DAG with `semstrait-core` as root.

---

## Deferred Items

| Item | Notes |
|---|---|
| DuckDB / Spark adapter execution | Dialect + adapter shells exist; `adapt()` returns `UnsupportedFeature` |
| Cross-kind metric refs | Prohibited; multi-kind planning deferred |
| Glue / Hive catalogs | Iceberg REST and Unity done; Glue/Hive deferred |
| Two-stage metric aggregation | Metric-level `agg:` with inner/outer grain |
| Ratio / window structured aggregation | `ratio:` and `window:` YAML tags |
| Model hash caching | Content hash as manifest cache key |

---

## Diagrams

The text diagrams above (Crate Layer, System Pipeline) are the authoritative architecture reference.

---

## Design Document Index

Read the relevant document before working on the corresponding area.

| Task Area | Document |
|-----------|----------|
| Catalog, storage providers, source resolution | `docs/CATALOG_RESOLUTION.md` |
| Function mapping between IR and engines | `docs/FUNCTION_CATALOG.md` |
| Grainset planning (multi-grain, multi-dataset) | `docs/GRAINSET.md` |
| Unionset planning (UNION ALL/DISTINCT) | `docs/UNIONSET.md` |
| Joinset planning (BFS join chains) | `docs/JOINSET.md` |
| Dataset planning (single-dataset fast path) | `docs/DATASET.md` |
| Semantic model scoping rules | `docs/SEMANTIC_RESOLUTION.md` |
| Computed dimensions and expression system | `docs/COMPUTED_EXPRESSIONS.md` |
| Known technical debt | `docs/TECH_DEBT.md` |

### Per-crate documentation

Each crate's `README.md` contains: purpose, module map, key types/traits, control flows, feature flags, and dependencies. **Read the crate README before modifying that crate.**
