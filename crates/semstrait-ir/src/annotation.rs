//! Semantic annotations for PlanNodes
//!
//! These annotations are serialized into Substrait AdvancedExtension.detail
//! with URN: "urn:semstrait:annotations:v1"

use serde::{Deserialize, Serialize};

/// Semantic annotation attached to PlanNodes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemAnnotation {
    /// Role of an aggregate node in additivity resolution
    AggregateRole(AggregateRole),
    /// Source/origin of a filter
    FilterSource(FilterSource),
    /// Additivity information for a measure
    Additivity(AdditivityAnnotation),
    /// Reference to the Kind being queried
    KindRef(String),
    /// Domain hint for dataset selection
    DomainHint(String),
}

/// Role of an aggregate in the additivity resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateRole {
    /// Final aggregation (top-level, produces final result)
    Final,
    /// Inner aggregation for semi-additive measures
    SemiAdditiveInner,
    /// Horizontal sub-result (partial aggregation across table groups)
    HorizontalSubResult,
    /// Fanout deduplication aggregate
    FanoutDedup,
}

/// Source/origin of a filter in the plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterSource {
    /// Filter from dataset.filter field
    DatasetFilter,
    /// Filter from measure.filter field
    MeasureFilter,
    /// Filter from metric.filter field
    MetricFilter,
    /// Filter from user query (QueryRequest.filters)
    UserFilter,
    /// SCD Type 2 current row filter
    ScdCurrentRow,
    /// Snapshot validity filter
    SnapshotValidity,
    /// Row-level security filter
    RowLevelSecurity,
}

/// Additivity information for measures
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditivityAnnotation {
    /// The additivity type (additive, semi_additive, non_additive)
    pub additivity_type: String,
    /// For semi-additive: the dimension(s) that cannot be summed across
    pub non_additive_dimensions: Vec<String>,
    /// The window function used for semi-additive resolution (e.g., "last", "first")
    pub window_function: Option<String>,
}

impl AdditivityAnnotation {
    pub fn additive() -> Self {
        Self {
            additivity_type: "additive".to_string(),
            non_additive_dimensions: Vec::new(),
            window_function: None,
        }
    }

    pub fn semi_additive(
        non_additive_dims: Vec<String>,
        window_fn: impl Into<String>,
    ) -> Self {
        Self {
            additivity_type: "semi_additive".to_string(),
            non_additive_dimensions: non_additive_dims,
            window_function: Some(window_fn.into()),
        }
    }

    pub fn non_additive() -> Self {
        Self {
            additivity_type: "non_additive".to_string(),
            non_additive_dimensions: Vec::new(),
            window_function: None,
        }
    }
}
