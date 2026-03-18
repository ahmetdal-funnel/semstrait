# semstrait-connectors

Compute connector traits and feature-gated engine implementations.

---

## Responsibility

Defines the three-phase compute pipeline:

1. **`ComputeEmitter`** — `LogicalPlan` → `ComputePayload` (SQL or Substrait bytes)
2. **`ComputeAdapter`** — adapts payload based on `ConsumerProfile` capabilities
3. **`ComputeConnector`** — async execution → `ComputeResult`

Does not own the compilation pipeline. Receives a `LogicalPlan` from the planner and routes execution to the appropriate engine.

---

## Key Types

```rust
#[async_trait]
pub trait ComputeEmitter: Send + Sync {
    fn name(&self) -> &str;
    fn supported_payloads(&self) -> Vec<PayloadKind>;
    fn emit(&self, plan: &LogicalPlan) -> Result<ComputePayload, EmitError>;
}

#[async_trait]
pub trait ComputeAdapter: Send + Sync {
    fn adapt(&self, payload: ComputePayload, profile: &ConsumerProfile)
        -> Result<ComputeRequest, AdaptError>;
}

#[async_trait]
pub trait ComputeConnector: Send + Sync {
    fn name(&self) -> &str;
    async fn health_check(&self) -> Result<(), ConnectorError>;
    async fn execute(&self, request: &ComputeRequest)
        -> Result<ComputeResult, ConnectorError>;
}
```

### ComputeResult

```rust
pub struct ComputeResult {
    pub data: ComputeResultData,
    pub stats: ExecutionStats,
}

pub enum ComputeResultData {
    Empty,
    Json(Vec<serde_json::Value>),
    Native(Box<dyn Any + Send + Sync>),  // downcasted via as_native::<T>()
}
```

---

## Included Connectors

### DataFusion (`feature = "datafusion"`)

Full SQL execution via DataFusion's `SessionContext`. Implements all three traits.

- Accepts: `PayloadKind::Sql`
- Returns: `ComputeResultData::Native` wrapping `ArrowBatches(Vec<RecordBatch>)`
- Uses DataFusion's re-exported Arrow types (no separate arrow dependency)

```rust
use semstrait_connectors::datafusion::{DataFusionConnector, ArrowBatches};

let connector = DataFusionConnector::new();
let result = connector.execute(&request).await?;
let batches = result.data.as_native::<ArrowBatches>().unwrap();
```

### DuckDB

Embedded DuckDB connector via `duckdb` crate v1.3.2 (Arrow 55, `bundled` feature). Uses `Arc<Mutex<Connection>>` + `spawn_blocking` for async safety. Supports CSV/Parquet file registration via `read_csv_auto()`/`read_parquet()`.

```rust
use semstrait_connectors::duckdb::DuckDbConnector;

let connector = DuckDbConnector::new()?;
connector.register_csv("orders", "data/orders.csv").await?;
let result = connector.execute(request).await?;
```

### Trino, Spark (stubs)

Feature flags exist (`trino`, `spark`) but implementations are not yet wired. Connector traits are ready for implementation.

---

## Dependencies

- `semstrait-core` — `ConsumerProfile`, `DataType`
- `semstrait-ir` — `LogicalPlan`, `PlanNode`
- `semstrait-sql` — SQL emission for SQL-based connectors
- `datafusion` v52 (optional, feature-gated)
- `duckdb` v1.3.2 (optional, feature-gated, `bundled`)
- `arrow` v55 (optional, for JSON serialization)
