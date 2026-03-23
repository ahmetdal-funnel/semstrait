//! Logical plan types (noun module)
//!
//! Represents a relational algebra tree that can be translated to Substrait.

mod expr;
mod node;

pub use expr::{AggregateExpr, BinaryOperator, Column, Expr, Literal};
pub use node::{
    Aggregate, CrossJoin, Filter, Join, JoinType, LiteralValue, PlanNode, Project, ProjectExpr,
    Scan, Sort, SortDirection, SortKey, Union, VirtualTable,
};
