# Manifest Compilation & Resolved Structure

**Version:** 2.0 | **Status:** Design
**Scope:** Manifest compiler, Catalog integration, Planner optimization

---

## 1. Problem Statement

### 1.1 Current State

The manifest compiler validates YAML structure and binds types, but **defers most resolution to plan time**:

| What | Current | Should Be |
|------|---------|-----------|
| Physical table locations | `resolved_sources: Vec<String>` -- raw strings from YAML | Fully qualified catalog references with metadata |
| Column mapping | `HashMap<String, ColumnMappingValue>` -- raw key-value | Pre-split by purpose: physical, literal, temporal |
| Schema metadata | `compiled_schema: Option<Vec<SchemaColumn>>` -- best-effort snapshot | Authoritative, type-validated column registry |
| Temporal grains | Declared in YAML, validated structurally | Mapped to partition specs from catalog |
| Dataset coverage | Union coverage check (name exists in >=1 mapping) | Pre-computed coverage bitmaps per dataset |
| Join relationships | Passed through as-is | Pre-built adjacency list with resolved column refs |
| Metric dependencies | Depth computed, cycle-checked | Pre-resolved dependency order with constituent tracking |
| Dimension classification | Enum on each dimension, pattern-matched at plan time | Pre-partitioned into typed buckets |
| Kind abstraction | Generic `CompiledKind` with `CompiledKindType` enum | Type-specific `DataKind` hierarchy with strategy payload |

### 1.2 Vision

After compilation, a manifest must be a **fully resolved, planner-optimized artifact**:

1. **No wildcards or placeholders** -- all globs expanded, all refs resolved, all `Auto`/`Inherited` mappings materialized
2. **All physical references resolved** -- table locations, column types, partition specs from catalog
3. **Pre-computed acceleration structures** -- coverage bitmaps, grain maps, adjacency lists, dimension buckets
4. **Snapshot-pinned** -- catalog metadata captured at a specific point in time for reproducibility

### 1.3 Design Principle: Logical Graph, Physical Indices

The manifest is **conceptually a semantic graph** -- datasets, dimensions, measures, metrics, and relationships form a DAG of nodes and edges. But the **runtime representation uses purpose-built indices** for each planner operation, not a generic graph library.

**Why not petgraph for the manifest?**
- Planner hot paths are **lookup-dominated**, not graph-traversal-dominated
- `HashMap`/`FixedBitSet` lookups are O(1); petgraph `NodeIndex` adds indirection
- petgraph doesn't implement `Serialize` by default -- awkward for JSON persistence
- Purpose-built structures (coverage bitmaps, adjacency lists) are 10-100x faster than generic graph queries

**Where petgraph stays**: Metric dependency graph (already uses `DiGraph` for cycle detection and topological sort during compilation). Not used at query time.

### 1.4 Inspiration

| Project | Key Pattern | Applicable To Semstrait |
|---------|-------------|------------------------|
| **dbt manifest.json** | Every node fully compiled with `compiled_code`, `depends_on`, `columns` resolved. `parent_map`/`child_map` for DAG traversal. Semantic manifest includes pre-computed `entity_links`: measures -> entities -> data sources | Pre-resolve all expressions, build entity-to-dataset routing |
| **MetricFlow** | `DataSourceSemantics` with resolved measures/dimensions per data source. `LinkableSpecSet` for pre-computed coverage. `semantic_model_by_measure_lookup` for O(1) measure -> dataset routing | Pre-compute coverage bitmaps, measure-to-dataset index |
| **Cube.js** | Join graph built at compile time. Dijkstra for shortest join path. Cached join trees. `sql()` references resolved to physical tables | Pre-build join adjacency lists, cache BFS results |
| **Iceberg** | Table metadata includes partition specs (transforms mapped to grains), schema evolution history, snapshot isolation | Map partition specs -> temporal grains, embed snapshot IDs |

---

## 2. DataKind Hierarchy

### 2.1 Replacing `CompiledKind`

`CompiledKind` was a generic container with a `CompiledKindType` enum discriminant. It has been replaced by a **`DataKind` 4-variant enum** where each variant is a dedicated struct with embedded acceleration indices:

```
DataKind (enum, serde tag = "kind_type")
|-- Dataset(Box<DatasetKind>)      -- single dataset, direct Scan → Agg → Project
|-- Grainset(Box<GrainsetKind>)    -- grain-based covering dataset selection
|-- Unionset(Box<UnionsetKind>)    -- UNION ALL across multiple datasets
|-- Joinset(Box<JoinsetKind>)      -- BFS join chain from anchor dataset
```

All variants embed `KindInterface` via composition (shared semantic fields: dimensions, measures, metrics, keys, filters, domain, temporal_dim). Multi-dataset variants also embed acceleration structures (CoverageIndex, DimensionIndex, etc.).

### 2.2 `DataKind` Enum

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind_type", rename_all = "snake_case")]
pub enum DataKind {
    Dataset(Box<DatasetKind>),
    Unionset(Box<UnionsetKind>),
    Grainset(Box<GrainsetKind>),
    Joinset(Box<JoinsetKind>),
}
```

### 2.3 `SemanticInterface` Trait

Shared interface across all queryable entities. Default methods delegate to `KindInterface`.

```rust
pub trait SemanticInterface {
    fn interface(&self) -> &KindInterface;
    fn dimensions(&self) -> &IndexMap<String, CompiledDimension> { .. }
    fn measures(&self) -> &IndexMap<String, CompiledMeasure> { .. }
    fn metrics(&self) -> &IndexMap<String, CompiledMetric> { .. }
    fn filters(&self) -> &[CompiledFilter] { .. }
    fn keys(&self) -> Option<&Keys> { .. }
    fn domain(&self) -> Option<&[String]> { .. }
    fn temporal_dimension(&self) -> Option<&str> { .. }
}
```

`DataKind` and all variant structs implement `SemanticInterface`. Multi-dataset variants also implement `MultiDatasetKind` (access to `bindings()`, `coverage_index()`, `dimension_index()`, `metric_order()`).

### 2.4 `CommonDataset` -- Single-Dataset Fast Path

For direct dataset queries. No routing, no union/join. Maps directly to one physical source.

```rust
pub struct CommonDataset {
    pub name: String,
    pub description: Option<String>,

    // Semantic interface
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    pub keys: Option<Keys>,
    pub filters: Vec<CompiledFilter>,
    pub domain: Option<Vec<String>>,

    // Physical binding (single dataset)
    pub dataset_ref: String,
    pub column_mapping: ResolvedColumnMapping,
    pub temporal_dimension: Option<String>,
    pub resolved_sources: Vec<ResolvedSource>,
}
```

**When is a CommonDataset created?**
- Explicit single-dataset grainsets/unionsets/joinsets (only 1 dataset in the list)
- Implicit kind wrapping when a user queries a dataset by name (`manifest.resolve_entity("orders")`)

### 2.5 `ComplexDataKind` -- Multi-Dataset Composition

```rust
pub struct ComplexDataKind {
    pub name: String,
    pub description: Option<String>,

    // Semantic interface
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    pub keys: Option<Keys>,
    pub filters: Vec<CompiledFilter>,
    pub domain: Option<Vec<String>>,

    // Strategy -- how datasets are composed
    pub strategy: KindStrategy,

    // Binding -- dataset implementations
    pub dataset_bindings: Vec<DatasetBinding>,
    pub relationships: Vec<CompiledRelationship>,

    // Acceleration structures (pre-computed at compile time)
    pub temporal_dimension: Option<String>,
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    pub metric_order: Option<MetricOrder>,
    pub grain_map: Option<GrainMap>,          // grainset only
    pub adjacency_index: Option<AdjacencyIndex>, // joinset only
}
```

### 2.6 `KindStrategy` Enum

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KindStrategy {
    Grainset,
    Unionset { mode: UnionMode },
    Joinset { associativity: JoinAssociativity },
}
```

### 2.7 Entity Resolution

The planner entry point. Checks data kinds first, then auto-wraps datasets.

```rust
impl CompiledManifest {
    pub fn resolve_entity(&self, name: &str) -> Option<&DataKind> {
        // Data kinds take precedence.
        if let Some(dk) = self.data_kinds.get(name) {
            return Some(dk);
        }
        // Auto-wrap dataset as CommonDataset handled
        // by pre-synthesis during compilation (see Section 5.1, step 9b).
        None
    }
}
```

---

## 3. Resolved Column Mapping

### 3.1 Problem

Column mapping is `HashMap<String, ColumnMappingValue>` -- a flat dictionary that the planner queries via `.get()` 100-500+ times per query. The `ColumnMappingValue` enum requires pattern matching at each access:

```rust
// Current: 2-step overhead per lookup
let mv = mapping.get(dim_name)?;              // HashMap lookup
match mv {                                     // Pattern match dispatch
    Simple(s) => s,
    WithGrain { column, .. } => column,
    Literal(_) => /* special handling */,
    Anchored(_) => unreachable!(),
}
```

### 3.2 `ResolvedColumnMapping` -- Pre-Split by Purpose

After compilation, the column mapping is split into purpose-specific maps that eliminate runtime branching:

```rust
/// Pre-resolved column mapping split by target type.
pub struct ResolvedColumnMapping {
    /// Physical column mappings: semantic_name -> physical_column_name.
    /// The hot-path lookup used by expr_lower. No pattern matching needed.
    pub physical: IndexMap<String, String>,

    /// Literal value injections: semantic_name -> literal_value.
    /// Used for constant dimensions (e.g., source='web_events').
    pub literals: HashMap<String, String>,

    /// Temporal dimension mappings: semantic_name -> TemporalMapping.
    /// Separated because temporal dims may have grain overrides.
    pub temporal: HashMap<String, TemporalMapping>,
}

pub struct TemporalMapping {
    pub physical_column: String,
    pub grain: Option<TemporalGrain>,
}
```

**Planner impact**: `expr_lower` only needs `physical: IndexMap<String, String>` -- a flat `&str -> &str` map. The `resolve_column_name()` function becomes a single `.get()` with no branching. Estimated 40-60% reduction in hot-path overhead.

### 3.3 `DatasetBinding` -- Per-Dataset Binding in Complex Kinds

Replaces `CompiledKindDataset`:

```rust
pub struct DatasetBinding {
    /// Dataset name (reference into manifest.datasets).
    pub dataset_name: String,

    /// Resolved column mapping (pre-split for fast planner access).
    pub column_mapping: ResolvedColumnMapping,

    /// Resolved physical sources (table/path URIs).
    pub resolved_sources: Vec<ResolvedSource>,

    /// Temporal configuration override for this binding.
    pub temporal_config: Option<TemporalConfig>,

    /// Reference to table snapshot in CatalogSnapshot.
    pub catalog_table: Option<String>,
}

pub struct ResolvedSource {
    pub reference: String,
    pub source_type: SourceType,
}

pub enum SourceType {
    Path,   // File path (S3, GCS, local)
    Table,  // Catalog-managed table name
}
```

---

## 4. Acceleration Structures

All acceleration structures are **serialized to JSON** alongside the manifest. They are pre-computed during compilation and consumed as read-only by the planner.

### 4.1 CoverageIndex -- Per-Dataset Coverage Bitmaps

The core data structure for dataset selection in grainset/joinset/unionset. Replaces O(datasets x names) `HashMap::contains_key()` iteration with O(datasets) bitwise AND operations.

```rust
pub struct CoverageIndex {
    /// Ordered list of all mappable field names (dimensions + measures).
    /// Position in this Vec = bit position in the bitmaps.
    /// Metrics excluded (derived, not used for coverage routing).
    pub field_names: Vec<String>,

    /// Reverse lookup: field_name -> bit position.
    pub field_positions: HashMap<String, usize>,

    /// Coverage bitmap per dataset binding. Index corresponds to
    /// ComplexDataKind.dataset_bindings order.
    pub dataset_bitmaps: Vec<FixedBitSet>,
}
```

**Planner usage**:
```rust
// Build a request mask (once per query)
let requested = coverage.build_field_mask(&["date", "region", "revenue"]);

// Check each dataset in O(bitwise_and) -- typically 1-2 CPU instructions
for (idx, bitmap) in coverage.dataset_bitmaps.iter().enumerate() {
    if requested.is_subset(bitmap) {
        // dataset covers all requested fields
    }
}
```

**Performance**: For 100 datasets x 50 fields: ~5000 HashMap lookups -> ~150 bitmap operations (~33x speedup).

### 4.2 DimensionIndex -- Pre-Classified Dimension Buckets

Eliminates per-query `iter().find_map()` and `match dim.dim_type` pattern matching.

```rust
pub struct DimensionIndex {
    /// Temporal dimension names (usually 0 or 1).
    pub temporal: Vec<String>,
    /// Metadata dimensions: (name, MetadataDimension config).
    pub metadata: Vec<(String, MetadataDimension)>,
    /// Categorical dimension names.
    pub categorical: Vec<String>,
    /// Literal dimensions per dataset: dataset_idx -> (dim_name -> literal_value).
    pub literals_by_dataset: Vec<HashMap<String, String>>,
}
```

**Planner impact**: `find_temporal_dimension()` becomes `dimension_index.temporal.first()` -- O(1). `partition_dimensions()` (classify Physical/MetadataLiteral/NullFill) becomes a HashSet membership check.

### 4.3 GrainMap -- Grainset-Specific

Pre-groups datasets by temporal grain. The key structure for grainset resolution.

```rust
pub struct GrainMap {
    /// Temporal dimension name this map applies to.
    pub temporal_dim: String,
    /// Groups of datasets by native grain, sorted coarsest -> finest.
    /// Each entry: (grain, dataset_binding indices).
    pub groups: Vec<(TemporalGrain, Vec<usize>)>,
    /// Per-dataset native grain: dataset_binding index -> grain.
    pub dataset_grains: Vec<Option<TemporalGrain>>,
}
```

**Planner impact**: `assign_to_grain_groups()` no longer computes grain groups at query time. Pre-built and sorted.

### 4.4 AdjacencyIndex -- Joinset-Specific

Pre-built adjacency lists for BFS join traversal. Eliminates O(relationships) linear scan per BFS step.

```rust
pub struct AdjacencyIndex {
    /// Forward edges: dataset_binding_idx -> [(neighbor_idx, relationship_idx)].
    pub forward: Vec<Vec<(usize, usize)>>,
    /// Reverse edges: dataset_binding_idx -> [(neighbor_idx, relationship_idx)].
    pub reverse: Vec<Vec<(usize, usize)>>,
    /// Dataset name -> dataset_binding index.
    pub dataset_index: HashMap<String, usize>,
}
```

**Planner impact**: BFS becomes O(edges) instead of O(nodes x edges). For a 10-dataset joinset, ~10x speedup.

### 4.5 MetricOrder -- Topological Sort + Constituent Measures

```rust
pub struct MetricOrder {
    /// Metric names in topological order (leaves to roots).
    pub evaluation_order: Vec<String>,
    /// Constituent measures for each metric: metric_name -> measure_names.
    pub metric_measures: HashMap<String, Vec<String>>,
}
```

**Planner impact**: Grainset metric assignment no longer parses metric expressions to find constituent measures -- pre-extracted.

---

## 5. Compilation Pipeline

### 5.1 Full Pipeline with Catalog Resolution and Acceleration

```
YAML Input
     |
[1]  Parse (serde_yaml -> SemanticModel)
[2]  Resolve refs (expand ref: entries)
[2.5] Build catalog provider (from model.catalog config)
[3]  Expand globs (CatalogProvider.list_tables)
[4]  Validate structure
[4.6] Validate temporal equivalence
[4.7] Validate storage
[4.8] Validate metadata dimensions
[4.5] Expand auto mappings
[5]  Validate mappings (union coverage check)
[5b] Validate grain compatibility
[6]  Build metric graph (petgraph DiGraph, cycle detection, topo sort)
[7]  Build relationship graph
[8]  Compile expressions (parse DSL -> Expr trees)
     |
     v
--- Catalog Resolution (best-effort, skipped when no catalog) ---
     |
[10] Resolve table references
     |  CatalogProvider.table_exists() per dataset source
     |  Capture table location, properties, format version
     |
[11] Resolve column schemas
     |  CatalogProvider.get_schema() per resolved table
     |  Validate column_mapping keys against physical schema
     |  Infer physical types for ResolvedColumnMapping entries
     |
[12] Resolve partition specs
     |  Extract partition-specs from Iceberg table metadata
     |  Map partition transforms -> TemporalGrain
     |  Auto-infer native_grain when not declared in YAML
     |
[13] Pin catalog snapshot
     |  Capture current-snapshot-id per table
     |  Assemble CatalogSnapshot
     |
     v
--- Acceleration Structure Building ---
     |
[14] Build resolved column mappings
     |  Split ColumnMapping -> ResolvedColumnMapping per dataset binding
     |  Populate physical, literals, temporal maps
     |
[15] Build dimension index per data kind
     |  Classify dimensions -> temporal/metadata/categorical/literal
     |
[16] Build coverage index per data kind
     |  Assign bit positions to field names
     |  Build per-dataset FixedBitSet bitmaps
     |
[17] Build grain map (grainset kinds only)
     |  Group datasets by native temporal grain
     |  Sort groups by coarseness
     |
[18] Build adjacency index (joinset kinds only)
     |  Populate forward/reverse edge lists from relationships
     |
[19] Resolve metric dependencies
     |  Extract constituent measures per metric (walk Expr tree)
     |  Build evaluation order from topological sort
     |
     v
--- Global Graph Structures (for ad-hoc join resolution) ---
     |
[20] Build relationship graph with shortest paths
     |  Populate forward/reverse adjacency from relationships
     |  BFS from every dataset node -> pre-compute shortest_paths
     |  O(N x (N + E)) where N=datasets, E=relationships
     |
[21] Build global field index
     |  Iterate all datasets: field_name -> provider datasets
     |  Collect all_dimensions, all_measures, all_metrics sets
     |
     v
--- Emit ---
     |
[9]  Emit CompiledManifest
     |  Assemble DataKind hierarchy (CommonDataset vs ComplexDataKind)
     |  Attach all acceleration structures
     |  Attach RelationshipGraph and FieldIndex
     |
[9b] Synthesize implicit CommonDatasets
     |  For each standalone dataset, pre-build a CommonDataset
     |  (avoids runtime allocation in resolve_entity)
```

### 5.2 Graceful Degradation

Catalog resolution is **best-effort**:

| Catalog Available | Behavior |
|-------------------|----------|
| Yes | Full resolution: types validated, partitions mapped, snapshot pinned |
| No (no catalog config) | Skip steps 10-13. physical_type = None. native_grain from YAML only |
| Error (catalog unreachable) | Warning, fall back to no-catalog. Compilation continues |

Acceleration structures (steps 14-19) always run -- they don't depend on catalog.

---

## 6. Top-Level Manifest Structure

```rust
pub struct CompiledManifest {
    pub version: u32,                          // Schema version 2
    pub compiled_at: DateTime<Utc>,
    pub source_hash: String,                   // SHA-256 of YAML input

    pub model_name: String,
    pub model_description: Option<String>,

    /// Physical datasets (schema definitions).
    pub datasets: IndexMap<String, CompiledDataset>,

    /// Queryable semantic entities (replaces `kinds`).
    pub data_kinds: IndexMap<String, DataKind>,

    /// Global relationship definitions.
    pub relationships: Vec<CompiledRelationship>,

    /// Global relationship graph -- adjacency + shortest paths.
    pub relationship_graph: RelationshipGraph,

    /// Global field index -- inverted index for ad-hoc join resolution.
    pub field_index: FieldIndex,

    /// Catalog state captured at compile time.
    pub catalog_snapshot: Option<CatalogSnapshot>,

    /// Compilation diagnostics (warnings, info).
    pub diagnostics: CompileDiagnostics,
}
```

### 6.1 `RelationshipGraph`

Global relationship adjacency with pre-computed shortest paths between all reachable dataset pairs. Enables ad-hoc join resolution when `from` is omitted.

```rust
/// Global relationship graph for ad-hoc join resolution.
pub struct RelationshipGraph {
    /// Forward edges: from_dataset -> [(to_dataset, relationship_idx)].
    pub forward: HashMap<String, Vec<(String, usize)>>,
    /// Reverse edges: to_dataset -> [(from_dataset, relationship_idx)].
    pub reverse: HashMap<String, Vec<(String, usize)>>,

    /// Pre-computed shortest paths between all reachable dataset pairs.
    /// Key = (from_dataset, to_dataset), Value = ordered list of relationship
    /// indices forming the shortest path.
    /// Computed via BFS from every dataset at compile time.
    pub shortest_paths: HashMap<(String, String), Vec<usize>>,

    /// Dataset name -> index (for bitmap operations).
    pub dataset_index: HashMap<String, usize>,
}
```

**Compilation**: Built in step 20 (see Section 5.1). BFS from every dataset node, recording shortest path to every reachable dataset. For N datasets with E relationships, complexity is O(N x (N + E)) -- acceptable for typical models (<100 datasets).

### 6.2 `FieldIndex`

Global inverted index mapping every semantic field name to the dataset(s) that provide it. The core structure for ad-hoc join resolution when `from` is omitted.

```rust
/// Global inverted index: field_name -> provider datasets.
/// Built from all datasets' dimensions and measures across the manifest.
pub struct FieldIndex {
    /// field_name -> Vec<dataset_name> that provide this field.
    /// A field may be provided by multiple datasets (e.g., "date" exists
    /// in both orders and customers).
    pub providers: HashMap<String, Vec<String>>,

    /// Dimension names (across all datasets). For field classification
    /// when `from` is omitted and the planner must classify select names.
    pub all_dimensions: HashSet<String>,

    /// Measure names (across all datasets).
    pub all_measures: HashSet<String>,

    /// Metric names (across all data kinds).
    pub all_metrics: HashSet<String>,
}
```

**Compilation**: Built in step 21 (see Section 5.1). Iterates all datasets and data kinds, populating the inverted index.

---

## 7. Catalog Snapshot

Captures catalog state at compilation time for reproducibility and drift detection.

### 7.1 `CatalogSnapshot`

```rust
pub struct CatalogSnapshot {
    /// Per-table metadata captured from the catalog.
    pub tables: HashMap<String, TableSnapshot>,
    /// Timestamp when catalog metadata was fetched.
    pub captured_at: DateTime<Utc>,
}
```

### 7.2 `TableSnapshot`

```rust
pub struct TableSnapshot {
    /// Fully qualified table reference (catalog.namespace.table).
    pub fqn: String,
    /// Column schema at compile time.
    pub columns: Vec<ResolvedColumn>,
    /// Iceberg-specific metadata (if applicable).
    pub iceberg: Option<IcebergMetadata>,
}

pub struct ResolvedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub comment: Option<String>,
    pub field_id: Option<i32>,   // Iceberg field ID
}

pub struct IcebergMetadata {
    pub snapshot_id: i64,
    pub partition_spec: Vec<PartitionField>,
    pub format_version: Option<u32>,
    pub location: Option<String>,
    pub properties: HashMap<String, String>,
}

pub struct PartitionField {
    pub source_column: String,
    pub transform: PartitionTransform,
    pub name: String,
    pub inferred_grain: Option<TemporalGrain>,
}

pub enum PartitionTransform {
    Identity,
    Year,       // -> TemporalGrain::Year
    Month,      // -> TemporalGrain::Month
    Day,        // -> TemporalGrain::Day
    Hour,       // -> TemporalGrain::Hour
    Bucket(u32),
    Truncate(u32),
}
```

---

## 8. Type System Integration

### 8.1 Type Inference Chain

```
YAML data_type (user-facing)     e.g., "f64", "string", "date"
         |
         v
Model DataType (semstrait-model) e.g., DataType::F64
         |
         v
[Catalog physical type]          e.g., DataType::Float64 (from Iceberg "double")
         |
         v
Core DataType (semstrait-core)   e.g., DataType::Float64 (Arrow-aligned)
```

### 8.2 Type Validation at Compile Time

With catalog schema available, the compiler validates type compatibility:

| Semantic Type | Physical Type | Result |
|---------------|---------------|--------|
| f64 | Float64 / Double | OK |
| f64 | Int64 / Long | Warning: implicit widening |
| string | Utf8 / String | OK |
| date | Date32 / date | OK |
| date | TimestampMicrosecond | Warning: temporal precision mismatch |
| f64 | Utf8 / String | Error: incompatible types |

### 8.3 Diagnostics

```rust
pub struct CompileDiagnostics {
    pub warnings: Vec<CompileWarning>,
}

pub struct CompileWarning {
    pub code: String,        // e.g., "COMPILE_W001"
    pub message: String,
    pub location: String,    // e.g., "kind 'orders', dataset 'us_orders', column 'amount'"
}
```

---

## 9. Planner API Surface

### 9.1 Dispatch Pattern

`from` is **optional**. When present, the planner resolves a named entity. When absent, the planner uses ad-hoc join resolution via the global `FieldIndex` and `RelationshipGraph`.

```rust
impl SemanticPlanner {
    pub fn plan(&self, request: &ResolvedQueryRequest, manifest: &CompiledManifest)
        -> Result<LogicalPlan, PlannerError>
    {
        match &request.from {
            // Named entity -- resolve and dispatch by type
            Some(entity_name) => {
                let data_kind = manifest.resolve_entity(entity_name)
                    .ok_or(PlannerError::EntityNotFound)?;
                self.plan_named_entity(data_kind, request, manifest)
            }
            // No entity -- resolve join from select fields
            None => {
                join::resolve_from_fields(request, manifest)
            }
        }
    }

    fn plan_named_entity(&self, data_kind: &DataKind, request: &ResolvedQueryRequest,
        manifest: &CompiledManifest) -> Result<LogicalPlan, PlannerError>
    {
        match data_kind {
            DataKind::CommonDataset(cd) => plan_common_dataset(cd, request),
            DataKind::ComplexDataKind(ck) => match &ck.strategy {
                KindStrategy::Grainset => plan_grainset(ck, request),
                KindStrategy::Unionset { mode } => plan_unionset(ck, request, *mode),
                KindStrategy::Joinset { .. } => plan_joinset(ck, request),
            },
        }
    }
}
```

### 9.2 Coverage API (Bitmap-Based)

```rust
// Build request mask once per query
let requested_mask = ck.coverage_index.build_field_mask(&requested_names);

// Find first covering dataset -- O(datasets x bitwise_and)
let covering = ck.dataset_bindings.iter().enumerate()
    .find(|(idx, _)| requested_mask.is_subset(&ck.coverage_index.dataset_bitmaps[*idx]));

// Find anchor (max coverage) for joinset -- O(datasets x popcount)
let anchor = ck.dataset_bindings.iter().enumerate()
    .max_by_key(|(idx, _)| {
        requested_mask.intersection(&ck.coverage_index.dataset_bitmaps[*idx]).count()
    });

// Filter by dimension coverage, assign measures greedily
let dim_mask = ck.coverage_index.build_field_mask(&dim_names);
let eligible: Vec<_> = ck.dataset_bindings.iter().enumerate()
    .filter(|(idx, _)| dim_mask.is_subset(&ck.coverage_index.dataset_bitmaps[*idx]))
    .collect();
```

### 9.3 Dimension Classification API

```rust
// O(1) temporal dimension lookup
let temporal = ck.temporal_dimension.as_deref(); // Option<&str>

// Pre-classified metadata dimensions
let is_meta = ck.dimension_index.metadata.iter().any(|(n, _)| n == dim_name);

// Partition requested dims into (metadata, regular) in O(n)
let meta_set: HashSet<&str> = ck.dimension_index.metadata.iter()
    .map(|(n, _)| n.as_str()).collect();
let (meta_dims, regular_dims): (Vec<_>, Vec<_>) = requested_dims.iter()
    .partition(|d| meta_set.contains(d.as_str()));
```

### 9.4 Join Traversal API

```rust
// BFS using pre-computed adjacency -- O(edges)
let adj = ck.adjacency_index.as_ref().unwrap();
let mut visited = FixedBitSet::with_capacity(ck.dataset_bindings.len());
let mut queue = VecDeque::new();
visited.insert(anchor_idx);
queue.push_back(anchor_idx);

while let Some(current) = queue.pop_front() {
    for &(neighbor, rel_idx) in &adj.forward[current] {
        if !visited.contains(neighbor) {
            visited.insert(neighbor);
            join_steps.push((neighbor, rel_idx, false));
            queue.push_back(neighbor);
        }
    }
    for &(neighbor, rel_idx) in &adj.reverse[current] {
        if !visited.contains(neighbor) {
            visited.insert(neighbor);
            join_steps.push((neighbor, rel_idx, true)); // reversed
            queue.push_back(neighbor);
        }
    }
}
```

### 9.5 Column Resolution in expr_lower

```rust
// Hot path: single HashMap lookup, no pattern matching
let physical_name = binding.column_mapping.physical.get(semantic_name)
    .ok_or(PlannerError::ColumnNotFound)?;
```

---

## 10. Ad-Hoc Join Resolution

### 10.1 Capability

When `from` is omitted from a query, the planner resolves the requested fields against the **global field index** and synthesizes a join plan using the **global relationship graph**. The user doesn't need to pre-define a joinset -- the system discovers join paths automatically.

```yaml
# User query -- no FROM, just field names
select: [date, customer_name, revenue]
```

The planner must:
1. Identify which datasets provide each field
2. Find the shortest join path connecting those datasets
3. Synthesize an ephemeral joinset plan

This is analogous to MetricFlow's entity-based join resolution and Cube.js's Dijkstra join graph.

### 10.2 Manifest Structures Used

| Structure | Role in Ad-Hoc Resolution |
|-----------|--------------------------|
| `FieldIndex.providers` | field_name -> candidate datasets |
| `FieldIndex.all_dimensions` / `all_measures` | Classify select names without a kind |
| `RelationshipGraph.shortest_paths` | Pre-computed shortest join path between any two datasets |
| `RelationshipGraph.forward` / `reverse` | Adjacency for fallback BFS if pair not in shortest_paths |
| `CompiledManifest.relationships` | Join type, columns, cardinality for each relationship |
| `CompiledManifest.datasets` | Physical schema, dimensions, measures for each dataset |

### 10.3 Resolution Algorithm (Planner -- Future Implementation)

The algorithm lives in `join.rs` and is invoked when `from` is absent. Joinset planner also delegates to `join::*` for BFS and join tree construction.

```
join::resolve_from_fields(request, manifest):
    field_index = manifest.field_index
    rel_graph   = manifest.relationship_graph

    // STEP 1: Classify requested fields
    For each name in request.select:
        If name in field_index.all_dimensions -> dimension
        If name in field_index.all_measures   -> measure
        If name in field_index.all_metrics    -> metric
        Else -> error: "unknown field '{name}'"

    // STEP 2: Resolve field -> dataset candidates
    For each field in (dimensions + measures):
        candidates[field] = field_index.providers[field]
        If candidates[field] is empty -> error: "no dataset provides '{field}'"

    // STEP 3: Find minimal dataset set (greedy set cover)
    //
    // Goal: smallest set of datasets that covers all requested fields.
    // Greedy: pick dataset covering most uncovered fields, repeat.
    //
    required_datasets = []
    uncovered = all requested fields
    While uncovered is not empty:
        best = dataset covering most fields in uncovered
        required_datasets.push(best)
        uncovered -= fields covered by best

    // STEP 4: Find join paths (shortest path)
    //
    // If only 1 dataset needed -> no join, plan as CommonDataset
    // If 2+ datasets -> connect via shortest paths
    //
    If required_datasets.len() == 1:
        return plan_single_dataset(required_datasets[0], request)

    // Pick anchor = dataset covering most fields (same as joinset)
    anchor = required_datasets with max field coverage

    // Build join tree via pre-computed shortest paths
    join_steps = []
    For each other_dataset in required_datasets:
        path = rel_graph.shortest_paths[(anchor, other_dataset)]
        If path is None:
            // Try transitive: anchor -> intermediate -> other
            // Or error if truly disconnected
            error: "no relationship path from '{anchor}' to '{other_dataset}'"
        join_steps.extend(path)

    // Deduplicate intermediate datasets that appear in multiple paths
    // Build left-deep join tree (same as joinset planner)

    // STEP 5: Build plan
    //
    // Synthesize an ephemeral ComplexDataKind with:
    //   strategy: Joinset
    //   dataset_bindings: required_datasets + intermediates
    //   relationships: from join_steps
    //
    // Delegate to plan_joinset()
```

### 10.4 Shortest Path Pre-Computation (Compilation Step 20)

At compile time, BFS from every dataset node to every reachable dataset:

```
build_shortest_paths(datasets, relationships):
    shortest_paths = {}

    For each source_dataset in datasets:
        // BFS from source
        visited = {source_dataset}
        queue = [(source_dataset, [])]  // (current, path_so_far)

        While queue not empty:
            (current, path) = queue.pop_front()

            For each relationship R where R.from == current or R.to == current:
                neighbor = R.other_side(current)
                If neighbor not in visited:
                    visited.add(neighbor)
                    new_path = path + [R.index]
                    shortest_paths[(source_dataset, neighbor)] = new_path
                    queue.push((neighbor, new_path))

    return shortest_paths
```

**Complexity**: O(N x (N + E)) where N = datasets, E = relationships. For 50 datasets and 100 relationships: ~5000 operations, <1ms.

**Space**: O(N^2 x avg_path_length). For 50 datasets with avg 2-hop paths: ~5000 entries x 2 indices = ~40KB. Negligible.

### 10.5 Field Index Construction (Compilation Step 21)

```
build_field_index(manifest):
    providers = {}
    all_dimensions = {}
    all_measures = {}
    all_metrics = {}

    // From datasets
    For each dataset in manifest.datasets:
        For each dim in dataset.dimensions:
            providers[dim.name].push(dataset.name)
            all_dimensions.add(dim.name)
        For each measure in dataset.measures:
            providers[measure.name].push(dataset.name)
            all_measures.add(measure.name)

    // From data kinds (for metrics, which are kind-level)
    For each data_kind in manifest.data_kinds:
        For each metric in data_kind.metrics:
            all_metrics.add(metric.name)

    return FieldIndex { providers, all_dimensions, all_measures, all_metrics }
```

### 10.6 Ambiguity Resolution

A field name may exist in multiple datasets (e.g., `date` in both `orders` and `customers`). The greedy set cover algorithm in Step 3 handles this naturally -- it picks the dataset that covers the most fields, breaking ties by preferring datasets already in the required set (minimizing joins).

**Unresolvable ambiguity**: If two disjoint sets of datasets could satisfy the query with different semantics (e.g., `revenue` means different things in `orders` vs `returns`), this is a **model authoring error**. The compiler should warn about ambiguous field names across disconnected dataset clusters (future validation step).

### 10.7 Intermediate Datasets

The shortest path between two datasets may traverse intermediate datasets that the user didn't request any fields from. These datasets are included in the join tree solely for connectivity.

```
Example:
  User requests: date (from orders), region_name (from regions)
  Shortest path:  orders -> customers -> regions
                           ^
                     intermediate (no requested fields, included for join)
```

The intermediate dataset (`customers`) is scanned only for its join key columns -- no dimensions or measures are projected from it.

### 10.8 Error Cases

| Error | When |
|-------|------|
| Unknown field | Field name not in `all_dimensions`, `all_measures`, or `all_metrics` |
| No provider | Field exists in index but mapped to zero datasets (shouldn't happen if index built correctly) |
| No join path | Required datasets are in disconnected components of the relationship graph |
| Ambiguous metric | Metric name exists in multiple data kinds (future: qualify with kind name) |

### 10.9 Interaction with Named Entities

When `from` IS specified, ad-hoc resolution is **not used**. The planner works within the scope of the named entity as before. The two paths are mutually exclusive:

```
from: present  -> resolve_entity() -> dispatch by DataKind type
from: absent   -> plan_adhoc_join() -> FieldIndex + RelationshipGraph
```

This means the same model supports both explicit composition (grainsets, unionsets, joinsets for well-known query patterns) and ad-hoc exploration (for discovery and one-off analysis).

---

## 11. Performance Impact

### 11.1 Expected Improvements

| Access Pattern | Current | After | Speedup |
|---------------|---------|-------|---------|
| Find temporal dimension | O(n) iter().find_map() | O(1) field access | ~10x for 10-dim kinds |
| Partition dimensions | O(n) per query | O(1) pre-classified | ~5x |
| Dataset coverage check | O(k) per dataset per name | O(1) bitmap AND | ~33x for 100 datasets x 50 fields |
| Grain group assignment | O(d) grouping + O(m) assignment | O(1) pre-grouped | ~5x |
| BFS edge lookup (joinset) | O(r) per step | O(1) adjacency list | ~10x for 10-dataset joinsets |
| Column name resolution | O(1) HashMap + pattern match | O(1) flat string map | ~2x (less branching) |
| Metric constituent lookup | O(expr tree) parsing | O(1) pre-extracted | ~10x |

### 11.2 Space Cost

| Structure | Size Estimate (per data kind) | Notes |
|-----------|-------------------------------|-------|
| CoverageIndex | ~100 bytes + datasets x (fields/8) | Bitmaps are compact |
| DimensionIndex | ~200 bytes + dim count x 50 | String allocations |
| GrainMap | ~100 bytes + datasets x 16 | Index + grain enum |
| AdjacencyIndex | ~200 bytes + edges x 32 | Vec<Vec<(usize, usize)>> |
| MetricOrder | ~50 bytes per metric | Vec of constituent names |

Total overhead: <10KB per data kind for typical models (5 datasets, 30 fields). Negligible compared to expression trees and mapping data already stored.

### 11.3 Global Structure Costs

| Structure | Size Estimate | Notes |
|-----------|--------------|-------|
| RelationshipGraph.shortest_paths | N^2 x avg_path x 8 bytes | 50 datasets, 2-hop avg = ~40KB |
| FieldIndex.providers | fields x avg_providers x 40 bytes | 200 fields, 2 providers = ~16KB |
| FieldIndex.all_* sets | ~50 bytes per field name | 200 fields = ~10KB |

Total global overhead: ~70KB for a 50-dataset, 200-field model. Computed once at compile time.

### 11.4 Memory Layout Rationale

**Array-of-Structs (AoS) retained** over Struct-of-Arrays (SoA):
- Planner accesses **multiple fields per dataset** (name + mapping + coverage) in tight loops
- AoS keeps related data on the same cache line
- SoA would require 3 separate Vec jumps per dataset access

**String interning deferred**: Converting field names to `u32` indices would save ~15% on string comparisons, but requires pervasive API changes (all planner functions become index-based). Defer until query volume exceeds 10K/sec per manifest -- unlikely in current use cases.
