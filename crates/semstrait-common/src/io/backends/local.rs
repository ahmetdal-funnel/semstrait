//! Per `31b §8.2`. Local-filesystem back-end thin-wrapping
//! `object_store::local::LocalFileSystem`. `describe()` returns the
//! absolute form of the configured path.

use std::borrow::Cow;
use std::path::PathBuf;

use bytes::Bytes;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;

use crate::io::error::IoErrorKind;
use crate::io::sink::Sink;
use crate::io::source::Source;

#[derive(Clone, Debug)]
pub struct LocalFile {
    path: PathBuf,
}

impl LocalFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

fn absolute_path(p: &std::path::Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

fn map_object_store_error(path: &std::path::Path, err: object_store::Error) -> IoErrorKind {
    let describe = absolute_path(path).display().to_string();
    match err {
        object_store::Error::NotFound { .. } => IoErrorKind::NotFound { describe },
        object_store::Error::PermissionDenied { .. } => {
            IoErrorKind::PermissionDenied { describe }
        }
        object_store::Error::Unauthenticated { .. } => IoErrorKind::PermissionDenied { describe },
        object_store::Error::NotSupported { .. } | object_store::Error::NotImplemented => {
            IoErrorKind::Unsupported {
                describe,
                reason: "operation not supported by local back-end".into(),
            }
        }
        object_store::Error::InvalidPath { .. } => IoErrorKind::Malformed {
            describe,
            reason: "invalid filesystem path".into(),
        },
        other => IoErrorKind::Network {
            describe,
            reason: "local filesystem operation failed".into(),
            source: Some(Box::new(other)),
        },
    }
}

fn store_for(path: &std::path::Path) -> Result<(LocalFileSystem, ObjectPath), IoErrorKind> {
    let absolute = absolute_path(path);
    let store = LocalFileSystem::new();
    let object_path = ObjectPath::from_absolute_path(&absolute).map_err(|e| {
        IoErrorKind::Malformed {
            describe: absolute.display().to_string(),
            reason: format!("path is not addressable by object_store: {e}").into(),
        }
    })?;
    Ok((store, object_path))
}

impl Source for LocalFile {
    async fn read_raw(&self) -> Result<Bytes, IoErrorKind> {
        let (store, location) = store_for(&self.path)?;
        let result = store
            .get(&location)
            .await
            .map_err(|e| map_object_store_error(&self.path, e))?;
        result
            .bytes()
            .await
            .map_err(|e| map_object_store_error(&self.path, e))
    }

    fn describe(&self) -> Cow<'_, str> {
        Cow::Owned(absolute_path(&self.path).display().to_string())
    }
}

impl Sink for LocalFile {
    async fn write_raw(&self, bytes: Bytes) -> Result<(), IoErrorKind> {
        let (store, location) = store_for(&self.path)?;
        store
            .put(&location, bytes.into())
            .await
            .map(|_| ())
            .map_err(|e| map_object_store_error(&self.path, e))
    }

    fn describe(&self) -> Cow<'_, str> {
        Cow::Owned(absolute_path(&self.path).display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[tokio::test]
    async fn read_returns_bytes_from_existing_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("payload.bin");
        std::fs::write(&path, b"abc123").unwrap();

        let f = LocalFile::new(&path);
        let b = f.read_raw().await.unwrap();
        assert_eq!(b.as_ref(), b"abc123");
    }

    #[tokio::test]
    async fn read_emits_not_found_for_missing_path() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("missing.bin");

        let f = LocalFile::new(&path);
        match f.read_raw().await.unwrap_err() {
            IoErrorKind::NotFound { describe } => {
                assert!(describe.ends_with("missing.bin"));
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_creates_file_with_payload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("out.bin");

        let f = LocalFile::new(&path);
        f.write_raw(Bytes::from_static(b"payload")).await.unwrap();

        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, b"payload");
    }

    #[tokio::test]
    async fn write_creates_missing_parent_directories() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("sub/dir/out.bin");
        assert!(!path.parent().unwrap().exists());

        let f = LocalFile::new(&path);
        f.write_raw(Bytes::from_static(b"x")).await.unwrap();

        assert!(path.exists());
    }

    #[tokio::test]
    async fn write_replaces_existing_file_atomically() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("replace.bin");
        std::fs::write(&path, b"old").unwrap();

        let f = LocalFile::new(&path);
        f.write_raw(Bytes::from_static(b"new")).await.unwrap();

        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, b"new");
    }

    #[tokio::test]
    async fn round_trip_via_string_typed_read() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("round.txt");
        let f = LocalFile::new(&path);
        <LocalFile as Sink>::write(&f, "hello").await.unwrap();
        let s: String = f.read().await.unwrap();
        assert_eq!(s, "hello");
    }

    #[tokio::test]
    async fn describe_returns_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("abs.txt");
        let f = LocalFile::new(&path);
        let described = <LocalFile as Source>::describe(&f);
        assert!(std::path::Path::new(described.as_ref()).is_absolute());
    }

    #[tokio::test]
    async fn describe_does_not_require_file_to_exist() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("never-created.bin");
        let f = LocalFile::new(&path);
        let described = <LocalFile as Source>::describe(&f);
        assert!(described.ends_with("never-created.bin"));
    }
}
