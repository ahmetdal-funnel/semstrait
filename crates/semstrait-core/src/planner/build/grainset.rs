//! Grainset plan building — single or multi-dataset.

use crate::diagnostics::CompileError;
use crate::schema::model::Kind;

use crate::planner::ir::expr::{Column, Expr};
use crate::planner::ir::plan_node::*;
use crate::planner::resolve::grainset::GrainsetPlan;

use super::common::*;

pub(super) fn grainset_to_plan(
    kind: &Kind,
    plan: &GrainsetPlan,
    dimensions: &[String],
    measures: &[String],
) -> Result<PlanNode, CompileError> {
    if plan.needs_union {
        grainset_union_plan(kind, plan, dimensions, measures)
    } else {
        grainset_single_plan(kind, plan, dimensions, measures)
    }
}

/// Single dataset grainset: Scan → [BucketProject?] → Filter? → Aggregate → [AdditivityWrap?] → Project
fn grainset_single_plan(
    kind: &Kind,
    plan: &GrainsetPlan,
    dimensions: &[String],
    measures: &[String],
) -> Result<PlanNode, CompileError> {
    let ds = get_dataset(kind, plan.selected_datasets[0])?;
    let mappings = &plan.column_mappings[0]; // [(semantic, physical)]

    // Detect bucketed dimensions and collect their source columns
    let bucketed_dims = collect_bucketed_dims(kind, dimensions);
    let has_bucketed = !bucketed_dims.is_empty();

    // Pre-compute extra dims needed for semi-additive pre-resolution
    let extra_dims = collect_additivity_extra_dims(kind, measures, dimensions);

    // Build Scan — include source columns for bucketed dims and extra additivity dims
    let (mut columns, mut types) =
        scan_columns_from_mappings_with_bucketed(kind, mappings, dimensions, measures, &bucketed_dims);

    // Add extra additivity dims to Scan
    {
        let seen: std::collections::HashSet<String> = columns.iter().cloned().collect();
        for dim in &extra_dims {
            if let Some(phys) = find_physical(mappings, dim) {
                if !seen.contains(&phys) {
                    columns.push(phys);
                    types.push(lookup_column_type(kind, dim));
                }
            }
        }
    }

    let table = resolve_table_path(ds);
    let mut node = PlanNode::Scan(
        Scan::new(&table)
            .with_alias(&ds.name)
            .with_columns(columns, types),
    );

    // Temporal filter
    if let Some(pred) = build_temporal_filter(ds, &ds.name) {
        node = PlanNode::Filter(Filter {
            input: Box::new(node),
            predicate: pred,
        });
    }

    // If there are bucketed dims, insert a pre-aggregate Project that computes CASE WHEN
    // and passes through other columns. After this, all dims have semantic-level aliases.
    if has_bucketed {
        node = build_bucketed_project(node, kind, dimensions, measures, mappings, &bucketed_dims);
    }

    // Aggregate
    let group_by: Vec<Column> = if has_bucketed {
        dimensions
            .iter()
            .map(Column::unqualified)
            .collect()
    } else {
        dimensions
            .iter()
            .filter_map(|d| find_physical(mappings, d))
            .map(|phys| Column::unqualified(&phys))
            .collect()
    };

    let aggregates = build_aggregates(kind, measures, mappings)?;

    node = PlanNode::Aggregate(Aggregate {
        input: Box::new(node),
        group_by,
        aggregates,
    });

    // Additivity: wrap in two-stage aggregation if semi-additive measures present
    let addit_mappings = if has_bucketed { None } else { Some(mappings.as_slice()) };
    node = maybe_wrap_additivity(node, kind, measures, dimensions, addit_mappings)?;

    // Project: rename physical → semantic
    if has_bucketed {
        let mut expressions = Vec::new();
        for dim in dimensions {
            expressions.push(ProjectExpr {
                expr: Expr::Column(Column::unqualified(dim)),
                alias: dim.clone(),
            });
        }
        for measure in measures {
            expressions.push(ProjectExpr {
                expr: Expr::Column(Column::unqualified(measure)),
                alias: measure.clone(),
            });
        }
        node = PlanNode::Project(Project {
            input: Box::new(node),
            expressions,
        });
    } else {
        node = build_rename_project(node, dimensions, measures, mappings);
    }

    Ok(node)
}

/// Multi-dataset grainset: per-dataset (Scan → Filter? → Project) → Union → Aggregate
fn grainset_union_plan(
    kind: &Kind,
    plan: &GrainsetPlan,
    dimensions: &[String],
    measures: &[String],
) -> Result<PlanNode, CompileError> {
    let mut branches = Vec::new();

    for (i, &ds_idx) in plan.selected_datasets.iter().enumerate() {
        let ds = get_dataset(kind, ds_idx)?;
        let mappings = &plan.column_mappings[i];

        let (columns, types) = scan_columns_from_mappings(kind, mappings, dimensions, measures);
        let table = resolve_table_path(ds);
        let mut branch = PlanNode::Scan(
            Scan::new(&table)
                .with_alias(&ds.name)
                .with_columns(columns, types),
        );

        if let Some(pred) = build_temporal_filter(ds, &ds.name) {
            branch = PlanNode::Filter(Filter {
                input: Box::new(branch),
                predicate: pred,
            });
        }

        // Project to align schema (physical → semantic names)
        branch = build_align_project(branch, dimensions, measures, mappings);
        branches.push(branch);
    }

    let mut node = PlanNode::Union(Union { inputs: branches });

    // Aggregate on unified semantic names
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
