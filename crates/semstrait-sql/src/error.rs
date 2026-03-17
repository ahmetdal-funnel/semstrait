//! Error types for SQL emission.

use thiserror::Error;

/// Errors that can occur during SQL emission.
#[derive(Debug, Error)]
pub enum EmitError {
    #[error("unsupported plan node for SQL emission: {0}")]
    UnsupportedNode(String),

    #[error("unsupported expression for SQL emission: {0}")]
    UnsupportedExpr(String),

    #[error("empty plan: no root node")]
    EmptyPlan,

    #[error("invalid plan structure: {0}")]
    InvalidPlan(String),
}
