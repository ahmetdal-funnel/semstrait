//! Lowers DSL AST (`DslExpr`) to planner `Expr`.
//!
//! This conversion happens during query resolution, once semantic names
//! (measures, dimensions, metrics) have been resolved to concrete columns.
//! The lowering validates that function names are recognized and that
//! expression structure is well-formed.

use super::ast::{BinaryOp, CaseExpr, ColumnRef, DslExpr, FunctionCall};
use crate::planner::ir::expr::{BinaryOperator, Column, Expr, Literal};
use crate::schema::types::Aggregation;

/// Error during DSL → Expr lowering.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LowerError {
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    #[error("function '{name}' expects {expected} arguments, got {got}")]
    WrongArgCount {
        name: String,
        expected: usize,
        got: usize,
    },
    #[error("DISTINCT modifier only valid for COUNT")]
    InvalidDistinct,
    #[error("{0}")]
    #[allow(dead_code)]
    Other(String),
}

/// Lower a DSL expression to a planner Expr.
///
/// This performs a structural conversion without semantic validation —
/// column references are lowered as unqualified Column nodes. The caller
/// is responsible for resolving names to physical columns.
pub fn lower_expr(dsl: &DslExpr) -> Result<Expr, LowerError> {
    match dsl {
        DslExpr::Number(n) => {
            if *n == (*n as i64) as f64 {
                Ok(Expr::Literal(Literal::Int(*n as i64)))
            } else {
                Ok(Expr::Literal(Literal::Float(*n)))
            }
        }
        DslExpr::StringLit(s) => Ok(Expr::Literal(Literal::String(s.clone()))),
        DslExpr::Bool(b) => Ok(Expr::Literal(Literal::Bool(*b))),
        DslExpr::Null => Ok(Expr::Literal(Literal::Null("unknown".to_string()))),
        DslExpr::ColumnRef(col_ref) => Ok(lower_column_ref(col_ref)),
        DslExpr::FunctionCall(fc) => lower_function_call(fc),
        DslExpr::BinaryOp { left, op, right } => {
            let left_expr = lower_expr(left)?;
            let right_expr = lower_expr(right)?;
            Ok(lower_binary_op(*op, left_expr, right_expr))
        }
        DslExpr::Negate(inner) => {
            let inner_expr = lower_expr(inner)?;
            Ok(Expr::Subtract(
                Box::new(Expr::Literal(Literal::Int(0))),
                Box::new(inner_expr),
            ))
        }
        DslExpr::Case(case) => lower_case(case),
        DslExpr::Paren(inner) => lower_expr(inner),
    }
}

fn lower_column_ref(col_ref: &ColumnRef) -> Expr {
    match &col_ref.qualifier {
        Some(q) => Expr::Column(Column::new(q, &col_ref.name)),
        None => Expr::Column(Column::unqualified(&col_ref.name)),
    }
}

fn lower_binary_op(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    match op {
        BinaryOp::Add => Expr::Add(Box::new(left), Box::new(right)),
        BinaryOp::Subtract => Expr::Subtract(Box::new(left), Box::new(right)),
        BinaryOp::Multiply => Expr::Multiply(Box::new(left), Box::new(right)),
        BinaryOp::Divide => Expr::Divide(Box::new(left), Box::new(right)),
        BinaryOp::Eq => Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Eq,
            right: Box::new(right),
        },
        BinaryOp::NotEq => Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::NotEq,
            right: Box::new(right),
        },
        BinaryOp::Lt => Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Lt,
            right: Box::new(right),
        },
        BinaryOp::LtEq => Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::LtEq,
            right: Box::new(right),
        },
        BinaryOp::Gt => Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Gt,
            right: Box::new(right),
        },
        BinaryOp::GtEq => Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::GtEq,
            right: Box::new(right),
        },
        BinaryOp::And => Expr::And(vec![left, right]),
        BinaryOp::Or => Expr::Or(vec![left, right]),
    }
}

/// Lower a function call. Recognizes built-in aggregation functions
/// and maps them to `AggregateExpr` or structural `Expr` forms.
fn lower_function_call(fc: &FunctionCall) -> Result<Expr, LowerError> {
    let upper_name = fc.name.to_ascii_uppercase();

    // Validate DISTINCT usage
    if fc.distinct && upper_name != "COUNT" {
        return Err(LowerError::InvalidDistinct);
    }

    match upper_name.as_str() {
        // Aggregation functions → lowered to Expr::Sql placeholder for now.
        // The planner will extract these into AggregateExpr during plan construction.
        "SUM" | "AVG" | "MIN" | "MAX" | "COUNT" => {
            if upper_name == "COUNT" && fc.args.is_empty() {
                // COUNT() with no args → COUNT(*)
                return Ok(Expr::Sql(format!("COUNT({})", if fc.distinct { "DISTINCT *" } else { "*" })));
            }
            if fc.args.len() != 1 {
                return Err(LowerError::WrongArgCount {
                    name: fc.name.clone(),
                    expected: 1,
                    got: fc.args.len(),
                });
            }
            let arg = lower_expr(&fc.args[0])?;
            let agg = match upper_name.as_str() {
                "SUM" => Aggregation::Sum,
                "AVG" => Aggregation::Avg,
                "MIN" => Aggregation::Min,
                "MAX" => Aggregation::Max,
                "COUNT" if fc.distinct => Aggregation::CountDistinct,
                "COUNT" => Aggregation::Count,
                _ => unreachable!(),
            };
            // Represent as a placeholder — the planner will extract this
            // into an AggregateExpr on the Aggregate node.
            Ok(Expr::Sql(format!(
                "{}({})",
                agg,
                expr_to_placeholder(&arg),
            )))
        }

        // Scalar functions
        "COALESCE" => {
            if fc.args.is_empty() {
                return Err(LowerError::WrongArgCount {
                    name: fc.name.clone(),
                    expected: 1,
                    got: 0,
                });
            }
            let args: Result<Vec<_>, _> = fc.args.iter().map(lower_expr).collect();
            Ok(Expr::Coalesce(args?))
        }

        "NULLIF" => {
            if fc.args.len() != 2 {
                return Err(LowerError::WrongArgCount {
                    name: fc.name.clone(),
                    expected: 2,
                    got: fc.args.len(),
                });
            }
            let a = lower_expr(&fc.args[0])?;
            let b = lower_expr(&fc.args[1])?;
            // NULLIF(a, b) → CASE WHEN a = b THEN NULL ELSE a END
            Ok(Expr::Case {
                when_then: vec![(
                    Expr::BinaryOp {
                        left: Box::new(a.clone()),
                        op: BinaryOperator::Eq,
                        right: Box::new(b),
                    },
                    Expr::Literal(Literal::Null("unknown".to_string())),
                )],
                else_result: Some(Box::new(a)),
            })
        }

        "DATE_TRUNC" | "CAST" => {
            // Pass through as SQL placeholder — dialect-specific
            let args: Result<Vec<_>, _> = fc.args.iter().map(lower_expr).collect();
            let args = args?;
            let arg_strs: Vec<String> = args.iter().map(expr_to_placeholder).collect();
            Ok(Expr::Sql(format!("{}({})", upper_name, arg_strs.join(", "))))
        }

        _ => Err(LowerError::UnknownFunction(fc.name.clone())),
    }
}

fn lower_case(case: &CaseExpr) -> Result<Expr, LowerError> {
    let mut when_then = Vec::new();
    for wc in &case.when_clauses {
        let condition = lower_expr(&wc.condition)?;
        let result = lower_expr(&wc.result)?;
        when_then.push((condition, result));
    }
    let else_result = case
        .else_expr
        .as_ref()
        .map(|e| lower_expr(e))
        .transpose()?
        .map(Box::new);

    Ok(Expr::Case {
        when_then,
        else_result,
    })
}

/// Extract an aggregate function and its inner expression from a DSL expression.
///
/// Returns `(aggregation, inner_expr, alias)`. For non-aggregate DSL
/// (e.g. a bare column ref), defaults to `Aggregation::Sum`.
pub fn lower_aggregate(
    dsl: &DslExpr,
    default_alias: &str,
) -> Result<(Aggregation, Expr, String), LowerError> {
    match dsl {
        DslExpr::FunctionCall(fc) => {
            let upper_name = fc.name.to_ascii_uppercase();

            if fc.distinct && upper_name != "COUNT" {
                return Err(LowerError::InvalidDistinct);
            }

            let func = match upper_name.as_str() {
                "SUM" => Aggregation::Sum,
                "AVG" => Aggregation::Avg,
                "MIN" => Aggregation::Min,
                "MAX" => Aggregation::Max,
                "COUNT" if fc.distinct => Aggregation::CountDistinct,
                "COUNT" => Aggregation::Count,
                _ => return Err(LowerError::UnknownFunction(fc.name.clone())),
            };

            if upper_name == "COUNT" && fc.args.is_empty() {
                return Ok((func, Expr::Literal(Literal::Int(1)), default_alias.to_string()));
            }

            if fc.args.len() != 1 {
                return Err(LowerError::WrongArgCount {
                    name: fc.name.clone(),
                    expected: 1,
                    got: fc.args.len(),
                });
            }

            let inner = lower_expr(&fc.args[0])?;
            Ok((func, inner, default_alias.to_string()))
        }
        other => {
            let inner = lower_expr(other)?;
            Ok((Aggregation::Sum, inner, default_alias.to_string()))
        }
    }
}

/// Quick placeholder serialization for embedding in Expr::Sql.
fn expr_to_placeholder(expr: &Expr) -> String {
    match expr {
        Expr::Column(col) => col.qualified_name(),
        Expr::Literal(Literal::Int(n)) => n.to_string(),
        Expr::Literal(Literal::Float(f)) => f.to_string(),
        Expr::Literal(Literal::String(s)) => format!("'{}'", s),
        Expr::Literal(Literal::Bool(b)) => b.to_string(),
        Expr::Literal(Literal::Null(_)) => "NULL".to_string(),
        _ => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::parser::parse_dsl;

    #[test]
    fn test_lower_simple_column() {
        let dsl = parse_dsl("amount").unwrap();
        let expr = lower_expr(&dsl).unwrap();
        assert!(matches!(expr, Expr::Column(_)));
    }

    #[test]
    fn test_lower_arithmetic() {
        let dsl = parse_dsl("revenue / users").unwrap();
        let expr = lower_expr(&dsl).unwrap();
        assert!(matches!(expr, Expr::Divide(_, _)));
    }

    #[test]
    fn test_lower_sum() {
        let dsl = parse_dsl("SUM(amount)").unwrap();
        let expr = lower_expr(&dsl).unwrap();
        // Currently lowered to Expr::Sql placeholder
        assert!(matches!(expr, Expr::Sql(_)));
    }

    #[test]
    fn test_lower_coalesce() {
        let dsl = parse_dsl("COALESCE(a, b, 0)").unwrap();
        let expr = lower_expr(&dsl).unwrap();
        assert!(matches!(expr, Expr::Coalesce(_)));
    }

    #[test]
    fn test_lower_case() {
        let dsl = parse_dsl("CASE WHEN status != 'cancelled' THEN amount ELSE 0 END").unwrap();
        let expr = lower_expr(&dsl).unwrap();
        assert!(matches!(expr, Expr::Case { .. }));
    }

    #[test]
    fn test_lower_unknown_function() {
        let dsl = parse_dsl("FOOBAR(x)").unwrap();
        let err = lower_expr(&dsl).unwrap_err();
        assert!(matches!(err, LowerError::UnknownFunction(_)));
    }

    #[test]
    fn test_lower_distinct_non_count() {
        let dsl = parse_dsl("SUM(DISTINCT x)").unwrap();
        let err = lower_expr(&dsl).unwrap_err();
        assert!(matches!(err, LowerError::InvalidDistinct));
    }

    #[test]
    fn test_lower_aggregate_sum() {
        let dsl = parse_dsl("SUM(amount)").unwrap();
        let (agg, inner, alias) = lower_aggregate(&dsl, "revenue").unwrap();
        assert_eq!(agg, Aggregation::Sum);
        assert!(matches!(inner, Expr::Column(_)));
        assert_eq!(alias, "revenue");
    }

    #[test]
    fn test_lower_aggregate_count_distinct() {
        let dsl = parse_dsl("COUNT(DISTINCT user_id)").unwrap();
        let (agg, inner, _) = lower_aggregate(&dsl, "users").unwrap();
        assert_eq!(agg, Aggregation::CountDistinct);
        assert!(matches!(inner, Expr::Column(_)));
    }

    #[test]
    fn test_lower_aggregate_bare_column_defaults_to_sum() {
        let dsl = parse_dsl("amount").unwrap();
        let (agg, inner, _) = lower_aggregate(&dsl, "rev").unwrap();
        assert_eq!(agg, Aggregation::Sum);
        assert!(matches!(inner, Expr::Column(_)));
    }
}
