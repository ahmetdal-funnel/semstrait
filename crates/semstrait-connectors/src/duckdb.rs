//! DuckDB compute connector.
//!
//! Executes SQL queries via DuckDB's embedded engine, returning
//! Arrow `RecordBatch`es converted to JSON rows.
//!
//! `Connection` is `Send` but `!Sync`, so all database operations
//! are dispatched through `tokio::task::spawn_blocking` with a
//! `tokio::sync::Mutex` guard.

use std::sync::Arc;
use std::time::Instant;

use duckdb_engine::arrow::array::RecordBatch;
use duckdb_engine::Connection;
use tokio::sync::Mutex;

use crate::payload::{
    ComputeResult, ComputeResultData, ConnectorError, ExecutionStats,
};
use crate::traits::ComputeConnector;
use semstrait_adapter::{DuckDbAdapter, EngineAdapter};
use semstrait_ir::PlanArtifact;

/// DuckDB-based compute connector.
///
/// Wraps an in-memory (or file-backed) DuckDB `Connection` behind a
/// `tokio::sync::Mutex` for safe async access. All blocking DuckDB calls
/// are dispatched via `spawn_blocking`.
pub struct DuckDbConnector {
    conn: Arc<Mutex<Connection>>,
    adapter: DuckDbAdapter,
}

impl DuckDbConnector {
    /// Create a new connector with an in-memory DuckDB database.
    pub fn new() -> Result<Self, ConnectorError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ConnectorError::Connection(format!("failed to open DuckDB: {e}")))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            adapter: DuckDbAdapter,
        })
    }

    /// Create a connector backed by a file-based DuckDB database.
    pub fn with_path(path: &str) -> Result<Self, ConnectorError> {
        let conn = Connection::open(path)
            .map_err(|e| ConnectorError::Connection(format!("failed to open DuckDB at '{path}': {e}")))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            adapter: DuckDbAdapter,
        })
    }

    /// Execute a batch SQL statement (DDL, DML) against the DuckDB connection.
    ///
    /// Use `prepare` + `execute` (single statement) to prevent multi-statement injection.
    pub async fn execute_sql(&self, sql: &str) -> Result<(), ConnectorError> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.prepare(&sql)
                .and_then(|mut stmt| stmt.execute([]))
                .map(|_| ())
                .map_err(|e| ConnectorError::Execution(e.to_string()))
        })
        .await
        .map_err(|e| ConnectorError::Internal(format!("spawn_blocking panicked: {e}")))?
    }

    /// Register a CSV file as a named table.
    ///
    /// Uses DuckDB's `read_csv_auto()` with validated identifiers to prevent injection.
    pub async fn register_csv(&self, table_name: &str, path: &str) -> Result<(), ConnectorError> {
        let sql = build_register_sql(table_name, path, "read_csv_auto")?;
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.prepare(&sql)
                .and_then(|mut stmt| stmt.execute([]))
                .map_err(|e| ConnectorError::Execution(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| ConnectorError::Internal(format!("spawn_blocking panicked: {e}")))?
    }

    /// Register a Parquet file as a named table.
    pub async fn register_parquet(
        &self,
        table_name: &str,
        path: &str,
    ) -> Result<(), ConnectorError> {
        let sql = build_register_sql(table_name, path, "read_parquet")?;
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.prepare(&sql)
                .and_then(|mut stmt| stmt.execute([]))
                .map_err(|e| ConnectorError::Execution(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| ConnectorError::Internal(format!("spawn_blocking panicked: {e}")))?
    }

    /// Register a file, auto-detecting format by extension (.csv or .parquet).
    pub async fn register_file(&self, table_name: &str, path: &str) -> Result<(), ConnectorError> {
        if path.ends_with(".parquet") || path.ends_with(".parq") {
            self.register_parquet(table_name, path).await
        } else if path.ends_with(".csv") || path.ends_with(".tsv") {
            self.register_csv(table_name, path).await
        } else {
            Err(ConnectorError::Execution(format!(
                "unsupported file format for '{}': expected .csv or .parquet",
                path
            )))
        }
    }
}

/// Build a `CREATE TABLE ... AS SELECT * FROM reader('path')` statement
/// with validated table name and path to prevent SQL injection.
///
/// Uses `prepare` + `execute` (single-statement) instead of `execute_batch`
/// (multi-statement) to block injection via semicolons.
fn build_register_sql(
    table_name: &str,
    path: &str,
    reader_fn: &str,
) -> Result<String, ConnectorError> {
    // Validate table name: reject control characters and semicolons.
    if table_name.is_empty()
        || table_name
            .chars()
            .any(|c| c.is_control() || c == ';')
    {
        return Err(ConnectorError::Execution(format!(
            "invalid table name: '{table_name}'"
        )));
    }
    // Validate path: reject control characters and semicolons.
    if path.is_empty()
        || path.chars().any(|c| c.is_control() || c == ';')
    {
        return Err(ConnectorError::Execution(format!(
            "invalid file path: '{path}'"
        )));
    }

    Ok(format!(
        "CREATE TABLE \"{}\" AS SELECT * FROM {}('{}')",
        table_name.replace('"', "\"\""),
        reader_fn,
        path.replace('\'', "''")
    ))
}

#[async_trait::async_trait]
impl ComputeConnector for DuckDbConnector {
    fn adapter(&self) -> &dyn EngineAdapter {
        &self.adapter
    }

    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError> {
        let sql = artifact.as_sql().ok_or_else(|| {
            ConnectorError::Execution(
                "DuckDB connector currently requires SQL artifact".to_string(),
            )
        })?;

        let conn = self.conn.clone();
        let sql = sql.to_string();
        let start = Instant::now();

        let batches = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ConnectorError::Execution(e.to_string()))?;
            let batches: Vec<RecordBatch> = stmt
                .query_arrow([])
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
                .collect();
            Ok::<_, ConnectorError>(batches)
        })
        .await
        .map_err(|e| ConnectorError::Internal(format!("spawn_blocking panicked: {e}")))??;

        let elapsed = start.elapsed();
        let rows_returned: u64 = batches.iter().map(|b| b.num_rows() as u64).sum();

        let json_rows = arrow_batches_to_json(&batches)?;

        Ok(ComputeResult {
            complete: true,
            stats: ExecutionStats {
                rows_returned,
                execution_time: Some(elapsed),
                bytes_scanned: None,
            },
            data: ComputeResultData::Json(json_rows),
        })
    }

    async fn health_check(&self) -> Result<(), ConnectorError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let val: i32 = conn
                .query_row("SELECT 1", [], |row| row.get(0))
                .map_err(|e| ConnectorError::Connection(e.to_string()))?;
            if val != 1 {
                return Err(ConnectorError::Connection(
                    "health check returned unexpected value".to_string(),
                ));
            }
            Ok(())
        })
        .await
        .map_err(|e| ConnectorError::Internal(format!("spawn_blocking panicked: {e}")))?
    }

    fn name(&self) -> &str {
        "duckdb"
    }
}

/// Convert Arrow RecordBatches to JSON rows using the workspace arrow crate's JSON writer.
///
/// DuckDB 1.3.x re-exports arrow 55 — the same version as our workspace — so
/// `duckdb_engine::arrow::array::RecordBatch` and `arrow::array::RecordBatch`
/// are the same type.
fn arrow_batches_to_json(
    batches: &[RecordBatch],
) -> Result<Vec<serde_json::Value>, ConnectorError> {
    use arrow::json::ArrayWriter;

    if batches.is_empty() {
        return Ok(vec![]);
    }

    let mut buf = Vec::new();
    let mut writer = ArrayWriter::new(&mut buf);
    let batch_refs: Vec<&RecordBatch> = batches.iter().collect();
    writer
        .write_batches(&batch_refs)
        .map_err(|e| ConnectorError::Internal(format!("Arrow JSON serialization failed: {e}")))?;
    writer
        .finish()
        .map_err(|e| ConnectorError::Internal(format!("Arrow JSON writer finish failed: {e}")))?;
    drop(writer);

    let rows: Vec<serde_json::Value> = serde_json::from_slice(&buf)
        .map_err(|e| ConnectorError::Internal(format!("JSON parse failed: {e}")))?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: set up a connector with a test table.
    async fn setup_with_orders() -> DuckDbConnector {
        let connector = DuckDbConnector::new().unwrap();
        connector
            .execute_sql("CREATE TABLE orders (region TEXT, amount DOUBLE, order_id INTEGER)")
            .await
            .unwrap();
        connector
            .execute_sql("INSERT INTO orders VALUES ('US', 100.0, 1), ('EU', 200.0, 2), ('US', 150.0, 3), ('EU', 50.0, 4)")
            .await
            .unwrap();
        connector
    }

    #[tokio::test]
    async fn test_health_check() {
        let connector = DuckDbConnector::new().unwrap();
        let result = connector.health_check().await;
        assert!(result.is_ok(), "health check should pass: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_execute_simple_query() {
        let connector = setup_with_orders().await;

        let artifact = PlanArtifact::Sql(
            "SELECT region, SUM(amount) as total FROM orders GROUP BY region ORDER BY region"
                .to_string(),
        );
        let result = connector.execute(&artifact).await.unwrap();

        assert!(result.complete);
        assert_eq!(result.stats.rows_returned, 2);

        match &result.data {
            ComputeResultData::Json(rows) => {
                assert_eq!(rows.len(), 2, "should have 2 JSON rows");
                assert!(rows[0].is_object(), "each row should be a JSON object");
                assert!(rows[0].get("region").is_some(), "row should have 'region' key");
                assert!(rows[0].get("total").is_some(), "row should have 'total' key");
            }
            other => panic!("expected Json result data, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_with_filter() {
        let connector = setup_with_orders().await;

        let artifact = PlanArtifact::Sql(
            "SELECT region, amount FROM orders WHERE region = 'US'".to_string(),
        );
        let result = connector.execute(&artifact).await.unwrap();

        assert_eq!(result.stats.rows_returned, 2);
    }

    #[tokio::test]
    async fn test_execute_empty_result() {
        let connector = DuckDbConnector::new().unwrap();
        connector
            .execute_sql("CREATE TABLE empty_table (id INTEGER, name TEXT)")
            .await
            .unwrap();

        let artifact = PlanArtifact::Sql("SELECT * FROM empty_table".to_string());
        let result = connector.execute(&artifact).await.unwrap();

        assert!(result.complete);
        assert_eq!(result.stats.rows_returned, 0);
        match &result.data {
            ComputeResultData::Json(rows) => assert!(rows.is_empty()),
            other => panic!("expected Json result data, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_name() {
        let connector = DuckDbConnector::new().unwrap();
        assert_eq!(connector.name(), "duckdb");
    }

    #[tokio::test]
    async fn test_adapter_accessible() {
        let connector = DuckDbConnector::new().unwrap();
        let adapter = connector.adapter();
        assert_eq!(adapter.name(), "duckdb");
    }

    #[tokio::test]
    async fn test_execution_error() {
        let connector = DuckDbConnector::new().unwrap();
        let artifact = PlanArtifact::Sql("SELECT * FROM nonexistent_table".to_string());
        let result = connector.execute(&artifact).await;
        assert!(result.is_err());
        match result {
            Err(ConnectorError::Execution(msg)) => {
                assert!(msg.contains("nonexistent_table"), "error should mention table: {msg}");
            }
            other => panic!("expected Execution error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_register_rejects_semicolons_in_path() {
        let connector = DuckDbConnector::new().unwrap();
        let result = connector.register_csv("test", "data.csv; DROP TABLE x").await;
        assert!(result.is_err(), "path with semicolons should be rejected");
    }

    #[tokio::test]
    async fn test_register_rejects_empty_table_name() {
        let connector = DuckDbConnector::new().unwrap();
        let result = connector.register_csv("", "data.csv").await;
        assert!(result.is_err(), "empty table name should be rejected");
    }
}
