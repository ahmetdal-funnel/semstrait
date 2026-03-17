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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticModel {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,

    // Top-level datasets (can also live inside kinds)
    #[serde(default)]
    pub datasets: Vec<Dataset>,

    // Kinds: semantic abstractions (grainset / unionset / joinset)
    #[serde(default)]
    pub kinds: Vec<Kind>,

    // Top-level relationships between datasets and/or kinds
    #[serde(default)]
    pub relationships: Vec<Relationship>,

    // Reusable definitions (referenced via `ref:` syntax)
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    pub measures: Vec<Measure>,
    #[serde(default)]
    pub metrics: Vec<Metric>,
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
}

/// Kind type specification (grainset, unionset, or joinset).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KindTypeSpec {
    Grainset,
    Unionset,
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
            "unionset" => Ok(KindTypeSpec::Unionset),
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

// =============================================================================
// Kind Dataset Entry
// =============================================================================

/// A dataset reference in a kind: either inline definition or ref to another kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
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
    pub column_mapping: HashMap<String, ColumnMappingValue>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogConfig>,
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
    #[serde(rename = "type")]
    pub dim_type: DimensionType,
}

/// Dimension type (temporal, categorical, binary, geo, bucketed).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionType {
    Temporal(TemporalDimension),
    Categorical(CategoricalDimension),
    Binary(BinaryDimension),
    Geo(GeoDimension),
    Bucketed(BucketedDimension),
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
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["temporal", "categorical", "binary", "geo", "bucketed"],
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
pub enum MeasureEntry {
    Ref(RefEntry),
    Inline(Measure),
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
    /// DSL expression (parsed by the DSL module).
    pub expr: String,
    #[serde(default)]
    pub additivity: Option<Additivity>,
    #[serde(default)]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Additivity {
    #[serde(rename = "type")]
    pub additivity_type: AdditivityType,
}

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
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "AdditivityType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "full" => Ok(AdditivityType::Full),
            "semi" => Ok(AdditivityType::Semi(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "non" => Ok(AdditivityType::Non),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["full", "semi", "non"],
            )),
        }
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
    /// DSL expression referencing measures or other metrics.
    pub expr: String,
    #[serde(default)]
    pub additivity: Option<Additivity>,
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
    #[serde(default)]
    pub user_attribute: Option<String>,
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
    pub path: String,
    #[serde(default)]
    pub partition_def: Option<PartitionDef>,
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
