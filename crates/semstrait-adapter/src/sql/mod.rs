//! SQL dialect emission from LogicalPlan IR.
//!
//! Walks the PlanNode tree and emits SQL via direct string building through
//! dialect-specific renderers. One `SqlEmitter` implementation per dialect.
//!
//! # Architecture
//!
//! - `SqlDialect` trait: identifier quoting, date_trunc syntax, etc.
//! - `SqlEmitter` trait: walks `LogicalPlan` -> SQL string
//! - `ExprSqlRenderer`: converts `Expr` -> SQL fragment
//! - Dialect implementations: `AnsiDialect`, `DuckDbDialect`, `SparkDialect`
//! - `AnsiSqlEmitter`: the default emitter that works with any dialect

mod dialect;
mod emit_error;
mod emitter;
mod expr_renderer;

#[cfg(any(feature = "duckdb", feature = "spark"))]
mod polyglot;
#[cfg(any(feature = "duckdb", feature = "spark"))]
mod polyglot_emitter;

pub use dialect::{AnsiDialect, SqlDialect, TargetDialect};
#[cfg(feature = "duckdb")]
pub use dialect::DuckDbDialect;
#[cfg(feature = "spark")]
pub use dialect::SparkDialect;
pub use emit_error::EmitError;
pub use emitter::{AnsiSqlEmitter, SqlEmitter};
pub use expr_renderer::ExprSqlRenderer;
#[cfg(any(feature = "duckdb", feature = "spark"))]
pub use polyglot_emitter::PolyglotEmitter;

#[cfg(test)]
mod tests;
