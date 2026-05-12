//! `Measure`, `AggregationType`, `AdditivityType`, `MeasureEntry` /
//! `MeasureRef` — `18 §1.2`, `18 §5`.

use crate::entities::ai::AiContext;
use crate::entities::filter::AggregationFilter;
use crate::expr_block::ExprSource;
use crate::types::SemanticsName;
use crate::yaml::tagged::single_key_map;
use bon::Builder;
use semstrait_core::DataType;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

/// Canonical Measure authoring shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct Measure {
    #[builder(start_fn, into)]
    pub name: SemanticsName,

    pub data_type: DataType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    /// REQUIRED on Measure declarations (SR-E-9). Selects the
    /// aggregation family.
    pub agg: AggregationType,

    /// Optional horizontal-only transform applied before aggregation.
    /// `None` means the aggregation is applied directly to the
    /// Semantic named by the Measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub expr: Option<ExprSource>,

    /// Optional additivity classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<AdditivityType>,

    /// Measure-level conditional-aggregation filters. Each filter
    /// wraps the Measure in a `CASE WHEN ... THEN expr ELSE NULL END`
    /// at compile time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub filters: Vec<AggregationFilter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AggregationType {
    Sum,
    Avg,
    Count,
    CountDistinct,
    Min,
    Max,
    Median,
    StdDev,
    Variance,
}

/// Authoring shape for additivity classification (`18 §5`). Bare-string
/// `full` / `non` for the body-less variants; single-key tagged map
/// `{semi: {axes: [...], strategy: ...}}` for the `Semi` body. Hand-
/// rolled `Deserialize` per [`crate::yaml::tagged`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdditivityType {
    Full,
    Semi(SemiAdditivity),
    Non,
}

impl<'de> Deserialize<'de> for AdditivityType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(s) => match s.as_str() {
                "full" => Ok(Self::Full),
                "non" => Ok(Self::Non),
                "semi" => Err(serde::de::Error::custom(
                    "AdditivityType::semi requires a body (`{ semi: { axes, strategy } }`)",
                )),
                other => Err(serde::de::Error::custom(format!(
                    "AdditivityType: unknown variant `{other}`"
                ))),
            },
            Value::Mapping(_) => {
                let (key, body) = single_key_map::<D::Error>(value, "AdditivityType")?;
                match key.as_str() {
                    "semi" => {
                        let s: SemiAdditivity =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Semi(s))
                    }
                    "full" | "non" => Err(serde::de::Error::custom(format!(
                        "AdditivityType::{key} is body-less; use the bare-string form"
                    ))),
                    other => Err(serde::de::Error::custom(format!(
                        "AdditivityType: unknown variant `{other}`"
                    ))),
                }
            }
            other => Err(serde::de::Error::custom(format!(
                "AdditivityType: expected string or mapping, got {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct SemiAdditivity {
    /// The Dimension axes along which this Measure is semi-additive.
    pub axes: Vec<SemanticsName>,
    /// Rollup strategy for the non-additive axes.
    pub strategy: SemiAdditivityStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemiAdditivityStrategy {
    Latest,
    Earliest,
    Average,
    First,
    Last,
}

// ── Reference / entry grammar (18 §1.2) ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum MeasureEntry {
    Inline(Measure),
    Ref(MeasureRef),
}

impl MeasureEntry {
    pub fn inline(m: Measure) -> Self {
        Self::Inline(m)
    }

    pub fn r#ref(name: impl Into<SemanticsName>) -> Self {
        Self::Ref(MeasureRef {
            name: name.into(),
            expr: None,
            filters: Vec::new(),
        })
    }

    pub fn name(&self) -> &SemanticsName {
        match self {
            Self::Inline(m) => &m.name,
            Self::Ref(r) => &r.name,
        }
    }
}

impl<'de> Deserialize<'de> for MeasureEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if let Value::Mapping(map) = &value {
            if map.contains_key(Value::String("ref".into())) {
                let r: MeasureRef =
                    serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
                return Ok(Self::Ref(r));
            }
        }
        let m: Measure = serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self::Inline(m))
    }
}

/// Reference site — `expr` and `filters` are the only locally
/// overridable fields (SR-E-1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MeasureRef {
    #[serde(rename = "ref")]
    pub name: SemanticsName,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprSource>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<AggregationFilter>,
}
