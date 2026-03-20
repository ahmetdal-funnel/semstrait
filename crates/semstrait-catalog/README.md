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
}
```

---

## Implementations

| Provider | Feature | Description |
|----------|---------|-------------|
| `NullCatalogProvider` | *(always)* | No-op: returns empty results. For testing and stateless operations. |
| `IcebergRestCatalog` | `iceberg` | Iceberg REST API client. OAuth2/Bearer auth, glob expansion. |
| `UnityCatalogProvider` | `unity` | Databricks Unity Catalog REST API. PAT/Bearer auth, pagination. |

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

## Dependencies

- `semstrait-core` -- `GlobPattern`, `DataType`
- `async-trait` -- async trait support
- `reqwest` (optional, behind `iceberg`/`unity`) -- HTTP client
- `serde_json` (optional, behind `iceberg`/`unity`) -- JSON parsing
