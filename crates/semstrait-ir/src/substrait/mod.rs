//! Substrait serialization: Expr ↔ Substrait Expression, LogicalPlan ↔ Substrait Plan.

pub(crate) mod anchors;
pub mod expr_converter;
pub mod serializer;
mod type_mapping;

pub use anchors::FunctionRegistry;
pub use expr_converter::ExprConverter;
pub use serializer::SubstraitSerializer;
pub(crate) use type_mapping::{datatype_to_substrait, substrait_to_datatype};
