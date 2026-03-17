//! SemstraitEngine — the central orchestrator.
//!
//! Coordinates manifest, planner, SQL emitter, and connector
//! to execute semantic queries end-to-end.

use crate::error::EngineError;
use crate::parse::RequestParser;
use crate::types::{ExplainResult, RawQueryRequest, ValidationResult};
use semstrait_manifest::{CompileSource, CompiledManifest, ManifestCompiler};
use semstrait_planner::SemanticPlanner;
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use std::sync::Arc;

/// The central engine that orchestrates semantic query execution.
///
/// Supports:
/// - `validate()` — parse + validate request against manifest
/// - `explain()` — compile → plan → emit SQL
/// - `query()` — explain + execute (requires connector)
pub struct SemstraitEngine {
    manifest: Option<CompiledManifest>,
    planner: SemanticPlanner,
}

impl SemstraitEngine {
    /// Create a new engine without a manifest. Only `validate()` works.
    pub fn new() -> Self {
        Self {
            manifest: None,
            planner: SemanticPlanner::builder().build(),
        }
    }

    /// Create an engine from a compiled manifest.
    pub fn with_manifest(manifest: CompiledManifest) -> Self {
        Self {
            manifest: Some(manifest),
            planner: SemanticPlanner::builder().build(),
        }
    }

    /// Create an engine by compiling a manifest YAML string.
    pub async fn with_manifest_yaml(yaml: &str) -> Result<Self, EngineError> {
        let compiler = ManifestCompiler::new();
        let manifest = compiler
            .compile(CompileSource::Yaml(yaml.to_string()))
            .await
            .map_err(|e| EngineError::Compile(e.to_string()))?;
        Ok(Self::with_manifest(manifest))
    }

    /// Get a reference to the compiled manifest (if loaded).
    pub fn manifest(&self) -> Option<&CompiledManifest> {
        self.manifest.as_ref()
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

        // If we have a manifest, validate names.
        if let Some(manifest) = &self.manifest {
            let mut errors = Vec::new();

            if manifest.get_kind(&raw.kind).is_none() {
                errors.push(format!("kind '{}' not found in manifest", raw.kind));
            } else {
                let kind = manifest.get_kind(&raw.kind).unwrap();
                for dim in &raw.dimensions {
                    if !kind.dimensions.contains_key(dim) {
                        errors.push(format!("dimension '{}' not found in kind '{}'", dim, raw.kind));
                    }
                }
                for mea in &raw.measures {
                    if !kind.measures.contains_key(mea) && !kind.metrics.contains_key(mea) {
                        errors.push(format!(
                            "measure/metric '{}' not found in kind '{}'",
                            mea, raw.kind
                        ));
                    }
                }
            }

            if errors.is_empty() {
                ValidationResult {
                    valid: true,
                    errors: vec![],
                    warnings: vec![],
                }
            } else {
                ValidationResult {
                    valid: false,
                    errors,
                    warnings: vec![],
                }
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

    /// Explain a query: compile, plan, emit SQL — without executing.
    pub async fn explain(
        &self,
        raw: &RawQueryRequest,
    ) -> Result<ExplainResult, EngineError> {
        let manifest = self
            .manifest
            .as_ref()
            .ok_or_else(|| EngineError::NotConfigured("no manifest loaded".to_string()))?;

        // Parse the raw request into a resolved query request.
        let request = RequestParser::to_resolved(raw, manifest)
            .map_err(|e| EngineError::Parse(e.to_string()))?;

        // Plan.
        let plan = self
            .planner
            .plan(&request, manifest)
            .await
            .map_err(|e| EngineError::Plan(e.to_string()))?;

        // Emit SQL via ANSI dialect.
        let emitter = AnsiSqlEmitter::new(AnsiDialect);
        let sql = emitter
            .emit(&plan)
            .map_err(|e| EngineError::Emit(e.to_string()))?;

        // Build plan text summary.
        let plan_text = format!(
            "LogicalPlan: {} output columns [{}]",
            plan.output_names.len(),
            plan.output_names.join(", ")
        );

        Ok(ExplainResult {
            sql: Some(sql),
            substrait_json: None,
            plan_text,
        })
    }

    /// Execute a query end-to-end. Requires a configured connector (v2).
    pub async fn query(
        &self,
        _raw: &RawQueryRequest,
    ) -> Result<serde_json::Value, EngineError> {
        Err(EngineError::NotConfigured(
            "query execution requires a configured connector (v2)".to_string(),
        ))
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
            kind: "sales".to_string(),
            dimensions: vec!["region".to_string()],
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = engine.validate(&raw);
        assert!(result.valid);
    }

    #[test]
    fn test_validate_invalid_request() {
        let engine = SemstraitEngine::new();
        let raw = RawQueryRequest {
            kind: "".to_string(),
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
            kind: "sales".to_string(),
            dimensions: vec!["region".to_string()],
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = engine.explain(&raw).await;
        assert!(matches!(result, Err(EngineError::NotConfigured(_))));
    }

    #[tokio::test]
    async fn test_explain_with_manifest() {
        let yaml = r#"
semantic_model:
  name: test_model
  kinds:
    - name: orders
      type:
        grainset:
      dimensions:
        - name: date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: region
          data_type: string
          type:
            categorical:
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              date: order_date
              region: region_name
              revenue: amount
            storage:
              path: public.orders_daily
"#;

        let engine = SemstraitEngine::with_manifest_yaml(yaml)
            .await
            .expect("engine should compile manifest");

        let raw = RawQueryRequest {
            kind: "orders".to_string(),
            dimensions: vec!["date".to_string(), "region".to_string()],
            measures: vec!["revenue".to_string()],
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
    }

    #[tokio::test]
    async fn test_validate_against_manifest() {
        let yaml = r#"
semantic_model:
  name: test_model
  kinds:
    - name: orders
      type:
        grainset:
      dimensions:
        - name: date
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
        - name: orders_daily
          extras:
            column_mapping:
              date: order_date
              revenue: amount
            storage:
              path: public.orders_daily
"#;

        let engine = SemstraitEngine::with_manifest_yaml(yaml)
            .await
            .expect("engine should compile manifest");

        // Valid request.
        let raw = RawQueryRequest {
            kind: "orders".to_string(),
            dimensions: vec!["date".to_string()],
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };
        let result = engine.validate(&raw);
        assert!(result.valid);

        // Invalid dimension.
        let raw_bad = RawQueryRequest {
            kind: "orders".to_string(),
            dimensions: vec!["nonexistent".to_string()],
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };
        let result_bad = engine.validate(&raw_bad);
        assert!(!result_bad.valid);
        assert!(result_bad.errors[0].contains("nonexistent"));
    }

    #[tokio::test]
    async fn test_query_not_configured() {
        let engine = SemstraitEngine::new();
        let raw = RawQueryRequest {
            kind: "sales".to_string(),
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = engine.query(&raw).await;
        assert!(matches!(result, Err(EngineError::NotConfigured(_))));
    }
}
