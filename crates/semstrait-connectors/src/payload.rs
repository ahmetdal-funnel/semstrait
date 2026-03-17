//! Compute payload types for the connector pipeline.

use std::any::Any;
use std::time::Duration;

use serde_json;

/// Kind of payload a connector can accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayloadKind {
    Sql,
    SubstraitPlan,
    NativePlan,
}

/// The output of `ComputeEmitter::emit()`.
pub enum ComputePayload {
    /// Dialect-specific SQL string.
    Sql(String),
    /// Serialized `substrait::proto::Plan` bytes.
    SubstraitPlan(Vec<u8>),
    /// Engine-specific plan object (e.g., DataFusion LogicalPlan).
    NativePlan(Box<dyn Any + Send + Sync>),
}

impl std::fmt::Debug for ComputePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(sql) => f.debug_tuple("Sql").field(&sql.len()).finish(),
            Self::SubstraitPlan(bytes) => {
                f.debug_tuple("SubstraitPlan").field(&bytes.len()).finish()
            }
            Self::NativePlan(_) => f.debug_tuple("NativePlan").field(&"<opaque>").finish(),
        }
    }
}

/// The adapted request ready for execution.
#[derive(Debug)]
pub struct ComputeRequest {
    pub payload: ComputePayload,
    pub timeout: Option<Duration>,
}

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
#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    pub rows_returned: u64,
    pub execution_time: Option<Duration>,
    pub bytes_scanned: Option<u64>,
}

/// Error from `ComputeEmitter::emit()`.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("unsupported plan node: {0}")]
    UnsupportedNode(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Error from `ComputeAdapter::adapt()`.
#[derive(Debug, thiserror::Error)]
pub enum AdaptError {
    #[error("unsupported payload kind: {0:?}")]
    UnsupportedPayload(PayloadKind),
    #[error("adaptation error: {0}")]
    Adaptation(String),
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
