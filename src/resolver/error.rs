use std::fmt;

/// Errors that can occur during query resolution
#[derive(Debug)]
pub enum ResolveError {
    ModelNotFound(String),
    DimensionNotFound(String),
    GrainSetNotFound(String),
    AttributeNotFound { dimension: String, attribute: String },
    MeasureNotFound(String),
    MetricNotFound(String),
    InvalidAttributeFormat(String),
    InvalidQuery(String),
    /// A virtual _dataset metadata attribute was requested but not available
    MetaAttributeNotFound(String),
    /// A virtual _dataset metadata attribute requires a value that isn't set
    /// (e.g., dataset.uuid when uuid is None)
    MetaAttributeNotSet { attribute: String, reason: String },
    /// Container path is invalid (not found, or path points to a union group instead of a leaf)
    ContainerPathNotFound { path: String, reason: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::ModelNotFound(name) => write!(f, "Model '{}' not found", name),
            ResolveError::DimensionNotFound(name) => write!(f, "Dimension '{}' not found", name),
            ResolveError::GrainSetNotFound(name) => write!(f, "Grain set '{}' not found", name),
            ResolveError::AttributeNotFound { dimension, attribute } => {
                write!(f, "Attribute '{}' not found in dimension '{}'", attribute, dimension)
            }
            ResolveError::MeasureNotFound(name) => write!(f, "Measure '{}' not found", name),
            ResolveError::MetricNotFound(name) => write!(f, "Metric '{}' not found", name),
            ResolveError::InvalidAttributeFormat(s) => {
                write!(f, "Invalid attribute format '{}', expected 'dimension.attribute' (conformed) or 'path.dimension.attribute' (path reflects container levels)", s)
            }
            ResolveError::InvalidQuery(msg) => write!(f, "Invalid query: {}", msg),
            ResolveError::MetaAttributeNotFound(name) => {
                write!(f, "Unknown _dataset attribute '{}'. Available: model, namespace, path, dataset, uuid, or any dataset property key", name)
            }
            ResolveError::MetaAttributeNotSet { attribute, reason } => {
                write!(f, "_dataset.{} is not available: {}", attribute, reason)
            }
            ResolveError::ContainerPathNotFound { path, reason } => {
                write!(f, "Container path '{}': {}", path, reason)
            }
        }
    }
}

impl std::error::Error for ResolveError {}
