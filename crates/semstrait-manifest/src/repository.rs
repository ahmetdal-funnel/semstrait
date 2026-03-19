//! Repository trait and implementations.
//!
//! Provides `InMemoryRepository` and `FileSystemRepository`.
//! The `Repository` trait exists for extensibility (ObjectStoreRepository, etc.).

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::compiled::CompiledManifest;
use crate::error::RepositoryError;

/// Storage abstraction for `CompiledManifest`.
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

// ── FileSystemRepository ─────────────────────────────────────────────────────

/// File-based repository that persists `CompiledManifest` as JSON.
///
/// Stores the manifest at a given file path. Uses atomic write (write to
/// temp file, then rename) to prevent corruption on crash.
#[derive(Debug)]
pub struct FileSystemRepository {
    path: PathBuf,
}

impl FileSystemRepository {
    /// Create a new file-system repository at the given path.
    ///
    /// The file does not need to exist yet — `load()` returns `NotFound` if absent.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the file path used by this repository.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Repository for FileSystemRepository {
    fn load(&self) -> Result<Arc<CompiledManifest>, RepositoryError> {
        let bytes = std::fs::read(&self.path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RepositoryError::NotFound
            } else {
                RepositoryError::Io(e)
            }
        })?;

        let manifest: CompiledManifest = serde_json::from_slice(&bytes)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        Ok(Arc::new(manifest))
    }

    fn save(&self, manifest: &CompiledManifest) -> Result<(), RepositoryError> {
        // Create parent directories if needed.
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(RepositoryError::Io)?;
        }

        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        // Atomic write: write to a temp file in the same directory, then rename.
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json.as_bytes()).map_err(RepositoryError::Io)?;
        std::fs::rename(&tmp_path, &self.path).map_err(RepositoryError::Io)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_test_manifest(name: &str) -> CompiledManifest {
        CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test".to_string(),
            datasets: IndexMap::new(),
            kinds: IndexMap::new(),
            relationships: Vec::new(),
            model_name: name.to_string(),
            model_description: None,
        }
    }

    #[test]
    fn test_fs_repo_load_not_found() {
        let repo = FileSystemRepository::new("/tmp/semstrait_test_nonexistent_file.json");
        let result = repo.load();
        assert!(matches!(result, Err(RepositoryError::NotFound)));
    }

    #[test]
    fn test_fs_repo_round_trip() {
        let dir = std::env::temp_dir().join("semstrait_test_fs_repo");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("manifest.json");

        let repo = FileSystemRepository::new(&path);
        let manifest = make_test_manifest("test_model");

        repo.save(&manifest).unwrap();
        assert!(path.exists());

        let loaded = repo.load().unwrap();
        assert_eq!(loaded.model_name, "test_model");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fs_repo_overwrite() {
        let dir = std::env::temp_dir().join("semstrait_test_fs_repo_overwrite");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("manifest.json");

        let repo = FileSystemRepository::new(&path);

        repo.save(&make_test_manifest("first")).unwrap();
        repo.save(&make_test_manifest("second")).unwrap();

        let loaded = repo.load().unwrap();
        assert_eq!(loaded.model_name, "second");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fs_repo_creates_parent_dirs() {
        let dir = std::env::temp_dir().join("semstrait_test_fs_repo_nested/a/b/c");
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("semstrait_test_fs_repo_nested"));
        let path = dir.join("manifest.json");

        let repo = FileSystemRepository::new(&path);
        repo.save(&make_test_manifest("nested")).unwrap();

        let loaded = repo.load().unwrap();
        assert_eq!(loaded.model_name, "nested");

        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("semstrait_test_fs_repo_nested"));
    }

    #[test]
    fn test_fs_repo_path() {
        let repo = FileSystemRepository::new("/tmp/test.json");
        assert_eq!(repo.path(), Path::new("/tmp/test.json"));
    }
}
