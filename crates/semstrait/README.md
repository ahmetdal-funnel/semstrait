# semstrait

Facade crate — single entry point for library consumers.

Re-exports key types from internal crates under a unified namespace. Most users depend on this crate rather than individual crates directly.

---

## Usage

```rust
use semstrait::SemstraitBuilder;

// Fast path — just file paths and engine name, like the CLI
let sem = SemstraitBuilder::new()
    .with_model_file("path/to/model.yaml")
    .with_catalogs_file("path/to/catalogs.yaml")
    .with_engine("datafusion")
    .build()
    .await?;

// Explain — plan + emit debug SQL
let sql = sem.explain(&request)?;

// Plan — plan + adapt to engine-native artifact
let artifact = sem.plan(&request)?;  // PlanArtifact::Substrait or PlanArtifact::Sql
```

Without an engine or adapter, both `explain()` and `plan()` fall back to ANSI SQL emission.

---

## Re-exports

```rust
// Core types
pub use semstrait_core::{DataType, Grain, Schema, SchemaColumn};

// IR types
pub use semstrait_ir::{LogicalPlan, PlanArtifact};

// Manifest types
pub use semstrait_manifest::CompiledManifest;

// Planner types
pub use semstrait_planner::request::ResolvedQueryRequest;

// Adapter types
pub use semstrait_adapter::{AdaptError, EngineAdapter};

// Catalog traits
pub use semstrait_catalog::{CatalogProvider, CatalogRegistry, NullCatalogProvider, TableRef};

// Iceberg catalog (requires `catalog-iceberg` feature)
#[cfg(feature = "catalog-iceberg")]
pub use semstrait_catalog::IcebergRestCatalog;

// I/O utilities
pub mod io { pub use semstrait_manifest::io::{load_text, IoError}; }

// Builder API
pub use builder::{BuildError, SemstraitBuilder, SemstraitInstance};
```

---

## Feature Flags

| Feature | Adds |
|---------|------|
| `duckdb` (default) | DuckDB adapter |
| `datafusion` | DataFusion adapter |
| `spark` | Spark adapter (structural) |
| `catalog-iceberg` | Iceberg REST catalog (OAuth2, Polaris) |
| `aws` | S3 model loading + AWS Secrets Manager for catalog credentials |
| `api-rest` | REST transport (axum) |
| `api-cli` | CLI transport (clap) |
| `api-grpc` | gRPC transport (tonic) |

Features propagate to `semstrait-adapter`, `semstrait-catalog`, or `semstrait-api` respectively.

---

## Builder API

`SemstraitBuilder` compiles a YAML manifest and produces a `SemstraitInstance`.

### Fast path (string-based, mirrors CLI)

- `.with_model_file(location)` — load model from file path or S3 URI
- `.with_catalogs_file(location)` — load `catalogs.yaml` from file path or S3 URI, auto-builds registry
- `.with_engine("datafusion")` — resolve adapter by name
- `.build().await` — compile manifest, wire adapter, build planner

### Explicit path (manual provider construction)

- `.with_model(yaml)` — inline YAML model string
- `.with_catalog(provider)` — set a single catalog provider (default: `NullCatalogProvider`)
- `.with_catalog_registry(registry)` — set a named registry for multi-catalog models
- `.with_adapter(adapter)` — set engine adapter directly
- `.build().await` — compile manifest, wire adapter, build planner

### SemstraitInstance

- `.manifest()` — access the compiled manifest
- `.model_yaml()` — access the raw model YAML source
- `.explain(&request)` — plan + emit debug SQL (synchronous)
- `.plan(&request)` — plan + adapt to `PlanArtifact` (synchronous)

---

## Examples

### Fast Path — Local Model + Catalogs + Engine

```rust
use semstrait::SemstraitBuilder;

let sem = SemstraitBuilder::new()
    .with_model_file("path/to/model.yaml")
    .with_catalogs_file("path/to/catalogs.yaml")
    .with_engine("datafusion")
    .build()
    .await?;

let sql = sem.explain(&request)?;
let artifact = sem.plan(&request)?;  // PlanArtifact::Substrait
```

### Fast Path — S3 Model + S3 Catalogs (EC2 / IAM Role)

Requires features: `aws`, `catalog-iceberg`, `datafusion`

```rust
use semstrait::SemstraitBuilder;

// On EC2, AWS credentials resolve via IAM instance role automatically.
// catalogs.yaml defines catalog connections (Polaris, OAuth2, Secrets Manager).
let sem = SemstraitBuilder::new()
    .with_model_file("s3://my-bucket/models/paid_media.yaml")
    .with_catalogs_file("s3://my-bucket/config/catalogs.yaml")
    .with_engine("datafusion")
    .build()
    .await?;
```

### Fast Path — Minimal (ANSI SQL, No Catalog)

```rust
use semstrait::SemstraitBuilder;

let sem = SemstraitBuilder::new()
    .with_model_file("model.yaml")
    .build()
    .await?;

let sql = sem.explain(&request)?;  // ANSI SQL
```

### Explicit — Manual Catalog + Adapter Construction

For advanced use cases where you need direct control over provider construction:

```rust
use semstrait::{SemstraitBuilder, CatalogRegistry, IcebergRestCatalog};
use std::sync::Arc;

let catalog = IcebergRestCatalog::new("https://polaris.example.com/api/catalog")
    .with_prefix("my_warehouse")
    .with_oauth2(token_url, client_id, client_secret, scope)
    .with_custom_header("Polaris-Realm", "my-realm");

let mut registry = CatalogRegistry::new();
registry.register("polaris", Arc::new(catalog));

let sem = SemstraitBuilder::new()
    .with_model_file("s3://my-bucket/models/paid_media.yaml")
    .with_catalog_registry(registry)
    .with_adapter(Arc::new(semstrait_adapter::DataFusionAdapter))
    .build()
    .await?;
```

### Explicit — AWS Secrets Manager (IAM Role)

```rust
use semstrait::{SemstraitBuilder, CatalogRegistry};
use std::sync::Arc;

let config = semstrait_catalog::secrets::PolarisCatalogConfig {
    catalog_url: "https://polaris.example.com/api/catalog",
    secret_arn: "arn:aws:secretsmanager:us-east-1:123456789:secret:polaris-oauth",
    aws_region: Some("us-east-1"),
    warehouse: Some("my_warehouse"),
    realm: Some("my-realm"),
    scope: Some("PRINCIPAL_ROLE:ALL"),
    aws_profile: None,           // None = use EC2 instance role
    aws_access_key_id: None,
    aws_secret_access_key: None,
    aws_session_token: None,
};
let catalog = semstrait_catalog::secrets::build_polaris_catalog(&config).await?;

let mut registry = CatalogRegistry::new();
registry.register("polaris", Arc::new(catalog));

let sem = SemstraitBuilder::new()
    .with_model_file("s3://my-bucket/models/paid_media.yaml")
    .with_catalog_registry(registry)
    .build()
    .await?;
```

### Loading Model YAML from S3 Manually

```rust
use semstrait::io::load_text;

let yaml = load_text("s3://my-bucket/models/paid_media.yaml").await?;
let sem = SemstraitBuilder::new()
    .with_model(yaml)
    .build()
    .await?;
```
