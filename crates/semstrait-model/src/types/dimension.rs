//! Dimension types for semantic models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::common::{AiContext, DataType, RefEntry};
use super::temporal::TemporalGrain;

/// Dimension entry: inline definition or reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum DimensionEntry {
    Ref(RefEntry),
    Inline(Dimension),
}

impl DimensionEntry {
    /// Extract the name regardless of variant.
    pub fn name(&self) -> &str {
        match self {
            DimensionEntry::Ref(r) => &r.ref_name,
            DimensionEntry::Inline(d) => &d.name,
        }
    }
}

/// A dimension definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dimension {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Data type. Optional — derived at compile time from `expr` when possible.
    /// If omitted and not derivable, compilation fails with a clear error.
    #[serde(default)]
    pub data_type: Option<DataType>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    #[serde(default, rename = "type")]
    pub dim_type: DimensionType,
    /// Computed expression — when present, this dimension is derived (not a physical column).
    #[serde(default)]
    pub expr: Option<crate::expr_block::ExprSource>,
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
