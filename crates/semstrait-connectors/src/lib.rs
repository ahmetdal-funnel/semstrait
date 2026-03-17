//! Compute connector traits and feature-gated engine implementations.
//!
//! Defines the three-phase compute pipeline:
//! 1. `ComputeEmitter` — LogicalPlan → ComputePayload
//! 2. `ComputeAdapter` — ConsumerProfile + payload adaptation
//! 3. `ComputeConnector` — async execution → ComputeResult
//!
//! Engine implementations (DuckDB, DataFusion, Trino, Spark) are feature-gated.

mod traits;
mod payload;

#[cfg(feature = "datafusion")]
pub mod datafusion;

pub use traits::{ComputeEmitter, ComputeAdapter, ComputeConnector};
pub use payload::{
    ComputePayload, ComputeRequest, ComputeResult, ComputeResultData, PayloadKind,
    ExecutionStats, EmitError, AdaptError, ConnectorError,
};
