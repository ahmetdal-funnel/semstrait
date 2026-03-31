//! DataKind types — the unified 4-variant entity enum and supporting types.

use serde::{Deserialize, Serialize};

use super::common::{
    build_semantic_interface, AiContext, ColumnMapping, MeasureFilter, SemanticInterface,
};
use super::dimension::DimensionEntry;
use super::keys::Keys;
use super::measure::MeasureEntry;
use super::metric::MetricEntry;
use super::relationship::DataKindRelationship;
use super::storage::{CatalogRef, StorageConfig};
use super::temporal::TemporalConfig;

// =============================================================================
// YAML-facing kind types (implicit type from array membership)
// =============================================================================

/// Grainset kind in YAML — type is implicit from being in `grainsets:` array.
/// Nesting matrix: grainset can contain unionsets and joinsets (NOT grainsets).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YamlGrainset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<DataKindEntry>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<DataKindExtras>,
    /// Nested unionset kinds (flattened to top-level during parse).
    #[serde(default)]
    pub unionsets: Vec<YamlUnionset>,
    /// Nested joinset kinds (flattened to top-level during parse).
    #[serde(default)]
    pub joinsets: Vec<YamlJoinset>,
}

/// Unionset kind in YAML — type is implicit, `mode` inlined.
/// Nesting matrix: unionset can contain grainsets, unionsets (with warning), and joinsets.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YamlUnionset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    #[serde(default)]
    pub mode: UnionMode,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<DataKindEntry>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<DataKindExtras>,
    /// Nested grainset kinds (flattened to top-level during parse).
    #[serde(default)]
    pub grainsets: Vec<YamlGrainset>,
    /// Nested unionset kinds (flattened to top-level during parse; emits COMP_W010).
    #[serde(default)]
    pub unionsets: Vec<YamlUnionset>,
    /// Nested joinset kinds (flattened to top-level during parse).
    #[serde(default)]
    pub joinsets: Vec<YamlJoinset>,
}

/// Joinset kind in YAML — type is implicit, `associativity` inlined.
/// Nesting matrix: joinset can contain grainsets and unionsets (NOT joinsets).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YamlJoinset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    #[serde(default = "default_join_associativity")]
    pub associativity: JoinAssociativity,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<DataKindEntry>,
    #[serde(default)]
    pub relationships: Vec<DataKindRelationship>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<DataKindExtras>,
    /// Nested grainset kinds (flattened to top-level during parse).
    #[serde(default)]
    pub grainsets: Vec<YamlGrainset>,
    /// Nested unionset kinds (flattened to top-level during parse).
    #[serde(default)]
    pub unionsets: Vec<YamlUnionset>,
}

impl TryFrom<YamlGrainset> for DataKind {
    type Error = crate::ModelError;
    fn try_from(g: YamlGrainset) -> Result<Self, Self::Error> {
        Ok(DataKind::Grainset(GrainsetKind {
            name: g.name.clone(),
            interface: build_semantic_interface(
                &g.name, g.description, g.ai_context, g.keys, g.dimensions, g.measures, g.metrics, g.filters,
            )?,
            datasets: g.datasets,
            extras: g.extras,
        }))
    }
}

impl TryFrom<YamlUnionset> for DataKind {
    type Error = crate::ModelError;
    fn try_from(u: YamlUnionset) -> Result<Self, Self::Error> {
        Ok(DataKind::Unionset(UnionsetKind {
            name: u.name.clone(),
            interface: build_semantic_interface(
                &u.name, u.description, u.ai_context, u.keys, u.dimensions, u.measures, u.metrics, u.filters,
            )?,
            mode: u.mode,
            datasets: u.datasets,
            extras: u.extras,
        }))
    }
}

impl TryFrom<YamlJoinset> for DataKind {
    type Error = crate::ModelError;
    fn try_from(j: YamlJoinset) -> Result<Self, Self::Error> {
        Ok(DataKind::Joinset(JoinsetKind {
            name: j.name.clone(),
            interface: build_semantic_interface(
                &j.name, j.description, j.ai_context, j.keys, j.dimensions, j.measures, j.metrics, j.filters,
            )?,
            associativity: j.associativity,
            datasets: j.datasets,
            relationships: j.relationships,
            extras: j.extras,
        }))
    }
}

// =============================================================================
// DataKind — unified 4-variant enum for all entity types
// =============================================================================

/// Unified entity type in the semantic model.
///
/// Each variant carries its own struct with `name`, `interface`, and
/// variant-specific fields.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "data_kind")]
pub enum DataKind {
    Dataset(DatasetKind),
    Grainset(GrainsetKind),
    Unionset(UnionsetKind),
    Joinset(JoinsetKind),
}

/// A standalone queryable dataset with dimensions, measures, and metrics.
#[derive(Debug, Clone, Serialize)]
pub struct DatasetKind {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    #[serde(default)]
    pub extras: Option<DatasetExtras>,
}

/// A grain-partitioned kind: each child dataset covers a different temporal grain.
#[derive(Debug, Clone, Serialize)]
pub struct GrainsetKind {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    pub datasets: Vec<DataKindEntry>,
    #[serde(default)]
    pub extras: Option<DataKindExtras>,
}

/// A union kind: child datasets are combined via UNION ALL or UNION DISTINCT.
#[derive(Debug, Clone, Serialize)]
pub struct UnionsetKind {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    #[serde(default)]
    pub mode: UnionMode,
    pub datasets: Vec<DataKindEntry>,
    #[serde(default)]
    pub extras: Option<DataKindExtras>,
}

/// A join kind: child datasets are joined via relationships.
#[derive(Debug, Clone, Serialize)]
pub struct JoinsetKind {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    #[serde(default = "default_join_associativity")]
    pub associativity: JoinAssociativity,
    pub datasets: Vec<DataKindEntry>,
    pub relationships: Vec<DataKindRelationship>,
    #[serde(default)]
    pub extras: Option<DataKindExtras>,
}

impl DataKind {
    /// Entity name.
    pub fn name(&self) -> &str {
        match self {
            DataKind::Dataset(d) => &d.name,
            DataKind::Grainset(g) => &g.name,
            DataKind::Unionset(u) => &u.name,
            DataKind::Joinset(j) => &j.name,
        }
    }

    /// Shared semantic interface (dimensions, measures, metrics, filters, keys).
    pub fn interface(&self) -> &SemanticInterface {
        match self {
            DataKind::Dataset(d) => &d.interface,
            DataKind::Grainset(g) => &g.interface,
            DataKind::Unionset(u) => &u.interface,
            DataKind::Joinset(j) => &j.interface,
        }
    }

    /// Mutable access to the semantic interface.
    pub fn interface_mut(&mut self) -> &mut SemanticInterface {
        match self {
            DataKind::Dataset(d) => &mut d.interface,
            DataKind::Grainset(g) => &mut g.interface,
            DataKind::Unionset(u) => &mut u.interface,
            DataKind::Joinset(j) => &mut j.interface,
        }
    }

    /// Child datasets (None for standalone DatasetKind).
    pub fn children(&self) -> Option<&[DataKindEntry]> {
        match self {
            DataKind::Dataset(_) => None,
            DataKind::Grainset(g) => Some(&g.datasets),
            DataKind::Unionset(u) => Some(&u.datasets),
            DataKind::Joinset(j) => Some(&j.datasets),
        }
    }

    /// Mutable child datasets (None for standalone DatasetKind).
    pub fn children_mut(&mut self) -> Option<&mut Vec<DataKindEntry>> {
        match self {
            DataKind::Dataset(_) => None,
            DataKind::Grainset(g) => Some(&mut g.datasets),
            DataKind::Unionset(u) => Some(&mut u.datasets),
            DataKind::Joinset(j) => Some(&mut j.datasets),
        }
    }

    /// Relationships (only JoinsetKind has non-empty; others return empty slice).
    pub fn relationships(&self) -> &[DataKindRelationship] {
        match self {
            DataKind::Joinset(j) => &j.relationships,
            _ => &[],
        }
    }

    /// Kind-level extras (None for DatasetKind).
    pub fn kind_extras(&self) -> Option<&DataKindExtras> {
        match self {
            DataKind::Dataset(_) => None,
            DataKind::Grainset(g) => g.extras.as_ref(),
            DataKind::Unionset(u) => u.extras.as_ref(),
            DataKind::Joinset(j) => j.extras.as_ref(),
        }
    }

    /// True if this is a standalone dataset (no children).
    pub fn is_dataset(&self) -> bool {
        matches!(self, DataKind::Dataset(_))
    }

    /// True if this is a joinset.
    pub fn is_joinset(&self) -> bool {
        matches!(self, DataKind::Joinset(_))
    }

    /// Kind variant name for error messages.
    pub fn kind_variant(&self) -> &'static str {
        self.kind_variant_enum().as_str()
    }

    pub fn kind_variant_enum(&self) -> KindVariant {
        match self {
            DataKind::Dataset(_) => KindVariant::Dataset,
            DataKind::Grainset(_) => KindVariant::Grainset,
            DataKind::Unionset(_) => KindVariant::Unionset,
            DataKind::Joinset(_) => KindVariant::Joinset,
        }
    }
}

fn default_join_associativity() -> JoinAssociativity {
    JoinAssociativity::Left
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinAssociativity {
    Left,
    Right,
    Full,
}

/// UNION mode: ALL (default) or UNIQUE (distinct rows).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnionMode {
    #[default]
    All,
    Unique,
}

// =============================================================================
// Kind Dataset Entry
// =============================================================================

/// A dataset reference in a kind: either inline definition or ref to another kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum DataKindEntry {
    Ref(DataKindRef),
    Inline(DataKindBinding),
}

/// Dataset binding within a kind (SR-4: physical only — no semantic fields allowed).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataKindBinding {
    pub name: DatasetName,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    pub extras: DataKindBindingExtras,
}

/// Dataset name: either a literal string or a glob pattern.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum DatasetName {
    Literal(String),
    Glob(GlobPattern),
}

impl<'de> Deserialize<'de> for DatasetName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.contains('*') || s.contains('?') {
            Ok(DatasetName::Glob(GlobPattern(s)))
        } else {
            Ok(DatasetName::Literal(s))
        }
    }
}

/// A glob pattern for matching multiple datasets.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobPattern(pub String);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataKindBindingExtras {
    #[serde(default = "ColumnMapping::default_inherited")]
    pub column_mapping: ColumnMapping,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
}

/// Kind-level default extras applied to all datasets in this kind.
/// Per-dataset extras (DataKindBinding.extras) override these defaults field by field.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DataKindExtras {
    #[serde(default)]
    pub column_mapping: Option<ColumnMapping>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
    #[serde(default)]
    pub partition_defs: Option<Vec<super::storage::PartitionDef>>,
}

/// Extras for standalone datasets (no kind-level defaults).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetExtras {
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
}

// =============================================================================
// DataKindRef / KindVariant
// =============================================================================

/// Typed reference to a nested kind (Grainset, Unionset, or Joinset).
///
/// Unlike `RefEntry` (used for dimension/measure/metric refs), `DataKindRef`
/// carries the variant of the referenced kind, eliminating string-based
/// lookups during nesting validation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DataKindRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// The kind variant of the referenced target.
    /// Set programmatically during `flatten_*` in parse.rs.
    #[serde(skip)]
    pub variant: KindVariant,
}

/// The kind variant carried by a `DataKindRef`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KindVariant {
    #[default]
    Dataset,
    Grainset,
    Unionset,
    Joinset,
}

impl KindVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            KindVariant::Dataset => "dataset",
            KindVariant::Grainset => "grainset",
            KindVariant::Unionset => "unionset",
            KindVariant::Joinset => "joinset",
        }
    }
}

impl DataKindRef {
    pub fn new(ref_name: String, variant: KindVariant) -> Self {
        Self { ref_name, variant }
    }

    pub fn name(&self) -> &str {
        &self.ref_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_ref_carries_variant() {
        let r = DataKindRef::new("my_unionset".to_string(), KindVariant::Unionset);
        assert_eq!(r.name(), "my_unionset");
        assert_eq!(r.variant, KindVariant::Unionset);
    }

    #[test]
    fn test_kind_ref_name_accessor() {
        let r = DataKindRef::new("sales_grain".to_string(), KindVariant::Grainset);
        assert_eq!(r.name(), "sales_grain");
    }

    #[test]
    fn test_kind_variant_as_str() {
        assert_eq!(KindVariant::Dataset.as_str(), "dataset");
        assert_eq!(KindVariant::Grainset.as_str(), "grainset");
        assert_eq!(KindVariant::Unionset.as_str(), "unionset");
        assert_eq!(KindVariant::Joinset.as_str(), "joinset");
    }

    #[test]
    fn test_kind_variant_default_is_dataset() {
        assert_eq!(KindVariant::default(), KindVariant::Dataset);
    }

    #[test]
    fn test_kind_ref_serde_roundtrip() {
        // Use serde_yaml since it's already a dependency of this crate.
        let r = DataKindRef::new("test_kind".to_string(), KindVariant::Joinset);
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("test_kind"));

        let deserialized: DataKindRef = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.ref_name, "test_kind");
        // Variant defaults to Dataset after deserialization (serde skip)
        assert_eq!(deserialized.variant, KindVariant::Dataset);
    }
}
