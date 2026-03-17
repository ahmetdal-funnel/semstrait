//! KindPlanner trait and registry — strategy pattern for kind-specific planning.

use crate::error::PlannerError;
use crate::request::{ResolvedQueryRequest, SessionVariables};
use semstrait_catalog::CatalogProvider;
use semstrait_core::ConsumerProfile;
use semstrait_ir::{DslExpr, PlanNode, Schema};
use semstrait_manifest::{CompiledKind, CompiledKindType, CompiledManifest};

/// Context passed to kind planners during resolution.
pub struct PlannerContext<'a> {
    pub manifest: &'a CompiledManifest,
    pub profile: &'a ConsumerProfile,
    pub catalog: Option<&'a dyn CatalogProvider>,
    pub session: &'a SessionVariables,
}

/// A partially-built plan from kind-specific resolution.
#[derive(Debug)]
pub struct PlanFragment {
    /// Root of the plan fragment tree.
    pub root: PlanNode,
    /// Output schema of the fragment.
    pub output_schema: Schema,
    /// Filters not yet injected into the plan.
    pub pending_filters: Vec<DslExpr>,
}

/// Strategy trait for kind-specific plan construction.
pub trait KindPlanner: Send + Sync {
    /// Returns true if this planner handles the given kind type.
    fn supports(&self, kind_type: &CompiledKindType) -> bool;

    /// Build a PlanFragment for the given kind and request.
    fn resolve(
        &self,
        kind: &CompiledKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError>;
}

/// Registry that dispatches to the appropriate KindPlanner based on kind type.
pub struct KindPlannerRegistry {
    planners: Vec<Box<dyn KindPlanner>>,
}

impl KindPlannerRegistry {
    /// Create a new registry with all built-in planners.
    pub fn new() -> Self {
        use crate::grainset_planner::GrainsetPlanner;
        use crate::joinset_planner::JoinsetPlanner;
        use crate::unionset_planner::UnionsetPlanner;

        Self {
            planners: vec![
                Box::new(GrainsetPlanner),
                Box::new(UnionsetPlanner),
                Box::new(JoinsetPlanner),
            ],
        }
    }

    /// Dispatch to the appropriate planner for the given kind type.
    pub fn dispatch(&self, kind_type: &CompiledKindType) -> Result<&dyn KindPlanner, PlannerError> {
        self.planners
            .iter()
            .find(|p| p.supports(kind_type))
            .map(|p| p.as_ref())
            .ok_or_else(|| PlannerError::UnsupportedKindType(format!("{:?}", kind_type)))
    }
}

impl Default for KindPlannerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
