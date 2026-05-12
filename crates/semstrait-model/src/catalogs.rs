//! `catalogs.yaml` — `32b`.
//!
//! Author surface for catalog connections. Typed roster of provider
//! configurations referenced from the model via `extras.catalog:` per
//! `32 §1.3`. Resolution against the runtime catalog provider happens
//! at compile (`33`); this crate only parses the file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CatalogsConfig {
    /// Named catalog entries. `BTreeMap` for I4 (deterministic
    /// iteration). Aliases are user-chosen (e.g. `"polaris_prod"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub catalogs: BTreeMap<String, CatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CatalogEntry {
    /// Provider type discriminator (`"polaris"`, `"iceberg_rest"`,
    /// `"unity"`, etc.). Free-form per `32b §2.2` — provider modules
    /// live in `semstrait-catalog`.
    #[serde(rename = "type")]
    pub provider_type: String,

    /// Catalog / warehouse name as the provider expects it.
    pub name: String,

    /// Catalog base URL.
    pub url: String,

    /// Provider-specific realm (Polaris uses this; most others ignore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,

    /// Default namespace used when the model's `CatalogRef` does not
    /// override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespace: Option<String>,

    /// Authentication configuration.
    pub auth: CatalogAuthMethod,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum CatalogAuthMethod {
    /// Direct OAuth2 with explicit credentials.
    Oauth2 {
        /// Token endpoint URL. If omitted, derived from catalog URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        client_id: String,
        client_secret: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// Static bearer token.
    Bearer { token: String },
    /// AWS Secrets Manager → fetches `client_id` / `client_secret`,
    /// then OAuth2.
    AwsSecrets {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_url: Option<String>,
        secret_arn: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_access_key_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_secret_access_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_session_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_profile: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret_keys: Option<SecretKeyMapping>,
    },
}

/// Custom key name mapping for Secrets Manager JSON payloads. Defaults
/// (`client_id`, `client_secret`) apply when this is absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SecretKeyMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_key: Option<String>,
}
