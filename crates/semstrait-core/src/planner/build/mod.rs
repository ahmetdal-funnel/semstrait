//! Converts resolved kind plans into PlanNode trees.
//!
//! This is the bridge between kind resolution (grainset/unionset/joinset algorithms)
//! and the SQL/Substrait emitters. Each resolved kind plan is converted to a
//! PlanNode tree that the emitters consume.

mod common;
mod grainset;
mod joinset;
mod unionset;

use std::collections::HashMap;

use crate::diagnostics::{codes, CompileError, Diagnostic};
use crate::dsl;
use crate::schema::model::{Kind, MetricEntry};

use crate::planner::validate::metric_chain;

use crate::planner::ir::expr::{Column, Expr};
use crate::planner::ir::plan_node::*;
use crate::planner::resolve::ResolvedKind;

use common::find_kind_measure;
#[cfg(test)]
use common::validate_key_aggregations;

// =============================================================================
// Public entry point
// =============================================================================

/// Build a PlanNode tree from a resolved kind.
///
/// `metrics` are post-aggregation computed columns. Their expressions
/// reference measure names from the Aggregate output.
pub(crate) fn build_plan(
    kind: &Kind,
    resolved: &ResolvedKind,
    dimensions: &[String],
    measures: &[String],
    metrics: &[String],
) -> Result<PlanNode, CompileError> {
    // Collect implicit measures needed by metrics
    let (all_measures, metric_exprs) =
        resolve_metric_dependencies(kind, measures, metrics)?;

    let mut node = match resolved {
        ResolvedKind::Grainset(plan) => grainset::grainset_to_plan(kind, plan, dimensions, &all_measures),
        ResolvedKind::Unionset(plan) => unionset::unionset_to_plan(kind, plan, dimensions, &all_measures),
        ResolvedKind::Joinset(plan) => joinset::joinset_to_plan(kind, plan, dimensions, &all_measures),
    }?;

    // If there are metrics, wrap with a Project that adds metric columns
    if !metric_exprs.is_empty() {
        node = append_metric_project(node, dimensions, measures, &metric_exprs);
    }

    Ok(node)
}

// =============================================================================
// Metrics
// =============================================================================

/// Find a metric definition by name in the kind.
fn find_kind_metric<'a>(kind: &'a Kind, name: &str) -> Option<&'a crate::schema::model::Metric> {
    kind.metrics.as_ref()?.iter().find_map(|entry| match entry {
        MetricEntry::Inline(m) if m.name == name => Some(m),
        _ => None,
    })
}

/// Resolve metric dependencies: return the full list of measures needed
/// (including implicit ones from metrics) and the lowered metric expressions.
///
/// Returns `(all_measures, metric_name_expr_pairs)` where `all_measures` includes
/// both explicitly requested measures and implicitly required ones.
fn resolve_metric_dependencies(
    kind: &Kind,
    measures: &[String],
    metrics: &[String],
) -> Result<(Vec<String>, Vec<(String, Expr)>), CompileError> {
    if metrics.is_empty() {
        return Ok((measures.to_vec(), vec![]));
    }

    let mut all_measures: Vec<String> = measures.to_vec();
    let mut metric_exprs: Vec<(String, Expr)> = Vec::new();
    let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();

    for metric_name in metrics {
        let metric = find_kind_metric(kind, metric_name).ok_or_else(|| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!("kind '{}': metric '{}' not found", kind.name, metric_name),
            ))
        })?;

        // Parse and lower the metric expression
        let dsl_ast = dsl::parse_dsl(&metric.expr).map_err(|e| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!("metric '{}': DSL parse error: {}", metric_name, e),
            ))
        })?;
        let expr = dsl::lower_expr(&dsl_ast).map_err(|e| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!("metric '{}': lower error: {}", metric_name, e),
            ))
        })?;

        // Extract column references — these are the metric's dependencies
        let refs = collect_expr_column_refs(&expr);
        dependencies.insert(metric_name.clone(), refs.clone());

        // Add any referenced measures that aren't already in the list
        for dep in &refs {
            // If it's a measure (not another metric), add it to all_measures
            if find_kind_measure(kind, dep).is_some() && !all_measures.contains(dep) {
                all_measures.push(dep.clone());
            }
        }

        metric_exprs.push((metric_name.clone(), expr));
    }

    // Validate metric chain depth
    metric_chain::validate_metric_depth(&dependencies).map_err(|diags| {
        CompileError::from_diagnostics(diags)
    })?;

    Ok((all_measures, metric_exprs))
}

/// Collect all unqualified column names referenced in an expression.
fn collect_expr_column_refs(expr: &Expr) -> Vec<String> {
    let mut refs = Vec::new();
    collect_refs_inner(expr, &mut refs);
    refs
}

fn collect_refs_inner(expr: &Expr, refs: &mut Vec<String>) {
    match expr {
        Expr::Column(col) if col.table.is_empty() => {
            if !refs.contains(&col.name) {
                refs.push(col.name.clone());
            }
        }
        Expr::Add(l, r) | Expr::Subtract(l, r) | Expr::Multiply(l, r) | Expr::Divide(l, r) => {
            collect_refs_inner(l, refs);
            collect_refs_inner(r, refs);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_refs_inner(left, refs);
            collect_refs_inner(right, refs);
        }
        Expr::And(exprs) | Expr::Or(exprs) => {
            for e in exprs {
                collect_refs_inner(e, refs);
            }
        }
        Expr::Case { when_then, else_result } => {
            for (cond, result) in when_then {
                collect_refs_inner(cond, refs);
                collect_refs_inner(result, refs);
            }
            if let Some(e) = else_result {
                collect_refs_inner(e, refs);
            }
        }
        _ => {}
    }
}

/// Wrap the existing plan with a new Project that includes dimensions,
/// explicitly-requested measures, and metric expressions.
fn append_metric_project(
    input: PlanNode,
    dimensions: &[String],
    explicit_measures: &[String],
    metric_exprs: &[(String, Expr)],
) -> PlanNode {
    let mut expressions = Vec::new();

    // Pass through dimensions
    for dim in dimensions {
        expressions.push(ProjectExpr {
            expr: Expr::Column(Column::unqualified(dim)),
            alias: dim.clone(),
        });
    }

    // Pass through explicitly-requested measures (not implicit ones)
    for m in explicit_measures {
        expressions.push(ProjectExpr {
            expr: Expr::Column(Column::unqualified(m)),
            alias: m.clone(),
        });
    }

    // Add metric computed columns
    for (name, expr) in metric_exprs {
        expressions.push(ProjectExpr {
            expr: expr.clone(),
            alias: name.clone(),
        });
    }

    PlanNode::Project(Project {
        input: Box::new(input),
        expressions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::planner::resolve::{self as kind_resolver, QueryRequest};
    use crate::planner::emit::sql as sql_emitter;
    use crate::planner::ir::expr::AggregateExpr;

    fn load_kind(fixture: &str) -> Kind {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/"
        ))
        .join(format!("{}.yaml", fixture));
        let model = parser::parse_file(&path).unwrap();
        model.semantic_model.kinds.unwrap().into_iter().next().unwrap()
    }

    fn resolve_and_build(
        fixture: &str,
        dimensions: Vec<String>,
        measures: Vec<String>,
    ) -> PlanNode {
        resolve_and_build_with_metrics(fixture, dimensions, measures, vec![])
    }

    fn resolve_and_build_with_metrics(
        fixture: &str,
        dimensions: Vec<String>,
        measures: Vec<String>,
        metrics: Vec<String>,
    ) -> PlanNode {
        let kind = load_kind(fixture);
        let request = QueryRequest {
            dimensions: dimensions.clone(),
            measures: measures.clone(),
            metrics: metrics.clone(),
            domain: None,
            aggregation: None,
        };
        let resolved = kind_resolver::resolve_kind(&kind, &request).unwrap();
        build_plan(&kind, &resolved, &dimensions, &measures, &metrics).unwrap()
    }

    fn resolve_and_sql(
        fixture: &str,
        dimensions: Vec<String>,
        measures: Vec<String>,
    ) -> String {
        let plan = resolve_and_build(fixture, dimensions, measures);
        sql_emitter::emit_sql(&plan, None).unwrap()
    }

    fn resolve_and_sql_with_metrics(
        fixture: &str,
        dimensions: Vec<String>,
        measures: Vec<String>,
        metrics: Vec<String>,
    ) -> String {
        let plan = resolve_and_build_with_metrics(fixture, dimensions, measures, metrics);
        sql_emitter::emit_sql(&plan, None).unwrap()
    }

    // --- Grainset tests ---

    #[test]
    fn test_grainset_single_plan_structure() {
        let plan = resolve_and_build(
            "grainset_basic",
            vec!["order_date".into()],
            vec!["revenue".into()],
        );
        match &plan {
            PlanNode::Project(proj) => match proj.input.as_ref() {
                PlanNode::Aggregate(agg) => {
                    assert_eq!(agg.group_by.len(), 1);
                    assert_eq!(agg.aggregates.len(), 1);
                    assert_eq!(agg.aggregates[0].alias, "revenue");
                }
                other => panic!("expected Aggregate, got {:?}", other),
            },
            other => panic!("expected Project, got {:?}", other),
        }
    }

    #[test]
    fn test_grainset_emits_valid_sql() {
        let sql = resolve_and_sql(
            "grainset_basic",
            vec!["order_date".into()],
            vec!["revenue".into()],
        );
        assert!(sql.contains("warehouse.orders_monthly"), "SQL: {}", sql);
        assert!(sql.contains("GROUP BY"), "SQL: {}", sql);
        assert!(sql.contains("SUM("), "SQL: {}", sql);
        assert!(!sql.contains("TODO"), "SQL: {}", sql);
    }

    // --- Unionset tests ---

    #[test]
    fn test_unionset_emits_union_all() {
        let sql = resolve_and_sql(
            "unionset_basic",
            vec!["event_date".into()],
            vec!["event_count".into()],
        );
        assert!(sql.contains("UNION ALL"), "SQL: {}", sql);
        assert!(sql.contains("warehouse.click_events"), "SQL: {}", sql);
        assert!(sql.contains("warehouse.purchase_events"), "SQL: {}", sql);
    }

    #[test]
    fn test_unionset_null_fill_in_sql() {
        let sql = resolve_and_sql(
            "unionset_basic",
            vec!["event_date".into()],
            vec!["revenue".into()],
        );
        assert!(sql.contains("NULL"), "should NULL-fill missing column: {}", sql);
    }

    // --- Joinset tests ---

    #[test]
    fn test_joinset_emits_join() {
        let sql = resolve_and_sql(
            "joinset_basic",
            vec!["order_date".into(), "customer_name".into()],
            vec!["revenue".into()],
        );
        assert!(sql.contains("JOIN"), "SQL: {}", sql);
        assert!(sql.contains("warehouse.orders"), "SQL: {}", sql);
        assert!(sql.contains("warehouse.customers"), "SQL: {}", sql);
    }

    #[test]
    fn test_joinset_prune_no_join() {
        let sql = resolve_and_sql(
            "joinset_basic",
            vec!["order_date".into()],
            vec!["revenue".into()],
        );
        assert!(!sql.contains("JOIN"), "should not JOIN: {}", sql);
        assert!(sql.contains("warehouse.orders"), "SQL: {}", sql);
    }

    // --- Measure filter tests ---

    #[test]
    fn test_measure_filter_case_when() {
        let sql = resolve_and_sql(
            "grainset_measure_filter",
            vec!["order_date".into()],
            vec!["revenue".into()],
        );
        assert!(sql.contains("CASE"), "measure filter should produce CASE: {}", sql);
        assert!(sql.contains("WHEN"), "measure filter should produce WHEN: {}", sql);
        assert!(sql.contains("SUM("), "should still aggregate: {}", sql);
        assert!(sql.contains("cancelled"), "filter condition should reference 'cancelled': {}", sql);
    }

    #[test]
    fn test_measure_without_filter_no_case() {
        let sql = resolve_and_sql(
            "grainset_measure_filter",
            vec!["order_date".into()],
            vec!["order_count".into()],
        );
        assert!(!sql.contains("CASE"), "unfiltered measure should not have CASE: {}", sql);
    }

    // --- Key validation tests ---

    #[test]
    fn test_key_validation_rejects_sum_on_pk() {
        use crate::schema::model::Keys;
        use crate::schema::Aggregation;

        let keys = Keys {
            primary: Some(vec!["order_id".into()]),
            unique: None,
            foreign: None,
        };
        let aggregates = vec![AggregateExpr {
            func: Aggregation::Sum,
            expr: Expr::Column(Column::unqualified("order_id")),
            alias: "bad_sum".into(),
        }];
        let mut kind = load_kind("grainset_basic");
        kind.keys = Some(keys);

        let result = validate_key_aggregations(&kind, &aggregates, &["bad_sum".into()]);
        assert!(result.is_err(), "SUM on PK should fail");
        assert!(result.unwrap_err().to_string().contains("CONST_E006"));
    }

    #[test]
    fn test_key_validation_allows_count_on_pk() {
        use crate::schema::model::Keys;
        use crate::schema::Aggregation;

        let keys = Keys {
            primary: Some(vec!["order_id".into()]),
            unique: None,
            foreign: None,
        };
        let aggregates = vec![AggregateExpr {
            func: Aggregation::Count,
            expr: Expr::Column(Column::unqualified("order_id")),
            alias: "ok_count".into(),
        }];
        let mut kind = load_kind("grainset_basic");
        kind.keys = Some(keys);

        let result = validate_key_aggregations(&kind, &aggregates, &["ok_count".into()]);
        assert!(result.is_ok(), "COUNT on PK should be ok");
    }

    // --- Bucketed dimension tests ---

    #[test]
    fn test_bucketed_dimension_case_when() {
        let sql = resolve_and_sql(
            "grainset_bucketed",
            vec!["order_date".into(), "price_bucket".into()],
            vec!["order_count".into()],
        );
        assert!(sql.contains("CASE"), "bucketed dim should produce CASE: {}", sql);
        assert!(sql.contains("'low'"), "should have bucket name 'low': {}", sql);
        assert!(sql.contains("'medium'"), "should have bucket name 'medium': {}", sql);
        assert!(sql.contains("'high'"), "should have bucket name 'high': {}", sql);
        assert!(sql.contains("GROUP BY"), "should have GROUP BY: {}", sql);
    }

    // --- Semi-additive / additivity tests ---

    #[test]
    fn test_semi_additive_two_stage_sql() {
        let sql = resolve_and_sql(
            "grainset_semi_additive",
            vec!["account_id".into()],
            vec!["balance".into()],
        );
        let group_count = sql.matches("GROUP BY").count();
        assert_eq!(group_count, 2, "semi-additive should have 2 GROUP BY clauses: {}", sql);
        assert!(sql.contains("MAX("), "inner agg should use MAX for latest strategy: {}", sql);
        assert!(sql.contains("SUM("), "outer agg should use SUM (declared agg): {}", sql);
    }

    #[test]
    fn test_semi_additive_standard_when_dim_present() {
        let sql = resolve_and_sql(
            "grainset_semi_additive",
            vec!["account_id".into(), "account_date".into()],
            vec!["balance".into()],
        );
        let group_count = sql.matches("GROUP BY").count();
        assert_eq!(group_count, 1, "should have single GROUP BY when non-additive dim present: {}", sql);
        assert!(sql.contains("SUM("), "should use normal SUM: {}", sql);
    }

    #[test]
    fn test_semi_additive_mixed_measures() {
        let sql = resolve_and_sql(
            "grainset_semi_additive",
            vec!["account_id".into()],
            vec!["balance".into(), "transaction_count".into()],
        );
        let group_count = sql.matches("GROUP BY").count();
        assert_eq!(group_count, 2, "mixed measures should still get two-stage: {}", sql);
    }

    // --- Metric tests ---

    #[test]
    fn test_metric_in_project_sql() {
        let sql = resolve_and_sql_with_metrics(
            "grainset_metrics",
            vec!["order_date".into()],
            vec!["revenue".into()],
            vec!["avg_order_value".into()],
        );
        assert!(sql.contains("avg_order_value"), "metric alias should appear: {}", sql);
        assert!(sql.contains("/"), "metric expression should have division: {}", sql);
    }

    #[test]
    fn test_metric_implicit_measures() {
        let sql = resolve_and_sql_with_metrics(
            "grainset_metrics",
            vec!["order_date".into()],
            vec!["revenue".into()],
            vec!["avg_order_value".into()],
        );
        assert!(sql.contains("COUNT("), "implicit measure should be aggregated: {}", sql);
        assert!(sql.contains("SUM("), "explicit measure should be aggregated: {}", sql);
    }

    #[test]
    fn test_metric_no_metrics_unchanged() {
        let sql_without = resolve_and_sql(
            "grainset_metrics",
            vec!["order_date".into()],
            vec!["revenue".into()],
        );
        assert!(!sql_without.contains("avg_order_value"), "no metrics should mean no metric columns: {}", sql_without);
    }
}
