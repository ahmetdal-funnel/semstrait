# Semstrait Architecture Document
**Version:** 3.0 | **Status:** Design — authoritative reference for per-module documentation

---

## 1. What Semstrait Is

A **manifest compiler + semantic query engine** written in Rust.

It resolves semantic models (defined in YAML) into engine-executable plans by:
1. Compiling YAML model files into a validated `CompiledManifest` artifact (offline)
2. Planning `QueryRequest`s against that manifest into a `LogicalPlan` IR (online)
3. Emitting and executing the plan against a target compute engine (online)

The output is `ComputeResult` — Arrow `RecordBatch`es or JSON.

---

## 2. Guiding Decisions

These decisions were locked in during design review. Each module document must respect them.

| # | Decision | Rationale |
|---|---|---|
| D1 | `semstrait-connectors` is one crate — traits + feature-gated engine impls | Circular dep between `semstrait-compute` ↔ `semstrait-planner` is broken by moving `ConsumerProfile` to `semstrait-core`. No separation benefit without that cycle. |
| D2 | `semstrait-api` is one crate with `grpc`, `rest`, `cli` submodules | All three share `RequestParser`, error types, and `SemstraitEngine`. Feature flags gate each transport. |
| D3 | `Optimizer` lives inside `semstrait-planner`, not a separate crate | The optimizer is an internal quality pass invoked at the end of `SemanticPlanner::plan()`. It is not a public pipeline stage. Callers receive a finished `LogicalPlan`. |
| D4 | `Optimizer` is empty by default in v1 | Zero passes = identity function. The `OptimizerPass` trait and `Optimizer` struct exist on day one for extensibility. Passes are opt-in at `SemanticPlanner` construction. |
| D5 | Glob expansion requires an explicit `CatalogProvider` | If the model contains `GlobPattern`s and `ManifestCompiler` has `catalog = None`, compilation fails with `CompileError::GlobRequiresCatalog`. No silent no-ops. |
| D6 | `Kind` has three distinct layers: interface, strategy, binding | The interface (dimensions/measures/metrics/constraints) is what users query. The strategy (`KindType`) determines the plan structure. The binding (datasets, column_mapping, relationships) is the physical implementation. |
| D7 | Manifest compilation is stateless-only in v1 | `InMemoryRepository` is the only `Repository` implementation. The trait exists for v2 extensibility. The document avoids `FileSystemRepository` or `ObjectStoreRepository` in v1 scope. |
| D8 | YAML field is `constraints`, not `requires` | Constraints are pre-resolution validity gates evaluated at step 0 of planning, before any dataset routing. They apply to the query scope, not to individual datasets. |

---

## 3. Crate Workspace

```
semstrait/                       Cargo workspace root
├── semstrait-core/              Foundation — shared primitives, zero internal deps
├── semstrait-model/             YAML model parsing and ref resolution
├── semstrait-catalog/           CatalogProvider trait + implementations
├── semstrait-manifest/          ManifestCompiler + Repository (InMemory v1)
├── semstrait-ir/                PlanNode IR + Substrait bridge
├── semstrait-planner/           SemanticPlanner + KindPlanners + Optimizer
├── semstrait-sql/               SqlEmitter trait + dialect implementations
├── semstrait-connectors/        Compute traits + feature-gated engine impls
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
| `semstrait-sql` | `semstrait-core`, `semstrait-ir` |
| `semstrait-connectors` | `semstrait-core`, `semstrait-ir`, `semstrait-sql` |
| `semstrait-api` | `semstrait-core`, `semstrait-planner`, `semstrait-manifest`, `semstrait-connectors` |
| `semstrait` (facade) | `semstrait-planner`, `semstrait-manifest`, `semstrait-connectors`, `semstrait-catalog` |

**Connection verification — cycle check:**
- `semstrait-connectors` needs `ConsumerProfile` (for `ComputeAdapter::consumer_profile()`). `ConsumerProfile` is in `semstrait-core`. `semstrait-planner` also needs `ConsumerProfile` (for `AdditivityResolver`). Both get it from `core`. No cycle. ✓
- `semstrait-planner` needs `ComputeEmitter` result to decide how to structure plans? No — the planner produces `LogicalPlan`. The emitter is downstream in `semstrait-connectors`. The planner only needs `ConsumerProfile` (from `core`) to select strategies. ✓
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
│  Connector layer                                                     │
│  semstrait-connectors — ComputeEmitter · ComputeAdapter ·           │
│    ComputeConnector traits (always compiled)                        │
│  engine impls: duckdb · datafusion · trino · spark (feature-gated)  │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  Planning layer                                                      │
│  semstrait-planner — kind planners · Optimizer (empty v1) ·         │
│    ConstraintEvaluator · AdditivityResolver                         │
│  semstrait-sql — SqlEmitter trait · dialect impls (ansi/trino/…)    │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  IR layer                                                            │
│  semstrait-ir — PlanNode enum · SemAnnotation · SubstraitSerializer  │
│  semstrait-manifest — ManifestCompiler · InMemoryRepository         │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  Definition layer                                                    │
│  semstrait-model — parsed YAML types · ref resolution · GlobPattern  │
│  semstrait-catalog — CatalogProvider trait · iceberg · unity · glue  │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│  Foundation — semstrait-core                                         │
│  Schema · DataType · DslExpr · ConsumerProfile · Grain · errors      │
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
YAML source files                 │  QueryRequest
        │                         │          │
        ▼                         │          ▼
CatalogProvider ──┐               │  RequestParser
(optional; req    │               │    parse · resolve refs · grain coerce
 for globs)       │               │          │
                  ▼               │          ▼
ManifestCompiler.compile() ◄──────┤  ConstraintEvaluator (step 0)
  parse → expand globs            │    pre-resolution validity gate
  validate → compile expressions  │          │  (Err → PlannerError::ConstraintViolation)
  build petgraph DAG              │          ▼
        │                         │  SemanticPlanner
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
                                  │  ComputeEmitter.emit()
                                  │    → SQL | SubstraitPlan | NativePlan
                                  │          │
                                  │          ▼
                                  │  ComputeAdapter.adapt()
                                  │    negotiate via ConsumerProfile
                                  │          │
                                  │          ▼
                                  │  ComputeConnector.execute()
                                  │    duckdb · datafusion · trino · spark
                                  │          │
                                  │          ▼
                                  │  ComputeResult → Arrow RecordBatches
──────────────────────────────────┴──────────────────────────────────────
```

---

## 5. Module Specifications

### 5.1 `semstrait-core`

**Role:** Shared primitives. Every other crate depends on this. Must stay small and zero-dep within the workspace.

**Key types:**

```rust
/// Canonical column schema. Ordinals are stable after construction.
/// Every PlanNode carries an output_schema; parent nodes derive field
/// references via schema.ordinal("name") — never by positional index.
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

pub struct SchemaColumn {
    pub name:      String,
    pub data_type: DataType,
    pub nullable:  bool,
    pub ordinal:   u32,
}

impl Schema {
    pub fn ordinal(&self, name: &str) -> Result<u32, SchemaError>;
    pub fn join(left: &Schema, right: &Schema) -> Schema; // ordinals: [left | left.len + right]
    pub fn project(&self, keep: &[&str]) -> Schema;
    pub fn emit_mapping(&self, target: &Schema) -> Vec<u32>;
}

/// Arrow-aligned type system. Bidirectional with arrow::datatypes::DataType.
pub enum DataType {
    Boolean,
    Int8 | Int16 | Int32 | Int64,
    UInt8 | UInt16 | UInt32 | UInt64,
    Float32 | Float64,
    Decimal { precision: u8, scale: i8 },
    Utf8 | LargeUtf8,
    Date32 | Date64,
    TimestampSecond | TimestampMillisecond | TimestampMicrosecond,
    Duration,
    List(Box<DataType>),
    Struct(Vec<StructField>),
    Binary,
}

/// ConsumerProfile lives here to break planner ↔ connector circular dep.
/// Connectors produce it; the planner reads it for strategy decisions.
pub struct ConsumerProfile {
    pub supports_window_functions: bool,
    pub supports_full_outer_join:  bool,
    pub supports_cte:              bool,
    pub supports_fetch_rel:        bool,
    pub max_join_depth:            Option<usize>,
    pub substrait_function_uris:   HashSet<String>,
}

impl ConsumerProfile {
    pub fn semi_additive_strategy(&self) -> SemiAdditiveStrategy;
}
pub enum SemiAdditiveStrategy { WindowFunction, DoubleAggregate }

/// Temporal grain levels — used in dimension types and DateTrunc expressions.
pub enum Grain { Minute, Hour, Day, Week, Month, Quarter, Year }

/// MeasureConstraints — the runtime type for the `constraints:` YAML field.
/// Evaluated at step 0 (pre-resolution). This is NOT `requires`.
pub struct MeasureConstraints {
    pub dimensions:   Option<DimensionConstraints>,
    pub aggregations: Option<AggregationConstraints>,
}
pub struct DimensionConstraints {
    pub one_of: Vec<String>,
    pub none_of: Vec<String>,
    pub all:    Vec<String>,
}
pub struct AggregationConstraints {
    pub allowed:    Vec<String>,
    pub prohibited: Vec<String>,
}

/// DSL expression tree — the ONLY way to express computations in v1.
/// Raw SQL strings are rejected at compile time.
pub enum DslExpr {
    Column(String),
    Literal(Literal),
    EntityRef(String),
    Sum(Box<DslExpr>) | Count(Box<DslExpr>) | CountDistinct(Box<DslExpr>)
      | Avg(Box<DslExpr>) | Min(Box<DslExpr>) | Max(Box<DslExpr>),
    Add(Vec<DslExpr>) | Subtract(..) | Multiply(..) | Divide(..)
      | SafeDivide(..) | Negate(..),
    Eq(..) | Ne(..) | Gt(..) | Gte(..) | Lt(..) | Lte(..)
      | InList(..) | Between(..) | Like(..) | IsNull(..) | IsNotNull(..),
    And(Vec<DslExpr>) | Or(..) | Not(..),
    Case { when: Vec<WhenClause>, else_expr: Option<Box<DslExpr>> },
    Coalesce(..) | NullIf(..),
    DateTrunc { grain: Grain, expr: Box<DslExpr> },
    /// Renders as CASE WHEN condition THEN expr END.
    /// Used for measure filters in multi-measure aggregation context.
    Guard { condition: Box<DslExpr>, expr: Box<DslExpr> },
}
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
    pub domain:  Option<DomainSpec>,   // DomainSpec::Single | Multi
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
    pub domain:     Option<DomainSpec>,
    pub keys:       Option<Keys>,
    pub dimensions: Vec<DimensionEntry>,
    pub measures:   Vec<MeasureEntry>,
    pub metrics:    Vec<MetricEntry>,
    pub datasets:   Vec<KindDatasetEntry>,     // Inline | Ref(kind_name)
    pub relationships: Vec<KindRelationship>,  // required for Joinset
}

/// Dataset name in a kind can be a literal name or a glob pattern.
/// Glob expansion happens in ManifestCompiler, not here.
pub enum DatasetName {
    Literal(String),
    Glob(GlobPattern),
}
pub struct GlobPattern(pub String);  // contains `*` or `?`

pub struct KindDataset {
    pub name:           DatasetName,
    pub column_mapping: ColumnMapping,  // semantic_name → physical_col (or {col, grain})
    pub extras:         Option<KindExtras>,
}

/// ColumnMapping value: simple string or structured with optional grain override.
pub enum ColumnMappingValue {
    Simple(String),               // physical column name
    WithGrain { column: String, grain: Option<Grain> },
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

| Feature flag | Impl | Backend | Protocol |
|---|---|---|---|
| `iceberg` | `IcebergRestCatalog` | Polaris, Gravitino | Iceberg REST spec |
| `unity` | `UnityCatalog` | Databricks Unity Catalog | UC REST API |
| `glue` | `GlueCatalog` | AWS Glue | AWS SDK |
| `hive` | `HiveCatalog` | Hive Metastore | Thrift / HTTP |
| *(always)* | `NullCatalogProvider` | None | — |

**External deps:** `semstrait-core`, `async-trait`, `tokio`, `reqwest` (optional)

---

### 5.4 `semstrait-manifest`

**Role:** Compile `SemanticModel` → `CompiledManifest`. Own the `Repository` trait. Ship only `InMemoryRepository` in v1.

#### ManifestCompiler

```rust
pub struct ManifestCompiler {
    catalog: Option<Arc<dyn CatalogProvider>>,
}

impl ManifestCompiler {
    pub fn new() -> Self;
    pub fn with_catalog(self, c: Arc<dyn CatalogProvider>) -> Self;

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
Step 1  parse              serde_yaml → SemanticModel (semstrait-model)
Step 2  resolve_refs       expand ref: entries into inline definitions
Step 3  expand_globs       GlobPattern → Vec<concrete KindDataset>
                           REQUIRES catalog. Err if catalog = None and globs exist.
Step 4  validate_structure dataset uniqueness, kind nesting matrix,
                           joinset anchor rules, ref target existence
Step 5  validate_mappings  column_mapping keys exist in kind interface;
                           physical columns verified against catalog.get_schema() if available
Step 6  build_metric_graph petgraph DiGraph — detect cycles, enforce depth ≤ 3
Step 7  build_rel_graph    petgraph DiGraph — joinset anchor inference (in-degree = 0)
Step 8  compile_exprs      parse DslExpr fields; reject raw SQL strings
Step 9  emit               serialize to CompiledManifest (JSON, versioned)
```

**Glob expansion detail (step 3):**
- For each `KindDataset { name: DatasetName::Glob(pattern), column_mapping: template, .. }`:
  - Call `catalog.list_tables(namespace, &pattern)` → `Vec<TableRef>`
  - For each `TableRef`: clone `template`, substitute `{name}` placeholder in mapping values
  - Append `KindDataset::Inline` entries for each match
- If `catalog = None` and any `GlobPattern` is found → `CompileError::GlobRequiresCatalog { pattern, kind }`
- The `CompiledManifest` contains only concrete, expanded datasets. No `GlobPattern` survives.

#### Repository

```rust
/// Storage abstraction for CompiledManifest.
/// v1 ships InMemoryRepository only. FileSystem and ObjectStore are v2.
pub trait Repository: Send + Sync {
    fn load(&self) -> Result<Arc<CompiledManifest>, RepositoryError>;
    fn save(&self, manifest: &CompiledManifest) -> Result<(), RepositoryError>;
}

pub struct InMemoryRepository(Arc<RwLock<Option<Arc<CompiledManifest>>>>);
```

#### CompiledManifest

```rust
#[derive(Serialize, Deserialize)]
pub struct CompiledManifest {
    pub version:       u32,
    pub compiled_at:   DateTime<Utc>,
    pub source_hash:   String,                          // SHA-256 of YAML input
    pub datasets:      IndexMap<String, CompiledDataset>,
    pub kinds:         IndexMap<String, CompiledKind>,
    pub relationships: Vec<CompiledRelationship>,
    pub diagnostics:   Vec<Diagnostic>,
}

pub struct CompiledKind {
    // Interface layer (what users query)
    pub dimensions:    IndexMap<String, CompiledDimension>,
    pub measures:      IndexMap<String, CompiledMeasure>,
    pub metrics:       IndexMap<String, CompiledMetric>,
    pub keys:          Option<CompiledKeys>,
    // Strategy layer
    pub kind_type:     KindType,    // Grainset | Unionset | Joinset { associativity }
    // Binding layer
    pub datasets:      Vec<CompiledKindDataset>,  // fully expanded, no GlobPattern
    pub relationships: Vec<CompiledRelationship>, // joinset only
    pub domain:        Option<Vec<String>>,       // domain hints for step 0 filter
}

pub struct CompiledMeasure {
    pub name:        String,
    pub data_type:   DataType,
    pub expr:        DslExpr,
    pub additivity:  Additivity,
    pub constraints: Option<MeasureConstraints>,  // field = `constraints:` in YAML
    pub filters:     Vec<MeasureFilter>,
}
```

**External deps:** `semstrait-core`, `semstrait-model`, `semstrait-catalog`, `petgraph`, `serde_json`, `sha2`, `tokio`

---

### 5.5 `semstrait-ir`

**Role:** Define the `LogicalPlan` IR in-memory. Provide bidirectional Substrait serialization. Carry semantic annotations per node.

**Design principle:** `PlanNode` ordinals == Substrait `structField` ordinals. Schema is always attached to each node. Parent nodes never guess field positions.

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
    DomainHint(String),
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

#### ExprConverter — DslExpr ↔ Substrait Expression

```rust
/// Converts DslExpr to/from substrait::proto::Expression.
/// Schema is required for Column → FieldReference ordinal lookup.
pub struct ExprConverter<'s> {
    schema: &'s Schema,
}

impl<'s> ExprConverter<'s> {
    pub fn to_substrait(&self, expr: &DslExpr)
        -> Result<substrait::proto::Expression, ConvertError>;
    pub fn from_substrait(&self, expr: &substrait::proto::Expression)
        -> Result<DslExpr, ConvertError>;
}
```

**Key DslExpr → Expression mappings:**

| `DslExpr` | `Expression` |
|---|---|
| `Column(name)` | `FieldReference { struct_field: { field: schema.ordinal(name) } }` |
| `Literal(v)` | `Literal { literal_type: ... }` |
| `Eq(l, r)` | `ScalarFunction { fn_anchor, args: [l, r] }` (ANSI comparison) |
| `Guard { cond, expr }` | `IfThen { ifs: [{ if: cond, then: expr }], else_: null_literal }` |
| `DateTrunc { grain, expr }` | `ScalarFunction { fn_anchor: date_trunc, args: [grain_lit, expr] }` |
| `Sum(inner)` | Used inside `AggregateRel.measures[i].function` |

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

**External deps:** `semstrait-core`, `substrait` (prost-generated), `uuid`, `prost`

---

### 5.6 `semstrait-planner`

**Role:** Build `LogicalPlan` from `QueryRequest` + `CompiledManifest`. Dispatch to kind-specific planners. Evaluate constraints, additivity, filters. Apply optimizer internally.

#### SemanticPlanner

```rust
pub struct SemanticPlanner {
    catalog:   Option<Arc<dyn CatalogProvider>>,
    optimizer: Optimizer,       // empty in v1; configured at construction
    planners:  KindPlannerRegistry,
}

pub struct SemanticPlannerBuilder {
    catalog: Option<Arc<dyn CatalogProvider>>,
    passes:  Vec<Box<dyn OptimizerPass>>,
}
impl SemanticPlannerBuilder {
    pub fn new() -> Self;
    pub fn with_catalog(self, c: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_optimizer_pass(self, p: impl OptimizerPass + 'static) -> Self;
    pub fn build(self) -> SemanticPlanner;
}

impl SemanticPlanner {
    pub fn builder() -> SemanticPlannerBuilder;

    pub async fn plan(
        &self,
        request: &ResolvedQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<LogicalPlan, PlannerError>;
}
```

**`plan()` internal steps:**
```
1. ConstraintEvaluator::check(request, manifest)   → Err if violated
2. domain filter                                    → narrow candidate set
3. KindPlannerRegistry::dispatch(kind_type)         → KindPlanner
4. KindPlanner::resolve(kind, request, ctx)         → PlanFragment
5. AdditivityResolver::resolve(fragment, measure, ..)
6. inject dataset filter         (FilterNode, source = DatasetFilter)
7. inject measure filter         (Guard in AggNode or FilterNode)
8. inject metric filter          (FilterNode, source = MetricFilter)
9. inject user filter            (FilterNode, source = UserFilter)
10. Optimizer::apply(plan)       → LogicalPlan  (identity in v1)
```

#### ConstraintEvaluator

```rust
pub struct ConstraintEvaluator;
impl ConstraintEvaluator {
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

#### KindPlanner — Strategy Pattern

```rust
pub trait KindPlanner: Send + Sync {
    fn supports(&self, kind_type: &KindType) -> bool;
    fn resolve(
        &self,
        kind: &CompiledKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext,
    ) -> Result<PlanFragment, PlannerError>;
}

pub struct PlannerContext<'a> {
    pub manifest: &'a CompiledManifest,
    pub profile:  &'a ConsumerProfile,   // from ComputeConnector
    pub catalog:  Option<&'a dyn CatalogProvider>,
    pub session:  &'a SessionVariables,  // runtime values (tenant_id, etc.)
}

pub struct PlanFragment {
    pub root:            PlanNode,
    pub output_schema:   Schema,
    pub pending_filters: Vec<DslExpr>,  // filters not yet injected
}
```

**KindPlanner implementations:**

| Impl | KindType | Resolution |
|---|---|---|
| `GrainsetPlanner` | `Grainset` | Route to cheapest covering dataset; FULL OUTER join if no single covers all |
| `UnionsetPlanner` | `Unionset` | Build UNION ALL branches; NULL-fill unmapped columns; prune NULL-excluding branches |
| `JoinsetPlanner` | `Joinset` | BFS from anchor dataset; join pruning; temporal filter injection per dataset |

#### AdditivityResolver

```rust
pub struct AdditivityResolver;
impl AdditivityResolver {
    /// Restructures the PlanFragment for semi/non additivity.
    /// Strategy selected from ConsumerProfile:
    ///   WindowFunction → ROW_NUMBER OVER (PARTITION BY non_additive_dim) + filter
    ///   DoubleAggregate → sub-query with MAX/LATEST, then re-aggregate
    pub fn resolve(
        fragment: PlanFragment,
        measure: &CompiledMeasure,
        request: &ResolvedQueryRequest,
        profile: &ConsumerProfile,
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

**Planned passes (v2+):**

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
    pub domain_hint:       Option<String>,    // optional pre-filter hint
    pub session_variables: SessionVariables,  // runtime values (RLS, tenant_id)
}
```

**External deps:** `semstrait-core`, `semstrait-ir`, `semstrait-manifest`, `semstrait-catalog`

---

### 5.7 `semstrait-sql`

**Role:** Emit SQL strings from `LogicalPlan`. Pure, synchronous, no I/O. One dialect per `SqlEmitter` implementation.

**Design:** SQL is generated by walking the `PlanNode` tree and emitting SQL fragments programmatically. There are no Jinja templates. Uses `sqlparser-rs` AST as an intermediate form to guarantee syntactically correct output before converting to string.

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
}

// DslExpr → dialect SQL string
pub struct DslExprSqlRenderer<'d> { dialect: &'d dyn SqlDialect }
impl<'d> DslExprSqlRenderer<'d> {
    pub fn render(&self, expr: &DslExpr, schema: &Schema) -> Result<String, EmitError>;
}

// Implementations
pub struct AnsiSqlEmitter;    // safe baseline
pub struct TrinoSqlEmitter;   // DATE_TRUNC syntax, double-quoted identifiers
pub struct DuckDbSqlEmitter;  // DuckDB-specific strftime, backtick identifiers
pub struct SparkSqlEmitter;   // Spark SQL date_trunc, backticks
```

**DslExpr → SQL lowering table:**

| `DslExpr` | ANSI SQL |
|---|---|
| `Column(name)` | `"name"` (quoted per dialect) |
| `Literal(Int(n))` | `n` |
| `Sum(x)` | `SUM(x)` |
| `CountDistinct(x)` | `COUNT(DISTINCT x)` |
| `Guard { cond, expr }` | `CASE WHEN cond THEN expr END` |
| `DateTrunc { Day, x }` | `DATE_TRUNC('day', x)` (dialect-specific) |
| `SafeDivide(a, b)` | `CASE WHEN b = 0 THEN NULL ELSE a / b END` |

**External deps:** `semstrait-core`, `semstrait-ir`, `sqlparser`

---

### 5.8 `semstrait-connectors`

**Role:** Define the compute trait surface. Provide feature-gated engine implementations.

**Structure within crate:**
```
semstrait-connectors/
├── src/
│   ├── lib.rs           re-exports all public traits
│   ├── traits.rs        ComputeEmitter, ComputeAdapter, ComputeConnector
│   ├── payload.rs       ComputePayload, ComputeRequest, ComputeResult
│   ├── duckdb.rs        #[cfg(feature = "duckdb")]
│   ├── datafusion.rs    #[cfg(feature = "datafusion")]
│   ├── trino.rs         #[cfg(feature = "trino")]
│   └── spark.rs         #[cfg(feature = "spark")]
```

#### Core traits

```rust
pub trait ComputeEmitter: Send + Sync {
    fn emit(&self, plan: &LogicalPlan) -> Result<ComputePayload, EmitError>;
    fn supported_payloads(&self) -> &[PayloadKind];
}
pub enum PayloadKind { Sql, SubstraitPlan, NativePlan }

pub enum ComputePayload {
    Sql(String),
    SubstraitPlan(Vec<u8>),                    // serialized substrait::proto::Plan
    NativePlan(Box<dyn Any + Send + Sync>),    // engine-specific
}

/// ComputeAdapter is a supertrait of ComputeConnector.
/// Provides ConsumerProfile (read by SemanticPlanner via core) and adapts payloads.
pub trait ComputeAdapter: Send + Sync {
    fn consumer_profile(&self) -> &ConsumerProfile;
    fn adapt(&self, payload: ComputePayload) -> Result<ComputeRequest, AdaptError>;
}

/// The main async execution interface. Every engine implementation satisfies this.
#[async_trait]
pub trait ComputeConnector: ComputeAdapter + Send + Sync {
    async fn execute(&self, request: ComputeRequest)
        -> Result<ComputeResult, ConnectorError>;
    async fn health_check(&self) -> Result<(), ConnectorError>;
    fn name(&self) -> &str;
}

pub struct ComputeResult {
    pub schema:      Schema,
    pub batches:     Vec<RecordBatch>,
    pub complete:    bool,                   // false = partial result
    pub diagnostics: Vec<Diagnostic>,
    pub stats:       ExecutionStats,
}
```

#### Engine connector specifications

**Diagram 6 — Connector Architecture:**
```
OptimizedLogicalPlan
        │
        ▼
ComputeEmitter.emit() ──────────────────────────────────────
  AnsiSqlEmitter │   SubstraitEmitter  │   NativeEmitter
        │                │                     │
        ▼                ▼                     ▼
   Sql(String)    SubstraitPlan(Vec<u8>)  NativePlan(Any)
        │                │                     │
        └────────────────┴─────────────────────┘
                         │
                         ▼
         ComputeAdapter.adapt() — ConsumerProfile check
                         │
        ┌────────────────┼──────────────────┬──────────────┐
        ▼                ▼                  ▼              ▼
    DuckDB         DataFusion            Trino           Spark
  ADBC native    native SessionCtx    ADBC driver    spark-connect
  embedded         or Flight SQL      HTTP fallback  gRPC (not ADBC)
        │                │                  │              │
        └────────────────┴──────────────────┴──────────────┘
                         │
                         ▼
               ComputeResult (Arrow RecordBatches)
```

**Per-engine notes:**

| Engine | Payload support | Wire | Notes |
|---|---|---|---|
| DuckDB | `Sql` + partial `SubstraitPlan` | DuckDB C API / ADBC | Embedded; `ConsumerProfile.supports_window_functions = true` |
| DataFusion | `Sql` + `SubstraitPlan` + `NativePlan(LogicalPlan)` | In-process Rust | `NativePlan` = zero-copy; uses `datafusion-substrait` for binary plan |
| Trino | `Sql` only | ADBC driver manager (columnar-tech) / HTTP REST | `TrinoSqlEmitter` for dialect; no Substrait endpoint |
| Spark | `Sql` + `SubstraitPlan` (experimental) | spark-connect gRPC (tonic) | Not ADBC; `SparkSqlEmitter` for safe fallback |

**External deps:** `semstrait-core`, `semstrait-ir`, `semstrait-sql`, `arrow`, `async-trait`

Engine deps (feature-gated):
- `duckdb`: `duckdb` crate
- `datafusion`: `datafusion`, `datafusion-substrait`
- `trino`: `adbc_driver_manager` (optional), `reqwest`
- `spark`: `tonic`, `prost` (spark-connect proto)

---

### 5.9 `semstrait-api`

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
    planner:   Arc<SemanticPlanner>,
    connector: Arc<dyn ComputeConnector>,
    manifest:  Arc<CompiledManifest>,
}

impl SemstraitEngine {
    pub async fn query(&self, req: ResolvedQueryRequest)
        -> Result<ComputeResult, EngineError>;
    pub async fn explain(&self, req: ResolvedQueryRequest)
        -> Result<ExplainResult, EngineError>;
    pub async fn validate(&self, req: ResolvedQueryRequest)
        -> Result<ValidationResult, EngineError>;
}

// gRPC transport — feature = "grpc"
#[cfg(feature = "grpc")]
pub mod grpc {
    pub struct SemstraitGrpcService { engine: Arc<SemstraitEngine> }
    // implements tonic generated service trait
    // Proto: SemstraitService { Query, Explain, Validate, Schema, Compile }
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
    pub fn run() -> Result<(), CliError>;
    // Commands: compile [--yaml] | explain [--json] | validate | query
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

### 5.10 `semstrait` (Facade)

**Role:** Single entry point for library consumers. Builder API. Feature flag coordination. Public API re-exports.

```rust
pub struct Semstrait {
    engine: SemstraitEngine,
}

pub struct SemstraitBuilder {
    manifest_source: Option<CompileSource>,
    catalog:         Option<Arc<dyn CatalogProvider>>,
    connector:       Option<Arc<dyn ComputeConnector>>,
    optimizer_passes: Vec<Box<dyn OptimizerPass>>,
}

impl SemstraitBuilder {
    pub fn new() -> Self;
    pub fn with_manifest_yaml(self, yaml: impl Into<String>) -> Self;
    pub fn with_manifest_file(self, path: impl Into<PathBuf>) -> Self;
    pub fn with_catalog(self, c: impl CatalogProvider + 'static) -> Self;
    pub fn with_connector(self, conn: impl ComputeConnector + 'static) -> Self;
    pub fn with_optimizer_pass(self, p: impl OptimizerPass + 'static) -> Self;
    pub async fn build(self) -> Result<Semstrait, BuildError>;
}

impl Semstrait {
    pub fn builder() -> SemstraitBuilder;
    pub async fn query(&self, req: QueryRequest) -> Result<ComputeResult, Error>;
    pub async fn explain(&self, req: QueryRequest) -> Result<ExplainResult, Error>;
    pub async fn validate(&self, req: QueryRequest) -> Result<ValidationResult, Error>;
    pub fn manifest(&self) -> Arc<CompiledManifest>;
}

// Usage example:
let sem = Semstrait::builder()
    .with_manifest_yaml(yaml_str)
    .with_connector(DuckDbConnector::new(":memory:"))
    .build()
    .await?;

let result = sem.query(QueryRequest {
    kind: "orders".into(),
    dimensions: vec!["region".into(), "order_date".into()],
    measures: vec!["revenue".into()],
    ..Default::default()
}).await?;
```

**Feature flags:**
```toml
[features]
default         = ["duckdb"]
duckdb          = ["semstrait-connectors/duckdb"]
datafusion      = ["semstrait-connectors/datafusion"]
trino           = ["semstrait-connectors/trino"]
spark           = ["semstrait-connectors/spark"]
catalog-iceberg = ["semstrait-catalog/iceberg"]
catalog-unity   = ["semstrait-catalog/unity"]
catalog-glue    = ["semstrait-catalog/glue"]
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
prost        = "0.13"
pbjson       = "0.7"

# Query IR
substrait    = { version = "0.58", features = ["serde"] }
arrow        = { version = "54", features = ["ipc"] }
arrow-schema = "54"

# SQL generation
sqlparser    = "0.53"

# Graph analysis (manifest compiler only)
petgraph     = "0.6"

# Async
tokio        = { version = "1", features = ["full"] }
async-trait  = "0.1"

# gRPC
tonic        = { version = "0.12", optional = true }

# Engine connectors (all optional)
duckdb                = { version = "1.1", optional = true }
datafusion            = { version = "44", optional = true }
datafusion-substrait  = { version = "44", optional = true }
adbc_driver_manager   = { version = "0.12", optional = true }
# spark-connect: tonic + prost with custom proto definitions

# Utilities
uuid         = { version = "1", features = ["v4"] }
indexmap     = "2"
thiserror    = "2"
tracing      = "0.1"
chrono       = { version = "0.4", features = ["serde"] }
glob         = "0.3"
sha2         = "0.10"
```

---

## 7. Connection Verification Summary

This table records every cross-crate type reference and confirms no cycle is introduced.

| Type | Defined in | Used in | Verification |
|---|---|---|---|
| `Schema`, `DataType`, `DslExpr` | `core` | All crates | ✓ all depend on core |
| `ConsumerProfile` | `core` | `planner`, `connectors` | ✓ breaks old cycle; both only depend on core |
| `GlobPattern` | `model` | `manifest` (expand_globs), `catalog` (list_tables) | ✓ model → core; no reverse dep |
| `SemanticModel`, `Kind`, `Dataset` | `model` | `manifest` only | ✓ model is input to manifest compiler |
| `CatalogProvider` trait | `catalog` | `manifest`, `planner` | ✓ catalog → core only; no dep on manifest or planner |
| `CompiledManifest`, `CompiledKind` | `manifest` | `planner`, `api`, `facade` | ✓ manifest → model, catalog; no dep on planner |
| `PlanNode`, `LogicalPlan` | `ir` | `planner`, `sql`, `connectors` | ✓ ir → core only; downstream use is one-way |
| `SubstraitSerializer` | `ir` | `connectors` (SubstraitEmitter) | ✓ ir → core; connectors → ir |
| `SemAnnotation` | `ir` | `planner` (sets annotations), `connectors` (reads for Explain) | ✓ annotation type defined in ir, set by planner, serialized by ir |
| `ResolvedQueryRequest` | `planner` | `planner` internally, `api` (calls planner) | ✓ lives in planner; api → planner |
| `KindPlanner` trait | `planner` | `planner` (registry) | ✓ internal to planner crate |
| `Optimizer`, `OptimizerPass` | `planner` | `planner` (internal), `facade` (configuration) | ✓ no external crate depends on these for execution |
| `SqlEmitter`, `SqlDialect` | `sql` | `connectors` (engine emitters use dialect) | ✓ sql → ir → core; connectors → sql |
| `ComputeEmitter`, `ComputeAdapter`, `ComputeConnector` | `connectors` | `api`, `facade` | ✓ connectors → core, ir, sql; api → connectors |
| `ComputeResult` | `connectors` | `api`, `facade` | ✓ same crate |
| `SemstraitEngine` | `api` | `facade` | ✓ api is only used by facade and binaries |
| `RequestParser` | `api` | `api` (internal to submodules) | ✓ |

**No cycles detected.** Dependency graph is a strict DAG with `semstrait-core` as the unique root with no outgoing workspace edges.

---

## 8. Open Items (v1 Scope)

| Item | Decision | Notes |
|---|---|---|
| `column_mapping: auto` | Deferred v1.1 | Map all physical columns from catalog |
| Spark Substrait support | Default to SQL emitter | Spark 3.4+ experimental |
| Multi-engine query fan-out | Deferred v2 | Single connector per `Semstrait` instance |
| `FileSystemRepository` | Deferred v2 | `InMemoryRepository` sufficient for v1 stateless operation |
| Cross-kind metric refs | Prohibited v1 (COMP_E006) | Multi-kind planning deferred |
| UNION DISTINCT | Deferred v1.1 | UNION ALL only |
| Many-to-many junction tables | Deferred v2 | Bridge as explicit dataset in joinset |
| Kind-level filter block | Deferred v1.1 | Applies to all queries against a kind regardless of dataset |
| Schema drift detection | Warn PLAN_W003 | Physical schema changed since compile |
| Row-level security user_attribute | Schema exists | Session parameter required; missing → PLAN_E005 |

---

## 9. Module Document Index

Each crate's `MODULE.md` is derived from the corresponding section in this document plus the per-module details listed below.

| Document | Primary source section | Additional content |
|---|---|---|
| `semstrait-core/MODULE.md` | §5.1 | Complete `DslExpr` variant catalog; `DataType` ↔ Arrow mapping table; error type hierarchy |
| `semstrait-model/MODULE.md` | §5.2 | Full YAML schema specification (v1.2); ref resolution algorithm; `GlobPattern` grammar |
| `semstrait-catalog/MODULE.md` | §5.3 | `CatalogProvider` contract; per-impl authentication flows; `NullCatalogProvider` behavior spec |
| `semstrait-manifest/MODULE.md` | §5.4 | Compilation pipeline step-by-step; `CompiledManifest` JSON schema; error codes (COMP_E*) |
| `semstrait-ir/MODULE.md` | §5.5 | Complete `PlanNode` variant specs; Substrait extension proto definition; ordinal invariant proofs |
| `semstrait-planner/MODULE.md` | §5.6 | Full evaluation order spec; `GrainsetPlanner` coverage algorithm; `UnionsetPlanner` branch pruning; `JoinsetPlanner` BFS; error codes (PLAN_E*, PLAN_W*) |
| `semstrait-sql/MODULE.md` | §5.7 | DslExpr → SQL lowering rules per dialect; dialect quirk registry; test matrix |
| `semstrait-connectors/MODULE.md` | §5.8 | Trait contracts; per-engine: wire protocol, auth methods, `ConsumerProfile` values, known limitations, payload support matrix |
| `semstrait-api/MODULE.md` | §5.9 | Proto definitions; `RequestParser` algorithm; per-transport setup guide |
| `semstrait/MODULE.md` | §5.10 | Builder usage examples; feature flag combinations; embedding guide |
