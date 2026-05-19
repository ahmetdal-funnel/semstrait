//! Error types for semstrait-core.

use thiserror::Error;

/// Errors related to schema operations.
#[derive(Debug, Error, Clone)]
pub enum SchemaError {
    #[error("Column '{0}' not found in schema")]
    ColumnNotFound(String),

    #[error("Duplicate column name '{0}' in schema")]
    DuplicateColumn(String),

    #[error("Schema mismatch: {0}")]
    SchemaMismatch(String),

    #[error("Invalid ordinal {0} for schema with {1} columns")]
    InvalidOrdinal(u32, usize),
}

/// General errors for core types.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Schema error: {0}")]
    Schema(#[from] SchemaError),

    #[error("Invalid data type: {0}")]
    InvalidDataType(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
