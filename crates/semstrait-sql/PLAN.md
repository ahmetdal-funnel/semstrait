# semstrait-sql — Implementation Plan

Covered by Phase 3 of the workspace plan. Depends on Phase 1 (semstrait-core) being complete.

---

## Phase 3.1 — Crate skeleton and trait definition

**Task:** Promote the stub crate to a real library. Define `Dialect`, `SqlDialectEmitter` trait, and `SqlError`.

The `Dialect` enum is defined in both `semstrait-core` (as a marker in `CompileOpts`) and here as the full type. Resolve this with a re-export: `semstrait-core` defines `Dialect`; `semstrait-sql` re-exports it and adds the emitter implementations. This avoids a dependency inversion where core would need to know about sql.

Alternatively: `Dialect` lives in `semstrait-core` as a plain enum with no behaviour. `semstrait-sql` imports it and builds `SqlDialectEmitter` implementations against it. This is the correct direction — core owns the type, sql provides behaviour.

**Deliverable:** Trait compiles. `SqlError` type is defined. No implementation yet.

---

## Phase 3.2 — polyglot-sql integration

**Task:** Add `polyglot-sql` dep. Implement `PolyglotEmitter`.

Map each `Dialect` variant to the corresponding `polyglot_sql::Dialect` identifier. Write the translation call:

```rust
impl SqlDialectEmitter for PolyglotEmitter {
    fn emit(&self, ansi_sql: &str) -> Result<String, SqlError> {
        polyglot_sql::transpile(ansi_sql, self.pg_dialect)
            .map_err(|e| SqlError {
                message: e.to_string(),
                source_sql: ansi_sql.to_owned(),
            })
    }
}
```

**Validation:** For each supported dialect, run the full semstrait steelwheels pipeline and compare the dialect-translated SQL against manually verified expected output. Start with DuckDB (most commonly used in local dev) and Spark (most commonly used in CI/batch). Add Snowflake, BigQuery, Trino once DuckDB and Spark are green.

The test pattern:
```rust
#[test]
fn duckdb_revenue_by_year() {
    let ansi_sql = compile_to_ansi("steelwheels.yaml", revenue_by_year_request());
    let duckdb_sql = PolyglotEmitter::new(Dialect::DuckDb).emit(&ansi_sql).unwrap();
    assert_snapshot!("duckdb_revenue_by_year", duckdb_sql);  // insta snapshot
}
```

Snapshot testing is appropriate here: dialect SQL is verbose and exact correctness matters. Reviewing snapshot diffs on dialect changes is the right workflow.

---

## Phase 3.3 — Wire into StatelessCompiler

**Task:** When `CompileOpts::sql_dialect` is not `Ansi`, pass the core ANSI SQL output through `semstrait-sql` before placing it in `CompiledPlan`.

The coupling question: `semstrait-core` must not depend on `semstrait-sql`. The solution is a callback/hook on `CompileOpts`:

```rust
// In semstrait-core/src/output.rs
pub struct CompileOpts {
    pub sql_dialect: Option<Dialect>,
    // Injected by the caller when semstrait-sql is available:
    pub(crate) sql_post_processor: Option<Box<dyn Fn(&str) -> Result<String, CompileError>>>,
}

impl CompileOpts {
    // Used by semstrait-sql/semstrait facade to inject the dialect emitter
    pub fn with_dialect_emitter(mut self, emitter: impl SqlDialectEmitter + 'static) -> Self {
        self.sql_post_processor = Some(Box::new(move |sql| {
            emitter.emit(sql).map_err(CompileError::from)
        }));
        self
    }
}
```

This keeps the dependency direction clean: core defines the hook slot, sql fills it, no import from sql into core.

In the `semstrait` facade crate, a convenience builder method `CompileOpts::with_sql(dialect: Dialect) -> CompileOpts` wires both the dialect marker and the `PolyglotEmitter` injector in one call. This is the API end-users see — they don't interact with the hook directly.

**Deliverable:** `semstrait compile -m m.yaml -q q.json --dialect duckdb` produces DuckDB SQL.
