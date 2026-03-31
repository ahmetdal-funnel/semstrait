//! ManifestCompiler — orchestrates the 9-step compilation pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};

use semstrait_catalog::{CatalogProvider, CatalogRegistry, StorageProvider};
use semstrait_model::parse;
use semstrait_model::resolve_refs;

use crate::compiled::CompiledManifest;
use crate::error::CompileError;
use crate::steps;

/// Source input for compilation.
#[derive(Debug)]
pub enum CompileSource {
    /// Raw YAML string.
    Yaml(String),
    /// One or more YAML file paths.
    YamlFiles(Vec<PathBuf>),
    /// A directory containing YAML files.
    Directory(PathBuf),
}

/// The manifest compiler. Orchestrates the compilation pipeline.
pub struct ManifestCompiler {
    catalog: Option<Arc<dyn CatalogProvider>>,
    catalog_registry: Option<CatalogRegistry>,
    storage: Option<Arc<dyn StorageProvider>>,
}

impl ManifestCompiler {
    /// Create a new compiler with no providers.
    pub fn new() -> Self {
        Self {
            catalog: None,
            catalog_registry: None,
            storage: None,
        }
    }

    /// Set a single catalog provider (legacy; prefer `with_catalog_registry` for multi-catalog).
    pub fn with_catalog(mut self, c: Arc<dyn CatalogProvider>) -> Self {
        self.catalog = Some(c);
        self
    }

    /// Set a named catalog registry built from `catalogs.yaml`.
    pub fn with_catalog_registry(mut self, registry: CatalogRegistry) -> Self {
        self.catalog_registry = Some(registry);
        self
    }

    /// Set a storage provider for filesystem/object store operations.
    pub fn with_storage(mut self, s: Arc<dyn StorageProvider>) -> Self {
        self.storage = Some(s);
        self
    }

    /// Run the compilation pipeline.
    ///
    /// Steps:
    /// 1. parse — serde_yaml -> SemanticModel
    /// 2. resolve_refs — expand ref: entries
    /// 3. resolve_sources — expand globs/wildcards, fetch catalog metadata
    /// 4. validate_structure — dataset uniqueness, kind nesting, joinset anchors
    ///    4.5. expand_auto_mappings
    ///    4.6-4.8. validate temporal, storage, metadata dimensions
    /// 5. validate_mappings — column_mapping keys exist in kind interface
    ///    5.5. validate_grain_compatibility
    /// 6. build_metric_graph — petgraph DiGraph, cycle detection, depth <= 3
    /// 7. build_rel_graph — relationship graph, joinset anchor inference
    /// 8. compile_exprs — parse Expr fields, reject raw SQL
    /// 9. emit — serialize to CompiledManifest (with resolution result)
    pub async fn compile(&self, source: CompileSource) -> Result<CompiledManifest, CompileError> {
        // Load YAML source(s) into a single string
        let yaml_source = self.load_source(source)?;

        // Compute source hash
        let source_hash = {
            let mut hasher = Sha256::new();
            hasher.update(yaml_source.as_bytes());
            format!("{:x}", hasher.finalize())
        };

        // Step 1: Parse
        let model = parse(&yaml_source)?;

        // Step 2: Resolve refs
        let model = resolve_refs(model)?;

        // Step 3: Resolve sources — expand globs/wildcards, fetch catalog metadata.
        // All physical binding (glob expansion, catalog metadata fetch) happens here.
        let resolution = steps::resolve_sources(
            &model,
            self.catalog_registry.as_ref(),
            self.catalog.as_deref(),
            self.storage.as_deref(),
        )
        .await?;

        // Step 4: Validate structure
        steps::validate_structure(&model)?;

        // Step 4.6: Validate temporal equivalence
        steps::validate_temporal_equivalence(&model)?;

        // Step 4.7: Validate storage config (paths/tables exclusivity, non-empty sources)
        steps::validate_storage(&model)?;

        // Step 4.8: Validate metadata dimensions (path/partition preconditions)
        steps::validate_metadata_dimensions(&model)?;

        // Step 4.5: Expand auto column mappings (before validation)
        let mut model = model;
        steps::expand_auto_mappings(&mut model);

        // Step 4.55: Validate temporal.dimension consistency across datasets
        steps::validate_temporal_dimension_consistency(&model)?;

        // Step 4.9: Derive dimension grains from dataset temporal configs
        let mut derivation_warnings = Vec::new();
        steps::derive_dimension_grains(&mut model, &mut derivation_warnings);

        // Step 5: Validate mappings
        steps::validate_mappings(&model)?;

        // Step 5.5: Validate grain compatibility
        steps::validate_grain_compatibility(&model)?;

        // Step 6: Build metric graph (cycle detection, depth check)
        let metric_depths = steps::build_metric_graph(&model)?;

        // Step 7: Build relationship graph
        let _rel_graph = steps::build_rel_graph(&model)?;

        // Step 8: Compile expressions
        // Step 9: Emit CompiledManifest (with resolution result for populated ResolvedSources)
        let manifest = steps::emit(model, source_hash, &metric_depths, resolution, derivation_warnings)?;

        let manifest = CompiledManifest {
            compiled_at: Utc::now(),
            ..manifest
        };

        Ok(manifest)
    }

    /// Load source YAML into a single string.
    fn load_source(&self, source: CompileSource) -> Result<String, CompileError> {
        match source {
            CompileSource::Yaml(s) => Ok(s),
            CompileSource::YamlFiles(paths) => {
                let mut combined = String::new();
                for path in paths {
                    let content = std::fs::read_to_string(&path)?;
                    combined.push_str(&content);
                    combined.push('\n');
                }
                Ok(combined)
            }
            CompileSource::Directory(dir) => {
                let mut combined = String::new();
                let mut entries: Vec<_> = std::fs::read_dir(&dir)?
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|ext| ext == "yaml" || ext == "yml")
                            .unwrap_or(false)
                    })
                    .collect();
                entries.sort_by_key(|e| e.path());
                for entry in entries {
                    let content = std::fs::read_to_string(entry.path())?;
                    combined.push_str(&content);
                    combined.push('\n');
                }
                if combined.is_empty() {
                    return Err(CompileError::Parse(format!(
                        "no YAML files found in directory: {}",
                        dir.display()
                    )));
                }
                Ok(combined)
            }
        }
    }

}

impl Default for ManifestCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ManifestCompiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManifestCompiler")
            .field("has_catalog", &self.catalog.is_some())
            .field("has_catalog_registry", &self.catalog_registry.is_some())
            .field("has_storage", &self.storage.is_some())
            .finish()
    }
}
