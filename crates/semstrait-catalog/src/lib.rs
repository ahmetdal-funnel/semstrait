//! Catalog metadata access abstraction for Semstrait.
//!
//! This crate provides the [`CatalogProvider`] trait for abstracting catalog metadata access.
//! Used by `ManifestCompiler` for glob expansion and schema validation, and optionally by
//! `SemanticPlanner` for schema freshness checks.
//!
//! # Core Types
//!
//! - [`CatalogProvider`]: The main trait for catalog operations
//! - [`TableRef`]: A reference to a table (catalog, namespace, name)
//! - [`CatalogColumn`]: Column metadata with name, type, nullability, and optional comment
//! - [`CatalogError`]: Error types for catalog operations
//!
//! # Implementations
//!
//! - [`NullCatalogProvider`]: No-op implementation for testing and stateless operations
//! - `IcebergRestCatalog` (feature `iceberg`): Stub for Iceberg REST catalog (v1, not implemented)
//!
//! # Example
//!
//! ```
//! use semstrait_catalog::{CatalogProvider, NullCatalogProvider, TableRef};
//! use semstrait_core::GlobPattern;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let catalog = NullCatalogProvider;
//! let pattern = GlobPattern::new("*");
//! let tables = catalog.list_tables("my_namespace", &pattern).await?;
//! assert!(tables.is_empty());
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod null_provider;
pub mod types;

#[cfg(feature = "iceberg")]
pub mod iceberg;

#[cfg(feature = "unity")]
pub mod unity;

use async_trait::async_trait;
use semstrait_core::GlobPattern;

// Re-export key types for convenience.
pub use error::CatalogError;
pub use null_provider::NullCatalogProvider;
pub use types::{CatalogColumn, TableRef};

#[cfg(feature = "iceberg")]
pub use iceberg::IcebergRestCatalog;

#[cfg(feature = "unity")]
pub use unity::UnityCatalogProvider;

/// Provides catalog metadata access for table discovery and schema retrieval.
///
/// This trait abstracts over different catalog implementations (Iceberg REST,
/// Unity Catalog, AWS Glue, Hive Metastore, etc.) and provides a uniform
/// interface for catalog operations.
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    /// Lists all table names matching a glob pattern in a given namespace.
    async fn list_tables(
        &self,
        namespace: &str,
        pattern: &GlobPattern,
    ) -> Result<Vec<TableRef>, CatalogError>;

    /// Returns column schema for a specific table.
    async fn get_schema(&self, table: &TableRef) -> Result<Vec<CatalogColumn>, CatalogError>;

    /// Checks if a table exists in the catalog.
    async fn table_exists(&self, table: &TableRef) -> Result<bool, CatalogError>;
}
