//! Per `31b §4`. The async [`Sink`] trait — every writable back-end
//! implements it.

use std::borrow::Cow;
use std::future::Future;

use bytes::Bytes;

use crate::io::convert::IntoIoBytes;
use crate::io::error::IoErrorKind;

/// Per `31b §4`. Writes are caller-atomic per back-end (`§4.1`); concurrent
/// writes on the same target are last-writer-wins (`§4.3`).
pub trait Sink: Send + Sync {
    fn write_raw(&self, bytes: Bytes) -> impl Future<Output = Result<(), IoErrorKind>> + Send;

    fn write<B: IntoIoBytes + Send>(
        &self,
        data: B,
    ) -> impl Future<Output = Result<(), IoErrorKind>> + Send
    where
        Self: Sync,
    {
        async move { self.write_raw(data.into_io_bytes()).await }
    }

    fn describe(&self) -> Cow<'_, str>;
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    struct Capture {
        last: Mutex<Option<Bytes>>,
        id: String,
    }

    impl Sink for Capture {
        async fn write_raw(&self, bytes: Bytes) -> Result<(), IoErrorKind> {
            *self.last.lock().unwrap() = Some(bytes);
            Ok(())
        }

        fn describe(&self) -> Cow<'_, str> {
            Cow::Borrowed(self.id.as_str())
        }
    }

    fn cap() -> Capture {
        Capture {
            last: Mutex::new(None),
            id: "cap:1".into(),
        }
    }

    #[tokio::test]
    async fn write_raw_persists_payload() {
        let c = cap();
        c.write_raw(Bytes::from_static(b"abc")).await.unwrap();
        let last = c.last.lock().unwrap().clone().unwrap();
        assert_eq!(last.as_ref(), b"abc");
    }

    #[tokio::test]
    async fn write_default_accepts_str() {
        let c = cap();
        c.write("payload").await.unwrap();
        let last = c.last.lock().unwrap().clone().unwrap();
        assert_eq!(last.as_ref(), b"payload");
    }

    #[tokio::test]
    async fn write_default_accepts_owned_string() {
        let c = cap();
        c.write(String::from("owned")).await.unwrap();
        let last = c.last.lock().unwrap().clone().unwrap();
        assert_eq!(last.as_ref(), b"owned");
    }

    #[tokio::test]
    async fn write_default_accepts_vec_u8() {
        let c = cap();
        c.write(vec![1u8, 2, 3]).await.unwrap();
        let last = c.last.lock().unwrap().clone().unwrap();
        assert_eq!(last.as_ref(), &[1, 2, 3]);
    }

    #[tokio::test]
    async fn write_default_accepts_byte_slice() {
        let c = cap();
        let s: &[u8] = b"slice";
        c.write(s).await.unwrap();
        let last = c.last.lock().unwrap().clone().unwrap();
        assert_eq!(last.as_ref(), b"slice");
    }

    #[tokio::test]
    async fn describe_returns_identity() {
        let c = cap();
        assert_eq!(c.describe().as_ref(), "cap:1");
    }
}
