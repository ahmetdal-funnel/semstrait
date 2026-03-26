//! No-op storage provider for testing and stateless operations.

use async_trait::async_trait;
use semstrait_core::DataFormat;

use crate::error::CatalogError;
use crate::storage::StorageProvider;
use crate::types::CatalogColumn;

/// No-op storage provider for testing and stateless operations.
///
/// Returns empty results for all operations. Use when source resolution
/// is not needed (e.g., unit tests, stateless compilation).
#[derive(Debug, Clone, Copy, Default)]
pub struct NullStorageProvider;

#[async_trait]
impl StorageProvider for NullStorageProvider {
    async fn expand_glob(&self, _pattern: &str) -> Result<Vec<String>, CatalogError> {
        Ok(Vec::new())
    }

    async fn read_schema(
        &self,
        _path: &str,
        _format: DataFormat,
    ) -> Result<Option<Vec<CatalogColumn>>, CatalogError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_null_storage_expand_glob() {
        let provider = NullStorageProvider;
        let result = provider.expand_glob("local:///data/*.parquet").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_null_storage_read_schema() {
        let provider = NullStorageProvider;
        let result = provider
            .read_schema("local:///data/file.parquet", DataFormat::Parquet)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
