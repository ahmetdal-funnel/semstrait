//! Emitter errors

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("Unsupported plan node: {0}")]
    UnsupportedNode(String),
    #[error("Unsupported expression: {0}")]
    UnsupportedExpression(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Column not found in schema: {0}")]
    ColumnNotFound(String),
    #[error("Invalid plan: {0}")]
    InvalidPlan(String),
}
