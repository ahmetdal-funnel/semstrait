//! Iceberg REST catalog implementation.
//!
//! Implements the [Iceberg REST Catalog API](https://iceberg.apache.org/spec/#rest-catalog)
//! spec for namespace listing, table discovery, and schema retrieval.
//!
//! Compatible with Polaris (Snowflake), Gravitino (Apache), Tabular, and other
//! Iceberg REST catalog servers.

use crate::{CatalogColumn, CatalogError, CatalogPartitionField, CatalogProvider, TableMetadataResponse, TableRef};
use async_trait::async_trait;
use reqwest::Client;
use semstrait_core::{DataType, GlobPattern};
use serde::Deserialize;
use std::collections::HashMap;
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
    /// Custom HTTP headers injected into every request (e.g., Polaris-Realm).
    custom_headers: HashMap<String, String>,
    /// Cached OAuth2 access token with optional expiry instant.
    #[allow(clippy::type_complexity)]
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
            custom_headers: HashMap::new(),
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

    /// Add a custom HTTP header to all requests (e.g., `Polaris-Realm`).
    pub fn with_custom_header(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.custom_headers.insert(key.into(), value.into());
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
                        if expiry.is_none_or(|exp| Instant::now() < exp) {
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

                let mut token_req = self.client.post(token_url).form(&params);
                for (k, v) in &self.custom_headers {
                    token_req = token_req.header(k.as_str(), v.as_str());
                }
                let resp = token_req
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
        for (k, v) in &self.custom_headers {
            req = req.header(k.as_str(), v.as_str());
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

    async fn load_table_metadata(
        &self,
        table: &TableRef,
    ) -> Result<Option<TableMetadataResponse>, CatalogError> {
        let resp = self.load_table(&table.namespace, &table.name).await?;
        let meta = &resp.metadata;

        // Find current schema.
        let schema = meta
            .schemas
            .iter()
            .find(|s| s.schema_id == meta.current_schema_id)
            .ok_or_else(|| {
                CatalogError::Internal(format!(
                    "no current schema found for {}",
                    table.fully_qualified()
                ))
            })?;

        // Build field_id → column_name map for partition spec resolution.
        let field_id_map: HashMap<i32, &str> = schema
            .fields
            .iter()
            .filter_map(|f| f.id.map(|id| (id, f.name.as_str())))
            .collect();

        // Convert schema fields to CatalogColumn.
        let columns: Vec<CatalogColumn> = schema
            .fields
            .iter()
            .map(|f| {
                let data_type = iceberg_type_to_datatype(&f.r#type);
                CatalogColumn::new(f.name.clone(), data_type, !f.required)
            })
            .collect();

        // Resolve partition spec: use default_spec_id or first spec.
        let partition_fields = if let Some(spec) = meta
            .partition_specs
            .iter()
            .find(|s| Some(s.spec_id) == meta.default_spec_id)
            .or(meta.partition_specs.first())
        {
            spec.fields
                .iter()
                .filter_map(|pf| {
                    let source_column = field_id_map.get(&pf.source_id)?;
                    Some(CatalogPartitionField {
                        source_column: source_column.to_string(),
                        transform: pf.transform.clone(),
                        name: pf.name.clone(),
                        field_id: pf.field_id,
                    })
                })
                .collect()
        } else {
            vec![]
        };

        Ok(Some(TableMetadataResponse {
            columns,
            partition_fields,
            snapshot_id: meta.current_snapshot_id,
            format_version: meta.format_version,
            location: meta.location.clone(),
            format: Some(semstrait_core::DataFormat::Iceberg),
            properties: meta.properties.clone().unwrap_or_default(),
        }))
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
    /// Current snapshot ID from the Iceberg table.
    #[serde(default)]
    current_snapshot_id: Option<i64>,
    /// Partition specs (Iceberg v1/v2).
    #[serde(default)]
    partition_specs: Vec<IcebergPartitionSpec>,
    /// Default partition spec ID.
    #[serde(default)]
    default_spec_id: Option<i32>,
    /// Iceberg format version (1 or 2).
    #[serde(default)]
    format_version: Option<u32>,
    /// Physical table location (e.g., S3 URI).
    #[serde(default)]
    location: Option<String>,
    /// Table properties from Iceberg metadata.
    #[serde(default)]
    properties: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct IcebergSchema {
    schema_id: i32,
    fields: Vec<IcebergField>,
}

#[derive(Debug, Deserialize)]
struct IcebergField {
    /// Iceberg field ID (used for partition spec resolution).
    #[serde(default)]
    id: Option<i32>,
    name: String,
    r#type: serde_json::Value,
    required: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct IcebergPartitionSpec {
    spec_id: i32,
    fields: Vec<IcebergPartitionField>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct IcebergPartitionField {
    source_id: i32,
    field_id: i32,
    name: String,
    transform: String,
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
        Some("int" | "long") => DataType::Integer,
        Some("float" | "double") => DataType::Number,
        Some("string") => DataType::String,
        Some("date") => DataType::Date,
        Some("timestamp" | "timestamptz") => DataType::Timestamp { precision: 6 },
        Some("binary") => DataType::Binary,
        Some(s) if s.starts_with("decimal") => DataType::Number, // simplified
        Some(s) if s.starts_with("fixed") => DataType::Binary,
        _ => {
            // Complex types (struct, list, map) → treat as String for v1.
            DataType::String
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
            DataType::Integer
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("long")),
            DataType::Integer
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("string")),
            DataType::String
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("double")),
            DataType::Number
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("date")),
            DataType::Date
        );
        assert_eq!(
            iceberg_type_to_datatype(&serde_json::json!("timestamp")),
            DataType::Timestamp { precision: 6 }
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

    #[test]
    fn test_parse_table_metadata_with_partitions() {
        let json = serde_json::json!({
            "metadata": {
                "format-version": 2,
                "current-schema-id": 0,
                "schemas": [{
                    "schema-id": 0,
                    "fields": [
                        {"id": 1, "name": "id", "type": "long", "required": true},
                        {"id": 2, "name": "order_date", "type": "timestamp", "required": true},
                        {"id": 3, "name": "amount", "type": "double", "required": false},
                        {"id": 4, "name": "region", "type": "string", "required": false}
                    ]
                }],
                "current-snapshot-id": 3497810964824022504_i64,
                "default-spec-id": 0,
                "partition-specs": [{
                    "spec-id": 0,
                    "fields": [
                        {"source-id": 2, "field-id": 1000, "name": "order_date_day", "transform": "day"},
                        {"source-id": 4, "field-id": 1001, "name": "region_identity", "transform": "identity"}
                    ]
                }],
                "location": "s3://warehouse/db/orders",
                "properties": {
                    "write.format.default": "parquet",
                    "write.parquet.compression-codec": "zstd"
                }
            }
        });
        let resp: LoadTableResponse = serde_json::from_value(json).unwrap();
        let meta = &resp.metadata;
        assert_eq!(meta.format_version, Some(2));
        assert_eq!(meta.current_snapshot_id, Some(3497810964824022504));
        assert_eq!(meta.location, Some("s3://warehouse/db/orders".to_string()));
        assert_eq!(meta.default_spec_id, Some(0));

        // Partition spec
        assert_eq!(meta.partition_specs.len(), 1);
        let spec = &meta.partition_specs[0];
        assert_eq!(spec.spec_id, 0);
        assert_eq!(spec.fields.len(), 2);
        assert_eq!(spec.fields[0].source_id, 2);
        assert_eq!(spec.fields[0].transform, "day");
        assert_eq!(spec.fields[0].name, "order_date_day");
        assert_eq!(spec.fields[1].transform, "identity");

        // Properties
        let props = meta.properties.as_ref().unwrap();
        assert_eq!(props.get("write.format.default"), Some(&"parquet".to_string()));

        // Field IDs on schema
        assert_eq!(meta.schemas[0].fields[0].id, Some(1));
        assert_eq!(meta.schemas[0].fields[1].id, Some(2));
    }

    #[test]
    fn test_parse_table_metadata_without_partitions() {
        // Minimal metadata — only schema, no partition specs or snapshot.
        let json = serde_json::json!({
            "metadata": {
                "current-schema-id": 0,
                "schemas": [{
                    "schema-id": 0,
                    "fields": [
                        {"id": 1, "name": "id", "type": "long", "required": true}
                    ]
                }]
            }
        });
        let resp: LoadTableResponse = serde_json::from_value(json).unwrap();
        let meta = &resp.metadata;
        assert_eq!(meta.current_snapshot_id, None);
        assert_eq!(meta.partition_specs.len(), 0);
        assert_eq!(meta.format_version, None);
        assert_eq!(meta.location, None);
    }
}
