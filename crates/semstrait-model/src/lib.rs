//! # semstrait-model
//!
//! Author-surface crate for the `semstrait` semantic-model layer.
//!
//! `semstrait-model` owns the in-memory [`SemanticModel`] type, the
//! per-DataKind hierarchy (`Dataset` / `Grainset` / `Unionset` /
//! `Joinset` and their nested counterparts), the canonical entity
//! types (`Dimension` / `Measure` / `Metric` / `Relationship` /
//! `TemporalShape` / …), and the two accumulating stages that
//! produce / validate models — [`parse`] and [`validate`].
//!
//! The full root-shape contract lives in
//! `docs/design/apis/32_semstrait_model.md`; the canonical entity
//! types in `docs/design/foundations/18_entities.md`; the catalogs
//! sibling file in `docs/design/apis/32b_catalogs_yaml.md`.
//!
//! ## Stages
//!
//! - [`parse`] — `&str` → `SemanticModel` (accumulating).
//! - [`validate`] — structural-precondition pass over a parsed
//!   `SemanticModel` (accumulating).
//! - [`SemanticModel::loader`] — fluent loader composing both,
//!   parameterised over a [`SourceFs`] strategy.
//! - [`parse_catalogs`] — `&str` → `CatalogsConfig` (accumulating).

// Diagnostic envelopes (Diagnostic<K> with location + notes + severity)
// are intentionally rich per `31 §6` / `30 §4`. The fail-fast Result paths
// (env-var substitution, Relationship::build, etc.) carry the same
// envelope by design — boxing would shift the cost from the rare error
// path onto every consumer of the public surface. The accumulating happy
// path uses Vec<Diagnostic<K>> and is unaffected by this lint.
#![allow(clippy::result_large_err)]

pub mod builder;
pub mod catalogs;
pub mod data_kind;
pub mod entities;
pub mod error;
pub mod expr_source;
pub mod loader;
pub mod model;
pub mod parse;
pub mod source_fs;
pub mod types;
pub mod validate;
pub(crate) mod yaml;

pub use catalogs::{CatalogAuthMethod, CatalogEntry, CatalogsConfig, SecretKeyMapping};
pub use data_kind::{
    AnyDataKindRef, CatalogRef, ComplexDataKind, ComplexDataKindRef, ComplexExtras, DataKind,
    DataKindBase, DataKindForm, DataKindVariant, Dataset, DatasetBody, ExtrasFlavor, Grainset,
    GrainsetBody, Joinset, JoinsetBody, LeafExtras, NestedDataKind, NestedDataKindRef,
    NestedDataset, NestedGrainset, NestedJoinset, NestedUnionset, PartitionDef, PublicDataKind,
    PublicDataKindRef, SimpleDataKind, SimpleDataKindRef, StorageConfig, StorageFormat, UnionMode,
    Unionset, UnionsetBody,
};
pub use entities::{
    AdditivityType, AggregationFilter, AggregationType, AiContext, BucketBound, BucketSpec,
    BucketedDimensionBody, Cardinality, CrossFilter, DataKindFilter, Dimension, DimensionEntry,
    DimensionRef, DimensionType, EventsBody, ForeignKeyDecl, Integrity, JoinKeyExprPair, JoinType,
    KeyDecl, Keys, LiteralValue, Measure, MeasureEntry, MeasureRef, MetadataDimensionBody,
    MetadataDimensionRecipe, MetadataExtraction, MetadataSource, Metric, MetricEntry, MetricRef,
    Optional, PartitionRef, PathTokenRef, Relationship, ScdBody,
    ScdType, SemanticInterface, SemanticMapping, SemanticMappingValue, SemiAdditivity,
    SemiAdditivityStrategy, SnapshotBody, TemporalDimensionBody, TemporalShape, TemporalShapeKind,
    TimeseriesBody,
};
pub use builder::{
    DatasetBuilder, GrainsetBuilder, JoinsetBuilder, NestedDatasetBuilder, NestedGrainsetBuilder,
    NestedJoinsetBuilder, NestedUnionsetBuilder, SemanticModelBuilder, UnionsetBuilder,
};
pub use expr_source::{parse_physical, parse_semantic, ExprSource, ParseError};
pub use error::{
    CatalogsParseErrorKind, ModelBuildErrorKind, ParseErrorKind, ValidateErrorKind,
};
pub use loader::SemanticModelLoader;
pub use model::SemanticModel;
pub use parse::{parse, parse_catalogs, parse_catalogs_with_source, parse_with_source};
pub use source_fs::{InMemoryFs, LocalFs, SourceFs};
pub use types::{DataKindName, FilterName, SemanticsName};
pub use validate::validate;
