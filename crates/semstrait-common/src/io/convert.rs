//! Per `31b §5`. Byte-to-typed and typed-to-byte conversion traits used
//! by [`crate::io::Source::read`] and [`crate::io::Sink::write`].

use bytes::Bytes;

use crate::io::error::IoErrorKind;

/// Per `31b §5`. Conversion is pure: implementations MUST NOT touch
/// the underlying store or perform I/O.
pub trait FromIoBytes: Sized {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind>;
}

impl FromIoBytes for Bytes {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind> {
        Ok(bytes)
    }
}

impl FromIoBytes for Vec<u8> {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind> {
        Ok(bytes.to_vec())
    }
}

impl FromIoBytes for String {
    fn from_io_bytes(bytes: Bytes) -> Result<Self, IoErrorKind> {
        String::from_utf8(bytes.to_vec()).map_err(|e| IoErrorKind::Malformed {
            describe: String::from("<in-conversion>"),
            reason: format!("invalid UTF-8 at byte {}", e.utf8_error().valid_up_to()).into(),
        })
    }
}

/// Per `31b §5`.
pub trait IntoIoBytes {
    fn into_io_bytes(self) -> Bytes;
}

impl IntoIoBytes for Bytes {
    fn into_io_bytes(self) -> Bytes {
        self
    }
}

impl IntoIoBytes for Vec<u8> {
    fn into_io_bytes(self) -> Bytes {
        Bytes::from(self)
    }
}

impl IntoIoBytes for &'_ [u8] {
    fn into_io_bytes(self) -> Bytes {
        Bytes::copy_from_slice(self)
    }
}

impl IntoIoBytes for String {
    fn into_io_bytes(self) -> Bytes {
        Bytes::from(self)
    }
}

impl IntoIoBytes for &'_ str {
    fn into_io_bytes(self) -> Bytes {
        Bytes::copy_from_slice(self.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_io_bytes_for_bytes_is_identity() {
        let b = Bytes::from_static(b"hello");
        let out = <Bytes as FromIoBytes>::from_io_bytes(b.clone()).unwrap();
        assert_eq!(out, b);
    }

    #[test]
    fn from_io_bytes_for_vec_copies() {
        let b = Bytes::from_static(b"hello");
        let v = <Vec<u8> as FromIoBytes>::from_io_bytes(b).unwrap();
        assert_eq!(v, b"hello".to_vec());
    }

    #[test]
    fn from_io_bytes_for_string_validates_utf8() {
        let b = Bytes::from_static("héllo".as_bytes());
        let s = <String as FromIoBytes>::from_io_bytes(b).unwrap();
        assert_eq!(s, "héllo");
    }

    #[test]
    fn from_io_bytes_for_string_emits_malformed_on_invalid_utf8() {
        let b = Bytes::from_static(&[0xff, 0xfe, 0xfd]);
        let err = <String as FromIoBytes>::from_io_bytes(b).unwrap_err();
        match err {
            IoErrorKind::Malformed { describe, reason } => {
                assert_eq!(describe, "<in-conversion>");
                assert!(reason.contains("invalid UTF-8"));
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn into_io_bytes_for_bytes_is_identity() {
        let b = Bytes::from_static(b"x");
        let out = b.clone().into_io_bytes();
        assert_eq!(out, b);
    }

    #[test]
    fn into_io_bytes_for_vec_moves_buffer() {
        let v = vec![1u8, 2, 3];
        let out = v.into_io_bytes();
        assert_eq!(out.as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn into_io_bytes_for_byte_slice_copies() {
        let s: &[u8] = b"abc";
        let out = s.into_io_bytes();
        assert_eq!(out.as_ref(), b"abc");
    }

    #[test]
    fn into_io_bytes_for_string_moves_buffer() {
        let s = String::from("payload");
        let out = s.into_io_bytes();
        assert_eq!(out.as_ref(), b"payload");
    }

    #[test]
    fn into_io_bytes_for_str_copies() {
        let s = "payload";
        let out = s.into_io_bytes();
        assert_eq!(out.as_ref(), b"payload");
    }
}
