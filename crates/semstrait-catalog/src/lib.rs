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

use async_trait::async_trait;
use semstrait_core::{DataType, GlobPattern};
use std::fmt;
use thiserror::Error;

// ============================================================================
// Core Trait
// ============================================================================

/// Provides catalog metadata access for table discovery and schema retrieval.
///
/// This trait abstracts over different catalog implementations (Iceberg REST,
/// Unity Catalog, AWS Glue, Hive Metastore, etc.) and provides a uniform
/// interface for catalog operations.
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    /// Lists all table names matching a glob pattern in a given namespace.
    ///
    /// Called by `ManifestCompiler` during glob expansion. Implementations should
    /// return all tables that match the pattern according to standard glob rules.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace (schema/database) to search within
    /// * `pattern` - The glob pattern to match against table names
    ///
    /// # Returns
    ///
    /// A vector of [`TableRef`] instances for all matching tables.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::NamespaceNotFound`] if the namespace doesn't exist,
    /// or [`CatalogError::ConnectionError`] if the catalog is unreachable.
    async fn list_tables(
        &self,
        namespace: &str,
        pattern: &GlobPattern,
    ) -> Result<Vec<TableRef>, CatalogError>;

    /// Returns column schema for a specific table.
    ///
    /// # Arguments
    ///
    /// * `table` - The table reference to retrieve schema for
    ///
    /// # Returns
    ///
    /// A vector of [`CatalogColumn`] instances describing the table schema.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::TableNotFound`] if the table doesn't exist,
    /// or [`CatalogError::ConnectionError`] if the catalog is unreachable.
    async fn get_schema(&self, table: &TableRef) -> Result<Vec<CatalogColumn>, CatalogError>;

    /// Checks if a table exists in the catalog.
    ///
    /// # Arguments
    ///
    /// * `table` - The table reference to check
    ///
    /// # Returns
    ///
    /// `true` if the table exists, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::ConnectionError`] if the catalog is unreachable.
    async fn table_exists(&self, table: &TableRef) -> Result<bool, CatalogError>;
}

// ============================================================================
// Types
// ============================================================================

/// A reference to a table in the catalog hierarchy.
///
/// Represents a fully-qualified table reference with optional catalog name,
/// required namespace (schema/database), and table name.
///
/// # Examples
///
/// ```
/// use semstrait_catalog::TableRef;
///
/// // Simple table reference (namespace.table)
/// let table = TableRef {
///     catalog: None,
///     namespace: "sales".to_string(),
///     name: "orders".to_string(),
/// };
///
/// // Fully qualified (catalog.namespace.table)
/// let table = TableRef {
///     catalog: Some("prod".to_string()),
///     namespace: "sales".to_string(),
///     name: "orders".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableRef {
    /// Optional catalog name (e.g., "prod", "dev")
    pub catalog: Option<String>,
    /// Namespace/schema/database name (e.g., "sales", "analytics")
    pub namespace: String,
    /// Table name (e.g., "orders", "customers")
    pub name: String,
}

impl TableRef {
    /// Creates a new table reference without a catalog.
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            catalog: None,
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// Creates a new table reference with a catalog.
    pub fn with_catalog(
        catalog: impl Into<String>,
        namespace: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            catalog: Some(catalog.into()),
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// Returns the fully qualified table name.
    ///
    /// Format: `[catalog.]namespace.name`
    pub fn fully_qualified(&self) -> String {
        if let Some(catalog) = &self.catalog {
            format!("{}.{}.{}", catalog, self.namespace, self.name)
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }
}

impl fmt::Display for TableRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fully_qualified())
    }
}

/// Column metadata from a catalog.
///
/// Represents a single column's schema information including name, data type,
/// nullability, and optional documentation comment.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogColumn {
    /// Column name
    pub name: String,
    /// Column data type (using semstrait-core DataType)
    pub data_type: DataType,
    /// Whether the column can contain NULL values
    pub nullable: bool,
    /// Optional documentation comment
    pub comment: Option<String>,
}

impl CatalogColumn {
    /// Creates a new catalog column.
    pub fn new(name: impl Into<String>, data_type: DataType, nullable: bool) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
            comment: None,
        }
    }

    /// Creates a new catalog column with a comment.
    pub fn with_comment(
        name: impl Into<String>,
        data_type: DataType,
        nullable: bool,
        comment: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
            comment: Some(comment.into()),
        }
    }
}

// ============================================================================
// Errors
// ============================================================================

/// Errors that can occur during catalog operations.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// Catalog is not available or not configured
    #[error("catalog not available: {0}")]
    NotAvailable(String),

    /// Table was not found in the catalog
    #[error("table not found: {0}")]
    TableNotFound(String),

    /// Namespace/schema/database was not found
    #[error("namespace not found: {0}")]
    NamespaceNotFound(String),

    /// Connection error (network, timeout, authentication, etc.)
    #[error("connection error: {0}")]
    ConnectionError(String),

    /// Internal catalog error
    #[error("internal error: {0}")]
    Internal(String),
}

// ============================================================================
// NullCatalogProvider
// ============================================================================

/// No-op catalog provider for testing and stateless operations.
///
/// Returns empty lists for all queries and `Ok(false)` for existence checks.
///
/// # Use Cases
///
/// - Unit testing without a real catalog
/// - Stateless query execution with no glob patterns
/// - Prototyping and development
///
/// # Example
///
/// ```
/// use semstrait_catalog::{CatalogProvider, NullCatalogProvider, TableRef};
/// use semstrait_core::GlobPattern;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let catalog = NullCatalogProvider;
///
/// // Always returns empty
/// let tables = catalog.list_tables("any_namespace", &GlobPattern::new("*")).await?;
/// assert!(tables.is_empty());
///
/// // Always returns false
/// let table = TableRef::new("ns", "table");
/// let exists = catalog.table_exists(&table).await?;
/// assert!(!exists);
///
/// // Always returns empty schema
/// let schema = catalog.get_schema(&table).await?;
/// assert!(schema.is_empty());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct NullCatalogProvider;

#[async_trait]
impl CatalogProvider for NullCatalogProvider {
    async fn list_tables(
        &self,
        _namespace: &str,
        _pattern: &GlobPattern,
    ) -> Result<Vec<TableRef>, CatalogError> {
        Ok(Vec::new())
    }

    async fn get_schema(&self, _table: &TableRef) -> Result<Vec<CatalogColumn>, CatalogError> {
        Ok(Vec::new())
    }

    async fn table_exists(&self, _table: &TableRef) -> Result<bool, CatalogError> {
        Ok(false)
    }
}

// ============================================================================
// Feature-gated implementations
// ============================================================================

#[cfg(feature = "iceberg")]
pub mod iceberg;

#[cfg(feature = "iceberg")]
pub use iceberg::IcebergRestCatalog;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_ref_new() {
        let table = TableRef::new("sales", "orders");
        assert_eq!(table.catalog, None);
        assert_eq!(table.namespace, "sales");
        assert_eq!(table.name, "orders");
    }

    #[test]
    fn test_table_ref_with_catalog() {
        let table = TableRef::with_catalog("prod", "sales", "orders");
        assert_eq!(table.catalog, Some("prod".to_string()));
        assert_eq!(table.namespace, "sales");
        assert_eq!(table.name, "orders");
    }

    #[test]
    fn test_table_ref_fully_qualified() {
        let table = TableRef::new("sales", "orders");
        assert_eq!(table.fully_qualified(), "sales.orders");

        let table = TableRef::with_catalog("prod", "sales", "orders");
        assert_eq!(table.fully_qualified(), "prod.sales.orders");
    }

    #[test]
    fn test_table_ref_display() {
        let table = TableRef::new("sales", "orders");
        assert_eq!(format!("{}", table), "sales.orders");
    }

    #[test]
    fn test_catalog_column_new() {
        let col = CatalogColumn::new("id", DataType::Int64, false);
        assert_eq!(col.name, "id");
        assert_eq!(col.data_type, DataType::Int64);
        assert!(!col.nullable);
        assert_eq!(col.comment, None);
    }

    #[test]
    fn test_catalog_column_with_comment() {
        let col = CatalogColumn::with_comment(
            "customer_id",
            DataType::Int64,
            true,
            "Foreign key to customers table",
        );
        assert_eq!(col.name, "customer_id");
        assert_eq!(col.data_type, DataType::Int64);
        assert!(col.nullable);
        assert_eq!(
            col.comment,
            Some("Foreign key to customers table".to_string())
        );
    }

    #[tokio::test]
    async fn test_null_catalog_provider_list_tables() {
        let catalog = NullCatalogProvider;
        let pattern = GlobPattern::new("*");
        let result = catalog.list_tables("test_namespace", &pattern).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_null_catalog_provider_get_schema() {
        let catalog = NullCatalogProvider;
        let table = TableRef::new("test_namespace", "test_table");
        let result = catalog.get_schema(&table).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_null_catalog_provider_table_exists() {
        let catalog = NullCatalogProvider;
        let table = TableRef::new("test_namespace", "test_table");
        let result = catalog.table_exists(&table).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[cfg(feature = "iceberg")]
    #[test]
    fn test_iceberg_rest_catalog_creation() {
        let catalog = IcebergRestCatalog::new("https://catalog.example.com")
            .with_warehouse("my_warehouse")
            .with_bearer_token("test-token");
        assert_eq!(catalog.base_url(), "https://catalog.example.com");
    }

    #[test]
    fn test_catalog_error_display() {
        let err = CatalogError::TableNotFound("sales.orders".to_string());
        assert_eq!(format!("{}", err), "table not found: sales.orders");

        let err = CatalogError::NamespaceNotFound("sales".to_string());
        assert_eq!(format!("{}", err), "namespace not found: sales");

        let err = CatalogError::ConnectionError("timeout".to_string());
        assert_eq!(format!("{}", err), "connection error: timeout");
    }
}
