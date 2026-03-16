//! Unionset plan building — UNION ALL with NULL-fill.

use crate::diagnostics::CompileError;
use crate::schema::model::Kind;

use crate::planner::ir::expr::{Column, Expr, Literal};
use crate::planner::ir::plan_node::*;
use crate::planner::resolve::unionset::UnionsetPlan;

use super::common::*;

pub(super) fn unionset_to_plan(
    kind: &Kind,
    plan: &UnionsetPlan,
    dimensions: &[String],
    measures: &[String],
) -> Result<PlanNode, CompileError> {
    let mut branches = Vec::new();

    for branch in &plan.branches {
        let ds = get_dataset(kind, branch.dataset_index)?;
        let table = resolve_table_path(ds);

        // Collect physical columns that exist (non-NULL)
        let existing: Vec<(&str, &str)> = branch
            .column_map
            .iter()
            .filter_map(|(logical, physical)| {
                physical.as_ref().map(|p| (logical.as_str(), p.as_str()))
            })
            .collect();

        let (columns, types): (Vec<String>, Vec<String>) = existing
            .iter()
            .map(|(logical, phys)| {
                let dt = lookup_column_type(kind, logical);
                (phys.to_string(), dt)
            })
            .unzip();

        let mut node = PlanNode::Scan(
            Scan::new(&table)
                .with_alias(&branch.dataset_name)
                .with_columns(columns, types),
        );

        if let Some(pred) = build_temporal_filter(ds, &branch.dataset_name) {
            node = PlanNode::Filter(Filter {
                input: Box::new(node),
                predicate: pred,
            });
        }

        // Project: map physical→semantic, NULL-fill missing
        let expressions = branch
            .column_map
            .iter()
            .map(|(logical, physical)| {
                let expr = match physical {
                    Some(phys) => Expr::Column(Column::unqualified(phys)),
                    None => {
                        let dt = lookup_column_type(kind, logical);
                        Expr::Literal(Literal::Null(dt))
                    }
                };
                ProjectExpr {
                    expr,
                    alias: logical.clone(),
                }
            })
            .collect();

        node = PlanNode::Project(Project {
            input: Box::new(node),
            expressions,
        });

        branches.push(node);
    }

    if branches.len() < 2 {
        // Single branch — skip Union
        let mut node = branches.into_iter().next().unwrap();
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
        node = maybe_wrap_additivity(node, kind, measures, dimensions, None)?;
        return Ok(node);
    }

    let mut node = PlanNode::Union(Union { inputs: branches });

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

    node = maybe_wrap_additivity(node, kind, measures, dimensions, None)?;

    Ok(node)
}
