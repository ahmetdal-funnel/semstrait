# semstrait

Facade crate — single entry point for library consumers.

Re-exports key types from all subsystems under a unified namespace. Most users depend on this crate rather than individual crates directly. Bridges adapter and planner via profile extraction (DL-059).

---

## Usage

```rust
use semstrait::SemstraitBuilder;
use semstrait_adapter::engines::DataFusionAdapter;

// Library mode — get a PlanArtifact, consumer owns execution
let sem = SemstraitBuilder::new()
    .with_manifest_yaml(yaml_string)
    .with_adapter(Arc::new(DataFusionAdapter::new()))
    .build()
    .await?;

let artifact = sem.plan(&request)?;  // PlanArtifact::Substrait

// Full mode — semstrait handles execution
let sem = SemstraitBuilder::new()
    .with_manifest_yaml(yaml_string)
    .with_adapter(Arc::new(DuckDbAdapter::new()))
    .with_connector(Arc::new(DuckDbConnector::new()?))
    .build()
    .await?;

let result = sem.query(&request).await?;  // ComputeResult
```

---

## Re-exports

```rust
// Core types
pub use semstrait_core::{DefaultProfile, DataType, Grain, Schema, SchemaColumn};

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
| `duckdb` (default) | DuckDB adapter + connector |
| `datafusion` | DataFusion adapter + connector |
| `trino` | Trino adapter + connector |
| `spark` | Spark adapter + connector (structural) |
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
- `.with_adapter(adapter)` — set engine adapter (extracts profile for planner)
- `.with_connector(connector)` — set compute connector for query execution
- `.build().await` — compile manifest, extract profile from adapter, build planner

`SemstraitInstance` provides:

- `.manifest()` — access the compiled manifest
- `.manifest_yaml()` — access the raw manifest YAML
- `.explain(&request)` — plan + emit debug SQL (synchronous, always ANSI)
- `.plan(&request)` — plan + adapt → `PlanArtifact` (library mode, requires adapter)
- `.query(&request).await` — plan + adapt + execute → `ComputeResult` (requires adapter + connector)
