//! Per `31b §6`. The polymorphic [`Location`] enum — `FromStr` parses
//! `s3://`, `mem:`, `file://`, and bare paths, dispatches `Source` /
//! `Sink` to the inner back-end.

use std::borrow::Cow;
use std::path::PathBuf;
use std::str::FromStr;

use bytes::Bytes;

use crate::io::backends;
use crate::io::error::IoErrorKind;
use crate::io::sink::Sink;
use crate::io::source::Source;

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Location {
    Local(backends::local::LocalFile),
    InMemory(backends::memory::InMemory),
    #[cfg(feature = "io-aws")]
    S3(backends::s3::S3Source),
}

impl Source for Location {
    async fn read_raw(&self) -> Result<Bytes, IoErrorKind> {
        match self {
            Self::Local(b) => b.read_raw().await,
            Self::InMemory(b) => b.read_raw().await,
            #[cfg(feature = "io-aws")]
            Self::S3(b) => b.read_raw().await,
        }
    }

    fn describe(&self) -> Cow<'_, str> {
        match self {
            Self::Local(b) => <backends::local::LocalFile as Source>::describe(b),
            Self::InMemory(b) => <backends::memory::InMemory as Source>::describe(b),
            #[cfg(feature = "io-aws")]
            Self::S3(b) => <backends::s3::S3Source as Source>::describe(b),
        }
    }
}

impl Sink for Location {
    async fn write_raw(&self, bytes: Bytes) -> Result<(), IoErrorKind> {
        match self {
            Self::Local(b) => b.write_raw(bytes).await,
            Self::InMemory(b) => b.write_raw(bytes).await,
            #[cfg(feature = "io-aws")]
            Self::S3(b) => b.write_raw(bytes).await,
        }
    }

    fn describe(&self) -> Cow<'_, str> {
        match self {
            Self::Local(b) => <backends::local::LocalFile as Sink>::describe(b),
            Self::InMemory(b) => <backends::memory::InMemory as Sink>::describe(b),
            #[cfg(feature = "io-aws")]
            Self::S3(b) => <backends::s3::S3Source as Sink>::describe(b),
        }
    }
}

impl FromStr for Location {
    type Err = IoErrorKind;

    fn from_str(s: &str) -> Result<Self, IoErrorKind> {
        if let Some(rest) = s.strip_prefix("mem:") {
            return parse_mem(rest);
        }
        if let Some(rest) = s.strip_prefix("s3://") {
            return parse_s3(rest);
        }
        if let Some(rest) = s.strip_prefix("file://") {
            return Ok(Self::Local(backends::local::LocalFile::new(PathBuf::from(
                rest,
            ))));
        }
        if let Some(idx) = s.find("://") {
            let scheme = &s[..idx];
            return Err(IoErrorKind::Unsupported {
                describe: s.to_owned(),
                reason: format!("unknown scheme '{scheme}'").into(),
            });
        }
        Ok(Self::Local(backends::local::LocalFile::new(PathBuf::from(s))))
    }
}

fn parse_mem(name: &str) -> Result<Location, IoErrorKind> {
    if name.is_empty() {
        return Err(IoErrorKind::Malformed {
            describe: "mem:".to_owned(),
            reason: "missing in-memory handle name".into(),
        });
    }
    match backends::memory::lookup(name) {
        Some(handle) => Ok(Location::InMemory(handle)),
        None => Err(IoErrorKind::NotFound {
            describe: format!("mem:{name}"),
        }),
    }
}

#[cfg(feature = "io-aws")]
fn parse_s3(rest: &str) -> Result<Location, IoErrorKind> {
    let (bucket, key) = rest.split_once('/').ok_or_else(|| IoErrorKind::Malformed {
        describe: format!("s3://{rest}"),
        reason: "expected s3://<bucket>/<key>".into(),
    })?;
    if bucket.is_empty() {
        return Err(IoErrorKind::Malformed {
            describe: format!("s3://{rest}"),
            reason: "missing S3 bucket".into(),
        });
    }
    if key.is_empty() {
        return Err(IoErrorKind::Malformed {
            describe: format!("s3://{rest}"),
            reason: "missing S3 key".into(),
        });
    }
    Ok(Location::S3(backends::s3::S3Source::new(bucket, key)))
}

#[cfg(not(feature = "io-aws"))]
fn parse_s3(rest: &str) -> Result<Location, IoErrorKind> {
    Err(IoErrorKind::Unsupported {
        describe: format!("s3://{rest}"),
        reason: "io-aws feature is disabled".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::io::backends::memory::InMemory;

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

    #[test]
    fn from_str_bare_path_dispatches_to_local() {
        let loc = Location::from_str("./model.yaml").unwrap();
        assert!(matches!(loc, Location::Local(_)));
    }

    #[test]
    fn from_str_absolute_path_dispatches_to_local() {
        let loc = Location::from_str("/tmp/model.yaml").unwrap();
        assert!(matches!(loc, Location::Local(_)));
    }

    #[test]
    fn from_str_relative_dotdot_dispatches_to_local() {
        let loc = Location::from_str("../sibling/model.yaml").unwrap();
        assert!(matches!(loc, Location::Local(_)));
    }

    #[test]
    fn from_str_file_scheme_strips_prefix() {
        let loc = Location::from_str("file:///abs/path/model.yaml").unwrap();
        match loc {
            Location::Local(lf) => {
                assert_eq!(lf.path(), std::path::Path::new("/abs/path/model.yaml"));
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn from_str_unknown_scheme_emits_unsupported() {
        let err = Location::from_str("ftp://host/file").unwrap_err();
        match err {
            IoErrorKind::Unsupported { describe, reason } => {
                assert_eq!(describe, "ftp://host/file");
                assert!(reason.contains("ftp"));
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn from_str_mem_with_empty_name_emits_malformed() {
        let err = Location::from_str("mem:").unwrap_err();
        assert!(matches!(err, IoErrorKind::Malformed { .. }));
    }

    #[test]
    fn from_str_mem_with_unregistered_name_emits_not_found() {
        let name = format!("never-registered-{}", fresh_name("missing"));
        let err = Location::from_str(&format!("mem:{name}")).unwrap_err();
        match err {
            IoErrorKind::NotFound { describe } => {
                assert_eq!(describe, format!("mem:{name}"));
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_str_mem_resolves_registered_handle() {
        let name = fresh_name("resolve");
        let _registered = InMemory::new(&name, "payload");

        let loc = Location::from_str(&format!("mem:{name}")).unwrap();
        let s: String = loc.read().await.unwrap();
        assert_eq!(s, "payload");
    }

    #[cfg(not(feature = "io-aws"))]
    #[test]
    fn from_str_s3_without_io_aws_emits_unsupported() {
        let err = Location::from_str("s3://bucket/key").unwrap_err();
        match err {
            IoErrorKind::Unsupported { describe, reason } => {
                assert_eq!(describe, "s3://bucket/key");
                assert!(reason.contains("io-aws"));
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[cfg(feature = "io-aws")]
    #[test]
    fn from_str_s3_with_io_aws_dispatches_to_s3() {
        let loc = Location::from_str("s3://bucket/k.yaml").unwrap();
        assert!(matches!(loc, Location::S3(_)));
    }

    #[cfg(feature = "io-aws")]
    #[test]
    fn from_str_s3_without_bucket_emits_malformed() {
        let err = Location::from_str("s3:///key").unwrap_err();
        assert!(matches!(err, IoErrorKind::Malformed { .. }));
    }

    #[cfg(feature = "io-aws")]
    #[test]
    fn from_str_s3_without_key_emits_malformed() {
        let err = Location::from_str("s3://bucket/").unwrap_err();
        assert!(matches!(err, IoErrorKind::Malformed { .. }));
    }

    #[tokio::test]
    async fn round_trip_local_via_location() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("via-location.bin");
        std::fs::write(&path, b"existing").unwrap();

        let loc = Location::from_str(path.to_str().unwrap()).unwrap();
        let bytes = loc.read_raw().await.unwrap();
        assert_eq!(bytes.as_ref(), b"existing");

        loc.write_raw(Bytes::from_static(b"replaced")).await.unwrap();
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, b"replaced");
    }

    #[tokio::test]
    async fn describe_delegates_to_inner_backend() {
        let name = fresh_name("describe-delegates");
        let _registered = InMemory::new(&name, "x");
        let loc = Location::from_str(&format!("mem:{name}")).unwrap();
        assert_eq!(<Location as Source>::describe(&loc), format!("mem:{name}"));
    }
}
