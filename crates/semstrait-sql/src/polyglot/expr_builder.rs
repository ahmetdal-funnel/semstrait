//! DslExpr → polyglot_sql::builder::Expr conversion.

use crate::error::EmitError;
use polyglot_sql::builder::{self, Expr};
use polyglot_sql::expressions::{Column, Expression, Function, Identifier, UnaryOp};
use semstrait_ir::{Aggregation, AggregateMeasure, BinaryOp, DslExpr};

/// Create a column reference with quoted identifiers so every dialect applies
/// its native quoting style (double-quotes, backticks, brackets, etc.).
pub(crate) fn quoted_col(name: &str) -> Expr {
    let (table, col_name) = match name.rsplit_once('.') {
        Some((t, c)) => (Some(Identifier::quoted(t)), c),
        None => (None, name),
    };
    Expr(Expression::boxed_column(Column {
        name: Identifier::quoted(col_name),
        table,
        join_mark: false,
        trailing_comments: Vec::new(),
        span: None,
        inferred_type: None,
    }))
}

/// Converts IR `DslExpr` trees into polyglot-sql builder `Expr` nodes.
pub struct ExprBuilder;

impl ExprBuilder {
    /// Convert a `DslExpr` to a polyglot-sql `Expr`.
    pub fn build(&self, expr: &DslExpr) -> Result<Expr, EmitError> {
        match expr {
            DslExpr::Column { name, qualifier } => {
                if let Some(q) = qualifier {
                    Ok(quoted_col(&format!("{q}.{name}")))
                } else {
                    Ok(quoted_col(name))
                }
            }

            DslExpr::Number(n) => {
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
                let built_args = args
                    .iter()
                    .map(|a| self.build(a))
                    .collect::<Result<Vec<_>, _>>()?;

                if *distinct {
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
                let e = self.build(inner)?;
                Ok(Expr(Expression::Neg(Box::new(UnaryOp::new(
                    e.into_inner(),
                )))))
            }

            DslExpr::Not(inner) => Ok(builder::not(self.build(inner)?)),

            DslExpr::Case {
                when_then,
                else_expr,
            } => {
                let mut case = builder::case();
                for (when, then) in when_then {
                    case = case.when(self.build(when)?, self.build(then)?);
                }
                if let Some(else_e) = else_expr {
                    case = case.else_(self.build(else_e)?);
                }
                Ok(case.build())
            }

            DslExpr::IsNull(inner) => Ok(self.build(inner)?.is_null()),

            DslExpr::IsNotNull(inner) => Ok(self.build(inner)?.is_not_null()),

            DslExpr::InList {
                expr,
                list,
                negated,
            } => {
                let e = self.build(expr)?;
                let items = list
                    .iter()
                    .map(|i| self.build(i))
                    .collect::<Result<Vec<_>, _>>()?;
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
                    Ok(builder::not(e.between(l, h)))
                } else {
                    Ok(e.between(l, h))
                }
            }

            DslExpr::Like { expr, pattern } => {
                Ok(self.build(expr)?.like(self.build(pattern)?))
            }

            DslExpr::Coalesce(exprs) => {
                let items = exprs
                    .iter()
                    .map(|e| self.build(e))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(builder::coalesce(items))
            }

            DslExpr::NullIf { expr, null_expr } => {
                Ok(builder::null_if(self.build(expr)?, self.build(null_expr)?))
            }

            DslExpr::DateTrunc { grain, expr } => {
                Ok(builder::func(
                    "DATE_TRUNC",
                    [builder::lit(grain.as_str()), self.build(expr)?],
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
                _ => Ok(distinct_func(aggregation_name(&measure.function), [inner])),
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

/// Create a function call with `DISTINCT` on the AST node.
/// Produces correct `FUNC(DISTINCT args)` via the `Function.distinct` field.
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
