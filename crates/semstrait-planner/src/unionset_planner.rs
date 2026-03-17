//! UnionsetPlanner — kind planner for Unionset kinds (stub).
//!
//! Builds UNION ALL branches with NULL-fill for unmapped columns.
//! v1 provides a basic stub; full branch pruning is v2.

use crate::error::PlannerError;
use crate::kind_planner::{KindPlanner, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_manifest::{CompiledKind, CompiledKindType};

/// Planner for Unionset kinds (v1 stub).
pub struct UnionsetPlanner;

impl KindPlanner for UnionsetPlanner {
    fn supports(&self, kind_type: &CompiledKindType) -> bool {
        matches!(kind_type, CompiledKindType::Unionset)
    }

    fn resolve(
        &self,
        kind: &CompiledKind,
        _request: &ResolvedQueryRequest,
        _ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        // v1 stub: Unionset planning is not yet implemented.
        Err(PlannerError::UnsupportedKindType(format!(
            "Unionset planning for kind '{}' is not yet implemented (v1 stub)",
            kind.name
        )))
    }
}
