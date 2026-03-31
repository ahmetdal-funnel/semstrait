# semstrait-connectors

Compute connector traits and feature-gated engine implementations.

---

## Responsibility

Provides the `ComputeConnector` trait and concrete implementations for executing `PlanArtifact`s (SQL or Substrait) against compute engines. Connectors are fully independent from adapters — pure artifact execution (DL-056).

Does not own the compilation or planning pipeline. Receives a `PlanArtifact` from upstream and routes execution to the appropriate engine.

---

## Key Types

### ComputeConnector (traits.rs)

```rust
#[async_trait]
pub trait ComputeConnector: Send + Sync {
    /// Execute a plan artifact against the compute engine.
    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError>;

    /// Health check -- verify the engine is reachable.
    async fn health_check(&self) -> Result<(), ConnectorError>;

    /// Human-readable connector name.
    fn name(&self) -> &str;
}
```

### Result types (payload.rs)

```rust
pub struct ComputeResult {
    pub complete: bool,
    pub stats: ExecutionStats,
    pub data: ComputeResultData,
}

pub enum ComputeResultData {
    Empty,
    Json(Vec<serde_json::Value>),
    Native(Box<dyn Any + Send + Sync>),  // downcastable via as_native::<T>()
}

pub struct ExecutionStats {
    pub rows_returned: u64,
    pub execution_time: Option<Duration>,
    pub bytes_scanned: Option<u64>,
}

pub enum ConnectorError {
    Connection(String),
    Execution(String),
    Timeout(Duration),
    NotImplemented(String),
    Internal(String),
}
```

---

## Included Connectors

### DataFusion (`feature = "datafusion"`)

Dual-path execution via DataFusion's `SessionContext`. Accepts both SQL and Substrait artifacts natively.

- Accepts: `PlanArtifact::Sql` (via `ctx.sql()`) and `PlanArtifact::Substrait` (via `datafusion-substrait::from_substrait_plan()`)
- Returns: `ComputeResultData::Json`
- Supports registering CSV, Parquet, and in-memory tables
- `register_manifest_sources()` auto-registers sources using planner's name priority (table_fqn → reference → dataset_name)
- Also exposes `ArrowBatches` helper with `to_json_rows()` for custom use

```rust
use semstrait_connectors::datafusion::DataFusionConnector;

let connector = DataFusionConnector::new();
connector.register_csv("orders", "data/orders.csv").await?;
// Executes Substrait plans natively — no SQL fallback needed
let result = connector.execute(&artifact).await?;
```

### DuckDB (`feature = "duckdb"`)

Embedded DuckDB connector. Uses `Arc<Mutex<Connection>>` + `spawn_blocking` for async safety (`Connection` is `Send` but `!Sync`). Converts Arrow batches to JSON via the workspace `arrow` crate.

- Requires: `PlanArtifact::Sql`
- Returns: `ComputeResultData::Json`
- Supports in-memory and file-backed databases
- Supports registering CSV and Parquet files via `read_csv_auto()`/`read_parquet()`

```rust
use semstrait_connectors::duckdb::DuckDbConnector;

let connector = DuckDbConnector::new()?;
connector.register_csv("orders", "data/orders.csv").await?;
let result = connector.execute(&artifact).await?;
```

### Trino (`feature = "trino"`)

REST API connector targeting Trino's `/v1/statement` endpoint. Submits SQL, polls `nextUri` for paginated results, and collects rows into JSON objects. Supports `None`, `Basic`, and `BearerToken` authentication.

- Requires: `PlanArtifact::Sql`
- Returns: `ComputeResultData::Json`

```rust
use semstrait_connectors::trino::TrinoConnector;

let connector = TrinoConnector::new("http://trino:8080", "hive", "default")
    .with_user("alice")
    .with_bearer_token("token123");
let result = connector.execute(&artifact).await?;
```

### Spark (`feature = "spark"`)

Structural implementation targeting Spark Connect (gRPC, Spark 3.4+). The `ComputeConnector` trait is implemented but `execute()` and `health_check()` return `ConnectorError::NotImplemented` pending `spark-connect-rs` or custom proto integration.

```rust
use semstrait_connectors::spark::SparkConnector;

let connector = SparkConnector::new("sc://spark:15002");
// execute() currently returns NotImplemented
```

---

## Feature Flags

| Feature | Enables | Key Dependencies |
|---|---|---|
| `datafusion` | `DataFusionConnector` | `datafusion` v52, `datafusion-substrait` v52, `tokio` |
| `duckdb` | `DuckDbConnector` | `duckdb` >=1.3.0 <1.4.0 (bundled), `arrow` v55, `tokio` |
| `trino` | `TrinoConnector` | `reqwest`, `serde`, `tokio` |
| `spark` | `SparkConnector` | `uuid` |
| `polyglot` | Polyglot SQL dialect support in adapter | (transitive via `semstrait-adapter`) |

---

## Dependencies

- `semstrait-core` -- shared types (`DataType`, `Schema`)
- `semstrait-ir` -- `PlanArtifact` (SQL string or Substrait plan)
- `async-trait` -- async trait support
- `thiserror` -- `ConnectorError` derive
- `serde_json` -- JSON result serialization

**Note:** `semstrait-adapter` is NOT a dependency. Connectors and adapters are independent (DL-056).
