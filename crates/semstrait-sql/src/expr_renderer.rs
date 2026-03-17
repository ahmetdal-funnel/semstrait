//! DslExpr → SQL string rendering.

use crate::dialect::SqlDialect;
use crate::error::EmitError;
use semstrait_ir::{Aggregation, AggregateMeasure, DslExpr};

/// Renders `DslExpr` trees into SQL string fragments using a given dialect.
pub struct DslExprSqlRenderer<'d> {
    dialect: &'d dyn SqlDialect,
}

impl<'d> DslExprSqlRenderer<'d> {
    pub fn new(dialect: &'d dyn SqlDialect) -> Self {
        Self { dialect }
    }

    /// Render a `DslExpr` into a SQL string fragment.
    pub fn render(&self, expr: &DslExpr) -> Result<String, EmitError> {
        match expr {
            DslExpr::Column { name, qualifier } => {
                let quoted = self.dialect.quote_identifier(name);
                match qualifier {
                    Some(q) => {
                        let quoted_q = self.dialect.quote_identifier(q);
                        Ok(format!("{quoted_q}.{quoted}"))
                    }
                    None => Ok(quoted),
                }
            }

            DslExpr::Number(n) => {
                // Render integers without decimal point
                if n.fract() == 0.0 && n.abs() < i64::MAX as f64 {
                    Ok(format!("{}", *n as i64))
                } else {
                    Ok(format!("{n}"))
                }
            }

            DslExpr::StringLit(s) => {
                // Escape single quotes by doubling them
                let escaped = s.replace('\'', "''");
                Ok(format!("'{escaped}'"))
            }

            DslExpr::Bool(b) => {
                Ok(if *b { "TRUE".to_string() } else { "FALSE".to_string() })
            }

            DslExpr::Null => Ok("NULL".to_string()),

            DslExpr::BinaryOp { left, op, right } => {
                let l = self.render(left)?;
                let r = self.render(right)?;
                if matches!(op, semstrait_ir::BinaryOp::SafeDivide) {
                    // Safe division: NULL when divisor is zero
                    Ok(format!("(CASE WHEN {r} = 0 THEN NULL ELSE {l} / {r} END)"))
                } else {
                    Ok(format!("({l} {} {r})", op.as_str()))
                }
            }

            DslExpr::FunctionCall { name, args, distinct } => {
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

            DslExpr::Negate(inner) => {
                let rendered = self.render(inner)?;
                Ok(format!("(-{rendered})"))
            }

            DslExpr::Case { when_then, else_expr } => {
                let mut sql = String::from("CASE");
                for (when, then) in when_then {
                    let w = self.render(when)?;
                    let t = self.render(then)?;
                    sql.push_str(&format!(" WHEN {w} THEN {t}"));
                }
                if let Some(else_e) = else_expr {
                    let e = self.render(else_e)?;
                    sql.push_str(&format!(" ELSE {e}"));
                }
                sql.push_str(" END");
                Ok(sql)
            }

            DslExpr::Not(inner) => {
                let rendered = self.render(inner)?;
                Ok(format!("NOT ({rendered})"))
            }

            DslExpr::IsNull(inner) => {
                let rendered = self.render(inner)?;
                Ok(format!("{rendered} IS NULL"))
            }

            DslExpr::IsNotNull(inner) => {
                let rendered = self.render(inner)?;
                Ok(format!("{rendered} IS NOT NULL"))
            }

            DslExpr::InList { expr, list, negated } => {
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

            DslExpr::Between { expr, low, high, negated } => {
                let rendered_expr = self.render(expr)?;
                let rendered_low = self.render(low)?;
                let rendered_high = self.render(high)?;
                if *negated {
                    Ok(format!("{rendered_expr} NOT BETWEEN {rendered_low} AND {rendered_high}"))
                } else {
                    Ok(format!("{rendered_expr} BETWEEN {rendered_low} AND {rendered_high}"))
                }
            }

            DslExpr::Like { expr, pattern } => {
                let rendered_expr = self.render(expr)?;
                let rendered_pattern = self.render(pattern)?;
                Ok(format!("{rendered_expr} LIKE {rendered_pattern}"))
            }

            DslExpr::Coalesce(exprs) => {
                let items: Result<Vec<String>, EmitError> =
                    exprs.iter().map(|e| self.render(e)).collect();
                let items = items?.join(", ");
                Ok(format!("COALESCE({items})"))
            }

            DslExpr::NullIf { expr, null_expr } => {
                let rendered_expr = self.render(expr)?;
                let rendered_null = self.render(null_expr)?;
                Ok(format!("NULLIF({rendered_expr}, {rendered_null})"))
            }

            DslExpr::DateTrunc { grain, expr } => {
                let rendered_expr = self.render(expr)?;
                Ok(format!("DATE_TRUNC('{grain}', {rendered_expr})"))
            }
        }
    }

    /// Render an `AggregateMeasure` into a SQL string (e.g., `SUM("amount")`).
    pub fn render_aggregate(&self, measure: &AggregateMeasure) -> Result<String, EmitError> {
        let inner = self.render(&measure.expr)?;
        let func_name = aggregation_sql_name(&measure.function);

        if measure.distinct || matches!(measure.function, Aggregation::CountDistinct) {
            Ok(format!("{func_name}(DISTINCT {inner})"))
        } else {
            Ok(format!("{func_name}({inner})"))
        }
    }
}

/// Map `Aggregation` enum to SQL function name.
fn aggregation_sql_name(agg: &Aggregation) -> &'static str {
    match agg {
        Aggregation::Sum => "SUM",
        Aggregation::Avg => "AVG",
        Aggregation::Count => "COUNT",
        Aggregation::CountDistinct => "COUNT",
        Aggregation::Min => "MIN",
        Aggregation::Max => "MAX",
    }
}
