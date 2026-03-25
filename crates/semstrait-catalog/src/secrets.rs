//! AWS Secrets Manager integration for catalog credential resolution.
//!
//! Provides secure credential fetching from AWS Secrets Manager for Polaris
//! (Iceberg REST catalog). Uses the AWS SDK default credential chain, which
//! supports SSO profiles locally and IAM instance roles on EC2.

use crate::CatalogError;
use aws_config::BehaviorVersion;
use aws_config::Region;

/// Credentials parsed from an AWS Secrets Manager secret.
#[derive(Debug, Clone)]
pub struct PolarisSecret {
    pub client_id: String,
    pub client_secret: String,
}

/// Fetches Polaris credentials from AWS Secrets Manager.
///
/// Uses the default AWS credential chain:
/// - Locally: SSO profiles via `~/.aws/config`
/// - On EC2: IAM instance role via IMDS
/// - EKS: Web identity token file
/// - Env vars: `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`
///
/// The secret value must be a JSON string with `client_id` and `client_secret` fields.
pub async fn resolve_polaris_secret(
    secret_arn: &str,
    region: Option<&str>,
) -> Result<PolarisSecret, CatalogError> {
    resolve_polaris_secret_with_credentials(secret_arn, region, None, None, None, None).await
}

/// Fetches Polaris credentials from AWS Secrets Manager with optional explicit
/// AWS credentials or profile.
///
/// Resolution priority:
/// 1. Explicit `access_key_id` + `secret_access_key` → static credentials
/// 2. Explicit `profile` → named SSO/credential profile from ~/.aws/config
/// 3. Default credential chain (env vars, SSO via AWS_PROFILE, IAM roles)
pub async fn resolve_polaris_secret_with_credentials(
    secret_arn: &str,
    region: Option<&str>,
    profile: Option<&str>,
    access_key_id: Option<&str>,
    secret_access_key: Option<&str>,
    session_token: Option<&str>,
) -> Result<PolarisSecret, CatalogError> {
    let mut config_loader = aws_config::defaults(BehaviorVersion::v2026_01_12());
    if let Some(r) = region {
        config_loader = config_loader.region(Region::new(r.to_owned()));
    }
    if let (Some(key_id), Some(secret_key)) = (access_key_id, secret_access_key) {
        let creds = aws_credential_types::Credentials::new(
            key_id,
            secret_key,
            session_token.map(|s| s.to_owned()),
            None,
            "semstrait",
        );
        config_loader = config_loader.credentials_provider(creds);
    } else if let Some(p) = profile {
        config_loader = config_loader.profile_name(p);
    }
    let config = config_loader.load().await;

    let client = aws_sdk_secretsmanager::Client::new(&config);
    let output = client
        .get_secret_value()
        .secret_id(secret_arn)
        .send()
        .await
        .map_err(|e| {
            CatalogError::ConnectionError(format!("AWS Secrets Manager error: {e:?}"))
        })?;

    let secret_string = output
        .secret_string()
        .ok_or_else(|| CatalogError::Internal("secret is binary, expected JSON string".into()))?;

    parse_polaris_secret(secret_string)
}

/// Parse a Polaris secret from a JSON string.
///
/// Expects: `{"client_id": "...", "client_secret": "..."}`
pub(crate) fn parse_polaris_secret(json: &str) -> Result<PolarisSecret, CatalogError> {
    let parsed: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| CatalogError::Internal(format!("failed to parse secret JSON: {e}")))?;

    let client_id = parsed["client_id"]
        .as_str()
        .ok_or_else(|| CatalogError::Internal("secret JSON missing 'client_id' field".into()))?
        .to_owned();

    let client_secret = parsed["client_secret"]
        .as_str()
        .ok_or_else(|| {
            CatalogError::Internal("secret JSON missing 'client_secret' field".into())
        })?
        .to_owned();

    Ok(PolarisSecret {
        client_id,
        client_secret,
    })
}

/// Configuration for building a Polaris catalog via AWS Secrets Manager.
pub struct PolarisCatalogConfig<'a> {
    pub catalog_url: &'a str,
    pub secret_arn: &'a str,
    pub aws_region: Option<&'a str>,
    pub warehouse: Option<&'a str>,
    pub realm: Option<&'a str>,
    pub scope: Option<&'a str>,
    pub aws_profile: Option<&'a str>,
    pub aws_access_key_id: Option<&'a str>,
    pub aws_secret_access_key: Option<&'a str>,
    pub aws_session_token: Option<&'a str>,
}

/// Build an [`IcebergRestCatalog`](crate::IcebergRestCatalog) for Polaris
/// using credentials from AWS Secrets Manager.
pub async fn build_polaris_catalog(
    config: &PolarisCatalogConfig<'_>,
) -> Result<crate::IcebergRestCatalog, CatalogError> {
    let secret = resolve_polaris_secret_with_credentials(
        config.secret_arn,
        config.aws_region,
        config.aws_profile,
        config.aws_access_key_id,
        config.aws_secret_access_key,
        config.aws_session_token,
    )
    .await?;

    let token_url = format!("{}/v1/oauth/tokens", config.catalog_url);

    let oauth_scope = config.scope.unwrap_or("PRINCIPAL_ROLE:ALL");
    let mut catalog = crate::IcebergRestCatalog::new(config.catalog_url).with_oauth2(
        token_url,
        secret.client_id,
        secret.client_secret,
        Some(oauth_scope.to_owned()),
    );

    if let Some(wh) = config.warehouse {
        catalog = catalog.with_warehouse(wh);
    }

    if let Some(r) = config.realm {
        catalog = catalog.with_custom_header("Polaris-Realm", r);
    }

    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_secret() {
        let json = r#"{"client_id": "my-id", "client_secret": "my-secret"}"#;
        let secret = parse_polaris_secret(json).unwrap();
        assert_eq!(secret.client_id, "my-id");
        assert_eq!(secret.client_secret, "my-secret");
    }

    #[test]
    fn test_parse_extra_fields_ignored() {
        let json =
            r#"{"client_id": "id", "client_secret": "secret", "extra": "field"}"#;
        let secret = parse_polaris_secret(json).unwrap();
        assert_eq!(secret.client_id, "id");
        assert_eq!(secret.client_secret, "secret");
    }

    #[test]
    fn test_parse_missing_client_id() {
        let json = r#"{"client_secret": "secret"}"#;
        let err = parse_polaris_secret(json).unwrap_err();
        assert!(
            err.to_string().contains("client_id"),
            "error should mention client_id: {}",
            err
        );
    }

    #[test]
    fn test_parse_missing_client_secret() {
        let json = r#"{"client_id": "id"}"#;
        let err = parse_polaris_secret(json).unwrap_err();
        assert!(
            err.to_string().contains("client_secret"),
            "error should mention client_secret: {}",
            err
        );
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_polaris_secret("not json").unwrap_err();
        assert!(
            err.to_string().contains("parse"),
            "error should mention parsing: {}",
            err
        );
    }

    #[test]
    fn test_parse_null_client_id() {
        let json = r#"{"client_id": null, "client_secret": "secret"}"#;
        let err = parse_polaris_secret(json).unwrap_err();
        assert!(err.to_string().contains("client_id"));
    }
}
