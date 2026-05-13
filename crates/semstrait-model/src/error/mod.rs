//! Stage-owned error rosters for the `semstrait-model` crate.
//!
//! Each stage owns a `Diagnose`-implementing kind enum (per `30 §5`):
//!
//! - [`parse::ParseErrorKind`] — YAML → `SemanticModel` (accumulating).
//! - [`validate::ValidateErrorKind`] — structural-precondition pass
//!   over a parsed `SemanticModel` (accumulating).
//! - [`catalogs::CatalogsParseErrorKind`] — `catalogs.yaml` parse
//!   (accumulating).
//! - [`build::ModelBuildErrorKind`] — fused kind for the loader
//!   composing parse + validate.

pub mod build;
pub mod catalogs;
pub mod parse;
pub mod validate;

pub use build::ModelBuildErrorKind;
pub use catalogs::CatalogsParseErrorKind;
pub use parse::ParseErrorKind;
pub use validate::ValidateErrorKind;
