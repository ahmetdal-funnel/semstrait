//! Selector error types

use std::fmt;

/// Errors that can occur during dataset selection
#[derive(Debug)]
pub enum SelectError {
    /// No dataset can serve the requested query
    NoFeasibleDataset { model: String, reason: String },
    /// Model has no datasets defined
    NoDatasetsInModel { model: String },
    /// Multiple grain sets can serve the query - ambiguous
    /// Use a cross-grain-set metric to disambiguate
    AmbiguousGrainSet {
        model: String,
        grain_sets: Vec<String>,
    },
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFeasibleDataset { model, reason } => {
                write!(
                    f,
                    "No dataset in model '{}' can serve the query: {}",
                    model, reason
                )
            }
            Self::NoDatasetsInModel { model } => {
                write!(f, "Model '{}' has no datasets defined", model)
            }
            Self::AmbiguousGrainSet { model, grain_sets } => {
                write!(
                    f,
                    "Query for model '{}' matches multiple grain sets: [{}]. Use a cross-grain-set metric to combine data from multiple sources.",
                    model,
                    grain_sets.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for SelectError {}
