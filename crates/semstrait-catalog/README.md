# semstrait-catalog

Catalog metadata access abstraction for semstrait.

Provides the `CatalogProvider` trait for abstracting catalog metadata access. Used by `ManifestCompiler` for glob expansion and schema validation, and by `SemstraitEngine` for schema drift detection.

---

## CatalogProvider Trait

```rust
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    async fn list_tables(&self, namespace: &str, pattern: &GlobPattern)
        -> Result<Vec<TableRef>, CatalogError>;
    async fn get_schema(&self, table: &TableRef)
        -> Result<Vec<CatalogColumn>, CatalogError>;
    async fn table_exists(&self, table: &TableRef)
        -> Result<bool, CatalogError>;
    async fn load_table_metadata(&self, table: &TableRef)
        -> Result<TableMetadata, CatalogError>;
}
```

---

## Implementations

| Provider | Feature | Description |
|----------|---------|-------------|
| `NullCatalogProvider` | *(always)* | No-op: returns empty results. For testing and stateless operations. |
| `IcebergRestCatalog` | `iceberg` | Iceberg REST API client. OAuth2/Bearer auth, glob expansion. |
| `UnityCatalogProvider` | `unity` | Databricks Unity Catalog REST API. PAT/Bearer auth, pagination. |
| `NullStorageProvider` | *(always)* | No-op storage: returns empty. For testing. |
| `LocalStorageProvider` | `local` | Local filesystem glob + schema reading |
| `S3StorageProvider` | `s3` | S3 storage (stub) |

### NullCatalogProvider

```rust
use semstrait_catalog::{CatalogProvider, NullCatalogProvider, TableRef};

let catalog = NullCatalogProvider;
let exists = catalog.table_exists(&TableRef::new("default", "orders")).await?;
assert!(!exists);
```

---

## Key Types

| Type | Description |
|------|-------------|
| `TableRef` | Reference to a table: namespace + name |
| `CatalogColumn` | Column metadata: name, data type, nullability, optional comment |
| `CatalogError` | Error types: not found, auth failure, connection, etc. |

---

## StorageProvider Trait

Defined in `storage.rs`. Abstracts storage-level operations for glob expansion and schema reading during compilation.

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn expand_glob(&self, pattern: &str) -> Result<Vec<String>, CatalogError>;
    async fn read_schema(&self, path: &str) -> Result<Vec<CatalogColumn>, CatalogError>;
}
```

---

## CatalogRegistry

Defined in `registry.rs`. A named catalog provider registry that allows multiple catalog backends to coexist, resolved by name at compile time.

---

## Storage Provider Implementations

| Provider | Feature | Description |
|----------|---------|-------------|
| `NullStorageProvider` | *(always)* | No-op storage: returns empty. For testing. |
| `LocalStorageProvider` | `local` | Local filesystem glob + schema reading |
| `S3StorageProvider` | `s3` | S3 storage (stub) |

---

## Dependencies

- `semstrait-core` -- `GlobPattern`, `DataType`
- `async-trait` -- async trait support
- `reqwest` (optional, behind `iceberg`/`unity`) -- HTTP client
- `serde_json` (optional, behind `iceberg`/`unity`) -- JSON parsing
- `glob` (optional, behind `local`) -- filesystem glob expansion
