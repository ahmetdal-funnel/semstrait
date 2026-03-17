//! Error types for IR operations

use thiserror::Error;

/// Error during expression conversion
#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("Column not found in schema: {0}")]
    ColumnNotFound(String),

    #[error("Unsupported expression type: {0}")]
    UnsupportedExpression(String),

    #[error("Invalid expression: {0}")]
    InvalidExpression(String),

    #[error("Type mismatch: {0}")]
    TypeMismatch(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Error during Substrait serialization
#[derive(Debug, Error)]
pub enum SerializeError {
    #[error("Failed to convert expression: {0}")]
    ExpressionConversion(#[from] ConvertError),

    #[error("Invalid plan structure: {0}")]
    InvalidPlan(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unsupported node type: {0}")]
    UnsupportedNode(String),
}

/// Error during Substrait deserialization
#[derive(Debug, Error)]
pub enum DeserializeError {
    #[error("Failed to convert expression: {0}")]
    ExpressionConversion(#[from] ConvertError),

    #[error("Invalid Substrait plan: {0}")]
    InvalidPlan(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unsupported Substrait construct: {0}")]
    UnsupportedConstruct(String),

    #[error("Schema mismatch: {0}")]
    SchemaMismatch(String),
}
