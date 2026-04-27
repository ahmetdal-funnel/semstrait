---
prereqs: [00]
authoritative-for:
  - cross-cutting semver posture for every `semstrait-*` crate
  - public vs internal surface policy (what `pub` means workspace-wide)
  - `#[non_exhaustive]`-by-default policy; the concrete non-exhaustive type roster
  - `Diagnostic`, `Severity`, `Span`, `ContextLine` canonical shapes (authoritative)
  - stable error-code format `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` and per-subsystem ranges
  - per-stage error-emission policy (accumulate vs fail-fast) at API boundaries
  - public trait surface rules (sealed-pattern vs open; invariants each trait documents)
  - per-crate sync/async posture (refines I6 / I11 at crate granularity)
  - feature-flag policy (adapter / catalog crates as separate crates, not features)
  - breaking-change governance and deprecation policy
  - per-crate stability tier assignment
refined-by:
  - 31 (`semstrait-core` — `Diagnostic`/`Span`/`Severity` placement, shared primitives)
  - 32 (`semstrait-model` — `ParseError`, `ValidateError`, `SourceId` variant shape)
  - 33 (`semstrait-manifest` — `CompileError`, `Manifest` struct `#[non_exhaustive]` roster, `Repository` trait)
  - 34 (`semstrait-planner` — `PlanError`, `OptimizeError`, Constraint / adapter-injection hook surface)
  - 35 (`semstrait-ir` — `SemanticPlan`, `PlanNode`, `EngineArtifact` non-exhaustive roster)
  - 36 (`semstrait-adapter` — `AdaptError`, `EngineAdapter` sealed-vs-open, `DialectId`)
  - 37 (`semstrait-catalog` — `CatalogProvider`, `FileSystem` trait surfaces, async posture)
  - 38 (`semstrait-api` — unified entry point, warning propagation)
  - 39 (`semstrait-facade` — one-shot use, re-export policy)
  - 41 (`implementation/41_deprecations.md` — deprecation lifecycle tracking)
  - 42 (`implementation/42_migration_notes.md` — MAJOR migration notes)
---

# 30. API Contracts

> **Status:** ratified. The cross-cutting policies in §§2–12 bind every `3x` per-crate doc; the stability table (§13) fixes v1 maturity markers; the non-exhaustive type roster (§4.2), the `Diagnostic` shape (§5), and the error-code number ranges (§6.6) are authoritative. Open reconciliation items are parked in `questions/open/30_questions.md`.

## 1. Purpose and Scope

`30` opens the `3x` per-crate API-contract series by fixing the policies every crate's public surface must satisfy. It is the cross-cutting contract each of `31`–`39` refines for its own types, traits, and functions.

**What `30` ratifies:**

- The **semver posture** for the workspace (§2) — what MAJOR / MINOR / PATCH mean when every `semstrait-*` crate ships on the same release cadence.
- The **public-vs-internal** discipline (§3) — minimally public by default, every `pub` type documented, `pub(crate)` for cross-module.
- The **`#[non_exhaustive]`-by-default** policy (§4) and the concrete roster of types this binds in v1.
- The **`Diagnostic`** shape (§5) — canonical struct, severity enum, span/context types — authoritative here; other docs refine but do not redefine.
- The **stable error-code format** `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` (§6), including concrete ranges reserved per subsystem and the stability promise.
- The **per-stage error-emission policy** at API boundaries (§7) — mirrors `10 §5` at the crate-surface level.
- The **public-trait rules** (§8) — sealed-pattern vs open, per-trait invariant documentation obligations.
- The **per-crate async posture** (§9) — the crate-granularity refinement of I6 / I11.
- The **feature-flag policy** (§10) — adapter and catalog crates as separate crates, not features.
- **Breaking-change governance** (§11) and the **deprecation lifecycle** (§12).
- The v1 **stability table** (§13) — per-crate maturity marker.
- The **Ratified Decisions Index** (§14) — a bullet roll-up of every ratification in this doc.

**What `30` does NOT ratify** (forward-refs):

- Per-crate type rosters, exact function signatures, and error-variant fanouts — `31`–`39`.
- Migration notes for any MAJOR transition — `42`.
- Deprecation tombstones for retired codes / symbols — `41`.
- Per-engine adapter specifics, dialect quirks — `36`.
- Per-catalog-provider specifics, schema-drift mechanics — `37`.

**Key invariants from `00` that `30` directly upholds:**

- **I7** — the crate DAG is strict and acyclic. `30` codifies this at the public-surface level: no crate may depend upward, and every `pub use` re-export is documented per `31`–`39`.
- **I10** — extensibility. `30 §4` is where the non-exhaustive policy stops being an invariant footnote and becomes a concrete roster.
- **I11** — gated I/O. `30 §9`'s per-crate async posture is the crate-level refinement of I11's "only compile-time and two out-of-band entries" rule.
- **I12** — first-class diagnostics. `30 §5`–`§6` are where `Diagnostic` stops being a vocabulary entry and becomes a public API contract with a binding struct shape and a stable code format.

## 2. Semver Posture

Every `semstrait-*` crate in this workspace participates in a **coordinated release**: the workspace ships a single version number, and every published crate bumps to that version in lock-step. No crate publishes independently of its peers.

This matters because I7's strict DAG means inter-crate API changes can only flow in one direction (upward through the layering), and a coordinated release eliminates the combinatorial surface of version-compatibility across crates.

### 2.1 What MAJOR / MINOR / PATCH mean

| Bump | Triggers |
|---|---|
| **MAJOR** | Any non-additive change to a `pub` type, function, trait, or trait method. Removing a variant, adding a required field to a non-`#[non_exhaustive]` struct, changing a function signature (other than relaxing bounds), retiring a stable error code, changing `Diagnostic` shape, changing pipeline-stage error policy. Every MAJOR requires a corresponding entry in `implementation/42_migration_notes.md`. |
| **MINOR** | Additive-only changes. Adding a variant to a `#[non_exhaustive]` sum type, adding a field to a `#[non_exhaustive]` struct, adding a new `FunctionSpec` to the registry, adding a new `pub fn` or `pub struct`, adding a default-impl method to a public trait (with a default), widening a method's accepted input type (e.g. `&str` → `impl AsRef<str>`), introducing a new `#[deprecated]` symbol. Warnings are additive. |
| **PATCH** | Bug fixes that preserve observable behavior. Doc-comment corrections, internal algorithm improvements that produce identical `Manifest`s / `SemanticPlan`s / `EngineArtifact`s, dependency bumps that do not change public types. |

### 2.2 What additivity means for specific changes

Concrete cases that arise repeatedly in the `3x` docs:

- **Adding a new variant to a `#[non_exhaustive]` enum** is MINOR. This is the whole point of the I10 roster.
- **Adding a new field to a `#[non_exhaustive]` struct** is MINOR. Consumers must use construction forms that tolerate new fields (`..Default::default()` or dedicated builders).
- **Widening an error-enum with a new variant** is MINOR, provided the enum is `#[non_exhaustive]` (which per §4.2 all `pub` error enums are).
- **Retiring an error code** — deleting the `&'static str` literal — is MAJOR. Deprecating one is MINOR (§12).
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
- **Diagnostic-surface enums.** `Severity` (§5.2), `SourceId` variants (§5.3).
- **Error enums.** Every per-stage error enum: `ParseError`, `ValidateError`, `CompileError`, `PlanError`, `OptimizeError`, `AdaptError`. New variant addition is the whole point — error enums grow as new conditions surface.

**Special case: `CanonicalFn`.** `CanonicalFn` is a newtype `struct CanonicalFn(&'static str)` with `pub const` identities (`CanonicalFn::UPPER`, `CanonicalFn::LOWER`, …) per `00 §4.1` and `14a §2`. It is **inherently extensible** — a new adapter-contributed constant does not change the type's shape. No `#[non_exhaustive]` is needed because there is no `enum` to annotate; extensibility is structural. Matching semantics use the `pub const` identities directly.

### 4.2 Structs — MUST be non-exhaustive when MAY-grow

Public structs whose field set may grow in MINOR are annotated `#[non_exhaustive]`:

- `Diagnostic` (§5.1) — `context` and source-chain details may gain fields.
- `FunctionSpec` (ratified in `14a §3.1` — already `#[non_exhaustive]`).
- `Manifest` and its `Resolved*` family (`ResolvedDataKind`, `ResolvedSource`, `ResolvedColumnMapping`, `ResolvedExprTable`). The `33` doc fixes the exact roster; every public leaf is `#[non_exhaustive]`.
- `SemanticPlan` and `PlanNode` sub-structs — indices and metadata may grow (`35`).
- `SemanticInterface`, `ComposedSemanticInterface` — fields grow as composition semantics sharpen (`16`).
- `Request`, `SessionContext` — session state evolves (`34`).
- `SqlArtifact`, `EngineAdapter` method return types (`36`).
- `Span`, `ContextLine` (§5.3) — location and context metadata may gain fields.

### 4.3 Internal-only enums MAY be exhaustive

Crate-private (`pub(crate)` or narrower) enums used on hot paths — e.g. a planner-internal strategy discriminator, a substitution-state flag — are free to be exhaustive for efficient match exhaustiveness checks. Exhaustiveness is a compile-time gift the matcher should not give up when nobody outside the crate sees the type.

A symbol promoted from `pub(crate)` to `pub` gains `#[non_exhaustive]` at the same PR; treating that as a minor refactor is a bug.

### 4.4 Match discipline for non-exhaustive types

Consumers of `#[non_exhaustive]` types must always include a wildcard arm. `30` carries one cross-cutting consumer rule: **a wildcard arm that returns a `Diagnostic` with a stable code is always acceptable; a wildcard arm that `panic!`s or `unreachable!()`s in a library is a bug.** End-user applications are free to panic; `semstrait-*` library code never does.

## 5. `Diagnostic` Structure

The `Diagnostic` is the sole public error surface across the `semstrait-*` workspace (I12). Every public entry point that can fail returns `Diagnostic` or `Vec<Diagnostic>`; raw `String` errors, `anyhow::Error`, `Box<dyn Error>` are banned on public APIs. Typed per-stage error enums (`ParseError`, `CompileError`, …) remain crate-internal carriers and convert into `Diagnostic` at the crate's public boundary via the `IntoDiagnostic` trait ratified in `10 §5.1`.

### 5.1 Canonical struct

```rust
#[non_exhaustive]
pub struct Diagnostic {
    /// Stable error code; see §6.
    pub code: &'static str,

    /// Severity class; see §5.2.
    pub severity: Severity,

    /// Rendered human-readable message. Produced by the typed error's
    /// `Display` impl at conversion time. Never a debug-format dump.
    pub message: String,

    /// Optional source-level location. `None` for context-free errors
    /// (e.g. a `CatalogUnavailable` that originates from a network
    /// failure with no document anchor).
    pub location: Option<Span>,

    /// Optional supplementary lines attached to the primary message.
    /// Each `ContextLine` is a short annotated pointer — "declared
    /// here", "referenced here", "conflicting occurrence here" — used
    /// to surface multi-site diagnostics without the caller having to
    /// walk nested source chains.
    pub context: Vec<ContextLine>,
}
```

Construction is performed by the `IntoDiagnostic` impl on each typed error enum; callers never construct `Diagnostic` by hand. The `#[non_exhaustive]` annotation permits adding fields (e.g. a `related_code: Option<&'static str>` cross-reference, a `notes: Vec<String>`) in a MINOR release.

### 5.2 `Severity`

```rust
#[non_exhaustive]
pub enum Severity {
    /// Informational. The stage proceeded; this diagnostic records a
    /// caller-visible decision (e.g. an auto-applied default, a
    /// coverage hint). MUST NOT be silently dropped at API boundaries.
    Info,

    /// Advisory. The stage proceeded; the condition merits author
    /// attention but does not halt work. MUST NOT be silently dropped.
    Warning,

    /// Fatal. In fail-fast stages (`compile`, `plan`, `optimize`,
    /// `adapt`), the first `Error` aborts. In accumulate stages
    /// (`parse`, `validate`), all `Error` diagnostics are collected
    /// and the stage returns the full vector.
    Error,
}
```

The three variants match `00 §4.1`'s Diagnostic-row canonical list (`Info`, `Warning`, `Error`). `#[non_exhaustive]` leaves room for future gradations (e.g. `Hint`) without breaking matcher code.

### 5.3 `Span` and `ContextLine`

```rust
/// Source-level location. Shape is narrow at the `30` layer; richer
/// types (YAML line/column, JSON pointer) live in the stage-owning
/// crate and convert into this form at Diagnostic construction.
#[non_exhaustive]
pub struct Span {
    pub source: SourceId,
    pub byte_range: ByteRange,
}

/// Opaque source identifier. `SourceId` enumerates the distinct
/// document kinds `semstrait-*` deals with (Model YAML file, inline
/// string for tests, etc.). Ratified in `32`; kept in
/// `semstrait-core` for cross-crate reuse.
#[non_exhaustive]
pub enum SourceId {
    ModelFile { path: PathBuf },
    ModelInline { label: &'static str },
}

/// Half-open byte range into the source document.
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// Supplementary line attached to a primary Diagnostic. Carries its
/// own span and a short annotation. Not recursive — context lines
/// never carry their own `ContextLine` sub-chains.
#[non_exhaustive]
pub struct ContextLine {
    pub label: &'static str,  // e.g. "declared here", "referenced here"
    pub location: Span,
}
```

**Placement.** `Diagnostic`, `Severity`, `Span`, `SourceId`, `ByteRange`, `ContextLine` live in `semstrait-core` (ratified in `31`). Every crate that surfaces diagnostics imports them; no crate defines competing local types. `SourceId`'s variant set is extended in `32` as `semstrait-model` adds source kinds — the extension is MINOR because `SourceId` is `#[non_exhaustive]`.

### 5.4 `IntoDiagnostic` conversion boundary

Every typed stage-error enum (`ParseError`, `ValidateError`, `CompileError`, `PlanError`, `OptimizeError`, `AdaptError`) implements:

```rust
pub trait IntoDiagnostic {
    fn into_diagnostic(self) -> Diagnostic;
}
```

Implementations are straightforward and mechanical:

- `code` — the stable literal from the subsystem / stage table (§6).
- `severity` — `Error` for every enum variant unless a specific variant is authored as `Warning` or `Info` (rare — most typed-error variants are fatal).
- `message` — the enum's `Display` output.
- `location` — variant-specific `Span`, `None` when context-free.
- `context` — zero or more `ContextLine`s; "declared here" / "referenced here" pairs are common for shape-unification conflicts.

A blanket impl converts `Vec<StageError>` into `Vec<Diagnostic>` by mapping each element. Mixed-severity accumulation is preserved — `parse` / `validate` callers receive warnings and errors interleaved in the same vector, and callers may partition by `.iter().filter(|d| d.severity == Severity::Error)` when they need only fatal entries.

### 5.5 Banned patterns

The following patterns are forbidden on any `pub` entry point in the workspace:

- Returning `Result<T, String>`, `Result<T, anyhow::Error>`, `Result<T, Box<dyn Error>>`, or any untyped error.
- Panicking on caller-reachable input (malformed YAML, missing catalog entries, unsupported engine features). A panic is a semstrait bug.
- Emitting a raw `eprintln!` / `tracing::error!` line for a condition a caller would want to react to. Callers receive diagnostics; logging is separate and additive.

Internal-only APIs (`pub(crate)`) may use typed enums directly without converting to `Diagnostic`.

## 6. Stable Error-Code Format

Every `Diagnostic::code` is a literal string of the form:

```
{SUBSYSTEM}_{SEVERITY}_{NUMBER}
```

where `SUBSYSTEM` is an uppercase ASCII identifier, `SEVERITY` is `E`/`W`/`I`, and `NUMBER` is a 4-digit zero-padded integer. Example: `EXPR_E_0301` (already ratified in `14 §7.3` / `14a §8`), `PLAN_E_0501`, `ADAPT_W_0401`.

### 6.1 Subsystem prefixes

Subsystems are pipeline-stage-aligned with one exception (`EXPR`, which spans parse/validate/compile for expression-specific errors):

| Prefix | Subsystem | Owning stage(s) |
|---|---|---|
| `PARSE` | YAML / schema parsing (non-expression) | `parse` |
| `VALID` | Structural Preconditions (non-expression) | `validate` |
| `COMP` | Compile-stage errors (non-expression) | `compile` |
| `EXPR` | Expression parsing / validation / compilation | `parse`, `validate`, `compile` |
| `PLAN` | Planner errors | `plan` |
| `OPT` | Optimizer errors and rewrite warnings | `optimize` |
| `ADAPT` | Adapter errors | `adapt` |
| `IR` | IR / `SemanticPlan` tree-shape errors (construction-time well-formedness) | owned by `semstrait-ir` (`35`); produced at `plan` boundary and `transform` hooks |
| `CAT` | Catalog-provider errors (structured metadata integration) | `semstrait-catalog` `CatalogProvider` (`37`) — `compile` stage + gated drift check |
| `FS` | Filesystem-provider errors (generic I/O) | `semstrait-catalog` `FileSystem` (`37`) — `compile` stage (glob, list, read) |
| `IO` | Manifest-persistence I/O errors (`Repository` layer) | `semstrait-manifest` (`33`) — load / save / list / delete |

**Why `EXPR` is cross-stage.** Expression-related errors have a single author-facing mental model — "something is wrong with this `expr:` block" — regardless of whether the parser rejected the DSL syntax, the wrapper invariants caught a misplaced `EntityRef`, or the type inference produced a conflict. Keeping them under one `EXPR` subsystem prefix (with sub-ranges per category) matches that mental model and matches the catalog already ratified in `14 §7`. Non-expression errors at the same stage use the stage prefix (`PARSE_E_0001` for a YAML top-level-key error, `EXPR_E_0001` for an Inline-DSL-syntax error).

### 6.2 Reserved number ranges

Ranges are reserved per subsystem + severity. Ranges are **structural, not sequential** — a new error within a category is assigned the next free number in its reserved sub-range. Gaps within a range are intentional (they reserve room for future additions without renumbering).

| Subsystem | Severity | Range | Sub-ranges (by category) |
|---|---|---|---|
| `PARSE` | `E` | `0001`–`0999` | `0001`–`0099` YAML syntax; `0100`–`0199` schema tag / discriminator; `0200`–`0299` structural (duplicate, missing required field); `0300`–`0399` include / multi-file (reserved); `0900`–`0999` internal parser faults. |
| `PARSE` | `W` | `0001`–`0999` | `0001`–`0099` advisory (anchor shadowing, deprecated syntax). |
| `VALID` | `E` | `0100`–`2599` | `0100`–`0199` name-related (`N-V1`..`N-V4`); `0200`–`0299` nesting (`12 §4`); `0300`–`0399` arity / structural shape (`11 §8.8` `ShapeFieldConflict`, `ShapeMalformed`); `0400`–`0499` temporal-shape / relationship well-formedness (legacy slot; superseded by `1700`–`1799` once `17` lands); `0500`–`0599` key declarations; `0900`–`0999` internal; `1700`–`1799` `TemporalShape` structural checks (`17 §9`); `2000`–`2099` DataKind shared structural (`20 §8.2`); `2100`–`2199` Dataset / Simple (`21 §7`); `2200`–`2299` Grainset (`22 §7`); `2300`–`2399` Unionset (`23 §8`); `2400`–`2499` Joinset (`24 §9`); `2500`–`2599` applicability / cross-variant (`25`). |
| `VALID` | `W` | `0100`–`2599` | reserved (sparingly used — advisory surfaces prefer `PLAN_W_*`). |
| `COMP` | `E` | `0100`–`2599` (non-expression) | `0100`–`0199` name resolution (`N-C1`..`N-C9`); `0200`–`0299` catalog / source resolution (`SourceNotFound`, `CatalogUnavailable`, `SchemaResolutionFailed`, `GlobExpansionFailed`); `0300`–`0399` schema / binding (`SemanticMapping` [pre-`18`-consolidation name: `ColumnMapping`], `PhysicalSource` issues per `15`); `0400`–`0499` relationship graph / index build / internal (`CircularRelationship`, `IndexBuildFailed`); `1700`–`1799` `TemporalShape` compile-stage (`17 §9`); `2000`–`2099` DataKind shared compile (`20 §8.2`); `2100`–`2199` Dataset compile (`21 §8`); `2200`–`2299` Grainset compile (`22 §8`); `2300`–`2399` Unionset compile (`23 §9`); `2400`–`2499` Joinset compile (`24 §10`); `2500`–`2599` applicability / cross-variant compile (`25`). |
| `COMP` | `W` | `0100`–`2599` | advisory (e.g. `AdditivityMismatch` when not routed through `EXPR`; per-kind widening / nullability advisories allocated from the same ranges as errors). |
| `EXPR` | `E` | `0001`–`0499` | `0001`–`0099` parse (`14 §7.1`); `0100`–`0199` validate wrapper invariants (`14 §7.2`); `0200`–`0299` compile name resolution (`14 §7.3` — entity-ref, column, reachability, cycle); `0300`–`0399` compile function resolution (`14a §8`); `0400`–`0499` compile type resolution (`14 §7.3` — inference failure, unify conflict, literal overflow). |
| `EXPR` | `W` | `0001`–`0499` | `0001`–`0099` narrowing / widening (`14 §7.4` — `CastNarrowing`, `AdditivityMismatch`). |
| `PLAN` | `E` | `0500`–`2599` | `0500`–`0599` Constraint-violation + request-shape errors (`11 §8.7` — `ConstraintViolation`, `UnknownReference`, `AmbiguousFieldFirstResolution`, `UnsupportedRequestShape`, `NonAdditiveRollupRequired`); `0600`–`0699` strategy dispatch / PlanNode construction / internal (`StrategyDispatchFailed`, per-DataKind planner errors per `20`–`25`); `1700`–`1799` `TemporalShape` plan-stage errors (`17 §9` — as-of gating, shape mismatch, SCD wide-composition); `2000`–`2099` DataKind shared plan-stage (`20 §8.2`); `2100`–`2199` Dataset plan-stage (`21 §9`); `2200`–`2299` Grainset plan-stage (`22 §9`); `2300`–`2399` Unionset plan-stage (`23 §10`); `2400`–`2499` Joinset plan-stage (`24 §11`); `2500`–`2599` applicability / cross-variant (`25`). |
| `PLAN` | `W` | `0500`–`2599` | advisory diagnostics — shape / additivity inconsistency (`17 §7`), Unionset shape-mismatch (`23 §6`), Grainset tie-break advisories (`22 §4.5`), Joinset `AsOf` activation (`24 §5`), etc. Allocated in parallel to the error sub-ranges above. |
| `PLAN` | `I` | `0500`–`2599` | reserved (e.g. "default substitution applied"). |
| `OPT` | `E` | `0100`–`0199` | adapter-pass failures (`PassFailed`, `InvalidRewrite`) — canonical passes in v1 do not error. |
| `OPT` | `W` | `0100`–`0199` | canonical rewrite warnings (e.g. "no optimization applicable"). |
| `ADAPT` | `E` | `0300`–`0499` | `0300`–`0399` unsupported feature / dialect (`UnsupportedFeature`, `DialectUnsupported`, `UnsupportedAggregateDistinct`); `0400`–`0499` emission / adaptation (`AdaptationFailed`, `EmitFailed`). |
| `ADAPT` | `W` | `0300`–`0499` | `0300`–`0399` per-adapter advisory (e.g. "rewrite applied"). |
| `IR` | `E` | `3500`–`3599` | `SemanticPlan` / `PlanNode` tree-shape well-formedness failures (`35 §10`): name validity, type unresolved, schema mismatch, union arity / schema divergence, column-ref unresolved, transform invariant violated, artifact serialization failure. |
| `IR` | `W` | `3500`–`3599` | reserved (v1 has no `IR` warnings; the tree-shape contract is binary). |
| `CAT` | `E` | `0100`–`0399` | `CatalogProvider` errors (`37 §8`): `0100`–`0199` availability / endpoint reachability; `0200`–`0299` transport / auth; `0300`–`0399` protocol / contract (schema drift, unknown namespace, unknown table, unsupported partition transform). |
| `CAT` | `W` | `0100`–`0399` | reserved (v1 — advisory drift warnings may populate this range). |
| `FS` | `E` | `0100`–`0199` | `FileSystem` errors (`37 §8`): `0100`–`0109` input / argument; `0110`–`0198` transport / auth / permissions / not-found / I/O; `0199` internal. |
| `FS` | `W` | `0100`–`0199` | reserved. |
| `IO` | `E` | `0100`–`0199` | `Repository` persistence errors (`33 §11`): load / save / list / delete failures, manifest-ID collision, encoding-format mismatch, integrity-check failure. |
| `IO` | `W` | `0100`–`0199` | reserved. |

**Note on `OPT` and `PLAN` overlap.** `PLAN_E_*` covers planner errors from `plan` (stage 4). `OPT_E_*` / `OPT_W_*` covers `optimize` (stage 5). Separate subsystem prefixes keep them lexically distinct even though both stages live in `semstrait-planner` (`10 §4`). `PLAN_I_*` is reserved against a future need for info-level planner diagnostics.

### 6.3 Stability promise

- A published error code's meaning (the condition it fires on, the enum variant it corresponds to) is **frozen at its first release**. A MAJOR release may retire the code but not repurpose it.
- Adding a new code within a reserved range is **MINOR** (per §2).
- Retiring a code is **MAJOR**; the retired literal is removed from the public `&'static str` constants and an entry appears in `implementation/42_migration_notes.md` documenting the replacement code (if any).
- Deprecating a code is **MINOR**; the literal remains in the public surface for at least one MINOR cycle with `#[deprecated(since, note)]` on the surrounding symbol.

### 6.4 Code assignment discipline

When a new typed error variant is added in a `3x` doc:

1. Pick the subsystem (stage-aligned; `EXPR` for expression-specific).
2. Pick the severity (`E`, `W`, `I`).
3. Assign the next free number in the reserved sub-range; never reuse a retired number within a 10-year horizon.
4. Document the code in the `3x` doc's error-variant table.
5. If the variant is the first in a previously-unused sub-range, cross-reference the new sub-range allocation in `30 §6.2` via a `42_migration_notes.md` entry.

### 6.5 Non-subsystem codes are banned

No `Diagnostic::code` is allowed that does not match the format of §6.1. Free-form codes, `serde`-internal codes, third-party error codes — all must be wrapped behind an `IntoDiagnostic` impl that assigns a semstrait code. The wrapping `Diagnostic` carries the upstream message in its `message` field and the upstream code (if any) as a `ContextLine`.

### 6.6 Reserved future prefixes

Prefixes reserved but not yet populated, to prevent collision with future subsystems:

- `REG` — registry / `FunctionRegistry` initialization-time errors (currently surfaced as panics per `14a §7.2`; may be promoted to a subsystem if future work needs recoverable init paths).
- `IO` — direct-I/O subsystem (currently all I/O flows through `CatalogProvider` / `FileSystem` / `Repository`; reserved against a future non-pipeline I/O subsystem).
- `ENG` — engine-specific adapter variations beyond `ADAPT` (reserved for possible per-engine diagnostic streams in `36`).

Subsystem-prefix additions are MINOR.

### 6.7 Retired codes

None in v1. This sub-section activates if any retirement occurs; its format will be:

```
| Code        | Retired in | Replacement    | Rationale |
```

The `implementation/42_migration_notes.md` entry carries the full migration story; `30 §6.7` is the quick lookup for log-scraping tools.

## 7. Error-Emission Policy per Stage

The per-stage policy is ratified in `10 §5`; `30` carries it forward to the crate-public-surface level, where the contract matters for callers assembling error handling.

| Stage | Public-API return shape | Policy |
|---|---|---|
| `parse` | `Result<SemanticModel, Vec<Diagnostic>>` with warnings interleaved | accumulate |
| `validate` | `Result<(), Vec<Diagnostic>>` with warnings interleaved | accumulate |
| `compile` | `Result<(Manifest, Vec<Diagnostic>), (Diagnostic, Vec<Diagnostic>)>` — error arm carries the fatal `Diagnostic` plus any warnings up to failure | fail-fast |
| `plan` | `Result<(SemanticPlan, Vec<Diagnostic>), (Diagnostic, Vec<Diagnostic>)>` | fail-fast |
| `optimize` | `Result<(SemanticPlan, Vec<Diagnostic>), (Diagnostic, Vec<Diagnostic>)>` | fail-fast |
| `adapt` | `Result<(EngineArtifact, Vec<Diagnostic>), (Diagnostic, Vec<Diagnostic>)>` | fail-fast |

**Warnings are never silently dropped.** Every fail-fast stage carries accumulated `Info`/`Warning` diagnostics back to the caller in both the success and failure arms. The exact `Result` shape is per-crate — some crates expose a builder-style API where warnings are drained via a separate accessor — but the principle binds.

**Stage-ownership of codes.** A `Diagnostic` with `EXPR_*` code MAY be produced by `parse`, `validate`, or `compile`; the subsystem prefix is **not** a claim about which stage raised it. A caller routing diagnostics by origin should key on the owning function / stage, not on the prefix.

### 7.1 Accumulation limits

`parse` and `validate` accumulate without an intrinsic limit — every independent error is reported. In practice, large malformed models can produce hundreds of errors; per-crate docs (`32`) may introduce a soft cap with a "further errors suppressed" diagnostic to protect caller UX. No cap is imposed at the `30` level.

### 7.2 Panic-freedom

Public entry points never panic on caller-reachable input. Internal `unreachable!()` / `panic!("invariant ...")` calls are permitted only where the invariant is genuinely impossible to violate without a semstrait bug (e.g. a `Manifest` field the compile pass is sworn to populate). A caller-reachable panic is a semstrait bug and is fixed as such.

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
| `RegistryExtension` | `function_registry()` initializer in `semstrait-core` | `semstrait-adapter-*` crates | open (see `questions/open/30_questions.md` Q-API-009) | `36` (via `14a §7`) |
| `IntoDiagnostic` | every public entry point | each stage's typed error enum | open — enables third-party error kinds to enter the Diagnostic pipeline | `31` |

### 8.3 Trait-method return shape

Every fallible public trait method returns `Result<T, Diagnostic>` (or `Result<T, Vec<Diagnostic>>` for accumulate contexts), not a typed error enum. Typed enums are internal carriers; the trait boundary is where the conversion to `Diagnostic` happens. This is consistent with §5.5's ban on raw-string errors on public surfaces and keeps the `EngineAdapter` / `CatalogProvider` / `FileSystem` contracts uniform for callers.

### 8.4 Async trait methods

When a trait method is async (only `CatalogProvider`, `FileSystem`, `Repository` in v1 — see §9), the trait uses `async fn` directly (Rust 1.75+ async-fn-in-trait). `dyn` dispatch over the trait is provided via an adjacent object-safe facade when needed; the trait itself is not object-safe. Per-crate docs (`33`, `37`) specify the exact facade shape.

## 9. Async Posture per Crate

Per I6 (sync hot path) and I11 (gated I/O), most crates are sync-only. The exceptions are the crates that orchestrate compile-time I/O or that bridge to async metadata / storage providers.

| Crate | Sync / async | I/O allowed? | Rationale |
|---|---|---|---|
| `semstrait-core` | Sync only | No | Pure primitives (`DataType`, `Diagnostic`, `Span`, `CanonicalFn`); no I/O surface. |
| `semstrait-model` | Sync only | No | `parse` and `validate` are pure transformations over in-memory YAML. |
| `semstrait-manifest` | Compile-time async; plan-time sync | Compile-time via providers | The `compile` entry point is `async fn` (awaits `CatalogProvider` / `FileSystem`). The `Manifest` is then consumed synchronously; no `async fn` at plan time. |
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

Every `semstrait-*` crate ships with default features equal to the minimum set needed to use that crate's primary function. No crate gates a core type or trait behind an opt-in feature. A consumer depending on `semstrait-core` gets every public type in `semstrait-core` with no `--features` hunt.

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

Serde support on public types is opt-in via a `serde` feature per crate. Default-off for `semstrait-core`, `semstrait-model`, `semstrait-manifest`, `semstrait-ir`. Enabled downstream (e.g. `semstrait-facade` turns it on by default for end-user convenience).

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

Internal refactors that preserve every observable output (same `Manifest` bytes, same `SemanticPlan` tree, same `EngineArtifact` text) are PATCH. Determinism (I4) makes this bit-comparable for `Manifest` and `SemanticPlan`; adapter output is compared at the `SqlArtifact::text` / `EnginePlan` serialization level. A refactor that produces equivalent but not byte-identical output is MINOR (callers comparing artifacts byte-for-byte, e.g. for content-addressable caching, see the change).

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

### 12.3 Deprecation ≠ retirement

A **deprecated** error code is still produced by the runtime; callers matching on it continue to work. A **retired** error code is no longer produced and its `&'static str` literal is gone from the public surface. §6.3 covers retirement; this section covers the deprecation window that precedes it.

### 12.4 Minimum window

At least one full MINOR cycle between `#[deprecated]` and removal. Longer for widely-used symbols — per-crate `3x` docs may specify extended windows (e.g. two MINOR cycles for a core `CatalogProvider` method).

## 13. Stability Table

v1 per-crate maturity markers. These lock at the v1.0 cut and evolve per the semver rules of §2.

| Crate | Stability | Notes |
|---|---|---|
| `semstrait-core` | Stable in v1 | Canonical shared primitives: `DataType`, `Diagnostic`, `Severity`, `Span`, `CanonicalFn` newtype, `FunctionRegistry` public surface. Breaking changes require a workspace-wide MAJOR. |
| `semstrait-model` | Stable in v1 | `SemanticModel`, `ParseError`, `ValidateError`, YAML grammar. The author-facing YAML shape extends non-exhaustively (new keys, new variants) in MINOR. |
| `semstrait-manifest` | Stable in v1 | `Manifest`, `Resolved*` family, `CompileError`, `Repository` trait. **Internal serialization format (Manifest on-disk bytes) is NOT a public API** — callers round-trip through `Repository::save` / `Repository::load`, not through direct byte access. |
| `semstrait-planner` | Stable in v1 | `plan`, `optimize`, `PlanError`, `OptimizeError`. Per-DataKind strategy dispatch is internal. `PlanNode` variants are I10 `#[non_exhaustive]`. |
| `semstrait-ir` | Stable in v1 | `SemanticPlan`, `PlanNode`, `EngineArtifact`, `SqlArtifact`, `EnginePlan`. Variant and field additions are MINOR via `#[non_exhaustive]`. |
| `semstrait-adapter` | Provisional | `EngineAdapter` trait stable; `AdaptError` stable; `DialectId` extends in MINOR. Per-engine adapter crates (`semstrait-adapter-datafusion`, `semstrait-adapter-duckdb`, `semstrait-adapter-spark`, `semstrait-adapter-substrait`) are **versioned independently** and may carry their own stability tier in their own `3x` appendix. |
| `semstrait-catalog` | Provisional | `CatalogProvider`, `FileSystem`, local-fs impl stable. Per-provider impls (`semstrait-catalog-iceberg`, `semstrait-catalog-unity`) are **versioned independently**; their stability follows their own maturity. |
| `semstrait-api` | Stable in v1 | Unified entry point wrapping the `parse → … → adapt` pipeline. Re-exports the minimum `semstrait-*` types required to use the pipeline end-to-end. |
| `semstrait-facade` | Stable in v1 | Facade over `semstrait-api` for one-shot use (single compile, single plan, single adapt). Default features enable the minimum useful adapter bundle; extension is via `semstrait-api`. |

**Provisional** crates may introduce non-additive changes in MINOR cycles (pre-1.0 semver rules continue for these crates past workspace-v1.0 if they are not yet promoted). Every provisional change still carries a `42` migration note.

## 14. Ratified Decisions Index

The following decisions are ratified in this document. Each bullet links to the section that carries the full rationale.

- **§2.1** — Coordinated workspace release: every `semstrait-*` crate ships on the same version number.
- **§2.1** — MINOR is strictly additive (new `#[non_exhaustive]` variant, new `#[non_exhaustive]` field, new public symbol, `#[deprecated]` attribute). MAJOR is anything non-additive. PATCH is behavior-preserving.
- **§2.2** — Retiring an error code is MAJOR; deprecating one is MINOR.
- **§2.3** — Pre-1.0 semver rules apply until the synchronized v1.0 cut; MINOR may carry breaking changes with migration notes.
- **§3.1** — Default-`pub(crate)`; `pub` only with documented consumer need, invariants, and inclusion in the owning `3x` doc.
- **§3.2** — `missing_docs` is a CI failure for every `semstrait-*` crate.
- **§4.1** — The public sum-type roster is `#[non_exhaustive]`: canonical domain enums (`DataType`, `DataKind`, `Additivity`, `Cardinality`, `JoinType`, `DialectId`, `EngineArtifact`, `EnginePlan`, `ExprSource` variants, `TemporalShape` + SCD subtype, composition-kind tag, `DimensionType`, `Grain`, `LiteralValue`, `BinaryOpKind`, `Aggregation`, `FunctionCategory`, `ParamType`, `ReturnTypeRule`, `Portability`), `Severity`, `SourceId` variants, every `StageError` enum.
- **§4.1** — `CanonicalFn` is a newtype; inherently extensible; no `#[non_exhaustive]` annotation needed.
- **§4.2** — `Diagnostic`, `FunctionSpec`, `Manifest` + `Resolved*` family, `SemanticPlan`, `PlanNode` sub-structs, `SemanticInterface`, `ComposedSemanticInterface`, `Request`, `SessionContext`, `SqlArtifact`, `Span`, `ContextLine` are `#[non_exhaustive]` public structs.
- **§4.3** — Internal-only (`pub(crate)` or narrower) enums MAY be exhaustive.
- **§4.4** — Library code never panics on an unknown non-exhaustive variant; wildcard arms return `Diagnostic`s.
- **§5.1** — `Diagnostic { code, severity, message, location, context }` is the canonical shape; `#[non_exhaustive]`.
- **§5.2** — `Severity ∈ {Info, Warning, Error}`; `#[non_exhaustive]`. Matches `00 §4.1` vocabulary.
- **§5.3** — `Span { source: SourceId, byte_range: ByteRange }` and `ContextLine { label, location }` live in `semstrait-core`. `SourceId` is `#[non_exhaustive]` and extended by `32`.
- **§5.4** — `IntoDiagnostic` is the conversion trait at every public API boundary; every typed stage-error enum implements it.
- **§5.5** — Raw-string errors, `anyhow::Error`, `Box<dyn Error>` are banned on public APIs. Panics on caller-reachable input are banned.
- **§6.1** — Stable error-code format: `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` with 4-digit zero-padded `NUMBER`.
- **§6.1** — Subsystem prefixes: `PARSE`, `VALID`, `COMP`, `EXPR`, `PLAN`, `OPT`, `ADAPT`. Reserved: `REG`, `IO`, `ENG`.
- **§6.2** — Concrete ranges:
  - `PARSE_E` `0001`–`0999` (sub-ranged); `PARSE_W` `0001`–`0999`.
  - `VALID_E` `0100`–`0999` (sub-ranged by `N-V*` / nesting / arity / shape / key); `VALID_W` `0100`–`0999`.
  - `COMP_E` `0100`–`0499` (name / catalog / schema / relationship); `COMP_W` `0100`–`0499`.
  - `EXPR_E` `0001`–`0499` (parse / validate / compile-name / compile-function / compile-type); `EXPR_W` `0001`–`0499`.
  - `PLAN_E` `0500`–`0699` (Constraint + shape / strategy); `PLAN_W` / `PLAN_I` `0500`–`0699`.
  - `OPT_E` `0100`–`0199` (adapter-pass failures); `OPT_W` `0100`–`0199`.
  - `ADAPT_E` `0300`–`0499` (unsupported / emission); `ADAPT_W` `0300`–`0499`.
- **§6.3** — A published code's meaning is frozen at first release; retirement is MAJOR, deprecation is MINOR.
- **§6.5** — Non-subsystem codes are banned on `Diagnostic::code`.
- **§7** — Per-stage API shapes match `10 §5`: `parse` / `validate` accumulate; `compile` / `plan` / `optimize` / `adapt` fail-fast. Warnings never silently drop.
- **§8.1** — Sealed public traits use the private-super-trait pattern.
- **§8.2** — Trait roster: `CatalogProvider`, `FileSystem`, `Repository`, `EngineAdapter`, `RegistryExtension`, `IntoDiagnostic` — all open in v1.
- **§8.3** — Public trait methods return `Result<T, Diagnostic>`, not a typed-error-enum.
- **§8.4** — Async trait methods use `async fn` in trait; object-safe facades are added per-crate when needed.
- **§9** — Per-crate async matrix: compile-time async is `semstrait-manifest`, `semstrait-api`, `semstrait-facade`; async trait surface is `semstrait-catalog`; everything else is sync.
- **§9** — No `.await` inside `plan` / `optimize` / `adapt` (I6).
- **§10.1** — Default features = minimum viable; no core type gated behind a feature.
- **§10.2** — Adapter support is delivered as separate crates (`semstrait-adapter-datafusion`, …), not as feature flags on a monolithic adapter crate.
- **§10.3** — Catalog provider support is delivered as separate crates (`semstrait-catalog-iceberg`, …).
- **§10.4** — `serde` support is opt-in per crate via a `serde` feature; documented per `3x`.
- **§10.5** — No `nightly` features; no `no_std`; no mutually exclusive features within a crate.
- **§11.2** — Every non-additive change requires a MAJOR changelog entry, a `42_migration_notes.md` entry, a deprecation window where feasible, and a `cargo-semver-checks` green-light (or waiver).
- **§12.1** — `#[deprecated]` lifecycle: Active → Deprecated (≥ 1 MINOR cycle) → Removed (MAJOR).
- **§12.2** — Deprecations are tracked in `implementation/41_deprecations.md`.
- **§13** — Stability table: `Stable in v1` for `semstrait-core`, `-model`, `-manifest`, `-planner`, `-ir`, `-api`, `-facade`; `Provisional` for `semstrait-adapter` (trait stable; per-engine crates version independently) and `semstrait-catalog` (trait stable; per-provider crates version independently).

## 15. Cross-References

- `00 §4.1` — `Diagnostic` row (authoritative-doc pointer → `30`).
- `00 §4.2` — verb catalog; `30 §7` ratifies the API-boundary return shapes.
- `00 §9` — I7 (strict DAG), I10 (non-exhaustive), I11 (gated I/O), I12 (first-class diagnostics).
- `10 §3` — per-stage contracts consumed by `30 §7`.
- `10 §5` — internal-error model; `30 §5`–`§6` are the public-boundary refinement.
- `11 §8` — Constraint framework; `PLAN_E_05xx` carries `ConstraintViolation`.
- `14 §7` — expression-error catalog under `EXPR_*`.
- `14a §3.1`, `§7`, `§8` — `FunctionSpec` `#[non_exhaustive]`, `RegistryExtension`, function-resolution error codes.
- `13 §6` — type-related Precondition IDs (`TG-*`) map to `VALID_*` / `COMP_*`.
- `31`–`39` — per-crate refinements of every policy in this doc.
- `implementation/41_deprecations.md` — deprecation lifecycle tracking.
- `implementation/42_migration_notes.md` — MAJOR migration entries.
- `questions/open/30_questions.md` — parked reconciliation items (notably Q-API-001 on `10 §5.1` alignment).
