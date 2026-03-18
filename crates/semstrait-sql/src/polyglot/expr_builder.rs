//! DslExpr → polyglot_sql::builder::Expr conversion.

use crate::error::EmitError;
use polyglot_sql::builder::{self, Expr};
use polyglot_sql::expressions::{Column, Expression, Function, Identifier, UnaryOp};
use semstrait_ir::{Aggregation, AggregateMeasure, BinaryOp, DslExpr};

/// Create a column reference with quoted identifiers so every dialect applies
/// its native quoting style (double-quotes, backticks, brackets, etc.).
pub(crate) fn quoted_col(name: &str) -> Expr {
    if let Some((table, column)) = name.rsplit_once('.') {
        Expr(Expression::boxed_column(Column {
            name: Identifier::quoted(column),
            table: Some(Identifier::quoted(table)),
            join_mark: false,
            trailing_comments: Vec::new(),
            span: None,
            inferred_type: None,
        }))
    } else {
        Expr(Expression::boxed_column(Column {
            name: Identifier::quoted(name),
            table: None,
            join_mark: false,
            trailing_comments: Vec::new(),
            span: None,
            inferred_type: None,
        }))
    }
}

/// Converts IR `DslExpr` trees into polyglot-sql builder `Expr` nodes.
pub struct ExprBuilder;

impl ExprBuilder {
    /// Convert a `DslExpr` to a polyglot-sql `Expr`.
    pub fn build(&self, expr: &DslExpr) -> Result<Expr, EmitError> {
        match expr {
            DslExpr::Column { name, qualifier } => {
                let col_ref = match qualifier {
                    Some(q) => format!("{q}.{name}"),
                    None => name.clone(),
                };
                Ok(quoted_col(&col_ref))
            }

            DslExpr::Number(n) => {
                // Render integers without fractional part.
                // Guard against precision loss near i64 boundary (f64 has 53-bit mantissa).
                if n.fract() == 0.0 && n.abs() < (1_i64 << 53) as f64 {
                    Ok(builder::lit(*n as i64))
                } else {
                    Ok(builder::lit(*n))
                }
            }

            DslExpr::StringLit(s) => Ok(builder::lit(s.as_str())),

            DslExpr::Bool(b) => Ok(builder::boolean(*b)),

            DslExpr::Null => Ok(builder::null()),

            DslExpr::BinaryOp { left, op, right } => {
                let l = self.build(left)?;
                let r = self.build(right)?;

                match op {
                    BinaryOp::Add => Ok(l.add(r)),
                    BinaryOp::Subtract => Ok(l.sub(r)),
                    BinaryOp::Multiply => Ok(l.mul(r)),
                    BinaryOp::Divide => Ok(l.div(r)),
                    BinaryOp::SafeDivide => {
                        // CASE WHEN r = 0 THEN NULL ELSE l / r END
                        let r2 = self.build(right)?;
                        let l2 = self.build(left)?;
                        Ok(builder::case()
                            .when(r.eq(builder::lit(0)), builder::null())
                            .else_(l2.div(r2))
                            .build())
                    }
                    BinaryOp::Eq => Ok(l.eq(r)),
                    BinaryOp::NotEq => Ok(l.neq(r)),
                    BinaryOp::Lt => Ok(l.lt(r)),
                    BinaryOp::LtEq => Ok(l.lte(r)),
                    BinaryOp::Gt => Ok(l.gt(r)),
                    BinaryOp::GtEq => Ok(l.gte(r)),
                    BinaryOp::And => Ok(l.and(r)),
                    BinaryOp::Or => Ok(l.or(r)),
                }
            }

            DslExpr::FunctionCall {
                name,
                args,
                distinct,
            } => {
                let built_args: Result<Vec<Expr>, EmitError> =
                    args.iter().map(|a| self.build(a)).collect();
                let built_args = built_args?;

                if *distinct {
                    // For COUNT(DISTINCT x), use the dedicated helper
                    if name.eq_ignore_ascii_case("COUNT") && built_args.len() == 1 {
                        Ok(builder::count_distinct(built_args.into_iter().next().unwrap()))
                    } else {
                        Ok(distinct_func(name, built_args))
                    }
                } else {
                    Ok(builder::func(name, built_args))
                }
            }

            DslExpr::Negate(inner) => {
                let inner_expr = self.build(inner)?;
                // Construct Expression::Neg directly (no builder helper exists)
                Ok(Expr(Expression::Neg(Box::new(UnaryOp::new(
                    inner_expr.into_inner(),
                )))))
            }

            DslExpr::Not(inner) => {
                let inner_expr = self.build(inner)?;
                Ok(builder::not(inner_expr))
            }

            DslExpr::Case {
                when_then,
                else_expr,
            } => {
                let mut case = builder::case();
                for (when, then) in when_then {
                    let w = self.build(when)?;
                    let t = self.build(then)?;
                    case = case.when(w, t);
                }
                if let Some(else_e) = else_expr {
                    case = case.else_(self.build(else_e)?);
                }
                Ok(case.build())
            }

            DslExpr::IsNull(inner) => {
                let inner_expr = self.build(inner)?;
                Ok(inner_expr.is_null())
            }

            DslExpr::IsNotNull(inner) => {
                let inner_expr = self.build(inner)?;
                Ok(inner_expr.is_not_null())
            }

            DslExpr::InList {
                expr,
                list,
                negated,
            } => {
                let e = self.build(expr)?;
                let items: Result<Vec<Expr>, EmitError> =
                    list.iter().map(|i| self.build(i)).collect();
                let items = items?;
                if *negated {
                    Ok(e.not_in(items))
                } else {
                    Ok(e.in_list(items))
                }
            }

            DslExpr::Between {
                expr,
                low,
                high,
                negated,
            } => {
                let e = self.build(expr)?;
                let l = self.build(low)?;
                let h = self.build(high)?;
                if *negated {
                    // No not_between in builder — use NOT (expr BETWEEN low AND high)
                    Ok(builder::not(e.between(l, h)))
                } else {
                    Ok(e.between(l, h))
                }
            }

            DslExpr::Like { expr, pattern } => {
                let e = self.build(expr)?;
                let p = self.build(pattern)?;
                Ok(e.like(p))
            }

            DslExpr::Coalesce(exprs) => {
                let items: Result<Vec<Expr>, EmitError> =
                    exprs.iter().map(|e| self.build(e)).collect();
                Ok(builder::coalesce(items?))
            }

            DslExpr::NullIf { expr, null_expr } => {
                let e = self.build(expr)?;
                let n = self.build(null_expr)?;
                Ok(builder::null_if(e, n))
            }

            DslExpr::DateTrunc { grain, expr } => {
                let e = self.build(expr)?;
                Ok(builder::func(
                    "DATE_TRUNC",
                    [builder::lit(grain.as_str()), e],
                ))
            }
        }
    }

    /// Convert an `AggregateMeasure` to a polyglot-sql `Expr`.
    pub fn build_aggregate(&self, measure: &AggregateMeasure) -> Result<Expr, EmitError> {
        let inner = self.build(&measure.expr)?;

        let is_distinct =
            measure.distinct || matches!(measure.function, Aggregation::CountDistinct);

        if is_distinct {
            match measure.function {
                Aggregation::Count | Aggregation::CountDistinct => {
                    Ok(builder::count_distinct(inner))
                }
                _ => {
                    let func_name = aggregation_name(&measure.function);
                    Ok(distinct_func(func_name, [inner]))
                }
            }
        } else {
            match measure.function {
                Aggregation::Sum => Ok(builder::sum(inner)),
                Aggregation::Avg => Ok(builder::avg(inner)),
                Aggregation::Count => Ok(builder::count(inner)),
                Aggregation::CountDistinct => Ok(builder::count_distinct(inner)),
                Aggregation::Min => Ok(builder::min_(inner)),
                Aggregation::Max => Ok(builder::max_(inner)),
            }
        }
    }
}

/// Create a function call with `DISTINCT` set on the AST node.
/// Produces `FUNC(DISTINCT args)` instead of the incorrect `FUNC DISTINCT(args)`.
fn distinct_func(name: &str, args: impl IntoIterator<Item = Expr>) -> Expr {
    Expr(Expression::Function(Box::new(Function {
        name: name.to_string(),
        args: args.into_iter().map(|a| a.into_inner()).collect(),
        distinct: true,
        ..Function::default()
    })))
}

fn aggregation_name(agg: &Aggregation) -> &'static str {
    match agg {
        Aggregation::Sum => "SUM",
        Aggregation::Avg => "AVG",
        Aggregation::Count | Aggregation::CountDistinct => "COUNT",
        Aggregation::Min => "MIN",
        Aggregation::Max => "MAX",
    }
}
