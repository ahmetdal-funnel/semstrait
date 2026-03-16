//! DSL expression system.
//!
//! Provides parsing and lowering for the semstrait DSL — the only expression
//! language accepted in v1.2 `expr:` fields. Raw SQL strings are rejected.
//!
//! # Pipeline
//!
//! ```text
//! DSL string → lexer::tokenize → parser::parse_dsl → DslExpr AST → lower::lower_expr → planner Expr
//! ```

mod ast;
mod lexer;
pub mod lower;
pub mod parser;
mod token;

pub use lower::{lower_aggregate, lower_expr};
pub use parser::parse_dsl;
