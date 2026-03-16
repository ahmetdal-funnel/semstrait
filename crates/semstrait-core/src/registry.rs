//! Schema registry — loads and caches semantic model definitions.
//!
//! The `SchemaRegistry` trait abstracts over model storage. The default
//! implementation reads YAML files from the filesystem.

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use crate::diagnostics::CompileError;
use crate::parser;
use crate::schema::model::SemanticModelFile;

/// Reference to a semantic model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelRef {
    /// Model name (matches `semantic_model.name` in the YAML).
    pub name: String,
}

impl ModelRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Trait for loading semantic model definitions.
pub trait SchemaRegistry {
    /// Load a semantic model by reference.
    fn load(&self, model_ref: &ModelRef) -> Result<SemanticModelFile, CompileError>;
}

/// Loads models from YAML files on the filesystem.
///
/// Convention: model name "my_model" maps to `{base_dir}/my_model.yaml`.
pub struct FileSystemRegistry {
    base_dir: PathBuf,
}

impl FileSystemRegistry {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn model_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.base_dir.join(format!("{}.yaml", model_ref.name))
    }
}

impl SchemaRegistry for FileSystemRegistry {
    fn load(&self, model_ref: &ModelRef) -> Result<SemanticModelFile, CompileError> {
        let path = self.model_path(model_ref);
        let model = parser::parse_file(&path)?;
        Ok(model)
    }
}

/// In-memory registry for testing. Stores pre-parsed models.
pub struct InMemoryRegistry {
    models: std::collections::HashMap<String, SemanticModelFile>,
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self {
            models: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, name: impl Into<String>, model: SemanticModelFile) {
        self.models.insert(name.into(), model);
    }
}

impl SchemaRegistry for InMemoryRegistry {
    fn load(&self, model_ref: &ModelRef) -> Result<SemanticModelFile, CompileError> {
        self.models.get(&model_ref.name).cloned().ok_or_else(|| {
            CompileError::single(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::codes::PARSE_E001,
                format!("model '{}' not found in registry", model_ref.name),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filesystem_registry_loads() {
        let base = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data"
        ));
        let registry = FileSystemRegistry::new(base);
        let model = registry.load(&ModelRef::new("minimal")).unwrap();
        assert_eq!(model.semantic_model.name, "minimal_test");
    }

    #[test]
    fn test_filesystem_registry_missing_model() {
        let base = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data"
        ));
        let registry = FileSystemRegistry::new(base);
        let err = registry.load(&ModelRef::new("nonexistent")).unwrap_err();
        assert!(err.to_string().contains("PARSE_E001"));
    }

    #[test]
    fn test_in_memory_registry() {
        let yaml = r#"
semantic_model:
  name: test_model
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parser::parse_str(yaml).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("test_model", model);

        let loaded = registry.load(&ModelRef::new("test_model")).unwrap();
        assert_eq!(loaded.semantic_model.name, "test_model");
    }
}
