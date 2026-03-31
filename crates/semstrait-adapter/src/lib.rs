//! Engine adapter layer for semstrait.
//!
//! Produces engine-appropriate artifacts (SQL or Substrait) from `LogicalPlan`.
//!
//! # Modules
//!
//! - `sql` — SQL dialect emission (dialect trait, emitter, expr renderer, polyglot)
//! - `engines` — Per-engine adapter implementations (DataFusion, DuckDB, Spark)

mod error;
mod traits;
pub mod sql;
mod engines;

pub use error::AdaptError;
pub use traits::EngineAdapter;

#[cfg(feature = "datafusion")]
pub use engines::DataFusionAdapter;
#[cfg(feature = "duckdb")]
pub use engines::DuckDbAdapter;
#[cfg(feature = "spark")]
pub use engines::SparkAdapter;
