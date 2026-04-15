# semstrait-api

API layer providing gRPC, REST, and CLI entry points for semstrait.

All transports share the `SemstraitEngine` orchestrator and `RequestParser` for consistent request handling. Each transport is feature-gated.

---

## SemstraitEngine

The central orchestrator that coordinates manifest compilation, query planning, adapter-based artifact generation, and connector execution:

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

### explain() flow

1. Parse `RawQueryRequest` into `ResolvedQueryRequest`
2. Plan via `SemanticPlanner::plan()`
3. If a connector is configured, use `adapter.adapt()` for the Substrait artifact and `adapter.debug_sql()` for the SQL representation
4. Otherwise, fall back to ANSI SQL emission and direct Substrait serialization

### query() flow

1. Parse and plan (same as explain)
2. `adapter.adapt()` produces a `PlanArtifact` (SQL or Substrait depending on engine)
3. `connector.execute()` runs the artifact directly — connectors handle both artifact types natively
4. Returns JSON result with rows and execution stats

---

## Unified Query API

All transports accept the same `RawQueryRequest`:

```rust
pub struct RawQueryRequest {
    pub model: Option<String>,      // semantic model source (file path or inline YAML/JSON)
    pub from: Option<String>,       // entity to query (None = resolve from select fields)
    pub select: Vec<String>,        // semantic names -- auto-classified into dims/measures/metrics
    pub filters: Vec<String>,       // named filters from the manifest
    pub raw_filters: Vec<RawFilter>, // inline filter expressions (not implemented in v1)
    pub grain: Option<String>,      // temporal grain override
    pub limit: Option<u64>,
    pub order_by: Vec<RawOrderBy>,
    pub session: HashMap<String, String>,
    pub engine: Option<String>,     // engine for plan generation (e.g., "datafusion", "duckdb")
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

## Error Handling

`EngineError` wraps errors from each pipeline stage:

- `Parse` -- from `ParseError` (request validation)
- `Compile` -- from `semstrait-manifest`
- `Plan` -- from `semstrait-planner`
- `Emit` -- from `semstrait-sql`
- `Adapt` -- from `semstrait-adapter::AdaptError`
- `Connector` -- from `semstrait-connectors`
- `NotConfigured` -- missing manifest or connector
- `Internal` -- unexpected failures

---

## Dependencies

- `semstrait-core` -- shared primitives
- `semstrait-ir` -- `LogicalPlan`, `SubstraitSerializer`
- `semstrait-sql` -- `SqlEmitter`, `AnsiSqlEmitter`
- `semstrait-planner` -- `SemanticPlanner`, `ResolvedQueryRequest`
- `semstrait-manifest` -- `ManifestCompiler`, `CompiledManifest`
- `semstrait-adapter` -- `EngineAdapter`, `AdaptError`
- `semstrait-connectors` -- `ComputeConnector`, `ComputeResult`
- `semstrait-catalog` -- `CatalogProvider`
- `clap` (optional, behind `cli`) -- argument parsing
- `axum`, `tower` (optional, behind `rest`) -- HTTP server
- `tonic`, `prost` (optional, behind `grpc`) -- gRPC server
