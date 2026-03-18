//! DataFusion compute connector.
//!
//! Executes SQL queries via DataFusion's `SessionContext`, returning
//! Arrow `RecordBatch`es wrapped in `ComputeResultData::Native`.

use std::sync::Arc;
use std::time::Instant;

use datafusion_engine::arrow::array::RecordBatch;
use datafusion_engine::prelude::*;

use crate::payload::{
    AdaptError, ComputePayload, ComputeRequest, ComputeResult, ComputeResultData,
    ConnectorError, EmitError, ExecutionStats, PayloadKind,
};
use crate::traits::{ComputeAdapter, ComputeConnector, ComputeEmitter};
use semstrait_core::ConsumerProfile;

/// Arrow RecordBatch results, extractable from `ComputeResultData::Native`.
pub struct ArrowBatches(pub Vec<RecordBatch>);

impl ArrowBatches {
    /// Convert Arrow record batches to JSON rows.
    ///
    /// Uses Arrow's built-in JSON writer to produce one `serde_json::Value`
    /// per row (as a JSON object with column names as keys).
    pub fn to_json_rows(&self) -> Result<Vec<serde_json::Value>, ConnectorError> {
        use datafusion_engine::arrow::json::ArrayWriter;

        let mut buf = Vec::new();
        let mut writer = ArrayWriter::new(&mut buf);
        let batch_refs: Vec<&RecordBatch> = self.0.iter().collect();
        writer
            .write_batches(&batch_refs)
            .map_err(|e| ConnectorError::Internal(format!("Arrow JSON serialization failed: {}", e)))?;
        writer
            .finish()
            .map_err(|e| ConnectorError::Internal(format!("Arrow JSON writer finish failed: {}", e)))?;
        drop(writer);

        // ArrayWriter produces a JSON array like [{...}, {...}, ...].
        // Parse it into Vec<serde_json::Value>.
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&buf)
            .map_err(|e| ConnectorError::Internal(format!("JSON parse failed: {}", e)))?;

        Ok(rows)
    }
}

/// DataFusion-based compute connector.
///
/// Wraps a `SessionContext` and executes SQL queries against registered tables.
/// Tables can be registered from Parquet, CSV, or in-memory data.
pub struct DataFusionConnector {
    ctx: Arc<SessionContext>,
    profile: ConsumerProfile,
}

impl DataFusionConnector {
    /// Create a new connector with a fresh `SessionContext`.
    pub fn new() -> Self {
        Self {
            ctx: Arc::new(SessionContext::new()),
            profile: ConsumerProfile::default(),
        }
    }

    /// Create a connector wrapping an existing `SessionContext`.
    pub fn with_context(ctx: SessionContext) -> Self {
        Self {
            ctx: Arc::new(ctx),
            profile: ConsumerProfile::default(),
        }
    }

    /// Access the underlying `SessionContext` for registering tables.
    pub fn session_context(&self) -> &SessionContext {
        &self.ctx
    }

    /// Register a SQL statement as a table (CREATE TABLE AS, CREATE VIEW, etc).
    pub async fn register_sql(&self, sql: &str) -> Result<(), ConnectorError> {
        self.ctx
            .sql(sql)
            .await
            .map_err(|e| ConnectorError::Execution(e.to_string()))?
            .collect()
            .await
            .map_err(|e| ConnectorError::Execution(e.to_string()))?;
        Ok(())
    }

    /// Register a CSV file as a named table.
    pub async fn register_csv(&self, table_name: &str, path: &str) -> Result<(), ConnectorError> {
        self.ctx
            .register_csv(table_name, path, CsvReadOptions::default())
            .await
            .map_err(|e| ConnectorError::Execution(e.to_string()))
    }

    /// Register a Parquet file as a named table.
    pub async fn register_parquet(
        &self,
        table_name: &str,
        path: &str,
    ) -> Result<(), ConnectorError> {
        self.ctx
            .register_parquet(table_name, path, ParquetReadOptions::default())
            .await
            .map_err(|e| ConnectorError::Execution(e.to_string()))
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

impl Default for DataFusionConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputeEmitter for DataFusionConnector {
    fn emit_sql(&self, sql: &str) -> Result<ComputePayload, EmitError> {
        Ok(ComputePayload::Sql(sql.to_string()))
    }

    fn emit_substrait(&self, _plan_bytes: &[u8]) -> Result<ComputePayload, EmitError> {
        // DataFusion can consume Substrait via datafusion-substrait, but for v1
        // we only support SQL execution.
        Err(EmitError::UnsupportedNode(
            "Substrait execution not yet supported for DataFusion; use SQL".to_string(),
        ))
    }

    fn supported_payloads(&self) -> &[PayloadKind] {
        &[PayloadKind::Sql]
    }
}

impl ComputeAdapter for DataFusionConnector {
    fn consumer_profile(&self) -> &ConsumerProfile {
        &self.profile
    }

    fn adapt(&self, payload: ComputePayload) -> Result<ComputeRequest, AdaptError> {
        match payload {
            ComputePayload::Sql(_) => Ok(ComputeRequest {
                payload,
                timeout: None,
            }),
            ComputePayload::SubstraitPlan(_) => {
                Err(AdaptError::UnsupportedPayload(PayloadKind::SubstraitPlan))
            }
            ComputePayload::NativePlan(_) => {
                Err(AdaptError::UnsupportedPayload(PayloadKind::NativePlan))
            }
        }
    }
}

#[async_trait::async_trait]
impl ComputeConnector for DataFusionConnector {
    async fn execute(&self, request: ComputeRequest) -> Result<ComputeResult, ConnectorError> {
        let sql = match &request.payload {
            ComputePayload::Sql(sql) => sql.as_str(),
            _ => {
                return Err(ConnectorError::Execution(
                    "DataFusion connector only supports SQL payloads".to_string(),
                ))
            }
        };

        let start = Instant::now();

        let df = self
            .ctx
            .sql(sql)
            .await
            .map_err(|e| ConnectorError::Execution(e.to_string()))?;

        // Enforce timeout if specified in the request.
        let batches = if let Some(timeout) = request.timeout {
            tokio::time::timeout(timeout, df.collect())
                .await
                .map_err(|_| {
                    ConnectorError::Execution(format!(
                        "query timed out after {:?}",
                        timeout
                    ))
                })?
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
        } else {
            df.collect()
                .await
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
        };

        let elapsed = start.elapsed();
        let rows_returned: u64 = batches.iter().map(|b: &RecordBatch| b.num_rows() as u64).sum();

        // Convert Arrow batches to JSON rows for universal consumption.
        // Callers needing raw Arrow can use execute_native() or downcast Native results.
        let arrow_batches = ArrowBatches(batches);
        let json_rows = arrow_batches.to_json_rows()?;

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
        // Execute a trivial query to verify the session is alive.
        self.ctx
            .sql("SELECT 1")
            .await
            .map_err(|e| ConnectorError::Connection(e.to_string()))?
            .collect()
            .await
            .map_err(|e| ConnectorError::Connection(e.to_string()))?;
        Ok(())
    }

    fn name(&self) -> &str {
        "datafusion"
    }

    fn preferred_dialect(&self) -> semstrait_sql::TargetDialect {
        semstrait_sql::TargetDialect::DataFusion
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion_engine::arrow::array::{Float64Array, Int64Array, StringArray};
    use datafusion_engine::arrow::datatypes::{DataType as ArrowDataType, Field as ArrowField, Schema as ArrowSchema};

    async fn setup_connector() -> DataFusionConnector {
        let connector = DataFusionConnector::new();

        // Create an in-memory table for testing.
        let schema = Arc::new(ArrowSchema::new(vec![
            ArrowField::new("region", ArrowDataType::Utf8, false),
            ArrowField::new("amount", ArrowDataType::Float64, false),
            ArrowField::new("order_id", ArrowDataType::Int64, false),
        ]));

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["US", "EU", "US", "EU"])),
                Arc::new(Float64Array::from(vec![100.0, 200.0, 150.0, 50.0])),
                Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
            ],
        )
        .unwrap();

        connector
            .session_context()
            .register_batch("orders", batch)
            .unwrap();

        connector
    }

    #[tokio::test]
    async fn test_health_check() {
        let connector = DataFusionConnector::new();
        let result = connector.health_check().await;
        assert!(result.is_ok(), "health check should pass");
    }

    #[tokio::test]
    async fn test_execute_simple_query() {
        let connector = setup_connector().await;
        let payload = connector.emit_sql("SELECT region, SUM(amount) as total FROM orders GROUP BY region ORDER BY region").unwrap();
        let request = connector.adapt(payload).unwrap();
        let result = connector.execute(request).await.unwrap();

        assert!(result.complete);
        assert_eq!(result.stats.rows_returned, 2); // US, EU

        // Results are now returned as JSON rows by default.
        match &result.data {
            ComputeResultData::Json(rows) => {
                assert_eq!(rows.len(), 2, "should have 2 JSON rows");
                // Each row should be an object with column keys.
                assert!(rows[0].is_object(), "each row should be a JSON object");
                assert!(rows[0].get("region").is_some(), "row should have 'region' key");
                assert!(rows[0].get("total").is_some(), "row should have 'total' key");
            }
            other => panic!("expected Json result data, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_with_filter() {
        let connector = setup_connector().await;
        let payload = connector
            .emit_sql("SELECT region, amount FROM orders WHERE region = 'US'")
            .unwrap();
        let request = connector.adapt(payload).unwrap();
        let result = connector.execute(request).await.unwrap();

        assert_eq!(result.stats.rows_returned, 2); // Two US rows
    }

    #[tokio::test]
    async fn test_substrait_not_supported() {
        let connector = DataFusionConnector::new();
        let result = connector.emit_substrait(&[]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_supported_payloads() {
        let connector = DataFusionConnector::new();
        assert_eq!(connector.supported_payloads(), &[PayloadKind::Sql]);
    }

    #[tokio::test]
    async fn test_name() {
        let connector = DataFusionConnector::new();
        assert_eq!(connector.name(), "datafusion");
    }
}
