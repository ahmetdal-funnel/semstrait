# semstrait (facade) — Implementation Plan

Phase 7 of workspace plan. This crate is assembled last — it has nothing to implement beyond re-exports and the convenience wiring. Its complexity is in getting the feature flag logic right.

---

## Phase 7.1 — Feature flag structure

**Task:** Define `Cargo.toml` feature flags. Wire conditional dependencies.

```toml
[package]
name = "semstrait"

[features]
default = ["core"]
core       = ["dep:semstrait-core"]
sql        = ["core", "dep:semstrait-sql"]
connectors = ["sql", "dep:semstrait-connectors"]
flight     = ["connectors", "semstrait-connectors/flight"]
full       = ["connectors"]

[dependencies]
semstrait-core        = { path = "../semstrait-core", optional = false }
semstrait-sql         = { path = "../semstrait-sql",        optional = true }
semstrait-connectors  = { path = "../semstrait-connectors", optional = true }
```

Note: `semstrait-core` is not optional — `core` feature is always active. The feature exists to document it, not to gate it.

Verify that `cargo build -p semstrait` (default features), `cargo build -p semstrait --features sql`, and `cargo build -p semstrait --features full` each compile without errors.

---

## Phase 7.2 — Re-exports and lib.rs

**Task:** Write `lib.rs`. Only re-exports and the `with_sql` convenience method. No logic.

Add a `compile.rs` module that holds the extended `CompileOpts::with_sql` impl under `#[cfg(feature = "sql")]`. Keep `lib.rs` clean.

Ensure that the crate-level docs (`//! semstrait ...`) are accurate and include a complete usage example that compiles in doctests. The doctest is the integration smoke test for the facade:

```rust
//! ```rust
//! use semstrait::{StatelessCompiler, FileSystemRegistry, SemanticCompiler,
//!                  ModelRef, QueryRequest, CompileOpts, Dialect};
//!
//! let compiler = StatelessCompiler::new(FileSystemRegistry::new("./test_data"));
//! let plan = compiler.compile(
//!     &ModelRef::file("steelwheels.yaml"),
//!     &QueryRequest::builder().model("steelwheels").measure("revenue").build(),
//!     &CompileOpts::default(),
//! ).unwrap();
//! assert!(!plan.substrait().is_empty());
//! ```
```

---

## Phase 7.3 — Compat module

**Task:** Add `semstrait::compat` with deprecated re-exports.

```rust
#[deprecated(since = "0.2.0", note = "Use semstrait::StatelessCompiler and compile() instead")]
pub use semstrait_core::emitter::emit_plan;

#[deprecated(since = "0.2.0", note = "PlanNode is now private. Use CompiledPlan instead")]
// Can't re-export PlanNode because it's pub(crate) — provide a stub type with a helpful message
```

For types that are now truly private (`PlanNode`), a deprecated type alias pointing to a `#[doc(hidden)]` placeholder struct can guide users to the migration guide via the deprecation message.

---

## Phase 7.4 — Changelog and semver

**Task:** Document the breaking changes from v0.1 (monolith) to v0.2 (workspace) in `CHANGELOG.md`.

Breaking changes:
- `PlanNode`, `Expr`, `Column`, `AggregateExpr` removed from public API
- `emit_plan()` now returns `CompileError` not `EmitError`; output is `Vec<u8>` not `proto::Plan`
- `emit_sql()` return type unchanged but now only accessible via `CompiledPlan::sql()`
- All internal error types removed from public surface; replaced by `CompileError`

Migration path for each breaking change should be one sentence with a code before/after example. Keep it mechanical — users migrating should be able to do so without reading design docs.
