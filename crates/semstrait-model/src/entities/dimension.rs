//! `Dimension`, `DimensionType`, `DimensionEntry` / `DimensionRef` —
//! `18 §1.2`, `18 §4`.

use crate::entities::ai::AiContext;
use crate::expr_ast::ExprSource;
use crate::types::SemanticsName;
use crate::yaml::tagged::single_key_map;
use bon::Builder;
use semstrait_core::{DataType, Grain};
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

/// Canonical Dimension authoring shape. Used inline on a DataKind's
/// SemanticInterface or in the root `dimensions:` pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct Dimension {
    #[builder(start_fn, into)]
    pub name: SemanticsName,

    /// Mandatory at declaration; immutable from the root-pool
    /// declaration (SR-E-10 / SR-E-12).
    pub data_type: DataType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    /// The Dimension category — drives planner and adapter behavior.
    #[serde(rename = "type")]
    pub dim_type: DimensionType,

    /// Optional derivation expression. `None` means the Dimension is
    /// bound directly from `semantic_mapping`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub expr: Option<ExprSource>,
}

/// `DimensionType` — six-variant roster (`18 §4.1`). Categorical,
/// Binary, and Geo carry no body; the others have variant-specific
/// bodies.
///
/// YAML accepts both bare-string forms (for body-less variants) and
/// the standard tagged map form (for variants with bodies).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DimensionType {
    Temporal(TemporalDimensionBody),
    Categorical,
    Binary,
    Geo,
    Bucketed(BucketedDimensionBody),
    Metadata(MetadataDimensionBody),
}

impl DimensionType {
    /// Convenience constructor for `Temporal`. Authors list only the
    /// grains the backing source actually supports.
    pub fn temporal(grains: impl IntoIterator<Item = Grain>) -> Self {
        Self::Temporal(TemporalDimensionBody {
            grains: grains.into_iter().collect(),
        })
    }

    pub fn categorical() -> Self {
        Self::Categorical
    }

    pub fn binary() -> Self {
        Self::Binary
    }

    pub fn geo() -> Self {
        Self::Geo
    }

    pub fn bucketed(buckets: impl IntoIterator<Item = BucketSpec>) -> Self {
        Self::Bucketed(BucketedDimensionBody {
            buckets: buckets.into_iter().collect(),
        })
    }

    pub fn metadata(source: MetadataSource) -> Self {
        Self::Metadata(MetadataDimensionBody { source })
    }
}

impl<'de> Deserialize<'de> for DimensionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(s) => match s.as_str() {
                "categorical" => Ok(Self::Categorical),
                "binary" => Ok(Self::Binary),
                "geo" => Ok(Self::Geo),
                "temporal" => Ok(Self::Temporal(TemporalDimensionBody::default())),
                "bucketed" => Err(serde::de::Error::custom(
                    "DimensionType::bucketed requires a body (buckets list)",
                )),
                "metadata" => Err(serde::de::Error::custom(
                    "DimensionType::metadata requires a body (source spec)",
                )),
                other => Err(serde::de::Error::custom(format!(
                    "DimensionType: unknown variant `{}`",
                    other
                ))),
            },
            Value::Mapping(_) => {
                let (key, body) = single_key_map::<D::Error>(value, "DimensionType")?;
                match key.as_str() {
                    "temporal" => {
                        let b: TemporalDimensionBody =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Temporal(b))
                    }
                    "bucketed" => {
                        let b: BucketedDimensionBody =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Bucketed(b))
                    }
                    "metadata" => {
                        let b: MetadataDimensionBody =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Metadata(b))
                    }
                    "categorical" | "binary" | "geo" => Err(serde::de::Error::custom(format!(
                        "DimensionType::{key} is body-less; use the bare-string form (`type: {key}`)"
                    ))),
                    other => Err(serde::de::Error::custom(format!(
                        "DimensionType: unknown variant `{other}`"
                    ))),
                }
            }
            other => Err(serde::de::Error::custom(format!(
                "DimensionType: expected string or mapping, got {:?}",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TemporalDimensionBody {
    /// The set of grains at which this Dimension can be rolled up.
    /// Empty means the Dimension is declared as a Timestamp but is not
    /// rollable (only the source grain on `extras.temporal.grain:`
    /// applies).
    #[serde(default)]
    pub grains: Vec<Grain>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct BucketedDimensionBody {
    /// Bucket boundaries. Each bucket spans `[lower_inclusive, upper_exclusive)`.
    pub buckets: Vec<BucketSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct BucketSpec {
    #[builder(start_fn, into)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<BucketBound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<BucketBound>,
}

/// Single bound on a [`BucketSpec`]. YAML form is the externally-tagged
/// single-key map, e.g. `{int: 100}`, `{decimal: "1.5"}`,
/// `{date: "2024-01-01"}`. Hand-rolled `Deserialize` per
/// [`crate::yaml::tagged`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BucketBound {
    Int(i64),
    Float(f64),
    /// Decimal preserved as a string for lossless round-trip.
    Decimal(String),
    /// ISO-8601 date.
    Date(String),
    /// ISO-8601 timestamp.
    Timestamp(String),
}

impl<'de> Deserialize<'de> for BucketBound {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let (key, body) = single_key_map::<D::Error>(value, "BucketBound")?;
        match key.as_str() {
            "int" => {
                let v: i64 = serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Int(v))
            }
            "float" => {
                let v: f64 = serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Float(v))
            }
            "decimal" => {
                let v: String = serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Decimal(v))
            }
            "date" => {
                let v: String = serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Date(v))
            }
            "timestamp" => {
                let v: String = serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Timestamp(v))
            }
            other => Err(serde::de::Error::custom(format!(
                "BucketBound: unknown variant `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MetadataDimensionBody {
    /// Where to extract the value from each `PhysicalSource` — path
    /// token, partition column, or S3 object metadata field. The full
    /// grammar lives in `15 §8`.
    pub source: MetadataSource,
}

/// Author surface of a metadata extraction site. v1 ratifies path-
/// token extraction only; partition / object-metadata fields are
/// reserved for v2 (`15 §8.0`). YAML form is the externally-tagged
/// single-key map; hand-rolled `Deserialize` per [`crate::yaml::tagged`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MetadataSource {
    /// Token at 0-indexed (scheme-stripped) segment position.
    Path(PathTokenRef),
    /// Reserved for future Hive-partition-column extraction.
    Partition(PartitionRef),
}

impl<'de> Deserialize<'de> for MetadataSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let (key, body) = single_key_map::<D::Error>(value, "MetadataSource")?;
        match key.as_str() {
            "path" => {
                let r: PathTokenRef =
                    serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Path(r))
            }
            "partition" => {
                let r: PartitionRef =
                    serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                Ok(Self::Partition(r))
            }
            other => Err(serde::de::Error::custom(format!(
                "MetadataSource: unknown variant `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PathTokenRef {
    /// Token at 0-indexed segment position.
    pub token: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PartitionRef {
    /// Partition-column name (e.g. `"year"`).
    pub column: String,
}

// ── Reference / entry grammar (18 §1.2) ─────────────────────────────

/// One entry under a `SemanticInterface.dimensions:` list — either an
/// inline declaration or a `ref` against the root pool.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum DimensionEntry {
    Inline(Dimension),
    Ref(DimensionRef),
}

impl DimensionEntry {
    pub fn inline(d: Dimension) -> Self {
        Self::Inline(d)
    }

    pub fn r#ref(name: impl Into<SemanticsName>) -> Self {
        Self::Ref(DimensionRef {
            name: name.into(),
            expr: None,
        })
    }

    pub fn ref_with_expr(name: impl Into<SemanticsName>, expr: ExprSource) -> Self {
        Self::Ref(DimensionRef {
            name: name.into(),
            expr: Some(expr),
        })
    }

    pub fn name(&self) -> &SemanticsName {
        match self {
            Self::Inline(d) => &d.name,
            Self::Ref(r) => &r.name,
        }
    }
}

impl<'de> Deserialize<'de> for DimensionEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if let Value::Mapping(map) = &value {
            if map.contains_key(Value::String("ref".into())) {
                let r: DimensionRef =
                    serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
                return Ok(Self::Ref(r));
            }
        }
        let d: Dimension = serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self::Inline(d))
    }
}

/// Reference site — the only fields that may be locally overridden.
/// Spec `18 §1.3`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DimensionRef {
    #[serde(rename = "ref")]
    pub name: SemanticsName,

    /// Local override of the root-pool expression. The other root-pool
    /// fields are immutable per SR-E-1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprSource>,
}
