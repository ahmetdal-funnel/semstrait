# semstrait-api

API layer providing gRPC, REST, and CLI entry points for semstrait.

All transports share the `SemstraitEngine` orchestrator and `RequestParser` for consistent request handling. Each transport is feature-gated.

---

## SemstraitEngine

The central orchestrator that coordinates manifest compilation, query planning, SQL emission, and connector execution:

```rust
pub struct SemstraitEngine { .. }

impl SemstraitEngine {
    // Construction
    pub fn new() -> Self;                                         // no manifest
    pub fn with_manifest(manifest: CompiledManifest) -> Self;     // pre-compiled
    pub fn with_connector(manifest: CompiledManifest, connector: Arc<dyn ComputeConnector>) -> Self;
    pub async fn with_manifest_yaml(yaml: &str) -> Result<Self, EngineError>;

    // Operations
    pub fn validate(&self, raw: &RawQueryRequest) -> ValidationResult;
    pub async fn explain(&self, raw: &RawQueryRequest) -> Result<ExplainResult, EngineError>;
    pub async fn query(&self, raw: &RawQueryRequest) -> Result<serde_json::Value, EngineError>;

    // Observability
    pub async fn check_schema_drift(&self, catalog: &dyn CatalogProvider, namespace: &str)
        -> Vec<PlannerWarning>;
}
```

---

## Unified Query API

All transports accept the same `RawQueryRequest`:

```rust
pub struct RawQueryRequest {
    pub from: String,           // entity (kind) to query
    pub select: Vec<String>,    // semantic names -- auto-classified into dims/measures/metrics
    pub filters: Vec<String>,   // named filters from the manifest
    pub grain: Option<String>,  // temporal grain override
    pub limit: Option<u64>,
    pub order_by: Vec<RawOrderBy>,
    pub session: HashMap<String, String>,
}
```

---

## Transports

### CLI (`feature = "cli"`)

Command-line interface via `clap`:

```
semstrait explain --model model.yaml --from orders --select region revenue
semstrait query   --model model.yaml --from orders --select region revenue --connector datafusion
semstrait validate --model model.yaml --from orders --select region revenue
```

### REST (`feature = "rest"`)

HTTP API via `axum`:

```
POST /v1/explain    { "from": "orders", "select": ["region", "revenue"] }
POST /v1/query      { "from": "orders", "select": ["region", "revenue"] }
POST /v1/validate   { "from": "orders", "select": ["region", "revenue"] }
GET  /health
```

### gRPC (`feature = "grpc"`)

gRPC service via `tonic`. Proto definition in `proto/service.proto`.

---

## Dependencies

- `semstrait-core` -- shared primitives
- `semstrait-ir` -- `LogicalPlan`, `SubstraitSerializer`
- `semstrait-sql` -- `SqlEmitter`, `AnsiSqlEmitter`
- `semstrait-planner` -- `SemanticPlanner`, `ResolvedQueryRequest`
- `semstrait-manifest` -- `ManifestCompiler`, `CompiledManifest`
- `semstrait-connectors` -- `ComputeConnector`, `ComputePayload`
- `semstrait-catalog` -- `CatalogProvider`
- `clap` (optional, behind `cli`) -- argument parsing
- `axum`, `tower` (optional, behind `rest`) -- HTTP server
- `tonic`, `prost` (optional, behind `grpc`) -- gRPC server
