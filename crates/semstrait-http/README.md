# semstrait-http

HTTP API server for semstrait. Wraps a `SemanticCompiler` and an optional `ConnectorAdapter` in an axum router.

---

## Responsibility

Expose semstrait's compilation and execution capabilities over HTTP. Handle request parsing, response serialisation, error mapping, and server lifecycle. Nothing else.

`semstrait-http` has no knowledge of the semantic model internals, plan structure, or engine specifics. It holds an `Arc<dyn SemanticCompiler>` and an optional `Arc<dyn ConnectorAdapter>`. The engine is interchangeable; the compiler is interchangeable; the HTTP layer is stable.

---

## API

### `POST /query`

Compile a query. Returns Substrait bytes and optionally SQL.

**Request:**
```json
{
  "model": "sales.yaml",
  "measures": ["revenue"],
  "dimensions": ["date.year", "region"],
  "filters": [{"column": "date.year", "op": "eq", "value": "2024"}],
  "options": { "sql_dialect": "duckdb" }
}
```

**Response:**
```json
{
  "substrait": "<base64-encoded bytes>",
  "sql": "SELECT ...",
  "output_schema": [
    {"semantic_name": "revenue", "physical_name": "revenue", "type": "f64"},
    {"semantic_name": "date.year", "physical_name": "date_year", "type": "i32"}
  ]
}
```

### `POST /execute`

Compile and execute. Requires a connector adapter to be configured.

Same request as `/query` plus `"adapter": "passthrough"` (optional if server has a default adapter).

**Response:**
```json
{
  "rows": [{"revenue": 1234.56, "date_year": 2024}],
  "row_count": 1
}
```

### `POST /validate`

Validate a model and/or query without compiling.

**Request:** `{"model": "sales.yaml", "query": {...}}` (query optional)

**Response:**
```json
{
  "valid": false,
  "diagnostics": [
    {"level": "Error", "code": "RESOL_E002", "message": "Unknown measure 'revnue'", "context": "did you mean 'revenue'?"}
  ]
}
```

### `POST /lineage`

Return OpenLineage event JSON for a query.

### `GET /schema`

Return model metadata: model names, measures, dimensions, dataset count.

### `GET /health`

Liveness check. Returns `{"status": "ok"}`.

---

## Configuration

```rust
pub struct ServerConfig {
    pub bind_addr: SocketAddr,             // default: 0.0.0.0:3000
    pub model_path: PathBuf,               // base path for model YAML files
    pub default_dialect: Option<Dialect>,  // if set, used when request omits dialect
    pub request_timeout: Duration,         // default: 30s
}
```

Loaded from environment variables with prefix `SEMSTRAIT_`:
`SEMSTRAIT_BIND_ADDR`, `SEMSTRAIT_MODEL_PATH`, `SEMSTRAIT_DEFAULT_DIALECT`.

---

## Error responses

All errors return structured JSON:
```json
{
  "error": "RESOL_E002",
  "message": "Unknown measure 'revnue'",
  "diagnostics": [...]
}
```

HTTP status mapping:
- `CompileError` with all Error-level diagnostics → 400 Bad Request
- `CompileError` with Warning-level only → 200 with diagnostics in body
- Server misconfiguration (e.g. no adapter for `/execute`) → 503 Service Unavailable
- Internal panics → 500 Internal Server Error (with request ID for log correlation)
