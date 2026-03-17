//! ManifestCompiler — orchestrates the 9-step compilation pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};

use semstrait_catalog::CatalogProvider;
use semstrait_model::{parse, resolve_refs};

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

/// The manifest compiler. Orchestrates the 9-step pipeline.
pub struct ManifestCompiler {
    catalog: Option<Arc<dyn CatalogProvider>>,
}

impl ManifestCompiler {
    /// Create a new compiler with no catalog provider.
    pub fn new() -> Self {
        Self { catalog: None }
    }

    /// Set a catalog provider (required for glob expansion).
    pub fn with_catalog(mut self, c: Arc<dyn CatalogProvider>) -> Self {
        self.catalog = Some(c);
        self
    }

    /// Run the 9-step compilation pipeline.
    ///
    /// Steps:
    /// 1. parse — serde_yaml -> SemanticModel
    /// 2. resolve_refs — expand ref: entries
    /// 3. expand_globs — GlobPattern -> concrete datasets (requires catalog)
    /// 4. validate_structure — dataset uniqueness, kind nesting, joinset anchors
    /// 5. validate_mappings — column_mapping keys exist in kind interface
    /// 6. build_metric_graph — petgraph DiGraph, cycle detection, depth <= 3
    /// 7. build_rel_graph — relationship graph, joinset anchor inference
    /// 8. compile_exprs — parse DslExpr fields, reject raw SQL
    /// 9. emit — serialize to CompiledManifest
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

        // Step 3: Expand globs
        let model = steps::expand_globs(model, self.catalog.as_deref()).await?;

        // Step 4: Validate structure
        steps::validate_structure(&model)?;

        // Step 5: Validate mappings
        steps::validate_mappings(&model)?;

        // Step 6: Build metric graph (cycle detection, depth check)
        let metric_depths = steps::build_metric_graph(&model)?;

        // Step 7: Build relationship graph
        let _rel_graph = steps::build_rel_graph(&model)?;

        // Step 8: Compile expressions
        // Step 9: Emit CompiledManifest
        let manifest = steps::emit(model, source_hash, &metric_depths)?;

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
