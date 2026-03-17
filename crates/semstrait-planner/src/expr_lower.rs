//! Expression lowering: core::DslExpr -> IR DslExpr with column mapping.
//!
//! Rewrites entity refs and column names from semantic to physical using
//! the dataset's column_mapping.

use semstrait_core::DslExpr as CoreExpr;
use semstrait_core::dsl_expr::{AggExpr, BinaryExpr, LiteralExpr, LogicalExpr};
use semstrait_ir::{AggregateMeasure, Aggregation, BinaryOp, DslExpr as IrExpr};
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
    pub post_agg_expr: IrExpr,
}

/// Lower a core DslExpr to an IR DslExpr, rewriting column names via mapping.
/// EntityRefs are resolved as column references through the mapping.
pub fn lower_expr(
    expr: &CoreExpr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<IrExpr, PlannerError> {
    match expr {
        // ── Leaf nodes ────────────────────────────────────────────────
        CoreExpr::Column(col) => {
            let physical = resolve_name(&col.name, column_mapping);
            Ok(IrExpr::Column {
                name: physical,
                qualifier: None,
            })
        }
        CoreExpr::EntityRef(entity) => {
            let physical = resolve_name(&entity.name, column_mapping);
            Ok(IrExpr::Column {
                name: physical,
                qualifier: None,
            })
        }
        CoreExpr::Literal(lit) => lower_literal(lit),

        // ── Aggregation (lowered as-is when inside a non-measure context) ─
        CoreExpr::Sum(agg) => lower_agg_as_function("SUM", agg, column_mapping),
        CoreExpr::Count(agg) => lower_agg_as_function("COUNT", agg, column_mapping),
        CoreExpr::CountDistinct(agg) => {
            let inner = lower_expr(&agg.expr, column_mapping)?;
            Ok(IrExpr::FunctionCall {
                name: "COUNT".to_string(),
                args: vec![inner],
                distinct: true,
            })
        }
        CoreExpr::Avg(agg) => lower_agg_as_function("AVG", agg, column_mapping),
        CoreExpr::Min(agg) => lower_agg_as_function("MIN", agg, column_mapping),
        CoreExpr::Max(agg) => lower_agg_as_function("MAX", agg, column_mapping),

        // ── Arithmetic ────────────────────────────────────────────────
        CoreExpr::Add(bin) => lower_binary(bin, BinaryOp::Add, column_mapping),
        CoreExpr::Subtract(bin) => lower_binary(bin, BinaryOp::Subtract, column_mapping),
        CoreExpr::Multiply(bin) => lower_binary(bin, BinaryOp::Multiply, column_mapping),
        CoreExpr::Divide(bin) => lower_binary(bin, BinaryOp::Divide, column_mapping),
        CoreExpr::SafeDivide(bin) => lower_binary(bin, BinaryOp::SafeDivide, column_mapping),

        // ── Unary ─────────────────────────────────────────────────────
        CoreExpr::Negate(u) => Ok(IrExpr::Negate(Box::new(lower_expr(&u.expr, column_mapping)?))),
        CoreExpr::Not(u) => Ok(IrExpr::Not(Box::new(lower_expr(&u.expr, column_mapping)?))),
        CoreExpr::IsNull(u) => Ok(IrExpr::IsNull(Box::new(lower_expr(&u.expr, column_mapping)?))),
        CoreExpr::IsNotNull(u) => {
            Ok(IrExpr::IsNotNull(Box::new(
                lower_expr(&u.expr, column_mapping)?,
            )))
        }

        // ── Comparison ────────────────────────────────────────────────
        CoreExpr::Eq(bin) => lower_binary(bin, BinaryOp::Eq, column_mapping),
        CoreExpr::Ne(bin) => lower_binary(bin, BinaryOp::NotEq, column_mapping),
        CoreExpr::Gt(bin) => lower_binary(bin, BinaryOp::Gt, column_mapping),
        CoreExpr::Gte(bin) => lower_binary(bin, BinaryOp::GtEq, column_mapping),
        CoreExpr::Lt(bin) => lower_binary(bin, BinaryOp::Lt, column_mapping),
        CoreExpr::Lte(bin) => lower_binary(bin, BinaryOp::LtEq, column_mapping),

        // ── Logical ───────────────────────────────────────────────────
        CoreExpr::And(log) => lower_logical(log, BinaryOp::And, column_mapping),
        CoreExpr::Or(log) => lower_logical(log, BinaryOp::Or, column_mapping),

        // ── List / range ──────────────────────────────────────────────
        CoreExpr::InList(il) => {
            let ir_expr = lower_expr(&il.expr, column_mapping)?;
            let ir_list: Vec<IrExpr> = il
                .list
                .iter()
                .map(|e| lower_expr(e, column_mapping))
                .collect::<Result<_, _>>()?;
            Ok(IrExpr::InList {
                expr: Box::new(ir_expr),
                list: ir_list,
                negated: false,
            })
        }
        CoreExpr::Between(bt) => {
            let ir_expr = lower_expr(&bt.expr, column_mapping)?;
            let ir_low = lower_expr(&bt.lower, column_mapping)?;
            let ir_high = lower_expr(&bt.upper, column_mapping)?;
            Ok(IrExpr::Between {
                expr: Box::new(ir_expr),
                low: Box::new(ir_low),
                high: Box::new(ir_high),
                negated: false,
            })
        }
        CoreExpr::Like(bin) => {
            let ir_left = lower_expr(&bin.left, column_mapping)?;
            let ir_right = lower_expr(&bin.right, column_mapping)?;
            Ok(IrExpr::Like {
                expr: Box::new(ir_left),
                pattern: Box::new(ir_right),
            })
        }

        // ── Conditional ───────────────────────────────────────────────
        CoreExpr::Case(c) => {
            let when_then: Vec<(IrExpr, IrExpr)> = c
                .when
                .iter()
                .map(|wc| {
                    let cond = lower_expr(&wc.condition, column_mapping)?;
                    let res = lower_expr(&wc.result, column_mapping)?;
                    Ok((cond, res))
                })
                .collect::<Result<_, PlannerError>>()?;
            let else_expr = c
                .else_expr
                .as_ref()
                .map(|e| lower_expr(e, column_mapping))
                .transpose()?
                .map(Box::new);
            Ok(IrExpr::Case {
                when_then,
                else_expr,
            })
        }
        CoreExpr::Coalesce(co) => {
            let exprs: Vec<IrExpr> = co
                .exprs
                .iter()
                .map(|e| lower_expr(e, column_mapping))
                .collect::<Result<_, _>>()?;
            Ok(IrExpr::Coalesce(exprs))
        }
        CoreExpr::NullIf(bin) => {
            let ir_left = lower_expr(&bin.left, column_mapping)?;
            let ir_right = lower_expr(&bin.right, column_mapping)?;
            Ok(IrExpr::NullIf {
                expr: Box::new(ir_left),
                null_expr: Box::new(ir_right),
            })
        }

        // ── Date/time ─────────────────────────────────────────────────
        CoreExpr::DateTrunc(dt) => {
            let ir_expr = lower_expr(&dt.expr, column_mapping)?;
            Ok(IrExpr::DateTrunc {
                grain: dt.grain.to_string(),
                expr: Box::new(ir_expr),
            })
        }

        // ── Guard ─────────────────────────────────────────────────────
        CoreExpr::Guard(g) => {
            let cond = lower_expr(&g.condition, column_mapping)?;
            let body = lower_expr(&g.expr, column_mapping)?;
            Ok(IrExpr::Case {
                when_then: vec![(cond, body)],
                else_expr: Some(Box::new(IrExpr::Null)),
            })
        }
    }
}

/// Extract aggregate information from a measure's core DslExpr.
/// Returns the aggregates and a post-aggregation projection expression.
///
/// If `filters` is non-empty, each aggregate's inner expression is wrapped in
/// `CASE WHEN (filter1 AND filter2 ...) THEN expr ELSE NULL END` — standard
/// conditional aggregation for measure-scoped filters.
pub fn lower_measure(
    measure_name: &str,
    expr: &CoreExpr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<LoweredMeasure, PlannerError> {
    lower_measure_with_filters(measure_name, expr, column_mapping, &[])
}

/// Like `lower_measure` but applies measure-level filters as conditional aggregation.
pub fn lower_measure_with_filters(
    measure_name: &str,
    expr: &CoreExpr,
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
            post_agg_expr: IrExpr::Column {
                name: measure_name.to_string(),
                qualifier: None,
            },
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
    expr: IrExpr,
    filters: &[CompiledFilter],
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<IrExpr, PlannerError> {
    if filters.is_empty() {
        return Ok(expr);
    }

    // Lower all filter expressions and AND them together.
    let mut combined: Option<IrExpr> = None;
    for filter in filters {
        let ir_filter = lower_expr(&filter.expr, column_mapping)?;
        combined = Some(match combined {
            None => ir_filter,
            Some(prev) => IrExpr::BinaryOp {
                left: Box::new(prev),
                op: BinaryOp::And,
                right: Box::new(ir_filter),
            },
        });
    }

    let condition = combined.unwrap(); // safe: filters is non-empty
    Ok(IrExpr::Case {
        when_then: vec![(condition, expr)],
        else_expr: Some(Box::new(IrExpr::Null)),
    })
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

fn lower_literal(lit: &LiteralExpr) -> Result<IrExpr, PlannerError> {
    match lit {
        LiteralExpr::Integer { value } => Ok(IrExpr::Number(*value as f64)),
        LiteralExpr::Float { value } => Ok(IrExpr::Number(*value)),
        LiteralExpr::String { value } => Ok(IrExpr::StringLit(value.clone())),
        LiteralExpr::Boolean { value } => Ok(IrExpr::Bool(*value)),
        LiteralExpr::Null => Ok(IrExpr::Null),
    }
}

fn lower_binary(
    bin: &BinaryExpr,
    op: BinaryOp,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<IrExpr, PlannerError> {
    let left = lower_expr(&bin.left, column_mapping)?;
    let right = lower_expr(&bin.right, column_mapping)?;
    Ok(IrExpr::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

fn lower_logical(
    log: &LogicalExpr,
    op: BinaryOp,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<IrExpr, PlannerError> {
    if log.exprs.is_empty() {
        return Err(PlannerError::Internal(
            "empty logical expression".to_string(),
        ));
    }
    let mut result = lower_expr(&log.exprs[0], column_mapping)?;
    for e in &log.exprs[1..] {
        let right = lower_expr(e, column_mapping)?;
        result = IrExpr::BinaryOp {
            left: Box::new(result),
            op,
            right: Box::new(right),
        };
    }
    Ok(result)
}

fn lower_agg_as_function(
    name: &str,
    agg: &AggExpr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
) -> Result<IrExpr, PlannerError> {
    let inner = lower_expr(&agg.expr, column_mapping)?;
    Ok(IrExpr::FunctionCall {
        name: name.to_string(),
        args: vec![inner],
        distinct: false,
    })
}

fn core_agg_to_ir(variant: &CoreExpr) -> Option<(Aggregation, bool)> {
    match variant {
        CoreExpr::Sum(_) => Some((Aggregation::Sum, false)),
        CoreExpr::Count(_) => Some((Aggregation::Count, false)),
        CoreExpr::CountDistinct(_) => Some((Aggregation::CountDistinct, true)),
        CoreExpr::Avg(_) => Some((Aggregation::Avg, false)),
        CoreExpr::Min(_) => Some((Aggregation::Min, false)),
        CoreExpr::Max(_) => Some((Aggregation::Max, false)),
        _ => None,
    }
}

fn agg_inner(expr: &CoreExpr) -> Option<&CoreExpr> {
    match expr {
        CoreExpr::Sum(a)
        | CoreExpr::Count(a)
        | CoreExpr::CountDistinct(a)
        | CoreExpr::Avg(a)
        | CoreExpr::Min(a)
        | CoreExpr::Max(a) => Some(&a.expr),
        _ => None,
    }
}

/// Walk the expression tree, pull out aggregates, and return a
/// post-aggregation expression that references them by synthetic name.
fn extract_aggregates(
    expr: &CoreExpr,
    column_mapping: &HashMap<String, ColumnMappingValue>,
    measure_name: &str,
    aggregates: &mut Vec<AggregateMeasure>,
) -> Result<IrExpr, PlannerError> {
    // If this node is an aggregate, extract it.
    if let Some((agg_fn, distinct)) = core_agg_to_ir(expr) {
        let inner_core = agg_inner(expr).unwrap();
        let inner_ir = lower_expr(inner_core, column_mapping)?;

        // If this is the only aggregate (checked by caller), use the measure name directly.
        // Otherwise use a synthetic name.
        let agg_alias = if aggregates.is_empty() {
            measure_name.to_string()
        } else {
            format!("__agg_{}", aggregates.len())
        };

        aggregates.push(AggregateMeasure {
            function: agg_fn,
            expr: inner_ir,
            distinct,
        });

        return Ok(IrExpr::Column {
            name: agg_alias,
            qualifier: None,
        });
    }

    // Non-aggregate node: recurse into children and rebuild as IR.
    match expr {
        CoreExpr::Add(bin) => rebuild_binary(bin, BinaryOp::Add, column_mapping, measure_name, aggregates),
        CoreExpr::Subtract(bin) => rebuild_binary(bin, BinaryOp::Subtract, column_mapping, measure_name, aggregates),
        CoreExpr::Multiply(bin) => rebuild_binary(bin, BinaryOp::Multiply, column_mapping, measure_name, aggregates),
        CoreExpr::Divide(bin) => rebuild_binary(bin, BinaryOp::Divide, column_mapping, measure_name, aggregates),
        CoreExpr::SafeDivide(bin) => rebuild_binary(bin, BinaryOp::SafeDivide, column_mapping, measure_name, aggregates),

        // For non-aggregate leaf/other nodes, just lower directly.
        _ => lower_expr(expr, column_mapping),
    }
}

fn rebuild_binary(
    bin: &BinaryExpr,
    op: BinaryOp,
    column_mapping: &HashMap<String, ColumnMappingValue>,
    measure_name: &str,
    aggregates: &mut Vec<AggregateMeasure>,
) -> Result<IrExpr, PlannerError> {
    let left = extract_aggregates(&bin.left, column_mapping, measure_name, aggregates)?;
    let right = extract_aggregates(&bin.right, column_mapping, measure_name, aggregates)?;
    Ok(IrExpr::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

// ────────────────────────────── tests ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::DslExpr as CoreExpr;

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
        let core = CoreExpr::column("revenue");
        let ir = lower_expr(&core, &mapping).unwrap();
        assert_eq!(
            ir,
            IrExpr::Column {
                name: "amount".to_string(),
                qualifier: None,
            }
        );
    }

    #[test]
    fn test_lower_column_passthrough() {
        let mapping = test_mapping();
        let core = CoreExpr::column("unknown_col");
        let ir = lower_expr(&core, &mapping).unwrap();
        assert_eq!(
            ir,
            IrExpr::Column {
                name: "unknown_col".to_string(),
                qualifier: None,
            }
        );
    }

    #[test]
    fn test_lower_entity_ref() {
        let mapping = test_mapping();
        let core = CoreExpr::entity_ref("region");
        let ir = lower_expr(&core, &mapping).unwrap();
        assert_eq!(
            ir,
            IrExpr::Column {
                name: "region_name".to_string(),
                qualifier: None,
            }
        );
    }

    #[test]
    fn test_lower_literals() {
        let mapping = test_mapping();

        assert_eq!(
            lower_expr(&CoreExpr::int(42), &mapping).unwrap(),
            IrExpr::Number(42.0)
        );
        assert_eq!(
            lower_expr(&CoreExpr::float(3.14), &mapping).unwrap(),
            IrExpr::Number(3.14)
        );
        assert_eq!(
            lower_expr(&CoreExpr::string("hello"), &mapping).unwrap(),
            IrExpr::StringLit("hello".to_string())
        );
        assert_eq!(
            lower_expr(&CoreExpr::bool(true), &mapping).unwrap(),
            IrExpr::Bool(true)
        );
        assert_eq!(
            lower_expr(&CoreExpr::null(), &mapping).unwrap(),
            IrExpr::Null
        );
    }

    // ── Test 2: simple aggregate lowering ─────────────────────────────
    #[test]
    fn test_lower_measure_simple_sum() {
        let mapping = test_mapping();
        let core = CoreExpr::sum(CoreExpr::column("revenue"));
        let lowered = lower_measure("total_revenue", &core, &mapping).unwrap();

        assert_eq!(lowered.aggregates.len(), 1);
        assert_eq!(lowered.aggregates[0].function, Aggregation::Sum);
        assert_eq!(
            lowered.aggregates[0].expr,
            IrExpr::Column {
                name: "amount".to_string(),
                qualifier: None,
            }
        );
        assert!(!lowered.aggregates[0].distinct);

        // post_agg_expr is a column ref to the measure name
        assert_eq!(
            lowered.post_agg_expr,
            IrExpr::Column {
                name: "total_revenue".to_string(),
                qualifier: None,
            }
        );
    }

    #[test]
    fn test_lower_measure_count_distinct() {
        let mapping = test_mapping();
        let core = CoreExpr::count_distinct(CoreExpr::column("order_count"));
        let lowered = lower_measure("unique_orders", &core, &mapping).unwrap();

        assert_eq!(lowered.aggregates.len(), 1);
        assert_eq!(lowered.aggregates[0].function, Aggregation::CountDistinct);
        assert!(lowered.aggregates[0].distinct);
        assert_eq!(
            lowered.aggregates[0].expr,
            IrExpr::Column {
                name: "order_id".to_string(),
                qualifier: None,
            }
        );
    }

    // ── Test 3: composed measure lowering ─────────────────────────────
    #[test]
    fn test_lower_measure_composed_divide() {
        let mapping = test_mapping();
        // SUM(revenue) / COUNT(order_count) — a composed measure
        let core = CoreExpr::divide(
            CoreExpr::sum(CoreExpr::column("revenue")),
            CoreExpr::count(CoreExpr::column("order_count")),
        );
        let lowered = lower_measure("avg_order_value", &core, &mapping).unwrap();

        assert_eq!(lowered.aggregates.len(), 2);

        // First aggregate: SUM(amount)
        assert_eq!(lowered.aggregates[0].function, Aggregation::Sum);
        assert_eq!(
            lowered.aggregates[0].expr,
            IrExpr::Column {
                name: "amount".to_string(),
                qualifier: None,
            }
        );

        // Second aggregate: COUNT(order_id)
        assert_eq!(lowered.aggregates[1].function, Aggregation::Count);
        assert_eq!(
            lowered.aggregates[1].expr,
            IrExpr::Column {
                name: "order_id".to_string(),
                qualifier: None,
            }
        );

        // post_agg_expr should be a division of two column refs
        match &lowered.post_agg_expr {
            IrExpr::BinaryOp { left, op, right } => {
                assert_eq!(*op, BinaryOp::Divide);
                // first agg gets the measure name
                assert_eq!(
                    **left,
                    IrExpr::Column {
                        name: "avg_order_value".to_string(),
                        qualifier: None,
                    }
                );
                // second agg gets synthetic name
                assert_eq!(
                    **right,
                    IrExpr::Column {
                        name: "__agg_1".to_string(),
                        qualifier: None,
                    }
                );
            }
            other => panic!("Expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_binary_arithmetic() {
        let mapping = test_mapping();
        let core = CoreExpr::add(CoreExpr::column("revenue"), CoreExpr::int(10));
        let ir = lower_expr(&core, &mapping).unwrap();
        assert_eq!(
            ir,
            IrExpr::BinaryOp {
                left: Box::new(IrExpr::Column {
                    name: "amount".to_string(),
                    qualifier: None,
                }),
                op: BinaryOp::Add,
                right: Box::new(IrExpr::Number(10.0)),
            }
        );
    }

    #[test]
    fn test_lower_case_expr() {
        let mapping = test_mapping();
        let core = CoreExpr::case(
            vec![semstrait_core::WhenClause::new(
                CoreExpr::eq(CoreExpr::column("region"), CoreExpr::string("US")),
                CoreExpr::int(1),
            )],
            Some(CoreExpr::int(0)),
        );
        let ir = lower_expr(&core, &mapping).unwrap();
        match ir {
            IrExpr::Case {
                when_then,
                else_expr,
            } => {
                assert_eq!(when_then.len(), 1);
                assert!(else_expr.is_some());
            }
            other => panic!("Expected Case, got {:?}", other),
        }
    }

    // ── Test: measure filters as conditional aggregation ────────────
    #[test]
    fn test_lower_measure_with_filters() {
        let mapping = test_mapping();

        // Filter: region = 'US'
        let filter_expr = CoreExpr::eq(CoreExpr::column("region"), CoreExpr::string("US"));
        let filters = vec![CompiledFilter {
            name: "us_only".to_string(),
            expr: filter_expr,
            expr_source: "region = 'US'".to_string(),
        }];

        let core = CoreExpr::sum(CoreExpr::column("revenue"));
        let lowered =
            lower_measure_with_filters("us_revenue", &core, &mapping, &filters).unwrap();

        assert_eq!(lowered.aggregates.len(), 1);
        assert_eq!(lowered.aggregates[0].function, Aggregation::Sum);

        // The aggregate's inner expression should be:
        // CASE WHEN region_name = 'US' THEN amount ELSE NULL END
        match &lowered.aggregates[0].expr {
            IrExpr::Case {
                when_then,
                else_expr,
            } => {
                assert_eq!(when_then.len(), 1);
                // The THEN branch is the physical column (amount)
                assert_eq!(
                    when_then[0].1,
                    IrExpr::Column {
                        name: "amount".to_string(),
                        qualifier: None,
                    }
                );
                assert_eq!(*else_expr, Some(Box::new(IrExpr::Null)));
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
                expr: CoreExpr::eq(CoreExpr::column("region"), CoreExpr::string("US")),
                expr_source: "region = 'US'".to_string(),
            },
            CompiledFilter {
                name: "high_value".to_string(),
                expr: CoreExpr::gt(CoreExpr::column("revenue"), CoreExpr::int(100)),
                expr_source: "revenue > 100".to_string(),
            },
        ];

        let core = CoreExpr::sum(CoreExpr::column("revenue"));
        let lowered =
            lower_measure_with_filters("filtered_rev", &core, &mapping, &filters).unwrap();

        // The aggregate inner should be CASE WHEN (f1 AND f2) THEN amount ELSE NULL END
        match &lowered.aggregates[0].expr {
            IrExpr::Case {
                when_then,
                else_expr,
            } => {
                assert_eq!(when_then.len(), 1);
                // The condition should be an AND of two filters
                match &when_then[0].0 {
                    IrExpr::BinaryOp { op, .. } => {
                        assert_eq!(*op, BinaryOp::And);
                    }
                    other => panic!("Expected AND condition, got {:?}", other),
                }
                assert_eq!(*else_expr, Some(Box::new(IrExpr::Null)));
            }
            other => panic!("Expected Case expression, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_measure_no_filters_unchanged() {
        let mapping = test_mapping();
        let core = CoreExpr::sum(CoreExpr::column("revenue"));

        let without = lower_measure("rev", &core, &mapping).unwrap();
        let with_empty = lower_measure_with_filters("rev", &core, &mapping, &[]).unwrap();

        // Both should produce the same aggregate (no CASE wrapping)
        assert_eq!(without.aggregates[0].expr, with_empty.aggregates[0].expr);
    }

    #[test]
    fn test_lower_guard_becomes_case() {
        let mapping = test_mapping();
        let core = CoreExpr::guard(
            CoreExpr::eq(CoreExpr::column("region"), CoreExpr::string("US")),
            CoreExpr::column("revenue"),
        );
        let ir = lower_expr(&core, &mapping).unwrap();
        match ir {
            IrExpr::Case {
                when_then,
                else_expr,
            } => {
                assert_eq!(when_then.len(), 1);
                assert_eq!(else_expr, Some(Box::new(IrExpr::Null)));
            }
            other => panic!("Expected Case from Guard, got {:?}", other),
        }
    }
}
