//! Type definitions for semantic models.
//!
//! This module contains all the types that map to the YAML semantic model schema.
//! All types support serde serialization/deserialization.
//!
//! Organized into submodules by domain:
//! - `common` — shared types (DataType, AiContext, ColumnMapping, SemanticModel, SemanticInterface)
//! - `keys` — key and constraint types
//! - `temporal` — temporal configuration and grain types
//! - `dimension` — dimension types and variants
//! - `measure` — measure types, aggregation, additivity
//! - `metric` — metric types
//! - `storage` — storage, catalog, and partition types
//! - `relationship` — relationship and join types
//! - `data_kind` — DataKind enum, variant structs, extras, YAML kind types

pub mod common;
pub mod data_kind;
pub mod dimension;
pub mod keys;
pub mod measure;
pub mod metric;
pub mod relationship;
pub mod storage;
pub mod temporal;

// Re-export all public types at the module level for backward compatibility.
pub use common::*;
pub use data_kind::*;
pub use dimension::*;
pub use keys::*;
pub use measure::*;
pub use metric::*;
pub use relationship::*;
pub use storage::*;
pub use temporal::*;
