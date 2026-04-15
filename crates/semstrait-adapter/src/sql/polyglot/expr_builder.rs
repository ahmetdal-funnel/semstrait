//! Expr -> polyglot_sql::builder::Expr conversion.

use crate::sql::emit_error::EmitError;
use polyglot_sql::builder::{self, Expr};
use polyglot_sql::expressions::{Column, Expression, Function, Identifier, UnaryOp};
use semstrait_core::expr::{
    AggregateExpr, BetweenExpr, BinaryExpr, CaseExpr, CoalesceExpr, DateTruncExpr,
    FunctionCallExpr, ILikeExpr, InListExpr, LikeExpr, NullIfExpr, RegexpExpr,
    RegexpExtractExpr, UnaryExpr,
};
use semstrait_ir::{Aggregation, AggregateMeasure, BinaryOp, Expr as IrExpr};

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

/// Converts IR `Expr` trees into polyglot-sql builder `Expr` nodes.
pub struct ExprBuilder;

impl ExprBuilder {
    /// Convert an `Expr` to a polyglot-sql `Expr`.
    pub fn build(&self, expr: &IrExpr) -> Result<Expr, EmitError> {
        match expr {
            IrExpr::Column(col) => {
                if let Some(q) = &col.qualifier {
                    Ok(quoted_col(&format!("{q}.{}", col.name)))
                } else {
                    Ok(quoted_col(&col.name))
                }
            }

            IrExpr::Literal(lit) => self.build_literal(lit),

            IrExpr::BinaryOp(BinaryExpr { left, op, right }) => {
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

            IrExpr::FunctionCall(FunctionCallExpr { name, args, distinct }) => {
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

            IrExpr::Aggregate(AggregateExpr { function, expr, distinct }) => {
                let inner = self.build(expr)?;
                let is_distinct =
                    *distinct || matches!(function, Aggregation::CountDistinct);

                if is_distinct {
                    match function {
                        Aggregation::Count | Aggregation::CountDistinct => {
                            Ok(builder::count_distinct(inner))
                        }
                        _ => Ok(distinct_func(function.sql_name(), [inner])),
                    }
                } else {
                    match function {
                        Aggregation::Sum => Ok(builder::sum(inner)),
                        Aggregation::Avg => Ok(builder::avg(inner)),
                        Aggregation::Count => Ok(builder::count(inner)),
                        Aggregation::CountDistinct => Ok(builder::count_distinct(inner)),
                        Aggregation::Min => Ok(builder::min_(inner)),
                        Aggregation::Max => Ok(builder::max_(inner)),
                    }
                }
            }

            IrExpr::Negate(UnaryExpr { expr }) => {
                let e = self.build(expr)?;
                Ok(Expr(Expression::Neg(Box::new(UnaryOp::new(
                    e.into_inner(),
                )))))
            }

            IrExpr::Not(UnaryExpr { expr }) => Ok(builder::not(self.build(expr)?)),

            IrExpr::Case(CaseExpr { when_then, else_expr }) => {
                let mut case = builder::case();
                for clause in when_then {
                    case = case.when(self.build(&clause.condition)?, self.build(&clause.result)?);
                }
                if let Some(else_e) = else_expr {
                    case = case.else_(self.build(else_e)?);
                }
                Ok(case.build())
            }

            IrExpr::IsNull(UnaryExpr { expr }) => Ok(self.build(expr)?.is_null()),

            IrExpr::IsNotNull(UnaryExpr { expr }) => Ok(self.build(expr)?.is_not_null()),

            IrExpr::InList(InListExpr { expr, list, negated }) => {
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

            IrExpr::Between(BetweenExpr { expr, low, high, negated }) => {
                let e = self.build(expr)?;
                let l = self.build(low)?;
                let h = self.build(high)?;
                if *negated {
                    Ok(builder::not(e.between(l, h)))
                } else {
                    Ok(e.between(l, h))
                }
            }

            IrExpr::Like(LikeExpr { expr, pattern }) => {
                Ok(self.build(expr)?.like(self.build(pattern)?))
            }

            IrExpr::ILike(ILikeExpr { expr, pattern }) => {
                // polyglot-sql .ilike() maps to dialect-appropriate form
                Ok(self.build(expr)?.ilike(self.build(pattern)?))
            }

            IrExpr::RegexpMatch(RegexpExpr { expr, pattern, full_match }) => {
                let e = self.build(expr)?;
                let p = self.build(pattern)?;
                // Use REGEXP_LIKE function — polyglot transpiles per dialect
                if *full_match {
                    Ok(builder::func("REGEXP_LIKE", [e, builder::func("CONCAT", [builder::lit("^"), p, builder::lit("$")])]))
                } else {
                    Ok(builder::func("REGEXP_LIKE", [e, p]))
                }
            }

            IrExpr::RegexpExtract(RegexpExtractExpr { expr, pattern, group_idx }) => {
                let e = self.build(expr)?;
                let p = self.build(pattern)?;
                let idx = builder::lit(group_idx.to_string());
                Ok(builder::func("REGEXP_EXTRACT", [e, p, idx]))
            }

            IrExpr::Coalesce(CoalesceExpr { exprs }) => {
                let items = exprs
                    .iter()
                    .map(|e| self.build(e))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(builder::coalesce(items))
            }

            IrExpr::NullIf(NullIfExpr { expr, null_expr }) => {
                Ok(builder::null_if(self.build(expr)?, self.build(null_expr)?))
            }

            IrExpr::DateTrunc(DateTruncExpr { grain, expr }) => {
                Ok(builder::func(
                    "DATE_TRUNC",
                    [builder::lit(grain.to_string().as_str()), self.build(expr)?],
                ))
            }

            IrExpr::Cast(c) => {
                let inner = self.build(&c.expr)?;
                Ok(inner.cast(&c.data_type.to_string()))
            }

            // EntityRef and Guard should never appear in plan nodes (resolved during planning).
            IrExpr::EntityRef(e) => Err(EmitError::UnsupportedExpr(format!(
                "EntityRef('{}') should have been resolved during planning",
                e.name
            ))),

            IrExpr::Guard(_) => Err(EmitError::UnsupportedExpr(
                "Guard should have been resolved during planning".to_string(),
            )),
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
                _ => Ok(distinct_func(measure.function.sql_name(), [inner])),
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

    /// Build a polyglot-sql `Expr` from a `Literal`.
    fn build_literal(&self, lit: &semstrait_core::expr::Literal) -> Result<Expr, EmitError> {
        use semstrait_core::expr::Literal;
        match lit {
            Literal::Integer { value } => Ok(builder::lit(*value)),
            Literal::Float { value } => Ok(builder::lit(*value)),
            Literal::String { value } => Ok(builder::lit(value.as_str())),
            Literal::Boolean { value } => Ok(builder::boolean(*value)),
            Literal::Null => Ok(builder::null()),
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
