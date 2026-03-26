//! SqlEmitter trait and AnsiSqlEmitter implementation.

use crate::dialect::SqlDialect;
use crate::error::EmitError;
use crate::expr_renderer::ExprSqlRenderer;
use semstrait_ir::{LogicalPlan, PlanNode, SortDirection};

/// Emits SQL strings from a `LogicalPlan`.
pub trait SqlEmitter: Send + Sync {
    /// Emit a complete SQL query string from a logical plan.
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError>;

    /// Return the dialect used by this emitter.
    fn dialect(&self) -> &dyn SqlDialect;
}

/// Default SQL emitter that works with any `SqlDialect`.
///
/// Walks the `PlanNode` tree recursively and generates SQL by
/// direct string building through the dialect.
pub struct AnsiSqlEmitter<D: SqlDialect> {
    dialect: D,
}

impl<D: SqlDialect> AnsiSqlEmitter<D> {
    pub fn new(dialect: D) -> Self {
        Self { dialect }
    }

    /// Emit SQL for a single PlanNode (recursive).
    fn emit_node(&self, node: &PlanNode) -> Result<String, EmitError> {
        let renderer = ExprSqlRenderer::new(&self.dialect);

        match node {
            PlanNode::Scan(scan) => {
                let cols = if scan.projection.is_empty() {
                    "*".to_string()
                } else {
                    scan.projection
                        .iter()
                        .map(|c| self.dialect.quote_identifier(c))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let table_ref = scan.table_name.split('.')
                    .map(|part| self.dialect.quote_identifier(part))
                    .collect::<Vec<_>>()
                    .join(".");
                Ok(format!("SELECT {cols} FROM {table_ref}"))
            }

            PlanNode::Filter(filter) => {
                let input_sql = self.emit_node(&filter.input)?;
                let predicate = renderer.render(&filter.predicate)?;
                Ok(format!("SELECT * FROM ({input_sql}) AS _f WHERE {predicate}"))
            }

            PlanNode::Project(project) => {
                let input_sql = self.emit_node(&project.input)?;
                let schema_fields = &project.meta.output_schema.fields;
                let mut select_parts = Vec::new();
                for (i, e) in project.expressions.iter().enumerate() {
                    let rendered = renderer.render(e)?;
                    if let Some(field) = schema_fields.get(i) {
                        let alias = self.dialect.quote_identifier(&field.name);
                        if rendered != alias {
                            select_parts.push(format!("{rendered} AS {alias}"));
                        } else {
                            select_parts.push(rendered);
                        }
                    } else {
                        select_parts.push(rendered);
                    }
                }
                let select_list = select_parts.join(", ");
                Ok(format!("SELECT {select_list} FROM ({input_sql}) AS _p"))
            }

            PlanNode::Aggregate(agg) => {
                let input_sql = self.emit_node(&agg.input)?;
                let schema_fields = &agg.meta.output_schema.fields;

                let group_exprs: Result<Vec<String>, EmitError> = agg
                    .group_by
                    .iter()
                    .map(|e| renderer.render(e))
                    .collect();
                let group_exprs = group_exprs?;

                let agg_exprs: Result<Vec<String>, EmitError> = agg
                    .aggregates
                    .iter()
                    .map(|m| renderer.render_aggregate(m))
                    .collect();
                let agg_exprs = agg_exprs?;

                // Alias group-by and aggregate expressions using schema field names.
                let num_groups = group_exprs.len();
                let mut select_parts = Vec::new();
                for (i, expr) in group_exprs.iter().enumerate() {
                    if let Some(field) = schema_fields.get(i) {
                        let alias = self.dialect.quote_identifier(&field.name);
                        if *expr != alias {
                            select_parts.push(format!("{expr} AS {alias}"));
                        } else {
                            select_parts.push(expr.clone());
                        }
                    } else {
                        select_parts.push(expr.clone());
                    }
                }
                for (i, expr) in agg_exprs.iter().enumerate() {
                    if let Some(field) = schema_fields.get(num_groups + i) {
                        let alias = self.dialect.quote_identifier(&field.name);
                        select_parts.push(format!("{expr} AS {alias}"));
                    } else {
                        select_parts.push(expr.clone());
                    }
                }
                let select_list = select_parts.join(", ");

                // GROUP BY uses the raw (unaliased) expressions.
                if group_exprs.is_empty() {
                    Ok(format!("SELECT {select_list} FROM ({input_sql}) AS _a"))
                } else {
                    let group_by = group_exprs.join(", ");
                    Ok(format!(
                        "SELECT {select_list} FROM ({input_sql}) AS _a GROUP BY {group_by}"
                    ))
                }
            }

            PlanNode::Join(join) => {
                let left_sql = self.emit_node(&join.left)?;
                let right_sql = self.emit_node(&join.right)?;
                let condition = renderer.render(&join.condition)?;

                let join_type = match join.join_type {
                    semstrait_ir::JoinType::Inner => "INNER JOIN",
                    semstrait_ir::JoinType::Left => "LEFT JOIN",
                    semstrait_ir::JoinType::Right => "RIGHT JOIN",
                    semstrait_ir::JoinType::Full => "FULL OUTER JOIN",
                };

                Ok(format!(
                    "SELECT * FROM ({left_sql}) AS _l {join_type} ({right_sql}) AS _r ON {condition}"
                ))
            }

            PlanNode::Union(union_node) => {
                if union_node.inputs.is_empty() {
                    return Err(EmitError::InvalidPlan(
                        "UNION requires at least one input".to_string(),
                    ));
                }
                let parts: Result<Vec<String>, EmitError> = union_node
                    .inputs
                    .iter()
                    .map(|n| self.emit_node(n))
                    .collect();
                let parts = parts?;
                let separator = if union_node.distinct {
                    " UNION DISTINCT "
                } else {
                    " UNION ALL "
                };
                Ok(parts.join(separator))
            }

            PlanNode::Sort(sort) => {
                let input_sql = self.emit_node(&sort.input)?;
                let keys: Result<Vec<String>, EmitError> = sort
                    .sort_keys
                    .iter()
                    .map(|k| {
                        let expr = renderer.render(&k.expr)?;
                        let dir = match k.direction {
                            SortDirection::Ascending => "ASC",
                            SortDirection::Descending => "DESC",
                        };
                        Ok(format!("{expr} {dir}"))
                    })
                    .collect();
                let keys = keys?;
                let order_by = keys.join(", ");
                Ok(format!("SELECT * FROM ({input_sql}) AS _s ORDER BY {order_by}"))
            }

            PlanNode::Fetch(fetch) => {
                let input_sql = self.emit_node(&fetch.input)?;
                let limit = self.dialect.limit_clause(fetch.count, fetch.offset);
                if limit.is_empty() {
                    Ok(format!("SELECT * FROM ({input_sql}) AS _t"))
                } else {
                    Ok(format!("SELECT * FROM ({input_sql}) AS _t {limit}"))
                }
            }
        }
    }
}

impl<D: SqlDialect + Send + Sync> SqlEmitter for AnsiSqlEmitter<D> {
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError> {
        self.emit_node(&plan.root)
    }

    fn dialect(&self) -> &dyn SqlDialect {
        &self.dialect
    }
}
