//! Substrait serialization: Expr ↔ Substrait Expression, LogicalPlan ↔ Substrait Plan.

pub(crate) mod anchors;
pub mod expr_converter;
pub mod serializer;

pub use anchors::FunctionRegistry;
pub use expr_converter::ExprConverter;
pub use serializer::SubstraitSerializer;
