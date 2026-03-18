//! API error types.

use thiserror::Error;

/// Errors from the SemstraitEngine.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("compile error: {0}")]
    Compile(#[from] semstrait_manifest::CompileError),

    #[error("plan error: {0}")]
    Plan(#[from] semstrait_planner::PlannerError),

    #[error("emit error: {0}")]
    Emit(#[from] semstrait_sql::EmitError),

    #[error("adapt error: {0}")]
    Adapt(#[from] semstrait_connectors::AdaptError),

    #[error("connector error: {0}")]
    Connector(#[from] semstrait_connectors::ConnectorError),

    #[error("not configured: {0}")]
    NotConfigured(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Errors from request parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("kind not found: {0}")]
    KindNotFound(String),

    #[error("dimension not found: {name} in kind {kind}")]
    DimensionNotFound { kind: String, name: String },

    #[error("measure not found: {name} in kind {kind}")]
    MeasureNotFound { kind: String, name: String },

    #[error("invalid grain: {0}")]
    InvalidGrain(String),

    #[error("invalid filter: {0}")]
    InvalidFilter(String),

    #[error("validation error: {0}")]
    Validation(String),
}
