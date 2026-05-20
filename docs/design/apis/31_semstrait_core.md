---
prereqs: [13, 14, 30]
authoritative-for:
  - the `semstrait-core` public-API surface (types, traits, free functions) after the second-cascade slimming — non-expression shared vocabulary only
  - module layout within `semstrait-core` (top-level `pub mod`s and their split rationale)
  - the canonical **logical type vocabulary** (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`) and the feature-gated serde derivations
  - the **constraint-DSL toolkit** types (`MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints`)
  - the cross-cutting **diagnostic primitives** (`Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose` trait)
  - feature-flag surface (`serde`, `schemars`, `io`, `io-aws`) and dependency posture — under default features core pulls `tokio`; under `--no-default-features` it retains the original zero-runtime-dep shape
  - mapping of design invariants I1, I2, I5, I7, I11, I12 to concrete crate-level guarantees
refined-by:
  - 13 (`DataType` / `Grain` / `TypeClass` rosters — `31` carries the implementation)
  - 11 (constraint-DSL semantics — `31` carries the type shapes only)
  - 30 (diagnostic-envelope contract — `31` carries the implementation)
  - 31b (`semstrait-core::io` — text-blob transport module, amends §1.3 / §2 / §6 of this doc)
  - 32 (`semstrait-model` declares its own `ParseError` and `ValidateError`, embeds `Ir(ir::ValidateError)` via D.ii; adds `io` wrappers over `31b`)
  - 33 (`semstrait-manifest` declares its own `CompileError`, embeds `Ir(ir::CompileError)` via D.ii; adds `RepositoryErrorKind`; adds `io` wrappers over `31b`)
  - 34 (`semstrait-planner` consumes the sealed `FunctionRegistry` from `semstrait-ir`; declares `PlanErrorKind` / `OptimizeErrorKind`)
  - 35 (`semstrait-ir` owns the canonical-IR expression types, the trait scaffolding `Tree` / `Visitor` / `Rewriter` / `ExprLeaf`, the structural-variant support enums, `Literal`, `ColumnRef`, `SemanticsName`, `ValidateError`, `CompileError`, and the `CanonicalFn` / `FunctionRegistry` family — all moved from `semstrait-core` per `14 §9.2` and `STATUS.md` item Q)
  - 36 (`semstrait-adapter` contributes `RegistryExtension` impls; declares `AdaptErrorKind`)
  - 38 (`semstrait-api` declares the sum-typed `SemStraitErrorKind` lifting per-stage kinds)
  - 40 (`implementation/40_refactor_plan.md` — current code vs target layout delta is tracked here)
---

# 31. semstrait-core

> **Status:** second cascade landing (2026-05-19, `STATUS.md` item Q). Full expression-vocabulary ejection per the Option A direction. After the first cascade (item N) moved `Expr<L>`, leaf sets, accessor enums, `Parameter`, `expr_fn` DSL, and `FunctionRegistry` to `semstrait-ir`, this second pass also moves out the **trait scaffolding** (`Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf`), the **structural-variant support enums** (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`, `Literal`), the **identifier carriers** (`ColumnRef`, `SemanticsName`), and the **narrow ir-emitted error kinds** (`ValidateError`, `CompileError`) — all now in `semstrait-ir` per `[35 §3.2](35_semstrait_ir.md)` / `[§3.4](35_semstrait_ir.md)` / `[§15](35_semstrait_ir.md)`. `31`'s post-second-cascade surface is the **non-expression shared vocabulary** only: the logical-type vocabulary (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`), the constraint DSL, the cross-cutting diagnostic primitives, and the `io` transport per `31b`.

## 1. Purpose and Scope

`semstrait-core` is the **leaf** of the semstrait workspace DAG (I7) — zero workspace dependencies; every other crate depends on it directly or transitively. After the second cascade (item Q), its sole job is to host the **non-expression shared vocabulary**: the logical type system, the constraint DSL block, the diagnostic envelope, and the byte-blob `io` transport. Everything tied to the expression tree — traits, support enums, literal carriers, identifier carriers, narrow error kinds emitted by those carriers — has moved to `semstrait-ir`, whose only upstream dep is this crate.

### 1.1 What `semstrait-core` OWNS

- The canonical **logical type system** (`§3`): `DataType`, `Grain`, `TypeClass` per `[13](../foundations/13_types_and_grain.md)`, plus `Schema` and `SchemaColumn` per `[15 §3.2](../foundations/15_mapping_and_binding.md)`. Used by every layer above (`semstrait-model` parses into them, `semstrait-manifest` stores `Schema` on `PhysicalSource`, `semstrait-ir` uses `DataType` inside `Expr<L>::Cast` and `PhysicalLeaf::Literal`, `semstrait-planner` reports output schemas, `semstrait-adapter` renders them).
- The **constraint-DSL toolkit** (`§4`): `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints` (per `[11 §8.3](../foundations/11_constraints.md)` / `§8.4`). Shared between `semstrait-model` (Measure / Metric carriers) and `semstrait-planner` (constraint evaluation).
- The cross-cutting **diagnostic primitives** per `[30 §5](../foundations/30_stability_diagnostics.md)` (`§5`): `Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose` trait. Generic over the per-stage kind enum `K`; carries no expression vocabulary.
- The `io` module per `[31b](31b_semstrait_core_io.md)` (`§6`): byte-blob transport primitives. Unchanged by either cascade.

### 1.2 What `semstrait-core` does NOT own

- **The universal-traversal trait family.** `Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf` live in `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §3.2](35_semstrait_ir.md)`. Both `Expr<L>` and `PlanNode` — the only implementers — live in `semstrait-ir`; co-locating the traits with their producers eliminates the cross-crate hop downstream consumers previously paid.
- **The structural-variant support enums.** `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`, `Literal` live in `semstrait-ir::expr_kinds` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §3.4](35_semstrait_ir.md)`. They are referenced only by `Expr<L>` structural variants — also in `semstrait-ir`.
- **The shared identifier carriers.** `ColumnRef` and `SemanticsName` live in `semstrait-ir::expr_kinds` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §3.4](35_semstrait_ir.md)`. They are referenced only by leaves in `PhysicalLeaf` and `SemanticLeaf` — also in `semstrait-ir`.
- **The narrow ir-emitted error kinds.** `ValidateError` (raised by `Tree::with_new_children` and `Rewriter<N>::f_*`) and `CompileError` (raised by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec`) live in `semstrait-ir::error` per `[35 §15.1](35_semstrait_ir.md)` / `[§15.2](35_semstrait_ir.md)`. Their producers are in `semstrait-ir`; their natural home is alongside. Downstream stages embed via D.ii kind-nesting: `semstrait-model::ValidateError` carries `Ir(ir::ValidateError)`; `semstrait-manifest::CompileError` carries `Ir(ir::CompileError)`. **Naming note:** these two enums drop the `Kind` suffix per the scoped cleanup tied to this move; broader `*ErrorKind` enums (`ParseError**Kind**`, `PlanError**Kind**`, `AdaptError**Kind**`, `IrError**Kind**`, `OptimizeError**Kind**`, `RepositoryError**Kind**`, `SemStraitError**Kind**`) keep the suffix until a future global rename pass (`STATUS.md` deferred).
- **The canonical-IR expression types, the authoring-surface DSL, and the function registry.** `Expr<L>` / leaf sets / accessor enums / `Parameter` / `expr_fn` / `CanonicalFn` / `FunctionRegistry` all live in `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)` and `[35](35_semstrait_ir.md)` (first-cascade move, `STATUS.md` item N).
- **The YAML authoring surface.** `ExprSource` and the reserved-tag dispatch live in `semstrait-model` per `[14 §9.3](../foundations/14_expressions.md)` and `[32](32_semstrait_model.md)`. The `ExprSource::Block(...)` variant carries `Expr<L>` directly via serde derives on `Expr<L>` (`[35 §14.1](35_semstrait_ir.md)`) — there is **no separate `ExprBlock` type** (second-cascade simplification, item Q).
- **SemanticManifest structure, planner + plan tree, engine identity / dialect, catalog / filesystem, name resolution algorithms, domain load / dump wrappers.** Per the per-crate routing in `[INDEX.md §3](../INDEX.md)`.

### 1.3 Design posture — non-expression shared-vocabulary crate

After the second cascade, `semstrait-core` is the **minimal non-expression vocabulary** every layer above it agrees on. The rule for "does it belong here?" is precise:

> A type belongs in `semstrait-core` iff it is consumed by two or more crates that do **not** depend on `semstrait-ir`. Any type whose only consumers are expression trees or plan trees (both in `semstrait-ir`) belongs in `semstrait-ir`, not here.

That rule places `DataType` / `Grain` / `Schema` / `SchemaColumn` here (they cross from `semstrait-model` to `semstrait-adapter`), places `MeasureConstraints` here (it crosses from `semstrait-model` to `semstrait-planner`), places `Diagnostic<K>` here (it is the envelope every stage uses), and places `io` here (it is the transport every domain crate composes around). It places the trait family + support enums + identifier carriers + narrow error kinds *out* of here — none of them have a consumer outside the expression / plan world that lives in `semstrait-ir`.

The crate remains the **leaf** of the semstrait workspace DAG (I7): zero workspace dependencies; every other crate depends on it directly or transitively. Engine-identity deps (datafusion, arrow, duckdb, substrait) are rejected outright.

**I/O amendment (ratified in `[31b](31b_semstrait_core_io.md)`).** The `io` module provides the shared transport vocabulary that every downstream load / dump wrapper composes. Under default features (`io` ON), `semstrait-core` pulls `tokio`. Under `--no-default-features`, the crate retains its original zero-runtime-dep posture. Cloud SDKs (`aws-sdk-s3`, future `gcs` / `azure`) sit behind additional opt-in flags. The "no async, no I/O in core" blanket from earlier drafts is replaced by: "text-blob transport is a first-class core concern; domain-specific wrappers are not." Unchanged by either cascade.

## 2. Module Layout

Top-level `pub mod` structure after the second cascade. One module per cohesive concept; no cross-module cycles; no `pub use` re-exports of internal modules outside this table.

```
semstrait-core
├── types                // DataType, Grain, TypeClass, Schema, SchemaColumn
├── constraints          // MeasureConstraints, DimensionConstraints, AggregationConstraints
├── diagnostic           // Diagnostic<K>, Diagnostics<K>, Severity, Location, Span,
│                        //   SourceId, Diagnose trait
└── io                   // Source, Sink, Location, IoError + backends::{memory, local, s3}
                         //   (feature "io", default ON; s3 under "io-aws")
                         //   Full spec: 31b
```

Post-second-cascade roster is **four modules** (was seven). Departed in this pass: `tree` (moved to `semstrait-ir::tree`), `expr_kinds` (moved to `semstrait-ir::expr_kinds`), `error` (the two narrow ir-emitted kinds moved to `semstrait-ir::error`). Departed in the first pass: `expr` and `functions`.

**Split rationale:**

- `types` vs `constraints` — `types` carries the logical-type vocabulary (changes at the cadence of canonical-type-system evolution: a new `DataType::List` lands with collection support per `[13 §2.5](../foundations/13_types_and_grain.md)`); `constraints` carries the constraint-DSL shapes (changes at the cadence of Measure / Metric authoring evolution). Separating lets downstream consumers import one without the other's recompile cost.
- `diagnostic` — its own module so the generic envelope (`Diagnostic<K>`, `Diagnose`) ships with no per-stage kind enum coupling. Per-stage kind enums (`ParseError`, `ValidateError`, `CompileError`, `PlanErrorKind`, `AdaptErrorKind`, …) live in their owning crates; only `Diagnose` ties them to the envelope.
- `io` — full split rationale and back-end roster live in `[31b §2](31b_semstrait_core_io.md)`.

**Re-exports.** The crate root (`lib.rs`) re-exports a curated surface (§7). Non-root re-exports of internal helpers are forbidden — consumers either import `semstrait_core::DataType` or `semstrait_core::types::DataType`, never both.

## 3. Trait Family — moved to `semstrait-ir`

> **Moved at the second cascade (2026-05-19, `STATUS.md` item Q).** The universal-traversal trait family (`Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf`) now lives in `[semstrait-ir::tree](35_semstrait_ir.md)` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §3.2](35_semstrait_ir.md)`. The rationale: both `Expr<L>` and `PlanNode` — the only implementers — live in `semstrait-ir`; placing the traits with their producers eliminates the cross-crate hop downstream consumers previously paid. Cross-references in other docs of the form `31 §3.x` should be retargeted to `35 §3.2` in a follow-up cleanup pass (tracked under `STATUS.md` item Q as a transient inconsistency).

## 4. Public Types — `DataType` Family

### 4.1 `DataType` — canonical logical types

```rust
/// Canonical logical data types per `13 §2.1`. 14 scalar variants,
/// engine-neutral names. Complex types (arrays, structs, maps) are
/// out of scope for v1 per `13 §2.5`.
#[non_exhaustive]
pub enum DataType {
    Boolean,
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
    Decimal   { precision: u8, scale: i8 },
    String,
    Binary,
    Date,
    Time      { precision: u8 },
    Timestamp { precision: u8 },
    Interval,
}
```

Construction of `Decimal { precision, scale }` with out-of-range values is rejected at `semstrait-model` parse time (`ParseErrorKind::InvalidDecimalParameters`); `semstrait-core` performs no validation at the constructor. `DataType` is referenced by every layer above core — expression leaves and `Cast { target }` in `semstrait-ir`, function signatures (`ParamType::Exact`), plan-tree primitives (`ResolvedColumn`), and adapter renderers.

### 4.2 `Grain` — temporal granularity lattice

```rust
/// Temporal Grain levels per `13 §3.1`. Total coarseness order per
/// `13 §3.2`; exposed via `Grain::coarseness() -> u8`.
#[non_exhaustive]
pub enum Grain {
    Minute, Hour, Day, Week, Month, Quarter, Year,
}

impl Grain {
    /// Selection-rank order: Minute(0) < ... < Year(6). Per `13 §3.2`.
    pub fn coarseness(self) -> u8;
}
```

### 4.3 `TypeClass` — type-class grouping (per `13 §4`)

```rust
/// Bounded type classes used by `FnSignature` parameter expression per
/// `14a §3.3`. Pure grouping vocabulary; no method surface. Per `13 §4`.
#[non_exhaustive]
pub enum TypeClass {
    Numeric,      // Byte | Short | Integer | Long | Float | Double | Decimal
    Integral,     // Byte | Short | Integer | Long
    FloatingPt,   // Float | Double
    Textual,      // String
    Temporal,     // Date | Time | Timestamp
    Comparable,   // everything except Binary in the canonical set
    Any,          // all canonical variants
}
```

`TypeClass` is exposed but **not** wired into the v1 `ParamType` activation — `[14a §3.3](../foundations/14a_function_catalog.md)` Q6 ratified overload-set polymorphism, not type-class generics, for v1. `TypeClass` exists as vocabulary for future registry evolution (`[TD-REGISTRY-TYPECLASS]`, `[14a §10.1](../foundations/14a_function_catalog.md)`) and for documentation / advisory diagnostics. The reserved `ParamType::TypeClass(TypeClass)` variant lives in `semstrait-ir::functions` per `[35 §7.2](35_semstrait_ir.md)`.

### 4.4 `Schema` and `SchemaColumn` — physical-source schema

```rust
/// The compile-time snapshot of physical columns exposed by a source.
/// Per `15 §3.2`. Referenced by:
/// - `15 §3.1 PhysicalSource::{File, Stream, Table, Snapshot}` for the
///   resolved schema attached to every `PhysicalSource` variant.
/// - `35 §10.1 NodeMeta.output_schema` for the per-`PlanNode` output
///   schema (via plan-level `Schema`, which has the same shape).
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

pub struct SchemaColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}
```

`Schema.columns` is ordered in the source's native order (Parquet footer, Iceberg field order, CSV header order, …) per `[15 §3.2](../foundations/15_mapping_and_binding.md)`. Compile preserves the ordering for determinism (I4); the planner does not semantically depend on it. `nullable` is read from source metadata when available; it is not inferred. Both types are **field-stable** (no `#[non_exhaustive]`) per shared-vocabulary policy.

### 4.5 Nullability

Nullability is **NOT** exposed as a separate enum. Per `[13 §2](../foundations/13_types_and_grain.md)` and `[14a §3.4](../foundations/14a_function_catalog.md)` Q7, `DataType` is nullable-by-default; per-column nullability lives as the `SchemaColumn.nullable: bool` field. A standalone `Nullability` type was considered and rejected at `[14a §3.4](../foundations/14a_function_catalog.md)`.

### 4.6 Serialization feature flag

```rust
#[cfg(feature = "serde")]
impl serde::Serialize for DataType { /* ... */ }
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for DataType { /* ... */ }
// ... similarly for Grain, TypeClass, Schema, SchemaColumn, and every type in §5–§7.
```

Feature-gated per §11. Off by default; `semstrait-ir` / `semstrait-model` / `semstrait-manifest` enable it transitively (they require YAML / JSON round-tripping of values carried through these primitives).

## 5. Structural-Variant Support Enums + Identifier Carriers — moved to `semstrait-ir`

> **Moved at the second cascade (2026-05-19, `STATUS.md` item Q).** The structural-variant support enums (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`), the typed-literal carrier `Literal`, and the shared identifier carriers `ColumnRef` and `SemanticsName` all live in `[semstrait-ir::expr_kinds](35_semstrait_ir.md)` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §3.4](35_semstrait_ir.md)`. The rationale: they are referenced only by `Expr<L>` structural variants and leaves — also in `semstrait-ir`. Variant rosters and shapes ratified at `[14 §3.3](../foundations/14_expressions.md)`. Cross-references in other docs of the form `31 §5.x` should be retargeted to `35 §3.4` in a follow-up cleanup pass.

## 6. Public Types — Constraint Family

Per `[11 §8.3](../foundations/11_constraints.md)`–`§8.4`. Current type name `MeasureConstraints` is a legacy artifact for the Measure + Metric carriers (`[11 §8.4.3](../foundations/11_constraints.md)`, `[TD-CONSTRAINT-RENAME]`); the three sub-blocks below retain their current names to avoid a breaking rename before the SemanticManifest-schema revision pass.

### 6.1 `MeasureConstraints`

```rust
/// Constraint DSL block attached to Measure and Metric carriers.
/// Per `11 §8.4.1` / `§8.4.2`. Shape-field (`11 §8.8`).
#[non_exhaustive]
pub struct MeasureConstraints {
    pub dimensions: Option<DimensionConstraints>,
    pub aggregations: Option<AggregationConstraints>,
}

impl MeasureConstraints {
    pub fn none() -> Self;
    pub fn is_empty(&self) -> bool;
}
```

### 6.2 `DimensionConstraints`

```rust
/// Three-way set-membership policy over Dimension names. Per `11 §8.3` /
/// `§8.4.1`. All three fields optional; AND-combined.
#[non_exhaustive]
pub struct DimensionConstraints {
    pub one_of: Vec<String>,
    pub none_of: Vec<String>,
    pub all: Vec<String>,
}

impl DimensionConstraints {
    pub fn none() -> Self;
    pub fn is_empty(&self) -> bool;
}
```

### 6.3 `AggregationConstraints`

```rust
/// Two-way whitelist/blacklist policy over UPPERCASE aggregation-name
/// tokens (matching the `AggregationOp` enum names plus `COUNT_DISTINCT`
/// for the `distinct: true` encoding). Per `11 §8.4.1`.
#[non_exhaustive]
pub struct AggregationConstraints {
    pub allowed: Vec<String>,
    pub prohibited: Vec<String>,
}

impl AggregationConstraints {
    pub fn none() -> Self;
    pub fn is_empty(&self) -> bool;
}
```

**Why `Vec<String>` rather than `Vec<AggregationOp>` for `allowed` / `prohibited`.** Per `[11 §8.4.1](../foundations/11_constraints.md)`'s DSL shape: the field accepts UPPERCASE tokens including `COUNT_DISTINCT` (which is not an `AggregationOp` enum variant but an `Expr<L>::Aggregate { op: Count, distinct: true, ... }` encoding). Matching is token-based against a planner-owned normalization; `semstrait-core` exposes the shape, not the matching logic (which lives in `semstrait-planner` per `[11 §8.6](../foundations/11_constraints.md)`).

## 7. Public Types — Diagnostic Primitives

`semstrait-core` provides the **diagnostic envelope** every consumer crate composes around its own per-stage typed-kind enum, plus the `Diagnose` trait those kinds implement. Authoritative sub-shape per `[30 §5](../foundations/30_stability_diagnostics.md)`.

### 7.1 `Severity`

```rust
/// Per `30 §5.2`. Two variants only — `Info` retired into the `tracing`
/// channel (`30 §6`).
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}
```

Severity carries message intent only; control flow (accumulating vs fail-fast) lives in the function signature, not the diagnostic. Per `[30 §5.2](../foundations/30_stability_diagnostics.md)`.

### 7.2 `Location`, `Span`, `SourceId`

```rust
/// Source-level location. Per `30 §5.3`.
#[non_exhaustive]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}

/// Half-open byte-offset span into the source buffer.
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Opaque identifier for the originating source document. Constructors
/// live on the producing crate (`semstrait-model` for YAML files,
/// inline-string for tests, etc.); `semstrait-core` exposes only the
/// shape consumers need.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SourceId(/* crate-private */);

impl SourceId {
    pub const fn unknown() -> Self;
    pub fn as_str(&self) -> &str;
}
```

`Span` retains its `[30 §5.3](../foundations/30_stability_diagnostics.md)` byte-range shape. `SourceId` is opaque — it is NOT `#[non_exhaustive]` because its variant set is private; the crate's public surface exposes only `SourceId::unknown()` and the `Eq` / `Hash` / `Display` traits consumers need.

### 7.3 `Diagnostic<K>` — generic envelope

```rust
/// Generic envelope wrapping a per-stage typed kind. Per `30 §5.1`.
/// Each consumer crate plugs in its own `K: Diagnose`.
#[non_exhaustive]
pub struct Diagnostic<K: Diagnose> {
    pub kind: K,
    pub severity: Severity,
    pub location: Option<Location>,
    pub notes: Vec<String>,
}

/// Type alias for an accumulating-stage diagnostic vector.
pub type Diagnostics<K> = Vec<Diagnostic<K>>;
```

Construction is via per-crate helpers; `semstrait-core` does not expose a `Diagnostic::new` or builder. Each consumer crate's helper sets `severity` from `K::severity_default()` (overridable) and attaches `location` / `notes` as appropriate.

### 7.4 `Diagnose` trait

```rust
/// The trait every per-stage kind implements. Per `30 §5.4`.
pub trait Diagnose {
    /// Human-readable rendering. Powers the `Display` impl on
    /// `Diagnostic<K>`. Must not include line breaks (callers add
    /// their own framing).
    fn message(&self) -> String;

    /// Default severity for this variant. Construction sites may
    /// override; most callers accept the default.
    fn severity_default(&self) -> Severity;

    /// Foreign-error chain for `std::error::Error` interop. Default
    /// `None`. Variants wrapping foreign errors override to return
    /// `Some(&inner)`.
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
```

`Diagnose` is **open** — third-party kind enums (e.g. a downstream plugin's typed error) implement it and slot into the `Diagnostic<K>` envelope.

### 7.5 Blanket impls on `Diagnostic<K>`

```rust
impl<K: Diagnose> std::fmt::Display for Diagnostic<K> { /* delegates to K::message() */ }
impl<K: Diagnose + std::fmt::Debug> std::error::Error for Diagnostic<K> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.cause()
    }
}
```

Both blanket impls live in `semstrait-core::diagnostic` because of the orphan rule — no other crate could provide them. `Diagnose::cause()` is the source of truth for the `std::error::Error` chain.

### 7.6 `Diagnostics<K>` ergonomics

`Diagnostics<K>` is a transparent `Vec<Diagnostic<K>>` alias, so all `Vec` methods apply directly. Per `[30 §5.6](../foundations/30_stability_diagnostics.md)`, fused helpers in `semstrait-api` use a sum-typed kind (`SemStraitErrorKind`) and lift per-stage results via `From<ParseErrorKind>`, `From<ValidateError>`, `From<CompileError>`, etc. — `From` impls live on the fused-kind enum, not on `Diagnostic<K>`.

## 8. Narrow Error Kinds — moved to `semstrait-ir`

> **Moved at the second cascade (2026-05-19, `STATUS.md` item Q).** The narrow `ValidateError` (raised by `Tree::with_new_children` and `Rewriter<N>::f_*`) and `CompileError` (raised by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec`) now live in `[semstrait-ir::error](35_semstrait_ir.md)` per `[35 §15.1](35_semstrait_ir.md)` / `[§15.2](35_semstrait_ir.md)`. The rationale: their producers (`Tree`, `FunctionSpec`) moved to `semstrait-ir` in the same cascade. Downstream stages embed via D.ii kind-nesting (`[30 §7.4](../foundations/30_stability_diagnostics.md)`): `model::ValidateError` carries `Ir(ir::ValidateError)`; `manifest::CompileError` carries `Ir(ir::CompileError)`.
>
> **Naming.** The `Kind` suffix is dropped on these two enums per the scoped cleanup tied to the second-cascade landing. The broader `*ErrorKind` enums elsewhere in the workspace (`ParseErrorKind` in 32, `PlanErrorKind` in 34, `AdaptErrorKind` in 36, `IrErrorKind` in 35, `RepositoryErrorKind` / `CatalogProviderErrorKind` / `FileSystemErrorKind` in 33 / 37, `SemStraitErrorKind` in 38, `OptimizeErrorKind` in 34) keep the suffix until a future global rename pass.

Cross-references in other docs of the form `31 §8.x` should be retargeted to `35 §15.1` / `§15.2` in a follow-up cleanup pass.


## 9. Public Free Functions

After the second cascade, `semstrait-core` exposes **no expression-related, function-registry-related, or YAML-parsing-related free functions**:

- `function_registry()` lives in `semstrait-ir::functions::function_registry()` per `[35 §7.2](35_semstrait_ir.md)` (every consumer of `Expr<L>::FunctionCall { name: CanonicalFn, ... }` needs the registry, and `Expr<L>` lives in `semstrait-ir`).
- `is_reserved_tag(&str) -> bool` lives in `semstrait-model` per `[14 §9.3](../foundations/14_expressions.md)` (the helper is consumed by the `ExprSource::Block(...)` parser, which lives in `semstrait-model`; co-locating it with the parse-site dispatch keeps the reserved-tag catalog single-sourced).

A `coarseness(g: Grain) -> u8` free-function form was considered and rejected — it is already exposed as `Grain::coarseness(self)` per `[13 §3.2](../foundations/13_types_and_grain.md)`. A `type_class_of(dt: DataType) -> TypeClass` helper was considered and rejected — it would encode classification policy that `[13 §4](../foundations/13_types_and_grain.md)` leaves to authors. Any new free function requires ratification against this section in an amendment.

## 10. Traits Exported

Per `[30](../foundations/30_stability_diagnostics.md)`'s trait-surface rules: every public trait that can be externally implemented SHOULD be evaluated against the sealed-trait pattern. Sealed traits prevent external impls; non-sealed traits are part of the external extension surface.

After the second cascade, `semstrait-core` exposes **one** trait: `Diagnose`. The `Tree` / `Visitor<N>` / `Rewriter<N>` / `ExprLeaf` family moved to `semstrait-ir` per `[35 §3.2](35_semstrait_ir.md)`; the `RegistryExtension` adapter-contribution hook moved to `semstrait-ir::functions::RegistryExtension` per `[35 §7.2](35_semstrait_ir.md)`.

| Trait | Surface | Externally implementable? | Sealed? | Source |
|---|---|---|---|---|
| `Diagnose` | `fn message`, `fn severity_default`, `fn cause` | yes — third-party kind enums slot into `Diagnostic<K>` | no | §7.4 |

**Sealed-trait justification, positive cases.** None. `Diagnose` is non-sealed because a third-party error type from e.g. a user-defined plugin MAY define its own kind and slot into `Diagnostic<K>`.

## 11. Feature Flags

v1 has a small, axis-orthogonal flag set. The `io`-family flags were added in the `[31b](31b_semstrait_core_io.md)` ratification. After the second cascade, the `serde` flag gates a much smaller surface area — the entire expression vocabulary (trait family, support enums, `Literal`, `ColumnRef`, `SemanticsName`, narrow error kinds, plus everything the first cascade already moved) lives in `semstrait-ir` per `[35 §14](35_semstrait_ir.md)`, and the YAML authoring surface (`ExprSource`) lives in `semstrait-model`.

| Feature | Default | Gates | Reason |
|---|---|---|---|
| `serde` | OFF | `Serialize` / `Deserialize` on every public type in this crate: `DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`, the constraint family, `Diagnostic<K>` (where `K: Serialize`), `Severity`, `Location`, `Span`, `SourceId` | keeps the crate's dependency footprint minimal for consumers that only need types; `semstrait-ir` / `semstrait-model` / `semstrait-manifest` enable it transitively |
| `schemars` | OFF | JSON schema derivations on the same types | consumers needing JSON-Schema emission pay a second compile cost; off by default per `[30](../foundations/30_stability_diagnostics.md)` |
| `io` | **ON** | The `io` module — `Source` / `Sink` / `FromIoBytes` / `IntoIoBytes` / `Location` / `IoError` + `backends::memory` + `backends::local`; pulls `tokio`, `bytes`, `object_store` (Local + InMemory features), `dashmap` | ergonomic common case for every downstream crate that wants transport; disable with `default-features = false` for pure-type consumers (see `[31b §9.1](31b_semstrait_core_io.md)`) |
| `io-aws` | OFF | `Location::S3` variant + `backends::s3::{S3Source, S3SourceBuilder}`; enables `object_store/aws` which transitively pulls the AWS config / credential crates | cloud SDK footprint stays opt-in; enabled explicitly by CLI / `semstrait-api` / `semstrait-facade` |

No other I/O features in v1. Future `io-http`, `io-gcs`, `io-azure` land additively behind the same gating pattern.

**Delta with current code.** Moving the first-cascade expression family + `FunctionRegistry` to `semstrait-ir` is tracked under `[TD-CORE-EXPR-MIGRATION]` in `[implementation/40_refactor_plan.md](../implementation/40_refactor_plan.md)`. The second-cascade move (trait family + support enums + identifier carriers + narrow error kinds) is tracked under `[TD-CORE-TRAIT-VOCAB-MIGRATION]`. Moving `serde` behind `#[cfg(feature = "serde")]` is tracked under `[TD-CORE-SERDE-GATING]`. The `io` module is a net-new addition per `[31b](31b_semstrait_core_io.md)` / `TD-008`. `arrow-feature` gating is rejected — engine-specific data-plane deps violate I11.

## 12. Dependency Posture

### 12.1 External dependencies

A canonical `Cargo.toml` target after the `[31b](31b_semstrait_core_io.md)` ratification and the `[14](../foundations/14_expressions.md)` cascade:

```toml
[dependencies]
thiserror = "^"                         # error enum derivations

[dependencies.serde]
version = "^"
optional = true
features = ["derive"]

[dependencies.schemars]
version = "^"
optional = true

[dependencies.tokio]                    # I/O runtime; gated by "io"
version = "^"
optional = true
features = ["rt", "fs", "io-util", "macros"]

[dependencies.bytes]                    # zero-copy byte buffers for Source::read_raw / Sink::write_raw
version = "^"
optional = true

[dependencies.dashmap]                  # (region, endpoint) → AmazonS3 client cache for Location dispatch
version = "^"
optional = true

[dependencies.object_store]             # back-end wrapper (Apache Arrow); gated by "io"
version = "^"
optional = true
default-features = false                # enable only what's needed per feature

[features]
default  = ["io"]
serde    = ["dep:serde"]
schemars = ["dep:schemars", "serde"]
io       = ["dep:tokio", "dep:bytes", "dep:dashmap", "dep:object_store"]
io-aws   = ["io", "object_store/aws"]
```

**Runtime dependency posture.** Under default features, `semstrait-core` pulls `tokio`, `bytes`, `dashmap`, and `object_store` (with its `Local` + `InMemory` back-ends compiled; no cloud SDKs unless `io-aws` is enabled). Under `--no-default-features`, the crate retains its historical zero-runtime-dep shape — only `thiserror` remains. Pure-type consumers take the `--no-default-features` path.

**`object_store` as internal detail.** Consumers never see `object_store::ObjectStore`, `object_store::Path`, or any of its error types on a public signature. The one escape hatch is `S3SourceBuilder::with_object_store_builder(object_store::aws::AmazonS3Builder)` — callers opting into advanced S3 configuration implicitly opt into `object_store` evolution. See `[31b §1.4](31b_semstrait_core_io.md)` for the adoption rationale and SR-IO-8 for the encapsulation rule.

**No other runtime deps.** No `async-trait` (stable async-fn-in-trait suffices), no `futures` beyond what `tokio` re-exports, no `reqwest`, no `hyper`, no `sqlx`, no direct `aws-sdk-s3` / `aws-config` (they come in transitively via `object_store/aws`).

**No engine-identity dependencies.** No `datafusion`, no `arrow`, no `spark-*`, no `duckdb`, no `substrait`. These live in `semstrait-adapter` and its per-engine modules. The pre-cascade `nonzero_ext` dep is removed — `Span`'s `usize` field stays plain.

### 12.2 Internal (workspace) dependencies

**Zero.** `semstrait-core` is the root of the workspace DAG per I7. Every other crate depends on `semstrait-core` directly or transitively; `semstrait-core` depends on no workspace crate. Attempting to add a `semstrait-*` dependency to `Cargo.toml` is a compile error in CI.

## 13. Invariants Upheld by the Crate

Concrete crate-level guarantees mapping to `[00 §9](../00_overview.md)` invariants. After the `14` cascade, several invariants narrow their crate-level scope because the types those invariants used to anchor have moved upstream.

| Invariant | `semstrait-core` guarantee |
|---|---|
| **I1** — no raw SQL in canonical layer | `semstrait-core` exposes no string-as-SQL types. The pre-cascade carriers (`Expr`, `Literal`, `ColumnRef`, `SemanticsName`, `ExprSource::Inline(String)`) moved to `semstrait-ir` / `semstrait-model`; every remaining surface here is typed-value (`DataType`) or constraint-DSL block. |
| **I2** — physical types belong to adapters | `DataType` variants are engine-neutral per `[13 §2](../foundations/13_types_and_grain.md)`. No `arrow::*` / `spark::*` / `datafusion::*` types are visible on any public surface. `Schema` and `SchemaColumn` carry `DataType`, not engine-native types. |
| **I5** — name resolution is compile-time | `semstrait-core` declares no semantic-reference types — those live in `semstrait-ir::SemanticLeaf` per `[35 §4.2](35_semstrait_ir.md)`. Resolution is performed by `semstrait-manifest::compile` per `[19 §3](../foundations/19_expression_flow.md)`. The core-level surface is *the absence* of any resolution shape or any identifier carrier. |
| **I7** — strict DAG | `Cargo.toml` contains zero `semstrait-*` entries in `[dependencies]`. A CI check greps the manifest and fails on any workspace-internal entry. |
| **I10** — extensibility | Every `pub enum` and `pub struct` (with the `[30](../foundations/30_stability_diagnostics.md)`-documented newtype-over-stable exception) carries `#[non_exhaustive]`. The exception set: `Span`, `SourceId` (opaque per `[30 §5.3](../foundations/30_stability_diagnostics.md)`), `Schema`, `SchemaColumn` (field-stable shared-vocabulary types). An `integration-test` over `cargo public-api` enforces the `#[non_exhaustive]` rule. |
| **I11** — no downward I/O surprises | Transport primitives (`io::Source`, `io::Sink`, `io::Location`, `io::backends::{memory, local, s3}`) live on `semstrait-core` under the `io` feature flag (ratified in `[31b](31b_semstrait_core_io.md)`). Domain-specific load / dump (`load_model`, `load_manifest`) do not — they live in the crate that owns the typed artifact. `reqwest`, `hyper`, raw `std::net` sockets remain rejected; cloud SDKs (`aws-sdk-s3`) sit behind opt-in `io-aws`. The dependency audit (§12.1) is enforced in CI via `cargo deny`. |
| **I12** — first-class diagnostics | `Diagnostic<K>` and `Diagnose` are the workspace's diagnostic primitives. `Diagnostic<K>` carries `kind: K, severity, location, notes`; the kind decides per-variant rendering and severity defaults via `Diagnose`. No central error-code allocation; stable identification is variant identity. The parallel observability channel (`tracing`) is described in `[30 §6](../foundations/30_stability_diagnostics.md)`; library code never writes to stdout / stderr. `IoError` per `[31b §6](31b_semstrait_core_io.md)` is its own kind enum implementing `Diagnose`. The narrow `ValidateError` / `CompileError` raised by trait / `FunctionSpec` machinery live in `semstrait-ir::error` per `[35 §15](35_semstrait_ir.md)`. |

## 14. Public API Surface Sketch

One rustdoc-style line per exported item, grouped by module. Doubles as the "test-the-contract" target — an integration test enumerates this list against `cargo public-api` output. After the second cascade, the surface is the four non-expression modules only.

### 14.1 `types`

```
pub enum   DataType                                       // 14 variants per 13 §2.1
pub enum   Grain                                          // 7 variants per 13 §3.1
pub enum   TypeClass                                      // 7 variants per 13 §4
pub struct Schema                                         // { columns: Vec<SchemaColumn> }
pub struct SchemaColumn                                   // { name, data_type, nullable }
impl Grain { pub fn coarseness(self) -> u8 }
```

### 14.2 `constraints`

```
pub struct MeasureConstraints                             // { dimensions, aggregations }
pub struct DimensionConstraints                           // { one_of, none_of, all }
pub struct AggregationConstraints                         // { allowed, prohibited }
// Each of the three exposes fn none() and fn is_empty(&self) -> bool.
```

### 14.3 `diagnostic`

```
pub struct Diagnostic<K: Diagnose>                        // { kind, severity, location, notes }
pub type   Diagnostics<K> = Vec<Diagnostic<K>>
pub enum   Severity                                       // Error | Warning
pub struct Location                                       // { source: SourceId, span: Span }
pub struct Span                                           // { start: usize, end: usize }
pub struct SourceId                                       // opaque; SourceId::unknown() + as_str()
pub trait  Diagnose                                       // fn message + severity_default + cause
// Blanket: Display / std::error::Error on Diagnostic<K> via Diagnose.
```

### 14.4 Crate-root re-exports (stable convenience surface)

```rust
// lib.rs
pub use crate::types::{DataType, Grain, TypeClass, Schema, SchemaColumn};
pub use crate::constraints::{
    MeasureConstraints, DimensionConstraints, AggregationConstraints,
};
pub use crate::diagnostic::{
    Diagnostic, Diagnostics, Severity, Location, Span, SourceId, Diagnose,
};
```

### 14.5 `io` (feature `io`, default ON)

Full spec: `[31b](31b_semstrait_core_io.md)`. This section is a re-export sketch only.

```
pub use self::io::{
    Source, Sink,                       // §31b §3–§4 — byte-blob transport traits
    FromIoBytes, IntoIoBytes,           // §31b §5 — byte↔typed conversion traits
    Location,                           // §31b §6 — polymorphic back-end dispatch
    IoError,                            // §31b §7 — #[non_exhaustive] error enum
};

pub mod io::backends::memory  { pub struct InMemory; }
pub mod io::backends::local   { pub struct LocalFile; }
#[cfg(feature = "io-aws")]
pub mod io::backends::s3 {
    pub struct S3Source;
    pub struct S3SourceBuilder;         // §31b §8.3 — custom S3 configuration
}
```

Internally every back-end thin-wraps `object_store` (Apache Arrow project); `object_store` types never appear on a public signature except the one documented escape hatch (`S3SourceBuilder::with_object_store_builder`). See `[31b §1.4](31b_semstrait_core_io.md)` for the adoption rationale and SR-IO-8 for the encapsulation rule.
