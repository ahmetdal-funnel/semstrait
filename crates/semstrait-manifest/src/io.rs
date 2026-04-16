//! Generic text loading from local filesystem or S3.
//!
//! Dispatches on URI scheme: `s3://` goes to AWS SDK, everything else
//! is treated as a local filesystem path.
//!
//! S3 support requires the `aws` feature flag.
//!
//! NOTE: This module lives in semstrait-manifest pragmatically.
//! See docs/TECH_DEBT.md TD-008 for the planned extraction to
//! a dedicated `semstrait-io` crate.

/// Errors from text loading operations.
#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("IO error reading '{path}': {source}")]
    FileSystem {
        path: String,
        source: std::io::Error,
    },

    #[error("S3 error reading '{uri}': {message}")]
    S3 { uri: String, message: String },

    #[error("S3 loading requires the 'aws' feature: {uri}")]
    S3NotEnabled { uri: String },

    #[error("invalid S3 URI (expected s3://bucket/key): {uri}")]
    InvalidS3Uri { uri: String },
}

/// Load text content from a local path or `s3://` URI.
///
/// - Local paths use `tokio::fs::read_to_string`.
/// - S3 URIs use `aws-sdk-s3` with the default credential chain
///   (environment variables, EC2 instance role, AWS profile).
///
/// # Errors
///
/// Returns `IoError::FileSystem` for local read failures,
/// `IoError::S3` for S3 access failures, or
/// `IoError::S3NotEnabled` when an S3 URI is used without the `aws` feature.
pub async fn load_text(location: &str) -> Result<String, IoError> {
    if location.starts_with("s3://") {
        load_s3_text(location).await
    } else {
        tokio::fs::read_to_string(location)
            .await
            .map_err(|e| IoError::FileSystem {
                path: location.to_string(),
                source: e,
            })
    }
}

/// Parse an S3 URI into (bucket, key).
#[cfg(any(feature = "aws", test))]
fn parse_s3_uri(uri: &str) -> Result<(&str, &str), IoError> {
    let without_scheme = uri
        .strip_prefix("s3://")
        .ok_or_else(|| IoError::InvalidS3Uri {
            uri: uri.to_string(),
        })?;
    let (bucket, key) = without_scheme
        .split_once('/')
        .ok_or_else(|| IoError::InvalidS3Uri {
            uri: uri.to_string(),
        })?;
    if bucket.is_empty() || key.is_empty() {
        return Err(IoError::InvalidS3Uri {
            uri: uri.to_string(),
        });
    }
    Ok((bucket, key))
}

#[cfg(feature = "aws")]
async fn load_s3_text(uri: &str) -> Result<String, IoError> {
    let (bucket, key) = parse_s3_uri(uri)?;

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = aws_sdk_s3::Client::new(&config);

    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| IoError::S3 {
            uri: uri.to_string(),
            message: e.to_string(),
        })?;

    let bytes = resp
        .body
        .collect()
        .await
        .map_err(|e| IoError::S3 {
            uri: uri.to_string(),
            message: format!("failed to read response body: {}", e),
        })?
        .into_bytes();

    String::from_utf8(bytes.to_vec()).map_err(|e| IoError::S3 {
        uri: uri.to_string(),
        message: format!("response is not valid UTF-8: {}", e),
    })
}

#[cfg(not(feature = "aws"))]
async fn load_s3_text(uri: &str) -> Result<String, IoError> {
    Err(IoError::S3NotEnabled {
        uri: uri.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_uri_valid() {
        let (bucket, key) = parse_s3_uri("s3://my-bucket/path/to/file.yaml").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "path/to/file.yaml");
    }

    #[test]
    fn test_parse_s3_uri_single_key() {
        let (bucket, key) = parse_s3_uri("s3://bucket/file.yaml").unwrap();
        assert_eq!(bucket, "bucket");
        assert_eq!(key, "file.yaml");
    }

    #[test]
    fn test_parse_s3_uri_missing_key() {
        let result = parse_s3_uri("s3://bucket-only");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_s3_uri_empty_bucket() {
        let result = parse_s3_uri("s3:///key");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_s3_uri_empty_key() {
        let result = parse_s3_uri("s3://bucket/");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_s3_uri_not_s3() {
        let result = parse_s3_uri("https://example.com/file");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_text_nonexistent_local_file() {
        let result = load_text("/nonexistent/path/model.yaml").await;
        assert!(matches!(result, Err(IoError::FileSystem { .. })));
    }

    #[cfg(not(feature = "aws"))]
    #[tokio::test]
    async fn test_load_text_s3_without_feature() {
        let result = load_text("s3://bucket/model.yaml").await;
        assert!(matches!(result, Err(IoError::S3NotEnabled { .. })));
    }
}
