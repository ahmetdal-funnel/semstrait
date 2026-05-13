//! # semstrait
//!
//! Facade crate for semstrait — semantic model to compute plan compiler.
//!
//! Single entry point for library consumers. Provides a builder API
//! and re-exports key types from internal crates.
//!
//! TODO(refactor): re-align to new semstrait-model surface
//! (`32_semstrait_model.md`). Wraps `semstrait-manifest` /
//! `semstrait-planner` / `semstrait-api`, all of which require
//! migration per `docs/design/implementation/40_refactor_plan.md`.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use semstrait::SemstraitBuilder;
//!
//! let sem = SemstraitBuilder::new()
//!     .with_model(yaml_string)
//!     .build()
//!     .await?;
//!
//! let sql = sem.explain(&request)?;
//! println!("{}", sql);
//! ```

// Re-export core types
pub use semstrait_core::{
    DataType, Grain, Schema, SchemaColumn,
};

// Re-export IR types
pub use semstrait_ir::{LogicalPlan, PlanArtifact};

// Re-export manifest types
pub use semstrait_manifest::CompiledManifest;

/// Re-export generic I/O utilities for loading model YAML from local paths or S3.
pub mod io {
    pub use semstrait_manifest::io::load_text;
    pub use semstrait_manifest::io::IoError;
}

// Re-export planner types
pub use semstrait_planner::request::ResolvedQueryRequest;

// Re-export adapter types
pub use semstrait_adapter::{AdaptError, EngineAdapter};

// Re-export catalog traits
pub use semstrait_catalog::{CatalogProvider, CatalogRegistry, NullCatalogProvider, TableRef};

#[cfg(feature = "catalog-iceberg")]
pub use semstrait_catalog::IcebergRestCatalog;

mod builder;

pub use builder::{BuildError, SemstraitBuilder, SemstraitInstance};
