//! Expression and metadata utility functions.
//!
//! Helpers for partitioning dimensions, extracting metadata values,
//! resolving temporal grains, and collecting column references from
//! expression trees.

use semstrait_ir::Expr;
use semstrait_manifest::{
    DatasetBinding, DimensionType, CompiledInterface, MetadataDimension, TemporalGrain,
};
use std::collections::HashSet;

/// Partition requested dimensions into (metadata, regular) using CompiledInterface.
///
/// - **Metadata**: `DimensionType::Metadata` — extracted from source paths/partitions
/// - **Regular**: physical column dimensions — scanned and grouped
pub(crate) fn partition_dimensions_iface(
    request_dims: &[String],
    iface: &CompiledInterface,
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

/// Separate computed dimensions from regular ones.
///
/// Computed dimensions have `expr: Some(...)` and are emitted as ProjectNode
/// expressions rather than ScanNode columns.
pub(crate) fn split_computed_dims(
    regular_dims: &[String],
    iface: &CompiledInterface,
) -> (Vec<String>, Vec<(String, semstrait_core::Expr)>) {
    let mut physical = Vec::new();
    let mut computed = Vec::new();

    for dim_name in regular_dims {
        if let Some(dim) = iface.dimensions.get(dim_name) {
            if let Some(ref expr) = dim.expr {
                computed.push((dim_name.clone(), expr.clone()));
                continue;
            }
        }
        physical.push(dim_name.clone());
    }

    (physical, computed)
}

/// Expand Guard sugar to Case in an expression tree without touching column names.
///
/// Guard is a model-level sugar: `GUARD(condition => expr)` → `CASE WHEN condition THEN expr ELSE NULL END`.
/// The PhysicalResolver does this during lowering, but when emitting the original semantic
/// expression (for post-aggregate projection), we need Guard expansion without column remapping.
pub(crate) fn resolve_guards(expr: &Expr) -> Expr {
    use semstrait_core::expr::WhenClause;
    expr.transform(&|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
        match e {
            Expr::Guard(g) => Ok(Some(Expr::case(
                vec![WhenClause::new((*g.condition).clone(), (*g.expr).clone())],
                Some(Expr::null()),
            ))),
            _ => Ok(None),
        }
    })
    .expect("Guard resolution is infallible")
}

/// Extract metadata dimension value from a single `ResolvedSource`.
///
/// Used by per-source layered plan construction (Phase 3) to extract
/// correct metadata values per source instead of always using the first.
pub(crate) fn extract_metadata_value_source(
    meta: &MetadataDimension,
    source: &semstrait_manifest::ResolvedSource,
) -> Option<String> {
    let location = source.location.as_deref().unwrap_or(&source.reference);

    if let Some(ref path_ext) = meta.path {
        let segments: Vec<&str> = location.split('/').collect();
        return segments.get(path_ext.token).map(|s: &&str| s.to_string());
    }

    if let Some(ref part_ext) = meta.partition {
        let kv_segments: Vec<&str> = location.split('/').filter(|s| s.contains('=')).collect();
        if part_ext.level == 0 || part_ext.level > kv_segments.len() {
            return None;
        }
        let segment = kv_segments[part_ext.level - 1];
        return segment.split_once('=').map(|(_, v): (&str, &str)| v.to_string());
    }

    None
}

/// Extract metadata dimension value from a DatasetBinding's resolved sources.
///
/// Delegates to `extract_metadata_value_source` using the first source.
/// For multi-source bindings, use `extract_metadata_value_source` per source instead.
pub(crate) fn extract_metadata_value_binding(
    meta: &MetadataDimension,
    binding: &DatasetBinding,
) -> Option<String> {
    let first = binding.resolved_sources.first()?;
    extract_metadata_value_source(meta, first)
}

/// Resolve the native temporal grain for a binding's temporal dimension.
pub(crate) fn resolve_native_grain_binding(
    binding: &DatasetBinding,
    dim_name: &str,
    iface: &CompiledInterface,
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

/// Recursively collect all column references from an Expr tree.
pub(crate) fn collect_column_refs(expr: &Expr, columns: &mut Vec<String>, seen: &mut HashSet<String>) {
    expr.walk(&mut |node| {
        if let Expr::Column(col) = node {
            if seen.insert(col.name.clone()) {
                columns.push(col.name.clone());
            }
        }
    });
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

    #[test]
    fn test_split_computed_dims() {
        use semstrait_manifest::{CompiledDimension, CategoricalDimension, DimensionType};
        use semstrait_manifest::acceleration::CompiledInterface;
        use indexmap::IndexMap;

        let mut dimensions = IndexMap::new();

        // Physical dimension
        dimensions.insert(
            "region".to_string(),
            CompiledDimension {
                name: "region".to_string(),
                description: None,
                data_type: semstrait_core::DataType::String,
                dim_type: DimensionType::Categorical(CategoricalDimension { enum_values: None }),
                expr: None,
                expr_source: None,
            },
        );

        // Computed dimension
        dimensions.insert(
            "market".to_string(),
            CompiledDimension {
                name: "market".to_string(),
                description: None,
                data_type: semstrait_core::DataType::String,
                dim_type: DimensionType::Categorical(CategoricalDimension { enum_values: None }),
                expr: Some(semstrait_core::Expr::function_call(
                    "UPPER",
                    vec![semstrait_core::Expr::column("region")],
                )),
                expr_source: None,
            },
        );

        let iface = CompiledInterface {
            name: "test".to_string(),
            description: None,
            dimensions,
            measures: IndexMap::new(),
            metrics: IndexMap::new(),
            keys: None,
            filters: vec![],
            temporal_dim: None,
        };

        let requested = vec!["region".to_string(), "market".to_string()];
        let (physical, computed) = split_computed_dims(&requested, &iface);

        assert_eq!(physical, vec!["region".to_string()]);
        assert_eq!(computed.len(), 1);
        assert_eq!(computed[0].0, "market");
        assert_eq!(
            computed[0].1,
            semstrait_core::Expr::function_call("UPPER", vec![semstrait_core::Expr::column("region")])
        );
    }
}
