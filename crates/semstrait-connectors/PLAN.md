# semstrait-connectors — Implementation Plan

Phase 4 of workspace plan. Depends on Phase 3 (semstrait-sql) being complete.

---

## Phase 4.1 — Trait and data types

**Task:** Define `ConnectorAdapter`, `InputKind`, `ConnectorResult`, `ExecContext`, `ConnectorError`, `ExecutableQuery`.

`ExecutableQuery` is constructed from `CompiledPlan`:

```rust
pub struct ExecutableQuery {
    pub substrait: Vec<u8>,           // always present (canonical)
    pub sql: Option<(Dialect, String)>, // present when adapter prefers SQL
    pub output_schema: Vec<OutputColumn>, // for result column naming
}

impl ExecutableQuery {
    pub fn from_plan(plan: &CompiledPlan) -> Self {
        Self {
            substrait: plan.substrait().to_vec(),
            sql: plan.sql().map(|s| (plan.dialect().unwrap(), s.to_owned())),
            output_schema: plan.output_schema().to_vec(),
        }
    }
}
```

`ConnectorError` carries an `InputKind` in the `UnsupportedInput` variant so callers can retry with a different format:

```rust
pub enum ConnectorError {
    UnsupportedInput(InputKind),
    TransportError(String),
    ExecutionError(String),
    NotImplemented,
}
```

**Deliverable:** Trait compiles. No adapters yet.

---

## Phase 4.2 — PassthroughAdapter

**Task:** Implement `PassthroughAdapter` using `reqwest`.

The adapter posts the SQL string to the configured HTTP endpoint and deserialises the response as JSON rows. The response format assumed is `{"rows": [...]}` — the simplest possible contract.

```rust
impl ConnectorAdapter for PassthroughAdapter {
    fn accepted_inputs(&self) -> &[InputKind] {
        &[InputKind::Sql(self.dialect.clone())]
    }

    async fn execute(&self, query: &ExecutableQuery, ctx: &ExecContext)
        -> Result<ConnectorResult, ConnectorError>
    {
        let (_, sql) = query.sql.as_ref()
            .ok_or(ConnectorError::UnsupportedInput(InputKind::SubstraitBytes))?;

        let resp = self.client
            .post(&self.endpoint)
            .timeout(ctx.timeout.unwrap_or(Duration::from_secs(30)))
            .json(&json!({"query": sql}))
            .send().await
            .map_err(|e| ConnectorError::TransportError(e.to_string()))?;

        let rows: Vec<serde_json::Value> = resp.json().await
            .map_err(|e| ConnectorError::ExecutionError(e.to_string()))?;

        Ok(ConnectorResult::Json(JsonResult { rows, schema: query.output_schema.clone() }))
    }
}
```

Integration test: spin up a mock HTTP server (use `wiremock` or `httpmock`) that accepts a POST, asserts the SQL body, and returns canned rows. Assert `ConnectorResult::Json` rows match.

---

## Phase 4.3 — FlightSQL stub

**Task:** Add `FlightSqlAdapter` struct with `accepted_inputs = [InputKind::SubstraitBytes]`. `execute()` returns `ConnectorError::NotImplemented`. `health_check()` attempts gRPC connection and returns `Ok` or `TransportError`.

Gate behind `feature = "flight"` in `Cargo.toml`:

```toml
[features]
flight = ["dep:arrow-flight", "dep:tonic"]
```

This keeps the default dependency surface lean. DataFusion and similar engines will use this feature. Simple HTTP integrations won't.

The stub is valuable even before implementation: it documents the intended interface, tests that the feature compiles, and lets downstream code reference `FlightSqlAdapter` in configurations without runtime errors (it will gracefully return `NotImplemented` until the implementation lands).

---

## Phase 4.4 — Adapter registry (optional, low priority)

A map of `adapter_id → Box<dyn ConnectorAdapter>` to support multi-engine configs in the HTTP server. Not needed in v1 where the HTTP server is configured with a single adapter. Implement when the HTTP server configuration needs it.
