---
prereqs: [00]
authoritative-for:
  - cross-cutting semver posture for every `semstrait-*` crate
  - public vs internal surface policy (what `pub` means workspace-wide)
  - `#[non_exhaustive]`-by-default policy; the concrete non-exhaustive type roster
  - generic `Diagnostic<K>` envelope, `Severity`, `Location`, `Span` canonical shapes (authoritative)
  - per-stage typed-kind discipline (no central error-code allocation; variant identity is the stable contract)
  - per-stage error-emission policy (accumulating vs fail-fast) at API boundaries
  - workspace observability policy (`tracing` adoption, no stdout/stderr from library code, canonical field names)
  - public trait surface rules (sealed-pattern vs open; invariants each trait documents)
  - per-crate sync/async posture (refines I6 / I11 at crate granularity)
  - feature-flag policy (adapter / catalog crates as separate crates, not features)
  - breaking-change governance and deprecation policy
  - per-crate stability tier assignment
refined-by:
  - 31 (`semstrait-common` — `Diagnostic<K>` / `Severity` / `Location` placement, `Diagnose` trait, narrow core kind enums)
  - 32 (`semstrait-model` — `ParseErrorKind`, `ValidateError`, `SourceId` constructors)
  - 33 (`semstrait-manifest` — `CompileError`, `RepositoryErrorKind`, `SemanticManifest` struct `#[non_exhaustive]` roster)
  - 34 (`semstrait-planner` — `PlanErrorKind`, `OptimizeErrorKind`, Constraint / adapter-injection hook surface)
  - 35 (`semstrait-ir` — `SemanticPlan`, `PlanNode`, `EngineArtifact` non-exhaustive roster)
  - 36 (`semstrait-adapter` — `AdaptErrorKind`, `EngineAdapter` sealed-vs-open, `DialectId`)
  - 37 (`semstrait-catalog` — `CatalogProvider`, `FileSystem` trait surfaces, `CatalogProviderErrorKind`, `FileSystemErrorKind`, async posture)
  - 38 (`semstrait-api` — unified entry point, `SemStraitErrorKind` sum, warning propagation)
  - 39 (`semstrait-facade` — one-shot use, re-export policy)
  - 41 (`implementation/41_deprecations.md` — deprecation lifecycle tracking)
  - 42 (`implementation/42_migration_notes.md` — MAJOR migration notes)
---

# 30. API Contracts

> **Status:** ratified. The cross-cutting policies in §§2–12 bind every `3x` per-crate doc; the stability table (§13) fixes v1 maturity markers; the non-exhaustive type roster (§4.2), the generic `Diagnostic<K>` envelope (§5), the workspace observability policy (§6), and the per-stage return-shape table (§7) are authoritative. Open reconciliation items are parked in `questions/open/30_questions.md`.
>
> **Ratification delta from earlier drafts.** §6 ("Stable Error-Code Format") is **retired**: the `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` allocation table, reserved-prefix list, and per-subsystem ranges are all gone. Diagnostic identification is now by **per-stage typed-kind variant identity**. The §6 slot now hosts the **observability policy** — a tracing-side companion to the diagnostics-side of §5. `Severity` is reduced from `{Info, Warning, Error}` to `{Error, Warning}`; the `Info` channel moved to `tracing::info!`. Every public stage entry-point that returns errors does so via the typed shapes of §7 — no `Diagnostic` without a `K` parameter, no `Vec<Diagnostic>` of mixed kinds.

## 1. Purpose and Scope

`30` opens the `3x` per-crate API-contract series by fixing the policies every crate's public surface must satisfy. It is the cross-cutting contract each of `31`–`39` refines for its own types, traits, and functions.

**What `30` ratifies:**

- The **semver posture** for the workspace (§2) — what MAJOR / MINOR / PATCH mean when every `semstrait-*` crate ships on the same release cadence.
- The **public-vs-internal** discipline (§3) — minimally public by default, every `pub` type documented, `pub(crate)` for cross-module.
- The **`#[non_exhaustive]`-by-default** policy (§4) and the concrete roster of types this binds in v1.
- The **generic `Diagnostic<K>` envelope** (§5) — generic struct, severity enum, location types, the `Diagnose` trait — authoritative here; other docs refine but do not redefine. The per-stage typed-kind discipline (no central error-code allocation; variant identity is the stable contract).
- The **observability policy** (§6) — `tracing` adoption, no stdout/stderr from library code, canonical field-name vocabulary, Diagnostic ↔ tracing orthogonality.
- The **per-stage error-emission policy** at API boundaries (§7) — mirrors `10 §5` at the crate-surface level.
- The **public-trait rules** (§8) — sealed-pattern vs open, per-trait invariant documentation obligations.
- The **per-crate async posture** (§9) — the crate-granularity refinement of I6 / I11.
- The **feature-flag policy** (§10) — adapter and catalog crates as separate crates, not features.
- **Breaking-change governance** (§11) and the **deprecation lifecycle** (§12).
- The v1 **stability table** (§13) — per-crate maturity marker.
- The **Ratified Decisions Index** (§14) — a bullet roll-up of every ratification in this doc.

**What `30` does NOT ratify** (forward-refs):

- Per-crate type rosters, exact function signatures, and per-stage kind-variant fanouts — `31`–`39`.
- Migration notes for any MAJOR transition — `42`.
- Deprecation tombstones for retired symbols — `41`.
- Per-engine adapter specifics, dialect quirks — `36`.
- Per-catalog-provider specifics, schema-drift mechanics — `37`.

**Key invariants from `00` that `30` directly upholds:**

- **I7** — the crate DAG is strict and acyclic. `30` codifies this at the public-surface level: no crate may depend upward, and every `pub use` re-export is documented per `31`–`39`.
- **I10** — extensibility. `30 §4` is where the non-exhaustive policy stops being an invariant footnote and becomes a concrete roster.
- **I11** — gated I/O. `30 §9`'s per-crate async posture is the crate-level refinement of I11's "only compile-time and two out-of-band entries" rule.
- **I12** — first-class diagnostics. `30 §5` defines the generic `Diagnostic<K>` envelope and the per-stage typed-kind discipline; `30 §6` defines the parallel tracing channel and the no-stdout invariant. Together they constitute the workspace's diagnostic-and-observability contract.

## 2. Semver Posture

Every `semstrait-*` crate in this workspace participates in a **coordinated release**: the workspace ships a single version number, and every published crate bumps to that version in lock-step. No crate publishes independently of its peers.

This matters because I7's strict DAG means inter-crate API changes can only flow in one direction (upward through the layering), and a coordinated release eliminates the combinatorial surface of version-compatibility across crates.

### 2.1 What MAJOR / MINOR / PATCH mean

| Bump | Triggers |
|---|---|
| **MAJOR** | Any non-additive change to a `pub` type, function, trait, or trait method. Removing or renaming a variant of a per-stage kind enum, adding a required field to a non-`#[non_exhaustive]` struct, changing a function signature (other than relaxing bounds), changing `Diagnostic<K>` shape, changing the per-stage classification (accumulating ↔ fail-fast), changing the canonical `tracing` field-name vocabulary in a non-additive way. Every MAJOR requires a corresponding entry in `implementation/42_migration_notes.md`. |
| **MINOR** | Additive-only changes. Adding a variant to a `#[non_exhaustive]` sum type, adding a field to a `#[non_exhaustive]` struct, adding a new `FunctionSpec` to the registry, adding a new `pub fn` or `pub struct`, adding a default-impl method to a public trait (with a default), widening a method's accepted input type (e.g. `&str` → `impl AsRef<str>`), introducing a new `#[deprecated]` symbol. Warnings are additive. |
| **PATCH** | Bug fixes that preserve observable behavior. Doc-comment corrections, internal algorithm improvements that produce identical `SemanticManifest`s / `SemanticPlan`s / `EngineArtifact`s, dependency bumps that do not change public types. |

### 2.2 What additivity means for specific changes

Concrete cases that arise repeatedly in the `3x` docs:

- **Adding a new variant to a `#[non_exhaustive]` enum** is MINOR. This is the whole point of the I10 roster.
- **Adding a new field to a `#[non_exhaustive]` struct** is MINOR. Consumers must use construction forms that tolerate new fields (`..Default::default()` or dedicated builders).
- **Widening an error-enum with a new variant** is MINOR, provided the enum is `#[non_exhaustive]` (which per §4.2 all `pub` error enums are).
- **Renaming or removing a per-stage kind variant** is MAJOR. Adding `#[deprecated]` to a variant is MINOR; the variant continues to be produced for at least one MINOR cycle before removal (§12).
- **Adding a new `Diagnostic` severity** is MINOR (`Severity` is `#[non_exhaustive]` per §4.2). Consumers pattern-matching must already have a wildcard arm.
- **Adding a new `EngineArtifact` variant** is MINOR. Adapters that consumed a specific variant via match need a wildcard arm per I10.
- **Widening a public trait's method set** is MAJOR unless the new method carries a default body. Default-bodied methods are MINOR.
- **Narrowing a trait bound on a generic parameter** is MINOR. Widening it is MAJOR.

### 2.3 Pre-1.0

All crates in this workspace are pre-1.0 until the design docs clear the `31`–`39` map and a synchronized v1.0 release is cut. Pre-1.0 semver rules apply: MINOR bumps may carry breaking changes, and every MINOR merits the same migration-note discipline MAJOR does. The v1.0 cut is the moment the stability tiers of §13 lock.

## 3. Public vs Internal Surface

### 3.1 Default stance: minimally public

Every crate starts every type, function, and trait as `pub(crate)`. A symbol is promoted to `pub` only when:

1. An external consumer has a documented need for it (another workspace crate or an end user going through `semstrait-facade`).
2. The symbol's invariants are captured in its doc comment.
3. The symbol is included in the crate's `3x` contract doc.

Symbols that satisfy (1) and (2) but are exposed only for macro-expansion or generic signature inference carry `#[doc(hidden)]` and are excluded from the `3x` surface discussion.

### 3.2 Documentation obligation

Every `pub` type, function, trait, trait method, and associated item MUST carry a doc comment. The doc comment for a public type states:

- What the type represents (the vocabulary anchor from `00 §4` where applicable).
- Its invariants that a consumer must uphold (or that the constructor upholds).
- Its `#[non_exhaustive]` status if applicable (§4).
- A pointer to the authoritative `3x` doc when the type's contract is ratified there.

Traits additionally document their sealed status, their consumer crate, their method invariants, and any blanket `impl`s they own (§8).

`cargo doc` warning-level is maintained at `missing_docs` for every `semstrait-*` crate. A missing doc comment fails CI.

### 3.3 `pub(crate)` vs `pub(super)` vs `pub`

- `pub` — consumed by another crate or by `semstrait-facade` end users. Listed in the crate's `3x` contract.
- `pub(crate)` — cross-module within the owning crate. Not on the `3x` surface.
- `pub(super)` — visibility within a module tree. Internal convenience only; never appears on a `3x` surface.

### 3.4 Re-export policy

`semstrait-facade` re-exports the minimum set users need to invoke the `parse → … → adapt` pipeline from a single entry. `semstrait-api` re-exports the mid-level surface (callers who want to pick their adapter / catalog but use a bundled pipeline). Other crates do not re-export each other's public symbols by default; a deliberate re-export is a `3x` decision, not a `30` policy.

Every re-export carries the same `#[non_exhaustive]` annotation as the underlying type — re-exports do not relax extensibility.

## 4. Non-Exhaustive-By-Default Policy

I10 binds: every public sum type that models a classification with future extensions is `#[non_exhaustive]`. `30` extends this to the matching discipline on public structs: every public struct whose field set may grow is `#[non_exhaustive]`.

### 4.1 Sum types — MUST be non-exhaustive

The v1 roster, pulled from `00 §9 I10` and extended by the error-enum families ratified here and in `10 §5`:

- **Canonical domain enums.** `DataType`, `DataKind`, `Additivity`, `Cardinality`, `JoinType`, `DialectId`, `EngineArtifact`, `EnginePlan`, `ExprSource` variants (`Inline`, `Declarative`), `TemporalShape` and its `Scd` subtype enum, the composition-kind tag of `ComposedSemanticInterface`, `DimensionType`, `Grain`, `LiteralValue`, `BinaryOpKind`, `Aggregation`, `FunctionCategory`, `ParamType`, `ReturnTypeRule`, `Portability`.
- **Diagnostic-surface enums.** `Severity` (§5.2). `SourceId` is opaque (no `#[non_exhaustive]` annotation needed — its variant set is private to its producing crate; see §5.3).
- **Per-stage kind enums.** Every per-stage typed-kind enum: `ParseErrorKind`, `ValidateError`, `CompileError`, `PlanErrorKind`, `OptimizeErrorKind`, `AdaptErrorKind`, `CatalogProviderErrorKind`, `FileSystemErrorKind`, `RepositoryErrorKind`, `SemStraitErrorKind`. New variant addition is the whole point — kind enums grow as new conditions surface.

**Special case: `CanonicalFn`.** `CanonicalFn` is a newtype `struct CanonicalFn(&'static str)` with `pub const` identities (`CanonicalFn::UPPER`, `CanonicalFn::LOWER`, …) per `00 §4.1` and `14a §2`. It is **inherently extensible** — a new adapter-contributed constant does not change the type's shape. No `#[non_exhaustive]` is needed because there is no `enum` to annotate; extensibility is structural. Matching semantics use the `pub const` identities directly.

### 4.2 Structs — MUST be non-exhaustive when MAY-grow

Public structs whose field set may grow in MINOR are annotated `#[non_exhaustive]`:

- `Diagnostic<K>` (§5.1) — `notes`, future cross-reference fields may grow.
- `Location` (§5.3) — location metadata may gain fields (e.g., file-relative line/column).
- `FunctionSpec` (ratified in `14a §3.1` — already `#[non_exhaustive]`).
- `SemanticManifest` and its `Resolved*` family (`ResolvedDataKind`, `ResolvedSource`, `ResolvedColumnMapping`, `ResolvedExprTable`). The `33` doc fixes the exact roster; every public leaf is `#[non_exhaustive]`.
- `SemanticPlan` and `PlanNode` sub-structs — indices and metadata may grow (`35`).
- `SemanticInterface`, `ComposedSemanticInterface` — fields grow as composition semantics sharpen (`16`).
- `Request`, `SessionContext` — session state evolves (`34`).
- `SqlArtifact`, `EngineAdapter` method return types (`36`).

**Retired.** `ContextLine` (was a public struct in earlier drafts) is gone — see §5.3.

### 4.3 Internal-only enums MAY be exhaustive

Crate-private (`pub(crate)` or narrower) enums used on hot paths — e.g. a planner-internal strategy discriminator, a substitution-state flag — are free to be exhaustive for efficient match exhaustiveness checks. Exhaustiveness is a compile-time gift the matcher should not give up when nobody outside the crate sees the type.

A symbol promoted from `pub(crate)` to `pub` gains `#[non_exhaustive]` at the same PR; treating that as a minor refactor is a bug.

### 4.4 Match discipline for non-exhaustive types

Consumers of `#[non_exhaustive]` types must always include a wildcard arm. `30` carries one cross-cutting consumer rule: **a wildcard arm that returns a `Diagnostic<K>` whose kind variant carries enough information to debug the path is always acceptable; a wildcard arm that `panic!`s or `unreachable!()`s in a library is a bug.** End-user applications are free to panic; `semstrait-*` library code never does.

## 5. Diagnostic Structure

The diagnostic surface is **typed** and **per-stage**. Every consumer crate declares its own kind enum (`ParseErrorKind`, `ValidateError`, `CompileError`, …) implementing the `Diagnose` trait; the generic `Diagnostic<K>` envelope wraps a kind with severity and source location. Public entry points that can fail return either `Result<…, Diagnostic<K>>` (fail-fast) or `Result<…, Diagnostics<K>>` (accumulating). Raw `String` errors, `anyhow::Error`, and `Box<dyn Error>` are banned on public APIs (§5.5).

There is no central error-code allocation table. Stable identification is by **variant identity** of the per-stage kind — renaming a variant is MAJOR per §2; adding one inside a `#[non_exhaustive]` enum is MINOR. The prior code-allocation framework (§6 of earlier drafts) is retired; §6 now hosts the workspace observability policy.

### 5.1 Generic `Diagnostic<K>` envelope

```rust
#[non_exhaustive]
pub struct Diagnostic<K: Diagnose> {
    /// The typed payload describing what failed. Each crate declares
    /// its own `K`; consumers `match` on variants for compile-time
    /// exhaustiveness over the kind enum.
    pub kind: K,

    /// Severity class; see §5.2.
    pub severity: Severity,

    /// Optional source-level location. `None` for context-free errors
    /// (e.g. a `CatalogUnavailable` that originates from a network
    /// failure with no document anchor).
    pub location: Option<Location>,

    /// Optional supplementary remarks attached to the primary kind —
    /// short free-text strings ("declared here at line 42", "see also
    /// the alias entry at …") that callers render alongside the primary
    /// message. Not recursive; not a structured cross-reference.
    pub notes: Vec<String>,
}

pub type Diagnostics<K> = Vec<Diagnostic<K>>;
```

`Diagnostic<K>` lives in `semstrait-common` (ratified in `31 §7`). The `#[non_exhaustive]` annotation permits adding fields (e.g. a `related: Vec<Location>` cross-reference) in a MINOR release. `Diagnostics<K>` is a transparent type alias — callers may use either form interchangeably.

Construction sites are crate-local; callers do not construct `Diagnostic<K>` by hand. Each per-stage helper in the owning crate builds a kind, sets severity (defaulting to `K::severity_default()`), attaches optional location / notes, and wraps.

### 5.2 `Severity`

```rust
#[non_exhaustive]
pub enum Severity {
    /// Fatal. In fail-fast stages, raises Err immediately. In
    /// accumulating stages, contributes to the final Err vector.
    Error,

    /// Advisory. Returned alongside Output on the success arm of
    /// either stage class. MUST NOT be silently dropped at API
    /// boundaries.
    Warning,
}
```

Severity carries **message intent only**. Control flow (accumulating vs fail-fast) is a property of the function's return signature (§7), not the diagnostic. `#[non_exhaustive]` leaves room for future gradations (e.g. `Note`) without breaking matcher code. The earlier `Info` variant retired in this ratification; informational signals belong on the `tracing` channel (§6), not the diagnostic channel.

### 5.3 `Location`, `Span`, `SourceId`

```rust
/// Source-level location.
#[non_exhaustive]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}

/// Half-open byte range into the source document.
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Opaque source identifier.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SourceId(/* crate-private */);
```

`Location`, `Span`, `SourceId` live in `semstrait-common`. `SourceId` is opaque — its variant set is private; constructors live on the producing crate (`semstrait-model` for YAML-file sources, etc.). Public surface exposes `SourceId::unknown()`, `as_str()`, plus `Eq` / `Hash` / `Display`.

**Retired.** `ContextLine` is retired. The earlier "supplementary line with annotated pointer" role is covered either by (a) `notes: Vec<String>` on `Diagnostic<K>` for short remarks, or (b) richer typed location information embedded directly in the kind variant when structurally meaningful (e.g., a `ShapeFieldConflict` variant carrying `occurrences: Vec<Location>`).

### 5.4 `Diagnose` trait

Each per-stage kind implements:

```rust
pub trait Diagnose {
    /// Human-readable rendering. Powers the Diagnostic's Display impl;
    /// must not include line breaks (callers add their own framing).
    fn message(&self) -> String;

    /// Default severity for this variant. Construction sites may
    /// override; most callers accept the default.
    fn severity_default(&self) -> Severity;

    /// Foreign-error chain for std::error::Error interop. Default None.
    /// Variants wrapping foreign errors (e.g. ParseErrorKind::Yaml(serde_yaml::Error))
    /// override this to return Some(&inner).
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
```

`semstrait-common` provides blanket impls:

```rust
impl<K: Diagnose> std::fmt::Display for Diagnostic<K> { /* delegates to K::message() */ }
impl<K: Diagnose + std::fmt::Debug> std::error::Error for Diagnostic<K> { /* chains via K::cause() */ }
```

Foreign-error wrapping is **variant-side**: a per-stage kind enum has typed variants for the foreign errors it actually wraps (`ParseErrorKind::Yaml(serde_yaml::Error)`, `FileSystemErrorKind::Io(std::io::Error)`); the wrapped error participates in the `std::error::Error` chain via the `cause()` override. There is no struct-side `source: Box<dyn Error>` field on `Diagnostic<K>` — the kind is the source of truth.

### 5.5 Banned patterns

The following patterns are forbidden on any `pub` entry point in the workspace:

- Returning `Result<T, String>`, `Result<T, anyhow::Error>`, `Result<T, Box<dyn Error>>`, or any untyped error.
- Returning `Result<T, Diagnostic>` without the type parameter — every `Diagnostic` is `Diagnostic<K>` for some specific `K`.
- Panicking on caller-reachable input (malformed YAML, missing catalog entries, unsupported engine features). A panic is a semstrait bug.
- Writing diagnostics directly to `stdout` / `stderr`. Diagnostics flow through `Result`; the embedder decides where to render them. See §6 for the parallel observability channel.

Internal-only APIs (`pub(crate)`) may use kind enums directly (without the `Diagnostic<K>` envelope) where construction-site location is implicit.

### 5.6 Cross-stage aggregation

When multiple stage kinds need to surface through a single helper (e.g. `compile_and_plan_and_adapt` in `semstrait-api`), the fused helper declares a sum-typed kind:

```rust
#[non_exhaustive]
pub enum SemStraitErrorKind {
    Parse(semstrait_model::ParseErrorKind),
    Validate(semstrait_model::ValidateError),
    Compile(semstrait_manifest::CompileError),
    Plan(semstrait_planner::PlanErrorKind),
    Adapt(semstrait_adapter::AdaptErrorKind),
}

impl From<semstrait_model::ParseErrorKind>     for SemStraitErrorKind { /* … */ }
impl From<semstrait_model::ValidateError>  for SemStraitErrorKind { /* … */ }
impl From<semstrait_manifest::CompileError> for SemStraitErrorKind { /* … */ }
// ...
```

The fused helper returns `Diagnostic<SemStraitErrorKind>` (or `Diagnostics<…>`); per-stage results lift via `From`. Cross-crate kind-nesting is permitted but not mandatory — a stage's kind enum MAY embed an upstream stage's kind directly (e.g. `CompileError::Parse(ParseErrorKind)`) when convenient; the fused sum at `38` is the canonical location for cross-stage aggregation.

## 6. Observability and Tracing

> **§6 was previously "Stable Error-Code Format" — retired.** The numeric `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` allocation table, reserved-prefix list, and per-subsystem ranges are gone. Diagnostic identification is now by **per-stage typed-kind variant identity** per §5; rename = MAJOR, additive variant inside `#[non_exhaustive]` = MINOR. This slot now hosts the workspace observability policy, the parallel channel to the `Result`-borne diagnostics of §5.

### 6.1 No direct stdout / stderr from library code

No `semstrait-*` library crate writes to `stdout` or `stderr`. There is no `println!`, `eprintln!`, `print!`, `eprint!`, or any equivalent on any library code path. Diagnostics returned via `Result` are the output channel for caller-actionable findings. An embedder (the eventual `semstrait-cli` binary, an LSP server, a web service, a test harness) configures its own destination for any visible text.

This is a workspace-level invariant, enforced by review and (optionally) by a `clippy` lint configuration that bans `print_stdout` / `print_stderr` for `semstrait-*` crates.

### 6.2 `tracing` is the observability channel

Every `semstrait-*` library crate uses [`tracing`](https://docs.rs/tracing) to emit structured spans and events. No other observability dependency is permitted (no `log`, no custom event channel, no embedder-coupled logger).

Every `pub` stage entry-point carries `#[tracing::instrument]`:

```rust
#[tracing::instrument(level = "info", skip(model), fields(stage = "compile"))]
pub fn compile(model: &SemanticModel) -> Result<…> { /* … */ }
```

The macro propagates spans across `.await` automatically (relevant for `compile` and other compile-time async boundaries — no manual `Instrument` wrapping required). Internal helpers below the public boundary inherit the parent span and need no instrumentation of their own unless an author wants finer-grained sub-spans.

### 6.3 Level discipline

| Level | Purpose | Examples |
|---|---|---|
| `error` | Library refused to proceed and is bubbling | reserved (most errors are returned via `Result`; `error!` is for the "this is an internal bug" class) |
| `warn` | Library-internal advisory not surfaced as a Diagnostic | retrying a transient I/O failure; falling back to a default config value; deprecation usage at boundary |
| `info` | "What is happening" signal a CLI user might want | "parsed 200 entities", "resolved 14 sources (8 cached, 6 fetched)", "compile completed in 240ms" |
| `debug` | Developer-targeting state visibility | scope graph constructed (N nodes, M edges); chosen adapter `Snowflake@v6` |
| `trace` | Deep inspection | every expression normalisation step; every binding resolution |

`tracing` short-circuits at the macro site when no subscriber filters the level in; off-by-default levels (`debug`, `trace`) carry near-zero overhead in production builds.

### 6.4 Diagnostic ↔ tracing orthogonality

A `Diagnostic<K>` returned via `Result` is **never also** emitted via `tracing::warn!` (or any other level) by the library itself. The two channels carry orthogonal information:

- A `Diagnostic` surfaces something *about the user's model* — the LSP / IDE / CLI renders it on the source. Caller MUST handle it; it is part of the API contract.
- A `tracing` event surfaces something *about library behavior* — operators / developers see it in logs. Caller MAY observe it; it is not part of the API contract.

Embedders that want diagnostics in their tracing stream convert in their own code:

```rust
for diag in diagnostics {
    match diag.severity {
        Severity::Error   => tracing::error!(?diag, "diagnostic"),
        Severity::Warning => tracing::warn!(?diag, "diagnostic"),
    }
}
```

Library-internal warnings (a fallback applied, a transient retry, a deprecated entry-point used) flow only through `tracing::warn!`. They are not `Diagnostic`s — they describe library behavior, not the user's model.

### 6.5 Canonical field-name vocabulary

Subscribers can rely on these field names being used consistently across every stage span / event in the workspace. When an author emits an event with information matching one of these, the canonical field name MUST be used; additional non-canonical fields MAY be added freely.

| Field | Type | Semantics |
|---|---|---|
| `stage` | `&'static str` | One of `parse`, `validate`, `compile`, `plan`, `optimize`, `adapt`, `catalog.resolve`, `fs.read`, `adapter.adapt`, `repository.load`, `repository.save`. Set on every stage span. |
| `entities` | `usize` | Entity count for any stage iterating over the model entity set. |
| `sources` | `usize` | Source-document or catalog-source count. |
| `duration_ms` | `u64` | Stage wall-time in milliseconds. Emitted on the closing `info` event of long stages. |
| `engine` | `&'static str` | Adapter target identifier (`snowflake`, `bigquery`, `duckdb`, …). |
| `adapter` | `&'static str` | Adapter crate identifier (`semstrait-adapter-snowflake`, …). |
| `kind` | `&'static str` | DataKind variant name when relevant (`Dataset`, `Grainset`, `Unionset`, `Joinset`). |

Canonical field names extend MINOR-additively: a new reserved field name lands with a MINOR bump and a `42_migration_notes.md` entry.

### 6.6 Subscriber configuration is the embedder's responsibility

`semstrait-*` library crates do not install a global `tracing` subscriber and do not declare a transitive dependency on a subscriber crate (`tracing-subscriber`, `tracing-bunyan-formatter`, etc.). Embedders configure their own.

The eventual `semstrait-cli` binary will configure a `tracing-subscriber::fmt` layer with verbosity controlled by `--info` / `--debug` / `--trace` flags; the API spec for that binary is out of scope for `30`. Any other embedder (LSP server, web service, test harness) is free to choose its own subscriber stack.

### 6.7 Async span propagation

Stages that run `async fn` bodies use `#[tracing::instrument]` on their public entry point. The macro hooks `.await` to keep spans entered/exited correctly across suspension points. No manual span management is required at the crate-author level. If a crate adds a manually-opened span around an `async` block, it MUST use `Instrument::instrument` / `.in_scope` correctly per the `tracing` documentation.

### 6.8 Banned patterns on the observability channel

- **`tracing::error!` at level `error` for caller-actionable conditions.** Caller-actionable failures flow through `Result<…, Diagnostic<K>>`. `error!` is reserved for bugs — conditions a `Diagnostic` would never describe.
- **Direct `stdout` / `stderr` writes** (re-stating §6.1).
- **Logging via `log` crate.** `tracing` re-exports a `log` compatibility layer; the workspace's `Cargo.toml` does not depend on `log` directly.
- **Subscriber registration in library code.** Only embedders register subscribers.

## 7. Error-Emission Policy per Stage

The per-stage policy is ratified in `10 §5`; `30` carries it forward to the crate-public-surface level. Two patterns bind every public stage entry-point:

- **Accumulating** stages process the whole input and collect every independent finding into one vector before deciding success vs failure. Used where a single pass can find many independent errors (multi-document YAML, structural well-formedness across the whole model).
- **Fail-fast** stages stop at the first hard inconsistency and return immediately. Used where downstream work cannot meaningfully proceed past the first failure (compile, plan, adapt — sequential transformations).

Both patterns carry warnings (`Severity::Warning` diagnostics) on the success arm. Fail-fast stages additionally preserve any warnings encountered before the fatal error on the failure arm — warnings are never silently dropped (§7.3).

### 7.1 Per-stage classification and return shape

The accumulating-vs-fail-fast classification is fixed at the spec level — switching a stage between classes is a MAJOR change.

| Stage | Crate | Class | Public-API return shape |
|---|---|---|---|
| `parse` | `32` (model) | accumulating | `Result<(SemanticModel, Diagnostics<ParseErrorKind>), Diagnostics<ParseErrorKind>>` |
| `validate` | `32` (model) | accumulating | `Result<Diagnostics<ValidateError>, Diagnostics<ValidateError>>` |
| `compile` | `33` (manifest) | fail-fast | `Result<(SemanticManifest, Diagnostics<CompileError>), (Diagnostic<CompileError>, Diagnostics<CompileError>)>` |
| `plan` | `34` (planner) | fail-fast | `Result<(SemanticPlan, Diagnostics<PlanErrorKind>), (Diagnostic<PlanErrorKind>, Diagnostics<PlanErrorKind>)>` |
| `optimize` | `34` (planner) | fail-fast | `Result<(SemanticPlan, Diagnostics<OptimizeErrorKind>), (Diagnostic<OptimizeErrorKind>, Diagnostics<OptimizeErrorKind>)>` |
| `adapt` | `36` (adapter) | fail-fast | `Result<(EngineArtifact, Diagnostics<AdaptErrorKind>), (Diagnostic<AdaptErrorKind>, Diagnostics<AdaptErrorKind>)>` |
| `CatalogProvider::resolve` | `37` (catalog) | fail-fast | `Result<(ResolvedSource, Diagnostics<CatalogProviderErrorKind>), (Diagnostic<CatalogProviderErrorKind>, Diagnostics<CatalogProviderErrorKind>)>` |
| `FileSystem::read` | `37` (catalog) | fail-fast | `Result<(Bytes, Diagnostics<FileSystemErrorKind>), (Diagnostic<FileSystemErrorKind>, Diagnostics<FileSystemErrorKind>)>` |
| `Repository::{load,save}` | `33` (manifest) | fail-fast | analogous to `compile`, with `RepositoryErrorKind` |
| Fused helpers (`compile_and_plan_and_adapt`, …) | `38` (api) | fail-fast | as their owning stages, with `SemStraitErrorKind` (§5.6) as the kind |

**`validate` special case.** Because `validate` produces no value on success, the success arm carries only the warnings vector (no tuple wrapper). This is the one exception to the uniform `(Output, Diagnostics<K>)` Ok shape.

### 7.2 Pattern shapes — stated abstractly

```rust
// Accumulating stage:
//   Ok  = (Output, Diagnostics<K>)   — Output + warnings collected during the pass
//   Err = Diagnostics<K>             — every diagnostic collected (errors + any warnings)
fn accumulating_stage(input: Input)
    -> Result<(Output, Diagnostics<K>), Diagnostics<K>>;

// Fail-fast stage:
//   Ok  = (Output, Diagnostics<K>)   — Output + warnings emitted before completion
//   Err = (Diagnostic<K>, Diagnostics<K>)
//                                    — the fatal diagnostic + any warnings preceding it
fn fail_fast_stage(input: Input)
    -> Result<(Output, Diagnostics<K>), (Diagnostic<K>, Diagnostics<K>)>;
```

The `Diagnostics<K>` warnings vector on the success arm contains **only `Severity::Warning`** diagnostics (errors would have routed the call to the Err arm). The `Diagnostics<K>` on an accumulating stage's Err arm is **mixed** (errors + warnings). The `Diagnostics<K>` on a fail-fast stage's Err-tuple second element is **warnings only** (the single fatal error lives in the tuple's first element).

### 7.3 Warnings are never silently dropped

Both arms of every stage signature carry warnings. A library implementation that accumulates warnings internally and discards them when raising an Err is a bug. Conversion helpers (`?` propagation, `From` impls) MUST preserve the warnings vector across stage boundaries.

### 7.4 Stage-ownership of kinds

A diagnostic's stage of origin is determined by the kind type (`ParseErrorKind`, `CompileError`, …), not by a substring of a code. Cross-crate kind-nesting is permitted (`CompileError::Parse(ParseErrorKind)` when compile internally re-parses a fragment); see §5.6 for the canonical aggregation pattern at fused helpers.

### 7.5 Accumulation limits

`parse` and `validate` accumulate without an intrinsic limit at the `30` level — every independent error is reported. In practice, large malformed models can produce hundreds of errors; per-crate docs (`32`) MAY introduce a soft cap with a sentinel kind variant ("further errors suppressed") to protect caller UX. No cap is imposed at the `30` level.

### 7.6 Panic-freedom

Public entry points never panic on caller-reachable input. Internal `unreachable!()` / `panic!("invariant ...")` calls are permitted only where the invariant is genuinely impossible to violate without a semstrait bug (e.g. a `SemanticManifest` field the compile pass is sworn to populate). A caller-reachable panic is a semstrait bug and is fixed as such.

## 8. Public Trait Surface Rules

Every public trait in the `semstrait-*` workspace documents:

1. **Its consumer crate** — the crate that calls the trait's methods.
2. **Its implementation crate(s)** — the crate(s) that provide `impl`s.
3. **Its method invariants** — what callers guarantee on input, what implementers guarantee on output.
4. **Its sealed/open status** — whether third-party impls are permitted.
5. **Its blanket impls** — which generic `impl` blocks ship with the trait.

### 8.1 Sealed vs open

A **sealed** public trait restricts implementations to the defining workspace. Sealing uses a private super-trait pattern:

```rust
mod sealed {
    pub trait Sealed {}
}

pub trait FooExt: sealed::Sealed {
    fn foo(&self) -> Result<Bar, Diagnostic>;
}

impl sealed::Sealed for LocalType {}
impl FooExt for LocalType { ... }
```

Sealed traits are used where semstrait must control the impl set for correctness (e.g. where invariants cross the trait boundary and a buggy external impl could violate I4 / I5 / I8).

An **open** public trait is implementable by any crate. Used where semstrait benefits from third-party extension and can tolerate misbehaving impls as caller errors.

### 8.2 Cross-cutting trait roster

| Trait | Consumer | Impl(s) | Sealed? | `3x` doc |
|---|---|---|---|---|
| `CatalogProvider` | `semstrait-manifest` (compile), `semstrait-api` (drift check) | `semstrait-catalog-*` crates | open — third-party catalog adapters are a supported extension | `37` |
| `FileSystem` | `semstrait-manifest` (compile, glob expansion), `semstrait-catalog-*` (source reads) | local-fs impl in `semstrait-catalog`; object-store impls in per-provider crates | open | `37` |
| `Repository` | callers at the `semstrait-api` / `semstrait-facade` layer | in-memory, filesystem-backed (bundled); third-party may add | open | `33` |
| `EngineAdapter` | `semstrait-planner` (injection hooks), `semstrait-api` (terminal `adapt`) | `semstrait-adapter-*` crates | open — new engine support is a primary extension axis | `36` |
| `RegistryExtension` | `function_registry()` initializer in `semstrait-common` | `semstrait-adapter-*` crates | open (see `questions/open/30_questions.md` Q-API-009) | `36` (via `14a §7`) |
| `Diagnose` | `Display` / `Error` blanket impls on `Diagnostic<K>`; the `?` operator | each per-stage kind enum | open — enables third-party kinds to participate in the diagnostic surface | `31` |

### 8.3 Trait-method return shape

Every fallible public trait method returns one of the per-stage shapes from §7.2 — `Result<(T, Diagnostics<K>), (Diagnostic<K>, Diagnostics<K>)>` (fail-fast) or `Result<(T, Diagnostics<K>), Diagnostics<K>>` (accumulating) — with `K` being the trait-owning crate's per-trait kind enum.

Concretely:

| Trait | Method | Return shape |
|---|---|---|
| `CatalogProvider` | `resolve(...)` | fail-fast with `CatalogProviderErrorKind` |
| `FileSystem` | `read(...)`, `list(...)`, `glob(...)` | fail-fast with `FileSystemErrorKind` |
| `Repository` | `load(...)`, `save(...)` | fail-fast with `RepositoryErrorKind` |
| `EngineAdapter` | `adapt(...)` | fail-fast with `AdaptErrorKind` |

Untyped or string-payloaded `Result<T, String>` / `Result<T, Box<dyn Error>>` returns are banned per §5.5. Typed enums ARE the API; they are not internal carriers converted at a boundary.

### 8.4 Async trait methods

When a trait method is async (only `CatalogProvider`, `FileSystem`, `Repository` in v1 — see §9), the trait uses `async fn` directly (Rust 1.75+ async-fn-in-trait). `dyn` dispatch over the trait is provided via an adjacent object-safe facade when needed; the trait itself is not object-safe. Per-crate docs (`33`, `37`) specify the exact facade shape.

## 9. Async Posture per Crate

Per I6 (sync hot path) and I11 (gated I/O), most crates are sync-only. The exceptions are the crates that orchestrate compile-time I/O or that bridge to async metadata / storage providers.

| Crate | Sync / async | I/O allowed? | Rationale |
|---|---|---|---|
| `semstrait-common` | Sync only | No | Pure primitives (`DataType`, `Diagnostic`, `Span`, `CanonicalFn`); no I/O surface. |
| `semstrait-model` | Sync only | No | `parse` and `validate` are pure transformations over in-memory YAML. |
| `semstrait-manifest` | Compile-time async; plan-time sync | Compile-time via providers | The `compile` entry point is `async fn` (awaits `CatalogProvider` / `FileSystem`). The `SemanticManifest` is then consumed synchronously; no `async fn` at plan time. |
| `semstrait-planner` | Sync only | No | `plan` and `optimize` are the I6 hot path. |
| `semstrait-ir` | Sync only | No | Canonical IR types; no I/O. |
| `semstrait-adapter` | Sync only | No | `adapt` is the I6 hot path. Per-engine adapter crates inherit the posture. |
| `semstrait-catalog` | Trait surface async; impls per-provider | Yes (sole I/O home) | `CatalogProvider` and `FileSystem` are `async fn`-in-trait. Individual impls may be sync-over-async or genuinely async. |
| `semstrait-api` | Async at compile-time entry; sync at plan-time entry | Via manifest + catalog | Bundles the compile-time async path and the query-time sync path under a single crate surface. |
| `semstrait-facade` | Same as `-api` | Same | Thin re-export + one-shot-use convenience over `semstrait-api`. |

**Runtime choice.** Async surfaces are executor-agnostic. `semstrait-manifest`'s `compile` and `semstrait-catalog`'s trait methods are `async fn` but do not pin a specific runtime. Bundled impls (e.g. `FileSystemRepository`) use `tokio` by convention; the API surface accepts any executor. Per-crate docs specify the runtime dependency stance.

**No `.await` inside the hot path.** The synchronous crates above MUST NOT introduce `.await` points, even via third-party dependencies. Dependency choice is a per-crate concern but is audited against I6 at review time.

## 10. Feature-Flag Policy

### 10.1 Default features: minimum viable

Every `semstrait-*` crate ships with default features equal to the minimum set needed to use that crate's primary function. No crate gates a core type or trait behind an opt-in feature. A consumer depending on `semstrait-common` gets every public type in `semstrait-common` with no `--features` hunt.

### 10.2 Adapter crates are SEPARATE crates, not feature flags

The adapter surface is the primary extension axis of the workspace. Each engine adapter lives in its own crate:

- `semstrait-adapter-datafusion`
- `semstrait-adapter-duckdb`
- `semstrait-adapter-spark`
- `semstrait-adapter-substrait`
- (future) per additional target engine

Consumers add the adapter crate to their `Cargo.toml` dependency list rather than flipping a feature on a monolithic `semstrait-adapter` crate. Rationale:

- Dependency closures are surgical: a DuckDB-only consumer never compiles Spark's (heavy) transitive deps.
- The `RegistryExtension` layering of `14a §7` is one registration path per crate; feature flags would scatter `cfg(feature = "…")` across the adapter trait's implementations.
- Stability tiers (§13) can be set per-adapter-crate independently — a `Provisional` adapter does not drag a `Stable in v1` workspace label down.

### 10.3 Catalog provider crates are also separate

Same reasoning as §10.2 for `CatalogProvider` impls:

- `semstrait-catalog` (trait + minimal local-fs impl)
- `semstrait-catalog-iceberg`
- `semstrait-catalog-unity`
- (future) per additional metadata source

### 10.4 Optional serialization features

Serde support on public types is opt-in via a `serde` feature per crate. Default-off for `semstrait-common`, `semstrait-model`, `semstrait-manifest`, `semstrait-ir`. Enabled downstream (e.g. `semstrait-facade` turns it on by default for end-user convenience).

Serde support is documented per-crate in `31`–`39`. A `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]` attribute on a public type is itself part of the public surface — adding it is MINOR (no existing caller code breaks); removing it is MAJOR.

### 10.5 Banned feature patterns

- **No `nightly`-only features.** Every crate compiles on stable Rust. `no_std` is not targeted.
- **No `default = ["every-adapter"]`.** A consumer that never mentions adapters gets no adapter crate in their build.
- **No mutually exclusive features within a crate.** Features compose additively; a consumer enabling `features = ["a", "b"]` gets the union.

## 11. Breaking-Change Governance

### 11.1 The additivity escape hatch

Per §2, `#[non_exhaustive]` variant addition and `#[non_exhaustive]` struct-field addition are MINOR. This is the workspace's primary "non-breaking evolution" mechanism. A new `JoinType::Semi` variant, a new `DataType::Json` variant, a new `Additivity::Partial` category — all ship in MINOR releases with a `42_migration_notes.md` entry describing caller-visible behavior (if any) and an updated match-recommendation table.

### 11.2 Non-additive changes

Any change to a `pub` type / function / trait that is not additive requires:

1. A **MAJOR changelog entry** in the release notes, calling out the break.
2. An **`implementation/42_migration_notes.md` entry** with before-and-after examples and replacement guidance.
3. A **deprecation window** of at least one MINOR cycle where feasible (§12).
4. A **`cargo-semver-checks` lint** green-light (or a documented waiver) before publish.

### 11.3 Cross-crate breaks

When a break propagates across crates (e.g. a `semstrait-ir` variant removal forces `semstrait-adapter` impls to re-match), every affected crate's `42` entry cross-references the others. The workspace's coordinated-release (§2) ensures no version pair of crates ever exposes a mid-break state.

### 11.4 Behavior-preserving refactors

Internal refactors that preserve every observable output (same `SemanticManifest` bytes, same `SemanticPlan` tree, same `EngineArtifact` text) are PATCH. Determinism (I4) makes this bit-comparable for `SemanticManifest` and `SemanticPlan`; adapter output is compared at the `SqlArtifact::text` / `EnginePlan` serialization level. A refactor that produces equivalent but not byte-identical output is MINOR (callers comparing artifacts byte-for-byte, e.g. for content-addressable caching, see the change).

## 12. Deprecation Policy

### 12.1 `#[deprecated]` lifecycle

A symbol slated for removal passes through three states:

1. **Active** — fully supported, documented, used in examples.
2. **Deprecated** — `#[deprecated(since = "VERSION", note = "use X instead; removed in ...")]` attribute present. Still compiled and callable; callers receive a rustc deprecation warning. Lives for at least one MINOR cycle.
3. **Removed** — the symbol is deleted in a MAJOR bump. The matching `42_migration_notes.md` entry references the earlier deprecation entry.

### 12.2 `implementation/41_deprecations.md`

Every deprecation — the moment a `#[deprecated]` attribute lands — is recorded in `implementation/41_deprecations.md` with:

- The symbol's fully-qualified path.
- The `since` version.
- The suggested replacement.
- The target removal version (best estimate).

When the symbol is removed, the `41` entry moves to the relevant `42` migration-note entry; `41` retains tombstones for at least one MAJOR after removal.

### 12.3 Deprecation ≠ removal

A **deprecated** symbol (per-stage kind variant, public function, public type) is still part of the surface and remains callable / matchable; rustc emits a deprecation warning at the use-site. A **removed** symbol is gone from the public surface entirely. Removal is MAJOR; deprecation is MINOR. The deprecation window MUST cover at least one MINOR cycle.

For per-stage kind variants specifically: deprecating a variant means the library MAY continue to produce it for one MINOR cycle, then either stop producing it (replacing emissions with a successor variant) or remove it (in the next MAJOR). The replacement-variant transition is the period in which both old and new variants may surface for the same condition; callers SHOULD match the new variant first.

### 12.4 Minimum window

At least one full MINOR cycle between `#[deprecated]` and removal. Longer for widely-used symbols — per-crate `3x` docs may specify extended windows (e.g. two MINOR cycles for a core `CatalogProvider` method).

## 13. Stability Table

v1 per-crate maturity markers. These lock at the v1.0 cut and evolve per the semver rules of §2.

| Crate | Stability | Notes |
|---|---|---|
| `semstrait-common` | Stable in v1 | Non-expression shared vocabulary only after the second-cascade landing (`STATUS.md` item Q): `DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`, `Diagnostic`, `Diagnostics`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose`, constraint DSL, `io` transport. Expression-tree vocabulary moved to `semstrait-ir`. Breaking changes require a workspace-wide MAJOR. |
| `semstrait-ir` | Stable in v1 | `Expr<L>`, `PhysicalExpr`, `SemanticExpr`, `PhysicalLeaf`, `SemanticLeaf`, `Tree` / `Visitor` / `Rewriter` / `ExprLeaf` traits, structural-variant support enums (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`), `Literal`, `ColumnRef`, `SemanticsName`, `Accessor` family, `Parameter`, `CanonicalFn`, `FunctionRegistry`, narrow `ValidateError` (trait-machinery) and narrow `CompileError` (`ReturnTypeRule::Custom`), `SemanticPlan`, `PlanNode`, `EngineArtifact`, `SqlArtifact`, `EnginePlan`. Variant and field additions are MINOR via `#[non_exhaustive]`. |
| `semstrait-model` | Stable in v1 | `SemanticModel`, `ExprSource`, `ParseErrorKind`, `ValidateError` (model-level, embedding `Ir(ir::ValidateError)` per D.ii — `35 §16.1`), YAML grammar. The author-facing YAML shape extends non-exhaustively (new keys, new variants) in MINOR. |
| `semstrait-manifest` | Stable in v1 | `SemanticManifest`, `Resolved*` family, `CompileError` (embedding `Ir(ir::CompileError)` per D.ii — `35 §16.2`), `RepositoryErrorKind`, `Repository` trait. **Internal serialization format (SemanticManifest on-disk bytes) is NOT a public API** — callers round-trip through `Repository::save` / `Repository::load`, not through direct byte access. |
| `semstrait-planner` | Stable in v1 | `plan`, `optimize`, `PlanError`, `OptimizeError`. Per-DataKind strategy dispatch is internal. `PlanNode` variants are I10 `#[non_exhaustive]` and defined in `semstrait-ir`. |
| `semstrait-adapter` | Provisional | `EngineAdapter` trait stable; `AdaptError` stable; `DialectId` extends in MINOR. Per-engine adapter crates (`semstrait-adapter-datafusion`, `semstrait-adapter-duckdb`, `semstrait-adapter-spark`, `semstrait-adapter-substrait`) are **versioned independently** and may carry their own stability tier in their own `3x` appendix. |
| `semstrait-catalog` | Provisional | `CatalogProvider`, `FileSystem`, local-fs impl stable. Per-provider impls (`semstrait-catalog-iceberg`, `semstrait-catalog-unity`) are **versioned independently**; their stability follows their own maturity. |
| `semstrait-api` | Stable in v1 | Unified entry point wrapping the `parse → … → adapt` pipeline. Re-exports the minimum `semstrait-*` types required to use the pipeline end-to-end. |
| `semstrait-facade` | Stable in v1 | Facade over `semstrait-api` for one-shot use (single compile, single plan, single adapt). Default features enable the minimum useful adapter bundle; extension is via `semstrait-api`. |

**Provisional** crates may introduce non-additive changes in MINOR cycles (pre-1.0 semver rules continue for these crates past workspace-v1.0 if they are not yet promoted). Every provisional change still carries a `42` migration note.

## 15. Cross-References

- `00 §4.1` — `Diagnostic` row (authoritative-doc pointer → `30`).
- `00 §4.2` — verb catalog; `30 §7` ratifies the API-boundary return shapes.
- `00 §9` — I7 (strict DAG), I10 (non-exhaustive), I11 (gated I/O), I12 (first-class diagnostics; cross-doc reconciliation queued under foundations — `STATUS.md §2`).
- `10 §3` — per-stage contracts consumed by `30 §7`.
- `10 §5` — internal-error model; `30 §5` is the public-boundary refinement (the prior code-allocation framework retired in this doc).
- `11 §8` — Constraint framework; `ConstraintViolation` is now a `PlanErrorKind` variant rather than a numeric code.
- `14 §7` — expression-error catalog (cross-doc reconciliation queued under foundations — these become variants on `ParseErrorKind` / `ValidateError` / `CompileError`).
- `14a §3.1`, `§7`, `§8` — `FunctionSpec` `#[non_exhaustive]`, `RegistryExtension`, function-resolution kinds (queued for foundations alignment).
- `13 §6` — type-related Precondition IDs (`TG-*`) map to `ValidateError` / `CompileError` variants.
- `18 §11` — SR-E rules become `ValidateError` variants (queued for foundations alignment).
- `31`–`39` — per-crate refinements of every policy in this doc.
- `implementation/41_deprecations.md` — deprecation lifecycle tracking.
- `implementation/42_migration_notes.md` — MAJOR migration entries.
- `questions/open/30_questions.md` — parked reconciliation items.
