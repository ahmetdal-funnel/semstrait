//! Repository trait and InMemoryRepository implementation.
//!
//! v1 ships only `InMemoryRepository`. The `Repository` trait exists for
//! v2 extensibility (FileSystemRepository, ObjectStoreRepository).

use std::sync::{Arc, RwLock};

use crate::compiled::CompiledManifest;
use crate::error::RepositoryError;

/// Storage abstraction for `CompiledManifest`.
///
/// v1 ships `InMemoryRepository` only. FileSystem and ObjectStore are v2.
pub trait Repository: Send + Sync {
    /// Load the stored manifest.
    fn load(&self) -> Result<Arc<CompiledManifest>, RepositoryError>;

    /// Save a manifest, replacing any existing one.
    fn save(&self, manifest: &CompiledManifest) -> Result<(), RepositoryError>;
}

/// In-memory repository backed by `RwLock`.
///
/// Thread-safe for concurrent reads and single-writer updates.
#[derive(Debug)]
pub struct InMemoryRepository {
    inner: Arc<RwLock<Option<Arc<CompiledManifest>>>>,
}

impl InMemoryRepository {
    /// Create a new empty in-memory repository.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a repository pre-loaded with a manifest.
    pub fn with_manifest(manifest: CompiledManifest) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(Arc::new(manifest)))),
        }
    }
}

impl Default for InMemoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Repository for InMemoryRepository {
    fn load(&self) -> Result<Arc<CompiledManifest>, RepositoryError> {
        self.inner
            .read()
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?
            .clone()
            .ok_or(RepositoryError::NotFound)
    }

    fn save(&self, manifest: &CompiledManifest) -> Result<(), RepositoryError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
        *guard = Some(Arc::new(manifest.clone()));
        Ok(())
    }
}
