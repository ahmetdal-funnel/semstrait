# semstrait-manifest

Manifest compiler and repository for semstrait semantic models.

Compiles `SemanticModel` (parsed YAML) into a validated `CompiledManifest` via a 9-step pipeline. Provides repository implementations for storing and retrieving compiled manifests.

---

## Compilation Pipeline

```
CompileSource (YAML string, files, or directory)
       |
  1. parse              serde_yaml -> SemanticModel
  2. resolve_refs       expand ref: entries to inline definitions
  3. expand_globs       GlobPattern -> concrete datasets (requires catalog)
  4. validate_structure dataset uniqueness, kind nesting, joinset anchors
  4.5 expand_auto       ColumnMapping::Auto -> identity mapping
  5. validate_mappings  column_mapping keys exist in kind interface
  6. build_metric_graph petgraph DiGraph, cycle detection, depth <= 3
  7. build_rel_graph    relationship graph, joinset anchor inference
  8. compile_exprs      parse Expr DSL fields, reject raw SQL
  9. emit               serialize to CompiledManifest
```

Each step is a pure function in the `steps` module. The compiler also:
- Computes a SHA-256 hash of the source for cache invalidation
- Records `compiled_at` timestamp
- Captures schema snapshots from catalog (when available) for drift detection

---

## Key Types

```rust
// Source input variants.
pub enum CompileSource {
    Yaml(String),
    YamlFiles(Vec<PathBuf>),
    Directory(PathBuf),
}

// The compiler.
pub struct ManifestCompiler { .. }

impl ManifestCompiler {
    pub fn new() -> Self;
    pub fn with_catalog(self, c: Arc<dyn CatalogProvider>) -> Self;
    pub async fn compile(&self, source: CompileSource) -> Result<CompiledManifest, CompileError>;
}

// The compiled output.
pub struct CompiledManifest {
    pub name: String,
    pub kinds: HashMap<String, CompiledKind>,
    pub datasets: HashMap<String, CompiledDataset>,
    pub source_hash: String,
    pub compiled_at: DateTime<Utc>,
}
```

---

## Repository

Repositories store and retrieve compiled manifests:

```rust
pub trait Repository: Send + Sync {
    async fn get(&self, name: &str) -> Result<CompiledManifest, RepositoryError>;
    async fn put(&self, manifest: CompiledManifest) -> Result<(), RepositoryError>;
    async fn list(&self) -> Result<Vec<String>, RepositoryError>;
    async fn delete(&self, name: &str) -> Result<(), RepositoryError>;
}
```

| Implementation | Storage | Notes |
|----------------|---------|-------|
| `InMemoryRepository` | `RwLock<HashMap>` | For testing and stateless use |
| `FileSystemRepository` | JSON files | Atomic write (tmp + rename), creates parent dirs |

---

## Dependencies

- `semstrait-core` -- `DataType`, `Expr`, `GlobPattern`
- `semstrait-model` -- `SemanticModel`, `parse()`, `resolve_refs()`
- `semstrait-catalog` -- `CatalogProvider` (optional, for glob expansion and schema snapshots)
- `chrono` -- timestamps
- `sha2` -- source hashing
- `petgraph` -- metric/relationship graph analysis
