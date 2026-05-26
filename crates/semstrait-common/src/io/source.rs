//! Per `31b §3`. The async [`Source`] trait — every readable back-end
//! implements it.

use std::borrow::Cow;
use std::future::Future;

use bytes::Bytes;

use crate::io::convert::FromIoBytes;
use crate::io::error::IoErrorKind;

/// Per `31b §3`. AFIT (Rust 1.75+) with `+ Send` so futures cross task
/// boundaries; consumers compose this trait with `Sink` (`§4`).
pub trait Source: Send + Sync {
    fn read_raw(&self) -> impl Future<Output = Result<Bytes, IoErrorKind>> + Send;

    fn read<T: FromIoBytes>(&self) -> impl Future<Output = Result<T, IoErrorKind>> + Send
    where
        Self: Sync,
    {
        async move { T::from_io_bytes(self.read_raw().await?) }
    }

    fn describe(&self) -> Cow<'_, str>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed {
        bytes: Bytes,
        id: String,
    }

    impl Source for Fixed {
        async fn read_raw(&self) -> Result<Bytes, IoErrorKind> {
            Ok(self.bytes.clone())
        }

        fn describe(&self) -> Cow<'_, str> {
            Cow::Borrowed(self.id.as_str())
        }
    }

    #[tokio::test]
    async fn read_raw_returns_payload() {
        let s = Fixed {
            bytes: Bytes::from_static(b"hello"),
            id: "fixed:1".into(),
        };
        let b = s.read_raw().await.unwrap();
        assert_eq!(b.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn read_default_delegates_into_string() {
        let s = Fixed {
            bytes: Bytes::from_static(b"yaml: ok"),
            id: "fixed:2".into(),
        };
        let text: String = s.read().await.unwrap();
        assert_eq!(text, "yaml: ok");
    }

    #[tokio::test]
    async fn read_default_delegates_into_vec() {
        let s = Fixed {
            bytes: Bytes::from_static(&[1, 2, 3]),
            id: "fixed:3".into(),
        };
        let v: Vec<u8> = s.read().await.unwrap();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn read_propagates_malformed_from_utf8_failure() {
        let s = Fixed {
            bytes: Bytes::from_static(&[0xff, 0xfe]),
            id: "fixed:4".into(),
        };
        let err: IoErrorKind = s.read::<String>().await.unwrap_err();
        assert!(matches!(err, IoErrorKind::Malformed { .. }));
    }

    #[tokio::test]
    async fn describe_returns_caller_supplied_identity() {
        let s = Fixed {
            bytes: Bytes::new(),
            id: "fixed:identity".into(),
        };
        assert_eq!(s.describe().as_ref(), "fixed:identity");
    }
}
