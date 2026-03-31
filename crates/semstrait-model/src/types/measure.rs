//! Measure types for semantic models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use super::common::{AiContext, DataType, MeasureFilter, RefEntry};

/// Measure entry: inline definition or reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum MeasureEntry {
    Ref(RefEntry),
    Inline(Measure),
}

impl MeasureEntry {
    /// Extract the name regardless of variant.
    pub fn name(&self) -> &str {
        match self {
            MeasureEntry::Ref(r) => &r.ref_name,
            MeasureEntry::Inline(m) => &m.name,
        }
    }
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
    /// Data type. Optional — derived at compile time from `agg` when possible.
    /// If omitted and not derivable, compilation fails with a clear error.
    #[serde(default)]
    pub data_type: Option<DataType>,
    #[serde(default)]
    pub ai_context: Option<AiContext>,
    /// Declarative aggregation function.
    ///
    /// When present, `expr` is optional and horizontal-only (no aggregation
    /// functions allowed). When absent, `expr` must contain the aggregation
    /// (legacy format, e.g. `"SUM(amount)"`).
    #[serde(default)]
    pub agg: Option<AggregationType>,
    /// Expression — inline DSL string or declarative block.
    ///
    /// When `agg` is set, this is a horizontal transformation applied before
    /// aggregation. When `agg` is absent, must contain an aggregation function
    /// (legacy format, e.g. `"SUM(amount)"`).
    #[serde(default)]
    pub expr: Option<crate::expr_block::ExprSource>,
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
