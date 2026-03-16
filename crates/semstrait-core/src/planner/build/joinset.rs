//! Joinset plan building — JOIN tree construction.

use crate::diagnostics::CompileError;
use crate::schema::model::Kind;

use crate::planner::ir::expr::Column;
use crate::planner::ir::plan_node::*;
use crate::planner::resolve::joinset::JoinsetPlan;

use super::common::*;

pub(super) fn joinset_to_plan(
    kind: &Kind,
    plan: &JoinsetPlan,
    dimensions: &[String],
    measures: &[String],
) -> Result<PlanNode, CompileError> {
    // Build anchor Scan
    let anchor_ds = find_dataset_by_name(kind, &plan.anchor)?;
    let anchor_table = resolve_table_path(anchor_ds);
    let anchor_mappings = plan.column_mappings.get(&plan.anchor);
    let (anchor_cols, anchor_types) =
        scan_columns_for_joinset(kind, anchor_mappings, anchor_ds);

    let mut node = PlanNode::Scan(
        Scan::new(&anchor_table)
            .with_alias(&plan.anchor)
            .with_columns(anchor_cols, anchor_types),
    );

    if let Some(pred) = build_temporal_filter(anchor_ds, &plan.anchor) {
        node = PlanNode::Filter(Filter {
            input: Box::new(node),
            predicate: pred,
        });
    }

    // Build join chain
    for edge in &plan.join_edges {
        let right_ds = find_dataset_by_name(kind, &edge.to_dataset)?;
        let right_table = resolve_table_path(right_ds);
        let right_mappings = plan.column_mappings.get(&edge.to_dataset);
        let (right_cols, right_types) =
            scan_columns_for_joinset(kind, right_mappings, right_ds);

        let mut right_node = PlanNode::Scan(
            Scan::new(&right_table)
                .with_alias(&edge.to_dataset)
                .with_columns(right_cols, right_types),
        );

        if let Some(pred) = build_temporal_filter(right_ds, &edge.to_dataset) {
            right_node = PlanNode::Filter(Filter {
                input: Box::new(right_node),
                predicate: pred,
            });
        }

        let join_type = convert_join_type(edge.join_type);

        node = PlanNode::Join(Join {
            left: Box::new(node),
            right: Box::new(right_node),
            join_type,
            left_keys: edge.column_pairs.iter()
                .map(|(from_col, _)| Column::new("_left", from_col))
                .collect(),
            right_keys: edge.column_pairs.iter()
                .map(|(_, to_col)| Column::new("_right", to_col))
                .collect(),
        });
    }

    // Project: select needed columns from join result, rename to semantic names
    let expressions = build_joinset_project_exprs(kind, plan, dimensions, measures);

    node = PlanNode::Project(Project {
        input: Box::new(node),
        expressions,
    });

    // Aggregate on semantic names
    let group_by = dimensions
        .iter()
        .map(Column::unqualified)
        .collect();
    let aggregates = build_aggregates_semantic(kind, measures)?;

    node = PlanNode::Aggregate(Aggregate {
        input: Box::new(node),
        group_by,
        aggregates,
    });

    // Additivity: wrap in two-stage aggregation if semi-additive measures present
    node = maybe_wrap_additivity(node, kind, measures, dimensions, None)?;

    Ok(node)
}
