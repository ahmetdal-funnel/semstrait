# semstrait — Implementation Plan

This document describes the phased migration from the current monolith (`src/`) to the target workspace layout. Each phase is a coherent, shippable unit. No phase breaks public API compatibility with the previous one.

---

## Current state

Single crate. Working pipeline: YAML → Schema → PlanNode → `substrait::proto::Plan` + ANSI SQL string. Both Substrait and SQL are emitted as parallel paths from `PlanNode`. Proto types are part of the public return type of `emit_plan`. `PlanNode` is publicly exported.

---

## Target state

Multi-crate workspace. `PlanNode` is private. `substrait::proto::Plan` is an internal conversion type. `CompiledPlan` is the public output with opaque `Vec<u8>` for Substrait bytes and `Option<String>` for SQL. All layers above core depend only on the `SemanticCompiler` trait.

---

## Phase 0 — Workspace scaffolding
*Goal: workspace compiles, all existing tests pass, no behaviour changes.*

**0.1 — Create workspace `Cargo.toml`**
At the repo root, create a `[workspace]` manifest that lists `crates/semstrait-core` as the sole member. Shared dependency versions go in `[workspace.dependencies]`. This is the only structural change in this step.

**0.2 — Move monolith into `crates/semstrait-core`**
Move `src/` → `crates/semstrait-core/src/`. Move `Cargo.toml` to `crates/semstrait-core/Cargo.toml`. Adjust the package name to `semstrait-core`. Move `test_data/` to workspace root (shared fixtures). All existing tests must pass unchanged. This step introduces zero logic changes.

**0.3 — Stub remaining crates**
Create `crates/semstrait-sql/`, `crates/semstrait-connectors/`, `crates/semstrait-http/`, `crates/semstrait-cli/`, `crates/semstrait/` each with a minimal `Cargo.toml` and a `src/lib.rs` (or `src/main.rs` for binaries) that compiles to nothing. Add all to workspace members. CI must pass.

---

## Phase 1 — Core IR and API correction
*Goal: establish correct IR model. `PlanNode` becomes private. `CompiledPlan` becomes the stable public type. Substrait bytes are always produced.*

**1.1 — Seal `PlanNode`**
Change `pub mod plan` to `pub(crate) mod plan` in `lib.rs`. Remove `PlanNode`, `Expr`, `Column`, `AggregateExpr` from the public re-exports. These types were never intended as public API — they leaked through the current flat structure. Any downstream code depending on them (none expected externally at this stage) will break; that is intentional.

**1.2 — Add `diagnostics` module**
Introduce `Diagnostic { level: DiagnosticLevel, code: String, message: String, context: Option<String> }`, `DiagnosticLevel { Error, Warning, Info }`, and `CompileError(Vec<Diagnostic>)`. This replaces the scattered `ParseError`, `SelectError`, `ResolveError`, `PlanError`, `EmitError` types as the unified error surface. The existing error types remain internally but are mapped to `CompileError` at the boundary.

**1.3 — Add `substrait_conv` module inside `planner/`**
Create `planner/substrait_conv.rs`. Move the logic from `emitter/substrait.rs` here. The function signature changes from `emit_plan(node: &PlanNode) -> Result<proto::Plan, EmitError>` to `pub(crate) fn to_substrait_bytes(node: &PlanNode) -> Result<Vec<u8>, CompileError>`. The proto types are used inside this function and never leave it. `encode_to_vec()` is called here — the only place in the entire codebase where Substrait serialization occurs.

**1.4 — Add `CompiledPlan` type**
```rust
pub struct CompiledPlan {
    substrait: Vec<u8>,         // always present; canonical IR bytes
    sql: Option<String>,         // ANSI SQL; present when CompileOpts requests it
    output_schema: Vec<OutputColumn>,
    lineage: Option<QueryLineage>,
    diagnostics: Vec<Diagnostic>,
}

pub struct OutputColumn {
    pub semantic_name: String,   // e.g. "revenue", "date.year"
    pub physical_name: String,   // column alias in the output
    pub data_type: DataType,
}
```
`sql_emitter` continues to emit from `PlanNode` (not from proto). `CompileOpts::with_sql(dialect)` enables SQL output. The planner always calls `to_substrait_bytes`. It calls `emit_sql` only when SQL is requested.

**1.5 — Add `ModelRef` and `SchemaRegistry` trait**
```rust
pub enum ModelRef {
    FilePath(PathBuf),
    Key { namespace: String, name: String },
}

pub trait SchemaRegistry: Send + Sync {
    fn load(&self, model_ref: &ModelRef) -> Result<Arc<Schema>, CompileError>;
}

pub struct FileSystemRegistry { base_path: PathBuf }
```
`FileSystemRegistry::load` wraps the existing `parser::parse_file`. `ModelRef::FilePath` is the v1 path. `ModelRef::Key` is reserved for future manifest/catalog integration.

**1.6 — Add `SemanticCompiler` trait and `StatelessCompiler`**
```rust
pub trait SemanticCompiler: Send + Sync {
    fn compile(&self, model_ref: &ModelRef, request: &QueryRequest, opts: &CompileOpts)
        -> Result<CompiledPlan, CompileError>;
    fn validate(&self, model_ref: &ModelRef, request: Option<&QueryRequest>)
        -> ValidationReport;
    fn schema_info(&self, model_ref: &ModelRef)
        -> Result<SchemaInfo, CompileError>;
}

pub struct StatelessCompiler {
    registry: Box<dyn SchemaRegistry>,
}
```
`StatelessCompiler::compile` calls: load schema → select datasets → resolve → plan → substrait_conv → (optionally) sql_emitter → assemble `CompiledPlan`. Re-parses schema on every call; no caching. Appropriate for v1 and CLI usage.

**1.7 — Update `lib.rs` public exports**
Remove all internal type re-exports. Public surface becomes:
`Schema, SemanticModel, QueryRequest, DataFilter, CompiledPlan, OutputColumn, CompileOpts, ModelRef, SemanticCompiler, StatelessCompiler, FileSystemRegistry, SchemaRegistry, ValidationReport, SchemaInfo, CompileError, Diagnostic, DiagnosticLevel`

---

## Phase 2 — Lineage
*Goal: `CompiledPlan` carries OpenLineage-compatible lineage derived from `ResolvedQuery`. No execution required.*

**2.1 — Add `lineage` module**
Introduce:
```rust
pub struct QueryLineage {
    pub inputs: Vec<DatasetRef>,
    pub output_columns: Vec<ColumnLineage>,
}
pub struct ColumnLineage {
    pub output_column: String,
    pub source_columns: Vec<SourceColumnRef>,
    pub transformation_type: TransformationType, // Direct | Aggregate | Expression
}
```
Derive lineage from `ResolvedQuery` inside the compiler, before plan construction. This is purely structural traversal — no SQL execution, no Substrait round-trip.

**2.2 — OpenLineage serialization**
Add `QueryLineage::to_openlineage_event(run_id, job_name, namespace) -> serde_json::Value`. This produces a valid OpenLineage `RunEvent` JSON. Enable via `CompileOpts::with_lineage()`.

---

## Phase 3 — SQL dialect layer (`semstrait-sql`)
*Goal: dialect-specific SQL from compiled plans. ANSI SQL from phase 1 is the input.*

**3.1 — `semstrait-sql` crate skeleton**
Add `semstrait-core` as a dependency. Define the trait:
```rust
pub trait SqlDialectEmitter: Send + Sync {
    fn dialect(&self) -> Dialect;
    fn emit(&self, ansi_sql: &str) -> Result<String, SqlError>;
}
```

**3.2 — Integrate `polyglot-sql`**
Add `polyglot-sql` crate dependency. Implement `PolyglotEmitter` that wraps it. Map `Dialect` enum variants to polyglot-sql's dialect identifiers. Add integration tests against known ANSI → DuckDB, ANSI → Spark, ANSI → Snowflake transformations using the existing steelwheels model queries.

**3.3 — Wire dialect into `CompileOpts`**
`CompileOpts::with_sql(Dialect)` passes the dialect through to `semstrait-sql`. The core crate emits ANSI SQL when SQL is requested; `semstrait-sql` post-processes it to the target dialect. Core has no dependency on `semstrait-sql` — dialect is applied by the caller after compilation, or by `StatelessCompiler` when `semstrait-sql` is a feature dep.

---

## Phase 4 — Connectors (`semstrait-connectors`)
*Goal: execute compiled plans against real engines.*

**4.1 — `ConnectorAdapter` trait**
```rust
#[async_trait]
pub trait ConnectorAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn accepted_inputs(&self) -> &[InputKind];
    async fn execute(&self, query: &ExecutableQuery, ctx: &ExecContext)
        -> Result<ConnectorResult, ConnectorError>;
    async fn health_check(&self) -> Result<(), ConnectorError>;
}
pub enum InputKind { Sql(Dialect), SubstraitBytes }
pub enum ConnectorResult { Json(JsonResult), Arrow(ArrowResult) }
```

**4.2 — `PassthroughAdapter`**
First concrete adapter. POSTs SQL to any HTTP SQL endpoint (DuckDB HTTP server, ClickHouse HTTP interface, etc.). Accepts `InputKind::Sql(dialect)`. Returns `ConnectorResult::Json`. This covers most local dev and cloud warehouse scenarios without engine-specific SDKs.

**4.3 — Arrow/FlightSQL foundation**
Add `arrow` and `arrow-flight` as optional deps (`feature = "flight"`). Define `ArrowResult` wrapping `Vec<RecordBatch>`. Add `FlightSqlAdapter` stub that connects to a FlightSQL endpoint and executes Substrait bytes. Full implementation deferred to a later iteration.

---

## Phase 5 — HTTP server (`semstrait-http`)
*Goal: serve compiled plans and execute queries over HTTP. Depends only on `SemanticCompiler` trait.*

**5.1 — axum router**
Endpoints:
- `POST /query` — compile only, return `{substrait: base64, sql: string?}`
- `POST /execute` — compile + execute via configured adapter, return row data
- `POST /validate` — return validation report
- `POST /lineage` — return OpenLineage event JSON
- `GET /schema` — return model schema info
- `GET /health` — liveness check

Request/response types are `serde` structs, not internal types. No `semstrait-core` internal types cross the HTTP boundary.

**5.2 — Server configuration**
`ServerConfig { bind_addr, model_path, default_dialect, adapter_config }`. Loaded from environment variables with reasonable defaults. The server holds a single `Arc<dyn SemanticCompiler>` — swappable without server restart in future.

**5.3 — Error mapping**
`CompileError` → structured JSON error response with diagnostic codes. Diagnostics array included in response body. HTTP status follows error severity: validation errors → 400, internal errors → 500.

---

## Phase 6 — CLI (`semstrait-cli`)
*Goal: developer-facing binary for local query compilation, execution, and introspection.*

**6.1 — clap command tree**
```
semstrait query    -m <model> -q <query.json> [--format sql|substrait|both] [--dialect <d>]
semstrait execute  -m <model> -q <query.json> [--endpoint <url>] [--dialect <d>]
semstrait explain  -m <model> -q <query.json>
semstrait validate -m <model> [-q <query.json>]
semstrait lineage  -m <model> -q <query.json> [--format openlineage|json]
semstrait schema   -m <model>
semstrait serve    [-m <model>] [-p <port>] [--flight-port <port>]
```

**6.2 — Output formatting**
`--format` controls output. `substrait` outputs base64 bytes to stdout (pipeable). `sql` outputs the SQL string. `both` outputs JSON `{substrait: ..., sql: ...}`. `explain` outputs a human-readable plan tree (using `substrait-explain` crate for Substrait visualization + custom PlanNode debug format for internal inspection).

**6.3 — `semstrait serve`**
Delegates to `semstrait-http`. The CLI crate depends on both `semstrait-http` and `semstrait-connectors`.

---

## Phase 7 — Facade crate (`semstrait`)
*Goal: single entry point for library users. Feature gates control which subsystems are compiled.*

**7.1 — Re-exports with feature gates**
```toml
[features]
default = ["core"]
core    = ["dep:semstrait-core"]
sql     = ["core", "dep:semstrait-sql"]
connectors = ["sql", "dep:semstrait-connectors"]
full    = ["connectors"]
```

**7.2 — Backwards compatibility module**
For any downstream code that was using the old flat `semstrait::emit_plan`, `semstrait::PlanNode` etc., provide a `semstrait::compat` module with deprecation warnings that delegates to the new API. Remove in the next minor version after adequate notice.

---

## Phase 8 — Future decomposition (post-stabilisation)
*Goal: extract schema and parser into independent crates when interfaces are stable.*

**8.1 — Extract `semstrait-schema`**
Move `semstrait-core/src/schema/` → `crates/semstrait-schema/src/`. It has only `serde` as a meaningful dep. `semstrait-core` adds it as a dependency. External users (LLM integrations, model builders, catalog tools) can depend on it without pulling in parser or planner.

**8.2 — Extract `semstrait-parser`**
Move `semstrait-core/src/parser/` → `crates/semstrait-parser/src/`. Depends on `semstrait-schema`, `serde_yaml`, `serde_json`. Enables alternative parsers (LookML, Cube.js JSON) as sibling crates without touching schema or planner.

---

## Progress tracker

| Phase | Status | Notes |
|---|---|---|
| 0 — Workspace scaffolding | ⬜ Not started | |
| 1 — Core IR and API correction | ⬜ Not started | |
| 2 — Lineage | ⬜ Not started | |
| 3 — SQL dialect layer | ⬜ Not started | |
| 4 — Connectors | ⬜ Not started | |
| 5 — HTTP server | ⬜ Not started | |
| 6 — CLI | ⬜ Not started | |
| 7 — Facade crate | ⬜ Not started | |
| 8 — Decomposition | ⬜ Deferred | |
