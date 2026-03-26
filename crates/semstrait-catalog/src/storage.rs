//! Storage provider abstraction for filesystem-level operations.
//!
//! Complements [`CatalogProvider`](super::CatalogProvider) which handles catalog-managed
//! tables. `StorageProvider` handles direct filesystem/object store access for:
//! - Glob expansion on local or S3 paths
//! - Schema extraction from Parquet footers / CSV headers

use async_trait::async_trait;
use semstrait_core::DataFormat;

use crate::error::CatalogError;
use crate::types::CatalogColumn;

/// Provides filesystem/object store access for source resolution.
///
/// Used by `resolve_sources` (compilation step 3.5) to expand glob patterns
/// in `StorageConfig.paths` and optionally extract schema from data files.
///
/// URI scheme determines the provider:
/// - `local://` or bare paths → [`LocalStorageProvider`](super::local_storage::LocalStorageProvider)
/// - `s3://` → [`S3StorageProvider`](super::s3_storage::S3StorageProvider)
/// - Tests → [`NullStorageProvider`](super::null_storage::NullStorageProvider)
#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Expand a glob pattern into concrete paths.
    ///
    /// The pattern may contain wildcards (`*`, `**`, `?`).
    /// Returns fully resolved paths in the same URI scheme as the input.
    ///
    /// # Errors
    /// - `CatalogError::Internal` if the pattern is malformed or I/O fails
    /// - `CatalogError::NotAvailable` if the storage backend is unreachable
    async fn expand_glob(&self, pattern: &str) -> Result<Vec<String>, CatalogError>;

    /// Read schema from a data file (best-effort).
    ///
    /// - Parquet: reads footer metadata for column names/types
    /// - CSV: reads header row (types inferred or all Utf8)
    /// - Iceberg: not applicable (use `CatalogProvider` instead)
    ///
    /// Returns `Ok(None)` if schema extraction is not supported for the
    /// given format or if the file cannot be read.
    async fn read_schema(
        &self,
        path: &str,
        format: DataFormat,
    ) -> Result<Option<Vec<CatalogColumn>>, CatalogError>;
}
