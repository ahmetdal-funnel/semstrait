//! DataFusion compute connector.
//!
//! Executes SQL queries via DataFusion's `SessionContext`, converting
//! Arrow `RecordBatch`es to JSON rows in `ComputeResultData::Json`.

use std::sync::Arc;
use std::time::Instant;

use datafusion_engine::arrow::array::RecordBatch;
use datafusion_engine::prelude::*;

use crate::payload::{
    ComputeResult, ComputeResultData, ConnectorError, ExecutionStats,
};
use crate::traits::ComputeConnector;
use semstrait_adapter::{DataFusionAdapter, EngineAdapter};
use semstrait_ir::PlanArtifact;

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
    adapter: DataFusionAdapter,
}

impl DataFusionConnector {
    /// Create a new connector with a fresh `SessionContext`.
    pub fn new() -> Self {
        Self {
            ctx: Arc::new(SessionContext::new()),
            adapter: DataFusionAdapter,
        }
    }

    /// Create a connector wrapping an existing `SessionContext`.
    pub fn with_context(ctx: SessionContext) -> Self {
        Self {
            ctx: Arc::new(ctx),
            adapter: DataFusionAdapter,
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

    /// Register all resolved sources from a compiled manifest.
    ///
    /// For each kind dataset's resolved sources:
    /// - **Path** sources → `register_file()` (auto-detect CSV/Parquet by extension)
    /// - **Table** sources → look up location from `catalog_snapshot`, register as Parquet
    ///
    /// Returns the list of successfully registered table names.
    /// Failures are logged but not fatal.
    pub async fn register_manifest_sources(
        &self,
        manifest: &semstrait_manifest::CompiledManifest,
    ) -> Result<Vec<String>, ConnectorError> {
        let mut registered = Vec::new();

        for data_kind in manifest.data_kinds.values() {
            for binding in data_kind.bindings() {
                if binding.resolved_sources.len() <= 1 {
                    // Single source: use same name priority as planner's
                    // build_scan_node_binding() — table_fqn > reference > dataset_name.
                    let source = binding.resolved_sources.first();
                    let table_name = source
                        .and_then(|s| s.table_fqn.as_deref())
                        .or_else(|| source.map(|s| s.reference.as_str()))
                        .unwrap_or(&binding.dataset_name);

                    if let Some(source) = source {
                        match self.register_source(table_name, source, manifest).await {
                            Ok(()) => registered.push(table_name.to_string()),
                            Err(e) => {
                                tracing::warn!(
                                    "failed to register source for dataset '{}': {}",
                                    binding.dataset_name,
                                    e
                                );
                            }
                        }
                    }
                } else {
                    // Multi-source: each source gets its own registration using
                    // table_fqn > reference (matching planner's union scan logic).
                    for source in &binding.resolved_sources {
                        let table_name = source
                            .table_fqn
                            .as_deref()
                            .unwrap_or(&source.reference);

                        match self.register_source(table_name, source, manifest).await {
                            Ok(()) => registered.push(table_name.to_string()),
                            Err(e) => {
                                tracing::warn!(
                                    "failed to register source '{}' for dataset '{}': {}",
                                    source.reference,
                                    binding.dataset_name,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(registered)
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

    /// Register a single resolved source, dispatching by source type.
    async fn register_source(
        &self,
        table_name: &str,
        source: &semstrait_manifest::ResolvedSource,
        manifest: &semstrait_manifest::CompiledManifest,
    ) -> Result<(), ConnectorError> {
        use semstrait_manifest::SourceType;

        match source.source_type {
            SourceType::Path => {
                self.register_file(table_name, &source.reference).await
            }
            SourceType::Table => {
                let location = manifest
                    .catalog_snapshot
                    .as_ref()
                    .and_then(|snap| snap.tables.get(&source.reference))
                    .and_then(|ts| ts.iceberg.as_ref())
                    .and_then(|ice| ice.location.as_deref());

                if let Some(loc) = location {
                    self.register_parquet(table_name, loc).await
                } else {
                    tracing::debug!(
                        "no location found for table source '{}' — skipping registration",
                        source.reference
                    );
                    Ok(())
                }
            }
        }
    }
}

impl Default for DataFusionConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ComputeConnector for DataFusionConnector {
    fn adapter(&self) -> &dyn EngineAdapter {
        &self.adapter
    }

    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError> {
        let start = Instant::now();

        let batches = match artifact {
            PlanArtifact::Sql(sql) => {
                self.ctx
                    .sql(sql)
                    .await
                    .map_err(|e| ConnectorError::Execution(e.to_string()))?
                    .collect()
                    .await
                    .map_err(|e| ConnectorError::Execution(e.to_string()))?
            }
            PlanArtifact::Substrait(plan) => {
                use datafusion_substrait::logical_plan::consumer::from_substrait_plan;
                let state = self.ctx.state();
                let df_logical = from_substrait_plan(&state, plan)
                    .await
                    .map_err(|e| {
                        ConnectorError::Execution(format!(
                            "Substrait plan consumption failed: {}",
                            e
                        ))
                    })?;
                self.ctx
                    .execute_logical_plan(df_logical)
                    .await
                    .map_err(|e| ConnectorError::Execution(e.to_string()))?
                    .collect()
                    .await
                    .map_err(|e| ConnectorError::Execution(e.to_string()))?
            }
        };

        let elapsed = start.elapsed();
        let rows_returned: u64 = batches.iter().map(|b: &RecordBatch| b.num_rows() as u64).sum();

        // Convert Arrow batches to JSON rows for universal consumption.
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
        let artifact = PlanArtifact::Sql(
            "SELECT region, SUM(amount) as total FROM orders GROUP BY region ORDER BY region"
                .to_string(),
        );
        let result = connector.execute(&artifact).await.unwrap();

        assert!(result.complete);
        assert_eq!(result.stats.rows_returned, 2); // US, EU

        // Results are returned as JSON rows.
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
        let connector = setup_connector().await;
        let artifact = PlanArtifact::Sql(
            "SELECT region, amount FROM orders WHERE region = 'US'".to_string(),
        );
        let result = connector.execute(&artifact).await.unwrap();

        assert_eq!(result.stats.rows_returned, 2); // Two US rows
    }

    #[tokio::test]
    async fn test_name() {
        let connector = DataFusionConnector::new();
        assert_eq!(connector.name(), "datafusion");
    }

    #[tokio::test]
    async fn test_adapter_accessible() {
        let connector = DataFusionConnector::new();
        let adapter = connector.adapter();
        assert_eq!(adapter.name(), "datafusion");
    }

    #[tokio::test]
    async fn test_execute_substrait_simple_query() {
        use datafusion_substrait::logical_plan::producer::to_substrait_plan;

        let connector = setup_connector().await;

        // Build a DataFusion LogicalPlan via SQL, then convert to Substrait.
        let df = connector.ctx.sql(
            "SELECT region, SUM(amount) as total FROM orders GROUP BY region ORDER BY region"
        ).await.unwrap();
        let logical_plan = df.logical_plan().clone();
        let state = connector.ctx.state();
        let substrait_plan = to_substrait_plan(&logical_plan, &state).unwrap();

        // Execute via the Substrait path.
        let artifact = PlanArtifact::Substrait(substrait_plan);
        let result = connector.execute(&artifact).await.unwrap();

        assert!(result.complete);
        assert_eq!(result.stats.rows_returned, 2); // US, EU

        match &result.data {
            ComputeResultData::Json(rows) => {
                assert_eq!(rows.len(), 2, "should have 2 JSON rows");
                assert!(rows[0].is_object(), "each row should be a JSON object");
            }
            other => panic!("expected Json result data, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_substrait_with_filter() {
        use datafusion_substrait::logical_plan::producer::to_substrait_plan;

        let connector = setup_connector().await;

        let df = connector.ctx.sql(
            "SELECT region, amount FROM orders WHERE region = 'US'"
        ).await.unwrap();
        let logical_plan = df.logical_plan().clone();
        let state = connector.ctx.state();
        let substrait_plan = to_substrait_plan(&logical_plan, &state).unwrap();

        let artifact = PlanArtifact::Substrait(substrait_plan);
        let result = connector.execute(&artifact).await.unwrap();

        assert_eq!(result.stats.rows_returned, 2); // Two US rows
    }

    #[tokio::test]
    async fn test_execute_substrait_rejects_invalid_plan() {
        use datafusion_substrait::logical_plan::producer::to_substrait_plan;

        let connector = DataFusionConnector::new();

        // Build a plan referencing a nonexistent table — consumption should fail.
        // We create a valid Substrait plan structure but the table won't exist
        // in the session context.
        let df = connector.ctx.sql("SELECT 1 AS x").await.unwrap();
        let logical_plan = df.logical_plan().clone();
        let state = connector.ctx.state();
        let mut substrait_plan = to_substrait_plan(&logical_plan, &state).unwrap();

        // Corrupt the plan to make it invalid.
        substrait_plan.relations.clear();

        let artifact = PlanArtifact::Substrait(substrait_plan);
        let result = connector.execute(&artifact).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            ConnectorError::Execution(msg) => {
                assert!(
                    msg.contains("Substrait plan consumption failed"),
                    "error should mention Substrait: {}",
                    msg
                );
            }
            other => panic!("expected Execution error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_sql_vs_substrait_produce_same_results() {
        use datafusion_substrait::logical_plan::producer::to_substrait_plan;

        let connector = setup_connector().await;
        let query = "SELECT region, SUM(amount) as total FROM orders GROUP BY region ORDER BY region";

        // SQL path
        let sql_artifact = PlanArtifact::Sql(query.to_string());
        let sql_result = connector.execute(&sql_artifact).await.unwrap();

        // Substrait path
        let df = connector.ctx.sql(query).await.unwrap();
        let logical_plan = df.logical_plan().clone();
        let state = connector.ctx.state();
        let substrait_plan = to_substrait_plan(&logical_plan, &state).unwrap();
        let substrait_artifact = PlanArtifact::Substrait(substrait_plan);
        let substrait_result = connector.execute(&substrait_artifact).await.unwrap();

        // Both paths should produce identical results.
        assert_eq!(sql_result.stats.rows_returned, substrait_result.stats.rows_returned);
        match (&sql_result.data, &substrait_result.data) {
            (ComputeResultData::Json(sql_rows), ComputeResultData::Json(sub_rows)) => {
                assert_eq!(sql_rows, sub_rows, "SQL and Substrait should produce identical JSON rows");
            }
            _ => panic!("expected Json data from both paths"),
        }
    }
}
