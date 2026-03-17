//! Iceberg REST catalog implementation.
//!
//! Implements the [Iceberg REST Catalog API](https://iceberg.apache.org/spec/#rest-catalog)
//! spec for namespace listing, table discovery, and schema retrieval.
//!
//! Compatible with Polaris (Snowflake), Gravitino (Apache), Tabular, and other
//! Iceberg REST catalog servers.

use crate::{CatalogColumn, CatalogError, CatalogProvider, TableRef};
use async_trait::async_trait;
use reqwest::Client;
use semstrait_core::{DataType, GlobPattern};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ============================================================================
// Auth configuration
// ============================================================================

/// Authentication configuration for the Iceberg REST catalog.
#[derive(Debug, Clone)]
pub enum AuthConfig {
    /// No authentication.
    None,
    /// Static Bearer token.
    BearerToken(String),
    /// OAuth2 client credentials flow.
    OAuth2 {
        token_url: String,
        client_id: String,
        client_secret: String,
        scope: Option<String>,
    },
}

// ============================================================================
// Iceberg REST Catalog
// ============================================================================

/// Client for the Iceberg REST Catalog API.
///
/// Supports OAuth2 and Bearer token authentication. Implements `CatalogProvider`
/// for integration with `ManifestCompiler` glob expansion.
#[derive(Debug, Clone)]
pub struct IcebergRestCatalog {
    base_url: String,
    warehouse: Option<String>,
    prefix: Option<String>,
    client: Client,
    auth: AuthConfig,
    /// Cached OAuth2 access token with optional expiry instant.
    token_cache: Arc<RwLock<Option<(String, Option<Instant>)>>>,
}

impl IcebergRestCatalog {
    /// Creates a new Iceberg REST catalog client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            warehouse: None,
            prefix: None,
            client: Client::new(),
            auth: AuthConfig::None,
            token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the warehouse identifier.
    pub fn with_warehouse(mut self, warehouse: impl Into<String>) -> Self {
        self.warehouse = Some(warehouse.into());
        self
    }

    /// Set the catalog prefix (used in URL path for multi-tenant catalogs).
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Set Bearer token authentication.
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = AuthConfig::BearerToken(token.into());
        self
    }

    /// Set OAuth2 client credentials authentication.
    pub fn with_oauth2(
        mut self,
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        scope: Option<String>,
    ) -> Self {
        self.auth = AuthConfig::OAuth2 {
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope,
        };
        self
    }

    /// Returns the base URL of the catalog.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build the base API path (with optional prefix).
    fn api_path(&self) -> String {
        match &self.prefix {
            Some(prefix) => format!("{}/v1/{}", self.base_url, prefix),
            None => format!("{}/v1", self.base_url),
        }
    }

    /// Get or refresh the auth token.
    async fn get_token(&self) -> Result<Option<String>, CatalogError> {
        match &self.auth {
            AuthConfig::None => Ok(None),
            AuthConfig::BearerToken(token) => Ok(Some(token.clone())),
            AuthConfig::OAuth2 {
                token_url,
                client_id,
                client_secret,
                scope,
            } => {
                // Check cache first; return cached token if still valid.
                {
                    let cached = self.token_cache.read().await;
                    if let Some((token, expiry)) = cached.as_ref() {
                        if expiry.map_or(true, |exp| Instant::now() < exp) {
                            return Ok(Some(token.clone()));
                        }
                        tracing::debug!("OAuth2 token expired, refreshing");
                    }
                }

                // Fetch new token.
                tracing::debug!("fetching OAuth2 token from {}", token_url);
                let mut params = vec![
                    ("grant_type", "client_credentials"),
                    ("client_id", client_id.as_str()),
                    ("client_secret", client_secret.as_str()),
                ];
                if let Some(s) = scope {
                    params.push(("scope", s.as_str()));
                }

                let resp = self
                    .client
                    .post(token_url)
                    .form(&params)
                    .send()
                    .await
                    .map_err(|e| CatalogError::ConnectionError(e.to_string()))?;

                if !resp.status().is_success() {
                    return Err(CatalogError::ConnectionError(format!(
                        "OAuth2 token request failed: {}",
                        resp.status()
                    )));
                }

                let body: TokenResponse = resp
                    .json()
                    .await
                    .map_err(|e| CatalogError::Internal(format!("failed to parse token: {e}")))?;

                let token = body.access_token;
                // Cache with expiry: subtract 30s buffer so we refresh before actual expiry.
                let expiry = body.expires_in.map(|secs| {
                    Instant::now() + Duration::from_secs(secs.saturating_sub(30))
                });
                *self.token_cache.write().await = Some((token.clone(), expiry));
                Ok(Some(token))
            }
        }
    }

    /// Make an authenticated GET request.
    async fn get(&self, url: &str) -> Result<reqwest::Response, CatalogError> {
        let mut req = self.client.get(url);

        if let Some(token) = self.get_token().await? {
            req = req.bearer_auth(token);
        }
        if let Some(wh) = &self.warehouse {
            req = req.query(&[("warehouse", wh.as_str())]);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CatalogError::ConnectionError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(CatalogError::TableNotFound(url.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(CatalogError::ConnectionError(format!(
                "HTTP {status}: {body}"
            )));
        }

        Ok(resp)
    }

    /// List tables in a namespace via the Iceberg REST API.
    async fn list_tables_raw(
        &self,
        namespace: &str,
    ) -> Result<Vec<TableIdentifier>, CatalogError> {
        let url = format!(
            "{}/namespaces/{}/tables",
            self.api_path(),
            encode_namespace(namespace)
        );
        let resp = self.get(&url).await?;
        let body: ListTablesResponse = resp
            .json()
            .await
            .map_err(|e| CatalogError::Internal(format!("failed to parse tables: {e}")))?;
        Ok(body.identifiers)
    }

    /// Load table metadata from the Iceberg REST API.
    async fn load_table(
        &self,
        namespace: &str,
        table: &str,
    ) -> Result<LoadTableResponse, CatalogError> {
        let url = format!(
            "{}/namespaces/{}/tables/{}",
            self.api_path(),
            encode_namespace(namespace),
            table
        );
        let resp = self.get(&url).await?;
        resp.json()
            .await
            .map_err(|e| CatalogError::Internal(format!("failed to parse table metadata: {e}")))
    }
}

#[async_trait]
impl CatalogProvider for IcebergRestCatalog {
    async fn list_tables(
        &self,
        namespace: &str,
        pattern: &GlobPattern,
    ) -> Result<Vec<TableRef>, CatalogError> {
        let identifiers = self.list_tables_raw(namespace).await?;

        let tables: Vec<TableRef> = identifiers
            .into_iter()
            .filter(|id| pattern.matches(&id.name))
            .map(|id| {
                let ns = id.namespace.join(".");
                TableRef::new(ns, id.name)
            })
            .collect();

        Ok(tables)
    }

    async fn get_schema(&self, table: &TableRef) -> Result<Vec<CatalogColumn>, CatalogError> {
        let resp = self.load_table(&table.namespace, &table.name).await?;

        let schema = resp
            .metadata
            .schemas
            .into_iter()
            .find(|s| s.schema_id == resp.metadata.current_schema_id)
            .ok_or_else(|| {
                CatalogError::Internal(format!(
                    "no current schema found for {}",
                    table.fully_qualified()
                ))
            })?;

        let columns = schema
            .fields
            .into_iter()
            .map(|f| {
                let data_type = iceberg_type_to_datatype(&f.r#type);
                CatalogColumn::new(f.name, data_type, !f.required)
            })
            .collect();

        Ok(columns)
    }

    async fn table_exists(&self, table: &TableRef) -> Result<bool, CatalogError> {
        // HEAD request or try loading.
        match self.load_table(&table.namespace, &table.name).await {
            Ok(_) => Ok(true),
            Err(CatalogError::TableNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ============================================================================
// Iceberg REST API response types
// ============================================================================

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Token lifetime in seconds (per OAuth2 spec).
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ListTablesResponse {
    identifiers: Vec<TableIdentifier>,
}

#[derive(Debug, Deserialize)]
struct TableIdentifier {
    namespace: Vec<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct LoadTableResponse {
    metadata: TableMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct TableMetadata {
    current_schema_id: i32,
    #[serde(default)]
    schemas: Vec<IcebergSchema>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct IcebergSchema {
    schema_id: i32,
    fields: Vec<IcebergField>,
}

#[derive(Debug, Deserialize)]
struct IcebergField {
    name: String,
    r#type: serde_json::Value,
    required: bool,
}

// ============================================================================
// Helpers
// ============================================================================

/// Encode a dotted namespace for the Iceberg REST URL path.
/// `a.b` → `a%1Fb` (unit separator encoding per Iceberg REST spec).
fn encode_namespace(namespace: &str) -> String {
    namespace.replace('.', "%1F")
}

/// Convert an Iceberg type (JSON value) to a semstrait-core DataType.
fn iceberg_type_to_datatype(type_val: &serde_json::Value) -> DataType {
    match type_val.as_str() {
        Some("boolean") => DataType::Boolean,
        Some("int") => DataType::Int32,
        Some("long") => DataType::Int64,
        Some("float") => DataType::Float32,
        Some("double") => DataType::Float64,
        Some("string") => DataType::Utf8,
        Some("date") => DataType::Date32,
        Some("timestamp" | "timestamptz") => DataType::TimestampMicrosecond,
        Some("binary") => DataType::Binary,
        Some(s) if s.starts_with("decimal") => DataType::Float64, // simplified
        Some(s) if s.starts_with("fixed") => DataType::Binary,
        _ => {
            // Complex types (struct, list, map) → treat as Utf8 for v1.
            DataType::Utf8
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_namespace() {
        assert_eq!(encode_namespace("sales"), "sales");
        assert_eq!(encode_namespace("prod.sales"), "prod%1Fsales");
        assert_eq!(
            encode_namespace("catalog.db.schema"),
            "catalog%1Fdb%1Fschema"
        );
    }

    #[test]
    fn test_iceberg_type_to_datatype() {
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("boolean")),
            DataType::Boolean
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("int")),
            DataType::Int32
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("long")),
            DataType::Int64
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("string")),
            DataType::Utf8
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("double")),
            DataType::Float64
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("date")),
            DataType::Date32
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("timestamp")),
            DataType::TimestampMicrosecond
        );
    }

    #[test]
    fn test_builder_pattern() {
        let catalog = IcebergRestCatalog::new("https://polaris.example.com")
            .with_warehouse("my_warehouse")
            .with_prefix("prefix")
            .with_bearer_token("token123");

        assert_eq!(catalog.base_url(), "https://polaris.example.com");
        assert_eq!(catalog.warehouse.as_deref(), Some("my_warehouse"));
        assert_eq!(catalog.prefix.as_deref(), Some("prefix"));
        assert!(matches!(catalog.auth, AuthConfig::BearerToken(_)));
    }

    #[test]
    fn test_api_path() {
        let catalog = IcebergRestCatalog::new("https://host:8181");
        assert_eq!(catalog.api_path(), "https://host:8181/v1");

        let catalog = IcebergRestCatalog::new("https://host:8181").with_prefix("my-catalog");
        assert_eq!(catalog.api_path(), "https://host:8181/v1/my-catalog");
    }

    #[test]
    fn test_parse_list_tables_response() {
        let json = serde_json::json!({
            "identifiers": [
                {"namespace": ["default"], "name": "orders"},
                {"namespace": ["default"], "name": "customers"}
            ]
        });
        let resp: ListTablesResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.identifiers.len(), 2);
        assert_eq!(resp.identifiers[0].name, "orders");
    }

    #[test]
    fn test_parse_table_metadata() {
        let json = serde_json::json!({
            "metadata": {
                "current-schema-id": 0,
                "schemas": [{
                    "schema-id": 0,
                    "fields": [
                        {"id": 1, "name": "id", "type": "long", "required": true},
                        {"id": 2, "name": "name", "type": "string", "required": false},
                        {"id": 3, "name": "amount", "type": "double", "required": false}
                    ]
                }]
            }
        });
        let resp: LoadTableResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.metadata.schemas.len(), 1);
        assert_eq!(resp.metadata.schemas[0].fields.len(), 3);
        assert_eq!(resp.metadata.schemas[0].fields[0].name, "id");
    }
}
