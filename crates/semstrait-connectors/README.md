# semstrait-connectors

Engine execution adapters for semstrait. Takes a `CompiledPlan` and executes it against a real compute engine.

---

## Responsibility

`semstrait-connectors` owns the boundary between compiled query representation (Substrait bytes or SQL) and query results (rows or Arrow record batches).

It does not own the compilation pipeline. It receives a `CompiledPlan` from `semstrait-core` and routes execution to the appropriate engine adapter.

---

## Architecture

```
CompiledPlan  (from semstrait-core)
      ↓
ConnectorAdapter::execute(ExecutableQuery, ExecContext)
      ↓
ConnectorResult  { Json(rows) | Arrow(record_batches) }
```

`ExecutableQuery` carries either the Substrait bytes or the dialect SQL from `CompiledPlan` — whichever the adapter declares it accepts. The adapter declares its accepted input formats via `accepted_inputs()`.

---

## Key types

```rust
/// The adapter contract. One implementation per engine.
#[async_trait]
pub trait ConnectorAdapter: Send + Sync {
    /// Unique adapter identifier, e.g. "duckdb-http", "flightsql"
    fn id(&self) -> &str;

    /// Which input formats this adapter can execute
    fn accepted_inputs(&self) -> &[InputKind];

    /// Execute a compiled query and return results
    async fn execute(
        &self,
        query: &ExecutableQuery,
        ctx: &ExecContext,
    ) -> Result<ConnectorResult, ConnectorError>;

    /// Verify the engine is reachable
    async fn health_check(&self) -> Result<(), ConnectorError>;
}

/// What the adapter receives for execution
pub enum InputKind {
    Sql(Dialect),         // SQL string in the specified dialect
    SubstraitBytes,       // raw Substrait protobuf bytes
}

/// What the adapter produces
pub enum ConnectorResult {
    Json(JsonResult),     // Vec<serde_json::Value> rows
    Arrow(ArrowResult),   // Vec<arrow::RecordBatch> (feature = "arrow")
}

/// Execution context — per-request metadata the adapter may use
pub struct ExecContext {
    pub request_id: String,
    pub timeout: Option<Duration>,
    pub engine_params: HashMap<String, String>,  // engine-specific hints
}
```

---

## Included adapters

### PassthroughAdapter (v1)

Posts SQL to any HTTP SQL endpoint. Compatible with:
- DuckDB HTTP server (`--listen` mode)
- ClickHouse HTTP interface
- Any service implementing the simple `POST /query → JSON rows` contract

```rust
pub struct PassthroughAdapter {
    endpoint: String,
    dialect: Dialect,
    client: reqwest::Client,
}
```

Accepts: `InputKind::Sql(dialect)`. Returns: `ConnectorResult::Json`. No engine SDK required — pure HTTP.

### FlightSqlAdapter (feature = "flight", stub in v1)

Connects to a FlightSQL gRPC endpoint and executes Substrait bytes directly. The engine (DataFusion, Velox, etc.) handles planning from the Substrait input.

Accepts: `InputKind::SubstraitBytes`. Returns: `ConnectorResult::Arrow`.

Full implementation requires `arrow-flight` with the `flight-sql` feature. The struct and trait impl are present in v1 but `execute()` returns `ConnectorError::NotImplemented` until wired up in a subsequent iteration.

---

## Transport data model

The adapter abstraction is deliberately narrow. `semstrait-connectors` does not:
- Own the Arrow schema (that's carried in `ArrowResult`)
- Serialize results to HTTP responses (that's `semstrait-http`)
- Cache results (a wrapper adapter can do this without touching the core adapter)

This keeps the adapter implementation surface small. Adding a new engine means implementing four methods.
