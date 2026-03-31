//! Engine adapter implementations.
//!
//! Each adapter implements `EngineAdapter` (plan -> artifact conversion)
//! for its target engine.
//!
//! - `ansi` — shared SQL base (dialect, emitter, expr renderer, polyglot AST)
//! - `datafusion` — produces Substrait plans
//! - `duckdb` — produces SQL with DuckDB dialect
//! - `spark` — produces SQL with Spark dialect

pub mod ansi;

#[cfg(feature = "datafusion")]
mod datafusion;
#[cfg(feature = "duckdb")]
mod duckdb;
#[cfg(feature = "spark")]
mod spark;

#[cfg(feature = "datafusion")]
pub use datafusion::DataFusionAdapter;
#[cfg(feature = "duckdb")]
pub use duckdb::DuckDbAdapter;
#[cfg(feature = "spark")]
pub use spark::SparkAdapter;
