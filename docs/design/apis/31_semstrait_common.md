---
prereqs: [11, 30]
authoritative-for:
  - the `semstrait-common` public-API surface (types, traits, free functions)
  - module layout within `semstrait-common` (top-level `pub mod`s)
  - the cross-cutting **diagnostic primitives** (`Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose` trait)
  - the **constraint-DSL toolkit** types (`MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints`) — shape-only DSL, no expression vocabulary
  - the boundary rule between shape-only constraints (here) and expression-bodied future constraints (in `semstrait-ir`)
  - feature-flag surface (`serde`, `schemars`, `io`, `io-aws`) and dependency posture
  - mapping of design invariants I7, I11, I12 to concrete crate-level guarantees
refined-by:
  - 11 (constraint-DSL semantics — `31` carries the type shapes only)
  - 30 (diagnostic-envelope contract — `31` carries the implementation)
  - 31b (`semstrait-common::io` — text-blob transport module, amends §1.3 / §2 / §5 of this doc)
  - 32 (`semstrait-model` declares its own `ParseError` and `ValidateError`; embeds `Ir(ir::ValidateError)` via D.ii; adds `io` wrappers over `31b`)
  - 33 (`semstrait-manifest` declares its own `CompileError`; embeds `Ir(ir::CompileError)` via D.ii; adds `io` wrappers over `31b`)
  - 34 (`semstrait-planner` declares `PlanErrorKind` / `OptimizeErrorKind`)
  - 35 (`semstrait-ir` owns the canonical type vocabulary `DataType` / `Grain` / `TypeClass` / `Schema` / `SchemaColumn`, all expression-tied types, the `FunctionRegistry` family, and the narrow `ValidateError` / `CompileError` ir-emitted kinds)
  - 36 (`semstrait-adapter` declares `AdaptErrorKind`)
  - 38 (`semstrait-api` declares the sum-typed `SemStraitErrorKind` lifting per-stage kinds)
  - 40 (`implementation/40_refactor_plan.md` — current code vs target layout delta)
---

# 31. semstrait-common

> **Status:** ratified. `semstrait-common` is the workspace's **infrastructure crate**: it owns the diagnostic envelope, byte-blob `io` transport, and shape-only constraint DSL. Stage-agnostic, expression-free, plan-free. Every other workspace crate depends on it directly or transitively.

## 1. Purpose and Scope

`semstrait-common` is the substrate crate at the root of the workspace DAG (I7). It owns stage-agnostic infrastructure: the diagnostic envelope, the byte-blob `io` transport, and the shape-only constraint DSL.

Surface roster: see §1.1.

### 1.1 What `semstrait-common` OWNS

- The cross-cutting **diagnostic primitives** (`§4`): `Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, and the `Diagnose` trait. Per `[30 §5](../foundations/30_stability_diagnostics.md)`. Every consumer crate composes these around its own per-stage typed-kind enum.
- The byte-blob **`io` transport** (`§5`): `Source`, `Sink`, `Location`, `IoError`, and the `backends::{memory, local, s3}` family. Full sub-spec at `[31b](31b_semstrait_common_io.md)`.
- The shape-only **constraint-DSL toolkit** (`§6`): `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints` per `[11 §8.3](../foundations/11_constraints.md)` / `§8.4`. Shared between `semstrait-model` (Measure / Metric authoring carriers) and `semstrait-planner` (constraint evaluation).

### 1.2 What `semstrait-common` does NOT own

| Surface | Owning crate | Doc |
|---|---|---|
| Canonical type vocabulary (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`) | `semstrait-ir::types` | `[35](35_semstrait_ir.md)` |
| Expression-tree types (`Expr<L>`, leaves, `Parameter`, op enums, `Literal`, `ColumnRef`, `SemanticsName`, `expr_fn`, `CanonicalFn` / `FunctionRegistry`, ir-emitted `ValidateError` / `CompileError`) | `semstrait-ir` | `[35](35_semstrait_ir.md)` |
| Plan-tree types (`SemanticPlan`, `PlanNode`, `NodeMeta`, `SourceRef`, `ResolvedColumn`) | `semstrait-ir` | `[35](35_semstrait_ir.md)` |
| YAML authoring surface (`ExprSource`, reserved-tag dispatch) | `semstrait-model` | `[14 §9.3](../foundations/14_expressions.md)`, `[32](32_semstrait_model.md)` |
| `SemanticManifest`, planner + plan tree, engine identity / dialect, catalog / filesystem, name resolution, domain load / dump wrappers | per-crate | `[INDEX.md §3](../INDEX.md)` |

### 1.3 Design posture — workspace-wide infrastructure

Placement rule: a type belongs in `semstrait-common` iff (a) consumed by two or more stages and (b) carries no per-stage vocabulary (no expression tree, no plan tree, no engine identity, no canonical type roster).

- Stage-agnostic: only types meeting both clauses above.
- Workspace-DAG root: zero `semstrait-*` workspace dependencies.
- No engine-identity dependencies: `datafusion`, `arrow`, `duckdb`, `substrait` are rejected.
- I/O posture: `io` feature default ON pulls `tokio` / `bytes` / `object_store` / `dashmap`; `--no-default-features` keeps only `thiserror`. Cloud SDKs sit behind additional opt-in flags. Full spec: `[31b](31b_semstrait_common_io.md)`.

### 1.4 Boundary rule for expression-bodied future constraints

Shape-only constraint blocks live in `semstrait-common::constraints` (§6); any constraint carrying `Expr<L>` or other expression-tree vocabulary lives in `semstrait-ir`.

Triggers: a new constraint field references `Expr<L>`, `SemanticLeaf`, or any expression-bearing type.

Escalation: introduce the new constraint in `semstrait-ir`, never in `semstrait-common`; cross-reference `[35 §1.1](35_semstrait_ir.md)`.

## 2. Module Layout

Top-level `pub mod` structure. One module per cohesive concept; no cross-module cycles.

```
semstrait-common
├── diagnostic           // Diagnostic<K>, Diagnostics<K>, Severity, Location, Span,
│                        //   SourceId, Diagnose trait
├── constraints          // MeasureConstraints, DimensionConstraints, AggregationConstraints
└── io                   // Source, Sink, Location, IoError + backends::{memory, local, s3}
                         //   (feature "io", default ON; s3 under "io-aws")
                         //   Full spec: 31b
```

**Three modules, no more.** Each module is independently importable; `lib.rs` re-exports a curated convenience surface (§7).

**Split rationale:**

- `diagnostic` is its own module so the generic envelope (`Diagnostic<K>`, `Diagnose`) ships with no per-stage kind enum coupling. Per-stage kind enums (`ParseErrorKind`, `ValidateError`, `CompileError`, `PlanErrorKind`, `AdaptErrorKind`, …) live in their owning crates; only `Diagnose` ties them to the envelope.
- `constraints` is separated from `diagnostic` because the two evolve at different cadences — `diagnostic` changes when stability rules around envelope rendering change; `constraints` changes when authoring-DSL shapes evolve. Separating lets downstream consumers import one without the other's recompile cost.
- `io` — full split rationale and back-end roster live in `[31b §2](31b_semstrait_common_io.md)`.

## 3. Trait Family

`semstrait-common` exposes **one** trait: `Diagnose` (§4.4). Every per-stage kind enum implements it.

The traversal trait family (`Tree` / `Visitor<N>` / `Rewriter<N>` / `ExprLeaf`) lives in `semstrait-ir` per `[35 §3.2](35_semstrait_ir.md)` — both `Expr<L>` and `PlanNode`, the only implementers, live there. The `RegistryExtension` adapter-contribution hook lives in `semstrait-ir::functions` per `[35 §8.2](35_semstrait_ir.md)`.

## 4. Public Types — Diagnostic Primitives

`semstrait-common` provides the **diagnostic envelope** every consumer crate composes around its own per-stage typed-kind enum, plus the `Diagnose` trait those kinds implement. Authoritative sub-shape per `[30 §5](../foundations/30_stability_diagnostics.md)`.

### 4.1 `Severity`

```rust
/// Per `30 §5.2`. Two variants only — `Info` retired into the `tracing`
/// channel (`30 §6`).
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}
```

Severity carries message intent only; control flow (accumulating vs fail-fast) lives in the function signature, not the diagnostic. Fail-fast stages return `Result<T, Diagnostic<K>>`; accumulating stages return `Diagnostics<K>` plus an outcome. Both modes use the same envelope shape.

### 4.2 `Location`, `Span`, `SourceId`

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
/// inline-string for tests, etc.); `semstrait-common` exposes only the
/// shape consumers need.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SourceId(/* crate-private */);

impl SourceId {
    pub const fn unknown() -> Self;
    pub fn as_str(&self) -> &str;
}
```

`Span` retains its `[30 §5.3](../foundations/30_stability_diagnostics.md)` byte-range shape. `SourceId` is opaque — it is NOT `#[non_exhaustive]` because its variant set is private; the crate's public surface exposes only `SourceId::unknown()` and the `Eq` / `Hash` / `Display` traits consumers need.

### 4.3 `Diagnostic<K>` — generic envelope

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

Construction is via per-crate helpers; `semstrait-common` does not expose a `Diagnostic::new` or builder. Each consumer crate's helper sets `severity` from `K::severity_default()` (overridable) and attaches `location` / `notes` as appropriate.

### 4.4 `Diagnose` trait

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

### 4.5 Blanket impls on `Diagnostic<K>`

```rust
impl<K: Diagnose> std::fmt::Display for Diagnostic<K> { /* delegates to K::message() */ }
impl<K: Diagnose + std::fmt::Debug> std::error::Error for Diagnostic<K> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.cause()
    }
}
```

Both blanket impls live in `semstrait-common::diagnostic` because of the orphan rule — no other crate could provide them. `Diagnose::cause()` is the source of truth for the `std::error::Error` chain.

### 4.6 `Diagnostics<K>` ergonomics

`Diagnostics<K>` is a transparent `Vec<Diagnostic<K>>` alias, so all `Vec` methods apply directly. Per `[30 §5.6](../foundations/30_stability_diagnostics.md)`, fused helpers in `semstrait-api` use a sum-typed kind (`SemStraitErrorKind`) and lift per-stage results via `From<ParseErrorKind>`, `From<ValidateError>`, `From<CompileError>`, etc. — `From` impls live on the fused-kind enum, not on `Diagnostic<K>`.

## 5. Public Types — `io` Transport

Full spec: `[31b](31b_semstrait_common_io.md)`. This section is a re-export sketch only; consult `31b` for trait shapes, back-end semantics, and `IoError` variants.

```
pub use self::io::{
    Source, Sink,                       // 31b §3–§4 — byte-blob transport traits
    FromIoBytes, IntoIoBytes,           // 31b §5 — byte↔typed conversion traits
    Location,                           // 31b §6 — polymorphic back-end dispatch
    IoError,                            // 31b §7 — #[non_exhaustive] error enum
};

pub mod io::backends::memory  { pub struct InMemory; }
pub mod io::backends::local   { pub struct LocalFile; }
#[cfg(feature = "io-aws")]
pub mod io::backends::s3 {
    pub struct S3Source;
    pub struct S3SourceBuilder;         // 31b §8.3 — custom S3 configuration
}
```

Internally every back-end thin-wraps `object_store` (Apache Arrow project); `object_store` types never appear on a public signature except the documented escape hatch `S3SourceBuilder::with_object_store_builder`. See `[31b §1.4](31b_semstrait_common_io.md)`.

## 6. Public Types — Constraint Family

Per `[11 §8.3](../foundations/11_constraints.md)`–`§8.4`. Shape-only DSL block attached to `Measure` and `Metric` carriers. All three sub-blocks are shape-only: scalar fields, scalar lists, no expression vocabulary. Per the §1.4 boundary rule, expression-bodied future constraints belong in `semstrait-ir`, not here.

The current type name `MeasureConstraints` is a legacy artifact for the `Measure` + `Metric` carriers (`[11 §8.4.3](../foundations/11_constraints.md)`, `[TD-CONSTRAINT-RENAME]`); the three sub-blocks below retain their current names to avoid a breaking rename before the SemanticManifest-schema revision pass.

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

The `allowed` / `prohibited` fields take UPPERCASE tokens (`COUNT_DISTINCT` included; this is not an `AggregationOp` enum variant but the `Expr<L>::Aggregate { op: Count, distinct: true, ... }` encoding). Token matching is planner-owned per `[11 §8.6](../foundations/11_constraints.md)`; `semstrait-common` exposes the shape, not the matching logic.

## 7. Public API Surface Sketch

One rustdoc-style line per exported item, grouped by module. Doubles as the "test-the-contract" target — an integration test enumerates this list against `cargo public-api` output.

### 7.1 `diagnostic`

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

### 7.2 `constraints`

```
pub struct MeasureConstraints                             // { dimensions, aggregations }
pub struct DimensionConstraints                           // { one_of, none_of, all }
pub struct AggregationConstraints                         // { allowed, prohibited }
// Each of the three exposes fn none() and fn is_empty(&self) -> bool.
```

### 7.3 `io` (feature `io`, default ON)

Full surface in `[31b](31b_semstrait_common_io.md)`. Re-export sketch in §5.

### 7.4 Crate-root re-exports (stable convenience surface)

```rust
// lib.rs
pub use crate::diagnostic::{
    Diagnostic, Diagnostics, Severity, Location, Span, SourceId, Diagnose,
};
pub use crate::constraints::{
    MeasureConstraints, DimensionConstraints, AggregationConstraints,
};
// io re-exports per §5 (feature-gated).
```

Non-root re-exports of internal helpers are forbidden — consumers either import `semstrait_common::Diagnostic` or `semstrait_common::diagnostic::Diagnostic`, never both via two paths.

## 8. Feature Flags

| Feature | Default | Gates | Reason |
|---|---|---|---|
| `serde` | OFF | `Serialize` / `Deserialize` on every public type in this crate: `Diagnostic<K>` (where `K: Serialize`), `Severity`, `Location`, `Span`, `SourceId`, the constraint family | keeps the crate's dependency footprint minimal for consumers that only need types; `semstrait-ir` / `semstrait-model` / `semstrait-manifest` enable it transitively |
| `schemars` | OFF | JSON schema derivations on the same types | consumers needing JSON-Schema emission pay a second compile cost |
| `io` | **ON** | The `io` module — `Source` / `Sink` / `FromIoBytes` / `IntoIoBytes` / `Location` / `IoError` + `backends::memory` + `backends::local`; pulls `tokio`, `bytes`, `object_store` (Local + InMemory features), `dashmap` | ergonomic common case for every downstream crate that wants transport; disable with `default-features = false` for pure-type consumers (see `[31b §9.1](31b_semstrait_common_io.md)`) |
| `io-aws` | OFF | `Location::S3` variant + `backends::s3::{S3Source, S3SourceBuilder}`; enables `object_store/aws` | cloud SDK footprint stays opt-in |

Future `io-http`, `io-gcs`, `io-azure` land additively behind the same gating pattern.

## 9. Dependency Posture

### 9.1 External dependencies

```toml
[dependencies]
thiserror = "^"

[dependencies.serde]
version = "^"
optional = true
features = ["derive"]

[dependencies.schemars]
version = "^"
optional = true

[dependencies.tokio]
version = "^"
optional = true
features = ["rt", "fs", "io-util", "macros"]

[dependencies.bytes]
version = "^"
optional = true

[dependencies.dashmap]
version = "^"
optional = true

[dependencies.object_store]
version = "^"
optional = true
default-features = false

[features]
default  = ["io"]
serde    = ["dep:serde"]
schemars = ["dep:schemars", "serde"]
io       = ["dep:tokio", "dep:bytes", "dep:dashmap", "dep:object_store"]
io-aws   = ["io", "object_store/aws"]
```

**Runtime dependency posture.** Under default features, `semstrait-common` pulls `tokio`, `bytes`, `dashmap`, and `object_store` (with `Local` + `InMemory` back-ends; no cloud SDKs unless `io-aws` is enabled). Under `--no-default-features`, only `thiserror` remains.

**`object_store` as internal detail.** Consumers never see `object_store::ObjectStore`, `object_store::Path`, or any of its error types on a public signature. Documented escape hatch: `S3SourceBuilder::with_object_store_builder`. See `[31b §1.4](31b_semstrait_common_io.md)`.

**No engine-identity dependencies.** No `datafusion`, no `arrow`, no `spark-*`, no `duckdb`, no `substrait`. These live in `semstrait-adapter` and its per-engine modules.

### 9.2 Internal (workspace) dependencies

**Zero.** `semstrait-common` is the root of the workspace DAG per I7. Every other crate depends on `semstrait-common` directly or transitively; `semstrait-common` depends on no workspace crate. Attempting to add a `semstrait-*` dependency to `Cargo.toml` is a compile error in CI.

## 10. Invariants Upheld by the Crate

Concrete crate-level guarantees mapping to `[00 §9](../00_overview.md)` invariants. The canonical-type invariants (I1, I2, I5, I10) anchor in `semstrait-ir` per `[35](35_semstrait_ir.md)` because the canonical type vocabulary lives there.

| Invariant | `semstrait-common` guarantee |
|---|---|
| **I7** — strict DAG | `Cargo.toml` contains zero `semstrait-*` entries in `[dependencies]`. A CI check greps the manifest and fails on any workspace-internal entry. |
| **I11** — no downward I/O surprises | Transport primitives (`io::Source`, `io::Sink`, `io::Location`, `io::backends::{memory, local, s3}`) live here under the `io` feature flag (ratified in `[31b](31b_semstrait_common_io.md)`). Domain-specific load / dump (`load_model`, `load_manifest`) do not — they live in the crate that owns the typed artifact. `reqwest`, `hyper`, raw `std::net` sockets remain rejected; cloud SDKs (`aws-sdk-s3`) sit behind opt-in `io-aws`. The dependency audit (§9.1) is enforced in CI via `cargo deny`. |
| **I12** — first-class diagnostics | `Diagnostic<K>` and `Diagnose` are the workspace's diagnostic primitives. `Diagnostic<K>` carries `kind: K, severity, location, notes`; the kind decides per-variant rendering and severity defaults via `Diagnose`. No central error-code allocation; stable identification is variant identity. The parallel observability channel (`tracing`) is described in `[30 §6](../foundations/30_stability_diagnostics.md)`; library code never writes to stdout / stderr. `IoError` per `[31b §6](31b_semstrait_common_io.md)` is its own kind enum implementing `Diagnose`. |

