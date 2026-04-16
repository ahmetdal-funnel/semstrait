//! Builder API for constructing a Semstrait instance.

use semstrait_catalog::{CatalogProvider, CatalogRegistry, NullCatalogProvider};
use semstrait_manifest::{CompileSource, CompiledManifest, ManifestCompiler};
use semstrait_planner::SemanticPlanner;
use semstrait_adapter::EngineAdapter;
use semstrait_adapter::sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use semstrait_ir::PlanArtifact;
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
    Io(#[from] semstrait_manifest::IoError),

    #[error("plan error: {0}")]
    Plan(#[from] semstrait_planner::PlannerError),

    #[error("emit error: {0}")]
    Emit(#[from] semstrait_adapter::sql::EmitError),

    #[error("adapt error: {0}")]
    Adapt(#[from] semstrait_adapter::AdaptError),

    #[error("configuration error: {0}")]
    Config(String),
}

/// Builder for constructing a `SemstraitInstance`.
///
/// Supports two usage styles:
///
/// **Fast path** — string-based, mirrors CLI simplicity:
/// ```rust,ignore
/// SemstraitBuilder::new()
///     .with_model_file("s3://bucket/model.yaml")
///     .with_catalogs_file("s3://bucket/catalogs.yaml")
///     .with_engine("datafusion")
///     .build().await?;
/// ```
///
/// **Explicit path** — construct providers manually:
/// ```rust,ignore
/// SemstraitBuilder::new()
///     .with_model(yaml_string)
///     .with_catalog_registry(registry)
///     .with_adapter(Arc::new(DataFusionAdapter))
///     .build().await?;
/// ```
pub struct SemstraitBuilder {
    model_yaml: Option<String>,
    model_location: Option<String>,
    catalogs_location: Option<String>,
    engine_name: Option<String>,
    catalog: Option<Arc<dyn CatalogProvider>>,
    catalog_registry: Option<CatalogRegistry>,
    adapter: Option<Arc<dyn EngineAdapter>>,
}

impl SemstraitBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            model_yaml: None,
            model_location: None,
            catalogs_location: None,
            engine_name: None,
            catalog: None,
            catalog_registry: None,
            adapter: None,
        }
    }

    /// Set the model from a YAML string.
    pub fn with_model(mut self, yaml: impl Into<String>) -> Self {
        self.model_yaml = Some(yaml.into());
        self
    }

    /// Set the model from a file path or S3 URI.
    ///
    /// Accepts local filesystem paths or `s3://bucket/key` URIs.
    /// S3 loading requires the `aws` feature.
    pub fn with_model_file(mut self, location: impl Into<String>) -> Self {
        self.model_location = Some(location.into());
        self
    }

    /// Set the catalogs configuration from a file path or S3 URI.
    ///
    /// Parses `catalogs.yaml` and builds a `CatalogRegistry` with concrete
    /// providers for each named catalog entry. Supports local paths and
    /// `s3://` URIs.
    ///
    /// Requires the `catalog-iceberg` feature for Polaris/Iceberg catalogs,
    /// and the `aws` feature for S3 loading and AWS Secrets Manager auth.
    pub fn with_catalogs_file(mut self, location: impl Into<String>) -> Self {
        self.catalogs_location = Some(location.into());
        self
    }

    /// Set the engine adapter by name.
    ///
    /// Supported engines:
    /// - `"datafusion"` — DataFusion adapter (Substrait output). Requires `datafusion` feature.
    /// - `"ansi"` — No adapter, ANSI SQL canonical output.
    ///
    /// When omitted, defaults to ANSI SQL.
    pub fn with_engine(mut self, engine: impl Into<String>) -> Self {
        self.engine_name = Some(engine.into());
        self
    }

    /// Set a single catalog provider.
    pub fn with_catalog(mut self, catalog: Arc<dyn CatalogProvider>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Set a named catalog registry for multi-catalog models.
    ///
    /// Use this when the semantic model references multiple catalogs by alias
    /// (e.g., `catalog: polaris` and `catalog: unity` in different datasets).
    pub fn with_catalog_registry(mut self, registry: CatalogRegistry) -> Self {
        self.catalog_registry = Some(registry);
        self
    }

    /// Set the engine adapter for plan conversion.
    ///
    /// The adapter's `plan_builder()` is wired into the planner for
    /// engine-specific node construction. When no adapter is set,
    /// `explain()` falls back to ANSI SQL and `plan()` produces ANSI SQL.
    pub fn with_adapter(mut self, adapter: Arc<dyn EngineAdapter>) -> Self {
        self.adapter = Some(adapter);
        self
    }

    /// Build the Semstrait instance by compiling the model.
    pub async fn build(self) -> Result<SemstraitInstance, BuildError> {
        let yaml = if let Some(yaml) = self.model_yaml {
            yaml
        } else if let Some(location) = self.model_location {
            semstrait_manifest::io::load_text(&location).await?
        } else {
            return Err(BuildError::NoManifest);
        };

        // Resolve catalog registry from catalogs file if specified.
        let catalog_registry = if let Some(cat_location) = self.catalogs_location {
            Some(build_catalog_registry(&cat_location).await?)
        } else {
            self.catalog_registry
        };

        let mut compiler = ManifestCompiler::new();

        if let Some(registry) = catalog_registry {
            compiler = compiler.with_catalog_registry(registry);
        }

        let catalog = self
            .catalog
            .unwrap_or_else(|| Arc::new(NullCatalogProvider));
        compiler = compiler.with_catalog(catalog);

        let manifest = compiler
            .compile(CompileSource::Yaml(yaml.clone()))
            .await?;

        // Resolve adapter from engine name or use explicit adapter.
        let adapter = if let Some(engine_name) = &self.engine_name {
            resolve_adapter(Some(engine_name))?
        } else {
            self.adapter
        };

        // Wire adapter's plan_builder into the planner.
        let mut planner_builder = SemanticPlanner::builder();
        if let Some(ref adapter) = adapter {
            if let Some(pb) = adapter.plan_builder() {
                planner_builder = planner_builder.with_plan_builder(pb);
            }
        }
        let planner = planner_builder.build();

        Ok(SemstraitInstance {
            model_yaml: yaml,
            manifest,
            planner,
            adapter,
        })
    }
}

/// Resolve an engine adapter by name.
///
/// - `None` or `"ansi"` / `"canonical"` → no adapter (ANSI SQL output)
/// - `"datafusion"` → DataFusion adapter (requires `datafusion` feature)
fn resolve_adapter(engine: Option<&str>) -> Result<Option<Arc<dyn EngineAdapter>>, BuildError> {
    match engine {
        None | Some("ansi") | Some("canonical") => Ok(None),
        #[cfg(feature = "datafusion")]
        Some("datafusion") => {
            Ok(Some(Arc::new(semstrait_adapter::DataFusionAdapter)))
        }
        #[cfg(not(feature = "datafusion"))]
        Some("datafusion") => {
            Err(BuildError::Config(
                "engine 'datafusion' requires the 'datafusion' feature".to_string(),
            ))
        }
        Some(other) => Err(BuildError::Config(
            format!("unknown engine '{}' (supported: datafusion, ansi)", other),
        )),
    }
}

/// Build a `CatalogRegistry` from a `catalogs.yaml` file path or S3 URI.
async fn build_catalog_registry(location: &str) -> Result<CatalogRegistry, BuildError> {
    let yaml = semstrait_manifest::io::load_text(location).await?;
    let config = semstrait_model::parse_catalogs(&yaml)
        .map_err(|e| BuildError::Config(format!("failed to parse catalogs.yaml: {}", e)))?;

    let mut registry = CatalogRegistry::new();

    for (alias, entry) in &config.catalogs {
        let provider = build_catalog_provider(alias, entry).await?;
        registry.register(alias, provider);
    }

    Ok(registry)
}

/// Build a single `CatalogProvider` from a `CatalogEntry`.
async fn build_catalog_provider(
    alias: &str,
    entry: &semstrait_model::CatalogEntry,
) -> Result<Arc<dyn CatalogProvider>, BuildError> {
    match entry.provider_type.as_str() {
        "polaris" | "iceberg_rest" => build_iceberg_provider(alias, entry).await,
        other => Err(BuildError::Config(format!(
            "catalog '{}': unsupported provider type '{}' (supported: polaris, iceberg_rest)",
            alias, other
        ))),
    }
}

/// Build an Iceberg REST catalog provider from a `CatalogEntry`.
#[cfg(feature = "catalog-iceberg")]
async fn build_iceberg_provider(
    alias: &str,
    entry: &semstrait_model::CatalogEntry,
) -> Result<Arc<dyn CatalogProvider>, BuildError> {
    use semstrait_model::CatalogAuthMethod;

    match &entry.auth {
        #[cfg(feature = "aws")]
        CatalogAuthMethod::AwsSecrets {
            secret_arn,
            region,
            scope,
            aws_profile,
            aws_access_key_id,
            aws_secret_access_key,
            aws_session_token,
            ..
        } => {
            let config = semstrait_catalog::secrets::PolarisCatalogConfig {
                catalog_url: &entry.url,
                secret_arn,
                aws_region: region.as_deref(),
                warehouse: Some(&entry.name),
                realm: entry.realm.as_deref(),
                scope: scope.as_deref(),
                aws_profile: aws_profile.as_deref(),
                aws_access_key_id: aws_access_key_id.as_deref(),
                aws_secret_access_key: aws_secret_access_key.as_deref(),
                aws_session_token: aws_session_token.as_deref(),
            };
            let catalog = semstrait_catalog::secrets::build_polaris_catalog(&config)
                .await
                .map_err(|e| BuildError::Config(
                    format!("catalog '{}': failed to build Polaris provider: {}", alias, e),
                ))?;
            Ok(Arc::new(catalog))
        }

        CatalogAuthMethod::Oauth2 {
            token_url,
            client_id,
            client_secret,
            scope,
        } => {
            let url = token_url
                .clone()
                .unwrap_or_else(|| format!("{}/v1/oauth/tokens", entry.url));
            let mut catalog = semstrait_catalog::IcebergRestCatalog::new(&entry.url)
                .with_prefix(&entry.name)
                .with_oauth2(url, client_id, client_secret, scope.clone());
            if let Some(ref realm) = entry.realm {
                catalog = catalog.with_custom_header("Polaris-Realm", realm);
            }
            Ok(Arc::new(catalog))
        }

        CatalogAuthMethod::Bearer { token } => {
            let mut catalog = semstrait_catalog::IcebergRestCatalog::new(&entry.url)
                .with_prefix(&entry.name)
                .with_bearer_token(token);
            if let Some(ref realm) = entry.realm {
                catalog = catalog.with_custom_header("Polaris-Realm", realm);
            }
            Ok(Arc::new(catalog))
        }

        #[cfg(not(feature = "aws"))]
        CatalogAuthMethod::AwsSecrets { .. } => {
            Err(BuildError::Config(format!(
                "catalog '{}': aws_secrets auth requires the 'aws' feature",
                alias
            )))
        }
    }
}

#[cfg(not(feature = "catalog-iceberg"))]
async fn build_iceberg_provider(
    alias: &str,
    _entry: &semstrait_model::CatalogEntry,
) -> Result<Arc<dyn CatalogProvider>, BuildError> {
    Err(BuildError::Config(format!(
        "catalog '{}': iceberg/polaris provider requires the 'catalog-iceberg' feature",
        alias
    )))
}

impl Default for SemstraitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A configured Semstrait instance ready for queries.
pub struct SemstraitInstance {
    model_yaml: String,
    manifest: CompiledManifest,
    planner: SemanticPlanner,
    adapter: Option<Arc<dyn EngineAdapter>>,
}

impl SemstraitInstance {
    /// Create a new builder.
    pub fn builder() -> SemstraitBuilder {
        SemstraitBuilder::new()
    }

    /// Get the raw model YAML source.
    pub fn model_yaml(&self) -> &str {
        &self.model_yaml
    }

    /// Get the compiled manifest.
    pub fn manifest(&self) -> &CompiledManifest {
        &self.manifest
    }

    /// Plan and emit SQL for a query request.
    ///
    /// If an adapter is configured, uses it to produce a debug SQL string.
    /// Otherwise falls back to ANSI SQL emission.
    pub fn explain(
        &self,
        request: &semstrait_planner::request::ResolvedQueryRequest,
    ) -> Result<String, BuildError> {
        let plan = self.planner.plan(request, &self.manifest)?;

        if let Some(adapter) = &self.adapter {
            let sql = adapter.debug_sql(&plan)?;
            Ok(sql)
        } else {
            let emitter = AnsiSqlEmitter::new(AnsiDialect);
            Ok(emitter.emit(&plan)?)
        }
    }

    /// Plan and produce an engine-appropriate artifact.
    ///
    /// If an adapter is configured, uses `adapter.adapt()` to produce
    /// the engine-native artifact (Substrait for DataFusion, SQL for others).
    /// Without an adapter, falls back to ANSI SQL emission.
    pub fn plan(
        &self,
        request: &semstrait_planner::request::ResolvedQueryRequest,
    ) -> Result<PlanArtifact, BuildError> {
        let plan = self.planner.plan(request, &self.manifest)?;

        if let Some(adapter) = &self.adapter {
            Ok(adapter.adapt(&plan)?)
        } else {
            let emitter = AnsiSqlEmitter::new(AnsiDialect);
            let sql = emitter.emit(&plan)?;
            Ok(PlanArtifact::Sql(sql))
        }
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
