//! Error types for model parsing and resolution.

use thiserror::Error;

/// Errors that can occur during model parsing and reference resolution.
#[derive(Debug, Error)]
pub enum ModelError {
    /// YAML deserialization error
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    /// Reference resolution error (unknown ref target)
    #[error("Reference resolution error: {0}")]
    RefResolution(String),

    /// Structural validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Environment variable resolution error
    #[error("Environment variable error: {0}")]
    EnvVar(String),
}
