# semstrait

The semstrait facade crate. Single entry point for library users.

This crate re-exports the public API of all semstrait subsystems under a unified namespace, controlled by feature flags. Most users depend on this crate rather than on the individual crates directly.

---

## Features

```toml
[dependencies]
semstrait = "0.2"                         # core only (compilation, no SQL dialect, no execution)
semstrait = { version = "0.2", features = ["sql"] }         # + dialect SQL translation
semstrait = { version = "0.2", features = ["connectors"] }  # + engine execution
semstrait = { version = "0.2", features = ["full"] }        # everything
```

| Feature | Adds |
|---|---|
| `core` (default) | `SemanticCompiler`, `StatelessCompiler`, `CompiledPlan`, `QueryRequest`, all core types |
| `sql` | `SqlDialectEmitter`, `PolyglotEmitter`, `CompileOpts::with_sql(dialect)` fully wired |
| `connectors` | `ConnectorAdapter`, `PassthroughAdapter`, `FlightSqlAdapter` (stub) |
| `full` | All of the above |

---

## Re-exports

```rust
// Core — always available
pub use semstrait_core::{
    SemanticCompiler, StatelessCompiler,
    SchemaRegistry, FileSystemRegistry,
    QueryRequest, DataFilter, OrderBy,
    CompiledPlan, OutputColumn, CompileOpts,
    ModelRef, Dialect,
    CompileError, Diagnostic, DiagnosticLevel,
    ValidationReport, SchemaInfo,
    QueryLineage, ColumnLineage,
    Schema, SemanticModel, DataType, Aggregation,
};

// SQL — with feature "sql"
#[cfg(feature = "sql")]
pub use semstrait_sql::{SqlDialectEmitter, PolyglotEmitter, SqlError};

// Connectors — with feature "connectors"
#[cfg(feature = "connectors")]
pub use semstrait_connectors::{
    ConnectorAdapter, PassthroughAdapter,
    ConnectorResult, ConnectorError, InputKind, ExecutableQuery, ExecContext,
};

#[cfg(all(feature = "connectors", feature = "flight"))]
pub use semstrait_connectors::FlightSqlAdapter;
```

---

## Convenience builders

The facade adds one convenience that individual crates don't provide — the fully-wired `CompileOpts::with_sql(dialect)` that injects the `PolyglotEmitter` hook when `semstrait-sql` is available:

```rust
#[cfg(feature = "sql")]
impl CompileOpts {
    /// Request SQL output in the given dialect.
    /// Automatically wires the polyglot-sql emitter as the post-processor.
    pub fn with_sql(self, dialect: Dialect) -> Self {
        let emitter = PolyglotEmitter::new(dialect.clone());
        self.with_sql_dialect(dialect)
            .with_dialect_emitter(emitter)
    }
}
```

Without the `sql` feature, `CompileOpts::with_sql(Dialect::Ansi)` still works — it produces ANSI SQL without post-processing. Requesting a non-ANSI dialect without the `sql` feature produces a `Diagnostic::Warning` and falls back to ANSI.

---

## Backwards compatibility

The `semstrait::compat` module provides deprecated re-exports for the old flat API (`emit_plan`, `emit_sql`, `PlanNode`, etc.) with `#[deprecated]` annotations. This allows downstream code to migrate at its own pace. The compat module will be removed in the next minor version after adequate notice in the changelog.
