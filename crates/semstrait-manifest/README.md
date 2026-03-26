# semstrait-manifest

Manifest compiler, compiled output types, acceleration structures, and repository layer for the semstrait semantic engine.

Compiles `SemanticModel` (parsed YAML) into a validated `CompiledManifest` via a multi-step pipeline. The compiled manifest is the single artifact consumed by the planner at query time — it contains all semantic definitions, physical bindings, and pre-computed indices needed for plan generation.

---

## Module Map

```
src/
  lib.rs                Public API, re-exports
  compiler.rs           ManifestCompiler orchestrator (load YAML, run pipeline, hash)
  steps.rs              All compilation steps: validation, graph building, expr parsing, emit
  compiled.rs           Output types: CompiledManifest, CompiledKind, CompiledDataset, etc.
  acceleration.rs       Planner-optimized types: DataKind, CoverageIndex, FieldIndex, GrainMap
  catalog_snapshot.rs   CatalogSnapshot, TableSnapshot, ResolvedColumn, IcebergMetadata
  repository.rs         Repository trait + InMemoryRepository, FileSystemRepository
  error.rs              CompileError, RepositoryError
```

---

## Compilation Pipeline

```
CompileSource (YAML string, files, or directory)
       |
  1. parse                serde_yaml -> SemanticModel
  2. resolve_refs         expand ref: entries to inline definitions
  3. resolve_sources      expand globs, fetch catalog metadata (async)
       |
  4. validate_structure   dataset uniqueness, kind nesting, joinset anchors
  4.5 expand_auto         ColumnMapping::Auto -> identity (metadata dims excluded)
  4.6 validate_temporal   kind/dataset temporal type equivalence
  4.7 validate_storage    paths/tables exclusivity, non-empty sources
  4.8 validate_metadata   path extraction requires storage.paths; partition requires partition_def
       |
  5. validate_mappings    column_mapping keys vs kind interface (union coverage)
  5.5 validate_grain      grain compatibility across kind datasets
       |
  6. build_metric_graph   petgraph DiGraph, cycle detection, depth <= 3
  7. build_rel_graph      relationship graph, joinset anchor inference
  8. compile_exprs        parse Expr DSL fields, reject raw SQL
       |
  9. emit                 serialize to CompiledManifest + build acceleration structures
       |
  CompiledManifest (JSON-serializable, planner-ready)
```

Steps 1-2 delegate to `semstrait-model`. Steps 3-9 are pure functions in `steps.rs`. The compiler also computes a SHA-256 source hash and records a `compiled_at` timestamp.

### Step 3: Source Resolution (async)

The only async step. Resolves physical storage bindings:

```
For each dataset with storage config:
  storage.paths with globs  -->  StorageProvider.expand_glob()  -->  Vec<PathBuf>
  storage.tables with globs -->  CatalogProvider.list_tables()  -->  Vec<TableRef>

  For each resolved path/table:
    fetch schema (columns, types) from catalog or storage provider
    capture location, format, Iceberg metadata (snapshot_id, partition spec)

  Output: SourceResolutionResult {
    resolved: HashMap<dataset_name, Vec<ResolvedSource>>,
    catalog_snapshot: Option<CatalogSnapshot>,
    warnings: Vec<CompileWarning>,
  }
```

Wildcard patterns require providers: paths need `StorageProvider`, tables need `CatalogProvider` (via `CatalogRegistry`). No glob survives into the compiled manifest.

### Step 8: Expression Compilation

Parses string expressions into typed `Expr` trees using the DSL parser:

- Measure expressions: `SUM(amount)`, `COUNT(DISTINCT id)`, or declarative `agg: sum` + horizontal expr
- Metric expressions: `clicks / impressions` (arithmetic over measure references)
- Filter expressions: `status = 'active'`, `amount > 100`
- Rejects raw SQL strings — only DSL constructs allowed

Model-level `DataType` (I64, F64, Date, String, ...) is converted to core `DataType` (Int64, Float64, Date32, Utf8, ...) via `map_data_type()` during emission.

### Step 9: Emit

Consumes the `SourceResolutionResult` by value (no clone) and builds:

1. **CompiledManifest** — `CompiledKind` (legacy), `CompiledDataset`, relationships
2. **DataKind** map — planner-optimized structures (primary planner API)
3. **FieldIndex** — global inverted index (field name -> provider datasets)
4. **RelationshipGraph** — BFS-ready graph with pre-computed shortest paths

---

## CompiledManifest Structure

```rust
pub struct CompiledManifest {
    pub version: u32,
    pub compiled_at: DateTime<Utc>,
    pub source_hash: String,

    // Model identity
    pub model_name: String,
    pub model_description: Option<String>,

    // Semantic entities
    pub datasets: IndexMap<String, CompiledDataset>,     // standalone datasets
    pub kinds: IndexMap<String, CompiledKind>,            // legacy (deprecated — planner uses data_kinds)
    pub data_kinds: IndexMap<String, DataKind>,           // planner-optimized kinds (primary API)

    // Relationships
    pub relationships: Vec<CompiledRelationship>,         // top-level join definitions
    pub relationship_graph: RelationshipGraph,            // pre-computed BFS graph

    // Search index
    pub field_index: FieldIndex,                          // field -> provider lookup

    // Physical metadata
    pub catalog_snapshot: Option<CatalogSnapshot>,        // catalog state at compile time

    // Diagnostics
    pub diagnostics: CompileDiagnostics,                  // warnings from compilation
}
```

### CompiledKind — Three Layers (Legacy)

> **Note:** `CompiledKind` is the v1 type retained for backward compatibility. The planner uses `DataKind` from `manifest.data_kinds` (see [Acceleration Structures](#acceleration-structures) below).

Each kind has three distinct layers:

```
Interface       what users query        dimensions, measures, metrics, keys
Strategy        how the plan works      kind_type: Grainset | Unionset | Joinset
Binding         where data lives        datasets[], column_mapping, resolved_sources
```

```rust
pub struct CompiledKind {
    pub name: String,

    // Interface
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    pub keys: Option<Keys>,

    // Strategy
    pub kind_type: CompiledKindType,    // Grainset | Unionset { mode } | Joinset { associativity }

    // Binding
    pub datasets: Vec<CompiledKindDataset>,
    pub relationships: Vec<CompiledRelationship>,
    pub domain: Option<Vec<String>>,
    pub filters: Vec<CompiledFilter>,
}
```

### Compiled Semantic Types

All compiled types use `semstrait_core::DataType` (Arrow-aligned) rather than strings:

```rust
pub struct CompiledDimension {
    pub name: String,
    pub description: Option<String>,
    pub data_type: DataType,         // Date32, Utf8, Int64, etc.
    pub dim_type: DimensionType,     // Categorical | Temporal | Metadata | Binary | Geo | Bucketed
}

pub struct CompiledMeasure {
    pub name: String,
    pub data_type: DataType,         // Float64, Int64, Decimal, etc.
    pub agg: Option<Aggregation>,    // declarative agg (sum, avg, count, min, max, count_distinct)
    pub expr: Expr,                  // parsed DSL expression tree
    pub expr_source: String,         // original string for debugging
    pub additivity: Option<AdditivityType>,
    pub constraints: Option<MeasureConstraints>,
    pub filters: Vec<CompiledFilter>,
}

pub struct CompiledMetric {
    pub name: String,
    pub data_type: DataType,
    pub agg: Option<Aggregation>,    // two-stage metric computation
    pub expr: Expr,                  // ratio/arithmetic expression over measures
    pub expr_source: String,
    pub depth: usize,                // depth in metric dependency graph (leaf = 0)
    pub filters: Vec<CompiledFilter>,
}
```

### CompiledKindDataset — Physical Binding

```rust
pub struct CompiledKindDataset {
    pub name: String,
    pub extras: KindDatasetExtras,              // column_mapping, temporal, storage, catalog
    pub resolved_sources: Vec<ResolvedSource>,  // expanded paths/tables with schema
}
```

`ResolvedSource` carries everything the planner needs for a physical scan:

```rust
pub struct ResolvedSource {
    pub reference: String,                      // original path or table pattern
    pub table_fqn: Option<String>,              // fully-qualified table name (namespace.table)
    pub location: Option<String>,               // physical URI (s3://, file://)
    pub format: Option<DataFormat>,             // Iceberg, Parquet, Csv
    pub catalog_alias: Option<String>,          // which catalog resolved this
    pub schema: Option<Vec<SchemaColumn>>,      // column names + types from catalog
    pub iceberg_metadata: Option<IcebergMetadata>,
}
```

---

## Search and Resolution Path

How a query request finds its way from a field name to a physical table scan:

```
QueryRequest { from: "orders", select: ["date", "revenue"] }
       |
  1. Entity Resolution (semstrait-api RequestParser)
       |
       +-- Check manifest.kinds["orders"]       --> CompiledKind (if found)
       +-- Check manifest.data_kinds["orders"]  --> DataKind (v2 fast path)
       +-- Check manifest.datasets["orders"]    --> CompiledDataset (standalone)
       |
  2. Kind Dispatch (semstrait-planner, via manifest.resolve() -> &DataKind)
       |
       +-- DataKind::Dataset    --> build_dataset_kind_plan (single-dataset fast path)
       +-- DataKind::Grainset   --> GrainsetPlanner::resolve()
       +-- DataKind::Unionset   --> UnionsetPlanner::resolve()
       +-- DataKind::Joinset    --> JoinsetPlanner::resolve()
       |
  3. Dataset Routing (within kind planner)
       |
       For each requested field:
         kind.dimensions["date"]   --> CompiledDimension { data_type: Date32, ... }
         kind.measures["revenue"]  --> CompiledMeasure { agg: Sum, expr: ..., ... }
         kind.metrics["ctr"]       --> CompiledMetric { expr: clicks/impressions, ... }
       |
       For each DatasetBinding:
         binding.column_mapping.physical["date"]    --> physical column name
         binding.column_mapping.physical["revenue"] --> physical column name
       |
       Coverage check: which datasets map which fields?
       |
  4. Physical Column Resolution
       |
       column_mapping values:
         ColumnMappingValue::Simple("phys_col")          --> direct column reference
         ColumnMappingValue::WithGrain { column, grain }  --> temporal with grain override
         ColumnMappingValue::Literal("search")            --> injected constant
       |
       Metadata dimensions (DimensionType::Metadata):
         PathExtraction { token: 0 }     --> extract from source path (0-indexed)
         PartitionExtraction { level: 1 } --> extract from Hive partition (1-indexed)
       |
  5. Source Selection
       |
       dataset.resolved_sources --> Vec<ResolvedSource>
         Single source  --> ScanNode { table_name, location, format }
         Multiple sources --> UnionNode { ScanNode, ScanNode, ... }
       |
  6. Type Resolution (planner reads from compiled types)
       |
       Dimensions:  kind.dimensions[name].data_type       (fallback: Utf8)
       Measures:    kind.measures[name].data_type          (fallback: Float64)
       Metrics:     kind.metrics[name].data_type           (fallback: Float64)
       Scan cols:   resolved_source.schema[col].data_type  (fallback: Utf8)
```

### Field-Based Resolution (ad-hoc joins)

When `FROM` is omitted, the `FieldIndex` resolves providers:

```
FieldIndex {
    providers: { "date" -> ["orders", "shipments"], "revenue" -> ["orders"] }
    all_dimensions: { "date", "region", "customer_name" }
    all_measures: { "revenue", "cost" }
    all_metrics: { "margin" }
}
       |
  For each requested field:
    field_index.providers["date"]    --> ["orders"]  (use first provider)
    field_index.providers["revenue"] --> ["orders"]  (same dataset = no join needed)
       |
  If multiple datasets needed:
    relationship_graph.shortest_path("orders", "customers") --> [rel_idx_0]
    manifest.relationships[rel_idx_0] --> CompiledRelationship { join_type, columns, ... }
```

### Metric Decomposition

Metrics are ratio/arithmetic expressions over measures. The planner decomposes them:

```
kind.metrics["ctr"].expr = Divide(EntityRef("clicks"), EntityRef("impressions"))
       |
  extract_metric_constituents() --> ["clicks", "impressions"]
       |
  For coverage checks: a dataset must map ALL constituents to contribute a metric
  For lowering: lower_metric_iface() decomposes into aggregates + post_agg_expr:
       |
       aggregates: [SUM(physical_clicks), SUM(physical_impressions)]
       post_agg:   CASE WHEN impressions = 0 THEN NULL ELSE clicks / impressions END
```

---

## Acceleration Structures

Built during `emit()` (step 9) for O(1) planner access. Stored in `manifest.data_kinds`.

### DataKind Enum

```rust
#[serde(tag = "kind_type")]
pub enum DataKind {
    Dataset(Box<DatasetKind>),       // single dataset, direct Scan → Agg → Project
    Unionset(Box<UnionsetKind>),     // UNION ALL across multiple datasets
    Grainset(Box<GrainsetKind>),     // grain-based covering dataset selection
    Joinset(Box<JoinsetKind>),       // BFS join chain from anchor dataset
}
```

All variants embed `KindInterface` via composition. `DataKind` implements `SemanticInterface` for uniform access to dimensions, measures, metrics.

### KindInterface (shared semantic fields)

```rust
pub struct KindInterface {
    pub name: String,
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    pub keys: Option<Keys>,
    pub filters: Vec<CompiledFilter>,
    pub domain: Option<Vec<String>>,
    pub temporal_dim: Option<String>,
}
```

Type resolution methods (`resolve_dim_type`, `resolve_measure_type`, `find_temporal_dimension`) live on `KindInterface` — one implementation, no duplication.

### DatasetKind (single-dataset fast path)

```rust
pub struct DatasetKind {
    pub interface: KindInterface,
    pub binding: DatasetBinding,        // single binding
}
```

### GrainsetKind

```rust
pub struct GrainsetKind {
    pub interface: KindInterface,
    pub bindings: Vec<DatasetBinding>,
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    pub metric_order: Option<MetricOrder>,
    pub grain_map: Option<GrainMap>,    // temporal routing
}
```

### UnionsetKind

```rust
pub struct UnionsetKind {
    pub interface: KindInterface,
    pub mode: UnionMode,                // All | Distinct
    pub bindings: Vec<DatasetBinding>,
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    pub metric_order: Option<MetricOrder>,
}
```

### JoinsetKind

```rust
pub struct JoinsetKind {
    pub interface: KindInterface,
    pub associativity: JoinAssociativity,
    pub bindings: Vec<DatasetBinding>,
    pub relationships: Vec<CompiledRelationship>,
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    pub metric_order: Option<MetricOrder>,
    pub adjacency_index: AdjacencyIndex,
}
```

### DatasetBinding (per-dataset physical mapping)

```rust
pub struct DatasetBinding {
    pub dataset_name: String,
    pub column_mapping: ResolvedColumnMapping,
    pub resolved_sources: Vec<ResolvedSource>,
}
```

Multi-dataset kinds (`Grainset`, `Unionset`, `Joinset`) implement `MultiDatasetKind` trait for shared access to `bindings()`, `coverage_index()`, `dimension_index()`, `metric_order()`.

### CoverageIndex (bitmap-based field coverage)

```rust
pub struct CoverageIndex {
    pub field_names: Vec<String>,                    // ordered dim + measure names
    pub field_positions: HashMap<String, usize>,     // name -> bit position
    pub dataset_bitmaps: Vec<FixedBitSet>,           // one bitmap per dataset
}
```

For query `[date, revenue]`: build field mask, bitwise-AND with each dataset bitmap to find covering datasets in O(1).

### GrainMap (grainset-specific)

```rust
pub struct GrainMap {
    pub temporal_dim: String,
    pub groups: Vec<(TemporalGrain, Vec<usize>)>,   // grain -> dataset indices, coarsest first
    pub dataset_grains: Vec<Option<TemporalGrain>>,  // native grain per dataset
}
```

Enables grain-aware dataset selection: prefer coarser datasets (cheaper) when query grain allows.

### DimensionIndex (pre-classified buckets)

```rust
pub struct DimensionIndex {
    pub temporal: Vec<String>,
    pub metadata: Vec<(String, MetadataDimension)>,
    pub categorical: Vec<String>,
    pub literals_by_dataset: Vec<HashMap<String, String>>,
}
```

### ResolvedColumnMapping (pre-split for planner)

```rust
pub struct ResolvedColumnMapping {
    pub physical: IndexMap<String, String>,                  // semantic -> physical column
    pub literals: HashMap<String, String>,                   // semantic -> literal value
    pub temporal: HashMap<String, TemporalMapping>,          // semantic -> {column, grain}
    pub anchored: HashMap<String, HashMap<String, String>>,  // composed mappings
}
```

Pre-split at compile time so the planner never parses `ColumnMappingValue` variants at query time.

### MetricOrder (topological sort)

```rust
pub struct MetricOrder {
    pub evaluation_order: Vec<String>,                   // metrics sorted leaves-first
    pub metric_measures: HashMap<String, Vec<String>>,   // metric -> leaf measures
}
```

### AdjacencyIndex (joinset-specific)

```rust
pub struct AdjacencyIndex {
    pub forward: Vec<Vec<(usize, usize)>>,     // dataset -> [(neighbor, rel_idx)]
    pub reverse: Vec<Vec<(usize, usize)>>,
    pub dataset_index: HashMap<String, usize>,
}
```

---

## Repository

Storage abstraction for compiled manifests:

```rust
pub trait Repository: Send + Sync {
    fn load(&self) -> Result<Arc<CompiledManifest>, RepositoryError>;
    fn save(&self, manifest: &CompiledManifest) -> Result<(), RepositoryError>;
}
```

| Implementation | Storage | Notes |
|----------------|---------|-------|
| `InMemoryRepository` | `Arc<RwLock<Option<Arc<CompiledManifest>>>>` | Thread-safe, for testing |
| `FileSystemRepository` | JSON file on disk | Atomic write (tmp + rename), creates parent dirs |

---

## Dependencies

| Crate | Role |
|-------|------|
| `semstrait-core` | `DataType`, `Expr`, `Aggregation`, `EngineProfile` |
| `semstrait-model` | `SemanticModel`, `parse()`, `resolve_refs()`, dimension/measure types |
| `semstrait-catalog` | `CatalogProvider`, `StorageProvider`, `CatalogRegistry` |
| `chrono` | `compiled_at` timestamp |
| `sha2` | source hash for cache invalidation |
| `petgraph` | metric dependency graph, relationship graph analysis |
| `indexmap` | insertion-order-preserving maps for deterministic output |
| `fixedbitset` | bitmap coverage index |
