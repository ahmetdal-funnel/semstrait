//! Manifest compiler and repository for semstrait semantic models.
//!
//! Compiles `SemanticModel` -> `CompiledManifest` via a 9-step pipeline.
//! Ships `InMemoryRepository` in v1; `FileSystemRepository` is v2.

pub mod acceleration;
pub mod catalog_snapshot;
mod compiled;
mod compiler;
mod error;
pub mod function_registry;
pub mod io;
mod repository;
mod steps;

pub use acceleration::{
    AdjacencyIndex, CompileDiagnostics, CompileWarning, CompiledDataKind, CompiledSimpleKind,
    CompiledGrainsetKind, CompiledInterface, CompiledJoinsetKind,
    CompiledUnionsetKind, CoverageIndex, DatasetBinding, DimensionIndex, FieldIndex, FieldType,
    GrainMap, MetricOrder, RelationshipGraph, ResolvedColumnMapping,
    ResolvedSource, SemanticEdge, SemanticGraph, SemanticNode, SourceType, TemporalMapping,
};
pub use catalog_snapshot::{
    CatalogSnapshot, IcebergMetadata, PartitionField, PartitionTransform, ResolvedColumn,
    TableSnapshot,
};
pub use compiled::*;
pub use compiler::{CompileSource, ManifestCompiler};
pub use error::{CompileError, RepositoryError};
pub use io::IoError;
pub use repository::{FileSystemRepository, InMemoryRepository, Repository};

// Re-export model types needed by downstream crates (planner, tests).
// This avoids forcing planner to depend directly on semstrait-model.
pub use semstrait_model::{
    AdditivityType, AggregationConstraints, Cardinality, CategoricalDimension,
    ColumnMapping, ColumnMappingValue, DimensionConstraints, DimensionType, LiteralValue,
    JoinAssociativity, JoinColumnPair, JoinType,
    InlineDatasetExtras, ComplexExtras,
    MeasureConstraints, MetadataDimension, PathExtraction, PartitionExtraction,
    TemporalDimension, TemporalGrain, UnionMode,
};
