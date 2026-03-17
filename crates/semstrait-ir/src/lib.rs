//! Semstrait Intermediate Representation (IR)
//!
//! This crate defines the `LogicalPlan` IR with Substrait serialization support.
//! It depends only on `semstrait-core` and provides:
//!
//! - `PlanNode` enum with semantic annotations
//! - `LogicalPlan` wrapper
//! - `ExprConverter` for DslExpr ↔ Substrait Expression conversion
//! - `SubstraitSerializer` for bidirectional Substrait serialization

pub mod annotation;
pub mod error;
pub mod expr_converter;
pub mod logical_plan;
pub mod node_meta;
pub mod plan_node;
pub mod schema;
pub mod serializer;

pub use annotation::{AdditivityAnnotation, AggregateRole, FilterSource, SemAnnotation};
pub use error::{ConvertError, DeserializeError, SerializeError};
pub use expr_converter::ExprConverter;
pub use logical_plan::LogicalPlan;
pub use node_meta::NodeMeta;
pub use plan_node::{
    AggNode, Aggregation, AggregateMeasure, FetchNode, FilterNode, JoinNode, JoinType, PlanNode,
    ProjectNode, ScanNode, SortDirection, SortNode, UnionNode, DslExpr, BinaryOp, SortKey,
};
pub use schema::{Field, Schema};
pub use serializer::SubstraitSerializer;
