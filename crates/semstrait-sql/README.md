# semstrait-sql

SQL dialect translation for semstrait compiled plans.

`semstrait-core` produces ANSI SQL from its internal plan. `semstrait-sql` post-processes that ANSI SQL into engine-specific dialects: DuckDB, Spark, Snowflake, BigQuery, Trino, Redshift, Postgres, and others.

---

## Responsibility

A single concern: given an ANSI SQL string, produce a semantically equivalent string in the target dialect.

This crate does not touch the semantic model, the query request, or the Substrait plan. It operates entirely on SQL text. This is intentional — dialect handling is a string transformation problem, not a semantic modeling problem.

---

## Why separate from core

`semstrait-core` must remain free of SQL dialect dependencies. A semantic compiler that requires DuckDB-specific libraries to compile would be a significant footgun. `semstrait-sql` is an optional layer: services that deliver Substrait bytes to engines never need it. Only SQL-consuming integrations pull this crate.

---

## Architecture

```
CompiledPlan.sql()   →  ANSI SQL string  (from semstrait-core)
                              ↓
                     SqlDialectEmitter::emit(ansi_sql, dialect)
                              ↓
                     engine-specific SQL string
```

The `SqlDialectEmitter` trait is the extension point. The default implementation wraps `polyglot-sql` (32+ dialects, pure Rust, 100% sqlglot fixture compliance). Custom emitters can be registered for dialects not covered by polyglot-sql, or for engines with unusual extension syntax.

---

## Key types

```rust
/// The target SQL dialect.
#[non_exhaustive]
pub enum Dialect {
    Ansi,
    DuckDb,
    Spark,
    Snowflake,
    BigQuery,
    Trino,
    Redshift,
    Postgres,
    // Additional dialects added as polyglot-sql support is validated
}

/// Translate ANSI SQL to a target dialect.
pub trait SqlDialectEmitter: Send + Sync {
    fn dialect(&self) -> Dialect;
    fn emit(&self, ansi_sql: &str) -> Result<String, SqlError>;
}

/// Default implementation using polyglot-sql.
pub struct PolyglotEmitter { dialect: Dialect }

impl SqlDialectEmitter for PolyglotEmitter {
    fn dialect(&self) -> Dialect { self.dialect.clone() }
    fn emit(&self, ansi_sql: &str) -> Result<String, SqlError> {
        polyglot_sql::transpile(ansi_sql, to_polyglot_dialect(&self.dialect))
            .map_err(SqlError::from)
    }
}

pub struct SqlError { pub message: String, pub source_sql: String }
```

---

## Scope limitation

`semstrait-sql` does not:
- Parse SQL (parsing is done inside polyglot-sql)
- Validate semantic correctness (that's `semstrait-core`'s job)
- Know anything about the semantic model or schema

If a dialect transformation is impossible for a given ANSI construct, `SqlError` is returned with the offending fragment in `source_sql`. Callers can fall back to Substrait delivery or ANSI SQL in that case.
