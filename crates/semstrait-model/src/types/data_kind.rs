//! DataKind types — the unified Simple/Complex entity enum and supporting types.

use serde::{Deserialize, Serialize};

use super::common::{
    build_semantic_interface, AiContext, ColumnMapping, MeasureFilter, SemanticInterface,
};
use super::dimension::DimensionEntry;
use super::keys::Keys;
use super::measure::MeasureEntry;
use super::metric::MetricEntry;
use super::relationship::JoinRelationship;
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
    pub datasets: Vec<ChildEntry>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<ComplexExtras>,
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
    pub datasets: Vec<ChildEntry>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<ComplexExtras>,
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
    pub datasets: Vec<ChildEntry>,
    #[serde(default)]
    pub relationships: Vec<JoinRelationship>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<ComplexExtras>,
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
        Ok(DataKind::Complex(ComplexDataKind::Grainset(GrainsetSpec {
            name: g.name.clone(),
            interface: build_semantic_interface(
                &g.name, g.description, g.ai_context, g.keys, g.dimensions, g.measures, g.metrics, g.filters,
            )?,
            children: g.datasets,
            extras: g.extras,
        })))
    }
}

impl TryFrom<YamlUnionset> for DataKind {
    type Error = crate::ModelError;
    fn try_from(u: YamlUnionset) -> Result<Self, Self::Error> {
        Ok(DataKind::Complex(ComplexDataKind::Unionset(UnionsetSpec {
            name: u.name.clone(),
            interface: build_semantic_interface(
                &u.name, u.description, u.ai_context, u.keys, u.dimensions, u.measures, u.metrics, u.filters,
            )?,
            mode: u.mode,
            children: u.datasets,
            extras: u.extras,
        })))
    }
}

impl TryFrom<YamlJoinset> for DataKind {
    type Error = crate::ModelError;
    fn try_from(j: YamlJoinset) -> Result<Self, Self::Error> {
        Ok(DataKind::Complex(ComplexDataKind::Joinset(JoinsetSpec {
            name: j.name.clone(),
            interface: build_semantic_interface(
                &j.name, j.description, j.ai_context, j.keys, j.dimensions, j.measures, j.metrics, j.filters,
            )?,
            associativity: j.associativity,
            children: j.datasets,
            relationships: j.relationships,
            extras: j.extras,
        })))
    }
}

// =============================================================================
// DataKind — unified Simple/Complex entity enum
// =============================================================================

/// Unified entity type in the semantic model.
///
/// `Simple` is the fundamental leaf building block — a singular queryable unit
/// (collection of files/tables sharing the same semantic structure).
/// `Complex` composes multiple children (Simple or other Complex) via strategy
/// (grain partitioning, union, join).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "data_kind")]
pub enum DataKind {
    Simple(SimpleDataKind),
    Complex(ComplexDataKind),
}

/// A standalone queryable dataset — the fundamental leaf building block.
/// A singular queryable unit: collection of files/tables sharing the same
/// semantic structure. Can be safely UNION ALL'd. No nesting, no children.
#[derive(Debug, Clone, Serialize)]
pub struct SimpleDataKind {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    #[serde(default)]
    pub extras: Option<DatasetExtras>,
}

/// Composite entity that composes children via a specific strategy.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "complex_kind")]
pub enum ComplexDataKind {
    Grainset(GrainsetSpec),
    Unionset(UnionsetSpec),
    Joinset(JoinsetSpec),
}

/// A grain-partitioned entity: each child covers a different temporal grain.
#[derive(Debug, Clone, Serialize)]
pub struct GrainsetSpec {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    pub children: Vec<ChildEntry>,
    #[serde(default)]
    pub extras: Option<ComplexExtras>,
}

/// A union entity: children are combined via UNION ALL or UNION DISTINCT.
#[derive(Debug, Clone, Serialize)]
pub struct UnionsetSpec {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    #[serde(default)]
    pub mode: UnionMode,
    pub children: Vec<ChildEntry>,
    #[serde(default)]
    pub extras: Option<ComplexExtras>,
}

/// A join entity: children are joined via relationships.
#[derive(Debug, Clone, Serialize)]
pub struct JoinsetSpec {
    pub name: String,
    #[serde(flatten)]
    pub interface: SemanticInterface,
    #[serde(default = "default_join_associativity")]
    pub associativity: JoinAssociativity,
    pub children: Vec<ChildEntry>,
    pub relationships: Vec<JoinRelationship>,
    #[serde(default)]
    pub extras: Option<ComplexExtras>,
}

impl ComplexDataKind {
    /// Entity name.
    pub fn name(&self) -> &str {
        match self {
            ComplexDataKind::Grainset(g) => &g.name,
            ComplexDataKind::Unionset(u) => &u.name,
            ComplexDataKind::Joinset(j) => &j.name,
        }
    }

    /// Shared semantic interface.
    pub fn interface(&self) -> &SemanticInterface {
        match self {
            ComplexDataKind::Grainset(g) => &g.interface,
            ComplexDataKind::Unionset(u) => &u.interface,
            ComplexDataKind::Joinset(j) => &j.interface,
        }
    }

    /// Mutable access to the semantic interface.
    pub fn interface_mut(&mut self) -> &mut SemanticInterface {
        match self {
            ComplexDataKind::Grainset(g) => &mut g.interface,
            ComplexDataKind::Unionset(u) => &mut u.interface,
            ComplexDataKind::Joinset(j) => &mut j.interface,
        }
    }

    /// Child entries (inline datasets and refs).
    pub fn children(&self) -> &[ChildEntry] {
        match self {
            ComplexDataKind::Grainset(g) => &g.children,
            ComplexDataKind::Unionset(u) => &u.children,
            ComplexDataKind::Joinset(j) => &j.children,
        }
    }

    /// Mutable child entries.
    pub fn children_mut(&mut self) -> &mut Vec<ChildEntry> {
        match self {
            ComplexDataKind::Grainset(g) => &mut g.children,
            ComplexDataKind::Unionset(u) => &mut u.children,
            ComplexDataKind::Joinset(j) => &mut j.children,
        }
    }

    /// Complex-level extras.
    pub fn extras(&self) -> Option<&ComplexExtras> {
        match self {
            ComplexDataKind::Grainset(g) => g.extras.as_ref(),
            ComplexDataKind::Unionset(u) => u.extras.as_ref(),
            ComplexDataKind::Joinset(j) => j.extras.as_ref(),
        }
    }

    /// Relationships (only JoinsetSpec has non-empty; others return empty slice).
    pub fn relationships(&self) -> &[JoinRelationship] {
        match self {
            ComplexDataKind::Joinset(j) => &j.relationships,
            _ => &[],
        }
    }

    /// Variant discriminant.
    pub fn variant(&self) -> DataKindVariant {
        match self {
            ComplexDataKind::Grainset(_) => DataKindVariant::Grainset,
            ComplexDataKind::Unionset(_) => DataKindVariant::Unionset,
            ComplexDataKind::Joinset(_) => DataKindVariant::Joinset,
        }
    }
}

impl DataKind {
    /// Entity name.
    pub fn name(&self) -> &str {
        match self {
            DataKind::Simple(d) => &d.name,
            DataKind::Complex(c) => c.name(),
        }
    }

    /// Shared semantic interface (dimensions, measures, metrics, filters, keys).
    pub fn interface(&self) -> &SemanticInterface {
        match self {
            DataKind::Simple(d) => &d.interface,
            DataKind::Complex(c) => c.interface(),
        }
    }

    /// Mutable access to the semantic interface.
    pub fn interface_mut(&mut self) -> &mut SemanticInterface {
        match self {
            DataKind::Simple(d) => &mut d.interface,
            DataKind::Complex(c) => c.interface_mut(),
        }
    }

    /// Child entries (None for Simple).
    pub fn children(&self) -> Option<&[ChildEntry]> {
        match self {
            DataKind::Simple(_) => None,
            DataKind::Complex(c) => Some(c.children()),
        }
    }

    /// Mutable child entries (None for Simple).
    pub fn children_mut(&mut self) -> Option<&mut Vec<ChildEntry>> {
        match self {
            DataKind::Simple(_) => None,
            DataKind::Complex(c) => Some(c.children_mut()),
        }
    }

    /// Relationships (only JoinsetSpec has non-empty; others return empty slice).
    pub fn relationships(&self) -> &[JoinRelationship] {
        match self {
            DataKind::Complex(c) => c.relationships(),
            _ => &[],
        }
    }

    /// Complex-level extras (None for Simple).
    pub fn complex_extras(&self) -> Option<&ComplexExtras> {
        match self {
            DataKind::Simple(_) => None,
            DataKind::Complex(c) => c.extras(),
        }
    }

    /// True if this is a standalone simple dataset (no children).
    pub fn is_simple(&self) -> bool {
        matches!(self, DataKind::Simple(_))
    }

    /// True if this is a complex composite entity.
    pub fn is_complex(&self) -> bool {
        matches!(self, DataKind::Complex(_))
    }

    /// True if this is a joinset.
    pub fn is_joinset(&self) -> bool {
        matches!(self, DataKind::Complex(ComplexDataKind::Joinset(_)))
    }

    /// Variant name for error messages.
    pub fn variant(&self) -> &'static str {
        self.variant_enum().as_str()
    }

    pub fn variant_enum(&self) -> DataKindVariant {
        match self {
            DataKind::Simple(_) => DataKindVariant::Simple,
            DataKind::Complex(c) => c.variant(),
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
// Child Entry
// =============================================================================

/// A child entry in a complex kind: either inline definition or ref to another entity.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum ChildEntry {
    Ref(ChildRef),
    Inline(InlineDataset),
}

/// Physical-only dataset nested in a complex kind (SR-4 enforced by deny_unknown_fields).
/// Has NO interface — inherits from parent complex kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InlineDataset {
    pub name: DatasetName,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    pub extras: InlineDatasetExtras,
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
pub struct InlineDatasetExtras {
    #[serde(default = "ColumnMapping::default_inherited")]
    pub column_mapping: ColumnMapping,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
}

/// Complex-level default extras applied to all children in this entity.
/// Per-child extras (InlineDataset.extras) override these defaults field by field.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ComplexExtras {
    #[serde(default)]
    pub column_mapping: Option<ColumnMapping>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
    #[serde(default)]
    pub partition_defs: Option<Vec<super::storage::PartitionDef>>,
}

/// Extras for standalone SimpleDataKind (no complex-level defaults).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetExtras {
    #[serde(default = "ColumnMapping::default_auto")]
    pub column_mapping: ColumnMapping,
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
}

// =============================================================================
// ChildRef / DataKindVariant
// =============================================================================

/// Typed reference to another top-level entity.
///
/// Unlike `RefEntry` (used for dimension/measure/metric refs), `ChildRef`
/// carries the variant of the referenced entity, eliminating string-based
/// lookups during nesting validation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChildRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// The variant of the referenced target.
    /// Set programmatically during `flatten_*` in parse.rs.
    #[serde(skip)]
    pub variant: DataKindVariant,
}

/// The variant discriminant for DataKind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DataKindVariant {
    #[default]
    Simple,
    Grainset,
    Unionset,
    Joinset,
}

impl DataKindVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataKindVariant::Simple => "dataset",
            DataKindVariant::Grainset => "grainset",
            DataKindVariant::Unionset => "unionset",
            DataKindVariant::Joinset => "joinset",
        }
    }
}

impl ChildRef {
    pub fn new(ref_name: String, variant: DataKindVariant) -> Self {
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
    fn test_child_ref_carries_variant() {
        let r = ChildRef::new("my_unionset".to_string(), DataKindVariant::Unionset);
        assert_eq!(r.name(), "my_unionset");
        assert_eq!(r.variant, DataKindVariant::Unionset);
    }

    #[test]
    fn test_child_ref_name_accessor() {
        let r = ChildRef::new("sales_grain".to_string(), DataKindVariant::Grainset);
        assert_eq!(r.name(), "sales_grain");
    }

    #[test]
    fn test_variant_as_str() {
        assert_eq!(DataKindVariant::Simple.as_str(), "dataset");
        assert_eq!(DataKindVariant::Grainset.as_str(), "grainset");
        assert_eq!(DataKindVariant::Unionset.as_str(), "unionset");
        assert_eq!(DataKindVariant::Joinset.as_str(), "joinset");
    }

    #[test]
    fn test_variant_default_is_simple() {
        assert_eq!(DataKindVariant::default(), DataKindVariant::Simple);
    }

    #[test]
    fn test_child_ref_serde_roundtrip() {
        let r = ChildRef::new("test_kind".to_string(), DataKindVariant::Joinset);
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("test_kind"));

        let deserialized: ChildRef = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.ref_name, "test_kind");
        // Variant defaults to Simple after deserialization (serde skip)
        assert_eq!(deserialized.variant, DataKindVariant::Simple);
    }
}
