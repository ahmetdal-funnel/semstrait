//! KindPlanner trait, registry, and kind-specific planner implementations.
//!
//! - [`KindPlanner`] trait: strategy pattern for kind-specific planning
//! - [`DatasetPlanner`]: single-dataset fast path
//! - [`GrainsetPlanner`]: single-dataset covering strategy
//! - [`UnionsetPlanner`]: UNION ALL across multiple datasets
//! - [`JoinsetPlanner`]: BFS join chain across datasets

pub mod dataset;
pub mod grainset;
pub mod joinset;
pub(crate) mod plan_builder;
pub mod unionset;

use crate::error::PlannerError;
use crate::request::{ResolvedQueryRequest, SessionVariables};
use semstrait_catalog::CatalogProvider;
use semstrait_ir::{Expr, PlanBuilder, PlanNode, Schema};
use semstrait_manifest::{
    CompiledManifest, CompiledDataKind, DimensionType, MetadataDimension,
};
use semstrait_manifest::acceleration::DatasetBinding;

// Re-export expression/metadata utilities so kind submodules can use `super::`.
pub(crate) use crate::expr::{
    collect_column_refs, extract_metadata_value_binding, grain_to_temporal,
    partition_dimensions_iface, resolve_native_grain_binding, split_computed_dims,
};

pub use dataset::DatasetPlanner;
pub use grainset::GrainsetPlanner;
pub use joinset::JoinsetPlanner;
pub use unionset::UnionsetPlanner;

/// Context passed to kind planners during resolution.
pub struct PlannerContext<'a> {
    #[allow(dead_code)] // Used in future phases (manifest-aware planning)
    pub manifest: &'a CompiledManifest,
    #[allow(dead_code)] // Used in future phases (catalog-aware planning)
    pub catalog: Option<&'a dyn CatalogProvider>,
    #[allow(dead_code)] // Used in future phases (session-aware planning)
    pub session: &'a SessionVariables,
    /// Engine-specific plan node builder. DefaultPlanBuilder when no adapter is configured.
    #[allow(dead_code)] // Used when adapters override node construction
    pub plan_builder: &'a dyn PlanBuilder,
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

/// A borrowed view of a CompiledDataKind with a subset of active bindings.
///
/// Avoids cloning the entire CompiledDataKind when pruning bindings by metadata
/// or literal filters. The original data kind is borrowed; only active binding
/// indices are tracked.
pub struct PrunedView<'a> {
    data_kind: &'a CompiledDataKind,
    /// None means all bindings are active (common case — avoids allocation).
    active_indices: Option<Vec<usize>>,
}

impl<'a> PrunedView<'a> {
    /// Create a view where all bindings are active.
    pub fn all(data_kind: &'a CompiledDataKind) -> Self {
        Self { data_kind, active_indices: None }
    }

    /// The underlying CompiledDataKind (for variant matching and field access).
    pub fn data_kind(&self) -> &'a CompiledDataKind {
        self.data_kind
    }

    /// Collect active bindings as references.
    pub fn active_bindings(&self) -> Vec<&'a DatasetBinding> {
        let all = self.data_kind.bindings();
        match &self.active_indices {
            None => all.iter().collect(),
            Some(indices) => indices.iter().map(|&i| &all[i]).collect(),
        }
    }

    /// Check if a binding at the given original index is active.
    pub fn is_active(&self, idx: usize) -> bool {
        match &self.active_indices {
            None => idx < self.data_kind.bindings().len(),
            Some(indices) => indices.contains(&idx),
        }
    }

    /// Number of active bindings.
    pub fn active_count(&self) -> usize {
        match &self.active_indices {
            None => self.data_kind.bindings().len(),
            Some(indices) => indices.len(),
        }
    }

    /// Prune bindings by metadata dimension equality filters.
    pub fn prune_by_metadata(
        &mut self,
        request: &ResolvedQueryRequest,
    ) -> Result<(), PlannerError> {
        let iface = self.data_kind.interface();

        // Collect metadata equality filters: (expected_value, MetadataDimension).
        let mut metadata_filters: Vec<(String, MetadataDimension)> = Vec::new();
        for filter in &request.filters {
            if !matches!(filter.operator, crate::request::FilterOperator::Eq) {
                continue;
            }
            if let Some(dim) = iface.dimensions.get(&filter.field) {
                if let DimensionType::Metadata(ref meta) = dim.dim_type {
                    if let Some(crate::request::FilterValue::String(ref val)) = filter.values.first() {
                        metadata_filters.push((val.clone(), meta.clone()));
                    }
                }
            }
        }

        if metadata_filters.is_empty() {
            return Ok(());
        }

        let all_bindings = self.data_kind.bindings();
        let current: Vec<usize> = match &self.active_indices {
            None => (0..all_bindings.len()).collect(),
            Some(indices) => indices.clone(),
        };

        let filtered: Vec<usize> = current
            .into_iter()
            .filter(|&i| {
                metadata_filters.iter().all(|(expected, meta)| {
                    match extract_metadata_value_binding(meta, &all_bindings[i]) {
                        Some(ref actual) => actual == expected,
                        None => true, // conservative: keep if extraction fails
                    }
                })
            })
            .collect();

        if filtered.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: "no datasets match metadata dimension filters".to_string(),
            });
        }

        self.active_indices = Some(filtered);
        Ok(())
    }

    /// Prune bindings by literal column mapping equality filters.
    pub fn prune_by_literals(
        &mut self,
        request: &ResolvedQueryRequest,
    ) -> Result<(), PlannerError> {
        let all_bindings = self.data_kind.bindings();

        // Collect equality filters on fields that have a literal mapping in at least one active binding.
        let active_bindings = self.active_bindings();
        let mut literal_filters: Vec<(String, String)> = Vec::new();
        for filter in &request.filters {
            if !matches!(filter.operator, crate::request::FilterOperator::Eq) {
                continue;
            }
            let field = &filter.field;
            let has_literal = active_bindings.iter().any(|b| b.column_mapping.literals.contains_key(field));
            if has_literal {
                if let Some(crate::request::FilterValue::String(ref val)) = filter.values.first() {
                    literal_filters.push((field.clone(), val.clone()));
                }
            }
        }

        if literal_filters.is_empty() {
            return Ok(());
        }

        let current: Vec<usize> = match &self.active_indices {
            None => (0..all_bindings.len()).collect(),
            Some(indices) => indices.clone(),
        };

        let filtered: Vec<usize> = current
            .into_iter()
            .filter(|&i| {
                literal_filters.iter().all(|(field, expected)| {
                    match all_bindings[i].column_mapping.literals.get(field) {
                        Some(actual) => actual == expected,
                        None => true, // no literal for this field → keep (conservative)
                    }
                })
            })
            .collect();

        if filtered.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: self.data_kind.interface().name.clone(),
                reason: "no datasets match literal dimension filters".to_string(),
            });
        }

        self.active_indices = Some(filtered);
        Ok(())
    }
}

/// Strategy trait for kind-specific plan construction.
pub trait KindPlanner: Send + Sync {
    /// Returns true if this planner handles the given CompiledDataKind variant.
    fn supports(&self, data_kind: &CompiledDataKind) -> bool;

    /// Build a PlanFragment for the given CompiledDataKind and request.
    fn resolve(
        &self,
        pruned: &PrunedView<'_>,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError>;
}

/// Registry that dispatches to the appropriate KindPlanner based on CompiledDataKind variant.
pub struct KindPlannerRegistry {
    planners: Vec<Box<dyn KindPlanner>>,
}

impl KindPlannerRegistry {
    /// Create a new registry with all built-in planners.
    pub fn new() -> Self {
        Self {
            planners: vec![
                Box::new(DatasetPlanner),
                Box::new(GrainsetPlanner),
                Box::new(UnionsetPlanner),
                Box::new(JoinsetPlanner),
            ],
        }
    }

    /// Dispatch to the appropriate planner for the given CompiledDataKind.
    pub fn dispatch(&self, data_kind: &CompiledDataKind) -> Result<&dyn KindPlanner, PlannerError> {
        self.planners
            .iter()
            .find(|p| p.supports(data_kind))
            .map(|p| p.as_ref())
            .ok_or_else(|| PlannerError::UnsupportedKindType(format!("{:?}", data_kind.name())))
    }
}

impl Default for KindPlannerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Dispatch a CompiledDataKind to the appropriate KindPlanner via the registry.
///
/// All kinds (Dataset, Grainset, Unionset, Joinset) go through the registry.
pub fn dispatch_data_kind(
    pruned: &PrunedView<'_>,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
    registry: &KindPlannerRegistry,
) -> Result<PlanFragment, PlannerError> {
    let planner = registry.dispatch(pruned.data_kind())?;
    planner.resolve(pruned, request, ctx)
}

