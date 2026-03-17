//! Error types for manifest compilation and repository operations.

use std::fmt;

/// Errors that can occur during manifest compilation.
#[derive(Debug)]
pub enum CompileError {
    /// YAML parse error (step 1)
    Parse(String),

    /// Reference resolution error (step 2)
    RefResolution(String),

    /// Glob expansion requires a catalog provider but none was configured (step 3)
    GlobRequiresCatalog { pattern: String, kind: String },

    /// Catalog error during glob expansion (step 3)
    CatalogError(String),

    /// Structural validation error (step 4)
    StructureValidation(Vec<String>),

    /// Column mapping validation error (step 5)
    MappingValidation(Vec<String>),

    /// Metric graph cycle detected (step 6)
    MetricCycle { cycle: Vec<String> },

    /// Metric graph depth exceeded (step 6)
    MetricDepthExceeded { metric: String, depth: usize, max_depth: usize },

    /// Relationship graph error (step 7)
    RelationshipGraph(String),

    /// Expression compilation error (step 8)
    ExprCompilation(Vec<String>),

    /// Raw SQL rejected (step 8)
    RawSqlRejected { entity: String, expr: String },

    /// IO error (reading files)
    Io(std::io::Error),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "parse error: {}", msg),
            Self::RefResolution(msg) => write!(f, "reference resolution error: {}", msg),
            Self::GlobRequiresCatalog { pattern, kind } => {
                write!(
                    f,
                    "glob pattern '{}' in kind '{}' requires a catalog provider",
                    pattern, kind
                )
            }
            Self::CatalogError(msg) => write!(f, "catalog error: {}", msg),
            Self::StructureValidation(errors) => {
                write!(f, "structure validation errors: {}", errors.join("; "))
            }
            Self::MappingValidation(errors) => {
                write!(f, "mapping validation errors: {}", errors.join("; "))
            }
            Self::MetricCycle { cycle } => {
                write!(f, "metric dependency cycle: {}", cycle.join(" -> "))
            }
            Self::MetricDepthExceeded { metric, depth, max_depth } => {
                write!(
                    f,
                    "metric '{}' has dependency depth {} exceeding maximum {}",
                    metric, depth, max_depth
                )
            }
            Self::RelationshipGraph(msg) => write!(f, "relationship graph error: {}", msg),
            Self::ExprCompilation(errors) => {
                write!(f, "expression compilation errors: {}", errors.join("; "))
            }
            Self::RawSqlRejected { entity, expr } => {
                write!(f, "raw SQL rejected for '{}': {}", entity, expr)
            }
            Self::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CompileError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<semstrait_model::ModelError> for CompileError {
    fn from(e: semstrait_model::ModelError) -> Self {
        match e {
            semstrait_model::ModelError::YamlParse(ye) => Self::Parse(ye.to_string()),
            semstrait_model::ModelError::RefResolution(msg) => Self::RefResolution(msg),
            semstrait_model::ModelError::Validation(msg) => {
                Self::StructureValidation(vec![msg])
            }
        }
    }
}

/// Errors that can occur during repository operations.
#[derive(Debug)]
pub enum RepositoryError {
    /// No manifest has been stored yet.
    NotFound,
    /// Serialization/deserialization error.
    Serialization(String),
    /// IO error.
    Io(std::io::Error),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "no compiled manifest found in repository"),
            Self::Serialization(msg) => write!(f, "serialization error: {}", msg),
            Self::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for RepositoryError {}
