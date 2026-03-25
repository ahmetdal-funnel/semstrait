//! KindPlanner trait, registry, and kind-specific planner implementations.
//!
//! - [`KindPlanner`] trait: strategy pattern for kind-specific planning
//! - [`GrainsetPlanner`]: single-dataset covering strategy
//! - [`UnionsetPlanner`]: UNION ALL across multiple datasets
//! - [`JoinsetPlanner`]: BFS join chain across datasets

pub(crate) mod common;
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
    ColumnMappingValue, CompiledKind, CompiledKindDataset, CompiledKindType, CompiledManifest,
    DimensionType, MetadataDimension, TemporalGrain,
};

pub use grainset::GrainsetPlanner;
pub use joinset::JoinsetPlanner;
pub use unionset::UnionsetPlanner;

/// Context passed to kind planners during resolution.
pub struct PlannerContext<'a> {
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
///
/// Literal and Anchored variants must be handled before calling this function
/// (Literal is routed to metadata_literals; Anchored is flattened to Simple
/// entries during compilation step 4.5).
pub fn resolve_column_name(mapping_value: &ColumnMappingValue) -> &str {
    match mapping_value {
        ColumnMappingValue::Simple(s) => s.as_str(),
        ColumnMappingValue::WithGrain { column, .. } => column.as_str(),
        ColumnMappingValue::Literal(_) | ColumnMappingValue::Anchored(_) => {
            unreachable!("Literal/Anchored must be handled before resolve_column_name")
        }
    }
}

/// Partition requested dimensions into (metadata, regular).
///
/// Metadata dimensions are virtual — their values come from source metadata
/// (path segments, partition values), not from physical columns.
/// Regular dimensions are resolved via column_mapping.
pub(crate) fn partition_dimensions(
    request_dims: &[String],
    kind: &CompiledKind,
) -> (Vec<(String, MetadataDimension)>, Vec<String>) {
    let mut metadata = Vec::new();
    let mut regular = Vec::new();

    for dim_name in request_dims {
        if let Some(dim) = kind.dimensions.get(dim_name) {
            if let DimensionType::Metadata(meta) = &dim.dim_type {
                metadata.push((dim_name.clone(), meta.clone()));
                continue;
            }
        }
        regular.push(dim_name.clone());
    }

    (metadata, regular)
}

/// Extract the metadata dimension value for a specific dataset.
///
/// For `PathExtraction { token }`: splits the first resolved_source on `/`
/// and returns the segment at position `token` (0-indexed).
///
/// For `PartitionExtraction { level }`: splits the storage path, finds
/// Hive-style `key=value` segments, and returns the value at the given
/// level (1-indexed).
///
/// Returns `None` if extraction fails (no sources, token out of range, etc.).
pub(crate) fn extract_metadata_value(
    meta: &MetadataDimension,
    dataset: &CompiledKindDataset,
) -> Option<String> {
    let source = &dataset.resolved_sources.first()?.reference;

    if let Some(ref path_ext) = meta.path {
        let segments: Vec<&str> = source.split('/').collect();
        return segments.get(path_ext.token).map(|s: &&str| s.to_string());
    }

    if let Some(ref part_ext) = meta.partition {
        // Find Hive-style key=value segments and return the value at the given level.
        let kv_segments: Vec<&str> = source
            .split('/')
            .filter(|s| s.contains('='))
            .collect();
        if part_ext.level == 0 || part_ext.level > kv_segments.len() {
            return None;
        }
        let segment = kv_segments[part_ext.level - 1];
        return segment.split_once('=').map(|(_, v): (&str, &str)| v.to_string());
    }

    None
}

/// Resolve the native temporal grain for a dataset's temporal dimension.
///
/// If the column mapping uses `WithGrain { grain: Some(g) }`, returns that grain.
/// If grain is unspecified (None) or the mapping is Simple, returns the finest
/// grain from the kind-level temporal dimension definition.
pub(crate) fn resolve_native_grain(
    dataset: &CompiledKindDataset,
    dim_name: &str,
    kind: &CompiledKind,
) -> Option<TemporalGrain> {
    // Check dataset-level explicit grain.
    if let Some(ColumnMappingValue::WithGrain { grain: Some(g), .. }) =
        dataset.extras.column_mapping.get(dim_name)
    {
        return Some(*g);
    }

    // Fall back to finest kind-level grain.
    if let Some(dim) = kind.dimensions.get(dim_name) {
        if let DimensionType::Temporal(ref td) = dim.dim_type {
            return td.grains.iter().copied().min_by_key(|g| g.coarseness());
        }
    }
    None
}

/// Find the temporal dimension name in a kind (first temporal dimension found).
pub(crate) fn find_temporal_dimension(kind: &CompiledKind) -> Option<&str> {
    kind.dimensions.iter().find_map(|(name, dim)| {
        if matches!(dim.dim_type, DimensionType::Temporal(_)) {
            Some(name.as_str())
        } else {
            None
        }
    })
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
    use semstrait_manifest::{KindDatasetExtras, PathExtraction, PartitionExtraction};

    fn make_dataset(sources: Vec<&str>) -> CompiledKindDataset {
        CompiledKindDataset {
            name: "test".to_string(),
            extras: KindDatasetExtras {
                column_mapping: std::collections::HashMap::new().into(),
                temporal: None,
                storage: None,
                catalog: None,
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
        let ds = make_dataset(vec!["bucket/shopify/data.parquet"]);
        assert_eq!(extract_metadata_value(&meta, &ds), Some("shopify".to_string()));
    }

    #[test]
    fn test_path_extraction_token_zero() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 0 }),
            partition: None,
        };
        let ds = make_dataset(vec!["bucket/shopify/data.parquet"]);
        assert_eq!(extract_metadata_value(&meta, &ds), Some("bucket".to_string()));
    }

    #[test]
    fn test_path_extraction_out_of_range() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 99 }),
            partition: None,
        };
        let ds = make_dataset(vec!["bucket/shopify/data.parquet"]);
        assert_eq!(extract_metadata_value(&meta, &ds), None);
    }

    #[test]
    fn test_partition_extraction() {
        let meta = MetadataDimension {
            path: None,
            partition: Some(PartitionExtraction { level: 1 }),
        };
        let ds = make_dataset(vec!["bucket/year=2024/month=01/data.parquet"]);
        assert_eq!(extract_metadata_value(&meta, &ds), Some("2024".to_string()));
    }

    #[test]
    fn test_partition_extraction_level_two() {
        let meta = MetadataDimension {
            path: None,
            partition: Some(PartitionExtraction { level: 2 }),
        };
        let ds = make_dataset(vec!["bucket/year=2024/month=01/data.parquet"]);
        assert_eq!(extract_metadata_value(&meta, &ds), Some("01".to_string()));
    }

    #[test]
    fn test_partition_extraction_out_of_range() {
        let meta = MetadataDimension {
            path: None,
            partition: Some(PartitionExtraction { level: 5 }),
        };
        let ds = make_dataset(vec!["bucket/year=2024/data.parquet"]);
        assert_eq!(extract_metadata_value(&meta, &ds), None);
    }

    #[test]
    fn test_no_sources_returns_none() {
        let meta = MetadataDimension {
            path: Some(PathExtraction { token: 0 }),
            partition: None,
        };
        let ds = make_dataset(vec![]);
        assert_eq!(extract_metadata_value(&meta, &ds), None);
    }
}
