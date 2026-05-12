//! `Metric`, `MetricEntry` / `MetricRef` — `18 §1.2`, `18 §6`.

use crate::entities::ai::AiContext;
use crate::entities::filter::AggregationFilter;
use crate::entities::measure::{AdditivityType, AggregationType};
use crate::expr_block::ExprSource;
use crate::types::SemanticsName;
use bon::Builder;
use semstrait_core::DataType;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

/// Canonical Metric authoring shape.
///
/// `expr:` is REQUIRED on a Metric *declaration site* (inline) but may
/// be deferred to a `Ref` site — see `18 §6.1` and the deferred-body
/// pattern in `18 §1.3`. SR-E-2 fires at validate when neither the
/// root-pool entry nor any ref site supplies `expr:`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct Metric {
    #[builder(start_fn, into)]
    pub name: SemanticsName,

    pub data_type: DataType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    /// OPTIONAL on Metric — the expression itself carries the
    /// aggregation intent when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agg: Option<AggregationType>,

    /// REQUIRED on Metric — the derivation expression over Measures /
    /// Dimensions. May be deferred-body on a root-pool Metric per
    /// `18 §1.3`; SR-E-2 catches the all-missing case at validate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub expr: Option<ExprSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<AdditivityType>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub filters: Vec<AggregationFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum MetricEntry {
    Inline(Metric),
    Ref(MetricRef),
}

impl MetricEntry {
    pub fn inline(m: Metric) -> Self {
        Self::Inline(m)
    }

    pub fn r#ref(name: impl Into<SemanticsName>) -> Self {
        Self::Ref(MetricRef {
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

impl<'de> Deserialize<'de> for MetricEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if let Value::Mapping(map) = &value {
            if map.contains_key(Value::String("ref".into())) {
                let r: MetricRef =
                    serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
                return Ok(Self::Ref(r));
            }
        }
        let m: Metric = serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self::Inline(m))
    }
}

/// Reference site — `expr` and `filters` are the only locally
/// overridable fields (SR-E-1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MetricRef {
    #[serde(rename = "ref")]
    pub name: SemanticsName,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprSource>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<AggregationFilter>,
}
