//! Expression resolution: Expr → Expr with column mapping.
//!
//! Resolves entity refs and column names from semantic to physical using
//! the dataset's column_mapping. Also expands Guard sugar to Case.

use semstrait_core::expr::WhenClause;
use semstrait_core::Expr;
use semstrait_ir::{AggregateMeasure, Aggregation};
use semstrait_manifest::{ColumnMappingValue, CompiledFilter};

use crate::error::PlannerError;
use std::collections::HashMap;

/// Result of lowering a measure expression: extracted aggregates plus a
/// post-aggregation projection expression.
pub struct LoweredMeasure {
    /// The aggregates extracted from the expression (for the AggNode).
    pub aggregates: Vec<AggregateMeasure>,
    /// The post-aggregation expression (for the ProjectNode).
    /// For simple `SUM(col)`, this is just a column ref to the aggregate output.
    /// For `SUM(a) / COUNT(b)`, this computes the division over aggregate outputs.
    pub post_agg_expr: Expr,
}

/// Resolve an Expr, rewriting column names via mapping.
/// EntityRefs are resolved as column references through the mapping.
/// Guard is expanded to Case.
pub fn lower_expr(
    expr: &Expr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<Expr, PlannerError> {
    expr.transform(&|e| match e {
        Expr::Column(col) => {
            let physical = resolve_name(&col.name, column_mapping);
            Ok(Some(Expr::column(physical)))
        }
        Expr::EntityRef(entity) => {
            let physical = resolve_name(&entity.name, column_mapping);
            Ok(Some(Expr::column(physical)))
        }
        Expr::Guard(g) => Ok(Some(Expr::case(
            vec![WhenClause::new((*g.condition).clone(), (*g.expr).clone())],
            Some(Expr::null()),
        ))),
        _ => Ok(None),
    })
}

/// Extract aggregate information from a measure's Expr.
/// Returns the aggregates and a post-aggregation projection expression.
///
/// If `filters` is non-empty, each aggregate's inner expression is wrapped in
/// `CASE WHEN (filter1 AND filter2 ...) THEN expr ELSE NULL END` — standard
/// conditional aggregation for measure-scoped filters.
#[allow(dead_code)] // Convenience wrapper; tests use it, production uses lower_measure_with_filters
pub(crate) fn lower_measure(
    measure_name: &str,
    expr: &Expr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<LoweredMeasure, PlannerError> {
    lower_measure_with_filters(measure_name, expr, column_mapping, &[])
}

/// Like `lower_measure` but applies measure-level filters as conditional aggregation.
pub fn lower_measure_with_filters(
    measure_name: &str,
    expr: &Expr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
    filters: &[CompiledFilter],
) -> Result<LoweredMeasure, PlannerError> {
    let mut aggregates: Vec<AggregateMeasure> = Vec::new();
    let post_agg_expr = extract_aggregates(expr, column_mapping, measure_name, &mut aggregates)?;

    // If no aggregates were found (e.g. a plain column ref), wrap in a default SUM.
    if aggregates.is_empty() {
        let ir_expr = lower_expr(expr, column_mapping)?;
        let wrapped = wrap_with_filters(ir_expr, filters, column_mapping)?;
        aggregates.push(AggregateMeasure {
            function: Aggregation::Sum,
            expr: wrapped,
            distinct: false,
        });
        Ok(LoweredMeasure {
            aggregates,
            post_agg_expr: Expr::column(measure_name),
        })
    } else if !filters.is_empty() {
        // Wrap each aggregate's inner expression with the filter condition.
        let wrapped_aggregates = aggregates
            .into_iter()
            .map(|agg| {
                let wrapped_expr = wrap_with_filters(agg.expr, filters, column_mapping)?;
                Ok(AggregateMeasure {
                    function: agg.function,
                    expr: wrapped_expr,
                    distinct: agg.distinct,
                })
            })
            .collect::<Result<Vec<_>, PlannerError>>()?;
        Ok(LoweredMeasure {
            aggregates: wrapped_aggregates,
            post_agg_expr,
        })
    } else {
        Ok(LoweredMeasure {
            aggregates,
            post_agg_expr,
        })
    }
}

/// Wrap an expression with `CASE WHEN (filters) THEN expr ELSE NULL END`.
/// If no filters, returns the expression unchanged.
fn wrap_with_filters(
    expr: Expr,
    filters: &[CompiledFilter],
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<Expr, PlannerError> {
    if filters.is_empty() {
        return Ok(expr);
    }

    // Lower all filter expressions and AND them together.
    let mut combined: Option<Expr> = None;
    for filter in filters {
        let resolved = lower_expr(&filter.expr, column_mapping)?;
        combined = Some(match combined {
            None => resolved,
            Some(prev) => Expr::and(prev, resolved),
        });
    }

    let condition = combined.expect("filters is non-empty; early return guards this");
    Ok(Expr::case(
        vec![WhenClause::new(condition, expr)],
        Some(Expr::null()),
    ))
}

// ────────────────────────── private helpers ──────────────────────────

/// Resolve a semantic name to a physical name via column_mapping.
/// If not found, pass through unchanged.
fn resolve_name(name: &str, column_mapping: &HashMap<String, ColumnMappingValue>) -> String {
    match column_mapping.get(name) {
        Some(ColumnMappingValue::Simple(s)) => s.clone(),
        Some(ColumnMappingValue::WithGrain { column, .. }) => column.clone(),
        None => name.to_string(),
    }
}

fn agg_info(expr: &Expr) -> Option<(Aggregation, &Expr, bool)> {
    match expr {
        Expr::Aggregate(agg) => Some((agg.function, &agg.expr, agg.distinct)),
        _ => None,
    }
}

/// Walk the expression tree, pull out aggregates, and return a
/// post-aggregation expression that references them by synthetic name.
fn extract_aggregates(
    expr: &Expr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
    measure_name: &str,
    aggregates: &mut Vec<AggregateMeasure>,
) -> Result<Expr, PlannerError> {
    // If this node is an aggregate, extract it.
    if let Some((agg_fn, inner, distinct)) = agg_info(expr) {
        let inner_resolved = lower_expr(inner, column_mapping)?;

        let agg_alias = if aggregates.is_empty() {
            measure_name.to_string()
        } else {
            format!("__agg_{}", aggregates.len())
        };

        aggregates.push(AggregateMeasure {
            function: agg_fn,
            expr: inner_resolved,
            distinct,
        });

        return Ok(Expr::column(agg_alias));
    }

    // Non-aggregate binary ops: recurse into children looking for aggregates.
    match expr {
        Expr::BinaryOp(bin) => {
            let left = extract_aggregates(&bin.left, column_mapping, measure_name, aggregates)?;
            let right = extract_aggregates(&bin.right, column_mapping, measure_name, aggregates)?;
            Ok(Expr::binary(left, bin.op, right))
        }
        // For non-aggregate leaf/other nodes, just lower directly.
        _ => lower_expr(expr, column_mapping),
    }
}

// ────────────────────────────── tests ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mapping() -> HashMap<String, ColumnMappingValue> {
        let mut m = HashMap::new();
        m.insert(
            "revenue".to_string(),
            ColumnMappingValue::Simple("amount".to_string()),
        );
        m.insert(
            "cost".to_string(),
            ColumnMappingValue::Simple("cost_usd".to_string()),
        );
        m.insert(
            "region".to_string(),
            ColumnMappingValue::Simple("region_name".to_string()),
        );
        m.insert(
            "order_count".to_string(),
            ColumnMappingValue::Simple("order_id".to_string()),
        );
        m
    }

    // ── Test 1: simple column lowering ────────────────────────────────
    #[test]
    fn test_lower_column_with_mapping() {
        let mapping = test_mapping();
        let expr = Expr::column("revenue");
        let resolved = lower_expr(&expr, &mapping).unwrap();
        assert_eq!(resolved, Expr::column("amount"));
    }

    #[test]
    fn test_lower_column_passthrough() {
        let mapping = test_mapping();
        let expr = Expr::column("unknown_col");
        let resolved = lower_expr(&expr, &mapping).unwrap();
        assert_eq!(resolved, Expr::column("unknown_col"));
    }

    #[test]
    fn test_lower_entity_ref() {
        let mapping = test_mapping();
        let expr = Expr::entity_ref("region");
        let resolved = lower_expr(&expr, &mapping).unwrap();
        assert_eq!(resolved, Expr::column("region_name"));
    }

    #[test]
    fn test_lower_literals() {
        let mapping = test_mapping();

        assert_eq!(
            lower_expr(&Expr::int(42), &mapping).unwrap(),
            Expr::int(42)
        );
        assert_eq!(
            lower_expr(&Expr::float(3.14), &mapping).unwrap(),
            Expr::float(3.14)
        );
        assert_eq!(
            lower_expr(&Expr::string("hello"), &mapping).unwrap(),
            Expr::string("hello")
        );
        assert_eq!(
            lower_expr(&Expr::boolean(true), &mapping).unwrap(),
            Expr::boolean(true)
        );
        assert_eq!(
            lower_expr(&Expr::null(), &mapping).unwrap(),
            Expr::null()
        );
    }

    // ── Test 2: simple aggregate lowering ─────────────────────────────
    #[test]
    fn test_lower_measure_simple_sum() {
        let mapping = test_mapping();
        let expr = Expr::sum(Expr::column("revenue"));
        let lowered = lower_measure("total_revenue", &expr, &mapping).unwrap();

        assert_eq!(lowered.aggregates.len(), 1);
        assert_eq!(lowered.aggregates[0].function, Aggregation::Sum);
        assert_eq!(lowered.aggregates[0].expr, Expr::column("amount"));
        assert!(!lowered.aggregates[0].distinct);

        // post_agg_expr is a column ref to the measure name
        assert_eq!(lowered.post_agg_expr, Expr::column("total_revenue"));
    }

    #[test]
    fn test_lower_measure_count_distinct() {
        let mapping = test_mapping();
        let expr = Expr::count_distinct(Expr::column("order_count"));
        let lowered = lower_measure("unique_orders", &expr, &mapping).unwrap();

        assert_eq!(lowered.aggregates.len(), 1);
        assert_eq!(lowered.aggregates[0].function, Aggregation::CountDistinct);
        assert!(lowered.aggregates[0].distinct);
        assert_eq!(lowered.aggregates[0].expr, Expr::column("order_id"));
    }

    // ── Test 3: composed measure lowering ─────────────────────────────
    #[test]
    fn test_lower_measure_composed_divide() {
        let mapping = test_mapping();
        // SUM(revenue) / COUNT(order_count) — a composed measure
        let expr = Expr::divide(
            Expr::sum(Expr::column("revenue")),
            Expr::count(Expr::column("order_count")),
        );
        let lowered = lower_measure("avg_order_value", &expr, &mapping).unwrap();

        assert_eq!(lowered.aggregates.len(), 2);

        // First aggregate: SUM(amount)
        assert_eq!(lowered.aggregates[0].function, Aggregation::Sum);
        assert_eq!(lowered.aggregates[0].expr, Expr::column("amount"));

        // Second aggregate: COUNT(order_id)
        assert_eq!(lowered.aggregates[1].function, Aggregation::Count);
        assert_eq!(lowered.aggregates[1].expr, Expr::column("order_id"));

        // post_agg_expr should be a division of two column refs
        match &lowered.post_agg_expr {
            Expr::BinaryOp(bin) => {
                assert_eq!(bin.op, semstrait_core::BinaryOp::Divide);
                assert_eq!(*bin.left, Expr::column("avg_order_value"));
                assert_eq!(*bin.right, Expr::column("__agg_1"));
            }
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_binary_arithmetic() {
        let mapping = test_mapping();
        let expr = Expr::add(Expr::column("revenue"), Expr::int(10));
        let resolved = lower_expr(&expr, &mapping).unwrap();
        assert_eq!(
            resolved,
            Expr::add(Expr::column("amount"), Expr::int(10))
        );
    }

    #[test]
    fn test_lower_case_expr() {
        let mapping = test_mapping();
        let expr = Expr::case(
            vec![WhenClause::new(
                Expr::eq(Expr::column("region"), Expr::string("US")),
                Expr::int(1),
            )],
            Some(Expr::int(0)),
        );
        let resolved = lower_expr(&expr, &mapping).unwrap();
        match resolved {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert!(case.else_expr.is_some());
            }
            other => panic!("Expected Case, got {:?}", other),
        }
    }

    // ── Test: measure filters as conditional aggregation ────────────
    #[test]
    fn test_lower_measure_with_filters() {
        let mapping = test_mapping();

        // Filter: region = 'US'
        let filter_expr = Expr::eq(Expr::column("region"), Expr::string("US"));
        let filters = vec![CompiledFilter {
            name: "us_only".to_string(),
            expr: filter_expr,
            expr_source: "region = 'US'".to_string(),
        }];

        let expr = Expr::sum(Expr::column("revenue"));
        let lowered =
            lower_measure_with_filters("us_revenue", &expr, &mapping, &filters).unwrap();

        assert_eq!(lowered.aggregates.len(), 1);
        assert_eq!(lowered.aggregates[0].function, Aggregation::Sum);

        // The aggregate's inner expression should be:
        // CASE WHEN region_name = 'US' THEN amount ELSE NULL END
        match &lowered.aggregates[0].expr {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert_eq!(case.when_then[0].result, Expr::column("amount"));
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case expression, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_measure_with_multiple_filters() {
        let mapping = test_mapping();

        let filters = vec![
            CompiledFilter {
                name: "us_only".to_string(),
                expr: Expr::eq(Expr::column("region"), Expr::string("US")),
                expr_source: "region = 'US'".to_string(),
                },
            CompiledFilter {
                name: "high_value".to_string(),
                expr: Expr::gt(Expr::column("revenue"), Expr::int(100)),
                expr_source: "revenue > 100".to_string(),
                },
        ];

        let expr = Expr::sum(Expr::column("revenue"));
        let lowered =
            lower_measure_with_filters("filtered_rev", &expr, &mapping, &filters).unwrap();

        // The aggregate inner should be CASE WHEN (f1 AND f2) THEN amount ELSE NULL END
        match &lowered.aggregates[0].expr {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                match &case.when_then[0].condition {
                    Expr::BinaryOp(bin) => {
                        assert_eq!(bin.op, semstrait_core::BinaryOp::And);
                    }
                    other => panic!("Expected AND condition, got {:?}", other),
                }
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case expression, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_measure_no_filters_unchanged() {
        let mapping = test_mapping();
        let expr = Expr::sum(Expr::column("revenue"));

        let without = lower_measure("rev", &expr, &mapping).unwrap();
        let with_empty = lower_measure_with_filters("rev", &expr, &mapping, &[]).unwrap();

        assert_eq!(without.aggregates[0].expr, with_empty.aggregates[0].expr);
    }

    #[test]
    fn test_lower_guard_becomes_case() {
        let mapping = test_mapping();
        let expr = Expr::guard(
            Expr::eq(Expr::column("region"), Expr::string("US")),
            Expr::column("revenue"),
        );
        let resolved = lower_expr(&expr, &mapping).unwrap();
        match resolved {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case from Guard, got {:?}", other),
        }
    }
}
