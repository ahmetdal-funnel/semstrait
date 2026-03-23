//! Emitters (verb modules)
//!
//! Transforms a PlanNode into an output format.
//!
//! - `substrait` – Substrait protobuf Plan
//! - `sql` – ANSI SQL string

mod error;
mod sql;
mod substrait;

pub use error::EmitError;
pub use sql::emit_sql;
pub use substrait::emit_plan;
