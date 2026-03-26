//! Catalog connection configuration types.
//!
//! Parsed from a separate `catalogs.yaml` file to separate connection
//! concerns from semantic model definitions. Supports multiple named
//! catalogs that entities reference via [`CatalogRef`](super::CatalogRef).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level catalogs configuration, parsed from `catalogs.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CatalogsConfig {
    /// Named catalog entries. Keys are user-chosen aliases (e.g., "polaris_prod").
    pub catalogs: HashMap<String, CatalogEntry>,
}

/// A single named catalog connection entry.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CatalogEntry {
    /// Provider type: "polaris", "iceberg_rest", "unity", etc.
    #[serde(rename = "type")]
    pub provider_type: String,
    /// Catalog/warehouse name.
    pub name: String,
    /// Catalog base URL.
    pub url: String,
    /// Realm (Polaris-specific, optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
    /// Default namespace for table resolution (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespace: Option<String>,
    /// Authentication configuration.
    pub auth: CatalogAuthMethod,
}

/// Authentication method for catalog connections.
///
/// Required fields depend on the variant. String values may contain
/// `${ENV_VAR}` references for environment variable substitution.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum CatalogAuthMethod {
    /// Direct OAuth2 with explicit credentials.
    Oauth2 {
        /// Token endpoint URL. If omitted, derived from catalog URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        /// OAuth2 client ID (value or `${ENV_VAR}`).
        client_id: String,
        /// OAuth2 client secret (value or `${ENV_VAR}`).
        client_secret: String,
        /// OAuth2 scope (e.g., "PRINCIPAL_ROLE:ALL").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// Static bearer token.
    Bearer {
        /// Bearer token (value or `${ENV_VAR}`).
        token: String,
    },
    /// AWS Secrets Manager → fetches client_id/client_secret, then OAuth2.
    AwsSecrets {
        /// Token endpoint URL. If omitted, derived from catalog URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        /// AWS Secrets Manager ARN containing credentials.
        secret_arn: String,
        /// AWS region for Secrets Manager (e.g., "eu-west-1").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        /// OAuth2 scope after credential retrieval.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        /// Explicit AWS access key ID (value or `${ENV_VAR}`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_access_key_id: Option<String>,
        /// Explicit AWS secret access key (value or `${ENV_VAR}`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_secret_access_key: Option<String>,
        /// AWS session token for temporary credentials.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_session_token: Option<String>,
        /// AWS SSO profile name (from ~/.aws/config).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_profile: Option<String>,
        /// Custom key name mapping for Secrets Manager JSON payload.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret_keys: Option<SecretKeyMapping>,
    },
}

/// Custom key name mapping for secrets store JSON.
///
/// By default, the Secrets Manager JSON is expected to have
/// `{"client_id": "...", "client_secret": "..."}`. Override these
/// key names if your secrets store uses different field names.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SecretKeyMapping {
    /// Key name for client_id in secrets JSON. Default: "client_id".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id_key: Option<String>,
    /// Key name for client_secret in secrets JSON. Default: "client_secret".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_key: Option<String>,
}

/// Parse catalogs configuration from YAML string.
pub fn parse_catalogs(yaml: &str) -> Result<CatalogsConfig, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_catalogs_oauth2() {
        let yaml = r#"
catalogs:
  polaris_dev:
    type: polaris
    name: dev_warehouse
    url: https://polaris-dev.example.com/api/catalog
    auth:
      method: oauth2
      client_id: my_client
      client_secret: my_secret
      scope: "PRINCIPAL_ROLE:ALL"
"#;
        let config = parse_catalogs(yaml).unwrap();
        assert_eq!(config.catalogs.len(), 1);
        let entry = &config.catalogs["polaris_dev"];
        assert_eq!(entry.provider_type, "polaris");
        assert_eq!(entry.name, "dev_warehouse");
        assert!(matches!(entry.auth, CatalogAuthMethod::Oauth2 { .. }));
    }

    #[test]
    fn test_parse_catalogs_aws_secrets() {
        let yaml = r#"
catalogs:
  polaris_prod:
    type: polaris
    name: fs1henp3k8p1hqo
    url: https://polaris.odp.test.funnel.io/api/catalog
    realm: default-realm
    default_namespace: default
    auth:
      method: aws_secrets
      secret_arn: arn:aws:secretsmanager:eu-west-1:123456:secret:my-secret
      region: eu-west-1
      scope: "PRINCIPAL_ROLE:ALL"
"#;
        let config = parse_catalogs(yaml).unwrap();
        let entry = &config.catalogs["polaris_prod"];
        assert_eq!(entry.realm.as_deref(), Some("default-realm"));
        assert_eq!(entry.default_namespace.as_deref(), Some("default"));
        if let CatalogAuthMethod::AwsSecrets { secret_arn, region, .. } = &entry.auth {
            assert!(secret_arn.starts_with("arn:aws:"));
            assert_eq!(region.as_deref(), Some("eu-west-1"));
        } else {
            panic!("Expected AwsSecrets auth method");
        }
    }

    #[test]
    fn test_parse_catalogs_bearer() {
        let yaml = r#"
catalogs:
  test_catalog:
    type: iceberg_rest
    name: test_wh
    url: https://catalog.example.com
    auth:
      method: bearer
      token: my-token-123
"#;
        let config = parse_catalogs(yaml).unwrap();
        let entry = &config.catalogs["test_catalog"];
        assert!(matches!(entry.auth, CatalogAuthMethod::Bearer { .. }));
    }

    #[test]
    fn test_parse_multi_catalog() {
        let yaml = r#"
catalogs:
  polaris_prod:
    type: polaris
    name: prod_wh
    url: https://polaris-prod.example.com
    auth:
      method: bearer
      token: prod-token
  polaris_dev:
    type: polaris
    name: dev_wh
    url: https://polaris-dev.example.com
    auth:
      method: oauth2
      client_id: dev_client
      client_secret: dev_secret
"#;
        let config = parse_catalogs(yaml).unwrap();
        assert_eq!(config.catalogs.len(), 2);
        assert!(config.catalogs.contains_key("polaris_prod"));
        assert!(config.catalogs.contains_key("polaris_dev"));
    }

    #[test]
    fn test_parse_aws_secrets_with_custom_key_mapping() {
        let yaml = r#"
catalogs:
  custom:
    type: polaris
    name: wh
    url: https://example.com
    auth:
      method: aws_secrets
      secret_arn: arn:aws:secretsmanager:us-east-1:123:secret:x
      secret_keys:
        client_id_key: polaris_client_id
        client_secret_key: polaris_client_secret
"#;
        let config = parse_catalogs(yaml).unwrap();
        if let CatalogAuthMethod::AwsSecrets { secret_keys, .. } = &config.catalogs["custom"].auth {
            let keys = secret_keys.as_ref().unwrap();
            assert_eq!(keys.client_id_key.as_deref(), Some("polaris_client_id"));
            assert_eq!(keys.client_secret_key.as_deref(), Some("polaris_client_secret"));
        } else {
            panic!("Expected AwsSecrets");
        }
    }

    #[test]
    fn test_catalogs_config_serde_roundtrip() {
        let yaml = r#"
catalogs:
  test:
    type: polaris
    name: wh
    url: https://example.com
    auth:
      method: bearer
      token: tok
"#;
        let config = parse_catalogs(yaml).unwrap();
        let yaml_out = serde_yaml::to_string(&config).unwrap();
        let back: CatalogsConfig = serde_yaml::from_str(&yaml_out).unwrap();
        assert_eq!(back.catalogs.len(), 1);
    }
}
