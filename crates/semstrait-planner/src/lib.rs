//! Semantic query planner with kind-specific planning strategies.
//!
//! Builds `LogicalPlan` from `ResolvedQueryRequest` + `CompiledManifest`.
//! Dispatches to kind-specific planners (Grainset, Unionset, Joinset).
//! Evaluates constraints, additivity, filters. Applies optimizer internally.
//!
//! # Architecture
//!
//! The planner follows the pipeline defined in CONTEXT.md section 5.6:
//!
//! 1. `ConstraintValidator::check()` — validate measure/dimension constraints
//! 2. `KindPlannerRegistry::dispatch()` — route to correct KindPlanner
//! 3. `KindPlanner::resolve()` — build PlanFragment
//! 4. `AdditivityResolver` — handle semi/non-additive measures
//! 5. Filter injection (dataset, measure, metric, user filters)
//! 6. `Optimizer::apply()` — identity in v1

pub mod error;
pub mod request;
pub(crate) mod validator;
pub(crate) mod expr;
pub(crate) mod resolver;
pub(crate) mod decomposer;
pub(crate) mod kind;
pub(crate) mod additivity;
pub(crate) mod entity_resolver;
pub(crate) mod ad_hoc_join;
pub(crate) mod simplify;
pub(crate) mod optimizer;
pub mod planner;

#[cfg(test)]
mod tests;

// Re-export primary public API.
pub use error::PlannerError;
pub use request::{
    FilterOperator, FilterValue, OrderByClause, QueryFilter, ResolvedQueryRequest,
    SessionVariables, SortDirection,
};
pub use planner::{SemanticPlanner, SemanticPlannerBuilder};
