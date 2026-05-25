//! Per `31b §8.1`. In-memory back-end. Storage is `tokio::sync::RwLock<Bytes>`
//! for atomic full-payload swaps; a process-global registry keyed by the
//! caller-supplied stable name lets `mem:<name>` URIs resolve to the same
//! handle (`§6.1`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use bytes::Bytes;
use tokio::sync::RwLock;

use crate::io::convert::IntoIoBytes;
use crate::io::error::IoErrorKind;
use crate::io::sink::Sink;
use crate::io::source::Source;

#[derive(Clone, Debug)]
pub struct InMemory {
    name: Arc<str>,
    buf: Arc<RwLock<Bytes>>,
}

impl InMemory {
    /// Per `31b §8.1`. The `name` participates in `describe()`; per SR-IO-9
    /// every `InMemory` MUST be named.
    pub fn new(name: impl Into<String>, bytes: impl IntoIoBytes) -> Self {
        let name: Arc<str> = Arc::from(name.into());
        let inner = Arc::new(RwLock::new(bytes.into_io_bytes()));
        let handle = Self {
            name: name.clone(),
            buf: inner,
        };
        register(name.as_ref(), handle.clone());
        handle
    }

    /// Per `31b §8.1`. Convenience: empty buffer registered under `name`.
    pub fn empty(name: impl Into<String>) -> Self {
        Self::new(name, Bytes::new())
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Source for InMemory {
    async fn read_raw(&self) -> Result<Bytes, IoErrorKind> {
        Ok(self.buf.read().await.clone())
    }

    fn describe(&self) -> Cow<'_, str> {
        Cow::Owned(format!("mem:{}", self.name))
    }
}

impl Sink for InMemory {
    async fn write_raw(&self, bytes: Bytes) -> Result<(), IoErrorKind> {
        let mut guard = self.buf.write().await;
        *guard = bytes;
        Ok(())
    }

    fn describe(&self) -> Cow<'_, str> {
        Cow::Owned(format!("mem:{}", self.name))
    }
}

static REGISTRY: OnceLock<Mutex<HashMap<String, InMemory>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, InMemory>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register(name: &str, handle: InMemory) {
    let mut guard = registry().lock().expect("InMemory registry mutex poisoned");
    guard.insert(name.to_owned(), handle);
}

/// Per `31b §6.1`. `Location::from_str("mem:<name>")` looks up a handle
/// previously registered via [`InMemory::new`] / [`InMemory::empty`].
pub(crate) fn lookup(name: &str) -> Option<InMemory> {
    let guard = registry().lock().expect("InMemory registry mutex poisoned");
    guard.get(name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_name(suffix: &str) -> String {
        format!(
            "test-{}-{}-{}",
            module_path!(),
            suffix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[tokio::test]
    async fn new_with_initial_bytes_reads_back_identical() {
        let m = InMemory::new(fresh_name("read"), "hello world");
        let b = m.read_raw().await.unwrap();
        assert_eq!(b.as_ref(), b"hello world");
    }

    #[tokio::test]
    async fn empty_reads_zero_bytes() {
        let m = InMemory::empty(fresh_name("empty"));
        let b = m.read_raw().await.unwrap();
        assert!(b.is_empty());
    }

    #[tokio::test]
    async fn write_replaces_buffer() {
        let m = InMemory::new(fresh_name("rw"), "old");
        m.write_raw(Bytes::from_static(b"new")).await.unwrap();
        let b = m.read_raw().await.unwrap();
        assert_eq!(b.as_ref(), b"new");
    }

    #[tokio::test]
    async fn describe_uses_mem_prefix_and_name() {
        let name = fresh_name("describe");
        let m = InMemory::new(name.clone(), "x");
        assert_eq!(<InMemory as Source>::describe(&m), format!("mem:{name}"));
        assert_eq!(<InMemory as Sink>::describe(&m), format!("mem:{name}"));
    }

    #[tokio::test]
    async fn registry_lookup_returns_registered_handle() {
        let name = fresh_name("registry-hit");
        let m = InMemory::new(&name, "x");
        let looked = lookup(&name).unwrap();
        let m_bytes = m.read_raw().await.unwrap();
        let l_bytes = looked.read_raw().await.unwrap();
        assert_eq!(m_bytes, l_bytes);
    }

    #[tokio::test]
    async fn registry_lookup_returns_none_for_missing_name() {
        let missing = format!("never-registered-{}", fresh_name("missing"));
        assert!(lookup(&missing).is_none());
    }

    #[tokio::test]
    async fn clones_share_underlying_buffer() {
        let m1 = InMemory::new(fresh_name("clone"), "before");
        let m2 = m1.clone();
        m1.write_raw(Bytes::from_static(b"after")).await.unwrap();
        let b = m2.read_raw().await.unwrap();
        assert_eq!(b.as_ref(), b"after");
    }

    #[tokio::test]
    async fn read_default_into_string_decodes_utf8() {
        let m = InMemory::new(fresh_name("utf8"), "héllo");
        let s: String = m.read().await.unwrap();
        assert_eq!(s, "héllo");
    }

    #[tokio::test]
    async fn write_default_accepts_str_through_sink() {
        let m = InMemory::new(fresh_name("str-write"), "");
        m.write("payload").await.unwrap();
        let s: String = m.read().await.unwrap();
        assert_eq!(s, "payload");
    }
}
