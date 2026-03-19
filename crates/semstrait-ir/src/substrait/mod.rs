//! Substrait serialization: Expr ↔ Substrait Expression, LogicalPlan ↔ Substrait Plan.

mod anchors;
pub mod expr_converter;
pub mod serializer;

pub use expr_converter::ExprConverter;
pub use serializer::SubstraitSerializer;
