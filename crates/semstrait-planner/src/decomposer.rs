//! Measure and metric decomposition: extract aggregates from expressions.
//!
//! Decomposes semantic measure/metric expressions into:
//! - A list of [`AggregateMeasure`]s (for the AggNode)
//! - A post-aggregation expression (for the ProjectNode)
//!
//! Uses [`ExprResolver`] to resolve column names during decomposition.

use semstrait_core::expr::WhenClause;
use semstrait_core::Expr;
use semstrait_ir::{AggregateMeasure, Aggregation};
use semstrait_manifest::{CompiledFilter, CompiledMetric, CompiledInterface};

use crate::error::PlannerError;
use crate::resolver::ExprResolver;
use std::collections::HashMap;

/// Result of decomposing a measure expression: extracted aggregates plus a
/// post-aggregation projection expression.
pub struct DecomposedMeasure {
    /// The aggregates extracted from the expression (for the AggNode).
    pub aggregates: Vec<AggregateMeasure>,
    /// The post-aggregation expression (for the ProjectNode).
    /// For simple `SUM(col)`, this is just a column ref to the aggregate output.
    /// For `SUM(a) / COUNT(b)`, this computes the division over aggregate outputs.
    pub post_agg_expr: Expr,
}

/// Decompose a measure with a declarative aggregation tag.
///
/// The `agg` function is specified explicitly (from `CompiledMeasure.agg`),
/// and the `expr` is a horizontal-only expression (no aggregation functions).
pub fn decompose_measure(
    resolver: &dyn ExprResolver,
    measure_name: &str,
    agg: Aggregation,
    expr: &Expr,
    filters: &[CompiledFilter],
    measure_data_type: &semstrait_core::DataType,
) -> Result<DecomposedMeasure, PlannerError> {
    let lowered_expr = resolver.resolve_expr(expr)?;
    let agg_inner = wrap_with_filters(resolver, lowered_expr, filters)?;

    let distinct = matches!(agg, Aggregation::CountDistinct);
    let data_type =
        semstrait_manifest::function_registry::derive_aggregate_type(agg, measure_data_type);
    let aggregates = vec![AggregateMeasure {
        function: agg,
        expr: agg_inner,
        distinct,
        data_type,
    }];

    Ok(DecomposedMeasure {
        aggregates,
        post_agg_expr: Expr::column(measure_name),
    })
}

/// Decompose a measure using legacy expression-embedded aggregation.
///
/// Walks the expression tree to find Aggregate nodes, extracts them,
/// and builds a post-aggregation expression referencing aggregate outputs.
/// If no aggregates found, wraps in a default SUM.
#[cfg(test)]
pub fn decompose_measure_legacy(
    resolver: &dyn ExprResolver,
    measure_name: &str,
    expr: &Expr,
    filters: &[CompiledFilter],
) -> Result<DecomposedMeasure, PlannerError> {
    let mut aggregates: Vec<AggregateMeasure> = Vec::new();
    let post_agg_expr = extract_aggregates(expr, resolver, measure_name, &mut aggregates)?;

    if aggregates.is_empty() {
        let ir_expr = resolver.resolve_expr(expr)?;
        let wrapped = wrap_with_filters(resolver, ir_expr, filters)?;
        aggregates.push(AggregateMeasure {
            function: Aggregation::Sum,
            expr: wrapped,
            distinct: false,
            data_type: semstrait_core::DataType::Number,
        });
        Ok(DecomposedMeasure {
            aggregates,
            post_agg_expr: Expr::column(measure_name),
        })
    } else if !filters.is_empty() {
        let wrapped_aggregates = aggregates
            .into_iter()
            .map(|agg| {
                let wrapped_expr = wrap_with_filters(resolver, agg.expr, filters)?;
                Ok(AggregateMeasure {
                    function: agg.function,
                    expr: wrapped_expr,
                    distinct: agg.distinct,
                    data_type: agg.data_type,
                })
            })
            .collect::<Result<Vec<_>, PlannerError>>()?;
        Ok(DecomposedMeasure {
            aggregates: wrapped_aggregates,
            post_agg_expr,
        })
    } else {
        Ok(DecomposedMeasure {
            aggregates,
            post_agg_expr,
        })
    }
}

/// Decompose a metric into constituent aggregates via CompiledInterface.
///
/// The `resolver` controls how column names are resolved in constituent
/// measure expressions. Post-rename callers pass an identity resolver
/// (semantic domain); scan-collection callers pass a PhysicalResolver.
pub fn decompose_metric(
    metric_name: &str,
    metric: &CompiledMetric,
    iface: &CompiledInterface,
    resolver: &dyn ExprResolver,
    max_depth: usize,
) -> Result<DecomposedMeasure, PlannerError> {
    let mut aggregates: Vec<AggregateMeasure> = Vec::new();
    let mut agg_names: HashMap<String, String> = HashMap::new();

    let post_agg = decompose_metric_expr(
        &metric.expr,
        iface,
        resolver,
        &mut aggregates,
        &mut agg_names,
        metric_name,
        0,
        max_depth,
    )?;

    Ok(DecomposedMeasure {
        aggregates,
        post_agg_expr: post_agg,
    })
}

// ────────────────────────── private helpers ──────────────────────────

/// Wrap an expression with `CASE WHEN (filters) THEN expr ELSE NULL END`.
/// If no filters, returns the expression unchanged.
fn wrap_with_filters(
    resolver: &dyn ExprResolver,
    expr: Expr,
    filters: &[CompiledFilter],
) -> Result<Expr, PlannerError> {
    if filters.is_empty() {
        return Ok(expr);
    }

    let mut combined: Option<Expr> = None;
    for filter in filters {
        let resolved = resolver.resolve_expr(&filter.expr)?;
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

#[cfg(test)]
fn agg_info(expr: &Expr) -> Option<(Aggregation, &Expr, bool)> {
    match expr {
        Expr::Aggregate(agg) => Some((agg.function, &agg.expr, agg.distinct)),
        _ => None,
    }
}

/// Walk the expression tree, pull out aggregates, and return a
/// post-aggregation expression that references them by synthetic name.
#[cfg(test)]
fn extract_aggregates(
    expr: &Expr,
    resolver: &dyn ExprResolver,
    measure_name: &str,
    aggregates: &mut Vec<AggregateMeasure>,
) -> Result<Expr, PlannerError> {
    if let Some((agg_fn, inner, distinct)) = agg_info(expr) {
        let inner_resolved = resolver.resolve_expr(inner)?;

        let agg_alias = if aggregates.is_empty() {
            measure_name.to_string()
        } else {
            format!("__agg_{}", aggregates.len())
        };

        aggregates.push(AggregateMeasure {
            function: agg_fn,
            expr: inner_resolved,
            distinct,
            // Legacy path (test-only): type not available, default to Number.
            data_type: semstrait_core::DataType::Number,
        });

        return Ok(Expr::column(agg_alias));
    }

    match expr {
        Expr::BinaryOp(bin) => {
            let left = extract_aggregates(&bin.left, resolver, measure_name, aggregates)?;
            let right = extract_aggregates(&bin.right, resolver, measure_name, aggregates)?;
            Ok(Expr::binary(left, bin.op, right))
        }
        _ => resolver.resolve_expr(expr),
    }
}

#[allow(clippy::too_many_arguments)]
fn decompose_metric_expr(
    expr: &Expr,
    iface: &CompiledInterface,
    resolver: &dyn ExprResolver,
    aggregates: &mut Vec<AggregateMeasure>,
    agg_names: &mut HashMap<String, String>,
    metric_name: &str,
    depth: usize,
    max_depth: usize,
) -> Result<Expr, PlannerError> {
    match expr {
        Expr::Column(col) => resolve_metric_leaf(
            &col.name, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
        ),
        Expr::EntityRef(er) => resolve_metric_leaf(
            &er.name, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
        ),
        Expr::BinaryOp(bin) => {
            let left = decompose_metric_expr(
                &bin.left, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
            )?;
            let right = decompose_metric_expr(
                &bin.right, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
            )?;
            Ok(Expr::binary(left, bin.op, right))
        }
        Expr::Literal(_) => Ok(expr.clone()),
        Expr::Case(case) => {
            let mut whens = Vec::new();
            for wt in &case.when_then {
                let cond = decompose_metric_expr(
                    &wt.condition, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
                )?;
                let result = decompose_metric_expr(
                    &wt.result, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
                )?;
                whens.push(WhenClause::new(cond, result));
            }
            let else_expr = if let Some(ref e) = case.else_expr {
                Some(decompose_metric_expr(
                    e, iface, resolver, aggregates, agg_names, metric_name, depth, max_depth,
                )?)
            } else {
                None
            };
            Ok(Expr::case(whens, else_expr))
        }
        _ => Ok(expr.clone()),
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_metric_leaf(
    name: &str,
    iface: &CompiledInterface,
    resolver: &dyn ExprResolver,
    aggregates: &mut Vec<AggregateMeasure>,
    agg_names: &mut HashMap<String, String>,
    metric_name: &str,
    depth: usize,
    max_depth: usize,
) -> Result<Expr, PlannerError> {
    if let Some(alias) = agg_names.get(name) {
        return Ok(Expr::column(alias.clone()));
    }

    if let Some(measure) = iface.measures.get(name) {
        let decomposed = decompose_measure(resolver, name, measure.agg, &measure.expr, &measure.filters, &measure.data_type)?;
        let alias = name.to_string();
        for a in decomposed.aggregates {
            aggregates.push(a);
        }
        agg_names.insert(name.to_string(), alias.clone());
        return Ok(decomposed.post_agg_expr);
    }

    if let Some(sub_metric) = iface.metrics.get(name) {
        if depth >= max_depth {
            return Err(PlannerError::Internal(format!(
                "metric '{}' exceeds max decomposition depth {} while resolving '{}'",
                name, max_depth, metric_name
            )));
        }
        let post_agg = decompose_metric_expr(
            &sub_metric.expr, iface, resolver, aggregates, agg_names,
            metric_name, depth + 1, max_depth,
        )?;
        return Ok(post_agg);
    }

    Ok(resolver.resolve_column(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::MappingResolver;
    use semstrait_manifest::ColumnMappingValue;

    fn test_mapping() -> HashMap<String, ColumnMappingValue> {
        let mut m = HashMap::new();
        m.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));
        m.insert("cost".to_string(), ColumnMappingValue::Simple("cost_usd".to_string()));
        m.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
        m.insert("order_count".to_string(), ColumnMappingValue::Simple("order_id".to_string()));
        m
    }

    #[test]
    fn test_decompose_measure_simple_sum() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::entity_ref("revenue");
        let decomposed = decompose_measure(&resolver, "total_revenue", Aggregation::Sum, &expr, &[], &semstrait_core::DataType::Number).unwrap();

        assert_eq!(decomposed.aggregates.len(), 1);
        assert_eq!(decomposed.aggregates[0].function, Aggregation::Sum);
        assert_eq!(decomposed.aggregates[0].expr, Expr::column("amount"));
        assert!(!decomposed.aggregates[0].distinct);
        assert_eq!(decomposed.post_agg_expr, Expr::column("total_revenue"));
    }

    #[test]
    fn test_decompose_measure_count_distinct() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::entity_ref("order_count");
        let decomposed = decompose_measure(&resolver, "unique_orders", Aggregation::CountDistinct, &expr, &[], &semstrait_core::DataType::Integer).unwrap();

        assert_eq!(decomposed.aggregates.len(), 1);
        assert_eq!(decomposed.aggregates[0].function, Aggregation::CountDistinct);
        assert!(decomposed.aggregates[0].distinct);
        assert_eq!(decomposed.aggregates[0].expr, Expr::column("order_id"));
    }

    #[test]
    fn test_decompose_measure_with_horizontal_expr() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::add(Expr::entity_ref("revenue"), Expr::entity_ref("cost"));
        let decomposed = decompose_measure(&resolver, "total", Aggregation::Sum, &expr, &[], &semstrait_core::DataType::Number).unwrap();

        assert_eq!(decomposed.aggregates.len(), 1);
        assert_eq!(decomposed.aggregates[0].function, Aggregation::Sum);
        assert_eq!(
            decomposed.aggregates[0].expr,
            Expr::add(Expr::column("amount"), Expr::column("cost_usd"))
        );
    }

    #[test]
    fn test_decompose_measure_with_filters() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let filter = CompiledFilter {
            name: "us_only".to_string(),
            expr: Expr::eq(Expr::column("region"), Expr::string("US")),
            expr_source: "region = 'US'".to_string(),
        };

        let expr = Expr::entity_ref("revenue");
        let decomposed = decompose_measure(&resolver, "us_revenue", Aggregation::Sum, &expr, &[filter], &semstrait_core::DataType::Number).unwrap();

        assert_eq!(decomposed.aggregates.len(), 1);
        match &decomposed.aggregates[0].expr {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert_eq!(case.when_then[0].result, Expr::column("amount"));
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case expression, got {:?}", other),
        }
    }

    #[test]
    fn test_decompose_legacy_simple_sum() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::sum(Expr::column("revenue"));
        let decomposed = decompose_measure_legacy(&resolver, "total_revenue", &expr, &[]).unwrap();

        assert_eq!(decomposed.aggregates.len(), 1);
        assert_eq!(decomposed.aggregates[0].function, Aggregation::Sum);
        assert_eq!(decomposed.aggregates[0].expr, Expr::column("amount"));
        assert_eq!(decomposed.post_agg_expr, Expr::column("total_revenue"));
    }

    #[test]
    fn test_decompose_legacy_composed_divide() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::divide(
            Expr::sum(Expr::column("revenue")),
            Expr::count(Expr::column("order_count")),
        );
        let decomposed = decompose_measure_legacy(&resolver, "avg_order_value", &expr, &[]).unwrap();

        assert_eq!(decomposed.aggregates.len(), 2);
        assert_eq!(decomposed.aggregates[0].function, Aggregation::Sum);
        assert_eq!(decomposed.aggregates[0].expr, Expr::column("amount"));
        assert_eq!(decomposed.aggregates[1].function, Aggregation::Count);
        assert_eq!(decomposed.aggregates[1].expr, Expr::column("order_id"));

        match &decomposed.post_agg_expr {
            Expr::BinaryOp(bin) => {
                assert_eq!(bin.op, semstrait_core::BinaryOp::Divide);
                assert_eq!(*bin.left, Expr::column("avg_order_value"));
                assert_eq!(*bin.right, Expr::column("__agg_1"));
            }
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_decompose_legacy_with_filters() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let filter = CompiledFilter {
            name: "us_only".to_string(),
            expr: Expr::eq(Expr::column("region"), Expr::string("US")),
            expr_source: "region = 'US'".to_string(),
        };

        let expr = Expr::sum(Expr::column("revenue"));
        let decomposed = decompose_measure_legacy(&resolver, "us_revenue", &expr, &[filter]).unwrap();

        assert_eq!(decomposed.aggregates.len(), 1);
        assert_eq!(decomposed.aggregates[0].function, Aggregation::Sum);
        match &decomposed.aggregates[0].expr {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert_eq!(case.when_then[0].result, Expr::column("amount"));
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case expression, got {:?}", other),
        }
    }

    #[test]
    fn test_decompose_legacy_with_multiple_filters() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
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
        let decomposed = decompose_measure_legacy(&resolver, "filtered_rev", &expr, &filters).unwrap();

        match &decomposed.aggregates[0].expr {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                match &case.when_then[0].condition {
                    Expr::BinaryOp(bin) => assert_eq!(bin.op, semstrait_core::BinaryOp::And),
                    other => panic!("Expected AND condition, got {:?}", other),
                }
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case expression, got {:?}", other),
        }
    }

    #[test]
    fn test_decompose_legacy_no_filters_unchanged() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::sum(Expr::column("revenue"));

        let without = decompose_measure_legacy(&resolver, "rev", &expr, &[]).unwrap();
        let with_empty = decompose_measure_legacy(&resolver, "rev", &expr, &[]).unwrap();

        assert_eq!(without.aggregates[0].expr, with_empty.aggregates[0].expr);
    }
}
