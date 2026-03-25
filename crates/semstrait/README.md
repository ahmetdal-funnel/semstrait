# semstrait

Facade crate — single entry point for library consumers.

Re-exports key types from all subsystems under a unified namespace. Most users depend on this crate rather than individual crates directly.

---

## Usage

```rust
use semstrait::SemstraitBuilder;

let sem = SemstraitBuilder::new()
    .with_manifest_yaml(yaml_string)
    .build()
    .await?;

let sql = sem.explain(&request)?;
```

---

## Re-exports

```rust
// Core types
pub use semstrait_core::{ConsumerProfile, DataType, Grain, Schema, SchemaColumn};

// IR types
pub use semstrait_ir::{LogicalPlan, PlanArtifact};

// Adapter types
pub use semstrait_adapter::{AdaptError, EngineAdapter};

// Connector traits
pub use semstrait_connectors::{ComputeConnector, ComputeResult, ComputeResultData};

// Catalog traits
pub use semstrait_catalog::{CatalogProvider, NullCatalogProvider, TableRef};

// Builder API
pub use builder::{BuildError, SemstraitBuilder, SemstraitInstance};
```

---

## Feature Flags

| Feature | Adds |
|---------|------|
| `duckdb` (default) | DuckDB connector |
| `datafusion` | DataFusion SQL execution |
| `trino` | Trino connector (stub) |
| `spark` | Spark connector (stub) |
| `catalog-iceberg` | Iceberg REST catalog |
| `api-rest` | REST transport (axum) |
| `api-cli` | CLI transport (clap) |
| `api-grpc` | gRPC transport (tonic) |

---

## Builder API

`SemstraitBuilder` compiles a YAML manifest and produces a `SemstraitInstance`:

- `.with_manifest_yaml(yaml)` — inline YAML string
- `.with_manifest_file(path)` — load from file
- `.with_catalog(provider)` — set catalog provider
- `.with_connector(connector)` — set compute connector for query execution
- `.build().await` — compile and return instance

`SemstraitInstance` provides:

- `.manifest()` — access the compiled manifest
- `.manifest_yaml()` — access the raw manifest YAML
- `.explain(&request)` — plan + emit SQL (synchronous; uses connector adapter if set, ANSI fallback otherwise)
- `.query(&request).await` — plan + execute via the configured connector
