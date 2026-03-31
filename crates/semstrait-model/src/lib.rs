//! # semstrait-model
//!
//! YAML model parsing and reference resolution for Semstrait semantic models.
//!
//! This crate handles deserialization of YAML semantic model files into typed Rust structs
//! and resolves `ref:` entries to their inline definitions. It depends only on `semstrait-core`
//! and provides the foundational types used by `semstrait-manifest` for compilation.
//!
//! ## Key Types
//!
//! - [`SemanticModel`] - Root model containing data kinds and relationships
//! - [`DataKind`] - Unified entity enum: Dataset, Grainset, Unionset, Joinset
//! - [`SemanticInterface`] - Shared interface (dimensions, measures, metrics, filters)
//! - [`Dimension`], [`Measure`], [`Metric`] - Core semantic definitions
//!
//! ## Usage
//!
//! ```rust,ignore
//! use semstrait_model::{parse, resolve_refs};
//!
//! let yaml = std::fs::read_to_string("model.yaml")?;
//! let model = parse(&yaml)?;
//! let resolved = resolve_refs(model)?;
//! ```

mod error;
pub mod expr_block;
mod parse;
mod types;
pub mod catalogs;

pub use error::ModelError;
pub use parse::{parse, resolve_refs};
pub use types::*;
pub use catalogs::{CatalogsConfig, CatalogEntry, CatalogAuthMethod, SecretKeyMapping, parse_catalogs};
