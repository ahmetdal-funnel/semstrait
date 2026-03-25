//! Engine adapter layer for semstrait.
//!
//! Produces engine-appropriate artifacts (SQL or Substrait) from `LogicalPlan`
//! based on engine capability profiles.

mod error;
mod traits;

#[cfg(feature = "datafusion")]
mod datafusion;
#[cfg(feature = "duckdb")]
mod duckdb;
#[cfg(feature = "trino")]
mod trino;
#[cfg(feature = "spark")]
mod spark;

pub use error::AdaptError;
pub use traits::EngineAdapter;

// Re-export EngineProfile from core for convenience
pub use semstrait_core::EngineProfile;

use semstrait_core::ConsumerProfile;

/// Build a `ConsumerProfile` from an `EngineAdapter` reference.
///
/// Bridges the adapter's capability flags into a `ConsumerProfile` that the
/// planner accepts as `Arc<dyn EngineProfile>`.
pub fn profile_from_adapter(adapter: &dyn EngineAdapter) -> ConsumerProfile {
    ConsumerProfile {
        supports_window_functions: adapter.supports_window_functions(),
        supports_full_outer_join: adapter.supports_full_outer_join(),
        supports_cte: adapter.supports_cte(),
        supports_fetch_rel: adapter.supports_fetch_rel(),
        max_join_depth: adapter.max_join_depth(),
        substrait_function_uris: Default::default(),
    }
}

#[cfg(feature = "datafusion")]
pub use datafusion::DataFusionAdapter;
#[cfg(feature = "duckdb")]
pub use duckdb::DuckDbAdapter;
#[cfg(feature = "trino")]
pub use trino::TrinoAdapter;
#[cfg(feature = "spark")]
pub use spark::SparkAdapter;
