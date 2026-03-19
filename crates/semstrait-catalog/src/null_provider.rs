//! No-op catalog provider for testing and stateless operations.

use async_trait::async_trait;
use semstrait_core::GlobPattern;

use crate::{CatalogColumn, CatalogError, CatalogProvider, TableRef};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
