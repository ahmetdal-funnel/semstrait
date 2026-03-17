//! Builder API for constructing a Semstrait instance.

use semstrait_catalog::{CatalogProvider, NullCatalogProvider};
use semstrait_manifest::{CompileSource, CompiledManifest, ManifestCompiler};
use semstrait_planner::SemanticPlanner;
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Errors from building a Semstrait instance.
#[derive(Debug, Error)]
pub enum BuildError {
    #[error("no manifest source specified")]
    NoManifest,

    #[error("compile error: {0}")]
    Compile(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Builder for constructing a `SemstraitInstance`.
pub struct SemstraitBuilder {
    manifest_yaml: Option<String>,
    manifest_path: Option<PathBuf>,
    catalog: Option<Arc<dyn CatalogProvider>>,
}

impl SemstraitBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            manifest_yaml: None,
            manifest_path: None,
            catalog: None,
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

    /// Build the Semstrait instance by compiling the manifest.
    pub async fn build(self) -> Result<SemstraitInstance, BuildError> {
        let yaml = if let Some(yaml) = self.manifest_yaml {
            yaml
        } else if let Some(path) = self.manifest_path {
            tokio::fs::read_to_string(&path).await?
        } else {
            return Err(BuildError::NoManifest);
        };

        let _catalog = self
            .catalog
            .unwrap_or_else(|| Arc::new(NullCatalogProvider));

        let compiler = ManifestCompiler::new();
        let manifest = compiler
            .compile(CompileSource::Yaml(yaml.clone()))
            .await
            .map_err(|e| BuildError::Compile(e.to_string()))?;

        Ok(SemstraitInstance {
            manifest_yaml: yaml,
            manifest,
            planner: SemanticPlanner::builder().build(),
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
    pub async fn explain(
        &self,
        request: &semstrait_planner::request::ResolvedQueryRequest,
    ) -> Result<String, String> {
        let plan = self
            .planner
            .plan(request, &self.manifest)
            .await
            .map_err(|e| e.to_string())?;

        let emitter = AnsiSqlEmitter::new(AnsiDialect);
        emitter.emit(&plan).map_err(|e| e.to_string())
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
