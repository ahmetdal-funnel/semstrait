//! # semstrait-core
//!
//! Foundation crate for the semstrait workspace.
//! Provides shared primitives with ZERO internal workspace dependencies.
//!
//! ## Key Types
//!
//! - [`DataType`] — Arrow-aligned data type system
//! - [`Schema`], [`SchemaColumn`] — ordinal-based column schema
//! - [`ConsumerProfile`] — capability flags shared by planner and connectors
//! - [`Grain`] — temporal granularity levels
//! - [`Expr`] — unified expression tree used across the entire pipeline
//! - [`GlobPattern`] — glob pattern for catalog table matching
//! - Constraint types for measure/dimension validation

pub mod error;
pub mod data_type;
pub mod schema;
pub mod consumer_profile;
pub mod engine_profile;
pub mod grain;
pub mod constraints;
pub mod expr;
pub mod types;

// Re-export key types for convenience
pub use error::{CoreError, SchemaError};
pub use data_type::{DataType, StructField};
pub use schema::{Schema, SchemaColumn};
pub use consumer_profile::{ConsumerProfile, SemiAdditiveStrategy};
pub use engine_profile::{EngineProfile, semi_additive_strategy};
pub use grain::Grain;
pub use constraints::{AggregationConstraints, DimensionConstraints, MeasureConstraints};
pub use expr::{Aggregation, BinaryOp, ColumnRef, Expr, Literal};
pub use types::GlobPattern;
