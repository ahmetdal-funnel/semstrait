//! PlanNode → polyglot_sql Expression AST conversion.

use crate::error::EmitError;
use polyglot_sql::builder::{self, Expr};
use polyglot_sql::expressions::{
    Expression, Fetch, From, Identifier, Join, JoinKind, Literal, Select, Star, Subquery,
    TableRef, Union,
};
use semstrait_ir::{JoinType, LogicalPlan, PlanNode, SortDirection};

use super::ExprBuilder;

/// Converts `PlanNode` IR trees into polyglot-sql `Expression` AST.
pub struct PlanBuilder {
    expr: ExprBuilder,
}

impl PlanBuilder {
    pub fn new() -> Self {
        Self {
            expr: ExprBuilder,
        }
    }

    /// Convert a full `LogicalPlan` to a polyglot-sql `Expression`.
    pub fn build(&self, plan: &LogicalPlan) -> Result<Expression, EmitError> {
        self.build_node(&plan.root)
    }

    /// Build a single PlanNode recursively.
    fn build_node(&self, node: &PlanNode) -> Result<Expression, EmitError> {
        match node {
            PlanNode::Scan(scan) => self.build_scan(scan),
            PlanNode::Filter(filter) => self.build_filter(filter),
            PlanNode::Project(project) => self.build_project(project),
            PlanNode::Aggregate(agg) => self.build_aggregate(agg),
            PlanNode::Join(join) => self.build_join(join),
            PlanNode::Union(union_node) => self.build_union(union_node),
            PlanNode::Sort(sort) => self.build_sort(sort),
            PlanNode::Fetch(fetch) => self.build_fetch(fetch),
        }
    }

    // -- Scan ------------------------------------------------------------------

    fn build_scan(&self, scan: &semstrait_ir::ScanNode) -> Result<Expression, EmitError> {
        let select_exprs: Vec<Expr> = if scan.projection.is_empty() {
            vec![builder::star()]
        } else {
            scan.projection
                .iter()
                .map(|c| super::expr_builder::quoted_col(c))
                .collect()
        };
        Ok(builder::select(select_exprs)
            .from_expr(quoted_table(&scan.table_name))
            .build())
    }

    // -- Filter ----------------------------------------------------------------

    fn build_filter(&self, filter: &semstrait_ir::FilterNode) -> Result<Expression, EmitError> {
        let child = self.build_node(&filter.input)?;
        let predicate = self.expr.build(&filter.predicate)?;

        Ok(builder::select([builder::star()])
            .from_expr(wrap_subquery(child, "_f"))
            .where_(predicate)
            .build())
    }

    // -- Project ---------------------------------------------------------------

    fn build_project(
        &self,
        project: &semstrait_ir::ProjectNode,
    ) -> Result<Expression, EmitError> {
        let child = self.build_node(&project.input)?;

        let exprs: Result<Vec<Expr>, EmitError> = project
            .expressions
            .iter()
            .map(|e| self.expr.build(e))
            .collect();

        Ok(builder::select(exprs?)
            .from_expr(wrap_subquery(child, "_p"))
            .build())
    }

    // -- Aggregate -------------------------------------------------------------

    fn build_aggregate(&self, agg: &semstrait_ir::AggNode) -> Result<Expression, EmitError> {
        let child = self.build_node(&agg.input)?;
        let schema_fields = &agg.meta.output_schema.fields;

        // Build GROUP BY expressions
        let group_exprs: Result<Vec<Expr>, EmitError> =
            agg.group_by.iter().map(|e| self.expr.build(e)).collect();
        let group_exprs = group_exprs?;

        // Build aggregate expressions
        let agg_exprs: Result<Vec<Expr>, EmitError> = agg
            .aggregates
            .iter()
            .map(|m| self.expr.build_aggregate(m))
            .collect();
        let agg_exprs = agg_exprs?;

        // Build SELECT list with aliases from schema
        let num_groups = group_exprs.len();
        let mut select_list: Vec<Expr> = Vec::with_capacity(num_groups + agg_exprs.len());

        for (i, expr) in group_exprs.iter().enumerate() {
            if let Some(field) = schema_fields.get(i) {
                select_list.push(expr.clone().alias(&field.name));
            } else {
                select_list.push(expr.clone());
            }
        }
        for (i, expr) in agg_exprs.iter().enumerate() {
            if let Some(field) = schema_fields.get(num_groups + i) {
                select_list.push(expr.clone().alias(&field.name));
            } else {
                select_list.push(expr.clone());
            }
        }

        // Reuse already-built group expressions for GROUP BY (unaliased)
        let mut query = builder::select(select_list)
            .from_expr(wrap_subquery(child, "_a"));

        if !group_exprs.is_empty() {
            let group_by_refs: Vec<Expr> = group_exprs.into_iter().collect();
            query = query.group_by(group_by_refs);
        }

        Ok(query.build())
    }

    // -- Join ------------------------------------------------------------------

    fn build_join(&self, join: &semstrait_ir::JoinNode) -> Result<Expression, EmitError> {
        let left = self.build_node(&join.left)?;
        let right = self.build_node(&join.right)?;
        let on_expr = self.expr.build(&join.condition)?;

        // Map IR JoinType to polyglot JoinKind
        let kind = match join.join_type {
            JoinType::Inner => JoinKind::Inner,
            JoinType::Left => JoinKind::Left,
            JoinType::Right => JoinKind::Right,
            JoinType::Full => JoinKind::Full,
        };

        // Construct Select AST directly — the builder lacks full_join() and
        // join-on-subquery support, so we build the struct manually.
        let mut select = Select::new();
        select.expressions = vec![Expression::Star(Star {
            table: None,
            except: None,
            replace: None,
            rename: None,
            trailing_comments: Vec::new(),
            span: None,
        })];
        select.from = Some(From {
            expressions: vec![Expression::Subquery(Box::new(make_subquery(left, "_l")))],
        });
        select.joins = vec![Join {
            kind,
            this: Expression::Subquery(Box::new(make_subquery(right, "_r"))),
            on: Some(on_expr.into_inner()),
            using: Vec::new(),
            use_inner_keyword: false,
            use_outer_keyword: matches!(kind, JoinKind::Full),
            deferred_condition: false,
            join_hint: None,
            match_condition: None,
            pivots: Vec::new(),
            comments: Vec::new(),
            nesting_group: 0,
            directed: false,
        }];

        Ok(Expression::Select(Box::new(select)))
    }

    // -- Union -----------------------------------------------------------------

    fn build_union(
        &self,
        union_node: &semstrait_ir::UnionNode,
    ) -> Result<Expression, EmitError> {
        if union_node.inputs.len() < 2 {
            return Err(EmitError::InvalidPlan(
                "UNION requires at least 2 inputs".to_string(),
            ));
        }

        // Build all inputs as Expression ASTs
        let exprs: Result<Vec<Expression>, EmitError> = union_node
            .inputs
            .iter()
            .map(|n| self.build_node(n))
            .collect();
        let exprs = exprs?;

        // Left-fold into nested Union(Union(a, b), c) tree
        let mut acc = exprs.into_iter();
        let first = acc.next().unwrap();
        let result = acc.fold(first, |left, right| {
            Expression::Union(Box::new(Union {
                left,
                right,
                all: !union_node.distinct,
                distinct: union_node.distinct,
                with: None,
                order_by: None,
                limit: None,
                offset: None,
                distribute_by: None,
                sort_by: None,
                cluster_by: None,
                by_name: false,
                side: None,
                kind: None,
                corresponding: false,
                strict: false,
                on_columns: Vec::new(),
            }))
        });

        Ok(result)
    }

    // -- Sort ------------------------------------------------------------------

    fn build_sort(&self, sort: &semstrait_ir::SortNode) -> Result<Expression, EmitError> {
        let child = self.build_node(&sort.input)?;

        let sort_exprs: Result<Vec<Expr>, EmitError> = sort
            .sort_keys
            .iter()
            .map(|k| {
                let e = self.expr.build(&k.expr)?;
                match k.direction {
                    SortDirection::Ascending => Ok(e.asc()),
                    SortDirection::Descending => Ok(e.desc()),
                }
            })
            .collect();

        Ok(builder::select([builder::star()])
            .from_expr(wrap_subquery(child, "_s"))
            .order_by(sort_exprs?)
            .build())
    }

    // -- Fetch -----------------------------------------------------------------

    fn build_fetch(&self, fetch: &semstrait_ir::FetchNode) -> Result<Expression, EmitError> {
        if fetch.offset < 0 {
            return Err(EmitError::InvalidPlan("negative offset".to_string()));
        }
        if let Some(count) = fetch.count {
            if count < 0 {
                return Err(EmitError::InvalidPlan("negative fetch count".to_string()));
            }
        }

        let child = self.build_node(&fetch.input)?;

        let mut query = builder::select([builder::star()])
            .from_expr(wrap_subquery(child, "_t"));

        if fetch.offset > 0 {
            query = query.offset(fetch.offset as usize);
        }

        // Build the Select, then set .fetch for FETCH FIRST semantics.
        // polyglot-sql converts FETCH → LIMIT for dialects that prefer it
        // (DuckDB, Spark, Databricks, etc.) and keeps FETCH FIRST for others
        // (Trino, PostgreSQL, DataFusion, Oracle).
        let mut select = match query.build() {
            Expression::Select(s) => *s,
            other => {
                let mut s = Select::new();
                s.expressions = vec![other];
                s
            }
        };

        if let Some(count) = fetch.count {
            select.fetch = Some(Fetch {
                direction: "FIRST".to_string(),
                count: Some(Expression::Literal(Literal::Number(count.to_string()))),
                percent: false,
                rows: true,
                with_ties: false,
            });
        }

        Ok(Expression::Select(Box::new(select)))
    }
}

// -- Helpers -------------------------------------------------------------------

/// Wrap an `Expression` as a subquery `Expr` with the given alias.
fn wrap_subquery(expr: Expression, alias: &str) -> Expr {
    builder::subquery_expr(expr, alias)
}

/// Create a table reference `Expr` with quoted identifiers for the table name.
fn quoted_table(name: &str) -> Expr {
    let parts: Vec<&str> = name.split('.').collect();
    let mut tref = TableRef::new(parts.last().copied().unwrap_or(""));
    match parts.len() {
        3 => {
            tref.catalog = Some(Identifier::quoted(parts[0]));
            tref.schema = Some(Identifier::quoted(parts[1]));
            tref.name = Identifier::quoted(parts[2]);
        }
        2 => {
            tref.schema = Some(Identifier::quoted(parts[0]));
            tref.name = Identifier::quoted(parts[1]);
        }
        _ => {
            tref.name = Identifier::quoted(parts[0]);
        }
    }
    Expr(Expression::Table(Box::new(tref)))
}

/// Create a `Subquery` struct with the given alias.
fn make_subquery(expr: Expression, alias: &str) -> Subquery {
    Subquery {
        this: expr,
        alias: Some(Identifier::new(alias)),
        column_aliases: Vec::new(),
        order_by: None,
        limit: None,
        offset: None,
        distribute_by: None,
        sort_by: None,
        cluster_by: None,
        lateral: false,
        modifiers_inside: false,
        trailing_comments: Vec::new(),
        inferred_type: None,
    }
}
