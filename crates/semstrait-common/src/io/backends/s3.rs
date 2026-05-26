//! Per `31b §8.3`. S3 back-end thin-wrapping `object_store::aws::AmazonS3`.
//! `io-aws` feature only. The escape hatch
//! [`S3SourceBuilder::with_object_store_builder`] is the single documented
//! exception to SR-IO-8 — advanced callers opt into `object_store` evolution.

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use bytes::Bytes;
use dashmap::DashMap;
use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;

use crate::io::error::IoErrorKind;
use crate::io::sink::Sink;
use crate::io::source::Source;

#[derive(Clone, Debug)]
pub struct S3Source {
    bucket: Arc<str>,
    key: Arc<str>,
    client: ClientHandle,
}

#[derive(Clone, Debug)]
enum ClientHandle {
    Cached,
    Custom(Arc<AmazonS3>),
}

impl S3Source {
    /// Per `31b §8.3`. Uses `object_store`'s default credential chain
    /// and the `Location`-level client cache.
    pub fn new(bucket: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            bucket: Arc::from(bucket.into()),
            key: Arc::from(key.into()),
            client: ClientHandle::Cached,
        }
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    fn descriptor(&self) -> String {
        format!("s3://{}/{}", self.bucket, self.key)
    }

    fn resolve_client(&self) -> Result<Arc<AmazonS3>, IoErrorKind> {
        match &self.client {
            ClientHandle::Custom(c) => Ok(c.clone()),
            ClientHandle::Cached => cached_client(&self.bucket, None, None),
        }
    }

    fn object_path(&self) -> Result<ObjectPath, IoErrorKind> {
        ObjectPath::parse(self.key.as_ref()).map_err(|e| IoErrorKind::Malformed {
            describe: self.descriptor(),
            reason: format!("invalid S3 key: {e}").into(),
        })
    }

    fn map_err(&self, err: object_store::Error) -> IoErrorKind {
        let describe = self.descriptor();
        match err {
            object_store::Error::NotFound { .. } => IoErrorKind::NotFound { describe },
            object_store::Error::PermissionDenied { .. }
            | object_store::Error::Unauthenticated { .. } => {
                IoErrorKind::PermissionDenied { describe }
            }
            object_store::Error::NotSupported { .. } | object_store::Error::NotImplemented => {
                IoErrorKind::Unsupported {
                    describe,
                    reason: "operation not supported by S3 back-end".into(),
                }
            }
            other => IoErrorKind::Network {
                describe,
                reason: "S3 operation failed".into(),
                source: Some(Box::new(other)),
            },
        }
    }
}

impl Source for S3Source {
    async fn read_raw(&self) -> Result<Bytes, IoErrorKind> {
        let client = self.resolve_client()?;
        let path = self.object_path()?;
        let result = client.get(&path).await.map_err(|e| self.map_err(e))?;
        result.bytes().await.map_err(|e| self.map_err(e))
    }

    fn describe(&self) -> Cow<'_, str> {
        Cow::Owned(self.descriptor())
    }
}

impl Sink for S3Source {
    async fn write_raw(&self, bytes: Bytes) -> Result<(), IoErrorKind> {
        let client = self.resolve_client()?;
        let path = self.object_path()?;
        client
            .put(&path, bytes.into())
            .await
            .map(|_| ())
            .map_err(|e| self.map_err(e))
    }

    fn describe(&self) -> Cow<'_, str> {
        Cow::Owned(self.descriptor())
    }
}

pub struct S3SourceBuilder {
    bucket: String,
    key: String,
    region: Option<String>,
    endpoint: Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
    session_token: Option<String>,
    allow_http: Option<bool>,
    raw_builder: Option<AmazonS3Builder>,
}

impl S3SourceBuilder {
    pub fn new(bucket: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            key: key.into(),
            region: None,
            endpoint: None,
            access_key: None,
            secret_key: None,
            session_token: None,
            allow_http: None,
            raw_builder: None,
        }
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    pub fn with_endpoint(mut self, url: impl Into<String>) -> Self {
        self.endpoint = Some(url.into());
        self
    }

    pub fn with_credentials(
        mut self,
        access: impl Into<String>,
        secret: impl Into<String>,
    ) -> Self {
        self.access_key = Some(access.into());
        self.secret_key = Some(secret.into());
        self
    }

    pub fn with_session_token(mut self, token: impl Into<String>) -> Self {
        self.session_token = Some(token.into());
        self
    }

    pub fn with_allow_http(mut self, allow: bool) -> Self {
        self.allow_http = Some(allow);
        self
    }

    /// Per `31b §8.3` — escape hatch (the single documented exception
    /// to SR-IO-8). Caller-supplied builder takes precedence over every
    /// `with_*` setter.
    pub fn with_object_store_builder(mut self, builder: AmazonS3Builder) -> Self {
        self.raw_builder = Some(builder);
        self
    }

    pub fn build(self) -> Result<S3Source, IoErrorKind> {
        let descriptor = format!("s3://{}/{}", self.bucket, self.key);
        let mut builder = match self.raw_builder {
            Some(b) => b,
            None => {
                let mut b = AmazonS3Builder::new().with_bucket_name(&self.bucket);
                if let Some(r) = &self.region {
                    b = b.with_region(r);
                }
                if let Some(e) = &self.endpoint {
                    b = b.with_endpoint(e);
                }
                if let (Some(ak), Some(sk)) = (&self.access_key, &self.secret_key) {
                    b = b.with_access_key_id(ak).with_secret_access_key(sk);
                }
                if let Some(t) = &self.session_token {
                    b = b.with_token(t);
                }
                if let Some(allow) = self.allow_http {
                    b = b.with_allow_http(allow);
                }
                b
            }
        };
        // Bucket name flows through whichever branch produced the builder.
        builder = builder.with_bucket_name(&self.bucket);

        let client = builder.build().map_err(|e| IoErrorKind::Malformed {
            describe: descriptor,
            reason: format!("could not build S3 client: {e}").into(),
        })?;

        Ok(S3Source {
            bucket: Arc::from(self.bucket),
            key: Arc::from(self.key),
            client: ClientHandle::Custom(Arc::new(client)),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ClientKey {
    bucket: String,
    region: Option<String>,
    endpoint: Option<String>,
}

static CLIENT_CACHE: OnceLock<DashMap<ClientKey, Arc<AmazonS3>>> = OnceLock::new();

fn cache() -> &'static DashMap<ClientKey, Arc<AmazonS3>> {
    CLIENT_CACHE.get_or_init(DashMap::new)
}

fn cached_client(
    bucket: &str,
    region: Option<&str>,
    endpoint: Option<&str>,
) -> Result<Arc<AmazonS3>, IoErrorKind> {
    let key = ClientKey {
        bucket: bucket.to_owned(),
        region: region.map(str::to_owned),
        endpoint: endpoint.map(str::to_owned),
    };
    if let Some(hit) = cache().get(&key) {
        return Ok(hit.clone());
    }
    let mut builder = AmazonS3Builder::from_env().with_bucket_name(bucket);
    if let Some(r) = region {
        builder = builder.with_region(r);
    }
    if let Some(e) = endpoint {
        builder = builder.with_endpoint(e);
    }
    let client = builder.build().map_err(|e| IoErrorKind::Malformed {
        describe: format!("s3://{bucket}/"),
        reason: format!("could not build S3 client from env: {e}").into(),
    })?;
    let arc = Arc::new(client);
    cache().insert(key, arc.clone());
    Ok(arc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_source_descriptor_format() {
        let s = S3Source::new("my-bucket", "path/to/key.yaml");
        assert_eq!(<S3Source as Source>::describe(&s), "s3://my-bucket/path/to/key.yaml");
    }

    #[test]
    fn s3_source_exposes_bucket_and_key() {
        let s = S3Source::new("b", "k");
        assert_eq!(s.bucket(), "b");
        assert_eq!(s.key(), "k");
    }

    #[test]
    fn builder_accumulates_settings_without_panicking() {
        let _ = S3SourceBuilder::new("b", "k")
            .with_region("us-east-1")
            .with_endpoint("https://example.invalid")
            .with_credentials("AK", "SK")
            .with_session_token("T")
            .with_allow_http(true);
    }

    #[test]
    fn builder_build_with_explicit_credentials_returns_source() {
        let src = S3SourceBuilder::new("b", "k")
            .with_region("us-east-1")
            .with_credentials("AK", "SK")
            .build()
            .expect("explicit credentials should construct an S3 client");
        assert_eq!(src.bucket(), "b");
        assert_eq!(src.key(), "k");
    }
}
