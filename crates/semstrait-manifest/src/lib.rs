//! Manifest compiler and repository for semstrait semantic models.
//!
//! Compiles `SemanticModel` -> `CompiledManifest` via a 9-step pipeline.
//! Ships `InMemoryRepository` in v1; `FileSystemRepository` is v2.

pub mod acceleration;
mod catalog_builder;
pub mod catalog_snapshot;
mod compiled;
mod compiler;
mod error;
mod repository;
mod steps;

pub use acceleration::{
    AdjacencyIndex, CommonDataset, CompileDiagnostics, CompileWarning, ComplexDataKind,
    CoverageIndex, DataKind, DatasetBinding, DimensionIndex, FieldIndex, GrainMap, KindStrategy,
    MetricOrder, RelationshipGraph, ResolvedColumnMapping, ResolvedSource, SemanticInterface,
    SourceType, TemporalMapping,
};
pub use catalog_snapshot::{
    CatalogSnapshot, IcebergMetadata, PartitionField, PartitionTransform, ResolvedColumn,
    TableSnapshot,
};
pub use compiled::*;
pub use compiler::{CompileSource, ManifestCompiler};
pub use error::{CompileError, RepositoryError};
pub use repository::{FileSystemRepository, InMemoryRepository, Repository};

// Re-export model types needed by downstream crates (planner, tests).
// This avoids forcing planner to depend directly on semstrait-model.
pub use semstrait_model::{
    AdditivityType, AggregationConstraints, Cardinality, CategoricalDimension,
    ColumnMapping, ColumnMappingValue, DimensionConstraints, DimensionType, LiteralValue,
    JoinAssociativity, JoinColumnPair, JoinType, KindDatasetExtras, KindExtras, KindTypeSpec,
    MeasureConstraints, MetadataDimension, PathExtraction, PartitionExtraction,
    TemporalDimension, TemporalGrain, UnionMode,
};
