//! Compute connector traits and feature-gated engine implementations.
//!
//! Connectors receive `PlanArtifact` and execute against compute engines.

mod traits;
mod payload;

#[cfg(feature = "datafusion")]
pub mod datafusion;

#[cfg(feature = "duckdb")]
pub mod duckdb;

#[cfg(feature = "trino")]
pub mod trino;

#[cfg(feature = "spark")]
pub mod spark;

pub use traits::ComputeConnector;
pub use payload::{
    ComputeResult, ComputeResultData, ExecutionStats, ConnectorError,
};
