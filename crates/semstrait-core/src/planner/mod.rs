//! Planner module — kind resolution, plan building, and emission.
//!
//! Organized into submodules:
//! - `ir`: IR foundation (expressions, plan nodes, errors)
//! - `resolve`: Kind resolution algorithms (grainset, unionset, joinset)
//! - `validate`: Constraint checking, additivity, domain, metric chains
//! - `transform`: Expression transformers (temporal filters, bucketed dims)
//! - `build`: Plan building (ResolvedKind → PlanNode tree)
//! - `emit`: Plan emission (PlanNode → SQL / Substrait)

pub(crate) mod ir;
pub(crate) mod resolve;
pub(crate) mod validate;
pub(crate) mod transform;
pub(crate) mod build;
pub(crate) mod emit;
