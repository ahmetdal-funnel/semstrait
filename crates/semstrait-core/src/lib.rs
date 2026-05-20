//! # semstrait-core
//!
//! Foundation crate for the semstrait workspace. Per spec
//! `[docs/design/apis/31_semstrait_core.md](../../../docs/design/apis/31_semstrait_core.md)`
//! (post-second-cascade, item Q), this crate owns the **non-expression
//! shared vocabulary** only. Expression-tree types, traits, support
//! enums, identifier carriers, and the narrow `ValidateError` /
//! `CompileError` live in `semstrait-ir`.
//!
//! ## Key Types
//!
//! - [`DataType`] — ANSI SQL logical data type system
//! - [`Schema`], [`SchemaColumn`] — ordinal-based column schema
//! - [`Grain`] — temporal granularity levels
//! - [`GlobPattern`] — glob pattern for catalog table matching
//! - [`Diagnostic`], [`Diagnose`] — typed diagnostic primitives shared
//!   across every stage in the pipeline
//! - Constraint types for measure/dimension validation

pub mod constraints;
pub mod data_type;
pub mod diagnostic;
pub mod error;
pub mod format;
pub mod grain;
pub mod schema;
pub mod types;

pub use constraints::{AggregationConstraints, DimensionConstraints, MeasureConstraints};
pub use data_type::DataType;
pub use diagnostic::{
    split_by_severity, Diagnose, Diagnostic, Diagnostics, Location, Severity, SourceId, Span,
};
pub use error::{CoreError, SchemaError};
pub use format::DataFormat;
pub use grain::Grain;
pub use schema::{Schema, SchemaColumn};
pub use types::GlobPattern;
