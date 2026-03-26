//! S3 object store storage provider.
//!
//! Handles `s3://` URI scheme for glob expansion and schema extraction
//! from Parquet/CSV objects in S3-compatible storage.
//!
//! Requires the `s3` feature flag.

use async_trait::async_trait;
use semstrait_core::DataFormat;

use crate::error::CatalogError;
use crate::storage::StorageProvider;
use crate::types::CatalogColumn;

/// S3 object store storage provider.
///
/// Expands glob patterns against S3 object listings and extracts
/// schema from Parquet footers via range reads.
#[derive(Debug, Clone)]
pub struct S3StorageProvider {
    /// S3 region for object store operations.
    pub region: Option<String>,
}

impl S3StorageProvider {
    pub fn new(region: Option<String>) -> Self {
        Self { region }
    }
}

#[async_trait]
impl StorageProvider for S3StorageProvider {
    async fn expand_glob(&self, pattern: &str) -> Result<Vec<String>, CatalogError> {
        if !pattern.starts_with("s3://") {
            return Err(CatalogError::Internal(format!(
                "S3StorageProvider requires s3:// URI scheme, got: {}",
                pattern
            )));
        }

        // TODO: Implement S3 object listing with prefix/glob matching.
        // Requires object_store crate dependency.
        tracing::warn!("S3 glob expansion not yet implemented for: {}", pattern);
        Ok(Vec::new())
    }

    async fn read_schema(
        &self,
        _path: &str,
        _format: DataFormat,
    ) -> Result<Option<Vec<CatalogColumn>>, CatalogError> {
        // TODO: Implement S3 Parquet footer reading via range requests.
        // Requires object_store + parquet crate dependencies.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_s3_rejects_non_s3_scheme() {
        let provider = S3StorageProvider::new(Some("us-east-1".to_string()));
        let result = provider.expand_glob("/local/path/*.parquet").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_s3_expand_glob_stub() {
        let provider = S3StorageProvider::new(Some("eu-west-1".to_string()));
        let result = provider.expand_glob("s3://bucket/prefix/*.parquet").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty()); // Stub returns empty
    }

    #[tokio::test]
    async fn test_s3_read_schema_stub() {
        let provider = S3StorageProvider::new(None);
        let result = provider
            .read_schema("s3://bucket/file.parquet", DataFormat::Parquet)
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
