//! SQL dialect emission from LogicalPlan IR.
//!
//! Walks the PlanNode tree and emits SQL via direct string building through
//! dialect-specific renderers. One `SqlEmitter` implementation per dialect.
//!
//! # Architecture
//!
//! - `SqlDialect` trait: identifier quoting, date_trunc syntax, etc.
//! - `SqlEmitter` trait: walks `LogicalPlan` → SQL string
//! - `DslExprSqlRenderer`: converts `DslExpr` → SQL fragment
//! - Dialect implementations: `AnsiDialect`, `DuckDbDialect`, `TrinoDialect`
//! - `AnsiSqlEmitter`: the default emitter that works with any dialect

mod dialect;
mod emitter;
mod error;
mod expr_renderer;

pub use dialect::{AnsiDialect, DuckDbDialect, SqlDialect, TrinoDialect};
pub use emitter::{AnsiSqlEmitter, SqlEmitter};
pub use error::EmitError;
pub use expr_renderer::DslExprSqlRenderer;

#[cfg(test)]
mod tests;
