# Semstrait Architecture Document
**Version:** 6.0 | **Status:** V1 — Core functionality implementation. All internal development phases (model redesign, DataKind planner, declarative measures, computed expressions, planner restructuring, temporal schema redesign, compiled layer consolidation, adapter restructuring) are part of V1.

---

## 1. What Semstrait Is

A **manifest compiler + semantic plan generation library** written in Rust.

It resolves semantic models (defined in YAML) into engine-executable artifacts by:
1. Compiling YAML model files into a validated `CompiledManifest` artifact (offline)
2. Planning `QueryRequest`s against that manifest into a `LogicalPlan` IR (online)
3. Adapting the plan into an engine-appropriate artifact — SQL or Substrait (online)
4. Optionally executing the artifact via a compute connector (online, convenience)

The primary output is `PlanArtifact` — either a SQL string or `substrait::proto::Plan`.
Optional execution produces `ComputeResult` — Arrow `RecordBatch`es or JSON.

---

## 2. Architectural Constraints

Active constraints that guide new code. Historical crate-organization decisions (D1–D4, D7) are archived in DECISION_LOG.md.

| # | Constraint | Rationale |
|---|---|---|
| D5 | Wildcard expansion requires a provider | Wildcard patterns in `storage.paths` require a `StorageProvider`; wildcards in `storage.tables` require a `CatalogProvider` (via `CatalogRegistry`). No silent pass-through of unresolved globs. |
| D6 | `Kind` has three distinct layers: interface, strategy, binding | Interface = what users query. Strategy (`KindType`) = plan structure. Binding = physical implementation. |
| D8 | YAML field is `constraints`, not `requires` | Pre-resolution validity gates at step 0, before dataset routing. Apply to query scope, not datasets. |
| E1 | Engine selection at request level | `engine` field in `RawQueryRequest`. Same server targets different engines per query. Model is engine-agnostic. |
| E2 | Artifact driven by engine adapter | `EngineAdapter` trait determines output type. DataFusion → Substrait, DuckDB/Trino → SQL. |
| E3 | `PlanBuilder` trait in IR, concrete impls in adapter | Breaks planner ↔ adapter coupling. Planner depends only on the trait. |
| E4 | Adapters and connectors are separate crates | `semstrait-adapter` (plan generation) vs `semstrait-connectors` (optional execution). |
| E5 | Semstrait is a plan-generation library | Primary output is `PlanArtifact`. Connectors are convenience for testing/CLI. |
| E6 | Primary path: DataFusion + Polaris/Iceberg | Polaris as catalog, DataFusion as compute, Substrait as interchange format. |
| E7 | Debug SQL always available | `EngineAdapter::debug_sql()` generates ANSI SQL regardless of primary artifact type. Default impl provided. |

---

## 2b. Versioning & V1 Scope

**Semstrait is V1.** There is no V2. All internal development phases (model redesign, DataKind planner, declarative measures, computed expressions, temporal schema, adapter/connector decoupling) are part of V1 core functionality. Future versions will follow V1 semantic versioning (1.x).

**V1 focus areas:**
1. **E2E functional query generation** — all core analytical functions (aggregation, additivity, grain, temporal, computed expressions, constraints)
2. **DataFusion as primary adapter** — Substrait as canonical IR, DataFusion-native execution
3. **Stable API** — for extending/constructing models and resolving query requests

| Area | V1 Support | Future (V1.x) |
|------|------------|----------------|
| **Compute** | DataFusion (SQL + Substrait) | DuckDB, Trino, Spark, Snowflake |
| **Catalog** | Iceberg REST (via Polaris) | Unity, Glue, Hive Metastore |
| **Storage** | Local paths (via `StorageProvider`), S3 (stub). Format-aware: Iceberg, Parquet, CSV | GCS, ADLS, HDFS |
| **Auth** | OAuth2, Bearer, Polaris (AWS Secrets) | — |
| **Multi-catalog** | Named catalogs via `catalogs.yaml` + `CatalogRegistry`. Per-entity `CatalogRef` | — |
| **IR** | Substrait (canonical, DataFusion native), SQL (on-demand, all engines) | — |
| **Optimizer** | Identity (no-op) | Predicate pushdown, projection pruning |
| **Repository** | InMemoryRepository | FileSystemRepository |
| **Ad-hoc joins** | Single-dataset resolution | Multi-dataset join synthesis |
| **Catalog resolution** | Source resolution (best-effort) | Strict mode, schema evolution tracking |
| **RLS** | Out of scope | Separate system layer |

---

## 3. Crate Workspace

```
semstrait/                       Cargo workspace root
├── semstrait-core/              Foundation — shared primitives
├── semstrait-model/             YAML model parsing and ref resolution
├── semstrait-catalog/           CatalogProvider trait + implementations (Iceberg/Polaris, Unity)
├── semstrait-manifest/          ManifestCompiler + Repository (InMemory v1)
├── semstrait-ir/                PlanNode IR + Substrait bridge + PlanArtifact + PlanBuilder trait
├── semstrait-planner/           SemanticPlanner + KindPlanners + Optimizer
├── semstrait-adapter/           EngineAdapter trait + SqlEmitter + dialect impls (includes former semstrait-sql)
├── semstrait-connectors/        ComputeConnector — optional execution (feature-gated)
├── semstrait-api/               gRPC + REST + CLI (submodules, feature-gated)
└── semstrait/                   Facade — builder, public API, feature flags
```

### 3.1 Dependency Rules

**Hard rules — enforced by Cargo:**
1. No cycles. Any proposed change that creates a cycle is rejected.
2. `semstrait-core` has zero internal workspace dependencies.
3. `semstrait-connectors` leaf engine modules (`duckdb`, `datafusion`, `trino`, `spark`) have no reverse dependencies.
4. `semstrait-api` and `semstrait` (facade) are the only crates that may depend on the full stack.

**Dependency table (direct only):**

| Crate | Depends on |
|---|---|
| `semstrait-core` | *(nothing internal)* |
| `semstrait-model` | `semstrait-core` |
| `semstrait-catalog` | `semstrait-core` |
| `semstrait-manifest` | `semstrait-core`, `semstrait-model`, `semstrait-catalog` |
| `semstrait-ir` | `semstrait-core` |
| `semstrait-planner` | `semstrait-core`, `semstrait-ir`, `semstrait-manifest`, `semstrait-catalog` |
| `semstrait-adapter` | `semstrait-core`, `semstrait-ir`, `polyglot-sql` (optional) |
| `semstrait-connectors` | `semstrait-adapter`, `semstrait-ir` |
| `semstrait-api` | `semstrait-core`, `semstrait-ir`, `semstrait-planner`, `semstrait-manifest`, `semstrait-adapter`, `semstrait-connectors`, `semstrait-catalog` |
| `semstrait` (facade) | `semstrait-core`, `semstrait-ir`, `semstrait-planner`, `semstrait-manifest`, `semstrait-adapter`, `semstrait-connectors`, `semstrait-catalog` |

**Connection verification — cycle check:**
- `semstrait-connectors` depends on `semstrait-adapter` (for `EngineAdapter` trait and per-engine adapters) and `semstrait-ir` (for `PlanArtifact`). No cycle with `semstrait-planner`. ✓
- `semstrait-planner` uses `PlanBuilder` (from `semstrait-ir`) for engine-specific node construction. It does NOT depend on connectors or adapters. ✓
- `semstrait-adapter` bridges `LogicalPlan` (IR) + internal `SqlEmitter` into `PlanArtifact`. No upward dependencies. ✓
- `semstrait-ir` must NOT depend on `semstrait-planner` or `semstrait-connectors`. IR nodes are data structures, not planners. ✓

### 3.2 Diagram 1 — Crate Layer Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Entry points                                                        │
│  semstrait (facade · builder · feature flags)                       │
│  semstrait-api (grpc · rest · cli submodules — feature-gated)       │
└──────────────────────────────┬──────────────────────────────────────┘
                               │ depends on ↓
┌──────────────────────────────▼──────────────────────────────────────┐
│  Execution layer (optional)                                          │
│  semstrait-connectors — ComputeConnector trait                      │
│  engine impls: datafusion · duckdb · trino · spark (feature-gated)  │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  Adapter layer                                                       │
│  semstrait-adapter — EngineAdapter trait · SqlEmitter · dialects     │
│  adapts LogicalPlan → PlanArtifact (SQL or Substrait)               │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  Planning layer                                                      │
│  semstrait-planner — kind planners · Optimizer (empty v1) ·         │
│    ConstraintValidator · AdditivityResolver                         │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  IR layer                                                            │
│  semstrait-ir — PlanNode · SemAnnotation · SubstraitSerializer ·     │
│    PlanArtifact · PlanBuilder trait                                   │
│  semstrait-manifest — ManifestCompiler · InMemoryRepository         │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  Definition layer                                                    │
│  semstrait-model — parsed YAML types · ref resolution · GlobPattern  │
│  semstrait-catalog — CatalogProvider · iceberg/polaris · unity       │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┘
│  Foundation — semstrait-core                                         │
│  Schema · DataType · Expr · Grain · errors                           │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 4. System Pipeline

### Diagram 2 — Full Pipeline (Compile + Query)

```
──────────────────────────────────┬──────────────────────────────────────
COMPILE TIME (offline)            │  QUERY TIME (online)
──────────────────────────────────┼──────────────────────────────────────
                                  │
YAML source files                 │  QueryRequest { from, select, engine }
        │                         │          │
        ▼                         │          ▼
CatalogProvider ──┐               │  RequestParser
(optional; req    │               │    parse · resolve refs · grain coerce
 for globs)       │               │          │
                  ▼               │          ▼
ManifestCompiler.compile() ◄──────┤  ConstraintValidator (step 0)
  parse → expand globs            │    pre-resolution validity gate
  validate → compile expressions  │          │  (Err → PlannerError::ConstraintViolation)
  build petgraph DAG              │          ▼
        │                         │  SemanticPlanner (uses &dyn PlanBuilder)
        ▼                         │    kind dispatch · additivity · filters
CompiledManifest ─── loaded ──────►          │
        │                         │          ▼
        ▼                         │  LogicalPlan (PlanNode IR)
InMemoryRepository.save()         │    Substrait-serializable + SemAnnotation
                                  │          │
                                  │          ▼ (internal to planner)
                                  │  Optimizer.apply()  ← empty in v1
                                  │          │
                                  │          ▼
                                  │  EngineAdapter.adapt(plan)
                                  │    engine-specific artifact generation
                                  │          │
                                  │    ┌─────┴─────┐
                                  │    ▼           ▼
                                  │  Sql(String)  Substrait(proto::Plan)
                                  │    │           │
                                  │    └─────┬─────┘
                                  │          ▼
                                  │  PlanArtifact ← primary output
                                  │          │
                                  │          ▼ (optional — connector execution)
                                  │  ComputeConnector.execute(artifact)
                                  │    datafusion · duckdb · trino · spark
                                  │          │
                                  │          ▼
                                  │  ComputeResult → JSON rows or Arrow
──────────────────────────────────┴──────────────────────────────────────
```

---

## 5. Module Specifications

### 5.1 `semstrait-core`

**Role:** Shared primitives. Every other crate depends on this. Must stay small and zero-dep within the workspace.

**Key types:**

```rust
/// Note: There are TWO Schema types in the workspace:
///   1. `semstrait_core::Schema` — simple Vec<SchemaColumn> with linear ordinal scan.
///      Used by manifest compiler for compile-time schema tracking.
///   2. `semstrait_ir::Schema` — with HashMap<String, usize> index for O(1) ordinal.
///      Used by PlanNode metadata, ExprConverter, and SQL renderers.
///      See §5.5 for the full IR Schema definition.
///
/// The IR Schema's HashMap index is built at construction and maintained through Clone.
/// PartialEq compares only fields (index is derived). #[serde(skip)] on the index.

/// ANSI SQL logical type system. Adapter layer maps to engine-specific physical types.
/// 8 variants — eliminates lossy Substrait type conversions (DL-044).
pub enum DataType {
    Integer,                          // ANSI INTEGER/BIGINT → engine maps to Int64
    Number,                           // ANSI DOUBLE PRECISION → engine maps to Float64
    Decimal { precision: u8, scale: i8 },
    String,                           // ANSI VARCHAR → engine maps to Utf8
    Boolean,
    Date,                             // ANSI DATE → engine maps to Date32
    Timestamp { precision: u8 },      // ANSI TIMESTAMP(p) — 0=sec, 3=ms, 6=us
    Binary,
}

/// Temporal grain levels — used in dimension types and DateTrunc expressions.
pub enum Grain { Minute, Hour, Day, Week, Month, Quarter, Year }

/// MeasureConstraints — the runtime type for the `constraints:` YAML field.
/// Evaluated at step 0 (pre-resolution). This is NOT `requires`.
///
/// Note: There are TWO sets of constraint types in the workspace:
///   1. `semstrait_core::constraints` — uses Vec<String> with serde skip_serializing_if.
///      Defined in core for shared use, but currently unused by other crates.
///   2. `semstrait_model::types` — uses Option<Vec<String>> for YAML deserialization.
///      Re-exported via semstrait-manifest. Used by ConstraintValidator and CompiledManifest.
///
/// The model types (shown below) are the ones used in practice:
pub struct MeasureConstraints {
    pub dimensions:   Option<DimensionConstraints>,
    pub aggregations: Option<AggregationConstraints>,
}
pub struct DimensionConstraints {
    pub one_of:  Option<Vec<String>>,
    pub none_of: Option<Vec<String>>,
    pub all:     Option<Vec<String>>,
}
pub struct AggregationConstraints {
    pub allowed:    Option<Vec<String>>,
    pub prohibited: Option<Vec<String>>,
}

/// Unified expression tree — the ONLY way to express computations.
/// Raw SQL strings are rejected at compile time.
///
/// Unified `Expr` in `semstrait-core::expr` (DL-020, Phases 1-5 complete).
/// Used from YAML parsing through planning, IR, SQL emission, and Substrait
/// serialization. Replaces the former dual DslExpr architecture.
///
/// Old `core::DslExpr` and `ir::DslExpr` alias removed (Phases 4+6 complete).
pub enum Expr {
    Column(ColumnRef),
    Literal(Literal),          // Integer(i64), Float(f64), String, Boolean, Null
    EntityRef(EntityRef),      // pre-resolution only; resolved to Column during planning
    Aggregate(AggregateExpr),  // typed: Sum, Avg, Count, CountDistinct, Min, Max
    BinaryOp(BinaryExpr),
    Negate(UnaryExpr),
    Not(UnaryExpr),
    Case(CaseExpr),
    InList(InListExpr),
    Between(BetweenExpr),
    Like(LikeExpr),
    ILike(ILikeExpr),              // case-insensitive LIKE
    RegexpMatch(RegexpExpr),       // regex match (full_match flag for engine compat)
    RegexpExtract(RegexpExtractExpr), // REGEXP_EXTRACT(expr, pattern, group_idx)
    IsNull(UnaryExpr),
    IsNotNull(UnaryExpr),
    Coalesce(CoalesceExpr),
    NullIf(NullIfExpr),
    DateTrunc(DateTruncExpr),  // typed Grain enum, not String
    FunctionCall(FunctionCallExpr),
    Guard(GuardExpr),          // sugar; resolved to Case during planning
}

pub enum BinaryOp {
    Add, Subtract, Multiply, Divide, SafeDivide,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

pub enum Aggregation { Sum, Avg, Count, CountDistinct, Min, Max }
pub enum Literal { Integer(i64), Float(f64), String(String), Boolean(bool), Null }
```

**External deps:** `serde`, `thiserror`, `arrow-schema` (type compat only)

---

### 5.2 `semstrait-model`

**Role:** Deserialize YAML into typed Rust structs. Resolve `ref:` entries. No validation beyond structural correctness.

**Key types:**

```rust
pub struct SemanticModel {
    pub name:          String,
    pub description:   Option<String>,
    pub ai_context:    Option<AiContext>,
    pub labels:        Vec<String>,
    pub namespace:     Option<String>,  // catalog namespace for glob expansion (defaults to "default")
    pub datasets:      Vec<Dataset>,
    pub kinds:         Vec<Kind>,
    pub relationships: Vec<Relationship>,
    // Top-level reusable definitions (referenced via `ref: name`)
    pub dimensions:    Vec<Dimension>,
    pub measures:      Vec<Measure>,
    pub metrics:       Vec<Metric>,
}

pub struct Dataset {
    pub name:    String,
    pub keys:    Option<Keys>,
    pub dimensions:  Vec<DimensionEntry>,  // Inline | Ref(name)
    pub measures:    Vec<MeasureEntry>,
    pub metrics:     Vec<MetricEntry>,
    pub filters:     Vec<DatasetFilter>,
    pub extras:      Option<Extras>,
}

pub struct Kind {
    pub name:       String,
    pub kind_type:  KindTypeSpec,  // Grainset | Unionset | Joinset { associativity }
    pub keys:       Option<Keys>,
    pub dimensions: Vec<DimensionEntry>,
    pub measures:   Vec<MeasureEntry>,
    pub metrics:    Vec<MetricEntry>,
    pub datasets:   Vec<KindDatasetEntry>,     // Inline | Ref(kind_name)
    pub relationships: Vec<Relationship>,      // required for Joinset; same type as top-level
    pub extras:     Option<Extras>,            // kind-level defaults, propagated to datasets
}

/// Extras — three-level inheritance with field-by-field override:
///   dataset.extras > kind.dataset.extras > kind.extras
///
/// Separate Rust types (KindExtras, KindDatasetExtras, DatasetExtras) for
/// serde defaulting: column_mapping defaults to Inherited on KindDatasetExtras
/// but is Option on KindExtras.
pub struct KindExtras {                            // kind-level defaults
    pub column_mapping: Option<ColumnMapping>,
    pub temporal:       Option<TemporalConfig>,
    pub catalog:        Option<CatalogRef>,
    pub partition_defs: Option<Vec<PartitionDef>>,
}
pub struct KindDatasetExtras {                     // per-dataset in kind
    pub column_mapping: ColumnMapping,             // defaults to Inherited
    pub temporal:       Option<TemporalConfig>,
    pub storage:        Option<StorageConfig>,
    pub catalog:        Option<CatalogRef>,
}
pub struct DatasetExtras {                         // standalone dataset
    pub catalog:  Option<CatalogRef>,
    pub temporal: Option<TemporalConfig>,
    pub storage:  Option<StorageConfig>,
}

/// Relationship — structurally identical at all scopes.
/// Used both at top-level (model.relationships) and kind-level
/// (kind.relationships). The scope determines namespace:
///   - Top-level: joins between datasets and/or kinds
///   - Kind-level: joins between datasets within the same kind (joinset)
///
/// Implementation note: The code currently uses separate Rust types
/// (Relationship, KindRelationship) for documentation clarity, but
/// both compile to the same CompiledRelationship.
pub struct Relationship {
    pub name:        String,
    pub from:        String,
    pub to:          String,
    pub join_type:   JoinType,
    pub columns:     Vec<JoinColumnPair>,
    pub cardinality: Cardinality,
}

pub struct KindDataset {
    pub name:    DatasetName,                // Literal(String) or Glob(GlobPattern)
    pub extras:  KindDatasetExtras,          // column_mapping, temporal, storage, catalog
}

/// NOTE on glob patterns:
/// Glob patterns (wildcards `*`, `?`) apply ONLY to storage-level properties
/// (`StorageConfig.paths`, `StorageConfig.tables`) — never to semantic entity names.
/// Storage globs define the scope of read/scan operations and are resolved against
/// the filesystem or catalog during compilation.
///
/// Implementation note: The code retains a legacy `DatasetName::Glob` variant that
/// allows catalog-driven dataset discovery (e.g., `name: "orders_*"` expands via
/// catalog.list_tables into multiple concrete datasets). This is a compile-time
/// template mechanism — no glob survives into CompiledManifest. New models should
/// prefer explicit dataset names with storage-level globs for multi-source binding.

/// ColumnMapping value: four shapes for physical binding.
pub enum ColumnMappingValue {
    Simple(String),                                    // direct column: `cost: adwords_cost`
    WithGrain { column: String, grain: Option<Grain> },// grain override: `{ column: created_at, grain: day }`
    Literal(LiteralValue),                             // constant injection: `{ literal: "search" }`
    Anchored(HashMap<String, String>),                 // sub-name mapping for composed expressions
}

/// Expression source in YAML — serde-discriminated by value type.
/// String → inline DSL parser; Map → declarative ExprBlock (1:1 with Expr variants).
/// Used on dimensions (computed dims), measures, metrics.
#[serde(untagged)]
pub enum ExprSource {
    Inline(String),              // "cost / clicks", "UPPER(region)"
    Declarative(ExprBlock),      // { case: { when: [...] } }
}

/// Dimension type. Default = Categorical (most common, reduces YAML verbosity).
pub enum DimensionType {
    Temporal(TemporalDimension),
    Categorical(CategoricalDimension),  // default when type omitted
    Binary(BinaryDimension),
    Geo(GeoDimension),
    Bucketed(BucketedDimension),
    Metadata(MetadataDimension),        // extracts from source metadata
}

/// Metadata dimension: extracts values from source metadata (paths, partitions).
/// Not a physical column — excluded from column_mapping completeness checks.
pub struct MetadataDimension {
    pub path: Option<PathExtraction>,
    pub partition: Option<PartitionExtraction>,
}
pub struct PathExtraction { pub token: usize }       // 0-indexed path segment
pub struct PartitionExtraction { pub level: usize }  // 1-indexed Hive partition value

pub struct Dimension {
    pub name: String,
    pub data_type: DataType,
    pub dim_type: DimensionType,       // default: Categorical
    pub expr: Option<ExprSource>,      // computed dimension expression (Phase G)
}

/// Declarative aggregation type. YAML `agg:` tag on measures/metrics.
/// Separate from core::Aggregation for layer separation (model vs IR).
pub enum AggregationType { Sum, Avg, Count, CountDistinct, Min, Max }

pub struct Measure {
    pub name: String,
    pub data_type: DataType,
    pub agg: Option<AggregationType>,  // declarative aggregation
    pub expr: Option<ExprSource>,      // inline string or declarative block
    pub additivity: Option<Additivity>,
    pub constraints: Option<MeasureConstraints>,
    pub filters: Vec<MeasureFilter>,
}

/// StorageConfig — multi-path/table with glob support.
/// `paths` and `tables` are mutually exclusive. Globs expanded at compile time.
pub struct StorageConfig {
    pub format: Option<DataFormat>,       // Iceberg, Parquet, Csv (from semstrait-core)
    pub paths: Vec<String>,              // file/object store paths (may contain globs)
    pub tables: Vec<String>,             // catalog table refs (may contain wildcards)
    pub partition_def: Option<PartitionDef>,
}

/// Reference to a named catalog from catalogs.yaml.
/// Shorthand: `catalog: polaris_prod` or struct: `{ alias: polaris_prod, namespace: analytics }`.
pub struct CatalogRef {
    pub alias: String,
    pub namespace: Option<String>,
}

/// Temporal configuration with grain auto-propagation and dimension linking.
pub struct TemporalConfig {
    pub grain: Option<TemporalGrain>,          // data-level cadence (enables grain auto-propagation)
    pub dimension: Option<String>,             // links to semantic dimension name
    pub temporal_type: TemporalHistorization,  // Timeseries | Events | Snapshot | Scd
}

pub enum TemporalHistorization {
    Timeseries(TimeseriesConfig),   // periodic, semi-additive
    Events(EventsConfig),           // independent occurrences, fully additive
    Snapshot(SnapshotConfig),       // point-in-time snapshots
    Scd(ScdConfig),                 // slowly changing dimensions (types 1-6)
}
```

**Key functions:**

```rust
/// Parse a single YAML string into a SemanticModel.
pub fn parse(yaml: &str) -> Result<SemanticModel, ModelError>;

/// Replace all DimensionEntry::Ref / MeasureEntry::Ref / MetricEntry::Ref
/// with the actual definitions from the top-level arrays.
/// Returns Err if a ref target is not found.
pub fn resolve_refs(model: SemanticModel) -> Result<SemanticModel, ModelError>;
```

**External deps:** `semstrait-core`, `serde`, `serde_yaml`, `indexmap`

---

### 5.3 `semstrait-catalog`

**Role:** Abstract catalog metadata access. Used by `ManifestCompiler` for glob expansion and schema validation, and optionally by `SemanticPlanner` for schema freshness checks.

**Key trait:**

```rust
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    /// List all table names matching a glob pattern in a given namespace.
    /// Called by ManifestCompiler during glob expansion.
    async fn list_tables(
        &self,
        namespace: &str,
        pattern: &GlobPattern,
    ) -> Result<Vec<TableRef>, CatalogError>;

    /// Return column schema for a specific table.
    async fn get_schema(&self, table: &TableRef)
        -> Result<Vec<CatalogColumn>, CatalogError>;

    async fn table_exists(&self, table: &TableRef) -> Result<bool, CatalogError>;
}

pub struct TableRef {
    pub catalog:   Option<String>,
    pub namespace: String,
    pub name:      String,
}

pub struct CatalogColumn {
    pub name:      String,
    pub data_type: DataType,
    pub nullable:  bool,
    pub comment:   Option<String>,
}

/// No-op implementation. Returns empty lists and Ok(false).
/// Use when: testing, stateless operation with no glob patterns.
pub struct NullCatalogProvider;
```

**Implementations (feature-gated within this crate):**

| Feature flag | Impl | Backend | Protocol | Status |
|---|---|---|---|---|
| `iceberg` | `IcebergRestCatalog` | Polaris, Gravitino | Iceberg REST spec | Implemented (v1) |
| `unity` | `UnityCatalog` | Databricks Unity Catalog | UC REST API | Planned |
| `glue` | `GlueCatalog` | AWS Glue | AWS SDK | Planned |
| `hive` | `HiveCatalog` | Hive Metastore | Thrift / HTTP | Planned |
| *(always)* | `NullCatalogProvider` | None | — | Implemented (v1) |

**External deps:** `semstrait-core`, `async-trait`, `tokio`, `reqwest` (optional)

---

### 5.4 `semstrait-manifest`

**Role:** Compile `SemanticModel` → `CompiledManifest`. Own the `Repository` trait. Ship only `InMemoryRepository` in v1.

#### ManifestCompiler

```rust
pub struct ManifestCompiler {
    catalog: Option<Arc<dyn CatalogProvider>>,
    catalog_registry: Option<CatalogRegistry>,
    storage: Option<Arc<dyn StorageProvider>>,
}

impl ManifestCompiler {
    pub fn new() -> Self;
    pub fn with_catalog(self, c: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_catalog_registry(self, registry: CatalogRegistry) -> Self;
    pub fn with_storage(self, s: Arc<dyn StorageProvider>) -> Self;

    pub async fn compile(
        &self,
        source: CompileSource,
    ) -> Result<CompiledManifest, CompileError>;
}

pub enum CompileSource {
    Yaml(String),
    YamlFiles(Vec<PathBuf>),
    Directory(PathBuf),
}
```

**Compilation pipeline (strict order):**

```
Step 1    parse                      serde_yaml → SemanticModel
Step 2    resolve_refs               expand ref: entries
Step 3    resolve_sources            expand globs/wildcards via StorageProvider/CatalogProvider,
                                     fetch catalog metadata, build CatalogSnapshot
                                     (async, requires providers for wildcards)
Step 4    validate_structure         dataset uniqueness, kind nesting
Step 4.5  expand_auto_mappings       ColumnMapping::Auto → identity, temporal/catalog
                                     propagation (kind→dataset), grain auto-propagation
                                     (temporal.grain → column_mapping when same physical col)
Step 4.6  validate_temporal_equiv    kind/dataset temporal type match
Step 4.7  validate_storage           paths/tables exclusivity, format requirements
Step 4.8  validate_metadata_dims     path/partition preconditions
Step 4.55 validate_temporal_dim_consistency  datasets within a kind must agree on
                                     temporal.dimension value (TemporalDimensionConflict)
Step 4.9  derive_dimension_grains    auto-derive empty temporal dimension grains from
                                     dataset temporal configs (emits COMP_I001 diagnostic)
Step 5    validate_mappings          column_mapping keys vs kind interface (union coverage)
                                     (excludes metadata dims and computed dims from checks)
Step 5.5  validate_grain_compat      grain compatibility across datasets
Step 6    build_metric_graph         cycle detection, depth ≤ 3
Step 7    build_rel_graph            relationship graph, joinset anchor inference
Step 8    compile_exprs              parse Expr DSL, reject raw SQL,
                                     compile computed dim expressions (no agg allowed),
                                     FunctionRegistry arity validation (28 ANSI functions)
Step 9    emit                       serialize to CompiledManifest (consumes SourceResolutionResult)
```

**Glob expansion detail (step 3):**

Glob patterns belong at the **storage level** (`StorageConfig.paths`, `StorageConfig.tables`),
defining the scope of read/scan operations resolved against filesystem or catalog.

Legacy support: The code also supports a `DatasetName::Glob` variant where
`name: "orders_*"` in a kind's datasets expands via catalog into multiple
concrete dataset entries. This is a compile-time template mechanism — the
preferred approach for new models is explicit dataset names with storage-level
globs for multi-source binding.

- If `catalog = None` and any glob pattern is found → `CompileError::GlobRequiresCatalog`
- The `CompiledManifest` contains only concrete, expanded datasets. No glob survives.

#### Repository

```rust
/// Storage abstraction for CompiledManifest.
/// Ships InMemoryRepository only. FileSystem and ObjectStore planned.
pub trait Repository: Send + Sync {
    fn load(&self) -> Result<Arc<CompiledManifest>, RepositoryError>;
    fn save(&self, manifest: &CompiledManifest) -> Result<(), RepositoryError>;
}

pub struct InMemoryRepository(Arc<RwLock<Option<Arc<CompiledManifest>>>>);
```

#### CompiledManifest

```rust
/// CompiledManifest v3 — single unified `data_kinds` map (Phase J consolidation).
/// All entity types (datasets, grainsets, unionsets, joinsets) live in one map.
/// Single-dataset kinds compile as CompiledDataKind::Dataset (fast path).
#[derive(Serialize, Deserialize)]
pub struct CompiledManifest {
    pub version:          u32,                                   // 3
    pub compiled_at:      DateTime<Utc>,
    pub source_hash:      String,                                // SHA-256 of YAML input
    pub data_kinds:       IndexMap<String, CompiledDataKind>,    // unified entity map
    pub relationships:    Vec<CompiledRelationship>,
    pub diagnostics:      Vec<Diagnostic>,
    pub catalog_snapshot: Option<CatalogSnapshot>,               // schema drift data
}

/// Compiled data kind — 4 variants, each with CompiledInterface + DatasetBinding(s).
/// Single-dataset kinds (declared as grainset/unionset/joinset with 1 dataset)
/// are compiled as Dataset (no routing overhead needed).
pub enum CompiledDataKind {
    Dataset(Box<CompiledDatasetKind>),
    Grainset(Box<CompiledGrainsetKind>),
    Unionset(Box<CompiledUnionsetKind>),
    Joinset(Box<CompiledJoinsetKind>),
}

impl CompiledDataKind {
    pub fn interface(&self) -> &CompiledInterface;
    pub fn bindings(&self) -> &[DatasetBinding];
}

/// Shared interface across all data kind variants.
pub struct CompiledInterface {
    pub dimensions:    IndexMap<String, CompiledDimension>,
    pub measures:      IndexMap<String, CompiledMeasure>,
    pub metrics:       IndexMap<String, CompiledMetric>,
    pub keys:          Option<CompiledKeys>,
    pub filters:       Vec<CompiledFilter>,
    pub temporal_dim:  Option<String>,
}

/// Per-dataset physical binding within a compiled data kind.
pub struct DatasetBinding {
    pub name:              String,
    pub column_mapping:    ResolvedColumnMapping,   // physical, literals, temporal, anchored
    pub resolved_sources:  Vec<ResolvedSource>,
}

pub struct CompiledDimension {
    pub name:        String,
    pub data_type:   DataType,
    pub dim_type:    DimensionType,
    pub expr:        Option<Expr>,                 // computed dim expression (Phase G); None = physical
    pub expr_source: Option<String>,               // original YAML string for debugging
}

pub struct CompiledMeasure {
    pub name:        String,
    pub data_type:   DataType,
    pub agg:         Aggregation,                  // always present (auto-upgraded from legacy)
    pub expr:        Expr,                         // horizontal-only (no aggregation functions)
    pub expr_source: String,                       // original expression for debugging
    pub additivity:  Option<AdditivityType>,       // derived from agg when not specified
    pub constraints: Option<MeasureConstraints>,   // field = `constraints:` in YAML
    pub filters:     Vec<CompiledFilter>,
}

pub enum MetricType { Simple, Ratio, Derived }     // compiler-inferred from metric expr

pub struct CompiledMetric {
    pub name:        String,
    pub data_type:   DataType,
    pub metric_type: MetricType,                   // inferred: Simple | Ratio | Derived
    pub agg:         Option<Aggregation>,           // two-stage (optional)
    pub expr:        Expr,
    pub additivity:  Option<AdditivityType>,       // worst-case of leaf measures
    pub depth:       usize,                        // metric graph depth
}
```

**External deps:** `semstrait-core`, `semstrait-model`, `semstrait-catalog`, `petgraph`, `serde_json`, `sha2`, `tokio`

---

### 5.5 `semstrait-ir`

**Role:** Define the `LogicalPlan` IR in-memory. Provide bidirectional Substrait serialization. Carry semantic annotations per node.

**Design principle:** `PlanNode` ordinals == Substrait `structField` ordinals. Schema is always attached to each node via `NodeMeta`. Parent nodes never guess field positions — they use `schema.ordinal(name)` which is O(1) via HashMap index.

#### PlanNode Enum

```rust
pub enum PlanNode {
    Scan(ScanNode),
    Filter(FilterNode),
    Project(ProjectNode),
    Aggregate(AggNode),
    Join(JoinNode),
    Union(UnionNode),
    Sort(SortNode),
    Fetch(FetchNode),
}

/// Every PlanNode carries this metadata.
pub struct NodeMeta {
    pub node_id:       Uuid,
    pub output_schema: Schema,           // stable ordinals after construction
    pub annotations:   Vec<SemAnnotation>,
}

/// Semantic annotations serialized into AdvancedExtension.detail
/// URN: "urn:semstrait:annotations:v1"
/// Format: prost-encoded SemstraitAnnotation proto (defined in ir/proto/semstrait.proto)
pub enum SemAnnotation {
    AggregateRole(AggregateRole),
    FilterSource(FilterSource),
    Additivity(AdditivityAnnotation),
    KindRef(String),
}

pub enum AggregateRole { Final, SemiAdditiveInner, HorizontalSubResult, FanoutDedup }
pub enum FilterSource {
    DatasetFilter | MeasureFilter | MetricFilter | UserFilter
    | ScdCurrentRow | SnapshotValidity | RowLevelSecurity
}
```

#### PlanNode → Substrait Correspondence

| `PlanNode` variant | `substrait::proto::Rel` | Notes |
|---|---|---|
| `ScanNode` | `ReadRel` | `named_table` for path; `projection` for column selection |
| `FilterNode` | `FilterRel` | `condition` = `ExprConverter::to_substrait(expr)` |
| `ProjectNode` | `ProjectRel` | one `Expression` per output column |
| `AggNode` | `AggregateRel` | `groupings` + `measures` with function URI |
| `JoinNode` | `JoinRel` | `type` maps join kind; `expression` = join condition |
| `UnionNode` | `SetRel` | `set_op = UNION_ALL` |
| `SortNode` | `SortRel` | `sorts` with field refs + direction |
| `FetchNode` | `FetchRel` | `count` for LIMIT; `offset` for OFFSET |
| `SemAnnotation` | `AdvancedExtension.detail` | prost-encoded `SemstraitAnnotation` proto |

#### ExprConverter — Expr ↔ Substrait Expression

```rust
/// Converts unified Expr to/from substrait::proto::Expression.
/// Schema is required for Column → FieldReference ordinal lookup.
pub struct ExprConverter<'s> {
    schema: &'s Schema,
}

impl<'s> ExprConverter<'s> {
    pub fn to_substrait(&self, expr: &Expr)
        -> Result<substrait::proto::Expression, ConvertError>;
    pub fn from_substrait(&self, expr: &substrait::proto::Expression)
        -> Result<Expr, ConvertError>;
}
```

**Expr ↔ Substrait Expression mappings (full round-trip tested):**

| `Expr` | `Expression` |
|---|---|
| `Column(ColumnRef { name, .. })` | `FieldReference { struct_field: { field: schema.ordinal(name) } }` |
| `Literal(Integer(i64))` | `Literal { literal_type: I64 }` |
| `Literal(Float(f64))` | `Literal { literal_type: Fp64 }` |
| `Literal(String/Boolean/Null)` | `Literal { literal_type: ... }` |
| `BinaryOp(BinaryExpr { l, Eq, r })` | `ScalarFunction { fn_anchor: 100, args: [l, r] }` |
| `Aggregate(AggregateExpr { Sum, expr })` | mapped to function call (aggregate handling in serializer) |
| `Case(CaseExpr { when_then, else_expr })` | `IfThen { ifs: [..], else_: .. }` |
| `Not(UnaryExpr)` | `ScalarFunction { fn_anchor: 205 }` |
| `IsNull(UnaryExpr)` | `ScalarFunction { fn_anchor: 202 }` |
| `IsNotNull(UnaryExpr)` | `ScalarFunction { fn_anchor: 203 }` |
| `InList(InListExpr)` | `ScalarFunction { fn_anchor: 206, args: [expr, list..] }` |
| `Between(BetweenExpr)` | `ScalarFunction { fn_anchor: 207, args: [expr, low, high] }` |
| `Like(LikeExpr)` | `ScalarFunction { fn_anchor: 208 }` |
| `ILike(ILikeExpr)` | `ScalarFunction { fn_anchor: 211 }` |
| `RegexpMatch(RegexpExpr)` | `ScalarFunction { fn_anchor: 212 }` |
| `RegexpExtract(RegexpExtractExpr)` | `ScalarFunction { fn_anchor: 213 }` |
| `Coalesce(CoalesceExpr)` | `ScalarFunction { fn_anchor: 204 }` |
| `NullIf(NullIfExpr)` | `ScalarFunction { fn_anchor: 209 }` |
| `DateTrunc(DateTruncExpr { grain, expr })` | `ScalarFunction { fn_anchor: 210, args: [grain_lit, expr] }` |
| Aggregation (Sum/Avg/...) | Used inside `AggregateRel.measures[i].function` |

#### SubstraitSerializer

```rust
pub struct SubstraitSerializer;
impl SubstraitSerializer {
    /// Serialize a LogicalPlan to substrait::proto::Plan.
    /// SemAnnotations are encoded in AdvancedExtension.detail per node.
    pub fn to_substrait(plan: &LogicalPlan)
        -> Result<substrait::proto::Plan, SerializeError>;

    /// Deserialize a substrait::proto::Plan back to LogicalPlan.
    /// SemAnnotations are decoded from AdvancedExtension.detail if present.
    pub fn from_substrait(plan: &substrait::proto::Plan)
        -> Result<LogicalPlan, DeserializeError>;
}
```

**Substrait extension spec:**
- URN: `urn:semstrait:annotations:v1`
- Proto: `SemstraitAnnotation` message defined in `semstrait-ir/proto/semstrait.proto`
- All extension blobs reside in `ExtensionLeafRel.common.advanced_extension.detail`
- Substrait consumers that don't understand the extension silently ignore it — the core plan remains valid

#### PlanBuilder — Engine-Specific Node Construction

```rust
/// Strategy trait for engine-specific plan node construction.
/// Lives in semstrait-ir to break planner ↔ adapter coupling.
/// Engines override only the node builders that differ from defaults.
pub trait PlanBuilder: Send + Sync {
    fn finalize_node(&self, node: PlanNode) -> PlanNode { node }
    fn build_scan(&self, schema: Schema, table_name: String, location: Option<String>,
        format: Option<DataFormat>, projection: Vec<String>) -> PlanNode { /* default ScanNode */ }
    fn build_filter(&self, schema: Schema, input: PlanNode, predicate: Expr) -> PlanNode { /* default FilterNode */ }
    fn build_project(&self, schema: Schema, input: PlanNode, expressions: Vec<Expr>) -> PlanNode { /* default ProjectNode */ }
    fn build_aggregate(&self, schema: Schema, input: PlanNode, group_by: Vec<Expr>,
        aggregates: Vec<AggregateMeasure>) -> PlanNode { /* default AggNode */ }
    fn build_join(&self, schema: Schema, left: PlanNode, right: PlanNode,
        join_type: JoinType, condition: Expr) -> PlanNode { /* default JoinNode */ }
    fn build_union(&self, schema: Schema, inputs: Vec<PlanNode>, distinct: bool) -> PlanNode { /* default UnionNode */ }
    fn build_sort(&self, schema: Schema, input: PlanNode, sort_keys: Vec<SortKey>) -> PlanNode { /* default SortNode */ }
    fn build_fetch(&self, schema: Schema, input: PlanNode, count: Option<i64>, offset: i64) -> PlanNode { /* default FetchNode */ }
}

/// Zero-state default implementation. All methods use standard PlanNode constructors.
#[derive(Debug, Clone, Copy)]
pub struct DefaultPlanBuilder;
impl PlanBuilder for DefaultPlanBuilder {}
```

**Design:** Every build method has a default implementation that constructs the standard PlanNode variant. `finalize_node()` is a cross-cutting hook called after every node construction — engines can use it for metadata injection, custom annotations, etc. The planner uses `let builder = adapter.plan_builder().unwrap_or(DefaultPlanBuilder)`.

**External deps:** `semstrait-core`, `substrait` (prost-generated), `uuid`, `prost`

---

### 5.6 `semstrait-planner`

**Role:** Build `LogicalPlan` from `QueryRequest` + `CompiledManifest`. Dispatch to kind-specific planners. Evaluate constraints, additivity, filters. Apply optimizer internally.

#### SemanticPlanner

```rust
pub struct SemanticPlanner {
    catalog:      Option<Arc<dyn CatalogProvider>>,
    optimizer:    Optimizer,       // empty in v1; configured at construction
    planners:     KindPlannerRegistry,
    plan_builder: Box<dyn PlanBuilder>, // engine-specific node construction (from IR)
}

pub struct SemanticPlannerBuilder {
    catalog:      Option<Arc<dyn CatalogProvider>>,
    passes:       Vec<Box<dyn OptimizerPass>>,
    plan_builder: Box<dyn PlanBuilder>,  // defaults to DefaultPlanBuilder
}
impl SemanticPlannerBuilder {
    pub fn new() -> Self;
    pub fn with_catalog(self, c: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_optimizer_pass(self, p: impl OptimizerPass + 'static) -> Self;
    pub fn with_plan_builder(self, builder: Box<dyn PlanBuilder>) -> Self;
    pub fn build(self) -> SemanticPlanner;
}

impl SemanticPlanner {
    pub fn builder() -> SemanticPlannerBuilder;

    /// Synchronous — no async work needed for plan generation.
    pub fn plan(
        &self,
        request: &ResolvedQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<LogicalPlan, PlannerError>;
}
```

**`plan()` internal steps:**
```
1. ConstraintValidator::check(request, manifest)    → Err if violated
2. KindPlannerRegistry::dispatch(kind_type)         → KindPlanner
3. KindPlanner::resolve(kind, request, ctx)         → PlanFragment
   - dimensions partitioned: metadata → computed → physical (expr/mod.rs)
   - expression resolution via ExprResolver trait (resolver.rs)
   - measure decomposition via decompose_measure() (decomposer.rs)
   - computed dims emitted as ProjectNode expressions (post-aggregation)
5. AdditivityResolver::resolve(fragment, measure, ..)
6. inject dataset filter (v1: skipped — handled inside KindPlanner)
7. inject measure filter         (CASE WHEN in AggNode — DL-008)
8. inject metric filter          (handled inside KindPlanner in v1)
9. inject user filter            (FilterNode, source = UserFilter)
10. ORDER BY + LIMIT/FETCH       (SortNode + FetchNode)
11. Optimizer::apply(plan)       → LogicalPlan  (identity in v1)
```
**Note:** In v1, steps 6 and 8 (dataset/metric filters) are applied inside the
KindPlanner's resolve() method rather than as separate post-processing steps.

#### ConstraintValidator

```rust
pub struct ConstraintValidator;
impl ConstraintValidator {
    /// Checks all measure and metric constraints in the request scope.
    /// "query scope" = request.group_by ∪ request.filter_dimensions
    /// Constraint blocks:
    ///   dimensions.one_of  → at least one must be in scope
    ///   dimensions.none_of → none may be in scope
    ///   dimensions.all     → all must be in scope
    ///   aggregations.allowed    → only these functions may be used
    ///   aggregations.prohibited → these functions must not be used
    pub fn check(
        request: &ResolvedQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<(), PlannerError>;
}
```

#### ExprResolver — Expression Resolution (Phase H)

```rust
/// Trait-based polymorphism for resolving semantic expressions to physical.
/// Two implementations cover all planner use cases.
pub trait ExprResolver {
    fn resolve_column(&self, name: &str) -> Expr;
    fn resolve_expr(&self, expr: &Expr) -> Result<Expr, PlannerError>;
    // Default impl: walks Expr tree, resolves Column/EntityRef via resolve_column(),
    // expands Guard → Case, passes through Literal/BinaryOp/Case/etc.
}

/// Resolves via IndexMap<String, String> (physical column name mapping).
/// Used by kind planners for dataset-level column resolution.
pub struct PhysicalResolver<'a> { physical: &'a IndexMap<String, String> }

/// Resolves via HashMap<String, ColumnMappingValue> (supports grain overrides).
/// Used by planner.rs for filter injection and ad-hoc join resolution.
pub struct MappingResolver<'a> { mapping: &'a HashMap<String, ColumnMappingValue> }
```

#### Decomposer — Measure/Metric Decomposition (Phase H)

```rust
/// Result of decomposing a measure: extracted aggregates + post-agg projection.
pub struct DecomposedMeasure {
    pub aggregates: Vec<AggregateMeasure>,  // for AggNode
    pub post_agg_expr: Expr,                // for ProjectNode
}

/// Declarative path: agg tag + horizontal expression.
pub fn decompose_measure(resolver: &dyn ExprResolver, name: &str, agg: Aggregation,
    expr: &Expr, filters: &[CompiledFilter]) -> Result<DecomposedMeasure, PlannerError>;

/// Recursive metric decomposition via KindInterface + DatasetBinding.
pub fn decompose_metric(name: &str, metric: &CompiledMetric, iface: &KindInterface,
    binding: &DatasetBinding, max_depth: usize) -> Result<DecomposedMeasure, PlannerError>;
```

#### KindPlanner — Strategy Pattern

```rust
pub trait KindPlanner: Send + Sync {
    fn supports(&self, data_kind: &DataKind) -> bool;
    fn resolve(
        &self,
        data_kind: &DataKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext,
    ) -> Result<PlanFragment, PlannerError>;
}

pub struct PlannerContext<'a> {
    pub manifest:     &'a CompiledManifest,
    pub plan_builder: &'a dyn PlanBuilder,  // engine-specific node construction (from IR)
    pub catalog:      Option<&'a dyn CatalogProvider>,
    pub session:      &'a SessionVariables,   // runtime values (tenant_id, etc.)
}

pub struct PlanFragment {
    pub root:            PlanNode,
    pub output_schema:   Schema,
    pub pending_filters: Vec<Expr>,     // filters not yet injected (unified Expr)
}
```

**KindPlanner implementations:**

| Impl | DataKind | Resolution |
|---|---|---|
| `DatasetPlanner` | `Dataset` | Single-dataset fast path: Scan → Agg → Project |
| `GrainsetPlanner` | `Grainset` | Route to cheapest covering dataset; FULL OUTER join if no single covers all |
| `UnionsetPlanner` | `Unionset` | Build UNION ALL branches; NULL-fill unmapped columns; prune NULL-excluding branches |
| `JoinsetPlanner` | `Joinset` | BFS from anchor dataset; join pruning; temporal filter injection per dataset |

#### AdditivityResolver

```rust
pub struct AdditivityResolver;
impl AdditivityResolver {
    /// Restructures the PlanFragment for semi/non additivity.
    /// V1 stub — logs warnings for semi/non-additive measures but does not
    /// restructure the plan. Full implementation deferred.
    pub fn resolve(
        fragment: PlanFragment,
        measure: &CompiledMeasure,
        request: &ResolvedQueryRequest,
    ) -> Result<PlanFragment, PlannerError>;
}
```

#### Optimizer (internal, not public API)

```rust
pub trait OptimizerPass: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError>;
    fn is_applicable(&self, plan: &LogicalPlan) -> bool { true }
}

pub struct Optimizer { passes: Vec<Box<dyn OptimizerPass>> }
impl Optimizer {
    pub fn empty() -> Self { Self { passes: vec![] } }
    pub fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError> {
        self.passes.iter().fold(Ok(plan), |acc, p| {
            let plan = acc?;
            if p.is_applicable(&plan) { p.apply(plan) } else { Ok(plan) }
        })
    }
}
```

**Planned passes:**

| Pass | Description |
|---|---|
| `PredicatePushdown` | Move `FilterNode` toward `ScanNode`; push through INNER/LEFT; blocked by FULL OUTER |
| `ProjectionPruning` | Trim `ScanNode.column_mapping` via top-down needed-set propagation |
| `NullBranchElimination` | Prune Union branches where predicate excludes NULLs on unmapped column |
| `ConstantFolding` | Evaluate constant sub-expressions at plan time |

#### ResolvedQueryRequest

Lives in `semstrait-planner` (it requires both manifest types and is only used by the planner).

```rust
pub struct ResolvedQueryRequest {
    pub kind_name:         String,
    pub dimensions:        Vec<String>,       // semantic dimension names
    pub measures:          Vec<String>,       // semantic measure/metric names
    pub filters:           Vec<QueryFilter>,  // user-supplied predicates
    pub grain:             Option<Grain>,     // temporal group-by grain
    pub limit:             Option<u64>,
    pub order_by:          Vec<OrderByClause>,
    pub session_variables: SessionVariables,  // runtime values (e.g. tenant_id)
}
```

**External deps:** `semstrait-core`, `semstrait-ir`, `semstrait-manifest`, `semstrait-catalog`

---

### 5.7 SQL Emission (within `semstrait-adapter`)

**Role:** Emit SQL strings from `LogicalPlan`. Pure, synchronous, no I/O. One dialect per `SqlEmitter` implementation. Formerly a separate `semstrait-sql` crate — merged into `semstrait-adapter` (Phase L).

**Design:** SQL is generated by walking the `PlanNode` tree and emitting SQL fragments via direct string building through the dialect. The base emitter (`AnsiSqlEmitter`) uses no Jinja templates and no sqlparser-rs AST intermediate — it produces SQL strings directly from `PlanNode` + `ExprSqlRenderer`. Both emitters use the unified `Expr` type from `semstrait-core::expr`. The optional `PolyglotEmitter` uses polyglot-sql's internal AST for dialect transpilation.

**Note:** Per DL-025/DL-030, `AnsiSqlEmitter`, `ExprSqlRenderer`, and per-dialect `SqlDialect` impls are deprecated in favor of `PolyglotEmitter`. They remain as fallback when the `polyglot` feature is disabled.

```rust
pub trait SqlEmitter: Send + Sync {
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError>;
    fn dialect(&self) -> &dyn SqlDialect;
}

pub trait SqlDialect: Send + Sync {
    fn quote_identifier(&self, ident: &str) -> String;
    fn supports_cte(&self) -> bool;
    fn date_trunc(&self, grain: &Grain, expr: &str) -> String;
    fn null_safe_eq(&self, l: &str, r: &str) -> String;
    fn current_timestamp(&self) -> String;
    fn window_row_number(&self, partition_by: &[&str], order_by: &str) -> String;
    /// Generate a LIMIT/FETCH clause. ANSI → FETCH FIRST N ROWS ONLY; DuckDB → LIMIT N.
    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String;
}

// Expr → dialect SQL string
pub struct ExprSqlRenderer<'d> { dialect: &'d dyn SqlDialect }
impl<'d> ExprSqlRenderer<'d> {
    pub fn render(&self, expr: &Expr) -> Result<String, EmitError>;
    pub fn render_aggregate(&self, measure: &AggregateMeasure) -> Result<String, EmitError>;
}

// Single parameterized emitter — dialect is a type parameter
pub struct AnsiSqlEmitter<D: SqlDialect> { dialect: D }
// Dialect structs (all use ANSI double-quoted identifiers):
pub struct AnsiDialect;   // FETCH FIRST, standard DATE_TRUNC
pub struct DuckDbDialect;  // LIMIT, lowercase date_trunc
pub struct TrinoDialect;   // FETCH FIRST, lowercase date_trunc
```

**Expr → SQL lowering table:**

| `Expr` | ANSI SQL |
|---|---|
| `Column(ColumnRef { name, .. })` | `"name"` (quoted per dialect) |
| `Literal(Integer/Float/String/Boolean/Null)` | literal value (i64 preserves precision) |
| `BinaryOp(BinaryExpr { l, Add, r })` | `(l + r)` |
| `BinaryOp(BinaryExpr { l, SafeDivide, r })` | `CASE WHEN r = 0 THEN NULL ELSE l / r END` |
| `Aggregate(AggregateExpr { Sum, expr })` | `SUM(expr)` — typed dispatch, no string matching |
| `Aggregate(AggregateExpr { CountDistinct, .. })` | `COUNT(DISTINCT expr)` |
| `FunctionCall(FunctionCallExpr { name, args })` | `name(args)` — escape hatch for non-standard functions |
| `Case(CaseExpr { when_then, else_expr })` | `CASE WHEN w THEN t ... ELSE e END` |
| `Not(UnaryExpr)` | `NOT (inner)` |
| `IsNull(UnaryExpr)` | `inner IS NULL` |
| `IsNotNull(UnaryExpr)` | `inner IS NOT NULL` |
| `InList(InListExpr)` | `expr IN (list)` |
| `Between(BetweenExpr)` | `expr BETWEEN low AND high` |
| `Like(LikeExpr)` | `expr LIKE pattern` |
| `ILike(ILikeExpr)` | `expr ILIKE pattern` (case-insensitive) |
| `RegexpMatch(RegexpExpr)` | dialect-specific: `REGEXP_LIKE` (ANSI), `~` (DuckDB), `REGEXP` (Spark) |
| `RegexpExtract(RegexpExtractExpr)` | `REGEXP_EXTRACT(expr, pattern, group)` |
| `Coalesce(CoalesceExpr)` | `COALESCE(exprs)` |
| `NullIf(NullIfExpr)` | `NULLIF(expr, null_expr)` |
| `DateTrunc(DateTruncExpr { grain, expr })` | `DATE_TRUNC('grain', expr)` (typed Grain enum, dialect-specific) |

**Note:** `Guard` and `EntityRef` are resolved during planning (to `Case` and `Column` respectively) and never appear in SQL emission. Both emitters return `EmitError::UnsupportedExpr` if encountered. Aggregates use typed `Aggregation` enum dispatch — no string-based function name matching.

**Polyglot SQL transpilation (feature-gated):**

When the `polyglot` feature is enabled, `PolyglotEmitter` generates dialect-specific SQL by:
1. Generating ANSI SQL via `AnsiSqlEmitter` (existing, well-tested pipeline)
2. Transpiling to the target dialect via `polyglot_sql::transpile()`

```rust
// Always available — connectors declare their preferred dialect
pub enum TargetDialect { Ansi, DataFusion, DuckDb, Trino, Spark, Snowflake, Databricks, PostgreSql }

// Feature-gated behind "polyglot"
pub struct PolyglotEmitter { base: AnsiSqlEmitter<AnsiDialect>, target: TargetDialect }
impl SqlEmitter for PolyglotEmitter { /* transpile(ansi_sql, DuckDB, target) */ }
```

Transpilation handles: identifier quoting (double-quotes → backticks for Spark/Databricks), FETCH FIRST → LIMIT conversion, and function name normalization across 34+ dialects.

**Location:** `semstrait-adapter/src/sql/` — `AnsiSqlEmitter`, `ExprSqlRenderer`, `SqlDialect`, `PolyglotEmitter` all live within the adapter crate.

**Feature gating:** `polyglot-sql` is an optional dependency of `semstrait-adapter`. DuckDB and Spark features activate it (`duckdb = ["dep:polyglot-sql"]`, `spark = ["dep:polyglot-sql"]`). DataFusion does not need polyglot (Substrait-only). ANSI SQL emission is always available.

---

### 5.8 `semstrait-adapter`

**Role:** Produce engine-appropriate artifacts (SQL or Substrait) from `LogicalPlan`. Contains SQL emission (SqlEmitter, dialects) and engine-specific adapters. This is the core value layer of semstrait's engine integration.

**Structure within crate:**
```
semstrait-adapter/
├── src/
│   ├── lib.rs              AdaptError, re-exports
│   ├── traits.rs           EngineAdapter trait
│   ├── sql/
│   │   ├── mod.rs          SqlEmitter, SqlDialect, AnsiSqlEmitter, ExprSqlRenderer
│   │   ├── dialect.rs      AnsiDialect, DuckDbDialect (feature), SparkDialect (feature)
│   │   ├── expr_renderer.rs Expr → SQL string rendering
│   │   ├── tests.rs        SQL emission tests (dialect-gated)
│   │   └── polyglot/       PolyglotEmitter (feature-gated: duckdb | spark)
│   │       └── expr_builder.rs
│   └── engines/
│       ├── mod.rs           engine module re-exports
│       ├── ansi.rs          AnsiAdapter (always available)
│       ├── datafusion/      #[cfg(feature = "datafusion")] DataFusionAdapter
│       ├── duckdb/          #[cfg(feature = "duckdb")]     DuckDbAdapter
│       └── spark/           #[cfg(feature = "spark")]      SparkAdapter
```

#### Core trait

```rust
/// Produces an engine-appropriate artifact from a LogicalPlan.
/// Simplified trait — no EngineProfile dependency (Phase L).
pub trait EngineAdapter: Send + Sync {
    /// Human-readable engine name (e.g., "datafusion", "duckdb", "spark").
    fn name(&self) -> &str;

    /// Return an engine-specific plan builder, or `None` for default behavior.
    /// When provided, the planner uses this builder to construct plan nodes,
    /// allowing engines to override specific node construction.
    fn plan_builder(&self) -> Option<Box<dyn PlanBuilder>> { None }

    /// Convert a LogicalPlan into an engine-appropriate artifact.
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError>;

    /// Generate ANSI-compatible debug SQL for any plan (always available).
    /// Default impl uses AnsiSqlEmitter.
    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError> { /* ANSI default */ }
}
```

**Key design decisions (Phase L):**
- `EngineProfile` and `ConsumerProfile` removed entirely — they were dead abstractions (v1 stub, zero reads in planner).
- `EngineAdapter` is now standalone (`Send + Sync`), not dependent on any profile trait.
- Engine-specific plan node construction is injected via `PlanBuilder` trait (defined in IR crate).
- The facade extracts `adapter.plan_builder()` and passes it to the planner builder.

#### Engine adapters

| Adapter | Output | Notes |
|---|---|---|
| `DataFusionAdapter` | `Substrait` | Serializes via `SubstraitSerializer`. No polyglot needed. |
| `DuckDbAdapter` | `Sql` | DuckDB dialect (LIMIT, lowercase). Uses polyglot-sql for transpilation. |
| `SparkAdapter` | `Sql` | Spark dialect. Uses polyglot-sql for transpilation. |

**External deps:** `semstrait-core`, `semstrait-ir`, `polyglot-sql` (optional, activated by `duckdb`/`spark` features)

---

### 5.9 `semstrait-connectors`

**Role:** Optional execution layer. Receives `PlanArtifact` and executes it against a compute engine. Fully independent from `semstrait-adapter` — pure artifact consumption (DL-056). Convenience for testing and CLI — not the core value.

**Structure within crate:**
```
semstrait-connectors/
├── src/
│   ├── lib.rs           re-exports
│   ├── traits.rs        ComputeConnector trait
│   ├── result.rs        ComputeResult, ComputeResultData, ExecutionStats
│   ├── duckdb.rs        #[cfg(feature = "duckdb")]
│   ├── datafusion.rs    #[cfg(feature = "datafusion")]
│   ├── trino.rs         #[cfg(feature = "trino")]
│   └── spark.rs         #[cfg(feature = "spark")]
```

#### Core trait (decoupled — no adapter dependency)

```rust
/// Optional compute execution. Receives a PlanArtifact and runs it.
/// No knowledge of EngineAdapter — pure execution boundary (DL-056).
#[async_trait]
pub trait ComputeConnector: Send + Sync {
    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError>;
    async fn health_check(&self) -> Result<(), ConnectorError>;
    fn name(&self) -> &str;
}

pub struct ComputeResult {
    pub complete:    bool,
    pub stats:       ExecutionStats,
    pub data:        ComputeResultData,
}

pub enum ComputeResultData {
    Empty,
    Json(Vec<serde_json::Value>),
    Native(Box<dyn Any + Send + Sync>),
}
```

**Key design decision (DL-056):** `ComputeConnector` has no `fn adapter()` method. Adapter and connector are independent: adapter transforms IR → artifact, connector executes artifact → result. They share no references. The facade orchestrates both (see §5.11).

#### Adapter + Connector Architecture

**Diagram 6 — Adapter/Connector Pipeline:**
```
LogicalPlan (from SemanticPlanner)
        │
        ▼
EngineAdapter.adapt(plan)
        │
  ┌─────┴─────────────────────────┐
  ▼                               ▼
Sql(String)              Substrait(proto::Plan)
  │                               │
  └───────────┬───────────────────┘
              ▼
     PlanArtifact ← primary output (semstrait boundary)
              │
              ▼ (optional — ComputeConnector execution)
  ┌───────────┼───────────────────┬──────────────┐
  ▼           ▼                   ▼              ▼
DuckDB    DataFusion            Trino          Spark
SQL exec  Substrait→DF plan    SQL REST    structural (deferred)
  │           │                   │              │
  └───────────┴───────────────────┴──────────────┘
              ▼
     ComputeResult (JSON rows or Arrow)
```

**Per-engine notes:**

| Engine | Adapter Output | Connector Wire | Notes |
|---|---|---|---|
| DataFusion | `Substrait` | In-process via `datafusion-substrait` | Deserializes `proto::Plan` → DF LogicalPlan → execute; SQL fallback available |
| DuckDB | `Sql` (DuckDB dialect) | DuckDB C API (embedded, `bundled`) | `Connection` is `Send`/`!Sync` → `Arc<Mutex<Connection>>` + `spawn_blocking` |
| Trino | `Sql` (Trino dialect) | reqwest REST v1/statement | POST→poll nextUri→collect pages. Basic/JWT auth. |
| Spark | `Sql` (Spark dialect) | structural (deferred) | Builder pattern, `execute()` returns `NotImplemented`. |

**External deps:** `semstrait-core`, `semstrait-ir`, `arrow`, `async-trait`

**Note:** `semstrait-adapter` is NOT a dependency of `semstrait-connectors`. Connectors depend only on `semstrait-ir` (for `PlanArtifact`) and `semstrait-core` (for shared types).

Engine deps (feature-gated):
- `datafusion`: `datafusion` v52 + `datafusion-substrait` v52 (Substrait consumption)
- `duckdb`: `duckdb` crate v1.3.x (>=1.3.0, <1.4.0 — pinned for arrow 55 per DL-031)
- `trino`: `reqwest` v0.12 — REST v1/statement API with pagination
- `spark`: `uuid` v1 — structural impl only

#### Diagram 7 — E2E Abstraction Graph (Traits, Dependencies, Control Flow)

```
═══════════════════════════════════════════════════════════════════════════
CORE ABSTRACTIONS (trait/struct ownership by crate)
═══════════════════════════════════════════════════════════════════════════

semstrait-core                     Shared primitives (Schema, DataType, Expr, Grain)

semstrait-model                    SemanticModel (parsed YAML)
semstrait-catalog                  CatalogProvider (trait), CatalogRegistry
semstrait-manifest                 ManifestCompiler → CompiledManifest

semstrait-ir                       LogicalPlan, PlanArtifact, SubstraitSerializer
                                   PlanBuilder (trait)
                                     ├── finalize_node() — cross-cutting hook
                                     ├── build_scan/filter/project/aggregate/...
                                     └── DefaultPlanBuilder (zero-state default)

semstrait-planner                  SemanticPlanner
                                     └── holds: Box<dyn PlanBuilder>

semstrait-adapter                  EngineAdapter (trait)
                                     ├── fn name() → &str
                                     ├── fn plan_builder() → Option<Box<dyn PlanBuilder>>
                                     ├── fn adapt(LogicalPlan) → PlanArtifact
                                     └── fn debug_sql(LogicalPlan) → String
                                   SqlEmitter (trait), dialect impls (from former semstrait-sql)
                                   Concrete: DataFusionAdapter, DuckDbAdapter, SparkAdapter

semstrait-connectors               ComputeConnector (trait)           ← INDEPENDENT
                                     ├── fn execute(PlanArtifact) → ComputeResult
                                     ├── fn health_check()
                                     └── fn name()
                                   Concrete: DataFusionConnector, DuckDbConnector,
                                             TrinoConnector, SparkConnector

semstrait-api                      SemstraitEngine, RequestParser
semstrait (facade)                 SemstraitBuilder, SemstraitInstance

═══════════════════════════════════════════════════════════════════════════
COMPILE-TIME DEPENDENCY GRAPH (crate → crate)
═══════════════════════════════════════════════════════════════════════════

                    semstrait (facade)
                   ╱      │       ╲
                  ╱       │        ╲
        semstrait-api  semstrait-connectors  semstrait-adapter
              │              │                    │
              └──────┬───────┘                    │
                     │                            │
              semstrait-planner ←── NO dep on adapter
                     │
                 semstrait-ir  ←── PlanBuilder trait lives here
                     │
             semstrait-manifest
                     │
              semstrait-catalog
                     │
              semstrait-model
                     │
              semstrait-core

  Key: planner → ir (for PlanBuilder). Facade bridges adapter→plan_builder.

═══════════════════════════════════════════════════════════════════════════
CONTROL FLOW — Query Execution (numbered call sequence)
═══════════════════════════════════════════════════════════════════════════

  Consumer App
      │
      │ ①  SemstraitBuilder::new()
      │      .with_manifest_yaml(yaml)
      │      .with_adapter(adapter)           // Arc<dyn EngineAdapter>
      │      .with_connector(connector)       // Optional
      │      .build().await
      │
      │    build() internals:
      │      a. ManifestCompiler.compile(yaml) → CompiledManifest
      │      b. adapter.plan_builder() → Option<Box<dyn PlanBuilder>>
      │      c. SemanticPlanner::builder()
      │           .with_plan_builder(builder)  // extracted from adapter
      │           .build()
      │      d. Store adapter + connector on SemstraitInstance
      │
      ▼
  SemstraitInstance
      │
      │ ②  instance.query(request)
      │
      ├──→ ③  planner.plan(request) → LogicalPlan
      │         Uses PlanBuilder for engine-specific node construction
      │
      ├──→ ④  adapter.adapt(plan) → PlanArtifact
      │         Substrait (DataFusion) or SQL (DuckDB/Trino/Spark)
      │
      ├──→ ⑤  connector.execute(artifact) → ComputeResult  [optional]
      │         Pure execution — no knowledge of adapter
      │
      └──→ ⑥  Return ComputeResult (or PlanArtifact if no connector)

  Library mode (no connector):
      Steps ①→④ only. Consumer receives PlanArtifact.
      Consumer app owns engine integration.

  Full mode (with connector):
      Steps ①→⑥. Semstrait handles execution end-to-end.

═══════════════════════════════════════════════════════════════════════════
TRAIT RELATIONSHIP CONSTRAINTS
═══════════════════════════════════════════════════════════════════════════

  EngineAdapter ──PROVIDES──► PlanBuilder  (fn plan_builder(), optional)
  ComputeConnector ──INDEPENDENT──►       (no adapter() method)
  SemanticPlanner ──DEPENDS-ON──► PlanBuilder (trait in IR)
  Facade ──BRIDGES──► adapter.plan_builder() → planner.with_plan_builder()

  DefaultPlanBuilder: zero-state unit struct implementing PlanBuilder.
    Used for: default behavior when adapter provides no custom builder.
```

---

### 5.10 `semstrait-api`

**Role:** Entry point for network/CLI consumers. All transports share `RequestParser` and `SemstraitEngine`.

**Submodule structure:**

```rust
// Shared — always compiled
pub mod parse {
    pub struct RequestParser;
    impl RequestParser {
        /// Parse a RawQueryRequest into a ResolvedQueryRequest.
        /// Resolves kind/measure names against the manifest.
        /// Performs grain coercion (e.g. if no grain specified → finest available).
        pub fn parse(
            raw: RawQueryRequest,
            manifest: &CompiledManifest,
        ) -> Result<ResolvedQueryRequest, ParseError>;
    }
}

pub struct SemstraitEngine {
    manifest:        Option<CompiledManifest>,
    default_adapter: Option<Arc<dyn EngineAdapter>>,    // adapter-based
    connector:       Option<Arc<dyn ComputeConnector>>, // optional execution
}

impl SemstraitEngine {
    pub fn new() -> Self;
    pub fn with_manifest(manifest: CompiledManifest) -> Self;
    pub fn with_adapter(
        manifest: CompiledManifest,
        adapter: Arc<dyn EngineAdapter>,
    ) -> Self;
    pub fn with_connector(
        manifest: CompiledManifest,
        connector: Arc<dyn ComputeConnector>,
    ) -> Self;
    pub async fn with_manifest_yaml(yaml: &str) -> Result<Self, EngineError>;

    pub fn validate(&self, raw: &RawQueryRequest) -> ValidationResult;
    pub async fn explain(&self, raw: &RawQueryRequest)
        -> Result<ExplainResult, EngineError>;          // returns PlanArtifact
    pub async fn query(&self, raw: &RawQueryRequest)
        -> Result<serde_json::Value, EngineError>;      // requires connector
}

// gRPC transport — feature = "grpc" (tonic 0.14, DL-034)
#[cfg(feature = "grpc")]
pub mod grpc {
    // Proto generated via tonic-prost-build from crates/semstrait-api/proto/service.proto
    pub struct SemstraitGrpcService { engine: SharedEngine }
    // Implements: Explain, Validate, Query, Health RPCs
    // Proto: semstrait.v1.SemstraitService
}

// REST transport — feature = "rest"
#[cfg(feature = "rest")]
pub mod rest {
    pub fn router(engine: Arc<SemstraitEngine>) -> axum::Router;
    // POST /query   → ComputeResult as JSON
    // POST /explain → ExplainResult as JSON
    // GET  /schema  → SchemaResponse as JSON
    // POST /compile → CompileResponse as JSON
}

// CLI transport — feature = "cli"
#[cfg(feature = "cli")]
pub mod cli {
    pub async fn run() -> Result<(), Box<dyn std::error::Error>>;
    // Commands: compile | explain | validate | query (datafusion) | query-duckdb (duckdb) | serve (rest)
    // Binary target: `cargo build -p semstrait-api --features cli,rest,datafusion`
}
```

**Proto surface:**
```protobuf
service SemstraitService {
  rpc Query    (QueryRequest)   returns (QueryResponse);
  rpc Explain  (QueryRequest)   returns (ExplainResponse);
  rpc Validate (QueryRequest)   returns (ValidationResponse);
  rpc Schema   (SchemaRequest)  returns (SchemaResponse);
  rpc Compile  (CompileRequest) returns (CompileResponse);
}
```

**External deps:** `semstrait-core`, `semstrait-planner`, `semstrait-manifest`, `semstrait-connectors`

Transport deps (feature-gated): `tonic` + `prost` (grpc), `axum` (rest), `clap` (cli)

---

### 5.11 `semstrait` (Facade)

**Role:** Single entry point for library consumers. Builder API. Feature flag coordination. Public API re-exports. Bridges adapter → plan_builder → planner.

```rust
pub struct SemstraitInstance {
    manifest_yaml: String,
    manifest: CompiledManifest,
    planner: SemanticPlanner,
    adapter: Option<Arc<dyn EngineAdapter>>,       // independent
    connector: Option<Arc<dyn ComputeConnector>>,   // independent
}

pub struct SemstraitBuilder {
    manifest_yaml: Option<String>,
    manifest_path: Option<PathBuf>,
    catalog:       Option<Arc<dyn CatalogProvider>>,
    adapter:       Option<Arc<dyn EngineAdapter>>,      // NEW (DL-059)
    connector:     Option<Arc<dyn ComputeConnector>>,
}

impl SemstraitBuilder {
    pub fn new() -> Self;
    pub fn with_manifest_yaml(self, yaml: impl Into<String>) -> Self;
    pub fn with_manifest_file(self, path: impl Into<PathBuf>) -> Self;
    pub fn with_catalog(self, catalog: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_adapter(self, adapter: Arc<dyn EngineAdapter>) -> Self;      // NEW
    pub fn with_connector(self, connector: Arc<dyn ComputeConnector>) -> Self;
    pub async fn build(self) -> Result<SemstraitInstance, BuildError>;
    // build() internals:
    //   1. ManifestCompiler.compile(yaml) → CompiledManifest
    //   2. adapter.plan_builder() → Option<Box<dyn PlanBuilder>> (or DefaultPlanBuilder)
    //   3. SemanticPlanner::builder().with_plan_builder(builder).build()
    //   4. Store adapter + connector independently on SemstraitInstance
}

impl SemstraitInstance {
    pub fn builder() -> SemstraitBuilder;
    pub fn manifest(&self) -> &CompiledManifest;
    pub fn explain(&self, req: &ResolvedQueryRequest) -> Result<String, String>; // sync SQL
    pub fn plan(&self, req: &ResolvedQueryRequest)
        -> Result<PlanArtifact, BuildError>;  // plan + adapt (library mode)
    pub async fn query(&self, req: &ResolvedQueryRequest)
        -> Result<ComputeResult, BuildError>;  // plan + adapt + execute (requires connector)
}

// Library mode (no connector — consumer owns execution):
let sem = Semstrait::builder()
    .with_manifest_yaml(yaml_str)
    .with_adapter(Arc::new(DataFusionAdapter::new()))
    .build()
    .await?;

let artifact = sem.plan(&request)?;  // PlanArtifact::Substrait
// Consumer passes artifact to their own DataFusion SessionContext

// Full mode (with connector — semstrait handles execution):
let sem = Semstrait::builder()
    .with_manifest_yaml(yaml_str)
    .with_adapter(Arc::new(DuckDbAdapter::new()))
    .with_connector(Arc::new(DuckDbConnector::new()?))
    .build()
    .await?;

let result = sem.query(&request).await?;  // ComputeResult
```

**Key design decisions:**
- Facade provides `with_adapter()`. Internally extracts `adapter.plan_builder()` for the planner. Planner keeps `with_plan_builder()` (cannot depend on adapter crate — DAG constraint).
- **DL-060:** `plan()` and `query()` pass `PlanArtifact` directly to connector. Never discard Substrait in favor of SQL.
- Adapter and connector are independent — either can be set without the other. `plan()` requires adapter; `query()` requires both.
- `DefaultPlanBuilder` (in `semstrait-ir`) is the fallback when no adapter is provided or adapter returns `None` from `plan_builder()`.
- **DL-061:** `ExprSource` custom Deserialize (fixes DL-049). Replaced `#[serde(untagged)]` with manual `impl Deserialize` that routes `String` → `Inline`, `Mapping` → `Declarative(ExprBlock)`. Enables declarative expressions at Tier 2 (kind-level dimensions).
- **DL-062:** Expression scope validation (SR-5a). `validate_expr_scope()` in `steps.rs` checks that all `Expr::Column` references in computed dimension and metric expressions refer to names exposed in the same interface. Measure base expressions are exempt (reference physical columns).
- **DL-063:** Parse-time uniqueness enforcement (SR-1/SR-3). `vec_to_btreemap_unique()` in `common.rs` detects duplicate dimension/measure/metric/filter names at parse time.
- **DL-064:** `DataKindBinding` + `DataKindBindingExtras` use `#[serde(deny_unknown_fields)]` (SR-4). Prevents semantic fields on dataset bindings within complex kinds.

**Feature flags:**
```toml
[features]
default         = ["duckdb"]
duckdb          = ["semstrait-adapter/duckdb", "semstrait-connectors/duckdb"]
datafusion      = ["semstrait-adapter/datafusion", "semstrait-connectors/datafusion"]
trino           = ["semstrait-adapter/trino", "semstrait-connectors/trino"]
spark           = ["semstrait-adapter/spark", "semstrait-connectors/spark"]
catalog-iceberg = ["semstrait-catalog/iceberg"]
api-grpc        = ["semstrait-api/grpc"]
api-rest        = ["semstrait-api/rest"]
api-cli         = ["semstrait-api/cli"]
```

---

## 6. Key Workspace Dependencies

```toml
[workspace.dependencies]
# Serialization
serde        = { version = "1", features = ["derive"] }
serde_yaml   = "0.9"
serde_json   = "1"
prost        = "0.14"

# Query IR
substrait    = { version = "0.62", features = ["serde"] }
arrow        = { version = "55", default-features = false, features = ["json"] }
arrow-schema = "55"
datafusion   = { version = "52", default-features = false, features = ["sql"] }

# SQL generation
polyglot-sql = { version = "0.1", default-features = false }  # transpilation (feature-gated)
# sqlparser "0.53" — retained for polyglot-sql transpile feature and future SQL expr parsing

# Graph analysis (manifest compiler only)
petgraph     = "0.7"

# Async
tokio        = { version = "1", features = ["full"] }
async-trait  = "0.1"

# HTTP client
reqwest      = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

# API (optional)
axum         = "0.8"
clap         = { version = "4", features = ["derive"] }
tonic        = "0.12"

# Utilities
uuid         = { version = "1", features = ["v4"] }
indexmap      = { version = "2", features = ["serde"] }
thiserror    = "2"
tracing      = "0.1"
chrono       = { version = "0.4", features = ["serde"] }
sha2         = "0.10"
```

---

## 7. Connection Verification Summary

This table records every cross-crate type reference and confirms no cycle is introduced.

| Type | Defined in | Used in | Verification |
|---|---|---|---|
| `Schema`, `DataType`, `Expr` | `core` | All crates | ✓ all depend on core |
| `PlanBuilder` trait | `ir` | `planner`, `adapter` | ✓ planner and adapter both depend on ir |
| `GlobPattern` | `model` | `manifest` (expand_globs), `catalog` (list_tables) | ✓ model → core; no reverse dep |
| `SemanticModel`, `Kind`, `Dataset` | `model` | `manifest` only | ✓ model is input to manifest compiler |
| `CatalogProvider` trait | `catalog` | `manifest`, `planner` | ✓ catalog → core only; no dep on manifest or planner |
| `CompiledManifest`, `CompiledDataKind` | `manifest` | `planner`, `api`, `facade` | ✓ manifest → model, catalog; no dep on planner |
| `PlanNode`, `LogicalPlan` | `ir` | `planner`, `sql`, `adapter` | ✓ ir → core only; downstream use is one-way |
| `PlanArtifact` | `ir` | `adapter`, `connectors`, `api` | ✓ ir → core; all consumers depend on ir |
| `SubstraitSerializer` | `ir` | `adapter` (DataFusionAdapter) | ✓ ir → core; adapter → ir |
| `SemAnnotation` | `ir` | `planner` (sets annotations), `adapter` (reads for Explain) | ✓ annotation type defined in ir, set by planner |
| `ResolvedQueryRequest` | `planner` | `planner` internally, `api` (calls planner) | ✓ lives in planner; api → planner |
| `KindPlanner` trait | `planner` | `planner` (registry) | ✓ internal to planner crate |
| `Optimizer`, `OptimizerPass` | `planner` | `planner` (internal), `facade` (configuration) | ✓ no external crate depends on these for execution |
| `SqlEmitter`, `SqlDialect` | `adapter` (internal) | `adapter` (SQL adapters use emitter) | ✓ internal to adapter crate |
| `EngineAdapter` trait | `adapter` | `connectors`, `api`, `facade` | ✓ adapter → core, ir, sql; downstream is one-way |
| `ComputeConnector` | `connectors` | `api`, `facade` | ✓ connectors → core, adapter; api → connectors |
| `ComputeResult` | `connectors` | `api`, `facade` | ✓ same crate |
| `SemstraitEngine` | `api` | `facade` | ✓ api is only used by facade and binaries |
| `RequestParser` | `api` | `api` (internal to submodules) | ✓ |

**No cycles detected.** Dependency graph is a strict DAG with `semstrait-core` as the unique root with no outgoing workspace edges.

---

## 8. Deferred Items

Items not yet implemented. Completed items archived in DECISION_LOG.md.

| Item | Notes |
|---|---|
| Arrow Flight SQL | Databricks-specific, not Spark/Trino (DL-029) |
| Spark Substrait support | Spark 3.4+ experimental; default to SQL emitter |
| Multi-engine query fan-out | Single connector per `Semstrait` instance |
| Cross-kind metric refs | Prohibited v1 (COMP_E006); multi-kind planning deferred |
| Many-to-many junction tables | Bridge as explicit dataset in joinset |
| Glue/Hive catalogs | Heavy deps; Unity done, Glue/Hive deferred |
| Unified three-shape column_mapping | Shape 1 (string), Shape 2 (anchor map), Shape 2.1 (inline expr) |
| Two-stage metric aggregation | Metric-level `agg:` with inner/outer grain planning |
| Ratio/window structured aggregation | `ratio:` and `window:` YAML tags on metrics |
| Planner metadata dim injection | ScanNode literal column injection per source path (metadata dims handled; computed dims done in Phase G) |
| Model hash caching | Content hash as manifest cache key — design decided, not implemented |
| Declarative YAML in nested kinds | serde_yaml 0.9 nested untagged enum limitation — declarative expr blocks work in `datasets:` but not `grainsets/unionsets/joinsets` (DL-049) |

---

## 9. Diagram Locations

Workspace-level diagrams live in `docs/`. Module-specific diagrams live in crate-level `docs/` directories.

| Diagram | Location |
|---------|----------|
| D1 — Crate Layer Architecture | `docs/D1_crate_layer_architecture.svg` |
| D2 — System Pipeline | `docs/D2_system_pipeline.svg` |
| D3 — Planner Evaluation Order | `crates/semstrait-planner/docs/D3_planner_evaluation_order.svg` |
| D4 — PlanNode Substrait Map | `crates/semstrait-ir/docs/D4_plannode_substrait_map.svg` |
| D5 — Kind Interface Binding | `crates/semstrait-planner/docs/D5_kind_interface_binding.svg` |
| D6 — Connector Architecture | `crates/semstrait-connectors/docs/D6_connector_architecture.svg` |
| D7 — E2E Abstraction Graph | Inline in CONTEXT.md §5.9 (traits, dependencies, control flow) |

---

## 10. Module Document Index

Each crate's `MODULE.md` is derived from the corresponding section in this document plus the per-module details listed below.

| Document | Primary source section | Additional content |
|---|---|---|
| `semstrait-core/MODULE.md` | §5.1 | Unified `Expr` variant catalog; `DataType` ANSI logical types (8 variants); error type hierarchy |
| `semstrait-model/MODULE.md` | §5.2 | Full YAML schema specification (v1.4); ref resolution algorithm; `GlobPattern` grammar |
| `semstrait-catalog/MODULE.md` | §5.3 | `CatalogProvider` contract; per-impl authentication flows; `NullCatalogProvider` behavior spec |
| `semstrait-manifest/MODULE.md` | §5.4 | Compilation pipeline step-by-step; `CompiledManifest` JSON schema; error codes (COMP_E*) |
| `semstrait-ir/MODULE.md` | §5.5 | Complete `PlanNode` variant specs; Substrait extension proto definition; ordinal invariant proofs; `PlanBuilder` trait |
| `semstrait-planner/MODULE.md` | §5.6 | Full evaluation order spec; module structure (Phase H); `ExprResolver` trait; `DecomposedMeasure`; `DatasetPlanner`; `GrainsetPlanner` coverage algorithm; `UnionsetPlanner` branch pruning; `JoinsetPlanner` BFS + field resolution; error codes (PLAN_E*, PLAN_W*) |
| `semstrait-connectors/MODULE.md` | §5.8 | Trait contracts; per-engine: wire protocol, auth methods, known limitations, payload support matrix. See also D6 in `crates/semstrait-connectors/docs/`. |
| `semstrait-api/MODULE.md` | §5.9 | Proto definitions; `RequestParser` algorithm; per-transport setup guide |
| `semstrait/MODULE.md` | §5.10 | Builder usage examples; feature flag combinations; embedding guide |
