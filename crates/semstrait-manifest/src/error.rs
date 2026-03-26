//! Error types for manifest compilation and repository operations.

/// Errors that can occur during manifest compilation.
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    /// YAML parse error (step 1)
    #[error("parse error: {0}")]
    Parse(String),

    /// Reference resolution error (step 2)
    #[error("reference resolution error: {0}")]
    RefResolution(String),

    /// Catalog/storage error during source resolution (step 3)
    #[error("catalog error: {0}")]
    CatalogError(String),

    /// Structural validation error (step 4)
    #[error("structure validation errors: {}", .0.join("; "))]
    StructureValidation(Vec<String>),

    /// Temporal type mismatch between kind extras and dataset extras (step 4.6)
    #[error("kind '{kind}', dataset '{dataset}': temporal type mismatch — kind has '{kind_type}' but dataset has '{dataset_type}'")]
    TemporalMismatch {
        kind: String,
        dataset: String,
        kind_type: String,
        dataset_type: String,
    },

    /// Column mapping validation error (step 5)
    #[error("mapping validation errors: {}", .0.join("; "))]
    MappingValidation(Vec<String>),

    /// Metric graph cycle detected (step 6)
    #[error("metric dependency cycle: {}", cycle.join(" -> "))]
    MetricCycle { cycle: Vec<String> },

    /// Metric graph depth exceeded (step 6)
    #[error("metric '{metric}' has dependency depth {depth} exceeding maximum {max_depth}")]
    MetricDepthExceeded {
        metric: String,
        depth: usize,
        max_depth: usize,
    },

    /// Relationship graph error (step 7)
    #[error("relationship graph error: {0}")]
    RelationshipGraph(String),

    /// Expression compilation error (step 8)
    #[error("expression compilation errors: {}", .0.join("; "))]
    ExprCompilation(Vec<String>),

    /// Raw SQL rejected (step 8)
    #[error("raw SQL rejected for '{entity}': {expr}")]
    RawSqlRejected { entity: String, expr: String },

    /// IO error (reading files)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<semstrait_model::ModelError> for CompileError {
    fn from(e: semstrait_model::ModelError) -> Self {
        match e {
            semstrait_model::ModelError::YamlParse(ye) => Self::Parse(ye.to_string()),
            semstrait_model::ModelError::RefResolution(msg) => Self::RefResolution(msg),
            semstrait_model::ModelError::Validation(msg) => {
                Self::StructureValidation(vec![msg])
            }
            semstrait_model::ModelError::EnvVar(msg) => Self::Parse(msg),
        }
    }
}

/// Errors that can occur during repository operations.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// No manifest has been stored yet.
    #[error("no compiled manifest found in repository")]
    NotFound,
    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
