# semstrait-core — Implementation Plan

Covers Phases 0–2 from the workspace plan. This is the foundational work everything else builds on.

---

## Phase 0.1–0.2 — Land the monolith in the workspace

**Task:** Move `src/` to `crates/semstrait-core/src/` and create the workspace manifest.

All existing tests must pass before proceeding. No code changes — structural move only. Verify with `cargo test -p semstrait-core`. This is a safe checkpoint: if anything fails here it's a packaging issue, not a logic issue.

Notable: `test_data/` moves to workspace root and becomes a shared fixture path. Update any hardcoded paths in tests to use `concat!(env!("CARGO_MANIFEST_DIR"), "/../../test_data/...")` or the `TEST_DATA_DIR` env var approach. Do not replicate fixture files across crates.

---

## Phase 1.1 — Seal PlanNode

**Task:** Make `plan/` module `pub(crate)`. Remove `PlanNode`, `Expr`, `Column`, `AggregateExpr` from `lib.rs` re-exports.

**Why:** These types are relational algebra building blocks, not semantic API. Exposing them creates a false contract: callers would depend on structural plan details that must remain free to change as the planner evolves (e.g., new node types for window functions, lateral joins, grain-aware routing).

**Risk:** If any integration test is currently pattern-matching on `PlanNode` variants, it will break. Fix by moving such tests inside the crate boundary (into `planner/` module tests) where `pub(crate)` is visible. External tests should assert on `CompiledPlan` outputs (SQL text, Substrait bytes roundtrip).

---

## Phase 1.2 — Unified diagnostics

**Task:** Create `diagnostics.rs`. Define `Diagnostic`, `DiagnosticLevel`, `CompileError`, `ValidationReport`.

Map existing error types at their exit points — not by replacing them internally:
```
parser::ParseError     →  mapped to CompileError at parser module boundary
selector::SelectError  →  mapped at selector boundary
resolver::ResolveError →  mapped at resolver boundary
planner::PlanError     →  mapped at planner boundary
emitter::EmitError     →  mapped at emitter boundary
```

Keep internal error types as-is. Add `impl From<ParseError> for CompileError`, etc. This preserves internal context while presenting a single error type to callers.

Error code convention: `{STAGE}_{LEVEL}{SEQ}` where STAGE is 5-letter uppercase, LEVEL is E/W/I, SEQ is zero-padded 3 digits. Examples: `PARSE_E001`, `RESOL_W002`, `PLAN_E003`. Assign codes in sequence as errors are defined — don't attempt a complete taxonomy upfront; add codes as needed.

---

## Phase 1.3 — substrait_conv module

**Task:** Create `planner/substrait_conv.rs`. Migrate Substrait emission logic from `emitter/substrait.rs` here. Change visibility and return type.

**Before:**
```rust
// emitter/substrait.rs — public function returning proto type
pub fn emit_plan(node: &PlanNode, output_names: Option<Vec<String>>)
    -> Result<proto::Plan, EmitError>
```

**After:**
```rust
// planner/substrait_conv.rs — pub(crate), returns opaque bytes
pub(crate) fn to_substrait_bytes(
    node: &PlanNode,
    output_names: Option<&[String]>,
) -> Result<Vec<u8>, CompileError> {
    let proto_plan = build_proto_plan(node, output_names)?;
    Ok(proto_plan.encode_to_vec())              // only serialization call in the codebase
}

fn build_proto_plan(node: &PlanNode, ...) -> Result<proto::Plan, CompileError> {
    // ... traversal of PlanNode to proto::Rel tree
    // substrait::proto::* types used freely here; they never leave this module
}
```

`substrait::proto` is imported only in this file. Add a `#![cfg_attr(not(doc), deny(unused_imports))]` or similar guard at the `emitter/` directory level to catch any accidental re-introduction of proto imports elsewhere.

Delete `emitter/substrait.rs` after migration. Keep `emitter/sql.rs` (rename to `planner/sql_emitter.rs` at the same time for module coherence — both emitters now live in `planner/`).

---

## Phase 1.4 — CompiledPlan

**Task:** Define `CompiledPlan` and `CompileOpts` in `output.rs`.

```rust
pub struct CompileOpts {
    pub sql_dialect: Option<Dialect>,      // None → no SQL produced
    pub include_lineage: bool,             // default false
}

pub enum Dialect {
    Ansi,
    // Dialect variants without implementations are stubs for phase 3.
    // They are defined here so CompileOpts is stable when semstrait-sql lands.
    DuckDb, Spark, Snowflake, BigQuery, Trino, Redshift, Postgres,
}

pub struct CompiledPlan {
    substrait: Vec<u8>,
    sql: Option<String>,
    output_schema: Vec<OutputColumn>,
    lineage: Option<QueryLineage>,
    diagnostics: Vec<Diagnostic>,
}
```

In phase 1, `Dialect` variants other than `Ansi` cause `compile()` to return a `Diagnostic` warning that dialect translation requires `semstrait-sql` and falls back to ANSI SQL. This is not an error — the bytes are still valid Substrait, and ANSI SQL is usable. When `semstrait-sql` is wired in (phase 3), those warnings go away.

---

## Phase 1.5 — ModelRef and SchemaRegistry

**Task:** Create `registry.rs`.

`FileSystemRegistry` accepts a `base_path`. When given a `ModelRef::FilePath`, it resolves relative paths against `base_path`. When given `ModelRef::Key`, it looks for `{base_path}/{namespace}/{name}.yaml`. This is the simplest registry behaviour that supports both single-file and multi-model directory layouts.

Thread-safety note: `FileSystemRegistry` contains only a `PathBuf`. It is `Send + Sync` trivially. `StatelessCompiler` re-parses on every call — no `Arc<Schema>` caching in v1. Caching is a `SchemaRegistry` implementation detail; a `CachedRegistry` wrapper can be added later without changing any trait.

---

## Phase 1.6 — SemanticCompiler trait and StatelessCompiler

**Task:** Create `compiler.rs`.

The compiler is the integration point that calls all pipeline stages in sequence:

```rust
impl SemanticCompiler for StatelessCompiler {
    fn compile(&self, model_ref, request, opts) -> Result<CompiledPlan, CompileError> {
        let schema = self.registry.load(model_ref)?;
        let model  = schema.get_model(&request.model)
                           .ok_or_else(|| CompileError::model_not_found(&request.model))?;

        let datasets = selector::select_datasets(&schema, model, &request.dimensions, &request.measures)
                                .map_err(CompileError::from)?;
        let resolved = resolver::resolve_query(&schema, request, &datasets[0])
                                .map_err(CompileError::from)?;

        let lineage = opts.include_lineage
            .then(|| lineage::derive(&resolved));

        let plan_node = planner::plan_query(&resolved)
                                .map_err(CompileError::from)?;

        let substrait = substrait_conv::to_substrait_bytes(&plan_node, None)?;

        let sql = match opts.sql_dialect {
            Some(Dialect::Ansi) | Some(_) =>   // dialect translation deferred to semstrait-sql
                Some(sql_emitter::emit_sql(&plan_node, None)?),
            None => None,
        };

        Ok(CompiledPlan {
            substrait,
            sql,
            output_schema: build_output_schema(&resolved),
            lineage,
            diagnostics: vec![],
        })
    }
}
```

`validate()` runs the same pipeline but collects all `Diagnostic` output without returning `Err`. It is a dry-run compilation with all errors converted to diagnostics.

`schema_info()` calls `registry.load()` and returns model names, measure names, dimension names, and dataset count — no planning involved. Used by the HTTP `/schema` endpoint and `semstrait schema` CLI command.

---

## Phase 1.7 — Clean up lib.rs exports

**Task:** Strip all internal type re-exports from `lib.rs`. Final public surface:

```rust
// lib.rs
pub use schema::{Schema, SemanticModel, DataType, Aggregation};
pub use query::{QueryRequest, DataFilter, OrderBy};
pub use output::{CompiledPlan, OutputColumn, CompileOpts, Dialect};
pub use compiler::{SemanticCompiler, StatelessCompiler, ValidationReport, SchemaInfo};
pub use registry::{SchemaRegistry, FileSystemRegistry, ModelRef};
pub use diagnostics::{CompileError, Diagnostic, DiagnosticLevel};
pub use lineage::{QueryLineage, ColumnLineage};  // phase 2
```

Write an integration test in `tests/compile_roundtrip.rs` that:
1. Loads `steelwheels.yaml` via `FileSystemRegistry`
2. Compiles a known query via `StatelessCompiler`
3. Asserts `plan.substrait()` is non-empty and decodes to a valid `substrait::proto::Plan`
4. Asserts `plan.sql()` is `Some` when `CompileOpts::with_sql(Dialect::Ansi)` is used
5. Asserts output column names match expected semantic names

This test is the integration smoke test for the entire phase 1 work.

---

## Phase 2.1 — Lineage derivation

**Task:** Create `lineage.rs`. Derive `QueryLineage` from `ResolvedQuery`.

`ResolvedQuery` already contains join paths, dimension-to-column mappings, and measure expressions. Lineage derivation is structural traversal: for each output column, walk the expression tree to find the source column references.

```rust
pub(crate) fn derive(resolved: &ResolvedQuery) -> QueryLineage {
    QueryLineage {
        inputs: resolved.datasets.iter()
            .map(|ds| DatasetRef { name: ds.source.clone() })
            .collect(),
        output_columns: resolved.measures.iter()
            .map(|m| ColumnLineage {
                output_column: m.name.clone(),
                source_columns: extract_source_refs(&m.expr),
                transformation_type: classify_transformation(&m.expr),
            })
            .chain(resolved.dimensions.iter().map(dim_to_lineage))
            .collect(),
    }
}
```

No execution needed. No Substrait round-trip. This is static analysis of the resolved query graph.

---

## Phase 2.2 — OpenLineage serialisation

**Task:** Add `QueryLineage::to_openlineage_event(...)` in `lineage.rs`.

OpenLineage `RunEvent` schema is simple JSON. Produce it via `serde_json::json!` macro — no need for a full OpenLineage Rust crate at this stage. Pin the OpenLineage spec version in a constant (`OPENLINEAGE_SPEC_VERSION = "1.0.0"`).

Test against a known steelwheels query: assert the produced JSON contains the expected `inputs`, `outputs`, and `columnLineage` fields with correct column names.

---

## Testing strategy

| Test file | What it covers |
|---|---|
| `tests/compile_roundtrip.rs` | Full pipeline, Substrait roundtrip, SQL output |
| `tests/validation.rs` | `validate()` on malformed models — asserts diagnostic codes |
| `tests/lineage.rs` | Lineage derivation, OpenLineage JSON structure |
| `planner/selector.rs` (unit) | Dataset selection with aggregate awareness |
| `planner/resolver.rs` (unit) | Join path resolution, filter predicate building |
| `planner/substrait_conv.rs` (unit) | Proto plan structure, field index correctness |
| `planner/sql_emitter.rs` (unit) | SQL string correctness per node type |

Existing tests in `src/` carry over unchanged. New tests are added alongside new code.
