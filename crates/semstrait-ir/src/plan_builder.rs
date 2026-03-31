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
/// The `finalize_node` hook is called on every node after construction,
/// providing a cross-cutting point for annotation or rewriting.
pub trait PlanBuilder: Send + Sync {
    /// Post-construction hook called on every node.
    ///
    /// Default: identity (returns node unchanged).
    /// Engines can use this for annotation, validation, or rewriting.
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
}
