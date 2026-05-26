//! `semstrait-common` is the workspace's substrate crate. It owns the
//! diagnostic envelope, the byte-blob `io` transport, and the shape-only
//! constraint DSL. Stage-agnostic, expression-free, plan-free.
//!
//! Spec: `docs/design/apis/31_semstrait_common.md`. The `io` sub-spec is
//! `docs/design/apis/31b_semstrait_common_io.md`.

pub mod constraints;
pub mod diagnostic;

#[cfg(feature = "io")]
pub mod io;

pub use crate::constraints::{AggregationConstraints, DimensionConstraints, MeasureConstraints};
pub use crate::diagnostic::{
    Diagnose, Diagnostic, Diagnostics, Location, Severity, SourceId, Span,
};

#[cfg(feature = "io")]
pub use crate::io::{FromIoBytes, IntoIoBytes, IoErrorKind, Sink, Source};
