//! Metric types for semantic models.

use serde::{Deserialize, Serialize};

use super::common::{AiContext, DataType, MeasureFilter, RefEntry};
use super::measure::{AdditivityType, AggregationType, MeasureConstraints};

/// Metric entry: inline definition or reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum MetricEntry {
    Ref(RefEntry),
    Inline(Metric),
}

impl MetricEntry {
    /// Extract the name regardless of variant.
    pub fn name(&self) -> &str {
        match self {
            MetricEntry::Ref(r) => &r.ref_name,
            MetricEntry::Inline(m) => &m.name,
        }
    }
}

/// A metric definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metric {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Data type. Optional — derived at compile time from leaf measures when possible.
    /// If omitted and not derivable, compilation fails with a clear error.
    #[serde(default)]
    pub data_type: Option<DataType>,
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
    /// Expression — inline DSL string or declarative block.
    /// References measures or other metrics.
    pub expr: crate::expr_block::ExprSource,
    #[serde(default)]
    pub additivity: Option<AdditivityType>,
    #[serde(default)]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default)]
    pub filters: Vec<MeasureFilter>,
}
