//! Polyglot-sql AST builder — programmatic SQL generation from PlanNode IR.
//!
//! Converts `PlanNode` and `DslExpr` trees into `polyglot_sql::Expression` AST
//! nodes via the fluent builder API, then renders to dialect-specific SQL via
//! `polyglot_sql::generate()`.

mod expr_builder;
mod plan_builder;

pub use expr_builder::ExprBuilder;
pub use plan_builder::PlanBuilder;
