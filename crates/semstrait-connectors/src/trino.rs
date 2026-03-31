//! Trino compute connector.
//!
//! Executes SQL queries via Trino's REST v1/statement API, returning
//! results as JSON rows. Supports Basic and Bearer token authentication.

use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;

use crate::payload::{
    ComputeResult, ComputeResultData, ConnectorError, ExecutionStats,
};
use crate::traits::ComputeConnector;
use semstrait_adapter::sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use semstrait_adapter::{AdaptError, EngineAdapter};
use semstrait_ir::{LogicalPlan, PlanArtifact};

/// Trino adapter — produces SQL with ANSI dialect.
///
/// Self-contained within the connector crate. Trino uses FETCH FIRST N ROWS ONLY
/// (ANSI standard).
struct TrinoAdapter;

impl EngineAdapter for TrinoAdapter {
    fn name(&self) -> &str { "trino" }

    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        let emitter = AnsiSqlEmitter::new(AnsiDialect);
        let sql = emitter.emit(plan).map_err(|e| AdaptError::SqlEmission(e.to_string()))?;
        Ok(PlanArtifact::Sql(sql))
    }
}

/// Authentication configuration for Trino.
#[derive(Debug, Clone)]
pub enum TrinoAuth {
    /// No authentication.
    None,
    /// HTTP Basic auth (username + optional password).
    Basic { username: String, password: Option<String> },
    /// Bearer token (JWT).
    BearerToken(String),
}

/// Trino REST API connector.
///
/// Submits SQL via POST to `/v1/statement`, polls `nextUri` until complete,
/// and collects all result pages into JSON rows.
pub struct TrinoConnector {
    base_url: String,
    catalog: String,
    schema: String,
    client: Client,
    auth: TrinoAuth,
    user: String,
    adapter: TrinoAdapter,
}

impl TrinoConnector {
    /// Create a new Trino connector.
    ///
    /// `base_url` should be the Trino coordinator URL (e.g., `http://trino:8080`).
    pub fn new(
        base_url: impl Into<String>,
        catalog: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            catalog: catalog.into(),
            schema: schema.into(),
            client: Client::new(),
            auth: TrinoAuth::None,
            user: "semstrait".to_string(),
            adapter: TrinoAdapter,
        }
    }

    /// Set the Trino user name (sent via X-Trino-User header).
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = user.into();
        self
    }

    /// Set Basic authentication.
    pub fn with_basic_auth(
        mut self,
        username: impl Into<String>,
        password: Option<String>,
    ) -> Self {
        self.auth = TrinoAuth::Basic {
            username: username.into(),
            password,
        };
        self
    }

    /// Set Bearer token authentication.
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = TrinoAuth::BearerToken(token.into());
        self
    }

    /// Submit a SQL statement and collect all result pages.
    async fn execute_statement(&self, sql: &str) -> Result<TrinoQueryResult, ConnectorError> {
        let url = format!("{}/v1/statement", self.base_url);
        let start = Instant::now();

        let mut req = self.client.post(&url)
            .header("X-Trino-User", &self.user)
            .header("X-Trino-Catalog", &self.catalog)
            .header("X-Trino-Schema", &self.schema)
            .body(sql.to_string());

        req = self.apply_auth(req);

        let resp = req.send().await
            .map_err(|e| ConnectorError::Connection(format!("failed to submit statement: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::Execution(format!("HTTP {status}: {body}")));
        }

        let mut response: TrinoResponse = resp.json().await
            .map_err(|e| ConnectorError::Execution(format!("failed to parse response: {e}")))?;

        // Collect all columns and data rows from paginated results.
        let columns = response.columns.unwrap_or_default();
        let mut all_rows: Vec<Vec<serde_json::Value>> = response.data.unwrap_or_default();

        // Poll nextUri until results are complete.
        while let Some(next_uri) = response.next_uri {
            // Brief pause to avoid hammering the coordinator.
            tokio::time::sleep(Duration::from_millis(100)).await;

            let mut req = self.client.get(&next_uri)
                .header("X-Trino-User", &self.user);
            req = self.apply_auth(req);

            let resp = req.send().await
                .map_err(|e| ConnectorError::Connection(format!("poll failed: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(ConnectorError::Execution(format!("poll HTTP {status}: {body}")));
            }

            response = resp.json().await
                .map_err(|e| ConnectorError::Execution(format!("failed to parse poll response: {e}")))?;

            if let Some(ref error) = response.error {
                return Err(ConnectorError::Execution(format!(
                    "Trino error {}: {}",
                    error.error_code,
                    error.message
                )));
            }

            if let Some(data) = response.data {
                all_rows.extend(data);
            }
        }

        let elapsed = start.elapsed();

        Ok(TrinoQueryResult {
            columns,
            rows: all_rows,
            elapsed,
        })
    }

    /// Apply authentication to a request builder.
    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            TrinoAuth::None => req,
            TrinoAuth::Basic { username, password } => {
                req.basic_auth(username, password.as_deref())
            }
            TrinoAuth::BearerToken(token) => req.bearer_auth(token),
        }
    }
}

// ── Trino REST API response types ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrinoResponse {
    #[serde(default)]
    columns: Option<Vec<TrinoColumn>>,
    #[serde(default)]
    data: Option<Vec<Vec<serde_json::Value>>>,
    #[serde(default)]
    next_uri: Option<String>,
    #[serde(default)]
    error: Option<TrinoError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrinoColumn {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrinoError {
    #[serde(default)]
    error_code: i32,
    message: String,
}

/// Collected result from a Trino query.
struct TrinoQueryResult {
    columns: Vec<TrinoColumn>,
    rows: Vec<Vec<serde_json::Value>>,
    elapsed: Duration,
}

impl TrinoQueryResult {
    /// Convert positional row arrays into named JSON objects.
    fn to_json_rows(&self) -> Vec<serde_json::Value> {
        self.rows
            .iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (i, col) in self.columns.iter().enumerate() {
                    let val = row.get(i).cloned().unwrap_or(serde_json::Value::Null);
                    obj.insert(col.name.clone(), val);
                }
                serde_json::Value::Object(obj)
            })
            .collect()
    }
}

// ── ComputeConnector ────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl ComputeConnector for TrinoConnector {
    fn adapter(&self) -> &dyn EngineAdapter {
        &self.adapter
    }

    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError> {
        let sql = artifact.as_sql().ok_or_else(|| {
            ConnectorError::Execution(
                "Trino connector currently requires SQL artifact".to_string(),
            )
        })?;

        let result = self.execute_statement(sql).await?;
        let json_rows = result.to_json_rows();
        let row_count = json_rows.len() as u64;

        Ok(ComputeResult {
            complete: true,
            stats: ExecutionStats {
                rows_returned: row_count,
                execution_time: Some(result.elapsed),
                bytes_scanned: None,
            },
            data: ComputeResultData::Json(json_rows),
        })
    }

    async fn health_check(&self) -> Result<(), ConnectorError> {
        self.execute_statement("SELECT 1").await?;
        Ok(())
    }

    fn name(&self) -> &str {
        "trino"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern() {
        let conn = TrinoConnector::new("http://trino:8080", "hive", "default")
            .with_user("alice")
            .with_bearer_token("token123");

        assert_eq!(conn.name(), "trino");
        assert_eq!(conn.base_url, "http://trino:8080");
        assert_eq!(conn.catalog, "hive");
        assert_eq!(conn.schema, "default");
        assert_eq!(conn.user, "alice");
        assert!(matches!(conn.auth, TrinoAuth::BearerToken(_)));
    }

    #[test]
    fn test_adapter_accessible() {
        let conn = TrinoConnector::new("http://trino:8080", "hive", "default");
        let adapter = conn.adapter();
        assert_eq!(adapter.name(), "trino");
    }

    #[test]
    fn test_trino_result_to_json_rows() {
        let result = TrinoQueryResult {
            columns: vec![
                TrinoColumn { name: "id".to_string() },
                TrinoColumn { name: "name".to_string() },
            ],
            rows: vec![
                vec![serde_json::json!(1), serde_json::json!("alice")],
                vec![serde_json::json!(2), serde_json::json!("bob")],
            ],
            elapsed: Duration::from_millis(42),
        };

        let rows = result.to_json_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], 1);
        assert_eq!(rows[0]["name"], "alice");
        assert_eq!(rows[1]["id"], 2);
        assert_eq!(rows[1]["name"], "bob");
    }

    #[test]
    fn test_parse_trino_response() {
        let json = serde_json::json!({
            "columns": [
                {"name": "x", "type": "integer"}
            ],
            "data": [[42]],
            "stats": {"state": "FINISHED"}
        });
        let resp: TrinoResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.columns.unwrap().len(), 1);
        assert_eq!(resp.data.unwrap().len(), 1);
        assert!(resp.next_uri.is_none());
    }

    #[test]
    fn test_parse_trino_error_response() {
        let json = serde_json::json!({
            "error": {
                "errorCode": 1,
                "errorName": "SYNTAX_ERROR",
                "message": "line 1:1: mismatched input"
            }
        });
        let resp: TrinoResponse = serde_json::from_value(json).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.error_code, 1);
        assert!(err.message.contains("mismatched"));
    }
}
