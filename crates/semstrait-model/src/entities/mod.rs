//! Canonical entity types embedded in a `SemanticModel` — the
//! foundations-layer concerns ratified in `docs/design/foundations/18_entities.md`.

pub mod ai;
pub mod dimension;
pub mod filter;
pub mod keys;
pub mod mapping;
pub mod measure;
pub mod metric;
pub mod relationship;
pub mod semantic_interface;
pub mod temporal;

pub use ai::AiContext;
pub use dimension::{
    BucketBound, BucketSpec, BucketedDimensionBody, Dimension, DimensionEntry, DimensionRef,
    DimensionType, MetadataDimensionBody, MetadataSource, PartitionRef, PathTokenRef,
    TemporalDimensionBody,
};
pub use filter::{AggregationFilter, DataKindFilter};
pub use keys::{ForeignKeyDecl, KeyDecl, Keys};
pub use mapping::{
    LiteralValue, MetadataDimensionRecipe, MetadataExtraction, SemanticMapping,
    SemanticMappingValue,
};
pub use measure::{
    AdditivityType, AggregationType, Measure, MeasureEntry, MeasureRef, SemiAdditivity,
    SemiAdditivityStrategy,
};
pub use metric::{Metric, MetricEntry, MetricRef};
pub use relationship::{
    Cardinality, CrossFilter, Integrity, JoinKeyExprPair, JoinType, Optional, Relationship,
};
pub use semantic_interface::SemanticInterface;
pub use temporal::{
    EventsBody, ScdBody, ScdType, SnapshotBody, TemporalShape, TemporalShapeKind, TimeseriesBody,
};
