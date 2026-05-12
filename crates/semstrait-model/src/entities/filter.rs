//! `DataKindFilter` and `AggregationFilter` — `18 §7`.
//!
//! Two distinct filter types share no common supertype; cross-
//! referencing between the two is rejected at validate (SR-E-11).

use crate::entities::ai::AiContext;
use crate::expr_ast::ExprSource;
use crate::types::FilterName;
use bon::Builder;
use serde::{Deserialize, Serialize};

/// DataKind-level filter — narrows the rowset a DataKind exposes.
/// Authored under a `SemanticInterface.filters:` list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct DataKindFilter {
    #[builder(start_fn, into)]
    pub name: FilterName,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[builder(into)]
    pub expr: ExprSource,
}

/// Measure / Metric-level filter — applies a conditional inside the
/// aggregation. Authored under a Measure's or Metric's `filters:` list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct AggregationFilter {
    #[builder(start_fn, into)]
    pub name: FilterName,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,

    #[builder(into)]
    pub expr: ExprSource,
}
