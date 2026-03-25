//! Type definitions for semantic models.
//!
//! This module contains all the types that map to the YAML semantic model schema.
//! All types support serde serialization/deserialization.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

// =============================================================================
// Model-level DataType (YAML-facing, simple type names)
// =============================================================================

/// Data types supported in semantic model YAML definitions.
///
/// These are simple, user-facing type names that map to the YAML `data_type` field.
/// They are distinct from the Arrow-aligned `semstrait_core::DataType` used in IR plans.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DataType {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    #[default]
    String,
    Date,
    Timestamp,
    Decimal { precision: u8, scale: u8 },
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::I8 => write!(f, "i8"),
            DataType::I16 => write!(f, "i16"),
            DataType::I32 => write!(f, "i32"),
            DataType::I64 => write!(f, "i64"),
            DataType::F32 => write!(f, "f32"),
            DataType::F64 => write!(f, "f64"),
            DataType::Bool => write!(f, "bool"),
            DataType::String => write!(f, "string"),
            DataType::Date => write!(f, "date"),
            DataType::Timestamp => write!(f, "timestamp"),
            DataType::Decimal { precision, scale } => write!(f, "decimal({}, {})", precision, scale),
        }
    }
}

impl FromStr for DataType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        if lower.starts_with("decimal(") && lower.ends_with(')') {
            let inner = &lower[8..lower.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() != 2 {
                return Err(format!("invalid decimal type: '{}'", s));
            }
            let precision: u8 = parts[0].parse().map_err(|_| format!("invalid precision in '{}'", s))?;
            let scale: u8 = parts[1].parse().map_err(|_| format!("invalid scale in '{}'", s))?;
            return Ok(DataType::Decimal { precision, scale });
        }
        match lower.as_str() {
            "i8" => Ok(DataType::I8),
            "i16" => Ok(DataType::I16),
            "i32" | "int" | "integer" => Ok(DataType::I32),
            "i64" | "long" | "bigint" | "int64" => Ok(DataType::I64),
            "f32" | "float" | "float32" => Ok(DataType::F32),
            "f64" | "double" | "float64" => Ok(DataType::F64),
            "bool" | "boolean" => Ok(DataType::Bool),
            "string" | "text" | "varchar" => Ok(DataType::String),
            "date" => Ok(DataType::Date),
            "timestamp" | "datetime" => Ok(DataType::Timestamp),
            _ => Err(format!("unknown data type: '{}'", s)),
        }
    }
}

impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::string::String::deserialize(deserializer)?;
        DataType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// =============================================================================
// Top-level SemanticModel
// =============================================================================

/// Root semantic model definition.
///
/// Kinds are represented as three implicit-type arrays in YAML:
/// `grainsets:`, `unionsets:`, `joinsets:`. After parsing, they are merged
/// into `kinds: Vec<Kind>` for the rest of the pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticModel {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Catalog namespace for glob expansion (defaults to "default").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    // Top-level datasets
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<Dataset>,

    // Merged kinds (populated from grainsets/unionsets/joinsets during parse).
    // Serialized as separate arrays.
    #[serde(skip)]
    pub kinds: Vec<Kind>,

    // Top-level relationships between datasets and/or kinds
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,

    // Reusable definitions (referenced via `ref:` syntax)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Dimension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measures: Vec<Measure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<Metric>,
}

// =============================================================================
// YAML-facing kind types (implicit type from array membership)
// =============================================================================

/// Grainset kind in YAML — type is implicit from being in `grainsets:` array.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YamlGrainset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<KindDatasetEntry>,
    #[serde(default)]
    pub relationships: Vec<KindRelationship>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<KindExtras>,
}

/// Unionset kind in YAML — type is implicit, `mode` inlined.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YamlUnionset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub mode: UnionMode,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<KindDatasetEntry>,
    #[serde(default)]
    pub relationships: Vec<KindRelationship>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<KindExtras>,
}

/// Joinset kind in YAML — type is implicit, `associativity` inlined.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YamlJoinset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_join_associativity")]
    pub associativity: JoinAssociativity,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<KindDatasetEntry>,
    #[serde(default)]
    pub relationships: Vec<KindRelationship>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    #[serde(default)]
    pub extras: Option<KindExtras>,
}

impl From<YamlGrainset> for Kind {
    fn from(g: YamlGrainset) -> Self {
        Kind {
            name: g.name,
            description: g.description,
            kind_type: KindTypeSpec::Grainset,
            domain: g.domain,
            keys: g.keys,
            dimensions: g.dimensions,
            measures: g.measures,
            metrics: g.metrics,
            datasets: g.datasets,
            relationships: g.relationships,
            filters: g.filters,
            extras: g.extras,
        }
    }
}

impl From<YamlUnionset> for Kind {
    fn from(u: YamlUnionset) -> Self {
        Kind {
            name: u.name,
            description: u.description,
            kind_type: KindTypeSpec::Unionset(UnionsetConfig { mode: u.mode }),
            domain: u.domain,
            keys: u.keys,
            dimensions: u.dimensions,
            measures: u.measures,
            metrics: u.metrics,
            datasets: u.datasets,
            relationships: u.relationships,
            filters: u.filters,
            extras: u.extras,
        }
    }
}

impl From<YamlJoinset> for Kind {
    fn from(j: YamlJoinset) -> Self {
        Kind {
            name: j.name,
            description: j.description,
            kind_type: KindTypeSpec::Joinset(JoinsetConfig {
                associativity: j.associativity,
            }),
            domain: j.domain,
            keys: j.keys,
            dimensions: j.dimensions,
            measures: j.measures,
            metrics: j.metrics,
            datasets: j.datasets,
            relationships: j.relationships,
            filters: j.filters,
            extras: j.extras,
        }
    }
}

// =============================================================================
// Dataset
// =============================================================================

/// A queryable dataset with dimensions, measures, and metrics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dataset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    #[serde(default)]
    pub filters: Vec<DatasetFilter>,
    #[serde(default)]
    pub extras: Option<DatasetExtras>,
}

// =============================================================================
// Kind
// =============================================================================

/// A semantic abstraction over one or more datasets.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Kind {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub kind_type: KindTypeSpec,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Vec<DimensionEntry>,
    #[serde(default)]
    pub measures: Vec<MeasureEntry>,
    #[serde(default)]
    pub metrics: Vec<MetricEntry>,
    pub datasets: Vec<KindDatasetEntry>,
    #[serde(default)]
    pub relationships: Vec<KindRelationship>,
    /// Kind-level filters applied to all queries against this kind.
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
    /// Kind-level default extras inherited by all datasets in this kind.
    /// Per-dataset extras override these defaults field by field.
    #[serde(default)]
    pub extras: Option<KindExtras>,
}

/// Kind type specification (grainset, unionset, or joinset).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KindTypeSpec {
    Grainset,
    Unionset(UnionsetConfig),
    Joinset(JoinsetConfig),
}

impl<'de> Deserialize<'de> for KindTypeSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "KindTypeSpec must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "grainset" => Ok(KindTypeSpec::Grainset),
            "unionset" => {
                // Support both bare `unionset: {}` and `unionset: { mode: "unique" }`.
                if value.is_null() || (value.is_mapping() && value.as_mapping().is_none_or(|m| m.is_empty())) {
                    Ok(KindTypeSpec::Unionset(UnionsetConfig::default()))
                } else {
                    Ok(KindTypeSpec::Unionset(
                        serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
                    ))
                }
            }
            "joinset" => Ok(KindTypeSpec::Joinset(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["grainset", "unionset", "joinset"],
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinsetConfig {
    #[serde(default = "default_join_associativity")]
    pub associativity: JoinAssociativity,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnionsetConfig {
    #[serde(default)]
    pub mode: UnionMode,
}

impl Default for UnionsetConfig {
    fn default() -> Self {
        Self {
            mode: UnionMode::All,
        }
    }
}

// =============================================================================
// Kind Dataset Entry
// =============================================================================

/// A dataset reference in a kind: either inline definition or ref to another kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum KindDatasetEntry {
    Ref(RefEntry),
    Inline(KindDataset),
}

/// Dataset binding within a kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindDataset {
    pub name: DatasetName,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    pub extras: KindDatasetExtras,
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
pub struct KindDatasetExtras {
    #[serde(default = "ColumnMapping::default_inherited")]
    pub column_mapping: ColumnMapping,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogConfig>,
}

/// Kind-level default extras applied to all datasets in this kind.
/// Per-dataset extras (KindDataset.extras) override these defaults field by field.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindExtras {
    #[serde(default)]
    pub column_mapping: Option<ColumnMapping>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogConfig>,
    #[serde(default)]
    pub partition_defs: Option<Vec<PartitionDef>>,
}

/// Column mapping: either `auto`, `inherited`, or an explicit map.
#[derive(Debug, Clone)]
pub enum ColumnMapping {
    /// Auto-map: all kind interface names are matched 1:1 to physical columns.
    /// Expanded to `Explicit` identity mapping during compilation (step 4.5).
    Auto,
    /// Inherit from kind.extras.column_mapping. Resolved in step 4.5.
    /// This is the default when `column_mapping:` is absent from a dataset's extras.
    Inherited,
    /// Explicit mapping of semantic name → physical column.
    Explicit(HashMap<String, ColumnMappingValue>),
}

impl ColumnMapping {
    /// Create an explicit column mapping.
    pub fn explicit(map: HashMap<String, ColumnMappingValue>) -> Self {
        ColumnMapping::Explicit(map)
    }

    /// Returns true if this is `Auto`.
    pub fn is_auto(&self) -> bool {
        matches!(self, ColumnMapping::Auto)
    }

    /// Returns true if this is `Inherited`.
    pub fn is_inherited(&self) -> bool {
        matches!(self, ColumnMapping::Inherited)
    }

    /// Default value for `KindDatasetExtras.column_mapping` when the field is absent.
    /// Used by `#[serde(default = "ColumnMapping::default_inherited")]`.
    pub fn default_inherited() -> Self {
        ColumnMapping::Inherited
    }

    /// Get the underlying map. Panics if `Auto` or `Inherited` (must be expanded before use).
    pub fn as_map(&self) -> &HashMap<String, ColumnMappingValue> {
        match self {
            ColumnMapping::Explicit(m) => m,
            ColumnMapping::Auto | ColumnMapping::Inherited => {
                panic!("column_mapping must be expanded before use (call expand_auto_mappings first)")
            }
        }
    }
}

impl From<HashMap<String, ColumnMappingValue>> for ColumnMapping {
    fn from(map: HashMap<String, ColumnMappingValue>) -> Self {
        ColumnMapping::Explicit(map)
    }
}

impl std::ops::Deref for ColumnMapping {
    type Target = HashMap<String, ColumnMappingValue>;

    fn deref(&self) -> &Self::Target {
        self.as_map()
    }
}

impl Serialize for ColumnMapping {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ColumnMapping::Auto => serializer.serialize_str("auto"),
            ColumnMapping::Inherited => serializer.serialize_str("inherited"),
            ColumnMapping::Explicit(map) => map.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ColumnMapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;

        struct ColumnMappingVisitor;

        impl<'de> de::Visitor<'de> for ColumnMappingVisitor {
            type Value = ColumnMapping;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"auto\", \"inherited\", or a column mapping object")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<ColumnMapping, E> {
                match v {
                    "auto" => Ok(ColumnMapping::Auto),
                    "inherited" => Ok(ColumnMapping::Inherited),
                    _ => Err(E::custom(format!(
                        "expected \"auto\", \"inherited\", or a mapping object, got \"{}\"",
                        v
                    ))),
                }
            }

            fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<ColumnMapping, M::Error> {
                let inner =
                    HashMap::<String, ColumnMappingValue>::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ColumnMapping::Explicit(inner))
            }
        }

        deserializer.deserialize_any(ColumnMappingVisitor)
    }
}

/// Column mapping value: simple string or structured with grain override.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ColumnMappingValue {
    Simple(String),
    WithGrain {
        column: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        grain: Option<TemporalGrain>,
    },
}

impl<'de> Deserialize<'de> for ColumnMappingValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WithGrainHelper {
            column: String,
            grain: Option<TemporalGrain>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Simple(String),
            WithGrain(WithGrainHelper),
        }

        match Raw::deserialize(deserializer)? {
            Raw::Simple(s) => Ok(ColumnMappingValue::Simple(s)),
            Raw::WithGrain(w) => Ok(ColumnMappingValue::WithGrain {
                column: w.column,
                grain: w.grain,
            }),
        }
    }
}

// =============================================================================
// Domain
// =============================================================================

/// Domain specification: single string or array of strings.
#[derive(Debug, Clone, Serialize)]
pub struct DomainSpec(pub Vec<String>);

impl<'de> Deserialize<'de> for DomainSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrVec {
            Single(String),
            Multiple(Vec<String>),
        }
        match StringOrVec::deserialize(deserializer)? {
            StringOrVec::Single(s) => Ok(DomainSpec(vec![s])),
            StringOrVec::Multiple(v) => Ok(DomainSpec(v)),
        }
    }
}

// =============================================================================
// Keys
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Keys {
    #[serde(default)]
    pub primary: Option<Vec<String>>,
    #[serde(default)]
    pub unique: Option<Vec<UniqueConstraint>>,
    #[serde(default)]
    pub foreign: Option<Vec<ForeignKey>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UniqueConstraint {
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub reference: String,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

// =============================================================================
// Dimensions
// =============================================================================

/// Dimension entry: inline definition or reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DimensionEntry {
    Ref(RefEntry),
    Inline(Dimension),
}

/// Reference entry for reusable definitions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefEntry {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

/// A dimension definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dimension {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub data_type: DataType,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    #[serde(default, rename = "type")]
    pub dim_type: DimensionType,
}

/// Dimension type (temporal, categorical, binary, geo, bucketed).
///
/// Defaults to `Categorical` with no enum constraint when omitted from YAML.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionType {
    Temporal(TemporalDimension),
    Categorical(CategoricalDimension),
    Binary(BinaryDimension),
    Geo(GeoDimension),
    Bucketed(BucketedDimension),
    Metadata(MetadataDimension),
}

impl Default for DimensionType {
    fn default() -> Self {
        DimensionType::Categorical(CategoricalDimension { enum_values: None })
    }
}

impl<'de> Deserialize<'de> for DimensionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "DimensionType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "temporal" => Ok(DimensionType::Temporal(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "categorical" => Ok(DimensionType::Categorical(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "binary" => Ok(DimensionType::Binary(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "geo" => Ok(DimensionType::Geo(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "bucketed" => Ok(DimensionType::Bucketed(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "metadata" => Ok(DimensionType::Metadata(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["temporal", "categorical", "binary", "geo", "bucketed", "metadata"],
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemporalDimension {
    pub grains: Vec<TemporalGrain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalGrain {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl TemporalGrain {
    /// Returns the grain's relative coarseness (higher = coarser).
    pub fn coarseness(self) -> u8 {
        match self {
            Self::Minute => 0,
            Self::Hour => 1,
            Self::Day => 2,
            Self::Week => 3,
            Self::Month => 4,
            Self::Quarter => 5,
            Self::Year => 6,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CategoricalDimension {
    #[serde(default, rename = "enum")]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BinaryDimension {
    #[serde(rename = "type")]
    pub binary_type: BinaryType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryType {
    Boolean,
    Bit,
    String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeoDimension {
    pub lat: String,
    pub lon: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BucketedDimension {
    pub column: String,
    pub buckets: Vec<Bucket>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Bucket {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

/// Metadata dimension — extracts values from source metadata rather than
/// physical columns. Supports path segment extraction and Hive-style
/// partition value extraction.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetadataDimension {
    /// Extract a segment from the source path (file path or table path).
    #[serde(default)]
    pub path: Option<PathExtraction>,
    /// Extract a value from Hive-style partitioning (key=value).
    #[serde(default)]
    pub partition: Option<PartitionExtraction>,
}

/// Path segment extraction: returns the raw segment at the given position.
/// Tokenizer splits on `/`. Position is 0-indexed.
///
/// Example: path `s3://bucket/month=01/data.parquet` with `token: 2`
/// returns `"month=01"` (raw, no key=value parsing).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathExtraction {
    /// 0-indexed position of the path segment to extract.
    pub token: usize,
}

/// Partition value extraction: returns the VALUE from a Hive-style
/// `key=value` partition at the specified level.
/// Level is 1-indexed.
///
/// Example: partition path `year=2024/month=01` with `level: 1`
/// returns `"2024"` (the value of the first partition key).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartitionExtraction {
    /// 1-indexed partition level.
    pub level: usize,
}

// =============================================================================
// AI Context
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiContext {
    #[serde(default)]
    pub synonyms: Option<Vec<String>>,
    #[serde(default)]
    pub query_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub value_examples: Option<Vec<String>>,
    #[serde(default)]
    pub semantic_tags: Option<Vec<String>>,
}

// =============================================================================
// Measures
// =============================================================================

/// Measure entry: inline definition or reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum MeasureEntry {
    Ref(RefEntry),
    Inline(Measure),
}

/// Declarative aggregation type for measures and metrics.
///
/// When specified on a measure, the `expr` field (if any) is treated as a
/// horizontal-only transformation applied *before* aggregation.
/// When specified on a metric, creates a two-stage aggregation plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationType {
    Sum,
    Avg,
    Count,
    CountDistinct,
    Min,
    Max,
}

/// A measure definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Measure {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub data_type: DataType,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    /// Declarative aggregation function.
    ///
    /// When present, `expr` is optional and horizontal-only (no aggregation
    /// functions allowed). When absent, `expr` must contain the aggregation
    /// (legacy format, e.g. `"SUM(amount)"`).
    #[serde(default)]
    pub agg: Option<AggregationType>,
    /// DSL expression. When `agg` is set, this is a horizontal transformation
    /// applied before aggregation. When `agg` is absent, must contain an
    /// aggregation function (legacy format).
    #[serde(default)]
    pub expr: Option<String>,
    #[serde(default)]
    pub additivity: Option<AdditivityType>,
    #[serde(default)]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
}

/// Additivity type for measures and metrics.
///
/// Defaults to `Full` when omitted from YAML (the field is `Option<AdditivityType>`).
///
/// YAML formats:
///   - String shorthand: `additivity: non` or `additivity: full`
///   - Object form for semi: `additivity: { semi: { non_additive_dimensions: [...] } }`
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdditivityType {
    Full,
    Semi(SemiAdditivity),
    Non,
}

impl<'de> Deserialize<'de> for AdditivityType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct AdditivityVisitor;

        impl<'de> de::Visitor<'de> for AdditivityVisitor {
            type Value = AdditivityType;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"full\", \"non\", or { semi: { non_additive_dimensions: [...] } }")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<AdditivityType, E> {
                match v {
                    "full" => Ok(AdditivityType::Full),
                    "non" => Ok(AdditivityType::Non),
                    other => Err(E::custom(format!(
                        "expected \"full\" or \"non\", got \"{}\"",
                        other
                    ))),
                }
            }

            fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<AdditivityType, M::Error> {
                let inner: HashMap<String, serde_yaml::Value> =
                    Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
                if inner.len() != 1 {
                    return Err(de::Error::custom(
                        "additivity object must have exactly one key: \"full\", \"semi\", or \"non\"",
                    ));
                }
                let (key, value) = inner.into_iter().next()
                    .ok_or_else(|| de::Error::custom("expected one variant key"))?;
                match key.as_str() {
                    "full" => Ok(AdditivityType::Full),
                    "semi" => Ok(AdditivityType::Semi(
                        serde_yaml::from_value(value).map_err(de::Error::custom)?,
                    )),
                    "non" => Ok(AdditivityType::Non),
                    other => Err(de::Error::unknown_variant(
                        other,
                        &["full", "semi", "non"],
                    )),
                }
            }
        }

        deserializer.deserialize_any(AdditivityVisitor)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemiAdditivity {
    pub non_additive_dimensions: Vec<String>,
    #[serde(default = "default_resolution_strategy")]
    pub resolution_strategy: ResolutionStrategy,
}

fn default_resolution_strategy() -> ResolutionStrategy {
    ResolutionStrategy::Latest
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStrategy {
    Latest,
    Earliest,
    Max,
    Min,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeasureConstraints {
    #[serde(default)]
    pub dimensions: Option<DimensionConstraints>,
    #[serde(default)]
    pub aggregations: Option<AggregationConstraints>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DimensionConstraints {
    #[serde(default)]
    pub one_of: Option<Vec<String>>,
    #[serde(default)]
    pub none_of: Option<Vec<String>>,
    #[serde(default)]
    pub all: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AggregationConstraints {
    #[serde(default)]
    pub allowed: Option<Vec<String>>,
    #[serde(default)]
    pub prohibited: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeasureFilter {
    pub name: String,
    pub expr: String,
}

// =============================================================================
// Metrics
// =============================================================================

/// Metric entry: inline definition or reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum MetricEntry {
    Ref(RefEntry),
    Inline(Metric),
}

/// A metric definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metric {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub data_type: DataType,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    /// Declarative aggregation for two-stage metric computation.
    ///
    /// When present, creates a two-stage plan: inner grain = all request
    /// dimensions, outer grain = remaining dimensions after metric's agg
    /// consumes inner groups. Constraint: metrics can only reference
    /// existing semantic elements (measures, dims, keys).
    #[serde(default)]
    pub agg: Option<AggregationType>,
    /// DSL expression referencing measures or other metrics.
    pub expr: String,
    #[serde(default)]
    pub additivity: Option<AdditivityType>,
    #[serde(default)]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
}

// =============================================================================
// Filters
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetFilter {
    pub name: String,
    pub expr: String,
}

// =============================================================================
// Extras
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetExtras {
    #[serde(default)]
    pub catalog: Option<CatalogConfig>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogConfig {
    #[serde(rename = "type")]
    pub catalog_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemporalConfig {
    #[serde(rename = "type")]
    pub temporal_type: TemporalHistorization,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalHistorization {
    Timeseries(TimeseriesConfig),
    Snapshot(SnapshotConfig),
    Scd(ScdConfig),
}

impl<'de> Deserialize<'de> for TemporalHistorization {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "TemporalHistorization must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "timeseries" => Ok(TemporalHistorization::Timeseries(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "snapshot" => Ok(TemporalHistorization::Snapshot(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "scd" => Ok(TemporalHistorization::Scd(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["timeseries", "snapshot", "scd"],
            )),
        }
    }
}

impl TemporalHistorization {
    /// Returns the variant name as a string for error messages.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Timeseries(_) => "timeseries",
            Self::Snapshot(_) => "snapshot",
            Self::Scd(_) => "scd",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeseriesConfig {
    pub occurred_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotConfig {
    pub snapshotted_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScdConfig {
    #[serde(flatten)]
    pub scd_type: ScdType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScdType {
    Type1,
    Type2(ScdVersionedColumns),
    Type3,
    Type4,
    Type5(ScdVersionedColumns),
    Type6(ScdVersionedColumns),
}

impl<'de> Deserialize<'de> for ScdType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "ScdType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "type_1" => Ok(ScdType::Type1),
            "type_2" => Ok(ScdType::Type2(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "type_3" => Ok(ScdType::Type3),
            "type_4" => Ok(ScdType::Type4),
            "type_5" => Ok(ScdType::Type5(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "type_6" => Ok(ScdType::Type6(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["type_1", "type_2", "type_3", "type_4", "type_5", "type_6"],
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScdVersionedColumns {
    pub valid_from: String,
    pub valid_to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    /// Single file/object store path (backward compat).
    #[serde(default)]
    pub path: Option<String>,
    /// Single catalog table reference (backward compat).
    #[serde(default)]
    pub table: Option<String>,
    /// Multiple file/object store paths (may contain globs like "*.parquet").
    #[serde(default)]
    pub paths: Vec<String>,
    /// Multiple catalog table references (may contain wildcards like "orders_*").
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub partition_def: Option<PartitionDef>,
}

/// Resolved storage sources — unified view of paths and tables.
#[derive(Debug, Clone)]
pub struct StorageSources {
    pub paths: Vec<String>,
    pub tables: Vec<String>,
}

impl StorageSources {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.tables.is_empty()
    }

    pub fn is_mixed(&self) -> bool {
        !self.paths.is_empty() && !self.tables.is_empty()
    }

    /// Returns all sources as a flat list (paths first, then tables).
    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.paths.iter().map(|s| s.as_str())
            .chain(self.tables.iter().map(|s| s.as_str()))
    }

    /// Returns the primary source (first path or first table).
    pub fn primary(&self) -> Option<&str> {
        self.paths.first()
            .or(self.tables.first())
            .map(|s| s.as_str())
    }
}

impl StorageConfig {
    /// Returns all source references as a unified `StorageSources`.
    /// Merges singular `path`/`table` into `paths`/`tables` respectively.
    pub fn all_sources(&self) -> StorageSources {
        let mut file_paths = self.paths.clone();
        if let Some(ref p) = self.path {
            if !file_paths.contains(p) {
                file_paths.insert(0, p.clone());
            }
        }
        let mut table_refs = self.tables.clone();
        if let Some(ref t) = self.table {
            if !table_refs.contains(t) {
                table_refs.insert(0, t.clone());
            }
        }
        StorageSources {
            paths: file_paths,
            tables: table_refs,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartitionDef {
    #[serde(rename = "type")]
    pub partition_type: PartitionType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionType {
    Range(RangePartition),
    List(ListPartition),
}

impl<'de> Deserialize<'de> for PartitionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "PartitionType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "range" => Ok(PartitionType::Range(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "list" => Ok(PartitionType::List(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(other, &["range", "list"])),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RangePartition {
    pub column: String,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListPartition {
    pub column: String,
    pub values: Vec<String>,
}

// =============================================================================
// Relationships
// =============================================================================

/// Top-level relationship (joins between datasets and/or kinds).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Relationship {
    pub name: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub join_type: JoinType,
    pub columns: Vec<JoinColumnPair>,
    pub cardinality: Cardinality,
}

/// Kind-internal relationship (used for joinset join paths).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindRelationship {
    pub name: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub join_type: JoinType,
    pub columns: Vec<JoinColumnPair>,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinColumnPair {
    pub from: String,
    pub to: String,
}
