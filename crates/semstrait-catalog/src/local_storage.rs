//! Local filesystem storage provider.
//!
//! Handles `local://` URI scheme and bare paths for glob expansion
//! and schema extraction from local Parquet/CSV files.
//!
//! Requires the `local` feature flag.

use async_trait::async_trait;
use semstrait_core::DataFormat;

use crate::error::CatalogError;
use crate::storage::StorageProvider;
use crate::types::CatalogColumn;

/// Local filesystem storage provider.
///
/// Expands glob patterns against the local filesystem and extracts
/// schema from Parquet footers / CSV headers.
#[derive(Debug, Clone, Default)]
pub struct LocalStorageProvider;

#[async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn expand_glob(&self, pattern: &str) -> Result<Vec<String>, CatalogError> {
        let has_scheme = pattern.starts_with("local://");
        let path_pattern = pattern.strip_prefix("local://").unwrap_or(pattern);

        let entries = glob::glob(path_pattern).map_err(|e| {
            CatalogError::Internal(format!("invalid glob pattern '{}': {}", pattern, e))
        })?;

        let mut paths = Vec::new();
        for entry in entries {
            match entry {
                Ok(path) => {
                    let path_str = path.to_str().ok_or_else(|| {
                        CatalogError::Internal(format!(
                            "non-UTF-8 path in glob result: {}",
                            path.display()
                        ))
                    })?;
                    if has_scheme {
                        paths.push(format!("local://{}", path_str));
                    } else {
                        paths.push(path_str.to_string());
                    }
                }
                Err(e) => {
                    return Err(CatalogError::Internal(format!(
                        "glob entry error for pattern '{}': {}",
                        pattern, e
                    )));
                }
            }
        }

        Ok(paths)
    }

    async fn read_schema(
        &self,
        _path: &str,
        _format: DataFormat,
    ) -> Result<Option<Vec<CatalogColumn>>, CatalogError> {
        // TODO: Implement Parquet footer reading and CSV header parsing.
        // Requires parquet and csv crate dependencies.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_expand_glob_no_matches() {
        let provider = LocalStorageProvider;
        let result = provider
            .expand_glob("/nonexistent/path/*.parquet")
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_local_strip_scheme() {
        let provider = LocalStorageProvider;
        // local:// scheme with nonexistent path — should return empty, not error
        let result = provider
            .expand_glob("local:///nonexistent/path/*.parquet")
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_local_read_schema_stub() {
        let provider = LocalStorageProvider;
        let result = provider
            .read_schema("/some/file.parquet", DataFormat::Parquet)
            .await
            .unwrap();
        assert!(result.is_none()); // Stub returns None
    }
}
