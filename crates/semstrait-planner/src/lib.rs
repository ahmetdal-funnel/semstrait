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
//! 1. `ConstraintEvaluator::check()` — validate measure/dimension constraints
//! 2. `KindPlannerRegistry::dispatch()` — route to correct KindPlanner
//! 3. `KindPlanner::resolve()` — build PlanFragment
//! 4. `AdditivityResolver` — handle semi/non-additive measures
//! 5. Filter injection (dataset, measure, metric, user filters)
//! 6. `Optimizer::apply()` — identity in v1

pub mod error;
pub mod request;
pub mod constraint_evaluator;
pub mod kind_planner;
pub mod expr_lower;
pub mod grainset_planner;
pub mod unionset_planner;
pub mod joinset_planner;
pub mod additivity_resolver;
pub mod optimizer;
pub mod planner;

#[cfg(test)]
pub(crate) mod test_helpers;
#[cfg(test)]
mod integration_tests;

// Re-export primary public API.
pub use error::PlannerError;
pub use request::{
    FilterOperator, FilterValue, OrderByClause, QueryFilter, ResolvedQueryRequest,
    SessionVariables, SortDirection,
};
pub use constraint_evaluator::ConstraintEvaluator;
pub use kind_planner::{resolve_column_name, KindPlanner, KindPlannerRegistry, PlanFragment, PlannerContext};
pub use grainset_planner::GrainsetPlanner;
pub use unionset_planner::UnionsetPlanner;
pub use joinset_planner::JoinsetPlanner;
pub use additivity_resolver::AdditivityResolver;
pub use optimizer::{Optimizer, OptimizerPass};
pub use planner::{SemanticPlanner, SemanticPlannerBuilder};
