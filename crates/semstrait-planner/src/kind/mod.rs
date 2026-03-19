//! KindPlanner trait, registry, and kind-specific planner implementations.
//!
//! - [`KindPlanner`] trait: strategy pattern for kind-specific planning
//! - [`GrainsetPlanner`]: single-dataset covering strategy
//! - [`UnionsetPlanner`]: UNION ALL across multiple datasets
//! - [`JoinsetPlanner`]: BFS join chain across datasets

pub mod grainset;
pub mod joinset;
pub(crate) mod shared;
pub mod unionset;

use crate::error::PlannerError;
use crate::request::{ResolvedQueryRequest, SessionVariables};
use semstrait_catalog::CatalogProvider;
use semstrait_core::ConsumerProfile;
use semstrait_ir::{Expr, PlanNode, Schema};
use semstrait_manifest::{ColumnMappingValue, CompiledKind, CompiledKindType, CompiledManifest};

pub use grainset::GrainsetPlanner;
pub use joinset::JoinsetPlanner;
pub use unionset::UnionsetPlanner;

/// Context passed to kind planners during resolution.
pub struct PlannerContext<'a> {
    pub manifest: &'a CompiledManifest,
    #[allow(dead_code)] // Used in future phases (catalog-aware planning)
    pub profile: &'a ConsumerProfile,
    #[allow(dead_code)] // Used in future phases (catalog-aware planning)
    pub catalog: Option<&'a dyn CatalogProvider>,
    #[allow(dead_code)] // Used in future phases (session-aware planning)
    pub session: &'a SessionVariables,
}

/// A partially-built plan from kind-specific resolution.
#[derive(Debug)]
pub struct PlanFragment {
    /// Root of the plan fragment tree.
    pub root: PlanNode,
    /// Output schema of the fragment.
    #[allow(dead_code)] // Read in tests; planner.rs reads via PlanNode::meta()
    pub(crate) output_schema: Schema,
    /// Filters not yet injected into the plan.
    #[allow(dead_code)] // Read in tests; reserved for filter injection pipeline
    pub(crate) pending_filters: Vec<Expr>,
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

/// Resolve a `ColumnMappingValue` to its physical column name.
pub fn resolve_column_name(mapping_value: &ColumnMappingValue) -> &str {
    match mapping_value {
        ColumnMappingValue::Simple(s) => s.as_str(),
        ColumnMappingValue::WithGrain { column, .. } => column.as_str(),
    }
}
