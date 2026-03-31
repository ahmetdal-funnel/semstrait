//! Builder API for constructing a Semstrait instance.

use semstrait_catalog::{CatalogProvider, NullCatalogProvider};
use semstrait_connectors::{ComputeConnector, ComputeResult};
use semstrait_manifest::{CompileSource, CompiledManifest, ManifestCompiler};
use semstrait_planner::SemanticPlanner;
use semstrait_adapter::sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Errors from building a Semstrait instance.
#[derive(Debug, Error)]
pub enum BuildError {
    #[error("no manifest source specified")]
    NoManifest,

    #[error("compile error: {0}")]
    Compile(#[from] semstrait_manifest::CompileError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("plan error: {0}")]
    Plan(#[from] semstrait_planner::PlannerError),

    #[error("emit error: {0}")]
    Emit(#[from] semstrait_adapter::sql::EmitError),

    #[error("adapt error: {0}")]
    Adapt(#[from] semstrait_adapter::AdaptError),

    #[error("connector error: {0}")]
    Connector(#[from] semstrait_connectors::ConnectorError),

    #[error("query error: {0}")]
    Query(String),
}

/// Builder for constructing a `SemstraitInstance`.
pub struct SemstraitBuilder {
    manifest_yaml: Option<String>,
    manifest_path: Option<PathBuf>,
    catalog: Option<Arc<dyn CatalogProvider>>,
    connector: Option<Arc<dyn ComputeConnector>>,
}

impl SemstraitBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            manifest_yaml: None,
            manifest_path: None,
            catalog: None,
            connector: None,
        }
    }

    /// Set the manifest from a YAML string.
    pub fn with_manifest_yaml(mut self, yaml: impl Into<String>) -> Self {
        self.manifest_yaml = Some(yaml.into());
        self
    }

    /// Set the manifest from a file path.
    pub fn with_manifest_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.manifest_path = Some(path.into());
        self
    }

    /// Set the catalog provider.
    pub fn with_catalog(mut self, catalog: Arc<dyn CatalogProvider>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Set the compute connector for query execution.
    pub fn with_connector(mut self, connector: Arc<dyn ComputeConnector>) -> Self {
        self.connector = Some(connector);
        self
    }

    /// Build the Semstrait instance by compiling the manifest.
    pub async fn build(self) -> Result<SemstraitInstance, BuildError> {
        let yaml = if let Some(yaml) = self.manifest_yaml {
            yaml
        } else if let Some(path) = self.manifest_path {
            tokio::fs::read_to_string(&path).await?
        } else {
            return Err(BuildError::NoManifest);
        };

        let catalog = self
            .catalog
            .unwrap_or_else(|| Arc::new(NullCatalogProvider));

        let compiler = ManifestCompiler::new().with_catalog(catalog);
        let manifest = compiler
            .compile(CompileSource::Yaml(yaml.clone()))
            .await?;

        let planner = SemanticPlanner::builder().build();

        Ok(SemstraitInstance {
            manifest_yaml: yaml,
            manifest,
            planner,
            connector: self.connector,
        })
    }
}

impl Default for SemstraitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A configured Semstrait instance ready for queries.
pub struct SemstraitInstance {
    manifest_yaml: String,
    manifest: CompiledManifest,
    planner: SemanticPlanner,
    connector: Option<Arc<dyn ComputeConnector>>,
}

impl SemstraitInstance {
    /// Create a new builder.
    pub fn builder() -> SemstraitBuilder {
        SemstraitBuilder::new()
    }

    /// Get the raw manifest YAML.
    pub fn manifest_yaml(&self) -> &str {
        &self.manifest_yaml
    }

    /// Get the compiled manifest.
    pub fn manifest(&self) -> &CompiledManifest {
        &self.manifest
    }

    /// Plan and emit SQL for a query request.
    ///
    /// If a connector is configured, uses its adapter to produce a debug SQL string.
    /// Otherwise falls back to ANSI SQL emission.
    pub fn explain(
        &self,
        request: &semstrait_planner::request::ResolvedQueryRequest,
    ) -> Result<String, BuildError> {
        let plan = self.planner.plan(request, &self.manifest)?;

        if let Some(connector) = &self.connector {
            let adapter = connector.adapter();
            let sql = adapter.debug_sql(&plan)?;
            Ok(sql)
        } else {
            let emitter = AnsiSqlEmitter::new(AnsiDialect);
            Ok(emitter.emit(&plan)?)
        }
    }

    /// Execute a query via the configured connector.
    pub async fn query(
        &self,
        request: &semstrait_planner::request::ResolvedQueryRequest,
    ) -> Result<ComputeResult, BuildError> {
        let connector = self
            .connector
            .as_ref()
            .ok_or_else(|| BuildError::Query("no connector configured".to_string()))?;

        let plan = self.planner.plan(request, &self.manifest)?;

        // Use the connector's adapter to produce the artifact.
        // If the adapter's primary artifact is Substrait but the connector only
        // handles SQL, fall back to SQL via adapter.debug_sql().
        let adapter = connector.adapter();
        let artifact = adapter.adapt(&plan)?;
        let artifact = if artifact.is_sql() {
            artifact
        } else {
            let sql = adapter.debug_sql(&plan)?;
            semstrait_ir::PlanArtifact::Sql(sql)
        };

        // Execute the artifact.
        Ok(connector.execute(&artifact).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_no_manifest() {
        let result = SemstraitBuilder::new().build().await;
        assert!(matches!(result, Err(BuildError::NoManifest)));
    }
}
