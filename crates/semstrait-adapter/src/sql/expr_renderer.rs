//! Expr -> SQL string rendering.

use super::dialect::SqlDialect;
use super::emit_error::EmitError;
use semstrait_core::expr::{
    AggregateExpr, BetweenExpr, BinaryExpr, CaseExpr, CoalesceExpr, DateTruncExpr,
    FunctionCallExpr, ILikeExpr, InListExpr, LikeExpr, NullIfExpr, RegexpExpr,
    RegexpExtractExpr, UnaryExpr,
};
use semstrait_ir::{Aggregation, AggregateMeasure, Expr};

/// Renders `Expr` trees into SQL string fragments using a given dialect.
pub struct ExprSqlRenderer<'d> {
    dialect: &'d dyn SqlDialect,
}

impl<'d> ExprSqlRenderer<'d> {
    pub fn new(dialect: &'d dyn SqlDialect) -> Self {
        Self { dialect }
    }

    /// Render an `Expr` into a SQL string fragment.
    pub fn render(&self, expr: &Expr) -> Result<String, EmitError> {
        match expr {
            Expr::Column(col) => {
                let quoted = self.dialect.quote_identifier(&col.name);
                match &col.qualifier {
                    Some(q) => {
                        let quoted_q = self.dialect.quote_identifier(q);
                        Ok(format!("{quoted_q}.{quoted}"))
                    }
                    None => Ok(quoted),
                }
            }

            Expr::Literal(lit) => self.render_literal(lit),

            Expr::BinaryOp(BinaryExpr { left, op, right }) => {
                let l = self.render(left)?;
                let r = self.render(right)?;
                if matches!(op, semstrait_core::expr::BinaryOp::SafeDivide) {
                    // Safe division: NULL when divisor is zero
                    Ok(format!("(CASE WHEN {r} = 0 THEN NULL ELSE {l} / {r} END)"))
                } else {
                    Ok(format!("({l} {} {r})", op.as_str()))
                }
            }

            Expr::FunctionCall(FunctionCallExpr { name, args, distinct }) => {
                let rendered_args: Result<Vec<String>, EmitError> =
                    args.iter().map(|a| self.render(a)).collect();
                let rendered_args = rendered_args?;
                let args_str = rendered_args.join(", ");
                if *distinct {
                    Ok(format!("{name}(DISTINCT {args_str})"))
                } else {
                    Ok(format!("{name}({args_str})"))
                }
            }

            Expr::Aggregate(AggregateExpr { function, expr, distinct }) => {
                let inner = self.render(expr)?;
                let func_name = function.sql_name();
                if *distinct || matches!(function, Aggregation::CountDistinct) {
                    Ok(format!("{func_name}(DISTINCT {inner})"))
                } else {
                    Ok(format!("{func_name}({inner})"))
                }
            }

            Expr::Negate(UnaryExpr { expr }) => {
                let rendered = self.render(expr)?;
                Ok(format!("(-{rendered})"))
            }

            Expr::Case(CaseExpr { when_then, else_expr }) => {
                let mut sql = String::from("CASE");
                for clause in when_then {
                    let w = self.render(&clause.condition)?;
                    let t = self.render(&clause.result)?;
                    sql.push_str(&format!(" WHEN {w} THEN {t}"));
                }
                if let Some(else_e) = else_expr {
                    let e = self.render(else_e)?;
                    sql.push_str(&format!(" ELSE {e}"));
                }
                sql.push_str(" END");
                Ok(sql)
            }

            Expr::Not(UnaryExpr { expr }) => {
                let rendered = self.render(expr)?;
                Ok(format!("NOT ({rendered})"))
            }

            Expr::IsNull(UnaryExpr { expr }) => {
                let rendered = self.render(expr)?;
                Ok(format!("{rendered} IS NULL"))
            }

            Expr::IsNotNull(UnaryExpr { expr }) => {
                let rendered = self.render(expr)?;
                Ok(format!("{rendered} IS NOT NULL"))
            }

            Expr::InList(InListExpr { expr, list, negated }) => {
                let rendered_expr = self.render(expr)?;
                let items: Result<Vec<String>, EmitError> =
                    list.iter().map(|e| self.render(e)).collect();
                let items = items?.join(", ");
                if *negated {
                    Ok(format!("{rendered_expr} NOT IN ({items})"))
                } else {
                    Ok(format!("{rendered_expr} IN ({items})"))
                }
            }

            Expr::Between(BetweenExpr { expr, low, high, negated }) => {
                let rendered_expr = self.render(expr)?;
                let rendered_low = self.render(low)?;
                let rendered_high = self.render(high)?;
                if *negated {
                    Ok(format!("{rendered_expr} NOT BETWEEN {rendered_low} AND {rendered_high}"))
                } else {
                    Ok(format!("{rendered_expr} BETWEEN {rendered_low} AND {rendered_high}"))
                }
            }

            Expr::Like(LikeExpr { expr, pattern }) => {
                let rendered_expr = self.render(expr)?;
                let rendered_pattern = self.render(pattern)?;
                Ok(format!("{rendered_expr} LIKE {rendered_pattern}"))
            }

            Expr::ILike(ILikeExpr { expr, pattern }) => {
                let rendered_expr = self.render(expr)?;
                let rendered_pattern = self.render(pattern)?;
                Ok(self.dialect.ilike(&rendered_expr, &rendered_pattern))
            }

            Expr::RegexpMatch(RegexpExpr { expr, pattern, full_match }) => {
                let rendered_expr = self.render(expr)?;
                let rendered_pattern = self.render(pattern)?;
                Ok(self.dialect.regexp_match(&rendered_expr, &rendered_pattern, *full_match))
            }

            Expr::RegexpExtract(RegexpExtractExpr { expr, pattern, group_idx }) => {
                let rendered_expr = self.render(expr)?;
                let rendered_pattern = self.render(pattern)?;
                Ok(self.dialect.regexp_extract(&rendered_expr, &rendered_pattern, *group_idx))
            }

            Expr::Coalesce(CoalesceExpr { exprs }) => {
                let items: Result<Vec<String>, EmitError> =
                    exprs.iter().map(|e| self.render(e)).collect();
                let items = items?.join(", ");
                Ok(format!("COALESCE({items})"))
            }

            Expr::NullIf(NullIfExpr { expr, null_expr }) => {
                let rendered_expr = self.render(expr)?;
                let rendered_null = self.render(null_expr)?;
                Ok(format!("NULLIF({rendered_expr}, {rendered_null})"))
            }

            Expr::DateTrunc(DateTruncExpr { grain, expr }) => {
                let rendered_expr = self.render(expr)?;
                Ok(self.dialect.date_trunc(grain, &rendered_expr))
            }

            Expr::Cast(c) => {
                let rendered_expr = self.render(&c.expr)?;
                Ok(format!("CAST({rendered_expr} AS {})", c.data_type))
            }

            // EntityRef and Guard should never appear in plan nodes (resolved during planning).
            Expr::EntityRef(e) => Err(EmitError::UnsupportedExpr(format!(
                "EntityRef('{}') should have been resolved during planning",
                e.name
            ))),

            Expr::Guard(_) => Err(EmitError::UnsupportedExpr(
                "Guard should have been resolved during planning".to_string(),
            )),
        }
    }

    /// Render an `AggregateMeasure` into a SQL string (e.g., `SUM("amount")`).
    pub fn render_aggregate(&self, measure: &AggregateMeasure) -> Result<String, EmitError> {
        let inner = self.render(&measure.expr)?;
        let func_name = measure.function.sql_name();

        if measure.distinct || matches!(measure.function, Aggregation::CountDistinct) {
            Ok(format!("{func_name}(DISTINCT {inner})"))
        } else {
            Ok(format!("{func_name}({inner})"))
        }
    }

    /// Render a `Literal` into a SQL string.
    fn render_literal(&self, lit: &semstrait_core::expr::Literal) -> Result<String, EmitError> {
        use semstrait_core::expr::Literal;
        match lit {
            Literal::Integer { value } => Ok(format!("{value}")),
            Literal::Float { value } => Ok(format!("{value}")),
            Literal::String { value } => {
                // Escape single quotes by doubling them
                let escaped = value.replace('\'', "''");
                Ok(format!("'{escaped}'"))
            }
            Literal::Boolean { value } => {
                Ok(if *value { "TRUE".to_string() } else { "FALSE".to_string() })
            }
            Literal::Null => Ok("NULL".to_string()),
        }
    }
}
