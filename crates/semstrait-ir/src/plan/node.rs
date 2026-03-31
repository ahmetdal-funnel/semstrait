//! PlanNode types for the logical plan IR.
//!
//! Expressions use the unified `Expr` type from `semstrait-core`.

use super::meta::NodeMeta;
pub use semstrait_core::expr::{Aggregation, BinaryOp, Expr};

/// A node in the logical plan tree.
#[derive(Debug, Clone)]
pub enum PlanNode {
    Scan(ScanNode),
    Filter(FilterNode),
    Project(ProjectNode),
    Aggregate(AggNode),
    Join(JoinNode),
    Union(UnionNode),
    Sort(SortNode),
    Fetch(FetchNode),
}

impl PlanNode {
    pub fn meta(&self) -> &NodeMeta {
        match self {
            PlanNode::Scan(n) => &n.meta,
            PlanNode::Filter(n) => &n.meta,
            PlanNode::Project(n) => &n.meta,
            PlanNode::Aggregate(n) => &n.meta,
            PlanNode::Join(n) => &n.meta,
            PlanNode::Union(n) => &n.meta,
            PlanNode::Sort(n) => &n.meta,
            PlanNode::Fetch(n) => &n.meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut NodeMeta {
        match self {
            PlanNode::Scan(n) => &mut n.meta,
            PlanNode::Filter(n) => &mut n.meta,
            PlanNode::Project(n) => &mut n.meta,
            PlanNode::Aggregate(n) => &mut n.meta,
            PlanNode::Join(n) => &mut n.meta,
            PlanNode::Union(n) => &mut n.meta,
            PlanNode::Sort(n) => &mut n.meta,
            PlanNode::Fetch(n) => &mut n.meta,
        }
    }
}

/// Table scan node.
#[derive(Debug, Clone)]
pub struct ScanNode {
    pub meta: NodeMeta,
    pub table_name: String,
    /// Fully resolved physical location (populated after source resolution).
    pub location: Option<String>,
    /// Data format (populated after source resolution).
    pub format: Option<semstrait_core::DataFormat>,
    pub projection: Vec<String>,
}

/// Filter node (WHERE clause).
#[derive(Debug, Clone)]
pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub predicate: Expr,
}

/// Project node (SELECT with computed expressions).
#[derive(Debug, Clone)]
pub struct ProjectNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub expressions: Vec<Expr>,
}

/// Aggregate node (GROUP BY).
#[derive(Debug, Clone)]
pub struct AggNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub group_by: Vec<Expr>,
    pub aggregates: Vec<AggregateMeasure>,
}

/// An aggregate measure: function(expr).
#[derive(Debug, Clone)]
pub struct AggregateMeasure {
    pub function: Aggregation,
    pub expr: Expr,
    pub distinct: bool,
    /// Output data type of this aggregate, derived from aggregation function + input type.
    pub data_type: semstrait_core::DataType,
}

/// Join node.
#[derive(Debug, Clone)]
pub struct JoinNode {
    pub meta: NodeMeta,
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,
    pub join_type: JoinType,
    pub condition: Expr,
}

/// Join type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// Union node (UNION ALL or UNION DISTINCT).
#[derive(Debug, Clone)]
pub struct UnionNode {
    pub meta: NodeMeta,
    pub inputs: Vec<PlanNode>,
    pub distinct: bool,
}

/// Sort node (ORDER BY).
#[derive(Debug, Clone)]
pub struct SortNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub sort_keys: Vec<SortKey>,
}

/// A sort key with direction.
#[derive(Debug, Clone)]
pub struct SortKey {
    pub expr: Expr,
    pub direction: SortDirection,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Fetch node (LIMIT / OFFSET).
#[derive(Debug, Clone)]
pub struct FetchNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub count: Option<i64>,
    pub offset: i64,
}
