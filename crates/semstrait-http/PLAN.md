# semstrait-http — Implementation Plan

Phase 5 of workspace plan. Depends on Phase 1 (semstrait-core) for the compiler trait. Phase 3 (semstrait-sql) for dialect SQL in responses. Phase 4 (semstrait-connectors) for `/execute`.

`/query`, `/validate`, `/schema`, `/health` can be built against Phase 1 only. `/execute` requires Phase 4.

---

## Phase 5.1 — axum router skeleton

**Task:** Wire up axum with empty handler stubs for all 7 routes. Shared state type:

```rust
#[derive(Clone)]
pub struct AppState {
    pub compiler: Arc<dyn SemanticCompiler>,
    pub adapter: Option<Arc<dyn ConnectorAdapter>>,
    pub config: Arc<ServerConfig>,
}
```

`AppState` is injected via axum's `State` extractor. All handlers receive it. This is the only way `semstrait-http` interacts with the compiler and adapter — through the trait objects. Swapping either requires only changing how `AppState` is constructed, not any handler code.

Add `tower_http::trace::TraceLayer` for structured request logging from day one. Request IDs via `tower_http::request_id::SetRequestIdLayer`. These are cheap to add now and painful to retrofit later.

**Deliverable:** Server starts, all routes return `501 Not Implemented`. `GET /health` returns `{"status": "ok"}`.

---

## Phase 5.2 — Request/response types

**Task:** Define serde types for all request/response bodies in `crate::types`.

Keep these types entirely separate from `semstrait-core` internal types. Map between them in the handlers. This is the anti-corruption boundary: HTTP schema evolves at HTTP pace; core types evolve at compiler pace.

Key types:
```rust
pub struct QueryRequestBody {
    pub model: String,
    pub measures: Vec<String>,
    pub dimensions: Vec<String>,
    pub filters: Option<Vec<FilterBody>>,
    pub options: Option<QueryOptions>,
}

pub struct QueryOptions {
    pub sql_dialect: Option<String>,   // string to avoid coupling to Dialect enum
    pub include_lineage: Option<bool>,
}

pub struct QueryResponseBody {
    pub substrait: String,             // base64
    pub sql: Option<String>,
    pub output_schema: Vec<ColumnInfo>,
    pub lineage: Option<serde_json::Value>,
    pub diagnostics: Vec<DiagnosticBody>,
}
```

Map `QueryRequestBody` → `semstrait_core::QueryRequest` + `CompileOpts` in handler. Map `CompiledPlan` → `QueryResponseBody` in handler. No core types leak into the HTTP response.

---

## Phase 5.3 — Implement `/query` and `/validate`

**Task:** Implement the two pure-compilation endpoints.

`POST /query` handler:
```rust
async fn handle_query(
    State(state): State<AppState>,
    Json(body): Json<QueryRequestBody>,
) -> Result<Json<QueryResponseBody>, AppError> {
    let request = map_request(&body)?;
    let opts = map_opts(&body.options)?;
    let model_ref = ModelRef::file(&body.model);

    let plan = state.compiler.compile(&model_ref, &request, &opts)
        .map_err(AppError::Compile)?;

    Ok(Json(map_response(plan)))
}
```

`AppError` is an axum `IntoResponse` impl that serializes `CompileError` diagnostics to JSON and sets the appropriate HTTP status.

`POST /validate` calls `compiler.validate()` which never returns `Err` — it accumulates all diagnostics and returns a `ValidationReport`. Map to `ValidateResponseBody { valid: bool, diagnostics: Vec<DiagnosticBody> }`.

---

## Phase 5.4 — Implement `/execute`

**Task:** Add execution path. Requires `AppState.adapter` to be `Some`.

```rust
async fn handle_execute(
    State(state): State<AppState>,
    Json(body): Json<ExecuteRequestBody>,
) -> Result<Json<ExecuteResponseBody>, AppError> {
    let adapter = state.adapter.as_ref()
        .ok_or(AppError::NoAdapter)?;

    let plan = state.compiler.compile(...)?;
    let exe_query = ExecutableQuery::from_plan(&plan);
    let ctx = ExecContext { request_id: ..., timeout: state.config.request_timeout.into(), ..Default::default() };

    let result = adapter.execute(&exe_query, &ctx).await
        .map_err(AppError::Connector)?;

    Ok(Json(map_connector_result(result)))
}
```

`AppError::NoAdapter` → 503.

---

## Phase 5.5 — `/lineage`, `/schema`, `/health`

**Task:** Implement remaining endpoints. These are straightforward mappings to compiler methods.

`/lineage` — calls `compile()` with `include_lineage: true`. Extracts `plan.lineage()`. Serializes via `QueryLineage::to_openlineage_event()`.

`/schema` — calls `compiler.schema_info(&model_ref)`. Maps to a flat JSON schema summary.

`/health` — no compiler call. Returns `{"status": "ok", "version": env!("CARGO_PKG_VERSION")}`.

---

## Phase 5.6 — Public server entrypoint

**Task:** Export `start_server(config: ServerConfig, compiler: ..., adapter: ...) -> Result<(), ServerError>` as the library entrypoint. The CLI's `semstrait serve` command calls this. This keeps `semstrait-http` usable as a library (embedded in other services) or as a binary (via CLI).

---

## Testing strategy

Unit tests mock the `SemanticCompiler` trait:
```rust
struct MockCompiler { response: CompiledPlan }
impl SemanticCompiler for MockCompiler {
    fn compile(&self, ..) -> Result<CompiledPlan, CompileError> { Ok(self.response.clone()) }
    ...
}
```

This lets HTTP handler tests run without the full compilation pipeline. Test HTTP contract (status codes, response shape, error serialization) independently from compilation correctness (tested in semstrait-core).

Integration tests use a real `StatelessCompiler` + `steelwheels.yaml`. Assert full round-trip HTTP → rows for the passthrough adapter using a mock HTTP backend (wiremock).
