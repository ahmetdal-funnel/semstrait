//! `Measure`, `AggregationType`, `AdditivityType`, `MeasureEntry` /
//! `MeasureRef` — `18 §1.2`, `18 §5`.

use crate::entities::ai::AiContext;
use crate::entities::filter::AggregationFilter;
use crate::expr_source::ExprSource;
use crate::types::SemanticsName;
use crate::yaml::tagged::single_key_map;
use bon::Builder;
use semstrait_core::DataType;
use semstrait_ir::SemanticLeaf;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

/// Canonical Measure authoring shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(
    start_fn = builder,
    finish_fn = build,
    state_mod(name = measure_builder, vis = "pub"),
)]
pub struct Measure {
    #[builder(start_fn, into)]
    pub name: SemanticsName,

    /// Measure-level conditional-aggregation filters. Each filter
    /// wraps the Measure in a `CASE WHEN ... THEN expr ELSE NULL END`
    /// at compile time. Accumulated via `#[builder(field)]`; per-item
    /// inserter `.filter(...)` lives on `MeasureBuilder` below.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(field)]
    pub filters: Vec<AggregationFilter>,

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
    /// Semantic named by the Measure. Per `14 §7`, Measure `expr:`
    /// parses to `SemanticExpr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub expr: Option<ExprSource<SemanticLeaf>>,

    /// Optional additivity classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<AdditivityType>,
}

// ── Facade methods on `MeasureBuilder` — `32 §9.7.8` ────────────────
//
// Aggregation / additivity shortcuts delegate to `.agg(...)` /
// `.additivity(...)`. Shared `IsUnset` bounds live on the impl
// blocks. `.filter(item)` pushes onto the `filters` collection;
// `.filters(items)` replaces it (back-compat with the removed
// bon-generated plural setter).

impl<S: measure_builder::State> MeasureBuilder<S>
where
    S::Agg: measure_builder::IsUnset,
{
    pub fn sum(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Sum)
    }
    pub fn avg(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Avg)
    }
    pub fn count(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Count)
    }
    pub fn count_distinct(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::CountDistinct)
    }
    pub fn min(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Min)
    }
    pub fn max(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Max)
    }
    pub fn median(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Median)
    }
    pub fn std_dev(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::StdDev)
    }
    pub fn variance(self) -> MeasureBuilder<measure_builder::SetAgg<S>> {
        self.agg(AggregationType::Variance)
    }
}

impl<S: measure_builder::State> MeasureBuilder<S>
where
    S::Additivity: measure_builder::IsUnset,
{
    pub fn full(self) -> MeasureBuilder<measure_builder::SetAdditivity<S>> {
        self.additivity(AdditivityType::Full)
    }
    pub fn semi(self, s: SemiAdditivity) -> MeasureBuilder<measure_builder::SetAdditivity<S>> {
        self.additivity(AdditivityType::Semi(s))
    }
    pub fn non(self) -> MeasureBuilder<measure_builder::SetAdditivity<S>> {
        self.additivity(AdditivityType::Non)
    }
}

impl<S: measure_builder::State> MeasureBuilder<S> {
    pub fn filter(mut self, f: AggregationFilter) -> Self {
        self.filters.push(f);
        self
    }

    pub fn filters(mut self, items: impl IntoIterator<Item = AggregationFilter>) -> Self {
        self.filters = items.into_iter().collect();
        self
    }
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
    pub expr: Option<ExprSource<SemanticLeaf>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<AggregationFilter>,
}
