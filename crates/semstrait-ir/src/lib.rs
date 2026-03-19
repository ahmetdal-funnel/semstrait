//! Semstrait Intermediate Representation (IR)
//!
//! This crate defines the `LogicalPlan` IR with Substrait serialization support.
//! It depends only on `semstrait-core` and provides:
//!
//! - `PlanNode` enum with semantic annotations
//! - `LogicalPlan` wrapper
//! - `ExprConverter` for Expr ↔ Substrait Expression conversion
//! - `SubstraitSerializer` for bidirectional Substrait serialization
//!
//! Expressions use the unified `Expr` type from `semstrait-core`.

pub mod annotation;
pub mod error;
pub mod plan;
pub mod schema;
pub mod substrait;

pub use annotation::{AdditivityAnnotation, AggregateRole, FilterSource, SemAnnotation};
pub use error::{ConvertError, DeserializeError, SerializeError};
pub use plan::{
    AggNode, AggregateMeasure, FetchNode, FilterNode, JoinNode, JoinType, LogicalPlan, NodeMeta,
    PlanNode, PlannerWarning, ProjectNode, ScanNode, SortDirection, SortKey, SortNode, UnionNode,
};
// Re-export unified expression types from core (via plan::node re-exports)
pub use plan::node::{Aggregation, BinaryOp, Expr};
pub use schema::{Field, Schema};
pub use substrait::{ExprConverter, SubstraitSerializer};
