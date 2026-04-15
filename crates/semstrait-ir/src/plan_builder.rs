//! PlanBuilder trait — engine-specific plan node construction.
//!
//! Engines override only the node types that differ from the default.
//! `DefaultPlanBuilder` provides the standard behavior for all node types.

use crate::plan::node::{
    AggNode, AggregateMeasure, Expr, FetchNode, FilterNode, JoinNode, JoinType,
    PlanNode, ProjectNode, ScanNode, SortKey, SortNode, UnionNode,
};
use crate::plan::meta::NodeMeta;
use crate::schema::Schema;

/// Strategy trait for engine-specific plan node construction.
///
/// Each method has a default implementation that constructs the standard
/// `PlanNode` variant. Engine adapters override only the methods where
/// their plan construction diverges from the default.
///
/// Two hooks are provided:
///
/// - **`rewrite_expr`**: Called on every `Expr` field before node construction.
///   Engines override this to remap function names, restructure expressions,
///   or handle dedicated `Expr` variants differently.
///
/// - **`finalize_node`**: Called on every node after construction.
///   Engines override this for node-level rewriting (e.g., decomposing
///   operators the engine does not support, such as APPLY or LATERAL).
///
/// # Override Hazard
///
/// If you override a `build_*` method, you **must**:
/// 1. Call `self.rewrite_expr(expr)` on every `Expr` field before placing it in a `PlanNode`
/// 2. Call `self.finalize_node(node)` on the constructed `PlanNode` before returning
///
/// The default implementations handle both automatically.
pub trait PlanBuilder: Send + Sync {
    /// Rewrite an expression for the target engine.
    ///
    /// Called by default `build_*` methods on every `Expr` field before
    /// node construction. Engines override this to apply function remaps,
    /// structural rewrites, or dedicated `Expr` variant handling.
    ///
    /// Default: identity (returns expr unchanged).
    fn rewrite_expr(&self, expr: Expr) -> Expr {
        expr
    }

    /// Post-construction hook called on every node.
    ///
    /// Default: identity (returns node unchanged).
    /// Engines can use this for annotation, validation, or node-level
    /// rewriting (e.g., decomposing operators the engine does not support).
    fn finalize_node(&self, node: PlanNode) -> PlanNode {
        node
    }

    /// Build a table scan node.
    fn build_scan(
        &self,
        schema: Schema,
        table_name: String,
        location: Option<String>,
        format: Option<semstrait_core::DataFormat>,
        projection: Vec<String>,
    ) -> PlanNode {
        self.finalize_node(PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(schema),
            table_name,
            location,
            format,
            projection,
        }))
    }

    /// Build a filter node.
    fn build_filter(
        &self,
        schema: Schema,
        input: PlanNode,
        predicate: Expr,
    ) -> PlanNode {
        let predicate = self.rewrite_expr(predicate);
        self.finalize_node(PlanNode::Filter(FilterNode {
            meta: NodeMeta::new(schema),
            input: Box::new(input),
            predicate,
        }))
    }

    /// Build a projection node.
    fn build_project(
        &self,
        schema: Schema,
        input: PlanNode,
        expressions: Vec<Expr>,
    ) -> PlanNode {
        let expressions = expressions.into_iter().map(|e| self.rewrite_expr(e)).collect();
        self.finalize_node(PlanNode::Project(ProjectNode {
            meta: NodeMeta::new(schema),
            input: Box::new(input),
            expressions,
        }))
    }

    /// Build an aggregate node.
    fn build_aggregate(
        &self,
        schema: Schema,
        input: PlanNode,
        group_by: Vec<Expr>,
        aggregates: Vec<AggregateMeasure>,
    ) -> PlanNode {
        let group_by = group_by.into_iter().map(|e| self.rewrite_expr(e)).collect();
        let aggregates = aggregates
            .into_iter()
            .map(|mut am| {
                am.expr = self.rewrite_expr(am.expr);
                am
            })
            .collect();
        self.finalize_node(PlanNode::Aggregate(AggNode {
            meta: NodeMeta::new(schema),
            input: Box::new(input),
            group_by,
            aggregates,
        }))
    }

    /// Build a join node.
    fn build_join(
        &self,
        schema: Schema,
        left: PlanNode,
        right: PlanNode,
        join_type: JoinType,
        condition: Expr,
    ) -> PlanNode {
        let condition = self.rewrite_expr(condition);
        self.finalize_node(PlanNode::Join(JoinNode {
            meta: NodeMeta::new(schema),
            left: Box::new(left),
            right: Box::new(right),
            join_type,
            condition,
        }))
    }

    /// Build a union node.
    fn build_union(
        &self,
        schema: Schema,
        inputs: Vec<PlanNode>,
        distinct: bool,
    ) -> PlanNode {
        self.finalize_node(PlanNode::Union(UnionNode {
            meta: NodeMeta::new(schema),
            inputs,
            distinct,
        }))
    }

    /// Build a sort node.
    fn build_sort(
        &self,
        schema: Schema,
        input: PlanNode,
        sort_keys: Vec<SortKey>,
    ) -> PlanNode {
        let sort_keys = sort_keys
            .into_iter()
            .map(|mut sk| {
                sk.expr = self.rewrite_expr(sk.expr);
                sk
            })
            .collect();
        self.finalize_node(PlanNode::Sort(SortNode {
            meta: NodeMeta::new(schema),
            input: Box::new(input),
            sort_keys,
        }))
    }

    /// Build a fetch (LIMIT/OFFSET) node.
    fn build_fetch(
        &self,
        schema: Schema,
        input: PlanNode,
        count: Option<i64>,
        offset: i64,
    ) -> PlanNode {
        self.finalize_node(PlanNode::Fetch(FetchNode {
            meta: NodeMeta::new(schema),
            input: Box::new(input),
            count,
            offset,
        }))
    }
}

/// Default plan builder — standard node construction with no engine-specific behavior.
#[derive(Debug, Clone, Copy)]
pub struct DefaultPlanBuilder;

impl PlanBuilder for DefaultPlanBuilder {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Field;
    use semstrait_core::DataType;

    #[test]
    fn default_builder_builds_scan() {
        let builder = DefaultPlanBuilder;
        let schema = Schema::new(vec![
            Field::new("id", DataType::Integer),
            Field::new("name", DataType::String),
        ]);
        let node = builder.build_scan(
            schema,
            "users".to_string(),
            None,
            None,
            vec!["id".to_string(), "name".to_string()],
        );
        assert!(matches!(node, PlanNode::Scan(_)));
    }

    #[test]
    fn default_builder_builds_filter() {
        let builder = DefaultPlanBuilder;
        let schema = Schema::new(vec![Field::new("id", DataType::Integer)]);
        let scan = builder.build_scan(
            schema.clone(),
            "t".to_string(),
            None,
            None,
            vec!["id".to_string()],
        );
        let node = builder.build_filter(schema, scan, Expr::column("id"));
        assert!(matches!(node, PlanNode::Filter(_)));
    }

    #[test]
    fn default_builder_builds_aggregate() {
        let builder = DefaultPlanBuilder;
        let schema = Schema::new(vec![Field::new("count", DataType::Integer)]);
        let scan = builder.build_scan(
            schema.clone(),
            "t".to_string(),
            None,
            None,
            vec!["id".to_string()],
        );
        let node = builder.build_aggregate(
            schema,
            scan,
            vec![],
            vec![AggregateMeasure {
                function: semstrait_core::Aggregation::Count,
                expr: Expr::column("id"),
                distinct: false,
                data_type: DataType::Integer,
            }],
        );
        assert!(matches!(node, PlanNode::Aggregate(_)));
    }

    #[test]
    fn custom_builder_finalize_hook() {
        /// A test builder that tags every node's table_name with a prefix.
        struct PrefixBuilder;
        impl PlanBuilder for PrefixBuilder {
            fn finalize_node(&self, mut node: PlanNode) -> PlanNode {
                if let PlanNode::Scan(ref mut scan) = node {
                    scan.table_name = format!("catalog.{}", scan.table_name);
                }
                node
            }
        }

        let builder = PrefixBuilder;
        let schema = Schema::new(vec![Field::new("id", DataType::Integer)]);
        let node = builder.build_scan(
            schema,
            "users".to_string(),
            None,
            None,
            vec!["id".to_string()],
        );
        if let PlanNode::Scan(scan) = &node {
            assert_eq!(scan.table_name, "catalog.users");
        } else {
            panic!("expected Scan node");
        }
    }

    #[test]
    fn plan_builder_is_object_safe() {
        fn assert_object_safe(_: &dyn PlanBuilder) {}
        let builder = DefaultPlanBuilder;
        assert_object_safe(&builder);
    }

    #[test]
    fn rewrite_expr_applied_in_build_filter() {
        /// Renames "position" to "strpos" in FunctionCall nodes.
        struct RenameBuilder;
        impl PlanBuilder for RenameBuilder {
            fn rewrite_expr(&self, expr: Expr) -> Expr {
                expr.transform(&|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
                    if let Expr::FunctionCall(fc) = e {
                        if fc.name == "position" {
                            return Ok(Some(Expr::function_call("strpos", fc.args.clone())));
                        }
                    }
                    Ok(None)
                })
                .expect("infallible")
            }
        }

        let builder = RenameBuilder;
        let schema = Schema::new(vec![Field::new("id", DataType::Integer)]);
        let scan = builder.build_scan(
            schema.clone(),
            "t".to_string(),
            None,
            None,
            vec!["id".to_string()],
        );
        let predicate = Expr::function_call("position", vec![Expr::column("name"), Expr::string("x")]);
        let node = builder.build_filter(schema, scan, predicate);

        if let PlanNode::Filter(f) = &node {
            if let Expr::FunctionCall(fc) = &f.predicate {
                assert_eq!(fc.name, "strpos", "rewrite_expr should rename position to strpos");
            } else {
                panic!("expected FunctionCall in predicate");
            }
        } else {
            panic!("expected Filter node");
        }
    }

    #[test]
    fn rewrite_expr_applied_in_build_aggregate() {
        /// Renames "upper" to "UPPER_REWRITTEN" in FunctionCall nodes.
        struct UpperRewriter;
        impl PlanBuilder for UpperRewriter {
            fn rewrite_expr(&self, expr: Expr) -> Expr {
                expr.transform(&|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
                    if let Expr::FunctionCall(fc) = e {
                        if fc.name == "upper" {
                            return Ok(Some(Expr::function_call("UPPER_REWRITTEN", fc.args.clone())));
                        }
                    }
                    Ok(None)
                })
                .expect("infallible")
            }
        }

        let builder = UpperRewriter;
        let schema = Schema::new(vec![
            Field::new("region", DataType::String),
            Field::new("total", DataType::Number),
        ]);
        let scan = builder.build_scan(
            schema.clone(),
            "t".to_string(),
            None,
            None,
            vec!["region".to_string()],
        );

        let group_by = vec![Expr::function_call("upper", vec![Expr::column("region")])];
        let aggregates = vec![AggregateMeasure {
            function: semstrait_core::Aggregation::Sum,
            expr: Expr::function_call("upper", vec![Expr::column("region")]),
            distinct: false,
            data_type: DataType::Number,
        }];

        let node = builder.build_aggregate(schema, scan, group_by, aggregates);

        if let PlanNode::Aggregate(agg) = &node {
            // Check group_by rewritten
            if let Expr::FunctionCall(fc) = &agg.group_by[0] {
                assert_eq!(fc.name, "UPPER_REWRITTEN");
            } else {
                panic!("expected FunctionCall in group_by");
            }
            // Check aggregate expr rewritten
            if let Expr::FunctionCall(fc) = &agg.aggregates[0].expr {
                assert_eq!(fc.name, "UPPER_REWRITTEN");
            } else {
                panic!("expected FunctionCall in aggregate expr");
            }
        } else {
            panic!("expected Aggregate node");
        }
    }
}
