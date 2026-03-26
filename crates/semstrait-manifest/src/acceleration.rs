//! Acceleration structures and resolved types for planner-optimized manifest.
//!
//! These types replace runtime pattern matching with pre-computed, purpose-built
//! indices. All structures are `Serialize + Deserialize` for JSON persistence.

use std::collections::{HashMap, HashSet};

use fixedbitset::FixedBitSet;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use semstrait_model::{
    ColumnMapping, ColumnMappingValue, DimensionType, JoinAssociativity, LiteralValue,
    MetadataDimension, TemporalGrain, UnionMode,
};

use crate::compiled::{
    CompiledDimension, CompiledFilter, CompiledKind, CompiledKindDataset, CompiledMeasure,
    CompiledMetric, CompiledRelationship,
};
use semstrait_model::Keys;

// ============================================================================
// KindInterface — shared semantic fields (zero duplication across variants)
// ============================================================================

/// The semantic interface of a queryable entity.
/// Every DataKind variant embeds this struct via composition.
/// Type resolution methods live here — one implementation, no duplication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindInterface {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Keys>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<CompiledFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_dim: Option<String>,
}

impl KindInterface {
    /// Resolve dimension DataType by name. O(1) via IndexMap.
    pub fn resolve_dim_type(&self, name: &str) -> semstrait_core::DataType {
        self.dimensions
            .get(name)
            .map(|d| d.data_type.clone())
            .unwrap_or(semstrait_core::DataType::Utf8)
    }

    /// Resolve measure or metric DataType by name. O(1) via IndexMap.
    pub fn resolve_measure_type(&self, name: &str) -> semstrait_core::DataType {
        self.measures
            .get(name)
            .map(|m| m.data_type.clone())
            .or_else(|| self.metrics.get(name).map(|m| m.data_type.clone()))
            .unwrap_or(semstrait_core::DataType::Float64)
    }

    /// Find the temporal dimension name (first temporal dimension found).
    pub fn find_temporal_dimension(&self) -> Option<&str> {
        self.dimensions.iter().find_map(|(name, dim)| {
            if matches!(dim.dim_type, DimensionType::Temporal(_)) {
                Some(name.as_str())
            } else {
                None
            }
        })
    }

    /// Returns true if this kind has any additivity type other than Full.
    pub fn has_non_full_additivity(&self) -> bool {
        self.measures.values().any(|m| {
            m.additivity
                .as_ref()
                .is_some_and(|a| !matches!(a, semstrait_model::AdditivityType::Full))
        })
    }

    /// Returns the grain entries for a specific temporal dimension, if present.
    pub fn temporal_grains(&self, dim_name: &str) -> Option<Vec<TemporalGrain>> {
        let dim = self.dimensions.get(dim_name)?;
        match &dim.dim_type {
            DimensionType::Temporal(t) => Some(t.grains.clone()),
            _ => None,
        }
    }
}

// ============================================================================
// DataKind — 4-variant enum (dataset, unionset, grainset, joinset)
// ============================================================================

/// A queryable semantic entity. Four variants map directly to the four
/// kind types in the semantic model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind_type", rename_all = "snake_case")]
pub enum DataKind {
    /// Single dataset, direct query. No dataset routing logic.
    Dataset(Box<DatasetKind>),
    /// UNION ALL across multiple datasets.
    Unionset(Box<UnionsetKind>),
    /// Grain-based covering dataset selection.
    Grainset(Box<GrainsetKind>),
    /// Join-based composition via BFS join chain.
    Joinset(Box<JoinsetKind>),
}

impl DataKind {
    /// Access the shared KindInterface regardless of variant.
    pub fn interface(&self) -> &KindInterface {
        match self {
            DataKind::Dataset(k) => &k.interface,
            DataKind::Unionset(k) => &k.interface,
            DataKind::Grainset(k) => &k.interface,
            DataKind::Joinset(k) => &k.interface,
        }
    }

    /// Convenience: entity name.
    pub fn name(&self) -> &str {
        &self.interface().name
    }

    /// Mutable access to the shared KindInterface (for tests / configuration).
    pub fn interface_mut(&mut self) -> &mut KindInterface {
        match self {
            DataKind::Dataset(k) => &mut k.interface,
            DataKind::Unionset(k) => &mut k.interface,
            DataKind::Grainset(k) => &mut k.interface,
            DataKind::Joinset(k) => &mut k.interface,
        }
    }

    /// All dataset bindings across all variants.
    pub fn bindings(&self) -> &[DatasetBinding] {
        match self {
            DataKind::Dataset(k) => std::slice::from_ref(&k.binding),
            DataKind::Unionset(k) => &k.bindings,
            DataKind::Grainset(k) => &k.bindings,
            DataKind::Joinset(k) => &k.bindings,
        }
    }
}

/// Shared interface across all queryable entities.
/// Returns references only — no cloning in the hot path.
pub trait SemanticInterface {
    fn interface(&self) -> &KindInterface;

    fn dimensions(&self) -> &IndexMap<String, CompiledDimension> {
        &self.interface().dimensions
    }
    fn measures(&self) -> &IndexMap<String, CompiledMeasure> {
        &self.interface().measures
    }
    fn metrics(&self) -> &IndexMap<String, CompiledMetric> {
        &self.interface().metrics
    }
    fn filters(&self) -> &[CompiledFilter] {
        &self.interface().filters
    }
    fn keys(&self) -> Option<&Keys> {
        self.interface().keys.as_ref()
    }
    fn domain(&self) -> Option<&[String]> {
        self.interface().domain.as_deref()
    }
    fn temporal_dimension(&self) -> Option<&str> {
        self.interface().temporal_dim.as_deref()
    }
}

impl SemanticInterface for DataKind {
    fn interface(&self) -> &KindInterface {
        self.interface()
    }
}

/// Shared behavior for multi-dataset kinds (unionset, grainset, joinset).
pub trait MultiDatasetKind: SemanticInterface {
    fn bindings(&self) -> &[DatasetBinding];
    fn coverage_index(&self) -> &CoverageIndex;
    fn dimension_index(&self) -> &DimensionIndex;
    fn metric_order(&self) -> Option<&MetricOrder>;
}

// ============================================================================
// DatasetKind — Single-Dataset Fast Path
// ============================================================================

/// Single dataset, direct Scan → Agg → Project. No routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetKind {
    #[serde(flatten)]
    pub interface: KindInterface,
    /// The single dataset binding for this entity.
    pub binding: DatasetBinding,
}

impl SemanticInterface for DatasetKind {
    fn interface(&self) -> &KindInterface {
        &self.interface
    }
}

// ============================================================================
// UnionsetKind — UNION ALL Across Datasets
// ============================================================================

/// UNION ALL across multiple datasets. Each branch scans one dataset;
/// unmapped columns are NULL-filled. Result is re-aggregated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnionsetKind {
    #[serde(flatten)]
    pub interface: KindInterface,
    pub mode: UnionMode,
    pub bindings: Vec<DatasetBinding>,
    // Acceleration structures
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_order: Option<MetricOrder>,
}

impl SemanticInterface for UnionsetKind {
    fn interface(&self) -> &KindInterface {
        &self.interface
    }
}

impl MultiDatasetKind for UnionsetKind {
    fn bindings(&self) -> &[DatasetBinding] {
        &self.bindings
    }
    fn coverage_index(&self) -> &CoverageIndex {
        &self.coverage_index
    }
    fn dimension_index(&self) -> &DimensionIndex {
        &self.dimension_index
    }
    fn metric_order(&self) -> Option<&MetricOrder> {
        self.metric_order.as_ref()
    }
}

// ============================================================================
// GrainsetKind — Grain-Based Covering Dataset Selection
// ============================================================================

/// Grain-based covering: routes queries to the cheapest covering dataset(s).
/// Multi-grain datasets are UNION ALL'd with DATE_TRUNC rollup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrainsetKind {
    #[serde(flatten)]
    pub interface: KindInterface,
    pub bindings: Vec<DatasetBinding>,
    // Acceleration structures
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_order: Option<MetricOrder>,
    /// Grain map for temporal routing. Present when kind has temporal dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grain_map: Option<GrainMap>,
}

impl SemanticInterface for GrainsetKind {
    fn interface(&self) -> &KindInterface {
        &self.interface
    }
}

impl MultiDatasetKind for GrainsetKind {
    fn bindings(&self) -> &[DatasetBinding] {
        &self.bindings
    }
    fn coverage_index(&self) -> &CoverageIndex {
        &self.coverage_index
    }
    fn dimension_index(&self) -> &DimensionIndex {
        &self.dimension_index
    }
    fn metric_order(&self) -> Option<&MetricOrder> {
        self.metric_order.as_ref()
    }
}

// ============================================================================
// JoinsetKind — Join-Based Composition
// ============================================================================

/// Join-based composition via BFS join chain from an anchor dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinsetKind {
    #[serde(flatten)]
    pub interface: KindInterface,
    pub associativity: JoinAssociativity,
    pub bindings: Vec<DatasetBinding>,
    pub relationships: Vec<CompiledRelationship>,
    // Acceleration structures
    pub coverage_index: CoverageIndex,
    pub dimension_index: DimensionIndex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_order: Option<MetricOrder>,
    pub adjacency_index: AdjacencyIndex,
}

impl SemanticInterface for JoinsetKind {
    fn interface(&self) -> &KindInterface {
        &self.interface
    }
}

impl MultiDatasetKind for JoinsetKind {
    fn bindings(&self) -> &[DatasetBinding] {
        &self.bindings
    }
    fn coverage_index(&self) -> &CoverageIndex {
        &self.coverage_index
    }
    fn dimension_index(&self) -> &DimensionIndex {
        &self.dimension_index
    }
    fn metric_order(&self) -> Option<&MetricOrder> {
        self.metric_order.as_ref()
    }
}

// ============================================================================
// ResolvedColumnMapping -- Pre-Split by Purpose
// ============================================================================

/// Pre-resolved column mapping split by target type.
/// Eliminates runtime pattern matching on `ColumnMappingValue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedColumnMapping {
    /// Physical column mappings: semantic_name -> physical_column_name.
    /// The hot-path lookup used by expr_lower. No pattern matching needed.
    pub physical: IndexMap<String, String>,

    /// Literal value injections: semantic_name -> literal_value.
    pub literals: HashMap<String, String>,

    /// Temporal dimension mappings: semantic_name -> TemporalMapping.
    /// Separated because temporal dims may have grain overrides.
    pub temporal: HashMap<String, TemporalMapping>,

    /// Anchored sub-name mappings: semantic_name -> (sub_name -> physical_column).
    /// Used for composed expressions with multiple physical columns.
    pub anchored: HashMap<String, HashMap<String, String>>,
}

impl ResolvedColumnMapping {
    /// Build a `ResolvedColumnMapping` from a raw `ColumnMapping`.
    ///
    /// Panics if the mapping is `Auto` or `Inherited` (those must be resolved
    /// before this function is called).
    pub fn from_column_mapping(mapping: &ColumnMapping) -> Self {
        let explicit = match mapping {
            ColumnMapping::Explicit(m) => m,
            ColumnMapping::Auto => panic!("Auto mapping must be expanded before resolution"),
            ColumnMapping::Inherited => {
                panic!("Inherited mapping must be resolved before resolution")
            }
        };

        let mut physical = IndexMap::new();
        let mut literals = HashMap::new();
        let mut temporal = HashMap::new();
        let mut anchored = HashMap::new();

        for (semantic_name, value) in explicit {
            match value {
                ColumnMappingValue::Simple(col) => {
                    physical.insert(semantic_name.clone(), col.clone());
                }
                ColumnMappingValue::WithGrain { column, grain } => {
                    // WithGrain entries go into temporal map AND physical map.
                    // The physical column is needed by expr_lower for the scan.
                    physical.insert(semantic_name.clone(), column.clone());
                    temporal.insert(
                        semantic_name.clone(),
                        TemporalMapping {
                            physical_column: column.clone(),
                            grain: *grain,
                        },
                    );
                }
                ColumnMappingValue::Literal(lit) => {
                    let val = match lit {
                        LiteralValue::String(s) => s.clone(),
                    };
                    literals.insert(semantic_name.clone(), val);
                }
                ColumnMappingValue::Anchored(sub_map) => {
                    // For anchored mappings, each sub-name maps to a physical column.
                    // Also add all sub-columns to physical so they get scanned.
                    for (sub_name, phys_col) in sub_map {
                        physical.insert(sub_name.clone(), phys_col.clone());
                    }
                    anchored.insert(semantic_name.clone(), sub_map.clone());
                }
            }
        }

        Self {
            physical,
            literals,
            temporal,
            anchored,
        }
    }

    /// Check if a semantic name is mapped (in any category).
    ///
    /// Note: `WithGrain` entries are in both `physical` and `temporal`.
    /// The `temporal` check is included for robustness.
    pub fn contains_key(&self, name: &str) -> bool {
        self.physical.contains_key(name)
            || self.literals.contains_key(name)
            || self.temporal.contains_key(name)
            || self.anchored.contains_key(name)
    }

    /// Get the physical column name for a semantic name.
    /// Returns None for literals, metadata, and anchored mappings.
    pub fn get_physical(&self, name: &str) -> Option<&str> {
        self.physical.get(name).map(|s| s.as_str())
    }

    /// Returns all mapped semantic-level names (dimension/measure names).
    ///
    /// For `Anchored` mappings, returns the parent semantic name only,
    /// not the sub-names (which are in `physical` for scan purposes).
    /// Use `physical.keys()` directly if you need scan-level column names.
    pub fn semantic_keys(&self) -> Vec<&String> {
        // Collect physical keys that are NOT anchored sub-names.
        let anchored_sub_names: HashSet<&String> = self
            .anchored
            .values()
            .flat_map(|sub_map| sub_map.keys())
            .collect();

        self.physical
            .keys()
            .filter(|k| !anchored_sub_names.contains(k))
            .chain(self.literals.keys())
            .chain(self.anchored.keys())
            .collect()
    }
}

/// Temporal dimension mapping with optional grain override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalMapping {
    pub physical_column: String,
    pub grain: Option<TemporalGrain>,
}

// ============================================================================
// DatasetBinding -- Per-Dataset Binding in Complex Kinds
// ============================================================================

/// A resolved dataset binding within a complex data kind.
/// Replaces `CompiledKindDataset`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetBinding {
    /// Dataset name (reference into manifest.datasets).
    pub dataset_name: String,

    /// Resolved column mapping (pre-split for fast planner access).
    pub column_mapping: ResolvedColumnMapping,

    /// Resolved physical source references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_sources: Vec<ResolvedSource>,
}

/// A resolved physical source reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedSource {
    /// Original user-provided reference string (pattern/glob for debugging).
    pub reference: String,
    pub source_type: SourceType,
    /// Fully qualified table name for SQL emission (e.g., "namespace.table_name").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_fqn: Option<String>,
    /// Fully resolved physical location (populated by resolve_sources step).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Data format (from storage config for paths, from catalog for tables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<semstrait_core::DataFormat>,
    /// Catalog alias that resolved this source (None for filesystem sources).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_alias: Option<String>,
    /// Schema snapshot captured at compile time (best-effort).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Vec<crate::catalog_snapshot::ResolvedColumn>>,
}

impl ResolvedSource {
    /// Create a path-type resolved source.
    pub fn path(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
            source_type: SourceType::Path,
            table_fqn: None,
            location: None,
            format: None,
            catalog_alias: None,
            schema: None,
        }
    }

    /// Create a table-type resolved source.
    pub fn table(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
            source_type: SourceType::Table,
            table_fqn: None,
            location: None,
            format: None,
            catalog_alias: None,
            schema: None,
        }
    }
}

/// Type of physical source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Path,
    Table,
}

// ============================================================================
// CoverageIndex -- Per-Dataset Coverage Bitmaps
// ============================================================================

/// Pre-computed coverage bitmaps for O(1) dataset selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageIndex {
    /// Ordered list of all mappable field names (dimensions + measures).
    /// Position in this Vec = bit position in the bitmaps.
    pub field_names: Vec<String>,

    /// Reverse lookup: field_name -> bit position.
    pub field_positions: HashMap<String, usize>,

    /// Coverage bitmap per dataset binding.
    /// Index corresponds to ComplexDataKind.dataset_bindings order.
    /// Serialized as Vec<Vec<usize>> (set bit positions) for JSON compatibility.
    #[serde(with = "bitset_vec_serde")]
    pub dataset_bitmaps: Vec<FixedBitSet>,
}

/// Custom serde for Vec<FixedBitSet> that serializes as Vec<Vec<usize>>
/// (set bit positions + capacity). FixedBitSet's built-in serde uses borrowed
/// byte arrays which don't round-trip through JSON.
mod bitset_vec_serde {
    use super::*;
    use serde::ser::SerializeSeq;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct BitsetRepr {
        capacity: usize,
        ones: Vec<usize>,
    }

    pub fn serialize<S: Serializer>(
        bitmaps: &[FixedBitSet],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(bitmaps.len()))?;
        for bitmap in bitmaps {
            let repr = BitsetRepr {
                capacity: bitmap.len(),
                ones: bitmap.ones().collect(),
            };
            seq.serialize_element(&repr)?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<FixedBitSet>, D::Error> {
        let reprs: Vec<BitsetRepr> = Vec::deserialize(deserializer)?;
        Ok(reprs
            .into_iter()
            .map(|r| {
                let mut bs = FixedBitSet::with_capacity(r.capacity);
                for bit in r.ones {
                    bs.insert(bit);
                }
                bs
            })
            .collect())
    }
}

impl CoverageIndex {
    /// Build a CoverageIndex from kind dimensions, measures, and dataset bindings.
    ///
    /// Metadata dimensions are excluded — they are derived from source metadata
    /// (paths, partitions), not physical columns. Metadata dimension availability
    /// should be validated through `DimensionIndex.metadata`, not `CoverageIndex`.
    pub fn build(
        dimensions: &IndexMap<String, CompiledDimension>,
        measures: &IndexMap<String, CompiledMeasure>,
        bindings: &[DatasetBinding],
    ) -> Self {
        // Collect all mappable field names (dimensions + measures, excluding metadata).
        let field_names: Vec<String> = dimensions
            .iter()
            .filter(|(_, d)| !matches!(d.dim_type, DimensionType::Metadata(_)))
            .map(|(name, _)| name.clone())
            .chain(measures.keys().cloned())
            .collect();

        let field_positions: HashMap<String, usize> = field_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i))
            .collect();

        let n_fields = field_names.len();

        let dataset_bitmaps = bindings
            .iter()
            .map(|binding| {
                let mut bitmap = FixedBitSet::with_capacity(n_fields);
                for (name, pos) in &field_positions {
                    if binding.column_mapping.contains_key(name) {
                        bitmap.insert(*pos);
                    }
                }
                bitmap
            })
            .collect();

        Self {
            field_names,
            field_positions,
            dataset_bitmaps,
        }
    }

    /// Build a field mask from a list of requested field names.
    pub fn build_field_mask(&self, names: &[&str]) -> FixedBitSet {
        let mut mask = FixedBitSet::with_capacity(self.field_names.len());
        for name in names {
            if let Some(&pos) = self.field_positions.get(*name) {
                mask.insert(pos);
            }
        }
        mask
    }
}

// ============================================================================
// DimensionIndex -- Pre-Classified Dimension Buckets
// ============================================================================

/// Pre-classified dimension buckets for O(1) lookups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionIndex {
    /// Temporal dimension names (usually 0 or 1).
    pub temporal: Vec<String>,
    /// Metadata dimensions: (name, MetadataDimension config).
    pub metadata: Vec<(String, MetadataDimension)>,
    /// Categorical dimension names (includes binary, geo, bucketed).
    pub categorical: Vec<String>,
    /// Literal dimensions per dataset: dataset_idx -> (dim_name -> literal_value).
    pub literals_by_dataset: Vec<HashMap<String, String>>,
}

impl DimensionIndex {
    /// Build a DimensionIndex from kind dimensions and dataset bindings.
    pub fn build(
        dimensions: &IndexMap<String, CompiledDimension>,
        bindings: &[DatasetBinding],
    ) -> Self {
        let mut temporal = Vec::new();
        let mut metadata = Vec::new();
        let mut categorical = Vec::new();

        for (name, dim) in dimensions {
            match &dim.dim_type {
                DimensionType::Temporal(_) => temporal.push(name.clone()),
                DimensionType::Metadata(meta) => metadata.push((name.clone(), meta.clone())),
                _ => categorical.push(name.clone()),
            }
        }

        let literals_by_dataset = bindings
            .iter()
            .map(|b| b.column_mapping.literals.clone())
            .collect();

        Self {
            temporal,
            metadata,
            categorical,
            literals_by_dataset,
        }
    }
}

// ============================================================================
// GrainMap -- Grainset-Specific
// ============================================================================

/// Pre-grouped datasets by temporal grain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrainMap {
    /// Temporal dimension name this map applies to.
    pub temporal_dim: String,
    /// Groups of datasets by native grain, sorted coarsest -> finest.
    /// Each entry: (grain, dataset_binding indices).
    pub groups: Vec<(TemporalGrain, Vec<usize>)>,
    /// Per-dataset native grain: dataset_binding index -> grain.
    pub dataset_grains: Vec<Option<TemporalGrain>>,
}

impl GrainMap {
    /// Build a GrainMap from dataset bindings and their temporal configurations.
    pub fn build(
        temporal_dim: &str,
        bindings: &[DatasetBinding],
    ) -> Self {
        let mut dataset_grains: Vec<Option<TemporalGrain>> = Vec::with_capacity(bindings.len());
        let mut grain_groups: HashMap<TemporalGrain, Vec<usize>> = HashMap::new();

        for (idx, binding) in bindings.iter().enumerate() {
            let grain = binding
                .column_mapping
                .temporal
                .get(temporal_dim)
                .and_then(|tm| tm.grain);
            dataset_grains.push(grain);
            if let Some(g) = grain {
                grain_groups.entry(g).or_default().push(idx);
            }
        }

        // Sort groups by grain coarseness (coarsest first).
        let mut groups: Vec<(TemporalGrain, Vec<usize>)> = grain_groups.into_iter().collect();
        // Sort groups coarsest first (highest coarseness value first).
        groups.sort_by(|(a, _), (b, _)| b.coarseness().cmp(&a.coarseness()));

        Self {
            temporal_dim: temporal_dim.to_string(),
            groups,
            dataset_grains,
        }
    }
}

// ============================================================================
// AdjacencyIndex -- Joinset-Specific
// ============================================================================

/// Pre-built adjacency lists for BFS join traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjacencyIndex {
    /// Forward edges: dataset_binding_idx -> [(neighbor_idx, relationship_idx)].
    pub forward: Vec<Vec<(usize, usize)>>,
    /// Reverse edges: dataset_binding_idx -> [(neighbor_idx, relationship_idx)].
    pub reverse: Vec<Vec<(usize, usize)>>,
    /// Dataset name -> dataset_binding index.
    pub dataset_index: HashMap<String, usize>,
}

impl AdjacencyIndex {
    /// Build an AdjacencyIndex from dataset bindings and relationships.
    pub fn build(
        bindings: &[DatasetBinding],
        relationships: &[CompiledRelationship],
    ) -> Self {
        let dataset_index: HashMap<String, usize> = bindings
            .iter()
            .enumerate()
            .map(|(i, b)| (b.dataset_name.clone(), i))
            .collect();

        let n = bindings.len();
        let mut forward = vec![Vec::new(); n];
        let mut reverse = vec![Vec::new(); n];

        for (rel_idx, rel) in relationships.iter().enumerate() {
            if let (Some(&from_idx), Some(&to_idx)) =
                (dataset_index.get(&rel.from), dataset_index.get(&rel.to))
            {
                forward[from_idx].push((to_idx, rel_idx));
                reverse[to_idx].push((from_idx, rel_idx));
            }
        }

        Self {
            forward,
            reverse,
            dataset_index,
        }
    }
}

// ============================================================================
// MetricOrder -- Topological Sort + Constituent Measures
// ============================================================================

/// Pre-computed metric evaluation order and constituent measures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricOrder {
    /// Metric names in topological order (leaves to roots).
    pub evaluation_order: Vec<String>,
    /// Constituent measures for each metric: metric_name -> measure_names.
    pub metric_measures: HashMap<String, Vec<String>>,
}

impl MetricOrder {
    /// Build MetricOrder from compiled metrics and measures.
    ///
    /// Walks each metric's `Expr` tree to find `EntityRef` nodes, then
    /// transitively resolves metric references to leaf measures.
    /// Uses metric `depth` for topological ordering (ascending = leaves first).
    pub fn build(
        metrics: &IndexMap<String, CompiledMetric>,
        measures: &IndexMap<String, CompiledMeasure>,
    ) -> Option<MetricOrder> {
        if metrics.is_empty() {
            return None;
        }

        let measure_names: HashSet<&str> = measures.keys().map(|s| s.as_str()).collect();
        let metric_names: HashSet<&str> = metrics.keys().map(|s| s.as_str()).collect();

        // First pass: extract direct references from each metric's Expr.
        let mut direct_refs: HashMap<&str, Vec<String>> = HashMap::new();
        for (name, metric) in metrics {
            let refs = collect_entity_refs(&metric.expr);
            direct_refs.insert(name.as_str(), refs);
        }

        // Second pass: resolve transitive measure dependencies.
        // For each metric, follow metric->metric chains until we reach measures.
        let mut metric_measures: HashMap<String, Vec<String>> = HashMap::new();
        for name in metrics.keys() {
            let mut leaf_measures: HashSet<String> = HashSet::new();
            let mut visited: HashSet<&str> = HashSet::new();
            let mut stack: Vec<&str> = vec![name.as_str()];

            while let Some(current) = stack.pop() {
                if !visited.insert(current) {
                    continue; // Already visited (cycle protection, though cycles are checked earlier)
                }
                if let Some(refs) = direct_refs.get(current) {
                    for r in refs {
                        if measure_names.contains(r.as_str()) {
                            leaf_measures.insert(r.clone());
                        } else if metric_names.contains(r.as_str()) {
                            stack.push(r.as_str());
                        }
                        // Unknown refs are ignored (could be dimensions in filters, etc.)
                    }
                }
            }

            let mut measures_vec: Vec<String> = leaf_measures.into_iter().collect();
            measures_vec.sort(); // Deterministic order
            metric_measures.insert(name.clone(), measures_vec);
        }

        // Build evaluation order sorted by depth (ascending = leaves first).
        let mut evaluation_order: Vec<(String, usize)> = metrics
            .iter()
            .map(|(name, m)| (name.clone(), m.depth))
            .collect();
        evaluation_order.sort_by_key(|(_, depth)| *depth);
        let evaluation_order: Vec<String> =
            evaluation_order.into_iter().map(|(name, _)| name).collect();

        Some(MetricOrder {
            evaluation_order,
            metric_measures,
        })
    }
}

/// Recursively collect all `EntityRef` names from an expression tree.
fn collect_entity_refs(expr: &semstrait_core::Expr) -> Vec<String> {
    let mut refs = Vec::new();
    collect_entity_refs_inner(expr, &mut refs);
    refs
}

fn collect_entity_refs_inner(expr: &semstrait_core::Expr, refs: &mut Vec<String>) {
    use semstrait_core::Expr;
    match expr {
        Expr::EntityRef(er) => refs.push(er.name.clone()),
        Expr::Column(_) | Expr::Literal(_) => {}
        Expr::Aggregate(agg) => collect_entity_refs_inner(&agg.expr, refs),
        Expr::BinaryOp(bin) => {
            collect_entity_refs_inner(&bin.left, refs);
            collect_entity_refs_inner(&bin.right, refs);
        }
        Expr::Negate(u) | Expr::Not(u) | Expr::IsNull(u) | Expr::IsNotNull(u) => {
            collect_entity_refs_inner(&u.expr, refs);
        }
        Expr::Case(c) => {
            for wc in &c.when_then {
                collect_entity_refs_inner(&wc.condition, refs);
                collect_entity_refs_inner(&wc.result, refs);
            }
            if let Some(else_expr) = &c.else_expr {
                collect_entity_refs_inner(else_expr, refs);
            }
        }
        Expr::InList(il) => {
            collect_entity_refs_inner(&il.expr, refs);
            for item in &il.list {
                collect_entity_refs_inner(item, refs);
            }
        }
        Expr::Between(b) => {
            collect_entity_refs_inner(&b.expr, refs);
            collect_entity_refs_inner(&b.low, refs);
            collect_entity_refs_inner(&b.high, refs);
        }
        Expr::Like(l) => {
            collect_entity_refs_inner(&l.expr, refs);
            collect_entity_refs_inner(&l.pattern, refs);
        }
        Expr::Coalesce(c) => {
            for e in &c.exprs {
                collect_entity_refs_inner(e, refs);
            }
        }
        Expr::NullIf(n) => {
            collect_entity_refs_inner(&n.expr, refs);
            collect_entity_refs_inner(&n.null_expr, refs);
        }
        Expr::DateTrunc(dt) => {
            collect_entity_refs_inner(&dt.expr, refs);
        }
        Expr::FunctionCall(fc) => {
            for arg in &fc.args {
                collect_entity_refs_inner(arg, refs);
            }
        }
        Expr::Guard(g) => {
            collect_entity_refs_inner(&g.condition, refs);
            collect_entity_refs_inner(&g.expr, refs);
        }
    }
}

// ============================================================================
// Global Graph Structures
// ============================================================================

/// Global relationship graph for ad-hoc join resolution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationshipGraph {
    /// Forward edges: from_dataset -> [(to_dataset, relationship_idx)].
    pub forward: HashMap<String, Vec<(String, usize)>>,
    /// Reverse edges: to_dataset -> [(from_dataset, relationship_idx)].
    pub reverse: HashMap<String, Vec<(String, usize)>>,
    /// Pre-computed shortest paths between all reachable dataset pairs.
    /// Key format: "from_dataset\0to_dataset" (null byte separator).
    /// Use `shortest_path()` / `set_shortest_path()` for access.
    pub shortest_paths: HashMap<String, Vec<usize>>,
    /// Dataset name -> index (for bitmap operations).
    pub dataset_index: HashMap<String, usize>,
}

impl RelationshipGraph {
    /// Key encoding for shortest_paths HashMap.
    fn path_key(from: &str, to: &str) -> String {
        format!("{}\0{}", from, to)
    }

    /// Look up the shortest path between two datasets.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<&Vec<usize>> {
        self.shortest_paths.get(&Self::path_key(from, to))
    }

    /// Store a shortest path between two datasets.
    pub fn set_shortest_path(&mut self, from: &str, to: &str, path: Vec<usize>) {
        self.shortest_paths.insert(Self::path_key(from, to), path);
    }
}

/// Global inverted index: field_name -> provider datasets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldIndex {
    /// field_name -> Vec<dataset_name> that provide this field.
    pub providers: HashMap<String, Vec<String>>,
    /// Dimension names (across all datasets).
    pub all_dimensions: HashSet<String>,
    /// Measure names (across all datasets).
    pub all_measures: HashSet<String>,
    /// Metric names (across all data kinds).
    pub all_metrics: HashSet<String>,
}

// ============================================================================
// Diagnostics
// ============================================================================

/// Compilation diagnostics (warnings, info messages).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompileDiagnostics {
    pub warnings: Vec<CompileWarning>,
}

/// A single compile-time warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileWarning {
    pub code: String,
    pub message: String,
    pub location: String,
}

// ============================================================================
// Conversion from existing types
// ============================================================================

impl DatasetBinding {
    /// Create a DatasetBinding from a CompiledKindDataset.
    pub fn from_compiled(ds: &CompiledKindDataset) -> Self {
        let column_mapping = ResolvedColumnMapping::from_column_mapping(&ds.extras.column_mapping);

        Self {
            dataset_name: ds.name.clone(),
            column_mapping,
            resolved_sources: ds.resolved_sources.clone(),
        }
    }
}

/// Build a `KindInterface` from a `CompiledKind` (shared extraction).
fn build_interface(kind: &CompiledKind) -> KindInterface {
    let temporal_dim = kind
        .dimensions
        .iter()
        .find(|(_, d)| matches!(d.dim_type, DimensionType::Temporal(_)))
        .map(|(name, _)| name.clone());

    KindInterface {
        name: kind.name.clone(),
        description: kind.description.clone(),
        dimensions: kind.dimensions.clone(),
        measures: kind.measures.clone(),
        metrics: kind.metrics.clone(),
        keys: kind.keys.clone(),
        filters: kind.filters.clone(),
        domain: kind.domain.clone(),
        temporal_dim,
    }
}

/// Build a DataKind from a CompiledKind.
///
/// Single-binding kinds are flattened to `DataKind::Dataset` regardless of the
/// declared `kind_type`. A single-dataset grainset/unionset/joinset is
/// semantically equivalent to a direct dataset query — no routing, union, or
/// join is needed. The planner dispatches `Dataset` via a simpler fast path.
pub fn data_kind_from_compiled_kind(kind: &CompiledKind) -> DataKind {
    use crate::compiled::CompiledKindType;

    let bindings: Vec<DatasetBinding> = kind
        .datasets
        .iter()
        .map(DatasetBinding::from_compiled)
        .collect();

    let interface = build_interface(kind);

    // Single-dataset kinds → DatasetKind (fast path)
    if bindings.len() == 1 {
        let binding = bindings.into_iter().next().unwrap();
        return DataKind::Dataset(Box::new(DatasetKind { interface, binding }));
    }

    // Multi-dataset kinds → variant per strategy
    let metric_order = MetricOrder::build(&kind.metrics, &kind.measures);
    let coverage_index = CoverageIndex::build(&kind.dimensions, &kind.measures, &bindings);
    let dimension_index = DimensionIndex::build(&kind.dimensions, &bindings);

    match &kind.kind_type {
        CompiledKindType::Grainset => {
            let grain_map = interface
                .temporal_dim
                .as_deref()
                .map(|td| GrainMap::build(td, &bindings));

            DataKind::Grainset(Box::new(GrainsetKind {
                interface,
                bindings,
                coverage_index,
                dimension_index,
                metric_order,
                grain_map,
            }))
        }
        CompiledKindType::Unionset { mode } => DataKind::Unionset(Box::new(UnionsetKind {
            interface,
            mode: *mode,
            bindings,
            coverage_index,
            dimension_index,
            metric_order,
        })),
        CompiledKindType::Joinset { associativity } => {
            let adjacency_index = AdjacencyIndex::build(&bindings, &kind.relationships);
            DataKind::Joinset(Box::new(JoinsetKind {
                interface,
                associativity: *associativity,
                bindings,
                relationships: kind.relationships.clone(),
                coverage_index,
                dimension_index,
                metric_order,
                adjacency_index,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_model::{CategoricalDimension, ColumnMapping, ColumnMappingValue, LiteralValue};
    use std::collections::HashMap;

    fn make_explicit_mapping(pairs: Vec<(&str, ColumnMappingValue)>) -> ColumnMapping {
        let map: HashMap<String, ColumnMappingValue> = pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ColumnMapping::Explicit(map)
    }

    #[test]
    fn test_resolved_column_mapping_simple() {
        let mapping = make_explicit_mapping(vec![
            ("date", ColumnMappingValue::Simple("order_date".into())),
            ("revenue", ColumnMappingValue::Simple("amount".into())),
        ]);

        let resolved = ResolvedColumnMapping::from_column_mapping(&mapping);

        assert_eq!(resolved.physical.get("date").unwrap(), "order_date");
        assert_eq!(resolved.physical.get("revenue").unwrap(), "amount");
        assert!(resolved.literals.is_empty());
        assert!(resolved.temporal.is_empty());
        assert!(resolved.anchored.is_empty());
    }

    #[test]
    fn test_resolved_column_mapping_with_grain() {
        let mapping = make_explicit_mapping(vec![(
            "date",
            ColumnMappingValue::WithGrain {
                column: "created_at".into(),
                grain: Some(TemporalGrain::Day),
            },
        )]);

        let resolved = ResolvedColumnMapping::from_column_mapping(&mapping);

        // Should be in both physical and temporal
        assert_eq!(resolved.physical.get("date").unwrap(), "created_at");
        let tm = resolved.temporal.get("date").unwrap();
        assert_eq!(tm.physical_column, "created_at");
        assert_eq!(tm.grain, Some(TemporalGrain::Day));
    }

    #[test]
    fn test_resolved_column_mapping_literal() {
        let mapping = make_explicit_mapping(vec![(
            "platform",
            ColumnMappingValue::Literal(LiteralValue::String("web".into())),
        )]);

        let resolved = ResolvedColumnMapping::from_column_mapping(&mapping);

        assert!(resolved.physical.get("platform").is_none());
        assert_eq!(resolved.literals.get("platform").unwrap(), "web");
        assert!(resolved.contains_key("platform"));
    }

    #[test]
    fn test_resolved_column_mapping_anchored() {
        let mut sub_map = HashMap::new();
        sub_map.insert("order_sum".into(), "physical_order_amount".into());
        sub_map.insert("delivery_cost".into(), "physical_delivery_fee".into());

        let mapping = make_explicit_mapping(vec![(
            "total_cost",
            ColumnMappingValue::Anchored(sub_map),
        )]);

        let resolved = ResolvedColumnMapping::from_column_mapping(&mapping);

        // Sub-names in physical
        assert_eq!(
            resolved.physical.get("order_sum").unwrap(),
            "physical_order_amount"
        );
        assert_eq!(
            resolved.physical.get("delivery_cost").unwrap(),
            "physical_delivery_fee"
        );
        // Anchored map present
        assert!(resolved.anchored.contains_key("total_cost"));
        assert!(resolved.contains_key("total_cost"));
    }

    #[test]
    fn test_contains_key_across_categories() {
        let mapping = make_explicit_mapping(vec![
            ("dim_a", ColumnMappingValue::Simple("col_a".into())),
            (
                "dim_b",
                ColumnMappingValue::Literal(LiteralValue::String("val".into())),
            ),
        ]);

        let resolved = ResolvedColumnMapping::from_column_mapping(&mapping);

        assert!(resolved.contains_key("dim_a"));
        assert!(resolved.contains_key("dim_b"));
        assert!(!resolved.contains_key("dim_c"));
    }

    fn make_test_dimension(name: &str) -> (String, CompiledDimension) {
        (
            name.to_string(),
            CompiledDimension {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(CategoricalDimension { enum_values: None }),
            },
        )
    }

    fn make_test_measure(name: &str) -> (String, CompiledMeasure) {
        (
            name.to_string(),
            CompiledMeasure {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Float64,
                agg: None,
                expr: semstrait_core::Expr::EntityRef(semstrait_core::expr::EntityRef {
                    name: name.to_string(),
                }),
                expr_source: name.to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        )
    }

    #[test]
    fn test_coverage_index_build() {
        let dimensions: IndexMap<String, CompiledDimension> =
            vec![make_test_dimension("date"), make_test_dimension("region")]
                .into_iter()
                .collect();

        let measures: IndexMap<String, CompiledMeasure> = vec![make_test_measure("revenue")]
            .into_iter()
            .collect();

        let binding_full = DatasetBinding {
            dataset_name: "ds_full".into(),
            column_mapping: ResolvedColumnMapping::from_column_mapping(&make_explicit_mapping(
                vec![
                    ("date", ColumnMappingValue::Simple("order_date".into())),
                    ("region", ColumnMappingValue::Simple("region_name".into())),
                    ("revenue", ColumnMappingValue::Simple("amount".into())),
                ],
            )),
            resolved_sources: vec![],
        };

        let binding_partial = DatasetBinding {
            dataset_name: "ds_partial".into(),
            column_mapping: ResolvedColumnMapping::from_column_mapping(&make_explicit_mapping(
                vec![
                    ("date", ColumnMappingValue::Simple("sale_date".into())),
                    ("revenue", ColumnMappingValue::Simple("total".into())),
                ],
            )),
            resolved_sources: vec![],
        };

        let coverage =
            CoverageIndex::build(&dimensions, &measures, &[binding_full, binding_partial]);

        assert_eq!(coverage.field_names.len(), 3);

        // ds_full covers all 3 fields
        let mask_all = coverage.build_field_mask(&["date", "region", "revenue"]);
        assert!(mask_all.is_subset(&coverage.dataset_bitmaps[0]));

        // ds_partial does NOT cover region
        assert!(!mask_all.is_subset(&coverage.dataset_bitmaps[1]));

        // ds_partial covers date + revenue
        let mask_partial = coverage.build_field_mask(&["date", "revenue"]);
        assert!(mask_partial.is_subset(&coverage.dataset_bitmaps[1]));
    }

    #[test]
    fn test_dimension_index_build() {
        let temporal_dim = CompiledDimension {
            name: "date".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Date32,
            dim_type: DimensionType::Temporal(semstrait_model::TemporalDimension {
                grains: vec![TemporalGrain::Day],
            }),
        };

        let meta_dim = CompiledDimension {
            name: "platform".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Utf8,
            dim_type: DimensionType::Metadata(MetadataDimension {
                path: Some(semstrait_model::PathExtraction { token: 1 }),
                partition: None,
            }),
        };

        let cat_dim = CompiledDimension {
            name: "region".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Utf8,
            dim_type: DimensionType::Categorical(CategoricalDimension { enum_values: None }),
        };

        let dimensions: IndexMap<String, CompiledDimension> = vec![
            ("date".to_string(), temporal_dim),
            ("platform".to_string(), meta_dim),
            ("region".to_string(), cat_dim),
        ]
        .into_iter()
        .collect();

        let binding = DatasetBinding {
            dataset_name: "ds1".into(),
            column_mapping: ResolvedColumnMapping::from_column_mapping(&make_explicit_mapping(
                vec![
                    ("date", ColumnMappingValue::Simple("order_date".into())),
                    ("region", ColumnMappingValue::Simple("region_name".into())),
                ],
            )),
            resolved_sources: vec![],
        };

        let dim_index = DimensionIndex::build(&dimensions, &[binding]);

        assert_eq!(dim_index.temporal, vec!["date".to_string()]);
        assert_eq!(dim_index.metadata.len(), 1);
        assert_eq!(dim_index.metadata[0].0, "platform");
        assert_eq!(dim_index.categorical, vec!["region".to_string()]);
    }

    #[test]
    fn test_adjacency_index_build() {
        let bindings = vec![
            DatasetBinding {
                dataset_name: "orders".into(),
                column_mapping: ResolvedColumnMapping::from_column_mapping(
                    &make_explicit_mapping(vec![]),
                ),
                resolved_sources: vec![],
            },
            DatasetBinding {
                dataset_name: "customers".into(),
                column_mapping: ResolvedColumnMapping::from_column_mapping(
                    &make_explicit_mapping(vec![]),
                ),
                resolved_sources: vec![],
            },
            DatasetBinding {
                dataset_name: "products".into(),
                column_mapping: ResolvedColumnMapping::from_column_mapping(
                    &make_explicit_mapping(vec![]),
                ),
                resolved_sources: vec![],
            },
        ];

        let relationships = vec![
            CompiledRelationship {
                name: "orders_to_customers".into(),
                from: "orders".into(),
                to: "customers".into(),
                join_type: semstrait_model::JoinType::Inner,
                columns: vec![],
                cardinality: semstrait_model::Cardinality::ManyToOne,
            },
            CompiledRelationship {
                name: "orders_to_products".into(),
                from: "orders".into(),
                to: "products".into(),
                join_type: semstrait_model::JoinType::Inner,
                columns: vec![],
                cardinality: semstrait_model::Cardinality::ManyToOne,
            },
        ];

        let adj = AdjacencyIndex::build(&bindings, &relationships);

        // orders -> customers (rel 0), orders -> products (rel 1)
        assert_eq!(adj.forward[0].len(), 2);
        assert_eq!(adj.reverse[1].len(), 1); // customers <- orders
        assert_eq!(adj.reverse[2].len(), 1); // products <- orders

        assert_eq!(*adj.dataset_index.get("orders").unwrap(), 0);
        assert_eq!(*adj.dataset_index.get("customers").unwrap(), 1);
    }

    #[test]
    fn test_grain_map_build() {
        let bindings = vec![
            DatasetBinding {
                dataset_name: "daily".into(),
                column_mapping: ResolvedColumnMapping {
                    physical: IndexMap::from([("date".into(), "event_date".into())]),
                    literals: HashMap::new(),
                    temporal: HashMap::from([(
                        "date".into(),
                        TemporalMapping {
                            physical_column: "event_date".into(),
                            grain: Some(TemporalGrain::Day),
                        },
                    )]),
                    anchored: HashMap::new(),
                },
                resolved_sources: vec![],
            },
            DatasetBinding {
                dataset_name: "monthly".into(),
                column_mapping: ResolvedColumnMapping {
                    physical: IndexMap::from([("date".into(), "month_date".into())]),
                    literals: HashMap::new(),
                    temporal: HashMap::from([(
                        "date".into(),
                        TemporalMapping {
                            physical_column: "month_date".into(),
                            grain: Some(TemporalGrain::Month),
                        },
                    )]),
                    anchored: HashMap::new(),
                },
                resolved_sources: vec![],
            },
        ];

        let grain_map = GrainMap::build("date", &bindings);

        assert_eq!(grain_map.temporal_dim, "date");
        assert_eq!(grain_map.groups.len(), 2);
        // Month should come before Day (coarsest first)
        assert_eq!(grain_map.groups[0].0, TemporalGrain::Month);
        assert_eq!(grain_map.groups[1].0, TemporalGrain::Day);
    }

    #[test]
    #[should_panic(expected = "Auto mapping must be expanded")]
    fn test_resolved_column_mapping_panics_on_auto() {
        ResolvedColumnMapping::from_column_mapping(&ColumnMapping::Auto);
    }

    #[test]
    #[should_panic(expected = "Inherited mapping must be resolved")]
    fn test_resolved_column_mapping_panics_on_inherited() {
        ResolvedColumnMapping::from_column_mapping(&ColumnMapping::Inherited);
    }

    #[test]
    fn test_grain_map_with_no_grain_dataset() {
        let bindings = vec![
            DatasetBinding {
                dataset_name: "daily".into(),
                column_mapping: ResolvedColumnMapping {
                    physical: IndexMap::from([("date".into(), "event_date".into())]),
                    literals: HashMap::new(),
                    temporal: HashMap::from([(
                        "date".into(),
                        TemporalMapping {
                            physical_column: "event_date".into(),
                            grain: Some(TemporalGrain::Day),
                        },
                    )]),
                    anchored: HashMap::new(),
                },
                resolved_sources: vec![],
            },
            DatasetBinding {
                dataset_name: "no_grain".into(),
                column_mapping: ResolvedColumnMapping {
                    physical: IndexMap::from([("date".into(), "some_date".into())]),
                    literals: HashMap::new(),
                    temporal: HashMap::from([(
                        "date".into(),
                        TemporalMapping {
                            physical_column: "some_date".into(),
                            grain: None,
                        },
                    )]),
                    anchored: HashMap::new(),
                },
                resolved_sources: vec![],
            },
        ];

        let grain_map = GrainMap::build("date", &bindings);

        // Only 1 group (Day), the no-grain dataset is excluded from groups
        assert_eq!(grain_map.groups.len(), 1);
        assert_eq!(grain_map.groups[0].0, TemporalGrain::Day);
        assert_eq!(grain_map.groups[0].1, vec![0]); // only first binding

        // But present in dataset_grains
        assert_eq!(grain_map.dataset_grains[0], Some(TemporalGrain::Day));
        assert_eq!(grain_map.dataset_grains[1], None);
    }

    #[test]
    fn test_coverage_index_json_roundtrip() {
        let dimensions: IndexMap<String, CompiledDimension> =
            vec![make_test_dimension("date")].into_iter().collect();
        let measures: IndexMap<String, CompiledMeasure> =
            vec![make_test_measure("revenue")].into_iter().collect();

        let binding = DatasetBinding {
            dataset_name: "ds1".into(),
            column_mapping: ResolvedColumnMapping::from_column_mapping(&make_explicit_mapping(
                vec![
                    ("date", ColumnMappingValue::Simple("order_date".into())),
                    ("revenue", ColumnMappingValue::Simple("amount".into())),
                ],
            )),
            resolved_sources: vec![],
        };

        let coverage = CoverageIndex::build(&dimensions, &measures, &[binding]);

        // Serialize to JSON and back
        let json = serde_json::to_string(&coverage).expect("serialize");
        let restored: CoverageIndex = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(coverage.field_names, restored.field_names);
        assert_eq!(coverage.field_positions, restored.field_positions);
        assert_eq!(coverage.dataset_bitmaps.len(), restored.dataset_bitmaps.len());
        assert_eq!(coverage.dataset_bitmaps[0], restored.dataset_bitmaps[0]);
    }

    #[test]
    fn test_build_field_mask_ignores_unknown() {
        let dimensions: IndexMap<String, CompiledDimension> =
            vec![make_test_dimension("date")].into_iter().collect();
        let measures: IndexMap<String, CompiledMeasure> =
            vec![make_test_measure("revenue")].into_iter().collect();

        let binding = DatasetBinding {
            dataset_name: "ds1".into(),
            column_mapping: ResolvedColumnMapping::from_column_mapping(&make_explicit_mapping(
                vec![
                    ("date", ColumnMappingValue::Simple("order_date".into())),
                    ("revenue", ColumnMappingValue::Simple("amount".into())),
                ],
            )),
            resolved_sources: vec![],
        };

        let coverage = CoverageIndex::build(&dimensions, &measures, &[binding]);

        // "unknown_field" should be silently ignored
        let mask = coverage.build_field_mask(&["date", "unknown_field"]);
        assert!(mask.is_subset(&coverage.dataset_bitmaps[0]));
    }

    #[test]
    fn test_semantic_keys_excludes_anchored_subnames() {
        let mut sub_map = HashMap::new();
        sub_map.insert("order_sum".into(), "phys_order".into());
        sub_map.insert("delivery".into(), "phys_delivery".into());

        let mapping = make_explicit_mapping(vec![
            ("date", ColumnMappingValue::Simple("order_date".into())),
            ("total_cost", ColumnMappingValue::Anchored(sub_map)),
        ]);

        let resolved = ResolvedColumnMapping::from_column_mapping(&mapping);
        let keys = resolved.semantic_keys();

        // Should contain "date" and "total_cost" but NOT "order_sum" or "delivery"
        let key_strs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
        assert!(key_strs.contains(&"date"));
        assert!(key_strs.contains(&"total_cost"));
        assert!(!key_strs.contains(&"order_sum"));
        assert!(!key_strs.contains(&"delivery"));
    }

    // ================================================================
    // MetricOrder tests
    // ================================================================

    fn make_test_metric(name: &str, expr: semstrait_core::Expr, depth: usize) -> (String, CompiledMetric) {
        (
            name.to_string(),
            CompiledMetric {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Float64,
                agg: None,
                expr,
                expr_source: String::new(),
                additivity: None,
                constraints: None,
                filters: vec![],
                depth,
            },
        )
    }

    fn entity_ref(name: &str) -> semstrait_core::Expr {
        semstrait_core::Expr::EntityRef(semstrait_core::expr::EntityRef {
            name: name.to_string(),
        })
    }

    fn bin_divide(left: semstrait_core::Expr, right: semstrait_core::Expr) -> semstrait_core::Expr {
        semstrait_core::Expr::BinaryOp(semstrait_core::expr::BinaryExpr {
            left: Box::new(left),
            op: semstrait_core::expr::BinaryOp::Divide,
            right: Box::new(right),
        })
    }

    #[test]
    fn test_metric_order_empty_metrics() {
        let metrics: IndexMap<String, CompiledMetric> = IndexMap::new();
        let measures: IndexMap<String, CompiledMeasure> = vec![make_test_measure("revenue")].into_iter().collect();
        assert!(MetricOrder::build(&metrics, &measures).is_none());
    }

    #[test]
    fn test_metric_order_simple_metric() {
        // aov = revenue / order_count (both are measures)
        let measures: IndexMap<String, CompiledMeasure> = vec![
            make_test_measure("revenue"),
            make_test_measure("order_count"),
        ]
        .into_iter()
        .collect();

        let metrics: IndexMap<String, CompiledMetric> = vec![make_test_metric(
            "aov",
            bin_divide(entity_ref("revenue"), entity_ref("order_count")),
            1,
        )]
        .into_iter()
        .collect();

        let order = MetricOrder::build(&metrics, &measures).unwrap();
        assert_eq!(order.evaluation_order, vec!["aov"]);
        let aov_measures = order.metric_measures.get("aov").unwrap();
        assert_eq!(aov_measures.len(), 2);
        assert!(aov_measures.contains(&"revenue".to_string()));
        assert!(aov_measures.contains(&"order_count".to_string()));
    }

    #[test]
    fn test_metric_order_transitive_metric() {
        // revenue and order_count are measures
        // aov = revenue / order_count (depth 1, references measures)
        // aov_premium = aov * 1.2 (depth 2, references aov metric)
        let measures: IndexMap<String, CompiledMeasure> = vec![
            make_test_measure("revenue"),
            make_test_measure("order_count"),
        ]
        .into_iter()
        .collect();

        let metrics: IndexMap<String, CompiledMetric> = vec![
            make_test_metric(
                "aov",
                bin_divide(entity_ref("revenue"), entity_ref("order_count")),
                1,
            ),
            make_test_metric(
                "aov_premium",
                semstrait_core::Expr::BinaryOp(semstrait_core::expr::BinaryExpr {
                    left: Box::new(entity_ref("aov")),
                    op: semstrait_core::expr::BinaryOp::Multiply,
                    right: Box::new(semstrait_core::Expr::Literal(
                        semstrait_core::expr::Literal::Float { value: 1.2 },
                    )),
                }),
                2,
            ),
        ]
        .into_iter()
        .collect();

        let order = MetricOrder::build(&metrics, &measures).unwrap();

        // Depth order: aov (1) before aov_premium (2)
        assert_eq!(order.evaluation_order, vec!["aov", "aov_premium"]);

        // aov references revenue + order_count directly
        let aov_measures = order.metric_measures.get("aov").unwrap();
        assert!(aov_measures.contains(&"revenue".to_string()));
        assert!(aov_measures.contains(&"order_count".to_string()));

        // aov_premium transitively needs revenue + order_count (through aov)
        let premium_measures = order.metric_measures.get("aov_premium").unwrap();
        assert!(premium_measures.contains(&"revenue".to_string()));
        assert!(premium_measures.contains(&"order_count".to_string()));
    }

    #[test]
    fn test_metric_order_depth_sorting() {
        let measures: IndexMap<String, CompiledMeasure> = vec![
            make_test_measure("a"),
            make_test_measure("b"),
        ]
        .into_iter()
        .collect();

        // Insert in reverse depth order to test sorting
        let metrics: IndexMap<String, CompiledMetric> = vec![
            make_test_metric("m3", entity_ref("m1"), 3),
            make_test_metric("m1", entity_ref("a"), 1),
            make_test_metric("m2", entity_ref("b"), 2),
        ]
        .into_iter()
        .collect();

        let order = MetricOrder::build(&metrics, &measures).unwrap();
        assert_eq!(order.evaluation_order, vec!["m1", "m2", "m3"]);
    }
}
