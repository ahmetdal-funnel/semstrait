//! Shared plan-building utilities used by grainset, joinset, and unionset planners.

use crate::error::PlannerError;
use crate::expr_lower;
use super::{resolve_column_name, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, Aggregation, AggregateMeasure, Expr, Field, NodeMeta, PlanNode, ProjectNode,
    ScanNode, Schema,
};
use semstrait_manifest::{CompiledKind, CompiledKindDataset};
use std::collections::HashSet;

use super::grainset::collect_column_refs;

/// Resolve the physical table name for a dataset binding.
///
/// In v1, always returns the dataset binding name. Future versions could
/// look up a physical table name from the manifest.
fn resolve_table_name(dataset_binding: &CompiledKindDataset) -> &str {
    &dataset_binding.name
}

/// Build a Scan → Aggregate → Project plan for a single dataset.
///
/// This is the common pattern shared by grainset (single-dataset path) and
/// joinset (single-dataset degenerate case). It:
/// 1. Maps requested dimensions to physical columns via column_mapping
/// 2. Lowers measures using parsed Expr trees + filters
/// 3. Optionally handles metrics (treated as SUM pass-through)
/// 4. Builds Scan → Aggregate → Project nodes
///
/// Set `handle_metrics = true` for grainset (which supports metrics in the
/// measures list) or `false` for joinset (which does not).
pub(crate) fn build_dataset_plan(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    dataset: &CompiledKindDataset,
    _ctx: &PlannerContext<'_>,
    handle_metrics: bool,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &dataset.extras.column_mapping;
    let table_name = resolve_table_name(dataset);

    // Collect all physical columns we need to scan (preserving insertion order).
    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Map dimensions to physical columns.
    let mut dim_physical: Vec<(String, String)> = Vec::new();
    for dim_name in &request.dimensions {
        let physical = mapping
            .get(dim_name)
            .map(resolve_column_name)
            .ok_or_else(|| PlannerError::DimensionNotFound {
                kind: kind.name.clone(),
                dimension: dim_name.clone(),
            })?;
        let phys = physical.to_string();
        dim_physical.push((dim_name.clone(), phys.clone()));
        if scan_seen.insert(phys.clone()) {
            scan_columns.push(phys);
        }
    }

    // Lower measures via the parsed Expr tree.
    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            let lowered = expr_lower::lower_measure_with_filters(
                measure_name,
                &measure.expr,
                mapping,
                &measure.filters,
            )?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if handle_metrics && kind.metrics.contains_key(measure_name) {
            // Metrics are derived — treat as a SUM pass-through column.
            let physical = mapping
                .get(measure_name)
                .map(resolve_column_name)
                .unwrap_or(measure_name.as_str());
            let phys = physical.to_string();
            let lowered = expr_lower::LoweredMeasure {
                aggregates: vec![AggregateMeasure {
                    function: Aggregation::Sum,
                    expr: Expr::column(phys.clone()),
                    distinct: false,
                }],
                post_agg_expr: Expr::column(measure_name.clone()),
            };
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: kind.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Build Scan node.
    let scan_schema = Schema::new(
        scan_columns
            .iter()
            .map(|c| Field::new(c.clone(), DataType::Utf8))
            .collect(),
    );
    let scan = PlanNode::Scan(ScanNode {
        meta: NodeMeta::new(scan_schema),
        table_name: table_name.to_string(),
        projection: scan_columns,
    });

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

    // Build Project node — maps physical names back to semantic names.
    let mut project_exprs: Vec<Expr> = request
        .dimensions
        .iter()
        .map(|name| Expr::column(name.clone()))
        .collect();
    for (_, lowered) in &lowered_measures {
        project_exprs.push(lowered.post_agg_expr.clone());
    }

    let project_fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), DataType::Utf8))
        .chain(
            lowered_measures
                .iter()
                .map(|(name, _)| Field::new(name.clone(), DataType::Float64)),
        )
        .collect();
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
