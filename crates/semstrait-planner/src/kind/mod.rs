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
use semstrait_core::EngineProfile;
use semstrait_ir::{Expr, PlanNode, Schema};
use semstrait_manifest::{
    CompiledManifest, DataKind, DatasetBinding, DimensionType, KindInterface, MetadataDimension,
    TemporalGrain,
};

pub use grainset::GrainsetPlanner;
pub use joinset::JoinsetPlanner;
pub use unionset::UnionsetPlanner;

/// Context passed to kind planners during resolution.
pub struct PlannerContext<'a> {
    #[allow(dead_code)] // Used in future phases (manifest-aware planning)
    pub manifest: &'a CompiledManifest,
    #[allow(dead_code)] // Used in future phases (catalog-aware planning)
    pub profile: &'a dyn EngineProfile,
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
    /// Returns true if this planner handles the given DataKind variant.
    fn supports(&self, data_kind: &DataKind) -> bool;

    /// Build a PlanFragment for the given DataKind and request.
    fn resolve(
        &self,
        data_kind: &DataKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError>;
}

/// Registry that dispatches to the appropriate KindPlanner based on DataKind variant.
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

    /// Dispatch to the appropriate planner for the given DataKind.
    pub fn dispatch(&self, data_kind: &DataKind) -> Result<&dyn KindPlanner, PlannerError> {
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

/// Dispatch a DataKind to the appropriate plan builder.
///
/// Dataset kinds use the single-dataset fast path; complex kinds (grainset,
/// unionset, joinset) delegate to their respective KindPlanner.
pub fn dispatch_data_kind(
    data_kind: &DataKind,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
    registry: &KindPlannerRegistry,
) -> Result<PlanFragment, PlannerError> {
    match data_kind {
        DataKind::Dataset(dk) => {
            shared::build_dataset_kind_plan(dk, request, ctx)
        }
        _ => {
            let planner = registry.dispatch(data_kind)?;
            planner.resolve(data_kind, request, ctx)
        }
    }
}

/// Partition requested dimensions into (metadata, regular) using KindInterface.
pub(crate) fn partition_dimensions_iface(
    request_dims: &[String],
    iface: &KindInterface,
) -> (Vec<(String, MetadataDimension)>, Vec<String>) {
    let mut metadata = Vec::new();
    let mut regular = Vec::new();

    for dim_name in request_dims {
        if let Some(dim) = iface.dimensions.get(dim_name) {
            if let DimensionType::Metadata(meta) = &dim.dim_type {
                metadata.push((dim_name.clone(), meta.clone()));
                continue;
            }
        }
        regular.push(dim_name.clone());
    }

    (metadata, regular)
}

/// Extract metadata dimension value from a DatasetBinding's resolved sources.
pub(crate) fn extract_metadata_value_binding(
    meta: &MetadataDimension,
    binding: &DatasetBinding,
) -> Option<String> {
    let first = binding.resolved_sources.first()?;
    let source = first.location.as_deref().unwrap_or(&first.reference);

    if let Some(ref path_ext) = meta.path {
        let segments: Vec<&str> = source.split('/').collect();
        return segments.get(path_ext.token).map(|s: &&str| s.to_string());
    }

    if let Some(ref part_ext) = meta.partition {
        let kv_segments: Vec<&str> = source.split('/').filter(|s| s.contains('=')).collect();
        if part_ext.level == 0 || part_ext.level > kv_segments.len() {
            return None;
        }
        let segment = kv_segments[part_ext.level - 1];
        return segment.split_once('=').map(|(_, v): (&str, &str)| v.to_string());
    }

    None
}

/// Resolve the native temporal grain for a binding's temporal dimension.
pub(crate) fn resolve_native_grain_binding(
    binding: &DatasetBinding,
    dim_name: &str,
    iface: &KindInterface,
) -> Option<TemporalGrain> {
    // Check binding-level explicit grain from ResolvedColumnMapping.temporal.
    if let Some(tm) = binding.column_mapping.temporal.get(dim_name) {
        if let Some(g) = tm.grain {
            return Some(g);
        }
    }

    // Fall back to finest kind-level grain.
    if let Some(dim) = iface.dimensions.get(dim_name) {
        if let DimensionType::Temporal(ref td) = dim.dim_type {
            return td.grains.iter().copied().min_by_key(|g| g.coarseness());
        }
    }
    None
}

/// Convert a core `Grain` to model `TemporalGrain`.
pub(crate) fn grain_to_temporal(grain: semstrait_core::Grain) -> TemporalGrain {
    match grain {
        semstrait_core::Grain::Minute => TemporalGrain::Minute,
        semstrait_core::Grain::Hour => TemporalGrain::Hour,
        semstrait_core::Grain::Day => TemporalGrain::Day,
        semstrait_core::Grain::Week => TemporalGrain::Week,
        semstrait_core::Grain::Month => TemporalGrain::Month,
        semstrait_core::Grain::Quarter => TemporalGrain::Quarter,
        semstrait_core::Grain::Year => TemporalGrain::Year,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_manifest::{PathExtraction, PartitionExtraction};
    use semstrait_manifest::acceleration::{DatasetBinding, ResolvedColumnMapping};

    fn make_binding(sources: Vec<&str>) -> DatasetBinding {
        DatasetBinding {
            dataset_name: "test".to_string(),
            column_mapping: ResolvedColumnMapping {
                physical: indexmap::IndexMap::new(),
                literals: std::collections::HashMap::new(),
                temporal: std::collections::HashMap::new(),
                anchored: std::collections::HashMap::new(),
            },
            resolved_sources: sources.into_iter().map(semstrait_manifest::ResolvedSource::path).collect(),
        }
    }

    #[test]
    fn test_path_extraction() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 1 }),
            partition: None,
        };
        let binding = make_binding(vec!["bucket/shopify/data.parquet"]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), Some("shopify".to_string()));
    }

    #[test]
    fn test_path_extraction_token_zero() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 0 }),
            partition: None,
        };
        let binding = make_binding(vec!["bucket/shopify/data.parquet"]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), Some("bucket".to_string()));
    }

    #[test]
    fn test_path_extraction_out_of_range() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 99 }),
            partition: None,
        };
        let binding = make_binding(vec!["bucket/shopify/data.parquet"]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), None);
    }

    #[test]
    fn test_partition_extraction() {
        let meta = MetadataDimension {
            path: None,
            partition: Some(PartitionExtraction { level: 1 }),
        };
        let binding = make_binding(vec!["bucket/year=2024/month=01/data.parquet"]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), Some("2024".to_string()));
    }

    #[test]
    fn test_partition_extraction_level_two() {
        let meta = MetadataDimension {
            path: None,
            partition: Some(PartitionExtraction { level: 2 }),
        };
        let binding = make_binding(vec!["bucket/year=2024/month=01/data.parquet"]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), Some("01".to_string()));
    }

    #[test]
    fn test_partition_extraction_out_of_range() {
        let meta = MetadataDimension {
            path: None,
            partition: Some(PartitionExtraction { level: 5 }),
        };
        let binding = make_binding(vec!["bucket/year=2024/data.parquet"]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), None);
    }

    #[test]
    fn test_no_sources_returns_none() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 0 }),
            partition: None,
        };
        let binding = make_binding(vec![]);
        assert_eq!(extract_metadata_value_binding(&meta, &binding), None);
    }
}
