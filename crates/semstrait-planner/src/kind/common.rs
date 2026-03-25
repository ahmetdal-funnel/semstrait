//! CommonDataset fast path — direct Scan → Aggregate → Project for single-dataset entities.
//!
//! Bypasses the KindPlanner registry and complex routing. Uses pre-resolved
//! `ResolvedColumnMapping.physical` for direct column mapping.

use crate::error::PlannerError;
use crate::expr_lower;
use super::grainset::collect_column_refs;
use super::{PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, AggregateMeasure, Aggregation, Expr, Field, NodeMeta, PlanNode, ProjectNode,
    ScanNode, Schema, UnionNode,
};
use semstrait_manifest::{CommonDataset, DimensionType, MetadataDimension, ResolvedSource};
use std::collections::HashSet;

/// Build a plan for a CommonDataset entity (single-dataset, no routing).
///
/// Uses `ResolvedColumnMapping.physical` for direct column resolution,
/// handling metadata dimensions, literal dimensions, and temporal dimensions.
pub(crate) fn build_common_dataset_plan(
    dataset: &CommonDataset,
    request: &ResolvedQueryRequest,
    _ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &dataset.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Partition dimensions into metadata and regular.
    let (metadata_dims, regular_dims) = partition_common_dimensions(&request.dimensions, dataset);

    // Map regular dimensions to physical columns or literal values.
    let mut dim_physical: Vec<(String, String)> = Vec::new();
    let mut metadata_literals: Vec<(String, Expr)> = Vec::new();

    for dim_name in &regular_dims {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            metadata_literals.push((dim_name.clone(), Expr::string(lit_val.clone())));
        } else if let Some(phys) = mapping.physical.get(dim_name) {
            dim_physical.push((dim_name.clone(), phys.clone()));
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys.clone());
            }
        } else {
            return Err(PlannerError::DimensionNotFound {
                kind: dataset.name.clone(),
                dimension: dim_name.clone(),
            });
        }
    }

    // Extract metadata dimension values as literals.
    for (dim_name, meta) in &metadata_dims {
        let value = extract_common_metadata(meta, &dataset.resolved_sources).unwrap_or_default();
        metadata_literals.push((dim_name.clone(), Expr::string(value)));
    }

    // Lower measures via the parsed Expr tree or declarative agg tag.
    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = dataset.measures.get(measure_name) {
            let lowered = if let Some(agg) = measure.agg {
                expr_lower::lower_measure_declarative_physical(
                    measure_name,
                    agg,
                    &measure.expr,
                    &mapping.physical,
                    &measure.filters,
                )?
            } else {
                expr_lower::lower_measure_with_filters_physical(
                    measure_name,
                    &measure.expr,
                    &mapping.physical,
                    &measure.filters,
                )?
            };
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if dataset.metrics.contains_key(measure_name) {
            // Metrics are derived — treat as a SUM pass-through.
            let phys = mapping
                .physical
                .get(measure_name)
                .map(|s| s.as_str())
                .unwrap_or(measure_name.as_str());
            let lowered = expr_lower::LoweredMeasure {
                aggregates: vec![AggregateMeasure {
                    function: Aggregation::Sum,
                    expr: Expr::column(phys),
                    distinct: false,
                }],
                post_agg_expr: Expr::column(measure_name.clone()),
            };
            if scan_seen.insert(phys.to_string()) {
                scan_columns.push(phys.to_string());
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: dataset.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Build Scan node (multi-source aware).
    let scan = build_common_scan_node(dataset, &scan_columns);

    // Build Aggregate node.
    let group_by: Vec<Expr> = dim_physical
        .iter()
        .map(|(_, physical)| Expr::column(physical.clone()))
        .collect();

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = dim_physical
        .iter()
        .map(|(semantic, _)| Field::new(semantic.clone(), DataType::Utf8))
        .collect();
    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        for (j, _) in lowered.aggregates.iter().enumerate() {
            if j == 0 {
                agg_fields.push(Field::new(semantic.clone(), DataType::Float64));
            } else {
                agg_fields.push(Field::new(format!("__agg_{}", agg_idx), DataType::Float64));
            }
            agg_idx += 1;
        }
    }
    let agg_schema = Schema::new(agg_fields);

    let agg = PlanNode::Aggregate(AggNode {
        meta: NodeMeta::new(agg_schema),
        input: Box::new(scan),
        group_by,
        aggregates,
    });

    // Build Project node.
    let mut project_exprs: Vec<Expr> = Vec::new();
    let mut project_fields: Vec<Field> = Vec::new();

    for dim_name in &request.dimensions {
        if let Some((_, lit_expr)) = metadata_literals.iter().find(|(n, _)| n == dim_name) {
            project_exprs.push(lit_expr.clone());
        } else {
            project_exprs.push(Expr::column(dim_name.clone()));
        }
        project_fields.push(Field::new(dim_name.clone(), DataType::Utf8));
    }
    for (_, lowered) in &lowered_measures {
        project_exprs.push(lowered.post_agg_expr.clone());
    }
    project_fields.extend(
        lowered_measures
            .iter()
            .map(|(name, _)| Field::new(name.clone(), DataType::Float64)),
    );
    let project_schema = Schema::new(project_fields);

    let project = PlanNode::Project(ProjectNode {
        meta: NodeMeta::new(project_schema.clone()),
        input: Box::new(agg),
        expressions: project_exprs,
    });

    Ok(PlanFragment {
        root: project,
        output_schema: project_schema,
        pending_filters: Vec::new(),
    })
}

/// Partition requested dimensions into (metadata, regular) for a CommonDataset.
fn partition_common_dimensions(
    request_dims: &[String],
    dataset: &CommonDataset,
) -> (Vec<(String, MetadataDimension)>, Vec<String>) {
    let mut metadata = Vec::new();
    let mut regular = Vec::new();

    for dim_name in request_dims {
        if let Some(dim) = dataset.dimensions.get(dim_name) {
            if let DimensionType::Metadata(meta) = &dim.dim_type {
                metadata.push((dim_name.clone(), meta.clone()));
                continue;
            }
        }
        regular.push(dim_name.clone());
    }

    (metadata, regular)
}

/// Extract metadata dimension value from resolved sources.
fn extract_common_metadata(
    meta: &MetadataDimension,
    resolved_sources: &[ResolvedSource],
) -> Option<String> {
    let source = resolved_sources.first()?;
    let path = &source.reference;

    if let Some(ref path_ext) = meta.path {
        let segments: Vec<&str> = path.split('/').collect();
        return segments.get(path_ext.token).map(|s| s.to_string());
    }

    if let Some(ref part_ext) = meta.partition {
        let kv_segments: Vec<&str> = path.split('/').filter(|s| s.contains('=')).collect();
        if part_ext.level == 0 || part_ext.level > kv_segments.len() {
            return None;
        }
        let segment = kv_segments[part_ext.level - 1];
        return segment.split_once('=').map(|(_, v)| v.to_string());
    }

    None
}

/// Build a scan node for a CommonDataset (handles multi-source).
fn build_common_scan_node(
    dataset: &CommonDataset,
    scan_columns: &[String],
) -> PlanNode {
    let scan_schema = Schema::new(
        scan_columns
            .iter()
            .map(|c| Field::new(c.clone(), DataType::Utf8))
            .collect(),
    );

    if dataset.resolved_sources.len() <= 1 {
        let table_name = dataset
            .resolved_sources
            .first()
            .map(|s| s.reference.as_str())
            .unwrap_or(&dataset.dataset_ref);
        PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(scan_schema),
            table_name: table_name.to_string(),
            projection: scan_columns.to_vec(),
        })
    } else {
        let inputs: Vec<PlanNode> = dataset
            .resolved_sources
            .iter()
            .map(|source| {
                PlanNode::Scan(ScanNode {
                    meta: NodeMeta::new(scan_schema.clone()),
                    table_name: source.reference.clone(),
                    projection: scan_columns.to_vec(),
                })
            })
            .collect();
        PlanNode::Union(UnionNode {
            meta: NodeMeta::new(scan_schema),
            inputs,
            distinct: false,
        })
    }
}
