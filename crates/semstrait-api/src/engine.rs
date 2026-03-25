//! SemstraitEngine — the central orchestrator.
//!
//! Coordinates manifest, planner, SQL emitter, and connector
//! to execute semantic queries end-to-end.

use crate::error::EngineError;
use crate::parse::RequestParser;
use crate::types::{ExplainResult, RawQueryRequest, ValidationResult};
use semstrait_catalog::{CatalogProvider, TableRef};
use semstrait_connectors::{ComputeConnector, ComputeResultData};
use semstrait_ir::{PlanArtifact, PlannerWarning, SubstraitSerializer};
use semstrait_manifest::{CompileSource, CompiledManifest, ManifestCompiler, SchemaColumn};
use semstrait_planner::SemanticPlanner;
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use std::sync::Arc;

/// The central engine that orchestrates semantic query execution.
///
/// Supports:
/// - `validate()` — parse + validate request against manifest
/// - `explain()` — compile → plan → emit SQL + Substrait JSON
/// - `query()` — explain + execute (requires connector)
pub struct SemstraitEngine {
    manifest: Option<CompiledManifest>,
    planner: SemanticPlanner,
    connector: Option<Arc<dyn ComputeConnector>>,
}

impl SemstraitEngine {
    /// Create a new engine without a manifest. Only `validate()` works.
    pub fn new() -> Self {
        Self {
            manifest: None,
            planner: SemanticPlanner::builder().build(),
            connector: None,
        }
    }

    /// Create an engine from a compiled manifest.
    pub fn with_manifest(manifest: CompiledManifest) -> Self {
        Self {
            manifest: Some(manifest),
            planner: SemanticPlanner::builder().build(),
            connector: None,
        }
    }

    /// Create an engine with a manifest and a compute connector for query execution.
    pub fn with_connector(
        manifest: CompiledManifest,
        connector: Arc<dyn ComputeConnector>,
    ) -> Self {
        let profile = semstrait_adapter::profile_from_adapter(connector.adapter());
        let planner = SemanticPlanner::builder()
            .with_profile(Arc::new(profile))
            .build();

        Self {
            manifest: Some(manifest),
            planner,
            connector: Some(connector),
        }
    }

    /// Create an engine by compiling a manifest YAML string.
    pub async fn with_manifest_yaml(yaml: &str) -> Result<Self, EngineError> {
        let compiler = ManifestCompiler::new();
        let manifest = compiler
            .compile(CompileSource::Yaml(yaml.to_string()))
            .await?;
        Ok(Self::with_manifest(manifest))
    }

    /// Get a reference to the compiled manifest (if loaded).
    pub fn manifest(&self) -> Option<&CompiledManifest> {
        self.manifest.as_ref()
    }

    /// Set a connector on an existing engine.
    pub fn set_connector(&mut self, connector: Arc<dyn ComputeConnector>) {
        let profile = semstrait_adapter::profile_from_adapter(connector.adapter());
        self.planner = SemanticPlanner::builder()
            .with_profile(Arc::new(profile))
            .build();
        self.connector = Some(connector);
    }

    /// Emit SQL from a logical plan using ANSI dialect.
    ///
    /// Used as the fallback when no connector/adapter is configured.
    fn emit_ansi_sql(plan: &semstrait_ir::LogicalPlan) -> Result<String, EngineError> {
        let emitter = AnsiSqlEmitter::new(AnsiDialect);
        Ok(emitter.emit(plan)?)
    }

    /// Validate a query request against the manifest.
    pub fn validate(&self, raw: &RawQueryRequest) -> ValidationResult {
        // Basic structural validation.
        if let Err(e) = RequestParser::parse(raw) {
            return ValidationResult {
                valid: false,
                errors: vec![e.to_string()],
                warnings: vec![],
            };
        }

        // If we have a manifest, validate names via full resolution.
        if let Some(manifest) = &self.manifest {
            match RequestParser::to_resolved(raw, manifest) {
                Ok(_) => ValidationResult {
                    valid: true,
                    errors: vec![],
                    warnings: vec![],
                },
                Err(e) => ValidationResult {
                    valid: false,
                    errors: vec![e.to_string()],
                    warnings: vec![],
                },
            }
        } else {
            // No manifest — structural validation only.
            ValidationResult {
                valid: true,
                errors: vec![],
                warnings: vec!["no manifest loaded; skipping semantic validation".to_string()],
            }
        }
    }

    /// Explain a query: compile, plan, emit SQL + Substrait JSON — without executing.
    pub async fn explain(
        &self,
        raw: &RawQueryRequest,
    ) -> Result<ExplainResult, EngineError> {
        let manifest = self
            .manifest
            .as_ref()
            .ok_or_else(|| EngineError::NotConfigured("no manifest loaded".to_string()))?;

        // Parse the raw request into a resolved query request.
        let request = RequestParser::to_resolved(raw, manifest)?;

        // Plan.
        let plan = self.planner.plan(&request, manifest)?;

        // If we have a connector, use its adapter for SQL and Substrait.
        // Otherwise fall back to ANSI SQL + direct Substrait serialization.
        let (sql, substrait_json) = if let Some(connector) = &self.connector {
            let adapter = connector.adapter();
            let artifact = adapter.adapt(&plan)?;
            let debug_sql = adapter.debug_sql(&plan)?;

            let substrait_json = artifact.to_json();
            (Some(debug_sql), substrait_json)
        } else {
            // No connector — emit ANSI SQL and Substrait JSON directly.
            let sql = Self::emit_ansi_sql(&plan)?;

            let substrait_json = match SubstraitSerializer::to_substrait(&plan) {
                Ok(proto_plan) => match serde_json::to_string_pretty(&proto_plan) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        tracing::warn!("Substrait JSON serialization failed: {}", e);
                        None
                    }
                },
                Err(e) => {
                    tracing::warn!("Substrait plan conversion failed: {}", e);
                    None
                }
            };

            (Some(sql), substrait_json)
        };

        // Build plan text summary.
        let plan_text = format!(
            "LogicalPlan: {} output columns [{}]",
            plan.output_names.len(),
            plan.output_names.join(", ")
        );

        Ok(ExplainResult {
            sql,
            substrait_json,
            plan_text,
        })
    }

    /// Execute a query end-to-end. Requires a configured connector.
    pub async fn query(
        &self,
        raw: &RawQueryRequest,
    ) -> Result<serde_json::Value, EngineError> {
        let connector = self
            .connector
            .as_ref()
            .ok_or_else(|| EngineError::NotConfigured("no connector configured".to_string()))?;

        let manifest = self
            .manifest
            .as_ref()
            .ok_or_else(|| EngineError::NotConfigured("no manifest loaded".to_string()))?;

        // Parse and plan.
        let request = RequestParser::to_resolved(raw, manifest)?;
        let plan = self.planner.plan(&request, manifest)?;

        // Use the connector's adapter to produce the artifact.
        // If the adapter's primary artifact is Substrait but the connector only
        // handles SQL (pending native Substrait consumption), fall back to a SQL
        // artifact via adapter.debug_sql().
        let adapter = connector.adapter();
        let artifact = adapter.adapt(&plan)?;
        let artifact = if artifact.is_sql() {
            artifact
        } else {
            let sql = adapter.debug_sql(&plan)?;
            PlanArtifact::Sql(sql)
        };

        // Execute the artifact.
        let result = connector.execute(&artifact).await?;

        // Convert result to JSON. Destructure to move fields instead of borrowing.
        let stats = result.stats;
        let complete = result.complete;
        match result.data {
            ComputeResultData::Json(rows) => Ok(serde_json::json!({
                "rows": rows,
                "stats": {
                    "rows_returned": stats.rows_returned,
                    "complete": complete,
                }
            })),
            ComputeResultData::Empty => Ok(serde_json::json!({
                "rows": [],
                "stats": {
                    "rows_returned": 0,
                    "complete": complete,
                }
            })),
            ComputeResultData::Native(native) => {
                // Attempt to extract ArrowBatches and convert to JSON as a fallback.
                // Connectors should prefer returning ComputeResultData::Json directly.
                #[cfg(feature = "datafusion")]
                {
                    if let Some(arrow_batches) = native.downcast_ref::<semstrait_connectors::datafusion::ArrowBatches>() {
                        let json_rows = arrow_batches.to_json_rows()
                            .map_err(|e| EngineError::Internal(format!("Arrow to JSON conversion failed: {}", e)))?;
                        return Ok(serde_json::json!({
                            "rows": json_rows,
                            "stats": {
                                "rows_returned": stats.rows_returned,
                                "complete": complete,
                            }
                        }));
                    }
                }
                let _ = native;
                Ok(serde_json::json!({
                    "format": "native",
                    "stats": {
                        "rows_returned": stats.rows_returned,
                        "complete": complete,
                    }
                }))
            }
        }
    }

    /// Check for schema drift between the compiled manifest and the live catalog.
    ///
    /// Returns PLAN_W003 warnings for each dataset where the catalog schema
    /// differs from the compiled schema snapshot. Requires a catalog provider.
    ///
    /// This is a best-effort check: datasets without compiled schema snapshots
    /// or inaccessible in the catalog are silently skipped.
    pub async fn check_schema_drift(
        &self,
        catalog: &dyn CatalogProvider,
        namespace: &str,
    ) -> Vec<PlannerWarning> {
        let mut warnings = Vec::new();
        let manifest = match &self.manifest {
            Some(m) => m,
            None => return warnings,
        };

        for (_, dataset) in &manifest.datasets {
            let compiled = match &dataset.compiled_schema {
                Some(s) => s,
                None => continue,
            };

            let table_ref = TableRef::new(namespace, &dataset.name);
            let live_columns = match catalog.get_schema(&table_ref).await {
                Ok(cols) => cols,
                Err(_) => continue,
            };

            let live: Vec<SchemaColumn> = live_columns
                .into_iter()
                .map(|c| SchemaColumn {
                    name: c.name,
                    data_type: format!("{:?}", c.data_type),
                    nullable: c.nullable,
                })
                .collect();

            if compiled != &live {
                let mut diffs = Vec::new();
                // Check for missing/added/changed columns.
                for cc in compiled {
                    match live.iter().find(|lc| lc.name == cc.name) {
                        None => diffs.push(format!("column '{}' removed", cc.name)),
                        Some(lc) if lc.data_type != cc.data_type => {
                            diffs.push(format!(
                                "column '{}' type changed: {} -> {}",
                                cc.name, cc.data_type, lc.data_type
                            ));
                        }
                        Some(lc) if lc.nullable != cc.nullable => {
                            diffs.push(format!(
                                "column '{}' nullability changed",
                                cc.name
                            ));
                        }
                        _ => {}
                    }
                }
                for lc in &live {
                    if !compiled.iter().any(|cc| cc.name == lc.name) {
                        diffs.push(format!("column '{}' added", lc.name));
                    }
                }

                if !diffs.is_empty() {
                    warnings.push(PlannerWarning::SchemaDrift {
                        dataset: dataset.name.clone(),
                        details: diffs.join("; "),
                    });
                }
            }
        }

        warnings
    }
}

impl Default for SemstraitEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared engine state for API transports.
pub type SharedEngine = Arc<SemstraitEngine>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_request() {
        let engine = SemstraitEngine::new();
        let raw = RawQueryRequest {
            from: "sales".to_string(),
            select: vec!["region".to_string(), "revenue".to_string()],
            ..Default::default()
        };

        let result = engine.validate(&raw);
        assert!(result.valid);
    }

    #[test]
    fn test_validate_invalid_request() {
        let engine = SemstraitEngine::new();
        let raw = RawQueryRequest {
            from: "".to_string(),
            ..Default::default()
        };

        let result = engine.validate(&raw);
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_explain_no_manifest() {
        let engine = SemstraitEngine::new();
        let raw = RawQueryRequest {
            from: "sales".to_string(),
            select: vec!["region".to_string(), "revenue".to_string()],
            ..Default::default()
        };

        let result = engine.explain(&raw).await;
        assert!(matches!(result, Err(EngineError::NotConfigured(_))));
    }

    fn load_model(name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = format!(
            "{}/../../tests/fixtures/models/{}.yaml",
            manifest_dir, name
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to load fixture '{}': {}", path, e))
    }

    #[tokio::test]
    async fn test_explain_with_manifest() {
        let yaml = load_model("orders_with_metrics");

        let engine = SemstraitEngine::with_manifest_yaml(&yaml)
            .await
            .expect("engine should compile manifest");

        let raw = RawQueryRequest {
            from: "orders".to_string(),
            select: vec!["date".to_string(), "region".to_string(), "revenue".to_string()],
            ..Default::default()
        };

        let result = engine.explain(&raw).await;
        assert!(result.is_ok(), "explain should succeed: {:?}", result.err());

        let explain = result.unwrap();
        assert!(explain.sql.is_some(), "should have SQL");
        let sql = explain.sql.unwrap();
        assert!(sql.contains("SELECT"), "SQL should contain SELECT: {}", sql);
        assert!(
            sql.contains("GROUP BY"),
            "SQL should contain GROUP BY: {}",
            sql
        );
        assert!(
            explain.substrait_json.is_some(),
            "should have Substrait JSON"
        );
    }

    #[tokio::test]
    async fn test_validate_against_manifest() {
        let yaml = load_model("orders_simple");

        let engine = SemstraitEngine::with_manifest_yaml(&yaml)
            .await
            .expect("engine should compile manifest");

        // Valid request.
        let raw = RawQueryRequest {
            from: "orders".to_string(),
            select: vec!["date".to_string(), "revenue".to_string()],
            ..Default::default()
        };
        let result = engine.validate(&raw);
        assert!(result.valid);

        // Invalid select name.
        let raw_bad = RawQueryRequest {
            from: "orders".to_string(),
            select: vec!["nonexistent".to_string()],
            ..Default::default()
        };
        let result_bad = engine.validate(&raw_bad);
        assert!(!result_bad.valid);
        assert!(result_bad.errors[0].contains("nonexistent"));
    }

    #[tokio::test]
    async fn test_explain_with_auto_column_mapping() {
        let yaml = r#"
semantic_model:
  name: auto_test
  grainsets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders_fact
          extras:
            column_mapping: auto
            storage:
              path: db.orders_fact
"#;

        let engine = SemstraitEngine::with_manifest_yaml(yaml)
            .await
            .expect("engine should compile with auto mapping");

        let raw = RawQueryRequest {
            from: "orders".to_string(),
            select: vec!["order_date".to_string(), "revenue".to_string()],
            ..Default::default()
        };

        let result = engine.explain(&raw).await;
        assert!(result.is_ok(), "explain should succeed: {:?}", result.err());

        let sql = result.unwrap().sql.unwrap();
        // With auto mapping, physical names = semantic names (identity).
        assert!(sql.contains("order_date"), "SQL should use identity-mapped column: {}", sql);
        assert!(sql.contains("SELECT"), "SQL should contain SELECT: {}", sql);
    }

    #[tokio::test]
    async fn test_query_not_configured() {
        let engine = SemstraitEngine::new();
        let raw = RawQueryRequest {
            from: "sales".to_string(),
            select: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = engine.query(&raw).await;
        assert!(matches!(result, Err(EngineError::NotConfigured(_))));
    }

    #[tokio::test]
    async fn test_schema_drift_detection() {
        use semstrait_catalog::NullCatalogProvider;
        use semstrait_manifest::CompiledDataset;

        // Build a manifest and inject a top-level dataset with a schema snapshot.
        let yaml = load_model("orders_simple");
        let mut engine = SemstraitEngine::with_manifest_yaml(&yaml)
            .await
            .expect("engine should compile manifest");

        // Insert a dataset with a compiled schema snapshot into manifest.datasets.
        if let Some(ref mut manifest) = engine.manifest {
            manifest.datasets.insert(
                "orders".to_string(),
                CompiledDataset {
                    name: "orders".to_string(),
                    description: None,
                    domain: None,
                    keys: None,
                    dimensions: Default::default(),
                    measures: Default::default(),
                    metrics: Default::default(),
                    compiled_schema: Some(vec![
                        SchemaColumn {
                            name: "id".to_string(),
                            data_type: "Int64".to_string(),
                            nullable: false,
                        },
                        SchemaColumn {
                            name: "amount".to_string(),
                            data_type: "Float64".to_string(),
                            nullable: true,
                        },
                    ]),
                },
            );
        }

        // NullCatalogProvider returns empty schemas, so all compiled columns look "removed".
        let warnings = engine
            .check_schema_drift(&NullCatalogProvider, "default")
            .await;
        // NullCatalogProvider returns Ok(Vec::new()), so empty schema vs 2 columns = drift
        assert!(!warnings.is_empty());
        assert!(matches!(
            &warnings[0],
            PlannerWarning::SchemaDrift { dataset, details }
            if dataset == "orders" && details.contains("removed")
        ));
    }
}
