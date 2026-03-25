//! Compute result types for the connector pipeline.

use std::any::Any;
use std::time::Duration;

/// The result of executing a compute request.
#[derive(Debug)]
pub struct ComputeResult {
    /// Whether the result set is complete (false = partial/truncated).
    pub complete: bool,
    /// Execution statistics.
    pub stats: ExecutionStats,
    /// Result data (format depends on the connector).
    pub data: ComputeResultData,
}

/// Result data from compute execution.
pub enum ComputeResultData {
    /// No data (e.g., DDL, health check).
    Empty,
    /// Rows as JSON values — universal format for any connector.
    Json(Vec<serde_json::Value>),
    /// Engine-specific data (e.g., Arrow RecordBatches). Downcastable.
    Native(Box<dyn Any + Send + Sync>),
}

impl ComputeResultData {
    /// Downcast native data to a concrete type.
    /// Returns `None` if the data is not `Native` or the type doesn't match.
    pub fn as_native<T: 'static>(&self) -> Option<&T> {
        match self {
            Self::Native(boxed) => boxed.downcast_ref::<T>(),
            _ => None,
        }
    }
}

impl std::fmt::Debug for ComputeResultData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Empty"),
            Self::Json(rows) => f.debug_tuple("Json").field(&rows.len()).finish(),
            Self::Native(_) => write!(f, "Native(<opaque>)"),
        }
    }
}

/// Statistics from compute execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecutionStats {
    pub rows_returned: u64,
    pub execution_time: Option<Duration>,
    pub bytes_scanned: Option<u64>,
}

/// Error from `ComputeConnector::execute()`.
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("connection error: {0}")]
    Connection(String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("internal error: {0}")]
    Internal(String),
}
