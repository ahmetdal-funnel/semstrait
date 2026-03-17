//! Manifest compiler and repository for semstrait semantic models.
//!
//! Compiles `SemanticModel` -> `CompiledManifest` via a 9-step pipeline.
//! Ships `InMemoryRepository` in v1; `FileSystemRepository` is v2.

mod compiled;
mod compiler;
mod error;
mod repository;
mod steps;

pub use compiled::*;
pub use compiler::{CompileSource, ManifestCompiler};
pub use error::{CompileError, RepositoryError};
pub use repository::{InMemoryRepository, Repository};

// Re-export model types needed by downstream crates (planner, tests).
// This avoids forcing planner to depend directly on semstrait-model.
pub use semstrait_model::{
    AdditivityType, AggregationConstraints, CategoricalDimension, ColumnMappingValue,
    DimensionConstraints, DimensionType, KindDatasetExtras, MeasureConstraints,
};
