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
//! let plan = sem.explain(QueryRequest {
//!     kind: "sales".into(),
//!     dimensions: vec!["region".into()],
//!     measures: vec!["revenue".into()],
//!     ..Default::default()
//! }).await?;
//! ```

// Re-export core types
pub use semstrait_core::{
    ConsumerProfile, DataType, Grain, Schema, SchemaColumn,
};

// Re-export IR types
pub use semstrait_ir::LogicalPlan;

// Re-export connector traits
pub use semstrait_connectors::{
    ComputeAdapter, ComputeConnector, ComputeEmitter, ComputePayload, ComputeResult,
};

// Re-export catalog traits
pub use semstrait_catalog::{CatalogProvider, NullCatalogProvider, TableRef};

mod builder;

pub use builder::{BuildError, SemstraitBuilder, SemstraitInstance};
