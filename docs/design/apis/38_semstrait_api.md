---
doc: design/apis/38_semstrait_api
status: Round-1 Draft
prereqs:
  - 30
  - 31
  - 32
  - 33
  - 34
  - 35
  - 36
  - 37
authoritative-for:
  - "`SemStrait` orchestrator: field shape, constructor entry (`SemStrait::builder`), method roster"
  - "`SemStraitBuilder` configuration surface (per-stage injection: catalog, filesystem, repository, function-registry handle, optimizer passes, warning policy)"
  - "`WarningPolicy` enum (accumulate / fail-on-warning / strict) and the stage-boundary escalation rules"
  - "`SemStraitError` unified error enum (per-stage variants; no new subsystem prefix)"
  - "warning propagation across the parse → validate → compile → plan → adapt pipeline at the API-crate boundary"
  - "one-shot convenience contract (`compile_and_plan_and_adapt`)"
  - "streaming / incremental contract (compile-once, plan-many against a shared `Manifest`)"
  - "async / sync boundary exposed on the API-crate surface (compile-stage async entries; plan / optimize / adapt sync entries; I11b drift-check async entry)"
  - "diagnostic-propagation discipline (source-order guarantee within a stage; cross-stage ordering; de-duplication)"
  - "crate boundaries: no new domain logic; no I/O beyond injected providers; no raw SQL; no catalog branching"
references:
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/32_semstrait_model.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
  - apis/35_semstrait_ir.md
  - apis/36_semstrait_adapter.md
  - apis/37_semstrait_catalog.md
  - 00_overview.md
---

# `semstrait-api` — Public API Contract

> Round-1 draft. Ratifies the public surface of the `semstrait-api` crate: the single orchestration entry point that stitches `parse` → `validate` → `compile` → `plan` → `adapt` into a caller-friendly flow. This crate owns zero domain logic; it owns one builder-configured orchestrator, one warning-propagation policy, and one unified error enum. Every stage-specific decision continues to live in its owning `3x` crate; `38` is the seam that makes those decisions addressable through one handle.

---

## 1. Purpose, scope, layering

### 1.1 Purpose

`semstrait-api` is the top-level orchestration crate for the semstrait toolchain. Its reason for existence is ergonomic: a direct caller of `semstrait-model`, `semstrait-manifest`, `semstrait-planner`, and `semstrait-adapter` must thread catalog providers, filesystems, function registries, optimizer-pass vectors, and warning sinks through five successive call sites, each with its own error type and its own async posture. `semstrait-api` collapses that into a single `SemStrait` handle with a `SemStraitBuilder` for setup.

`semstrait-facade` (`39`) sits one layer above `38`, exposing a still-narrower "one-shot-no-configuration" entry point for users who want defaults everywhere. This crate (`38`) is the configurable layer; `39` is the zero-config layer.

### 1.2 Scope — what this crate owns

- **Orchestration.** The `SemStrait` struct and its method roster (`§3`), composing per-stage crates in canonical pipeline order.
- **Configuration carrier.** The `SemStraitBuilder` (`§4`), collecting `CatalogProvider` / `FileSystem` / `Repository` handles, the function-registry handle, optimizer-pass pipeline, and `WarningPolicy`.
- **Warning-propagation policy.** `WarningPolicy` (`§5`) and the accumulate / escalate / strict rules at stage boundaries.
- **Unified error type.** `SemStraitError` (`§6`), wrapping each stage's typed-error carrier without a new subsystem prefix.
- **One-shot convenience.** `compile_and_plan_and_adapt` (`§7`).
- **Streaming / incremental support.** Compile-once / plan-many lifecycle (`§8`).
- **I11b plumbing.** `save_manifest` / `load_manifest` (thin-wraps `Repository::save` / `load`, `33 §11`) and `validate_manifest` (thin-wraps `CatalogProvider::check_schema_drift`, `37 §9`).

### 1.3 Scope — what this crate does NOT own

- **No new domain types.** Every exposed struct is a re-export from `31`–`37` or a small orchestration helper. No new `Resolved*`, `PlanNode`, `EngineArtifact` variant, or `DataType`.
- **No I/O beyond injected providers.** Every catalog / byte / persistence access flows through `&dyn CatalogProvider` / `&dyn FileSystem` / `&dyn Repository`. No network or disk code lives in this crate.
- **No SQL emission.** SQL is an adapter concern (`36 §8`); callers reach it via `EngineAdapter::emit` on the injected adapter, not through `SemStrait`.
- **No planner / adapter logic.** `plan` and `adapt` are thin wrappers over `34` and `36 §3`; they add warning-policy plumbing and error-enum wrapping only.
- **No function-registry construction.** The process-global `function_registry()` (`31 §5`) is the sole source; `SemStrait` carries a `&'static FunctionRegistry` handle.
- **No session state.** Every runtime input (`request`, `adapter`) is passed explicitly per call; configuration is frozen at build-time; the handle is `Send + Sync + Clone`.

### 1.4 Layering

```
caller → SemStrait (configured by SemStraitBuilder)
           ↓
    semstrait-model    semstrait-manifest    semstrait-planner
    semstrait-ir       semstrait-adapter     semstrait-catalog
```

Methods group by I-invariant: `compile_from_yaml` / `compile_from_model` are I11a (async); `plan` / `adapt` are I6 (sync); `compile_and_plan_and_adapt` is async because it straddles compile; `save_manifest` / `load_manifest` / `validate_manifest` are I11b (async out-of-band). Dependency edges flow strictly down and across per I7. `semstrait-api` re-exports the minimum set of types needed to drive the pipeline end-to-end (`§2.2`); no private internals, no crate-boundary-crossing hooks.

---

## 2. Public crate surface

### 2.1 Roster

| Name                         | Kind          | `#[non_exhaustive]`? | Stability (v1) |
|------------------------------|---------------|----------------------|----------------|
| `SemStrait`                  | struct        | no (opaque; new ctor only via builder) | Stable |
| `SemStraitBuilder`           | struct        | no (builder; `Default` + chain methods) | Stable |
| `WarningPolicy`              | enum          | yes                  | Stable         |
| `SemStraitError`             | enum          | yes                  | Stable         |
| `StageOrigin`                | enum          | yes                  | Stable         |
| `PipelineOutcome`            | struct        | yes                  | Provisional    |

Plus the re-exports (`§2.2`).

"Stability (v1)" follows `30 §13`. `semstrait-api` is **Stable in v1**; every struct and enum in its roster inherits that tier. `PipelineOutcome` is `Provisional` because its exact shape depends on ratification of Q-API-002 (one-shot return shape) during the next docs cycle.

### 2.2 Re-exports

The minimum set of types a caller needs to drive the pipeline end-to-end:

- From `31`: `Diagnostic`, `Severity`, `Request`, `SessionContext`.
- From `32`: `SemanticModel`, `ParseErrors`, `ValidateErrors`.
- From `33`: `Manifest`, `ManifestId`, `ManifestMetadata`, `CompileErrors`, `Repository`, `RepositoryError`.
- From `34`: `PlanErrors`.
- From `35`: `SemanticPlan`, `EngineArtifact`, `SqlArtifact`, `EnginePlan`.
- From `36`: `EngineAdapter`, `AdapterId`, `AdaptError`.
- From `37`: `CatalogProvider`, `FileSystem`, `DriftReport`, `CatalogError`, `FileSystemError`.

Re-exports are additive (adding one is MINOR per `30 §2`); removing one is MAJOR. Consumer code SHOULD import through `semstrait_api::…` unless it has a reason to depend directly on an inner crate.

### 2.3 Module layout (informative)

```rust
pub mod builder;   // SemStrait, SemStraitBuilder
pub mod policy;    // WarningPolicy
pub mod error;     // SemStraitError, StageOrigin
pub mod outcome;   // PipelineOutcome

pub use builder::{SemStrait, SemStraitBuilder};
pub use policy::WarningPolicy;
pub use error::{SemStraitError, StageOrigin};
pub use outcome::PipelineOutcome;

pub use semstrait_core::{Diagnostic, Severity, Request, SessionContext};
pub use semstrait_model::{SemanticModel, ParseErrors, ValidateErrors};
pub use semstrait_manifest::{
    Manifest, ManifestId, ManifestMetadata, CompileErrors,
    Repository,
};
pub use semstrait_planner::PlanErrors;
pub use semstrait_ir::{SemanticPlan, EngineArtifact, SqlArtifact, EnginePlan};
pub use semstrait_adapter::{EngineAdapter, AdapterId, AdaptError};
pub use semstrait_catalog::{CatalogProvider, FileSystem, DriftReport};
```

Re-exports at the crate root per `30 §3`. Module names are informative; the exact layout may shift in refactors without MINOR impact so long as the crate-root re-exports hold.

---

## 3. `SemStrait` orchestrator

### 3.1 Purpose

`SemStrait` is a configured, thread-safe handle over the pipeline. It owns `Arc`-shared provider handles and process-global function-registry access; every method consumes `&self`. Two `SemStrait` values constructed with identical builder input are **observationally equivalent**: calling the same method sequence on either produces the same output modulo catalog-snapshot drift.

### 3.2 Struct shape

```rust
pub struct SemStrait {
    catalog_provider: Arc<dyn CatalogProvider>,
    file_system: Arc<dyn FileSystem>,
    repository: Option<Arc<dyn Repository>>,
    function_registry: &'static FunctionRegistry,
    optimizer_passes: Arc<[Box<dyn OptimizerPass>]>,
    warning_policy: WarningPolicy,
}
```

Fields are `pub(crate)`; construction goes through `SemStraitBuilder::build` (`§4.7`). There is no `Default` for `SemStrait` — every usable configuration requires at least one caller decision (the `CatalogProvider` injection), and surfacing a `Default` would force a hidden `NoopCatalogProvider` choice that a caller might not want.

`Arc<[Box<dyn OptimizerPass>]>` is chosen over `Vec<…>` so cloning the `SemStrait` (e.g. into worker tasks) does not re-allocate the pass vector.

### 3.3 Method roster and signatures

```rust
impl SemStrait {
    pub fn builder() -> SemStraitBuilder;

    pub async fn compile_from_yaml(&self, yaml: &str)
        -> Result<Manifest, CompileErrors>;
    pub async fn compile_from_model(&self, model: SemanticModel)
        -> Result<Manifest, CompileErrors>;

    pub fn plan(&self, manifest: &Manifest, request: Request)
        -> Result<SemanticPlan, PlanErrors>;
    pub fn adapt(&self, adapter: &dyn EngineAdapter,
                 plan: &SemanticPlan, manifest: &Manifest)
        -> Result<EngineArtifact, AdaptError>;

    pub async fn compile_and_plan_and_adapt(
        &self, yaml: &str, request: Request, adapter: &dyn EngineAdapter,
    ) -> Result<(EngineArtifact, Vec<Diagnostic>), SemStraitError>;

    // I11b — out-of-band gated entries
    pub async fn save_manifest(&self, manifest: &Manifest)
        -> Result<ManifestId, SemStraitError>;
    pub async fn load_manifest(&self, id: ManifestId)
        -> Result<Manifest, SemStraitError>;
    pub async fn validate_manifest(&self, manifest: &Manifest)
        -> Result<DriftReport, SemStraitError>;

    pub fn warning_policy(&self) -> WarningPolicy;
    pub fn function_registry(&self) -> &'static FunctionRegistry;
    pub fn adapter_id_for<'a>(&'a self, adapter: &'a dyn EngineAdapter) -> AdapterId;
}
```

`adapt` takes `&Manifest` because adapters reach into resolved bindings (`36 §3.1`); `compile_and_plan_and_adapt` is async only so it can straddle the compile I/O boundary. `save_manifest` / `load_manifest` require `self.repository.is_some()` — otherwise they return `SemStraitError::NoRepositoryConfigured`.

### 3.4 Method contracts

- **`compile_from_yaml`** — `parse` (`32 §9`) → `validate` (`32 §11`) → `compile` (`33 §9`). Errors short-circuit into `CompileErrors` (fail-fast carrier per `30 §7`). Parse / validate errors are mapped into the `CompileErrors.fatal` slot via `IntoDiagnostic` (`30 §5.4`).
- **`compile_from_model`** — Skips parse; still runs validate + compile. For callers that synthesize `SemanticModel`s or apply custom post-parse transforms.
- **`plan`** — Wraps `semstrait_planner::plan(manifest, request, &self.optimizer_passes, self.function_registry)` (signature per `34 §*`, drafted in parallel); applies `self.warning_policy` to the returned warnings.
- **`adapt`** — Wraps `adapter.adapt(plan, manifest)` (`36 §3.1`). Per-stage method preserves native `AdaptError`; only the fused helper wraps it as `SemStraitError::AdaptStage`.
- **`compile_and_plan_and_adapt`** — Runs compile → plan → adapt. Accumulates warnings across stages in source order (`§10.2`). On error, returns the first failing stage as a `SemStraitError` variant with warnings up to that point preserved inside the variant.
- **`save_manifest` / `load_manifest`** — Wrap `Repository::save` / `Repository::load` (`33 §11`). Require `self.repository.is_some()`; otherwise `SemStraitError::NoRepositoryConfigured`.
- **`validate_manifest`** — Iterates resolved physical sources (`33 §8`), calls `CatalogProvider::check_schema_drift` (`37 §9`) per source, aggregates under `Unchanged < Compatible < Breaking`. Per-source `DriftKind` details concatenate.

### 3.5 Invariants

| Ref | Invariant |
|-----|-----------|
| I6  | `plan`, `adapt`, and every accessor on `SemStrait` are synchronous. No `.await` point is reachable from these methods. |
| I7  | Depends on `31`–`37`; depended on by `39`. No cycles. |
| I10 | `WarningPolicy`, `SemStraitError`, `StageOrigin`, `PipelineOutcome` are `#[non_exhaustive]`. |
| I11 | `compile_*` is async (I11a); `save_manifest`, `load_manifest`, `validate_manifest` are async (I11b); no other method is async. |
| I12 | Every error carries a `Diagnostic` with a stable code from its owning subsystem (`PARSE_*` / `COMP_*` / `PLAN_*` / `ADAPT_*` / `IO_*` / `CAT_*` / `FS_*`); `SemStraitError` has no subsystem prefix of its own. |

---

## 4. `SemStraitBuilder`

### 4.1 Purpose

`SemStraitBuilder` is the single construction path for `SemStrait`. Chain-style configuration methods populate its fields; `build` produces a validated `SemStrait` (or fails with a `SemStraitError::BuilderInvalid`).

### 4.2 Struct shape

```rust
pub struct SemStraitBuilder {
    catalog_provider: Option<Arc<dyn CatalogProvider>>,
    file_system: Option<Arc<dyn FileSystem>>,
    repository: Option<Arc<dyn Repository>>,
    function_registry: Option<&'static FunctionRegistry>,
    optimizer_passes: Vec<Box<dyn OptimizerPass>>,
    warning_policy: WarningPolicy,
}
```

The builder itself is `pub`, but fields are `pub(crate)`; configuration flows through chain methods.

### 4.3 Required fields

| Field               | Required? | Default if unset                                        |
|---------------------|-----------|----------------------------------------------------------|
| `catalog_provider`  | yes       | — (build fails with `BuilderInvalid`)                   |
| `file_system`       | yes       | — (build fails with `BuilderInvalid`)                   |
| `repository`        | no        | `None` — save/load methods then return `NoRepositoryConfigured` |
| `function_registry` | no        | `semstrait_core::function_registry()` (the process-global instance) |
| `optimizer_passes`  | no        | the canonical-pass list from `34 §*` (`Vec::new()` semantically means "use defaults", not "no passes") |
| `warning_policy`    | no        | `WarningPolicy::Accumulate` (`§5.2`)                    |

`catalog_provider` and `file_system` are required because every non-trivial compile invocation needs both, and silently defaulting either to `NoopCatalogProvider` / `LocalFileSystem` would mask configuration errors. Callers who want a truly zero-I/O build (unit tests, model-only work) construct an explicit `NoopCatalogProvider` + an explicit `LocalFileSystem` pointing at an empty root.

### 4.4 Chain methods

```rust
impl SemStraitBuilder {
    pub fn new() -> Self;            // all None / empty / default
    pub fn catalog_provider(self, cp: Arc<dyn CatalogProvider>) -> Self;
    pub fn file_system(self, fs: Arc<dyn FileSystem>) -> Self;
    pub fn repository(self, repo: Arc<dyn Repository>) -> Self;
    pub fn function_registry(self, reg: &'static FunctionRegistry) -> Self;
    pub fn optimizer_pass(self, pass: Box<dyn OptimizerPass>) -> Self;
    pub fn optimizer_passes(self, passes: Vec<Box<dyn OptimizerPass>>) -> Self;
    pub fn warning_policy(self, policy: WarningPolicy) -> Self;
    pub fn build(self) -> Result<SemStrait, SemStraitError>;
}

impl Default for SemStraitBuilder {
    fn default() -> Self { Self::new() }
}
```

`optimizer_pass` appends; `optimizer_passes` replaces. This mirrors common builder conventions (e.g. `reqwest::ClientBuilder`) and lets callers either additively extend the canonical-pass list or replace it wholesale.

### 4.5 Field rationale

- **`catalog_provider` / `file_system` / `repository`** — `Arc<dyn ...>` because the same provider commonly backs multiple `SemStrait` handles in a service, and I3 forbids branching on concrete type. All three traits are `Send + Sync` (`37 §3.2`, `37 §5.2`, `33 §11.1`). `repository` is optional; its absence gates `save_manifest` / `load_manifest` into `NoRepositoryConfigured` but does not affect compile / plan / adapt.
- **`function_registry: &'static FunctionRegistry`** — Per `31 §5.2`, the canonical registry is process-global and sealed. The field is `Option` only so `build` can fill it from `semstrait_core::function_registry()` when unset. In v1 the process-global registry is the only option; `Q-API-006` parks the question of per-handle registries.
- **`optimizer_passes: Vec<Box<dyn OptimizerPass>>`** — Per `34 §*`, the planner accepts an explicit pass pipeline. `optimizer_pass` appends (additive); `optimizer_passes` replaces wholesale. `build` prepends the canonical-pass list from `34` to the caller-accumulated pipeline unless replace-mode was used.
- **`warning_policy: WarningPolicy`** — See `§5`.

### 4.6 Validation at `build`

`build` checks required fields (`catalog_provider`, `file_system`) and fills unset optional fields with their defaults (`§4.3`). No I/O runs at build time: a builder with an unreachable catalog builds successfully and the first `compile_*` call surfaces the connectivity failure.

```rust
pub fn build(self) -> Result<SemStrait, SemStraitError>;
```

On missing required fields: `Err(SemStraitError::BuilderInvalid { missing })`.

### 4.7 `const fn` constructors

`SemStraitBuilder::new` is `const fn` (all fields initialize to `None` / `Vec::new()` / `WarningPolicy::Accumulate`), letting callers declare builders in `static` / `const` items. Chain methods accepting `Arc<dyn …>` cannot be `const`; `warning_policy` could in principle be made `const fn` in a future MINOR.

---

## 5. `WarningPolicy`

### 5.1 Purpose

The semstrait pipeline produces three severities of `Diagnostic`: `Info`, `Warning`, `Error`. `Error` is always fatal at its owning stage (`30 §7`, accumulate vs fail-fast per stage). `Warning` and `Info` are informational by default but some callers want stricter semantics. `WarningPolicy` is the knob.

### 5.2 Variants

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningPolicy {
    Accumulate,    // default — all Info/Warning pass through to the outcome vector
    FailOnWarning, // first Warning at any stage escalates to that stage's fatal
    Strict,        // like FailOnWarning, but Info escalates too
}
```

`#[non_exhaustive]` so future policies (e.g. `FailOnWarningAfter(Stage)`, `FilterByPrefix(&'static str)`) extend via MINOR (`30 §4`). `Default::default()` returns `Accumulate`.

### 5.3 Escalation rules

- **`Accumulate`.** No-op. Every `Info`/`Warning` pass through to the outcome `Vec<Diagnostic>`. `Error` is handled by the owning stage's fail-fast policy (`30 §7`); warnings produced up to the failure still accumulate.

- **`FailOnWarning`.** Any `Diagnostic { severity: Warning, … }` produced by any stage is re-wrapped into the stage's typed-error carrier (e.g. `CompileErrors { fatal: <warning-diag>, warnings: [everything-so-far] }`) and returned via the error arm. The re-wrapping preserves the `Diagnostic.code` — the code still reads `COMP_W_0301`, not `COMP_E_0301`; only the carrier's discriminator changes. This is intentional: the error code remains a structural predicate for machine triage.

- **`Strict`.** Same as `FailOnWarning`, but the trigger fires on `Severity::Info` too.

### 5.4 Where the policy is applied

Warning-policy application is the LAST step on each stage's return path inside `SemStrait`: the stage-specific call returns its `Result<Output, StageErrors>`; `SemStrait` inspects the warnings slot; if the policy escalates, re-wraps into the stage's error carrier and returns `Err`; otherwise passes the original `Result` through unchanged. Stage implementations are unaware of `WarningPolicy` — the policy is strictly a boundary concern at `semstrait-api`.

### 5.5 Interaction with `compile_and_plan_and_adapt`

In the fused helper (`§7`), `WarningPolicy` applies at every stage boundary. If compile, plan, or adapt produces an escalation-eligible diagnostic, the helper returns `SemStraitError::{Compile,Plan,Adapt}Stage(...)` with warnings from prior stages preserved inside the stage's typed carrier. All stages clean → `Ok((artifact, w1 ++ w2 ++ w3))` concatenated in source-stage order.

### 5.6 Invariants

No policy changes a diagnostic's code, severity, or message. `FailOnWarning` surfacing `COMP_W_0301` differs from an intrinsic `COMP_E_0xxx` in carrier discriminator only — pattern-matching on codes remains a stable predicate for distinguishing escalated warnings from intrinsic errors.

---

## 6. `SemStraitError`

### 6.1 Purpose

`SemStraitError` is the single error type for the fused helpers (`compile_and_plan_and_adapt`) and the out-of-band I11b methods. Per-stage methods (`compile_*`, `plan`, `adapt`) preserve their native carrier types (`CompileErrors`, `PlanErrors`, `AdaptError`) to maintain structural parity with the owning `3x` docs.

### 6.2 Enum shape

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SemStraitError {
    #[error("parse stage failed")]     ParseStage(ParseErrors),
    #[error("validate stage failed")]  ValidateStage(ValidateErrors),
    #[error("compile stage failed")]   CompileStage(CompileErrors),
    #[error("plan stage failed")]      PlanStage(PlanErrors),
    #[error("adapt stage failed")]     AdaptStage(AdaptError),
    #[error("repository op failed")]   Repository(RepositoryError),
    #[error("catalog op failed")]      Catalog(CatalogError),
    #[error("builder invalid: missing {missing}")] BuilderInvalid { missing: &'static str },
    #[error("no repository configured")]            NoRepositoryConfigured,
}
```

Inner carriers come from `32` (`ParseErrors`, `ValidateErrors`), `33` (`CompileErrors`, `RepositoryError`), `34` (`PlanErrors`), `36` (`AdaptError`), and `37` (`CatalogError`). `thiserror` provides `Error` + summary `Display`; per-stage carrier holds the `Diagnostic`s.

### 6.3 No new subsystem prefix

`SemStraitError` does NOT introduce a new `API_E_*` subsystem prefix. The per-stage variants carry their own diagnostics with their own subsystem codes (`PARSE_E_*`, `VALID_E_*`, `COMP_E_*`, `PLAN_E_*`, `ADAPT_E_*`, `IO_E_*`, `CAT_E_*`). The two non-stage variants (`BuilderInvalid`, `NoRepositoryConfigured`) are structural failures of configuration; they carry diagnostics with code `COMP_E_0101` (name-resolution-class) for `NoRepositoryConfigured` and no code for `BuilderInvalid` (pre-compile configuration error). `Q-API-001` tracks whether to introduce an `API_E_*` range for these two.

### 6.4 Variant-to-subsystem map

| Variant               | Origin doc | Owning subsystem prefix  | Notes |
|-----------------------|------------|--------------------------|-------|
| `ParseStage`          | `32`       | `PARSE_*` / `EXPR_*`     | YAML syntax / expression DSL parse errors |
| `ValidateStage`       | `32`       | `VALID_*` / `EXPR_*`     | structural-preconditions failures |
| `CompileStage`        | `33`       | `COMP_*` / `EXPR_*`      | name / catalog / schema / binding resolution |
| `PlanStage`           | `34`       | `PLAN_*` / `OPT_*`       | planner + optimizer failures |
| `AdaptStage`          | `36`       | `ADAPT_*`                | adapter failures |
| `Repository`          | `33`       | `IO_*`                   | persistence I/O |
| `Catalog`             | `37`       | `CAT_*` / `FS_*`         | drift-check I/O |
| `BuilderInvalid`      | `38`       | (none — configuration)   | reported without a Diagnostic code |
| `NoRepositoryConfigured` | `38`    | (none — configuration)   | ditto |

The rightmost column is the subsystem you would grep for to find the diagnostic. A caller writing a log-router pattern-matches on the outer `SemStraitError` variant AND on the inner `Diagnostic.code`; neither alone is sufficient.

### 6.5 `IntoDiagnostic` impl

`SemStraitError` implements `IntoDiagnostic` per `30 §5.4`. Stage variants delegate to the inner carrier's `fatal.into_diagnostic()` (or direct `into_diagnostic` for `AdaptError` / `RepositoryError` / `CatalogError`); `BuilderInvalid` and `NoRepositoryConfigured` emit a `Diagnostic` with code `COMP_E_0101` (placeholder pending `Q-API-001`).

### 6.6 `StageOrigin`

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOrigin {
    Parse, Validate, Compile, Plan, Adapt, Repository, Catalog, Builder,
}

impl SemStraitError {
    pub fn origin(&self) -> StageOrigin;
}
```

Provided so callers can branch on origin without pattern-matching the whole enum (typical use: error messaging, log routing).

### 6.7 Stability

`SemStraitError` and `StageOrigin` are `#[non_exhaustive]`; variant additions are MINOR (`30 §4`). Inner carrier shapes inherit stability from their owning docs. `BuilderInvalid.missing` carries a `&'static str` matching a field name in `§4.3`; a new required field without default is MAJOR, with default is MINOR.

---

## 7. One-shot convenience

### 7.1 `compile_and_plan_and_adapt`

The fused pipeline entry:

```rust
pub async fn compile_and_plan_and_adapt(
    &self,
    yaml: &str,
    request: Request,
    adapter: &dyn EngineAdapter,
) -> Result<(EngineArtifact, Vec<Diagnostic>), SemStraitError>;
```

**Semantics.** Runs the full pipeline:

1. Parse+validate+compile (`compile_from_yaml` logic).
2. Plan+optimize (`plan` logic, internally synchronous).
3. Adapt (`adapt` logic, internally synchronous).

Async because step 1 is async (I11a); steps 2 and 3 execute synchronously inside the same future, polled to completion before the future yields. There is no `.await` point between step 2 and step 3 — callers observe this as a single async call that internally switches from async (I/O-bearing) to sync (hot-path) execution.

**Success arm.** Returns the final `EngineArtifact` and the accumulated warning vector in source-stage order (parse → validate → compile → plan → adapt). Empty vector if every stage was clean.

**Error arm.** Returns the first stage's fatal. Warnings produced up to that stage live inside the stage's typed carrier (`CompileErrors.warnings`, etc.) per `30 §7`'s fail-fast rule. The `SemStraitError` variant identifies which stage failed; the inner carrier identifies what exactly went wrong.

### 7.2 Why this fused helper exists

Without it, the caller writes:

```rust
let manifest = semstrait.compile_from_yaml(yaml).await.map_err(/* wrap */)?;
let plan = semstrait.plan(&manifest, request).map_err(/* wrap */)?;
let artifact = semstrait.adapt(adapter, &plan, &manifest).map_err(/* wrap */)?;
```

Three error-wrapping boilerplate sites. The fused helper centralizes the wrapping, so the caller's code becomes:

```rust
let (artifact, diagnostics) = semstrait
    .compile_and_plan_and_adapt(yaml, request, adapter)
    .await?;
```

One call site, one error mapping. `semstrait-facade` (`39`) builds on this pattern to offer a zero-configuration equivalent (its `compile_and_plan_and_adapt_default` takes only `(yaml, request, adapter)` and wires up a default `SemStrait`).

### 7.3 When NOT to use the fused helper

- **Streaming / incremental.** Callers reusing a `Manifest` across many requests prefer the compile-once pattern (`§8`).
- **Per-stage inspection.** Callers that need to inspect the `Manifest` (e.g. to write it to a cache, to log the schema, to emit a dataflow diagram) must use the per-stage methods.
- **Custom error routing.** Callers that want to route parse errors differently from adapt errors inspect the `SemStraitError` variant on `Err`, which is identical cost to using per-stage methods.

---

## 8. Streaming / incremental use

### 8.1 Compile-once, plan-many

The expected pattern for services answering many requests against a stable model:

```rust
// Once at startup:
let manifest: Arc<Manifest> = Arc::new(semstrait.compile_from_yaml(&yaml).await?);

// Per request:
let plan = semstrait.plan(&manifest, request)?;
let artifact = semstrait.adapt(adapter, &plan, &manifest)?;
```

`Manifest` is `Send + Sync` and cheap to `Arc`-share (`33 §7.6`). The `plan` + `adapt` hot path is synchronous (I6), so a service that holds an `Arc<Manifest>` answers requests on many threads without re-entering async.

### 8.2 Lifecycle expectations

- **Manifest freshness.** A `Manifest` captures a catalog snapshot at compile time (`33 §13.2`). Long-lived manifests SHOULD be periodically re-validated via `SemStrait::validate_manifest` (I11b); a `Breaking` `DriftStatus` means re-compile.
- **No implicit caching.** `SemStrait` does NOT cache manifests, plans, or artifacts. Callers that want caching hold `Arc<Manifest>` themselves.
- **Per-request construction.** `Request` (`31 §14`) is a value type; `SessionContext` may shift per request without forcing re-compile.
- **Fan-out.** `SemStrait` is `Send + Sync + Clone` (cheap `Arc`-clone); fan out via `Arc<SemStrait>` into worker tasks.

### 8.3 Compile-and-persist

```rust
let manifest = semstrait.compile_from_yaml(&yaml).await?;
let id = semstrait.save_manifest(&manifest).await?;
// Later, in a different process:
let manifest = semstrait.load_manifest(id).await?;
let plan = semstrait.plan(&manifest, request)?;
```

`ManifestId` round-trips; identical `(model, catalog snapshot)` pairs produce the same id (`33 §3.5`), so save/load is content-addressable.

### 8.4 Out of v1 scope

Incremental model mutation (re-compile on every change; `Q-API-005`) and streaming request iteration (caller-side loop in v1) are not supported by `SemStrait` directly.

---

## 9. Async boundaries

### 9.1 Per-method async posture

| Method                        | Async? | Why |
|-------------------------------|--------|-----|
| `builder()`                   | sync   | pure construction |
| `compile_from_yaml`           | async  | I11a — awaits `CatalogProvider` / `FileSystem` via `semstrait_manifest::compile` |
| `compile_from_model`          | async  | I11a — same as above, parse skipped |
| `plan`                        | sync   | I6 — planner hot path |
| `adapt`                       | sync   | I6 — adapter hot path |
| `compile_and_plan_and_adapt`  | async  | I11a — straddles compile; `.await` only inside the compile segment |
| `save_manifest`               | async  | I11b — awaits `Repository::save` |
| `load_manifest`               | async  | I11b — awaits `Repository::load` |
| `validate_manifest`           | async  | I11b — awaits `CatalogProvider::check_schema_drift` |
| accessors / utilities         | sync   | pure |

This matches `30 §9`'s per-crate posture row for `semstrait-api` (async at compile-time entry; sync at plan-time entry). The fused helper `compile_and_plan_and_adapt` is the only async method that internally contains synchronous stages; all other async methods are async end-to-end.

### 9.2 No `.await` in the hot path

`plan` and `adapt` are synchronous by signature and synchronous by implementation. No transitive `.await` point is reachable from them. This is enforced by:

- Method signatures (no `async fn` / no `-> impl Future<…>` / no `futures::executor::block_on`).
- `34`'s planner crate is entirely synchronous (`30 §9`).
- `36`'s adapter crate is entirely synchronous (`30 §9`).
- `semstrait-api` introduces no new async dependency inside the `plan` / `adapt` method bodies.

### 9.3 Runtime independence

Per `30 §9`, async surfaces are executor-agnostic. `semstrait-api`'s async methods do not pin a runtime; callers drive them on whatever executor they prefer. Bundled providers (`S3FileSystem`, `IcebergRestCatalogProvider`, `FileSystemRepository`) use `tokio` by convention, so most callers will have a `tokio` runtime in scope.

### 9.4 Fused helper: async → sync transition

Inside `compile_and_plan_and_adapt`, the last `.await` is in the compile step; `plan` and `adapt` execute synchronously on the same future. From the caller's perspective, a single async call internally transitions from I/O-bearing execution to pure CPU work once the Manifest lands. This is the I6 "hot path is synchronous" discipline surfaced at the fused-helper level.

### 9.5 Blocking in callers

Callers that cannot block a thread drive the async future on an appropriate executor. `semstrait-api` provides no `sync_*` variants; callers who need a sync-over-async bridge wrap with their runtime's `block_on`. See `Q-API-007`.

---

## 10. Diagnostic propagation

### 10.1 Stage-internal ordering

Within a single stage, diagnostics appear in **source order**:

- Parse / validate diagnostics are produced by visiting the YAML AST in lexical order (`32 §10`).
- Compile diagnostics are produced by visiting the `SemanticModel` in canonical BTreeMap order (`33 §13`) and, within each data-kind, in the order defined by the owning `20`–`25` spec.
- Plan / adapt diagnostics inherit their ordering from the planner's and adapter's internal traversal order, which is deterministic per I4.

### 10.2 Cross-stage ordering

The accumulated warning vector in `compile_and_plan_and_adapt`'s success arm is **stage-concatenated**:

```
[ parse_diags ... , validate_diags ... , compile_diags ... , plan_diags ... , adapt_diags ... ]
```

Within each stage block, source-order. Across stage blocks, pipeline order. No interleaving. This guarantees that two identical runs produce byte-identical diagnostic vectors (subject to non-determinism in the catalog — which `33 §13.2` requires to be stabilized into the Manifest).

### 10.3 De-duplication

`semstrait-api` does NOT de-duplicate diagnostics. Two reasons:

1. Duplication across stages is **informative**: a missing column reported once at validate and again at compile is a signal that the author saw the early error message but did not act on it, and the redundant later report helps downstream tooling present a consolidated view.
2. De-duplication requires a deterministic equivalence relation on `Diagnostic` (what makes two diagnostics equal? same code? same code + span? same code + span + message?) that has not been ratified in `30` and would need its own `Q-API-*` to land.

Callers that want de-duplicated output apply their own predicate to the returned `Vec<Diagnostic>`. A conservative default (same code + same `Span` = same diagnostic) is sketched in `Q-API-004`.

### 10.4 Severity ordering is not enforced

The accumulated vector may interleave `Info`, `Warning`, and `Error` severities in any order. Callers that want to surface errors first apply `.sort_by_key(|d| d.severity.sort_rank())` or an equivalent. `Severity` does not implement `Ord` in `31` (per `30 §5.2`, the enum is `#[non_exhaustive]`); a caller-supplied rank function is the idiomatic pattern.

### 10.5 Warning-policy escalation

When `WarningPolicy ∈ {FailOnWarning, Strict}` escalates a warning into a stage-fatal, the escalated diagnostic lands in the stage's typed `fatal` slot (`CompileErrors.fatal`, etc.). Prior warnings in the same stage are preserved in the stage's `warnings` slot. Warnings from prior stages are preserved in the stage's typed `warnings` slot as well, keeping the cross-stage history intact. See `§5.3` for the escalation mechanics.

### 10.6 Propagation example

Given per-stage output `parse=[]`, `validate=[VALID_W_2101]`, `compile=[COMP_I_2107, COMP_W_2101]`, `plan=[PLAN_W_2202]`, `adapt=[ADAPT_W_0301]`:

- `Accumulate` → `Ok((artifact, [VALID_W_2101, COMP_I_2107, COMP_W_2101, PLAN_W_2202, ADAPT_W_0301]))`.
- `FailOnWarning` → `Err(SemStraitError::ValidateStage(...))` with `fatal: VALID_W_2101`.
- `Strict` → identical to `FailOnWarning` here (no `Info` fires before the first `Warning`).

---

## 11. Stability

### 11.1 Crate-level stability

Per `30 §13`, `semstrait-api` is **Stable in v1**. The `SemStrait` / `SemStraitBuilder` method rosters, the `WarningPolicy` variants, the `SemStraitError` variant set, and the re-export list (`§2.2`) all participate in the workspace's MAJOR / MINOR / PATCH discipline.

### 11.2 Per-item stability

All `SemStrait` methods, `SemStraitBuilder` chain methods, `WarningPolicy`, `SemStraitError`, and `StageOrigin` are **Stable** in v1. `PipelineOutcome` is **Provisional** pending `Q-API-002`. Adding a new `SemStrait` method is MINOR (struct, not trait); adding a `SemStraitBuilder` field with a default is MINOR; a new required field is MAJOR. `#[non_exhaustive]` enums (`WarningPolicy`, `SemStraitError`, `StageOrigin`) admit MINOR variant additions.

### 11.3 Trait-object surfaces

The builder exposes `Arc<dyn CatalogProvider>`, `Arc<dyn FileSystem>`, `Arc<dyn Repository>`, `Box<dyn OptimizerPass>`. These traits are owned by `37` / `33` / `34`; MINOR additions to those traits (via default methods per `30 §11`) stay MINOR here. MAJORs there cascade to MAJOR here.

### 11.4 Const-fn factories

`SemStraitBuilder::new` is `const fn`. `SemStrait::builder` cannot be `const fn` (builder's `Vec::new()` and `Arc` usage are non-const); callers needing a compile-time builder hold a `SemStraitBuilder` in `const` directly.

### 11.5 Auto-trait posture

`SemStrait: Send + Sync + Clone` (`Arc::clone` on every field). `SemStraitBuilder: Send + Sync`, not `Clone` (consumed by `build`; multiple builders for multiple `SemStrait`s).

---

## 12. Crate boundaries

| Boundary                                         | Status                                                                 |
|--------------------------------------------------|------------------------------------------------------------------------|
| New domain types (`Resolved*`, `PlanNode`, `EngineArtifact` variants) | **NO.** Zero new types cross the boundary. |
| New subsystem prefix                             | **NO.** `SemStraitError` variants carry diagnostics with their owning subsystems' prefixes; no `API_*` prefix in v1. |
| SQL emission                                     | **NO.** `adapt` returns `EngineArtifact`; SQL comes from `EngineAdapter::emit` on the adapter. |
| Parse / validate / compile / plan / adapt algorithms | **NO.** All delegated to `32` / `33` / `34` / `36`. |
| Direct I/O                                       | **NO.** Every I/O call routes through `Arc<dyn CatalogProvider>`, `Arc<dyn FileSystem>`, `Arc<dyn Repository>`. No network, disk, or process-call lives inside `semstrait-api`. |
| Catalog-type branching (I3)                      | **NO.** Every dispatch over `&dyn CatalogProvider`; concrete type is not inspected. |
| Function-registry construction                   | **NO.** `function_registry()` from `31` is the sole source of the global registry; the builder's field accepts a pre-built `&'static FunctionRegistry` only. |
| Manifest mutation                                | **NO.** `Manifest` is sealed per `33 §7.4`. `SemStrait::save_manifest` takes `&Manifest` and produces a `ManifestId`; it does not mutate. |
| Plan mutation                                    | **NO.** `SemanticPlan` is `#[non_exhaustive]` but treated as immutable downstream of the planner. |
| Async runtime pinning                            | **NO.** `tokio`-neutral per `30 §9`. |
| Warning-policy enforcement at stages              | **NO.** Stages are unaware of `WarningPolicy`; enforcement is strictly at `semstrait-api`'s return boundary (`§5.4`). |
| Cross-stage dependency reasoning                 | **NO.** `SemStrait::plan` does not inspect the `Manifest`'s internal structure; it hands the `Manifest` + `Request` to the planner and consumes the result. |
| One-shot convenience                             | **YES.** `compile_and_plan_and_adapt` is owned here. |
| Warning aggregation at stage boundaries          | **YES.** `WarningPolicy` + diagnostic concatenation live here. |
| Builder ergonomics                               | **YES.** `SemStraitBuilder` is owned here. |
| Unified error type                               | **YES.** `SemStraitError` is owned here. |

The rightmost "YES" row is narrow: orchestration, warning policy, builder, unified error. Everything else is delegation.

---

## 13. Round-1 open items

The following drafting decisions are **defaulted** in this document but MUST be confirmed before ratification. All are captured in `docs/design/open_questions/38_open_questions.md`:

- **Q-API-001** — Should `SemStraitError::{BuilderInvalid, NoRepositoryConfigured}` carry a stable `API_E_*` code (new subsystem prefix) or re-use `COMP_E_0101` as currently drafted?
- **Q-API-002** — `PipelineOutcome`: should the fused helper return a dedicated `PipelineOutcome { artifact, diagnostics, per_stage_timings }` struct or stay on the current `(EngineArtifact, Vec<Diagnostic>)` tuple?
- **Q-API-003** — `WarningPolicy::FailOnWarning` — on escalation, which stage "owns" the escalated warning? Current default: the stage that produced it. Alternative: the API crate (all escalations appear as `SemStraitError::ApiEscalated`).
- **Q-API-004** — Diagnostic de-duplication policy. Current default: no de-duplication. Alternative: (code, span) pair is the identity and duplicates are folded.
- **Q-API-005** — Incremental-compile surface. Current default: not in v1. Alternative: `SemStrait::recompile_with_changes(&manifest, diff)` in a Round-2 MINOR.
- **Q-API-006** — Per-`SemStrait`-handle `FunctionRegistry`. Current default: the process-global registry is the only option. Alternative: builder accepts `&'static FunctionRegistry` constructed by a caller-supplied `RegistryBuilder` (interacts with `31 §5.5`).
- **Q-API-007** — Sync wrappers (`compile_from_yaml_blocking`) for non-async callers. Current default: not provided; callers bridge via `block_on`.
- **Q-API-008** — `SemStrait::builder` as `const fn`. Current default: non-`const` due to `Vec` / `Arc` in builder. Alternative: lazy field initialization that defers allocation to `build`.
- **Q-API-009** — `compile_and_plan_and_adapt` batch variant accepting `Vec<Request>`. Current default: not provided; callers loop over `plan` + `adapt`.
- **Q-API-010** — `validate_manifest` aggregation: max-severity (current) vs per-source `Vec<DriftReport>`. The per-source variant preserves more detail; the max-severity variant matches the caller policy in `37 §9.4`.
- **Q-API-011** — Whether `SemStrait` should expose `emit` directly (SQL emission convenience) or require callers to go through `EngineAdapter::emit`. Current default: no direct emit; callers use the adapter.

Each item is parked with arguments-for, arguments-against, and a next-step in `open_questions/38`.

---

## 14. Cross-references

- Overview: `00 §4.2 verbs`, `00 §5 pipeline`.
- Invariants: `00 §9 I3, I6, I7, I10, I11 (I11a + I11b), I12`.
- API contracts: `30 §4 (non-exhaustive policy)`, `30 §5–§6 (Diagnostic + error codes)`, `30 §7 (stage return shapes)`, `30 §8 (trait rules)`, `30 §9 (per-crate async matrix)`, `30 §10 (feature-flag policy)`, `30 §13 (stability tiers)`.
- Shared primitives: `31 §5 (FunctionRegistry)`, `31 §7 (Diagnostic)`, `31 §14 (Request / SessionContext)`.
- Parse / validate: `32 §9 (parse signature)`, `32 §11 (validate signature)`.
- Compile: `33 §9 (compile signature)`, `33 §10 (CompileErrors)`, `33 §11 (Repository)`, `33 §12 (I11b drift-check caller)`.
- Plan / optimize: `34 §*` (drafted in parallel; cross-refs here are speculative until `34` lands).
- IR types: `35 §3 (SemanticPlan)`, `35 §5 (EngineArtifact)`.
- Adapt: `36 §3 (EngineAdapter::adapt)`, `36 §9 (EngineArtifact / SqlArtifact shape)`, `36 §10 (AdaptError)`.
- Catalog / FileSystem: `37 §3 (CatalogProvider)`, `37 §5 (FileSystem)`, `37 §9 (I11b drift gate)`.
- Downstream: `39 (semstrait-facade)` — zero-config wrapper over this crate.

---

## 15. Round-1 ratifications

- §2.1 public-item roster and stability tier; §2.2 re-export set from `31`–`37`.
- §3.4 `SemStrait` method signatures (adapting `adapt` to include `&Manifest` per `36 §3.1`); §3.6 I6 / I10 / I11 / I12 invariants at method level.
- §4.3–§4.4 `SemStraitBuilder` required / optional fields and chain-method roster.
- §5.2 `WarningPolicy` three-variant set; §5.3–§5.4 escalation and boundary-application rules.
- §6.2 `SemStraitError` nine-variant set; §6.3 no new subsystem prefix (per-stage diagnostics carry their owning subsystem's code); §6.6 `StageOrigin` enum.
- §7.1 `compile_and_plan_and_adapt` signature and semantics.
- §8.1–§8.3 compile-once / plan-many and compile-and-persist lifecycles.
- §9.1–§9.2 per-method async posture matrix; no `.await` in the hot path.
- §10.1–§10.3 diagnostic ordering (source order within stage; pipeline order across stages; no v1 de-duplication).
- §11.2 per-item stability tier.
- §12 crate-boundary negatives (no new domain logic; no I/O beyond injected providers; no SQL; no catalog-type branching).

Numeric code literals in §6.5 for `BuilderInvalid` / `NoRepositoryConfigured` depend on `Q-API-001`. Shape, variant names, severity, and diagnostic discipline are ratified; only code digits may shift.
