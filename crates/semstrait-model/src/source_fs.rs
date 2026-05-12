//! [`SourceFs`] strategy trait + built-in implementations — `32 §9.6`.
//!
//! The loader is parametrized by a filesystem strategy rather than a
//! phase typestate. `LocalFs` is the production default; `InMemoryFs`
//! is the test affordance. Adding a new strategy is a MINOR change per
//! `32 §9.6.2`.

use std::collections::HashMap;
use std::io::{self, ErrorKind};

/// Filesystem-strategy trait. Implementors decide how a logical path
/// resolves to bytes. Sync only — async I/O is out of v1 scope per
/// `32 §10.4`.
pub trait SourceFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, io::Error>;
}

/// Production strategy — delegates to [`std::fs::read`].
#[derive(Debug, Clone, Default)]
pub struct LocalFs;

impl LocalFs {
    pub fn new() -> Self {
        Self
    }
}

impl SourceFs for LocalFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, io::Error> {
        std::fs::read(path)
    }
}

/// Test strategy — `HashMap<path, bytes>` lookup. Miss yields
/// [`ErrorKind::NotFound`].
#[derive(Debug, Clone, Default)]
pub struct InMemoryFs {
    files: HashMap<String, Vec<u8>>,
}

impl InMemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), contents.into());
    }

    pub fn with_file(mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) -> Self {
        self.insert(path, contents);
        self
    }
}

impl SourceFs for InMemoryFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, io::Error> {
        self.files.get(path).cloned().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                format!("InMemoryFs: no such path `{}`", path),
            )
        })
    }
}
