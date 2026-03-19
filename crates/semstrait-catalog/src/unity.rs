//! Databricks Unity Catalog implementation.
//!
//! Implements the [Unity Catalog REST API](https://docs.databricks.com/api/workspace/tables)
//! for namespace listing, table discovery, and schema retrieval.
//!
//! Compatible with Databricks workspace and Unity Catalog OSS.

use crate::{CatalogColumn, CatalogError, CatalogProvider, TableRef};
use async_trait::async_trait;
use reqwest::Client;
use semstrait_core::{DataType, GlobPattern};
use serde::Deserialize;

/// Databricks Unity Catalog REST client.
///
/// Queries the Unity Catalog API for table metadata and schema information.
/// Requires a Databricks workspace URL and personal access token (PAT) or
/// OAuth2 token.
#[derive(Debug, Clone)]
pub struct UnityCatalogProvider {
    host: String,
    catalog_name: String,
    client: Client,
    auth: UnityAuth,
}

/// Authentication for Unity Catalog.
#[derive(Debug, Clone)]
pub enum UnityAuth {
    /// Databricks personal access token (PAT).
    Pat(String),
    /// OAuth2 bearer token.
    BearerToken(String),
}

impl UnityCatalogProvider {
    /// Create a new Unity Catalog provider.
    ///
    /// `host` is the Databricks workspace URL (e.g., `https://dbc-xxx.cloud.databricks.com`).
    /// `catalog_name` is the Unity Catalog name (e.g., `main`).
    pub fn new(
        host: impl Into<String>,
        catalog_name: impl Into<String>,
        auth: UnityAuth,
    ) -> Self {
        Self {
            host: host.into(),
            catalog_name: catalog_name.into(),
            client: Client::new(),
            auth,
        }
    }

    /// Returns the workspace host URL.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the catalog name.
    pub fn catalog_name(&self) -> &str {
        &self.catalog_name
    }

    /// Make an authenticated GET request with optional query parameters.
    async fn get(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, CatalogError> {
        let token = match &self.auth {
            UnityAuth::Pat(pat) => pat.clone(),
            UnityAuth::BearerToken(token) => token.clone(),
        };

        let resp = self
            .client
            .get(url)
            .query(query)
            .bearer_auth(token)
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

    /// List tables in a schema via the Unity Catalog API.
    async fn list_tables_raw(
        &self,
        schema_name: &str,
    ) -> Result<Vec<UnityTableInfo>, CatalogError> {
        let url = format!("{}/api/2.1/unity-catalog/tables", self.host);

        let mut all_tables = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut query: Vec<(&str, &str)> = vec![
                ("catalog_name", &self.catalog_name),
                ("schema_name", schema_name),
            ];
            if let Some(token) = &page_token {
                query.push(("page_token", token));
            }

            let resp = self.get(&url, &query).await?;
            let body: ListTablesResponse = resp
                .json()
                .await
                .map_err(|e| CatalogError::Internal(format!("failed to parse tables: {e}")))?;

            all_tables.extend(body.tables.unwrap_or_default());

            match body.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        Ok(all_tables)
    }

    /// Get table metadata for a specific table.
    async fn get_table(
        &self,
        schema_name: &str,
        table_name: &str,
    ) -> Result<UnityTableInfo, CatalogError> {
        let full_name = format!("{}.{}.{}", self.catalog_name, schema_name, table_name);
        let url = format!(
            "{}/api/2.1/unity-catalog/tables/{}",
            self.host, full_name
        );

        let resp = self.get(&url, &[]).await?;
        resp.json()
            .await
            .map_err(|e| CatalogError::Internal(format!("failed to parse table: {e}")))
    }
}

#[async_trait]
impl CatalogProvider for UnityCatalogProvider {
    async fn list_tables(
        &self,
        namespace: &str,
        pattern: &GlobPattern,
    ) -> Result<Vec<TableRef>, CatalogError> {
        let tables = self.list_tables_raw(namespace).await?;

        let matched: Vec<TableRef> = tables
            .into_iter()
            .filter(|t| pattern.matches(&t.name))
            .map(|t| {
                TableRef::with_catalog(&self.catalog_name, &t.schema_name, &t.name)
            })
            .collect();

        Ok(matched)
    }

    async fn get_schema(&self, table: &TableRef) -> Result<Vec<CatalogColumn>, CatalogError> {
        let info = self.get_table(&table.namespace, &table.name).await?;

        let columns = info
            .columns
            .unwrap_or_default()
            .into_iter()
            .map(|c| {
                let data_type = unity_type_to_datatype(&c.type_name);
                CatalogColumn::new(c.name, data_type, c.nullable.unwrap_or(true))
            })
            .collect();

        Ok(columns)
    }

    async fn table_exists(&self, table: &TableRef) -> Result<bool, CatalogError> {
        match self.get_table(&table.namespace, &table.name).await {
            Ok(_) => Ok(true),
            Err(CatalogError::TableNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ============================================================================
// Unity Catalog REST API response types
// ============================================================================

#[derive(Debug, Deserialize)]
struct ListTablesResponse {
    tables: Option<Vec<UnityTableInfo>>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UnityTableInfo {
    name: String,
    schema_name: String,
    #[serde(default)]
    columns: Option<Vec<UnityColumn>>,
}

#[derive(Debug, Deserialize)]
struct UnityColumn {
    name: String,
    type_name: String,
    #[serde(default)]
    nullable: Option<bool>,
}

// ============================================================================
// Type mapping
// ============================================================================

/// Convert a Unity Catalog type name to a semstrait-core DataType.
fn unity_type_to_datatype(type_name: &str) -> DataType {
    match type_name.to_uppercase().as_str() {
        "BOOLEAN" => DataType::Boolean,
        "BYTE" | "TINYINT" | "SHORT" | "SMALLINT" | "INT" | "INTEGER" => DataType::Int32,
        "LONG" | "BIGINT" => DataType::Int64,
        "FLOAT" => DataType::Float32,
        "DOUBLE" => DataType::Float64,
        "STRING" | "CHAR" | "VARCHAR" => DataType::Utf8,
        "DATE" => DataType::Date32,
        "TIMESTAMP" | "TIMESTAMP_NTZ" => DataType::TimestampMicrosecond,
        "BINARY" => DataType::Binary,
        s if s.starts_with("DECIMAL") => DataType::Float64, // simplified
        _ => DataType::Utf8, // complex types → Utf8
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unity_type_to_datatype() {
        assert_eq!(unity_type_to_datatype("BOOLEAN"), DataType::Boolean);
        assert_eq!(unity_type_to_datatype("INT"), DataType::Int32);
        assert_eq!(unity_type_to_datatype("LONG"), DataType::Int64);
        assert_eq!(unity_type_to_datatype("BIGINT"), DataType::Int64);
        assert_eq!(unity_type_to_datatype("FLOAT"), DataType::Float32);
        assert_eq!(unity_type_to_datatype("DOUBLE"), DataType::Float64);
        assert_eq!(unity_type_to_datatype("STRING"), DataType::Utf8);
        assert_eq!(unity_type_to_datatype("DATE"), DataType::Date32);
        assert_eq!(unity_type_to_datatype("TIMESTAMP"), DataType::TimestampMicrosecond);
        assert_eq!(unity_type_to_datatype("DECIMAL(10,2)"), DataType::Float64);
        assert_eq!(unity_type_to_datatype("BINARY"), DataType::Binary);
    }

    #[test]
    fn test_builder() {
        let catalog = UnityCatalogProvider::new(
            "https://dbc-xxx.cloud.databricks.com",
            "main",
            UnityAuth::Pat("dapi123".to_string()),
        );
        assert_eq!(catalog.host(), "https://dbc-xxx.cloud.databricks.com");
        assert_eq!(catalog.catalog_name(), "main");
    }

    #[test]
    fn test_parse_list_tables_response() {
        let json = serde_json::json!({
            "tables": [
                {
                    "name": "orders",
                    "catalog_name": "main",
                    "schema_name": "sales",
                    "table_type": "MANAGED",
                    "columns": [
                        {"name": "id", "type_name": "LONG", "nullable": false},
                        {"name": "amount", "type_name": "DOUBLE", "nullable": true}
                    ]
                },
                {
                    "name": "customers",
                    "catalog_name": "main",
                    "schema_name": "sales",
                    "table_type": "MANAGED"
                }
            ]
        });
        let resp: ListTablesResponse = serde_json::from_value(json).unwrap();
        let tables = resp.tables.unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].name, "orders");
        assert_eq!(tables[0].columns.as_ref().unwrap().len(), 2);
        assert!(tables[1].columns.is_none());
    }

    #[test]
    fn test_parse_table_info_with_columns() {
        let json = serde_json::json!({
            "name": "orders",
            "catalog_name": "main",
            "schema_name": "sales",
            "table_type": "MANAGED",
            "columns": [
                {"name": "id", "type_name": "LONG", "nullable": false},
                {"name": "date", "type_name": "DATE", "nullable": false},
                {"name": "amount", "type_name": "DECIMAL(10,2)", "nullable": true},
                {"name": "customer_name", "type_name": "STRING", "nullable": true}
            ]
        });
        let info: UnityTableInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.name, "orders");
        let cols = info.columns.unwrap();
        assert_eq!(cols.len(), 4);
        assert_eq!(cols[0].type_name, "LONG");
        assert_eq!(cols[2].nullable, Some(true));
    }

    #[test]
    fn test_parse_empty_list_response() {
        let json = serde_json::json!({});
        let resp: ListTablesResponse = serde_json::from_value(json).unwrap();
        assert!(resp.tables.is_none());
        assert!(resp.next_page_token.is_none());
    }
}
