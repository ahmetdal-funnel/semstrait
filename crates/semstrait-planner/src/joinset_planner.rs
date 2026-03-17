//! JoinsetPlanner — kind planner for Joinset kinds (stub).
//!
//! Uses BFS from an anchor dataset to construct a join chain.
//! v1 provides a basic stub; full join pruning and temporal injection is v2.

use crate::error::PlannerError;
use crate::kind_planner::{KindPlanner, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_manifest::{CompiledKind, CompiledKindType};

/// Planner for Joinset kinds (v1 stub).
pub struct JoinsetPlanner;

impl KindPlanner for JoinsetPlanner {
    fn supports(&self, kind_type: &CompiledKindType) -> bool {
        matches!(kind_type, CompiledKindType::Joinset { .. })
    }

    fn resolve(
        &self,
        kind: &CompiledKind,
        _request: &ResolvedQueryRequest,
        _ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        // v1 stub: Joinset planning is not yet implemented.
        Err(PlannerError::UnsupportedKindType(format!(
            "Joinset planning for kind '{}' is not yet implemented (v1 stub)",
            kind.name
        )))
    }
}
