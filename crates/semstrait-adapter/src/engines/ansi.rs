//! ANSI SQL base engine — shared SQL infrastructure used by all SQL-emitting engines.
//!
//! Re-exports from `crate::sql` for engines that need SQL generation.
//! This module positions ANSI SQL as an engine alongside DataFusion, DuckDB, and Spark.

#[allow(unused_imports)]
pub use crate::sql::*;
