//! # semstrait
//!
//! Facade crate for semstrait — semantic model to compute plan compiler.
//!
//! Single entry point for library consumers. Provides a builder API
//! and re-exports key types from internal crates.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use semstrait::SemstraitBuilder;
//!
//! let sem = SemstraitBuilder::new()
//!     .with_manifest_yaml(yaml_string)
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

// Re-export adapter types
pub use semstrait_adapter::{AdaptError, EngineAdapter};

// Re-export catalog traits
pub use semstrait_catalog::{CatalogProvider, NullCatalogProvider, TableRef};

mod builder;

pub use builder::{BuildError, SemstraitBuilder, SemstraitInstance};
