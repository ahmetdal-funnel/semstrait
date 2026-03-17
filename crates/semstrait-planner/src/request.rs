//! ResolvedQueryRequest — the planner's input type.
//!
//! Produced by the API layer's RequestParser from a raw QueryRequest.
//! Contains only resolved names (no raw SQL, no unresolved references).

use semstrait_core::Grain;
use std::collections::HashMap;

/// The resolved query request — input to `SemanticPlanner::plan()`.
#[derive(Debug, Clone)]
pub struct ResolvedQueryRequest {
    /// The kind (semantic entity) to query.
    pub kind_name: String,
    /// Semantic dimension names to include in GROUP BY.
    pub dimensions: Vec<String>,
    /// Semantic measure/metric names to include.
    pub measures: Vec<String>,
    /// User-supplied filter predicates.
    pub filters: Vec<QueryFilter>,
    /// Temporal grain for date grouping.
    pub grain: Option<Grain>,
    /// Maximum number of rows to return.
    pub limit: Option<u64>,
    /// ORDER BY clauses.
    pub order_by: Vec<OrderByClause>,
    /// Optional domain hint to narrow candidate datasets.
    pub domain_hint: Option<String>,
    /// Runtime session variables (tenant_id, user_id, etc.).
    pub session_variables: SessionVariables,
}

/// A user-supplied filter predicate.
#[derive(Debug, Clone)]
pub struct QueryFilter {
    /// The dimension or measure name being filtered.
    pub field: String,
    /// The filter operator.
    pub operator: FilterOperator,
    /// The value(s) to compare against.
    pub values: Vec<FilterValue>,
}

/// Filter operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOperator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    In,
    NotIn,
    Between,
    IsNull,
    IsNotNull,
}

/// A filter value (typed).
#[derive(Debug, Clone)]
pub enum FilterValue {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

/// ORDER BY clause.
#[derive(Debug, Clone)]
pub struct OrderByClause {
    /// Field name (dimension or measure).
    pub field: String,
    /// Sort direction.
    pub direction: SortDirection,
}

/// Sort direction for ORDER BY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Runtime session variables provided by the API layer.
/// Used for row-level security, tenant isolation, etc.
pub type SessionVariables = HashMap<String, String>;
