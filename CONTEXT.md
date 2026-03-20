# Semstrait Architecture Document
**Version:** 4.0 | **Status:** V1 Complete — authoritative reference for per-module documentation

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
| `semstrait-api` | `semstrait-core`, `semstrait-ir`, `semstrait-sql`, `semstrait-planner`, `semstrait-manifest`, `semstrait-connectors`, `semstrait-catalog` |
| `semstrait` (facade) | `semstrait-core`, `semstrait-ir`, `semstrait-sql`, `semstrait-planner`, `semstrait-manifest`, `semstrait-connectors`, `semstrait-catalog` |

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
│  Schema · DataType · Expr · ConsumerProfile · Grain · errors          │
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
                                  │  SqlEmitter.emit(plan)
                                  │    → SQL string (dialect-specific)
                                  │  SubstraitSerializer.to_substrait(plan)
                                  │    → substrait::proto::Plan (JSON)
                                  │          │
                                  │          ▼
                                  │  ComputeAdapter.adapt(payload)
                                  │    negotiate via ConsumerProfile
                                  │          │
                                  │          ▼
                                  │  ComputeConnector.execute(request)
                                  │    datafusion (v1) · duckdb (v1) · trino (planned) · spark (planned)
                                  │          │
                                  │          ▼
                                  │  ComputeResult → JSON rows (default) or Arrow
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
///
/// Note: There are TWO sets of constraint types in the workspace:
///   1. `semstrait_core::constraints` — uses Vec<String> with serde skip_serializing_if.
///      Defined in core for shared use, but currently unused by other crates.
///   2. `semstrait_model::types` — uses Option<Vec<String>> for YAML deserialization.
///      Re-exported via semstrait-manifest. Used by ConstraintEvaluator and CompiledManifest.
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

| Feature flag | Impl | Backend | Protocol | Status |
|---|---|---|---|---|
| `iceberg` | `IcebergRestCatalog` | Polaris, Gravitino | Iceberg REST spec | Implemented (v1) |
| `unity` | `UnityCatalog` | Databricks Unity Catalog | UC REST API | Planned (v2) |
| `glue` | `GlueCatalog` | AWS Glue | AWS SDK | Planned (v2) |
| `hive` | `HiveCatalog` | Hive Metastore | Thrift / HTTP | Planned (v2) |
| *(always)* | `NullCatalogProvider` | None | — | Implemented (v1) |

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
Step 8  compile_exprs      parse expression fields (Expr); reject raw SQL strings
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
    pub expr:        Expr,
    pub additivity:  Additivity,
    pub constraints: Option<MeasureConstraints>,  // field = `constraints:` in YAML
    pub filters:     Vec<MeasureFilter>,
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
    profile:   ConsumerProfile, // wired from connector; defaults to full capabilities
}

pub struct SemanticPlannerBuilder {
    catalog: Option<Arc<dyn CatalogProvider>>,
    passes:  Vec<Box<dyn OptimizerPass>>,
    profile: ConsumerProfile,
}
impl SemanticPlannerBuilder {
    pub fn new() -> Self;
    pub fn with_catalog(self, c: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_optimizer_pass(self, p: impl OptimizerPass + 'static) -> Self;
    pub fn with_profile(self, profile: ConsumerProfile) -> Self;
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
1. ConstraintEvaluator::check(request, manifest)   → Err if violated
2. domain filter → filter datasets by domain_hint (Cow<CompiledKind>)
3. KindPlannerRegistry::dispatch(kind_type)         → KindPlanner
4. KindPlanner::resolve(kind, request, ctx)         → PlanFragment
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
Domain filtering (step 2) implemented in v1.1-B.2 — filters datasets by `domain_hint` field.

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
    pub pending_filters: Vec<Expr>,     // filters not yet injected (unified Expr)
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
    pub session_variables: SessionVariables,  // runtime values (e.g. tenant_id)
}
```

**External deps:** `semstrait-core`, `semstrait-ir`, `semstrait-manifest`, `semstrait-catalog`

---

### 5.7 `semstrait-sql`

**Role:** Emit SQL strings from `LogicalPlan`. Pure, synchronous, no I/O. One dialect per `SqlEmitter` implementation.

**Design:** SQL is generated by walking the `PlanNode` tree and emitting SQL fragments via direct string building through the dialect. The base emitter (`AnsiSqlEmitter`) uses no Jinja templates and no sqlparser-rs AST intermediate — it produces SQL strings directly from `PlanNode` + `ExprSqlRenderer`. Both emitters use the unified `Expr` type from `semstrait-core::expr`. The optional `PolyglotEmitter` (§5.7 below) uses polyglot-sql's internal AST for dialect transpilation.

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
| `Coalesce(CoalesceExpr)` | `COALESCE(exprs)` |
| `NullIf(NullIfExpr)` | `NULLIF(expr, null_expr)` |
| `DateTrunc(DateTruncExpr { grain, expr })` | `DATE_TRUNC('grain', expr)` (typed Grain enum, dialect-specific) |

**Note:** `Guard` and `EntityRef` are resolved during planning (to `Case` and `Column` respectively) and never appear in SQL emission. Both emitters return `EmitError::UnsupportedExpr` if encountered. Aggregates use typed `Aggregation` enum dispatch — no string-based function name matching.

**Polyglot SQL transpilation (V2-A, feature-gated):**

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

**External deps:** `semstrait-core`, `semstrait-ir`, `polyglot-sql` (optional, feature-gated)

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
│   ├── trino.rs         #[cfg(feature = "trino")]  — REST v1/statement via reqwest
│   └── spark.rs         #[cfg(feature = "spark")]  — structural impl (execution deferred)
```

#### Core traits

```rust
/// Converts LogicalPlan to a compute-ready payload.
/// Note: in the current engine pipeline, SemstraitEngine calls SqlEmitter and
/// SubstraitSerializer directly. ComputeEmitter is available for connectors
/// that want to customize payload creation.
pub trait ComputeEmitter: Send + Sync {
    fn emit_sql(&self, sql: &str) -> Result<ComputePayload, EmitError>;
    fn emit_substrait(&self, plan_bytes: &[u8]) -> Result<ComputePayload, EmitError>;
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
    /// The SQL dialect preferred by this engine. Default: Ansi.
    fn preferred_dialect(&self) -> TargetDialect { TargetDialect::Ansi }
}

pub struct ComputeResult {
    pub complete:    bool,                   // false = partial result
    pub stats:       ExecutionStats,         // rows_returned, execution_time, bytes_scanned
    pub data:        ComputeResultData,
}

pub enum ComputeResultData {
    Empty,                                   // DDL, health check
    Json(Vec<serde_json::Value>),            // universal format (DataFusion default)
    Native(Box<dyn Any + Send + Sync>),      // engine-specific (Arrow batches)
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
  C API (bundled)  native SessionCtx   REST client    spark-connect
  embedded         in-process          reqwest         structural (deferred)
        │                │                  │              │
        └────────────────┴──────────────────┴──────────────┘
                         │
                         ▼
               ComputeResult (Arrow RecordBatches)
```

**Per-engine notes:**

| Engine | Payload support | Wire | Notes |
|---|---|---|---|
| DuckDB | `Sql` only | DuckDB C API (embedded, `bundled`) | `Connection` is `Send`/`!Sync` — wrapped in `Arc<Mutex<Connection>>` + `spawn_blocking`; `query_arrow` → Arrow 55 batches → JSON via `arrow::json::ArrayWriter`; CSV/Parquet via `read_csv_auto()`/`read_parquet()`; `preferred_dialect = DuckDb` |
| DataFusion | `Sql` only (v1) | In-process Rust | Returns `ComputeResultData::Json` via Arrow→JSON; timeout enforcement via `tokio::time::timeout` |
| Trino | `Sql` only | reqwest REST v1/statement (DL-032) | POST→poll nextUri→collect pages. Basic/JWT auth. JSON rows in `ComputeResultData::Json`. `preferred_dialect = Trino` |
| Spark | `Sql` only (structural) | uuid only (DL-033) | Builder pattern, full trait interface. `execute()` returns `NotImplemented`. gRPC client deferred pending spark-connect-rs fork. `preferred_dialect = Spark` |

**External deps:** `semstrait-core`, `semstrait-ir`, `semstrait-sql`, `arrow`, `async-trait`

Engine deps (feature-gated):
- `duckdb`: `duckdb` crate v1.3.x (>=1.3.0, <1.4.0 — pinned for arrow 55 per DL-031), aliased as `duckdb-engine` in Cargo.toml; `arrow` v55 (for `json` feature)
- `datafusion`: `datafusion` v52
- `trino`: `reqwest` v0.12 (DL-032) — REST v1/statement API with pagination
- `spark`: `uuid` v1 (DL-033) — structural impl only; spark-connect-rs deferred

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
    manifest:  Option<CompiledManifest>,
    planner:   SemanticPlanner,           // sync; not Arc-wrapped
    connector: Option<Arc<dyn ComputeConnector>>,
}

impl SemstraitEngine {
    pub fn new() -> Self;                               // no manifest, no connector
    pub fn with_manifest(manifest: CompiledManifest) -> Self;
    pub fn with_connector(
        manifest: CompiledManifest,
        connector: Arc<dyn ComputeConnector>,
    ) -> Self;                                          // extracts ConsumerProfile → planner
    pub async fn with_manifest_yaml(yaml: &str) -> Result<Self, EngineError>;
    pub fn set_connector(&mut self, connector: Arc<dyn ComputeConnector>);

    pub fn validate(&self, raw: &RawQueryRequest) -> ValidationResult;
    pub async fn explain(&self, raw: &RawQueryRequest)
        -> Result<ExplainResult, EngineError>;          // SQL + Substrait JSON
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

### 5.10 `semstrait` (Facade)

**Role:** Single entry point for library consumers. Builder API. Feature flag coordination. Public API re-exports.

```rust
pub struct SemstraitInstance {
    manifest_yaml: String,
    manifest: CompiledManifest,
    planner: SemanticPlanner,
    connector: Option<Arc<dyn ComputeConnector>>,
}

pub struct SemstraitBuilder {
    manifest_yaml: Option<String>,
    manifest_path: Option<PathBuf>,
    catalog:       Option<Arc<dyn CatalogProvider>>,
    connector:     Option<Arc<dyn ComputeConnector>>,
}

impl SemstraitBuilder {
    pub fn new() -> Self;
    pub fn with_manifest_yaml(self, yaml: impl Into<String>) -> Self;
    pub fn with_manifest_file(self, path: impl Into<PathBuf>) -> Self;
    pub fn with_catalog(self, catalog: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_connector(self, connector: Arc<dyn ComputeConnector>) -> Self;
    pub async fn build(self) -> Result<SemstraitInstance, BuildError>;
    // build() compiles manifest, wires catalog into ManifestCompiler,
    // extracts ConsumerProfile from connector into planner builder
}

impl SemstraitInstance {
    pub fn builder() -> SemstraitBuilder;
    pub fn manifest(&self) -> &CompiledManifest;
    pub fn explain(&self, req: &ResolvedQueryRequest) -> Result<String, String>; // sync SQL
    pub async fn query(&self, req: &ResolvedQueryRequest)
        -> Result<ComputeResult, BuildError>;  // requires connector
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
# catalog-unity and catalog-glue planned for v2
polyglot        = ["semstrait-sql/polyglot", "semstrait-connectors/polyglot"]
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
| `column_mapping: auto` | **Done** (DL-038) | Identity mapping expansion in step 4.5; `ColumnMapping` enum with `Auto`/`Explicit` |
| Domain filter step | **Done** (v1.1-B.2) | Planner step 3 filters datasets by `domain_hint` using `Cow<CompiledKind>` |
| Aggregation constraints | **Done** (v1.1-B.1) | `check_aggregation_constraints()` validates allowed/prohibited lists against `expr_source` |
| REST /schema and /compile endpoints | **Done** (v1.1-B.4) | GET /schema returns kind introspection; POST /compile accepts YAML, returns manifest JSON |
| gRPC transport | **Done** (V2-F.1, DL-034) | tonic 0.14 server with 4 RPCs (Explain, Validate, Query, Health). Proto at `crates/semstrait-api/proto/service.proto`. |
| Polyglot SQL transpilation | **Done** (V2-A, DL-030) | `PolyglotEmitter` transpiles ANSI SQL to 34+ dialects via `polyglot-sql`. Feature-gated behind `polyglot`. |
| DuckDB connector | **Done** (V2-B, DL-031) | `DuckDbConnector` — embedded DuckDB 1.3.2, `Arc<Mutex<Connection>>` + `spawn_blocking`, CSV/Parquet registration, CLI `query-duckdb` command |
| Trino connector | **Done** (V2-C, DL-032) | reqwest REST v1/statement with pagination, Basic/JWT auth, 10 tests |
| Spark connector | **Done** structural (V2-D, DL-033) | Full trait interface, builder pattern. `execute()` returns `NotImplemented`. gRPC client deferred. |
| Arrow Flight SQL | Deferred v2+ (DL-029) | Databricks-specific, not Spark/Trino |
| Spark Substrait support | Default to SQL emitter | Spark 3.4+ experimental |
| Multi-engine query fan-out | Deferred v2 | Single connector per `Semstrait` instance |
| `FileSystemRepository` | **Done** | JSON-backed persistent manifest storage with atomic write (tmp+rename) |
| Cross-kind metric refs | Prohibited v1 (COMP_E006) | Multi-kind planning deferred |
| UNION DISTINCT | **Done** (v1.1-B.6) | `UnionMode::All` (default) or `UnionMode::Distinct` on Unionset kind type |
| Many-to-many junction tables | Deferred v2 | Bridge as explicit dataset in joinset |
| Kind-level filter block | **Done** (v1.1-B.5) | `CompiledKind.filters` injected as FilterNodes before user filters |
| Schema drift detection | **Done** (DL-037) | `check_schema_drift()` on `SemstraitEngine`, `PlannerWarning::SchemaDrift`, schema snapshots in `CompiledDataset` |
| ComputeEmitter integration | **Closed** by-design (DL-023) | Engine uses `SqlEmitter` + `SubstraitSerializer` directly. `ComputeEmitter` is optional connector capability. |
| Unity/Glue/Hive catalogs | Unity **Done**, Glue/Hive deferred | `UnityCatalogProvider` implemented; Glue/Hive deferred (heavy deps) |
| Unified Expr migration | **Complete** (DL-020, Phases 1-6) | Single `Expr` type in `core::expr` used across entire pipeline. Old `core::DslExpr` and `ir::DslExpr` alias removed. |
| SafeDivide Substrait anchor | By design (DL-024) | SafeDivide maps to Divide in Substrait; null-guard is SQL-only |
| Glob namespace hardcoded | **Done** (v1.1-B.3) | `SemanticModel.namespace` field used; defaults to `"default"` |

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

---

## 10. Module Document Index

Each crate's `MODULE.md` is derived from the corresponding section in this document plus the per-module details listed below.

| Document | Primary source section | Additional content |
|---|---|---|
| `semstrait-core/MODULE.md` | §5.1 | Unified `Expr` variant catalog; `DataType` ↔ Arrow mapping table; error type hierarchy |
| `semstrait-model/MODULE.md` | §5.2 | Full YAML schema specification (v1.2); ref resolution algorithm; `GlobPattern` grammar |
| `semstrait-catalog/MODULE.md` | §5.3 | `CatalogProvider` contract; per-impl authentication flows; `NullCatalogProvider` behavior spec |
| `semstrait-manifest/MODULE.md` | §5.4 | Compilation pipeline step-by-step; `CompiledManifest` JSON schema; error codes (COMP_E*) |
| `semstrait-ir/MODULE.md` | §5.5 | Complete `PlanNode` variant specs; Substrait extension proto definition; ordinal invariant proofs |
| `semstrait-planner/MODULE.md` | §5.6 | Full evaluation order spec; `GrainsetPlanner` coverage algorithm; `UnionsetPlanner` branch pruning; `JoinsetPlanner` BFS; error codes (PLAN_E*, PLAN_W*) |
| `semstrait-sql/MODULE.md` | §5.7 | Expr → SQL lowering rules per dialect; dialect quirk registry; test matrix |
| `semstrait-connectors/MODULE.md` | §5.8 | Trait contracts; per-engine: wire protocol, auth methods, `ConsumerProfile` values, known limitations, payload support matrix. See also D6 in `crates/semstrait-connectors/docs/`. |
| `semstrait-api/MODULE.md` | §5.9 | Proto definitions; `RequestParser` algorithm; per-transport setup guide |
| `semstrait/MODULE.md` | §5.10 | Builder usage examples; feature flag combinations; embedding guide |
