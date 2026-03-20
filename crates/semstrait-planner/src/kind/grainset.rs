//! GrainsetPlanner — kind planner for Grainset kinds.
//!
//! Routes to the cheapest covering dataset. A covering dataset is one whose
//! column_mapping includes all requested dimensions and measures.
//! If no single dataset covers everything, falls back to a horizontal join:
//! multiple partial-coverage datasets are each aggregated independently, then
//! FULL OUTER JOINed on shared dimension columns.

use crate::error::PlannerError;
use super::{KindPlanner, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    Expr, NodeMeta, PlanNode, ProjectNode, Schema, Field, JoinNode, JoinType,
};
use semstrait_manifest::{CompiledKind, CompiledKindDataset, CompiledKindType};
use std::collections::HashSet;

/// Recursively collect all column references from an Expr tree.
pub fn collect_column_refs(expr: &Expr, columns: &mut Vec<String>, seen: &mut HashSet<String>) {
    match expr {
        Expr::Column(col) => {
            if seen.insert(col.name.clone()) {
                columns.push(col.name.clone());
            }
        }
        Expr::BinaryOp(bin) => {
            collect_column_refs(&bin.left, columns, seen);
            collect_column_refs(&bin.right, columns, seen);
        }
        Expr::Case(case) => {
            for wc in &case.when_then {
                collect_column_refs(&wc.condition, columns, seen);
                collect_column_refs(&wc.result, columns, seen);
            }
            if let Some(e) = &case.else_expr {
                collect_column_refs(e, columns, seen);
            }
        }
        Expr::FunctionCall(fc) => {
            for arg in &fc.args {
                collect_column_refs(arg, columns, seen);
            }
        }
        Expr::Aggregate(agg) => {
            collect_column_refs(&agg.expr, columns, seen);
        }
        Expr::Negate(u) | Expr::Not(u) | Expr::IsNull(u) | Expr::IsNotNull(u) => {
            collect_column_refs(&u.expr, columns, seen);
        }
        Expr::InList(il) => {
            collect_column_refs(&il.expr, columns, seen);
            for item in &il.list {
                collect_column_refs(item, columns, seen);
            }
        }
        Expr::Between(bt) => {
            collect_column_refs(&bt.expr, columns, seen);
            collect_column_refs(&bt.low, columns, seen);
            collect_column_refs(&bt.high, columns, seen);
        }
        Expr::Like(lk) => {
            collect_column_refs(&lk.expr, columns, seen);
            collect_column_refs(&lk.pattern, columns, seen);
        }
        Expr::Coalesce(co) => {
            for e in &co.exprs {
                collect_column_refs(e, columns, seen);
            }
        }
        Expr::NullIf(ni) => {
            collect_column_refs(&ni.expr, columns, seen);
            collect_column_refs(&ni.null_expr, columns, seen);
        }
        Expr::DateTrunc(dt) => {
            collect_column_refs(&dt.expr, columns, seen);
        }
        Expr::Guard(g) => {
            collect_column_refs(&g.condition, columns, seen);
            collect_column_refs(&g.expr, columns, seen);
        }
        Expr::Literal(_) | Expr::EntityRef(_) => {}
    }
}

/// Planner for Grainset kinds — route to cheapest covering dataset.
pub struct GrainsetPlanner;

impl KindPlanner for GrainsetPlanner {
    fn supports(&self, kind_type: &CompiledKindType) -> bool {
        matches!(kind_type, CompiledKindType::Grainset)
    }

    fn resolve(
        &self,
        kind: &CompiledKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        // Try single-dataset covering first.
        match find_covering_dataset(kind, request) {
            Ok(dataset_binding) => {
                super::shared::build_dataset_plan(kind, request, dataset_binding, ctx, true)
            }
            Err(PlannerError::NoCoveringDataset { .. }) => {
                // Fall back to horizontal join across multiple partial-coverage datasets.
                let covering_set = find_covering_datasets(kind, request)?;
                build_horizontal_join_plan(kind, request, &covering_set, ctx)
            }
            Err(e) => Err(e),
        }
    }
}

/// Find the first dataset whose column_mapping covers all requested
/// dimensions and measures.
fn find_covering_dataset<'a>(
    kind: &'a CompiledKind,
    request: &ResolvedQueryRequest,
) -> Result<&'a CompiledKindDataset, PlannerError> {
    let needed: Vec<&str> = request
        .dimensions
        .iter()
        .chain(request.measures.iter())
        .map(|s| s.as_str())
        .collect();

    // Score each dataset by how many needed fields it covers.
    let mut best: Option<(&CompiledKindDataset, usize)> = None;

    for ds in &kind.datasets {
        let covered = needed
            .iter()
            .filter(|name| ds.extras.column_mapping.contains_key(**name))
            .count();

        if covered == needed.len() {
            // Full coverage — pick this one (first wins as "cheapest" heuristic).
            return Ok(ds);
        }

        match best {
            None => best = Some((ds, covered)),
            Some((_, prev_covered)) if covered > prev_covered => {
                best = Some((ds, covered));
            }
            _ => {}
        }
    }

    Err(PlannerError::NoCoveringDataset {
        kind: kind.name.clone(),
        reason: format!(
            "no single dataset covers all requested fields: [{}]",
            needed.join(", ")
        ),
    })
}

/// A dataset binding paired with the measures it will provide in a horizontal join.
struct DatasetAssignment<'a> {
    dataset: &'a CompiledKindDataset,
    measures: Vec<String>,
}

/// Find a minimal set of datasets that together cover all requested dimensions and measures.
///
/// All datasets must cover every requested dimension (shared join keys).
/// Measures are partitioned greedily across datasets.
fn find_covering_datasets<'a>(
    kind: &'a CompiledKind,
    request: &ResolvedQueryRequest,
) -> Result<Vec<DatasetAssignment<'a>>, PlannerError> {
    let dim_names: Vec<&str> = request.dimensions.iter().map(|s| s.as_str()).collect();
    let measure_names: Vec<&str> = request.measures.iter().map(|s| s.as_str()).collect();

    // Filter datasets that cover ALL requested dimensions.
    let eligible: Vec<&CompiledKindDataset> = kind
        .datasets
        .iter()
        .filter(|ds| {
            dim_names
                .iter()
                .all(|d| ds.extras.column_mapping.contains_key(*d))
        })
        .collect();

    if eligible.is_empty() {
        return Err(PlannerError::NoCoveringDataset {
            kind: kind.name.clone(),
            reason: format!(
                "no dataset covers all requested dimensions: [{}]",
                dim_names.join(", ")
            ),
        });
    }

    // Greedy set-cover: assign each measure to the first eligible dataset that has it.
    let mut assignments: Vec<DatasetAssignment<'a>> = Vec::new();
    let mut uncovered: HashSet<&str> = measure_names.iter().copied().collect();

    for ds in &eligible {
        let covered_here: Vec<String> = uncovered
            .iter()
            .filter(|m| ds.extras.column_mapping.contains_key(**m))
            .map(|m| m.to_string())
            .collect();

        if !covered_here.is_empty() {
            for m in &covered_here {
                uncovered.remove(m.as_str());
            }
            assignments.push(DatasetAssignment {
                dataset: ds,
                measures: covered_here,
            });
        }

        if uncovered.is_empty() {
            break;
        }
    }

    if !uncovered.is_empty() {
        return Err(PlannerError::NoCoveringDataset {
            kind: kind.name.clone(),
            reason: format!(
                "no combination of datasets covers measures: [{}]",
                uncovered.into_iter().collect::<Vec<_>>().join(", ")
            ),
        });
    }

    Ok(assignments)
}

/// Build a horizontal join plan: per-dataset (Scan→Aggregate) joined on shared dimension columns.
///
/// Each dataset produces an aggregated sub-plan with dimensions + its assigned measures.
/// Sub-plans are FULL OUTER JOINed on the dimension columns.
/// A final Project maps all columns back to semantic names.
fn build_horizontal_join_plan(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    assignments: &[DatasetAssignment<'_>],
    ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    assert!(
        assignments.len() >= 2,
        "horizontal join requires at least 2 datasets"
    );

    // Build a sub-request + sub-plan per dataset assignment.
    let mut sub_plans: Vec<PlanNode> = Vec::new();
    // Track dimension aliases per sub-plan for join condition construction.
    // All sub-plans share the same semantic dimension names in their output schema.
    let dim_semantic_names: Vec<String> = request.dimensions.clone();

    for (idx, assignment) in assignments.iter().enumerate() {
        let sub_request = ResolvedQueryRequest {
            entity_name: request.entity_name.clone(),
            dimensions: request.dimensions.clone(),
            measures: assignment.measures.clone(),
            filters: vec![],
            grain: None,
            limit: None,
            order_by: vec![],
            domain_hint: None,
            session_variables: request.session_variables.clone(),
        };

        let fragment =
            super::shared::build_dataset_plan(kind, &sub_request, assignment.dataset, ctx, true)?;

        // If not the first sub-plan, alias dimension columns to avoid ambiguity.
        // We wrap in a Project that renames dims to __d{idx}_{name}.
        if idx == 0 {
            sub_plans.push(fragment.root);
        } else {
            // Wrap with a project that aliases dimension columns for the right side.
            let mut alias_exprs: Vec<Expr> = Vec::new();
            let mut alias_fields: Vec<Field> = Vec::new();

            for dim in &dim_semantic_names {
                let alias = format!("__d{}_{}", idx, dim);
                alias_exprs.push(Expr::column(dim.clone()));
                alias_fields.push(Field::new(alias, DataType::Utf8));
            }
            for measure_name in &assignment.measures {
                alias_exprs.push(Expr::column(measure_name.clone()));
                alias_fields.push(Field::new(measure_name.clone(), DataType::Float64));
            }
            let alias_schema = Schema::new(alias_fields);
            let alias_project = PlanNode::Project(ProjectNode {
                meta: NodeMeta::new(alias_schema),
                input: Box::new(fragment.root),
                expressions: alias_exprs,
            });
            sub_plans.push(alias_project);
        }
    }

    // Left-fold FULL OUTER JOIN across all sub-plans on dimension columns.
    let mut joined = sub_plans.remove(0);
    for (idx, right_plan) in sub_plans.into_iter().enumerate() {
        let right_idx = idx + 1; // 1-based because idx=0 was the left plan

        // Build join condition: left.dim1 = right.__d{idx}_dim1 AND left.dim2 = right.__d{idx}_dim2 ...
        let mut condition: Option<Expr> = None;
        for dim in &dim_semantic_names {
            let left_col = Expr::column(dim.clone());
            let right_col = Expr::column(format!("__d{}_{}", right_idx, dim));
            let eq = Expr::eq(left_col, right_col);
            condition = Some(match condition {
                Some(prev) => Expr::and(prev, eq),
                None => eq,
            });
        }

        // Build the join output schema: left fields + right measure fields only.
        // (Right dimension aliases are used for the join but dropped in final project.)
        let left_schema = joined.meta().output_schema.clone();
        let right_schema = right_plan.meta().output_schema.clone();

        let mut join_fields: Vec<Field> = left_schema.fields.clone();
        join_fields.extend(right_schema.fields.iter().cloned());
        let join_schema = Schema::new(join_fields);

        joined = PlanNode::Join(JoinNode {
            meta: NodeMeta::new(join_schema),
            left: Box::new(joined),
            right: Box::new(right_plan),
            join_type: JoinType::Full,
            condition: condition.expect("at least one dimension for join condition"),
        });
    }

    // Final Project: select semantic dimension names + all measure names.
    // For dimensions, use COALESCE(left.dim, right.__d1_dim, ...) to handle FULL OUTER JOIN nulls.
    let mut final_exprs: Vec<Expr> = Vec::new();
    let mut final_fields: Vec<Field> = Vec::new();

    for dim in &dim_semantic_names {
        let mut coalesce_args = vec![Expr::column(dim.clone())];
        for idx in 1..assignments.len() {
            coalesce_args.push(Expr::column(format!("__d{}_{}", idx, dim)));
        }
        if coalesce_args.len() == 1 {
            final_exprs.push(coalesce_args.remove(0));
        } else {
            final_exprs.push(Expr::coalesce(coalesce_args));
        }
        final_fields.push(Field::new(dim.clone(), DataType::Utf8));
    }

    for measure_name in &request.measures {
        final_exprs.push(Expr::column(measure_name.clone()));
        final_fields.push(Field::new(measure_name.clone(), DataType::Float64));
    }

    let final_schema = Schema::new(final_fields);
    let final_project = PlanNode::Project(ProjectNode {
        meta: NodeMeta::new(final_schema.clone()),
        input: Box::new(joined),
        expressions: final_exprs,
    });

    Ok(PlanFragment {
        root: final_project,
        output_schema: final_schema,
        pending_filters: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use semstrait_ir::Aggregation;

    /// Parse an aggregation function name from an expression source string.
    fn parse_aggregation(expr_source: &str) -> Aggregation {
        let upper = expr_source.trim().to_uppercase();
        if upper.starts_with("SUM") {
            Aggregation::Sum
        } else if upper.starts_with("AVG") {
            Aggregation::Avg
        } else if upper.starts_with("COUNT_DISTINCT") || upper.starts_with("COUNT(DISTINCT") {
            Aggregation::CountDistinct
        } else if upper.starts_with("COUNT") {
            Aggregation::Count
        } else if upper.starts_with("MIN") {
            Aggregation::Min
        } else if upper.starts_with("MAX") {
            Aggregation::Max
        } else {
            Aggregation::Sum
        }
    }

    #[test]
    fn test_parse_aggregation() {
        assert_eq!(parse_aggregation("SUM(amount)"), Aggregation::Sum);
        assert_eq!(parse_aggregation("avg(price)"), Aggregation::Avg);
        assert_eq!(parse_aggregation("COUNT(*)"), Aggregation::Count);
        assert_eq!(
            parse_aggregation("COUNT_DISTINCT(user_id)"),
            Aggregation::CountDistinct
        );
        assert_eq!(parse_aggregation("MIN(ts)"), Aggregation::Min);
        assert_eq!(parse_aggregation("MAX(ts)"), Aggregation::Max);
        assert_eq!(parse_aggregation("unknown_func"), Aggregation::Sum);
    }

    #[test]
    fn test_find_covering_dataset() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let result = find_covering_dataset(kind, &request);
        assert!(result.is_ok(), "should find a covering dataset");
        let dataset = result.unwrap();
        assert_eq!(dataset.name, "orders_daily");
    }

    #[test]
    fn test_no_covering_dataset() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        // Request a dimension that doesn't exist in the mapping.
        let request = make_test_request("orders", vec!["nonexistent_dim"], vec!["revenue"]);

        let result = find_covering_dataset(kind, &request);
        assert!(result.is_err(), "should fail when no dataset covers fields");
        assert!(matches!(result.unwrap_err(), PlannerError::NoCoveringDataset { .. }));
    }

    #[test]
    fn test_no_covering_dataset_falls_back_to_horizontal_join() {
        let manifest = make_multi_dataset_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        // Request both measures — no single dataset covers both.
        let request = make_test_request("orders", vec!["date", "region"], vec!["cost", "revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
        assert!(result.is_ok(), "horizontal join should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Output schema should have dimensions + both measures.
        let field_names: Vec<&str> = fragment.output_schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(field_names, vec!["date", "region", "cost", "revenue"]);

        // Root should be Project -> Join.
        match &fragment.root {
            PlanNode::Project(project) => {
                match project.input.as_ref() {
                    PlanNode::Join(join) => {
                        assert_eq!(join.join_type, JoinType::Full);
                    }
                    other => panic!("Expected Join under Project, got {:?}", std::mem::discriminant(other)),
                }
            }
            other => panic!("Expected Project as root, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn test_find_covering_datasets_greedy() {
        let manifest = make_multi_dataset_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["cost", "revenue"]);

        let result = find_covering_datasets(kind, &request);
        assert!(result.is_ok());
        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 2, "should use 2 datasets");

        // First dataset should cover "cost", second "revenue" (greedy order).
        assert_eq!(assignments[0].dataset.name, "cost_daily");
        assert_eq!(assignments[0].measures, vec!["cost"]);
        assert_eq!(assignments[1].dataset.name, "revenue_daily");
        assert_eq!(assignments[1].measures, vec!["revenue"]);
    }

    #[test]
    fn test_single_dataset_still_preferred() {
        // The standard manifest has one dataset covering everything — should NOT use horizontal join.
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
        assert!(result.is_ok());

        // Root should be Project -> Aggregate -> Scan (no Join).
        match &result.unwrap().root {
            PlanNode::Project(p) => {
                assert!(matches!(p.input.as_ref(), PlanNode::Aggregate(_)));
            }
            _ => panic!("Expected Project as root"),
        }
    }

    #[test]
    fn test_build_grainset_plan() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        let dataset = find_covering_dataset(kind, &request).unwrap();

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let result = super::super::shared::build_dataset_plan(kind, &request, dataset, &ctx, true);

        assert!(result.is_ok(), "build_dataset_plan should succeed");
        let fragment = result.unwrap();

        // Verify plan structure: should have Project -> Aggregate -> Scan
        match &fragment.root {
            PlanNode::Project(project_node) => {
                match project_node.input.as_ref() {
                    PlanNode::Aggregate(agg_node) => {
                        assert!(matches!(agg_node.input.as_ref(), PlanNode::Scan(_)));
                    }
                    _ => panic!("Expected Aggregate node under Project"),
                }
            }
            _ => panic!("Expected Project node as root"),
        }
    }
}
