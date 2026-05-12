//! Shared author-surface newtypes for `semstrait-model`.
//!
//! Each newtype reserves a slot for future identifier-grammar tightening
//! (per `00 §4.1` / `11 §4`) without breaking callers — today they
//! deserialize transparently from / to bare `String` values.

pub mod names;

pub use names::{DataKindName, FilterName, SemanticsName};
