//! PlanNode types for the logical plan IR

use crate::node_meta::NodeMeta;
use serde::{Deserialize, Serialize};

/// Aggregation functions for measures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregation {
    Sum,
    Avg,
    Count,
    CountDistinct,
    Min,
    Max,
}

/// A node in the logical plan tree
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
    /// Get the metadata for this node
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

    /// Get mutable metadata for this node
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

/// Table scan node
#[derive(Debug, Clone)]
pub struct ScanNode {
    pub meta: NodeMeta,
    /// Table name (possibly qualified: schema.table)
    pub table_name: String,
    /// Columns to project from the table (in ordinal order)
    pub projection: Vec<String>,
}

/// Filter node (WHERE clause)
#[derive(Debug, Clone)]
pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Filter predicate expression
    pub predicate: DslExpr,
}

/// Project node (SELECT with computed expressions)
#[derive(Debug, Clone)]
pub struct ProjectNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Expressions to compute (one per output column)
    pub expressions: Vec<DslExpr>,
}

/// Aggregate node (GROUP BY)
#[derive(Debug, Clone)]
pub struct AggNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// GROUP BY expressions
    pub group_by: Vec<DslExpr>,
    /// Aggregate measures
    pub aggregates: Vec<AggregateMeasure>,
}

/// An aggregate measure: function(expr)
#[derive(Debug, Clone)]
pub struct AggregateMeasure {
    pub function: Aggregation,
    pub expr: DslExpr,
    pub distinct: bool,
}

/// Join node
#[derive(Debug, Clone)]
pub struct JoinNode {
    pub meta: NodeMeta,
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,
    pub join_type: JoinType,
    /// Join condition expression
    pub condition: DslExpr,
}

/// Join type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// Union node (UNION ALL or UNION DISTINCT)
#[derive(Debug, Clone)]
pub struct UnionNode {
    pub meta: NodeMeta,
    /// Input branches (must have compatible schemas)
    pub inputs: Vec<PlanNode>,
    /// If true, emit UNION DISTINCT (deduplicate rows); otherwise UNION ALL.
    pub distinct: bool,
}

/// Sort node (ORDER BY)
#[derive(Debug, Clone)]
pub struct SortNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Sort keys
    pub sort_keys: Vec<SortKey>,
}

/// A sort key with direction
#[derive(Debug, Clone)]
pub struct SortKey {
    /// Expression to sort by
    pub expr: DslExpr,
    pub direction: SortDirection,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Fetch node (LIMIT / OFFSET)
#[derive(Debug, Clone)]
pub struct FetchNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Number of rows to return (LIMIT)
    pub count: Option<i64>,
    /// Number of rows to skip (OFFSET)
    pub offset: i64,
}

// =============================================================================
// DslExpr - simplified expression type for IR
// =============================================================================

/// Expression in the IR (simplified from semstrait-core's internal types)
#[derive(Debug, Clone, PartialEq)]
pub enum DslExpr {
    /// Column reference by name
    Column { name: String, qualifier: Option<String> },
    /// Numeric literal
    Number(f64),
    /// String literal
    StringLit(String),
    /// Boolean literal
    Bool(bool),
    /// NULL literal
    Null,
    /// Binary operation
    BinaryOp {
        left: Box<DslExpr>,
        op: BinaryOp,
        right: Box<DslExpr>,
    },
    /// Function call (including aggregates when not in AggregateMeasure)
    FunctionCall {
        name: String,
        args: Vec<DslExpr>,
        distinct: bool,
    },
    /// Unary negation
    Negate(Box<DslExpr>),
    /// Logical NOT
    Not(Box<DslExpr>),
    /// CASE expression
    Case {
        when_then: Vec<(DslExpr, DslExpr)>,
        else_expr: Option<Box<DslExpr>>,
    },
    /// expr IN (list) / expr NOT IN (list)
    InList {
        expr: Box<DslExpr>,
        list: Vec<DslExpr>,
        negated: bool,
    },
    /// expr BETWEEN low AND high
    Between {
        expr: Box<DslExpr>,
        low: Box<DslExpr>,
        high: Box<DslExpr>,
        negated: bool,
    },
    /// expr LIKE pattern
    Like {
        expr: Box<DslExpr>,
        pattern: Box<DslExpr>,
    },
    /// expr IS NULL
    IsNull(Box<DslExpr>),
    /// expr IS NOT NULL
    IsNotNull(Box<DslExpr>),
    /// COALESCE(expr1, expr2, ...)
    Coalesce(Vec<DslExpr>),
    /// NULLIF(expr1, expr2)
    NullIf {
        expr: Box<DslExpr>,
        null_expr: Box<DslExpr>,
    },
    /// DATE_TRUNC('grain', expr)
    DateTrunc {
        grain: String,
        expr: Box<DslExpr>,
    },
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Safe division: returns NULL when divisor is zero
    SafeDivide,
    // Comparison
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    // Logical
    And,
    Or,
}

impl BinaryOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::SafeDivide => "/", // rendered specially by SQL emitter
            BinaryOp::Eq => "=",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "AND",
            BinaryOp::Or => "OR",
        }
    }
}
