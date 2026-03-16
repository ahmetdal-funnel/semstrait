//! v1.2 Semantic Model type hierarchy.
//!
//! Maps 1:1 to `schema/schema.yml`. All types are serde-compatible for
//! YAML deserialization via `serde_yaml`.

use serde::{Deserialize, Serialize};

use super::types::DataType;

// =============================================================================
// Top-level wrapper
// =============================================================================

/// Root document shape: `semantic_model: { ... }`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticModelFile {
    pub semantic_model: SemanticModel,
}

/// The semantic model definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticModel {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<String>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,

    // Top-level datasets (live here OR inside a kind, never both).
    #[serde(default)]
    pub datasets: Option<Vec<Dataset>>,

    // Kinds: grainset / unionset / joinset.
    #[serde(default)]
    pub kinds: Option<Vec<Kind>>,

    // Top-level relationships (joins between datasets and/or kinds).
    #[serde(default)]
    pub relationships: Option<Vec<Relationship>>,

    // Reusable definitions (referenced via `ref:` syntax).
    #[serde(default)]
    pub dimensions: Option<Vec<Dimension>>,
    #[serde(default)]
    pub measures: Option<Vec<Measure>>,
    #[serde(default)]
    pub metrics: Option<Vec<Metric>>,
}

// =============================================================================
// Dataset
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dataset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<DatasetTags>,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Option<Vec<DimensionEntry>>,
    #[serde(default)]
    pub measures: Option<Vec<MeasureEntry>>,
    #[serde(default)]
    pub metrics: Option<Vec<MetricEntry>>,
    #[serde(default)]
    pub filters: Option<Vec<DatasetFilter>>,
    #[serde(default)]
    pub extras: Option<DatasetExtras>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetTags {
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
}

// =============================================================================
// Domain
// =============================================================================

/// Domain can be a single string or an array of strings.
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

/// A dimension entry: either an inline definition or a `ref:` reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DimensionEntry {
    Ref(RefEntry),
    Inline(Dimension),
}

/// A ref entry: `{ ref: "name" }`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefEntry {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

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
        let map: std::collections::HashMap<String, serde_yaml::Value> =
            Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "DimensionType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next().unwrap();
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

/// A measure entry: either inline or `ref:`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MeasureEntry {
    Ref(RefEntry),
    Inline(Measure),
}

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
    pub filters: Option<Vec<MeasureFilter>>,
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
        let map: std::collections::HashMap<String, serde_yaml::Value> =
            Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "AdditivityType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next().unwrap();
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

/// A metric entry: either inline or `ref:`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MetricEntry {
    Ref(RefEntry),
    Inline(Metric),
}

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
    pub filters: Option<Vec<MeasureFilter>>,
}

// =============================================================================
// Dataset Filters
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
        let map: std::collections::HashMap<String, serde_yaml::Value> =
            Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "TemporalHistorization must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next().unwrap();
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
        let map: std::collections::HashMap<String, serde_yaml::Value> =
            Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "ScdType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next().unwrap();
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

/// Columns for SCD types that track validity windows.
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
        let map: std::collections::HashMap<String, serde_yaml::Value> =
            Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "PartitionType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next().unwrap();
        match key.as_str() {
            "range" => Ok(PartitionType::Range(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "list" => Ok(PartitionType::List(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["range", "list"],
            )),
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
// Kinds
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Kind {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub domain: Option<DomainSpec>,
    #[serde(rename = "type")]
    pub kind_type: KindType,
    #[serde(default)]
    pub keys: Option<Keys>,
    #[serde(default)]
    pub dimensions: Option<Vec<DimensionEntry>>,
    #[serde(default)]
    pub measures: Option<Vec<MeasureEntry>>,
    #[serde(default)]
    pub metrics: Option<Vec<MetricEntry>>,
    pub datasets: Vec<KindDatasetEntry>,
    #[serde(default)]
    pub relationships: Option<Vec<KindRelationship>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KindType {
    Grainset,
    Unionset,
    Joinset(JoinsetConfig),
}

impl<'de> Deserialize<'de> for KindType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: std::collections::HashMap<String, serde_yaml::Value> =
            Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "KindType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next().unwrap();
        match key.as_str() {
            "grainset" => Ok(KindType::Grainset),
            "unionset" => Ok(KindType::Unionset),
            "joinset" => Ok(KindType::Joinset(
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

/// A kind dataset entry: either inline or `ref:` to another kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum KindDatasetEntry {
    Ref(RefEntry),
    Inline(KindDataset),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindDataset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    pub extras: KindDatasetExtras,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindDatasetExtras {
    pub column_mapping: std::collections::HashMap<String, ColumnMappingValue>,
    #[serde(default)]
    pub temporal: Option<TemporalConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    #[serde(default)]
    pub catalog: Option<CatalogConfig>,
}

/// Column mapping value: either a simple string (physical column name) or
/// a complex mapping with grain specification.
#[derive(Debug, Clone, Serialize)]
pub enum ColumnMappingValue {
    Simple(String),
    Complex {
        column: String,
        grain: Option<TemporalGrain>,
    },
}

impl<'de> Deserialize<'de> for ColumnMappingValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ComplexMapping {
            column: String,
            grain: Option<TemporalGrain>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Simple(String),
            Complex(ComplexMapping),
        }

        match Raw::deserialize(deserializer)? {
            Raw::Simple(s) => Ok(ColumnMappingValue::Simple(s)),
            Raw::Complex(c) => Ok(ColumnMappingValue::Complex {
                column: c.column,
                grain: c.grain,
            }),
        }
    }
}

// =============================================================================
// Relationships
// =============================================================================

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
