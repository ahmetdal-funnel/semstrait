//! `SemanticInterface` — the per-Public-DataKind block that lists the
//! data kind's Dimensions / Measures / Metrics / Filters / Keys.
//! Authored directly on every Public form (never on `Nested*`).

use crate::entities::dimension::DimensionEntry;
use crate::entities::filter::DataKindFilter;
use crate::entities::keys::Keys;
use crate::entities::measure::MeasureEntry;
use crate::entities::metric::MetricEntry;
use bon::Builder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct SemanticInterface {
    /// Dimensions exposed by this DataKind. Each entry is either an
    /// inline declaration or a `ref` against the root pool (`18 §1.2`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub dimensions: Vec<DimensionEntry>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub measures: Vec<MeasureEntry>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub metrics: Vec<MetricEntry>,

    /// DataKind-level filters. `AggregationFilter`s are authored on
    /// individual Measures / Metrics, not here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub filters: Vec<DataKindFilter>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Keys>,
}

impl SemanticInterface {
    pub fn is_empty(&self) -> bool {
        self.dimensions.is_empty()
            && self.measures.is_empty()
            && self.metrics.is_empty()
            && self.filters.is_empty()
            && self.keys.is_none()
    }
}
