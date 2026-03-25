//! Core catalog types: table references and column metadata.

use semstrait_core::DataType;
use std::collections::HashMap;
use std::fmt;

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

/// Extended table metadata response including partitions, snapshots, and location.
///
/// Returned by `CatalogProvider::load_table_metadata()`. Contains all metadata
/// needed for catalog resolution steps 10-13.
#[derive(Debug, Clone)]
pub struct TableMetadataResponse {
    /// Column schema.
    pub columns: Vec<CatalogColumn>,
    /// Partition spec fields (Iceberg-specific).
    pub partition_fields: Vec<CatalogPartitionField>,
    /// Current snapshot ID (Iceberg-specific).
    pub snapshot_id: Option<i64>,
    /// Table format version (e.g., Iceberg v1 or v2).
    pub format_version: Option<u32>,
    /// Physical table location (e.g., S3 URI).
    pub location: Option<String>,
    /// Table properties.
    pub properties: HashMap<String, String>,
}

/// A partition field from an Iceberg partition spec.
#[derive(Debug, Clone)]
pub struct CatalogPartitionField {
    /// Source column name (resolved from field ID).
    pub source_column: String,
    /// Partition transform string (e.g., "identity", "year", "month", "day", "hour", "bucket[N]").
    pub transform: String,
    /// Partition field name in the spec.
    pub name: String,
    /// Partition field ID.
    pub field_id: i32,
}

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
}
