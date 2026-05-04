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
  - "`SemStraitErrorKind` unified typed-error enum (per-stage variants; identification by variant identity per `30 §5`) and its `Diagnose` impl per `31 §3`"
  - "warning propagation across the parse → validate → compile → plan → adapt pipeline at the API-crate boundary"
  - "one-shot convenience contract (`compile_and_plan_and_adapt`)"
  - "streaming / incremental contract (compile-once, plan-many against a shared `SemanticManifest`)"
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
- **Unified error type.** `SemStraitErrorKind` (`§6`), a typed-kind enum that wraps each stage's `*ErrorKind` per `30 §5`. Identification by variant identity; no numeric subsystem prefix.
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
| `SemStraitErrorKind`         | enum          | yes                  | Stable         |
| `StageOrigin`                | enum          | yes                  | Stable         |
| `PipelineOutcome`            | struct        | yes                  | Provisional    |

Plus the re-exports (`§2.2`).

"Stability (v1)" follows `30 §13`. `semstrait-api` is **Stable in v1**; every struct and enum in its roster inherits that tier. `PipelineOutcome` is `Provisional` because its exact shape depends on ratification of Q-API-002 (one-shot return shape) during the next docs cycle.

### 2.2 Re-exports

The minimum set of types a caller needs to drive the pipeline end-to-end:

- From `31`: `Diagnostic` (i.e. `Diagnostic<K>`), `Diagnostics<K>`, `Diagnose`, `Severity`, `Request`, `SessionContext`.
- From `32`: `SemanticModel`, `ParseErrorKind`, `ValidateErrorKind`, `ModelBuildErrorKind`.
- From `33`: `SemanticManifest`, `SemanticManifestId`, `SemanticManifestMetadata`, `CompileErrorKind`, `Repository`, `RepositoryErrorKind`, `SemanticManifestLoadErrorKind`, `SemanticManifestDumpErrorKind`.
- From `34`: `PlanErrorKind`, `OptimizeErrorKind`.
- From `35`: `SemanticPlan`, `EngineArtifact`, `SqlArtifact`, `EnginePlan`, `IrErrorKind`.
- From `36`: `EngineAdapter`, `AdapterId`, `AdaptErrorKind`.
- From `37`: `CatalogProvider`, `FileSystem`, `DriftReport`, `CatalogProviderErrorKind`, `FileSystemErrorKind`.

Re-exports are additive (adding one is MINOR per `30 §2`); removing one is MAJOR. Consumer code SHOULD import through `semstrait_api::…` unless it has a reason to depend directly on an inner crate.

### 2.3 Module layout (informative)

```rust
pub mod builder;   // SemStrait, SemStraitBuilder
pub mod policy;    // WarningPolicy
pub mod error;     // SemStraitErrorKind, StageOrigin
pub mod outcome;   // PipelineOutcome

pub use builder::{SemStrait, SemStraitBuilder};
pub use policy::WarningPolicy;
pub use error::{SemStraitErrorKind, StageOrigin};
pub use outcome::PipelineOutcome;

pub use semstrait_core::{
    diagnostic::{Diagnostic, Diagnostics, Diagnose},
    Severity, Request, SessionContext,
};
pub use semstrait_model::{
    SemanticModel, ParseErrorKind, ValidateErrorKind, ModelBuildErrorKind,
};
pub use semstrait_manifest::{
    SemanticManifest, SemanticManifestId, SemanticManifestMetadata,
    CompileErrorKind, Repository, RepositoryErrorKind,
};
pub use semstrait_planner::{PlanErrorKind, OptimizeErrorKind};
pub use semstrait_ir::{SemanticPlan, EngineArtifact, SqlArtifact, EnginePlan, IrErrorKind};
pub use semstrait_adapter::{EngineAdapter, AdapterId, AdaptErrorKind};
pub use semstrait_catalog::{
    CatalogProvider, FileSystem, DriftReport,
    CatalogProviderErrorKind, FileSystemErrorKind,
};
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

    // Method-level error kind selection rule (Option A in §6.1):
    //   - Single-stage methods PASSTHROUGH the underlying crate's typed kind.
    //   - Multi-stage methods (those that orchestrate two or more stage entries)
    //     RETURN `SemStraitErrorKind` — the unified kind whose variants wrap the
    //     upstream `*ErrorKind`s via `From` impls (§6.3).
    // Callers that prefer the underlying surface for a multi-stage method
    // destructure `SemStraitErrorKind::Compile(CompileErrorKind::…)` etc.

    pub async fn compile_from_yaml(&self, yaml: &str)               // multi-stage: parse + validate + compile
        -> Result<
            (SemanticManifest, Diagnostics<SemStraitErrorKind>),
            (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
        >;
    pub async fn compile_from_model(&self, model: SemanticModel)    // multi-stage: validate + compile
        -> Result<
            (SemanticManifest, Diagnostics<SemStraitErrorKind>),
            (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
        >;

    pub fn plan(&self, manifest: &SemanticManifest, request: Request)  // multi-stage: plan + optimize
        -> Result<
            (SemanticPlan, Diagnostics<SemStraitErrorKind>),
            (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
        >;
    pub fn adapt(&self, adapter: &dyn EngineAdapter,                  // single-stage: passthrough
                 plan: &SemanticPlan, manifest: &SemanticManifest)
        -> Result<
            (EngineArtifact, Diagnostics<AdaptErrorKind>),
            (Diagnostic<AdaptErrorKind>, Diagnostics<AdaptErrorKind>),
        >;

    // The fused helper is the single site that unifies all per-stage kinds into
    // `SemStraitErrorKind` (§6). Warnings concatenate across stages on success
    // (§10.2). On failure, the originating stage's typed-kind is wrapped via
    // `From` impls (§6.3) into `SemStraitErrorKind`, with warnings observed up
    // to that stage preserved in the `Diagnostics<SemStraitErrorKind>` slot.
    pub async fn compile_and_plan_and_adapt(
        &self, yaml: &str, request: Request, adapter: &dyn EngineAdapter,
    ) -> Result<
        (EngineArtifact, Diagnostics<SemStraitErrorKind>),
        (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
    >;

    // I11b — out-of-band gated entries. Repository / catalog-provider transports
    // bring in their own typed kinds that fold into `SemStraitErrorKind` via
    // `From` impls (§6.3); `validate_manifest` aggregates drift across many
    // catalog sources and returns the unified kind for the same reason.
    pub async fn save_manifest(&self, manifest: &SemanticManifest)
        -> Result<
            (SemanticManifestId, Diagnostics<SemStraitErrorKind>),
            (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
        >;
    pub async fn load_manifest(&self, id: SemanticManifestId)
        -> Result<
            (SemanticManifest, Diagnostics<SemStraitErrorKind>),
            (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
        >;
    pub async fn validate_manifest(&self, manifest: &SemanticManifest)
        -> Result<DriftReport, Diagnostic<SemStraitErrorKind>>;

    pub fn warning_policy(&self) -> WarningPolicy;
    pub fn function_registry(&self) -> &'static FunctionRegistry;
    pub fn adapter_id_for<'a>(&'a self, adapter: &'a dyn EngineAdapter) -> AdapterId;
}
```

`adapt` takes `&SemanticManifest` because adapters reach into resolved bindings (`36 §3.1`); `compile_and_plan_and_adapt` is async only so it can straddle the compile I/O boundary. `save_manifest` / `load_manifest` require `self.repository.is_some()` — otherwise they fail-fast with a `Diagnostic<SemStraitErrorKind>` whose kind is `NoRepositoryConfigured` (§6.2).

### 3.4 Method contracts

- **`compile_from_yaml`** — `parse` (`32 §9.1`) → `validate` (`32 §9.4`) → `compile` (`33 §9`). Errors short-circuit per `30 §7`. Each per-stage failure is wrapped into `SemStraitErrorKind` via the `From<ParseErrorKind> | From<ValidateErrorKind> | From<CompileErrorKind>` impls in §6.3, preserving the inner `*ErrorKind` variant verbatim. Stage-internal warnings are re-keyed via the same `From` impls and accumulated in source-stage order. The first stage to fail short-circuits the chain; warnings observed up to that point ride alongside the fatal in the `Err((fatal, warnings))` tuple.
- **`compile_from_model`** — Skips parse; still runs validate + compile. For callers that synthesize `SemanticModel`s (or apply custom post-parse transforms) and want the API crate to perform validation before compile. Same wrapping discipline as `compile_from_yaml`, minus the `Parse(...)` variant.
- **`plan`** — Runs `semstrait_planner::plan(&SemanticManifest, Request)` (canonical 2-arg signature per `34 §6.1`), then applies optimization via `34 §12.5`'s `OptimizerBuilder` (`OptimizerBuilder::new().with(self.optimizer_passes.clone()).build().apply(...)` when the builder slot is non-empty; otherwise `Optimizer::with_v1_passes()` per `34 §11.2`). The function-registry handle is consumed by the planner via the process-global `function_registry()` (`31 §5.2`) — not threaded as a per-call argument. `self.warning_policy` is applied to the returned warnings on success. `PlanErrorKind` and `OptimizeErrorKind` are wrapped into `SemStraitErrorKind` via `From` impls (§6.3); the optimizer pass that produced an `OptimizeErrorKind` is identifiable via the inner kind's payload.
- **`adapt`** — Single-stage delegate over `adapter.adapt(plan, manifest)` (`36 §3.1`). Returns `Diagnostic<AdaptErrorKind>` directly — same surface a caller would observe using `semstrait-adapter` as a free crate. The fused helper (§7) re-wraps via `From<AdaptErrorKind> for SemStraitErrorKind` (§6.3) when needed.
- **`compile_and_plan_and_adapt`** — Runs compile → plan → adapt. Every per-stage failure folds into `SemStraitErrorKind` via the §6.3 `From` impls. Warnings concatenate across stages in source order (`§10.2`). On error, the first failing stage's typed-kind is wrapped into a `Diagnostic<SemStraitErrorKind>` with warnings up to that point preserved in the `Diagnostics<SemStraitErrorKind>` slot of the `Err` tuple.
- **`save_manifest` / `load_manifest`** — Wrap `Repository::save` / `Repository::load` (`33 §11`). Require `self.repository.is_some()`; otherwise return `Err((Diagnostic::error(SemStraitErrorKind::NoRepositoryConfigured), Diagnostics::empty()))`. Underlying `RepositoryErrorKind`s fold into `SemStraitErrorKind::Repository(...)` via §6.3.
- **`validate_manifest`** — Iterates resolved physical sources (`33 §8`), calls `CatalogProvider::check_schema_drift` (`37 §9`) per source, aggregates under `Unchanged < Compatible < Breaking`. Per-source `DriftKind` details concatenate. Returns the simple `Result<DriftReport, Diagnostic<SemStraitErrorKind>>` shape since the call produces no advisory warnings; `CatalogProviderErrorKind`s fold into `SemStraitErrorKind::CatalogProvider(...)` via §6.3.

### 3.5 Invariants

| Ref | Invariant |
|-----|-----------|
| I6  | `plan`, `adapt`, and every accessor on `SemStrait` are synchronous. No `.await` point is reachable from these methods. |
| I7  | Depends on `31`–`37`; depended on by `39`. No cycles. |
| I10 | `WarningPolicy`, `SemStraitErrorKind`, `StageOrigin`, `PipelineOutcome` are `#[non_exhaustive]`. |
| I11 | `compile_*` is async (I11a); `save_manifest`, `load_manifest`, `validate_manifest` are async (I11b); no other method is async. |
| I12 | Errors are typed-kind enums per `30 §5` / `31 §3` (`ParseErrorKind`, `ValidateErrorKind`, `CompileErrorKind`, `PlanErrorKind`, `OptimizeErrorKind`, `AdaptErrorKind`, `RepositoryErrorKind`, `CatalogProviderErrorKind`, `FileSystemErrorKind`, `IrErrorKind`). The fused helper unifies these via `From` impls into `SemStraitErrorKind`; no numeric subsystem prefix exists in v1. Per-stage observability flows through `tracing` per `30 §6`; library code never writes to stdout/stderr. |

### 3.6 Observability via `tracing`

`SemStrait`'s methods emit structured `tracing` events and spans for the
work they orchestrate; informational signals never reach the
`Diagnostic<K>` channel (`30 §6`).

**What this crate emits.** The orchestrator opens one `tracing::span!` per
public-method invocation and a child span per delegated stage (parse,
validate, compile, plan, optimize, adapt). Stage spans carry stable
fields: `stage` (one of `parse | validate | compile | plan | optimize |
adapt | repository | catalog_provider | file_system`), `model_revision`
when reachable from the inputs, and `request_id` when the `Request`
carries one (`31 §14`). Inside each stage span the owning crate emits
its own events — `semstrait-api` adds none of its own beyond span
boundaries.

**Levels.**

| Level     | What rides on it                                                              |
|-----------|--------------------------------------------------------------------------------|
| `error`   | `Severity::Error` diagnostics that were just produced, mirrored as a tracing event with the kind variant in a structured field. The diagnostic itself is still returned in the `Result`; `tracing` is for the embedder's logging pipeline. |
| `warn`    | `Severity::Warning` diagnostics, mirrored as above. Mirroring is opt-out at the embedder via filter. |
| `info`    | Stage transitions: "compile starting", "compile finished in 12 ms", "loaded manifest from repository". One event per stage entry / exit. |
| `debug`   | Per-step granularity inside each stage: "resolving dataset `orders`", "binding column `customer_id`". Most of these come from `33`/`34`/`36`, not from `38` itself. |
| `trace`   | Fine-grained traversal: per-row, per-binding, per-rewrite-rule events. Reserved for diagnosing pipeline internals. |

**Tracing is opt-in.** No subscriber is installed by `semstrait-api`.
Embedders configure `tracing-subscriber` to route events to their
preferred sink. The library never writes to stdout / stderr; the only
writers are the embedder's subscriber and the `Diagnostic<K>` carriers
returned in `Result`. This matches the discipline established in
`30 §6` and the per-crate I12 invariants.

**CLI convention.** When a binary embedder (e.g. `semstrait-cli`,
explorer harnesses, examples) maps verbosity flags onto `tracing` levels,
the canonical mapping is:

| Flag          | `RUST_LOG` equivalent | Stages emit                                  |
|---------------|------------------------|-----------------------------------------------|
| (none)        | `warn`                 | `error` + `warn` (default; quiet operation)   |
| `--info`      | `info`                 | `error` + `warn` + `info` (stage transitions) |
| `--debug`     | `debug`                | adds per-step events                          |
| `--trace`     | `trace`                | adds fine-grained per-binding / per-row events|

The flag set is documented as a recommendation, not an enforced
contract: `semstrait-api` is a library and does not provide a CLI.
Embedders are free to expose their own knobs, but ratifying this
mapping at the spec level keeps multiple front-ends consistent.

**Relation to `WarningPolicy`.** Tracing mirroring is independent of
`WarningPolicy`. The diagnostic vector is the canonical record;
`tracing` is a side-channel for embedder visibility. Setting
`WarningPolicy::FailOnWarning` does not suppress the `warn`-level
mirror — the embedder still sees the mirrored event, alongside the
`Err((fatal, warnings))` tuple returned to the caller.

---

## 4. `SemStraitBuilder`

### 4.1 Purpose

`SemStraitBuilder` is the single construction path for `SemStrait`. Chain-style configuration methods populate its fields; `build` produces a validated `SemStrait` (or fails with a `Diagnostic<SemStraitErrorKind>` whose kind is `BuilderInvalid`, §6.2).

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
| `catalog_provider`  | yes       | — (build fails with `SemStraitErrorKind::BuilderInvalid`)                   |
| `file_system`       | yes       | — (build fails with `SemStraitErrorKind::BuilderInvalid`)                   |
| `repository`        | no        | `None` — save/load methods then return `SemStraitErrorKind::NoRepositoryConfigured` |
| `function_registry` | no        | `semstrait_core::function_registry()` (the process-global instance) |
| `optimizer_passes`  | no        | the canonical-pass list from `34 §*` (`Vec::new()` semantically means "use defaults", not "no passes") |
| `warning_policy`    | no        | `WarningPolicy::Accumulate` (`§5.2`)                    |

`catalog_provider` and `file_system` are required because every non-trivial compile invocation needs both, and silently defaulting either to `NoopCatalogProvider` / `LocalFileSystem` would mask configuration errors. Callers who want a truly zero-I/O build (unit tests, model-only work) construct an explicit `NoopCatalogProvider` + an explicit `LocalFileSystem` pointing at an empty root.

### 4.4 Chain methods

```rust
impl SemStraitBuilder {
    pub fn new() -> Self;            // all None / empty / default
    pub fn with_catalog_provider(self, cp: Arc<dyn CatalogProvider>) -> Self;
    pub fn with_file_system(self, fs: Arc<dyn FileSystem>) -> Self;
    pub fn with_repository(self, repo: Arc<dyn Repository>) -> Self;
    pub fn with_function_registry(self, reg: &'static FunctionRegistry) -> Self;
    pub fn with_optimizer_pass(self, pass: Box<dyn OptimizerPass>) -> Self;
    pub fn with_optimizer_passes(self, passes: Vec<Box<dyn OptimizerPass>>) -> Self;
    pub fn with_warning_policy(self, policy: WarningPolicy) -> Self;
    pub fn build(self) -> Result<SemStrait, Diagnostic<SemStraitErrorKind>>;
}

impl Default for SemStraitBuilder {
    fn default() -> Self { Self::new() }
}
```

Setters use the Rust idiomatic `with_*` prefix to clearly signal "consume self, return modified self" — the chain-builder convention shared with `reqwest::ClientBuilder` / `tokio::runtime::Builder` / `clap::Command`. `with_optimizer_pass` appends; `with_optimizer_passes` replaces. The field names on `SemStraitBuilder` and `SemStrait` themselves stay un-prefixed (`catalog_provider`, `file_system`, …) — `with_` is a setter convention, not a field convention.

### 4.6 Validation at `build`

`build` checks required fields (`catalog_provider`, `file_system`) and fills unset optional fields with their defaults (`§4.3`). No I/O runs at build time: a builder with an unreachable catalog builds successfully and the first `compile_*` call surfaces the connectivity failure.

```rust
pub fn build(self) -> Result<SemStrait, Diagnostic<SemStraitErrorKind>>;
```

On missing required fields: `Err(Diagnostic::error(SemStraitErrorKind::BuilderInvalid { missing }))`. `build` is a one-shot configuration check, not a stage entry-point — it doesn't produce warnings, so the simpler `Result<_, Diagnostic<K>>` shape is preferable to the fail-fast tuple.

### 4.7 `const fn` constructors

`SemStraitBuilder::new` is `const fn` (all fields initialize to `None` / `Vec::new()` / `WarningPolicy::Accumulate`), letting callers declare builders in `static` / `const` items. Chain methods accepting `Arc<dyn …>` cannot be `const`; `with_warning_policy` could in principle be made `const fn` in a future MINOR.

---

## 5. `WarningPolicy`

### 5.1 Purpose

The semstrait pipeline produces two diagnostic severities: `Severity::Error` (always fatal at its owning stage per `30 §7`) and `Severity::Warning` (advisory, preserved alongside success per `30 §7`). Informational signals — progress, telemetry, debug detail — flow through `tracing` (`30 §6`) and are not on the `Diagnostic<K>` channel. `WarningPolicy` is the knob for callers that want to escalate the advisory channel into a fatal at the API boundary.

### 5.2 Variants

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningPolicy {
    Accumulate,    // default — Severity::Warning passes through to the success arm's warnings slot
    FailOnWarning, // first Severity::Warning at any stage escalates to that stage's fatal
    Strict,        // reserved alias of FailOnWarning until Severity grows a third variant (see §5.3)
}
```

`#[non_exhaustive]` so future policies (e.g. `FailOnWarningAfter(Stage)`, `FilterByKind(&[&'static str])`) extend via MINOR (`30 §4`). `Default::default()` returns `Accumulate`.

### 5.3 Escalation rules

- **`Accumulate`.** No-op. Every `Severity::Warning` flows through to the success arm's `Diagnostics<K>` slot. `Severity::Error` is handled by the owning stage's fail-fast policy (`30 §7`); warnings produced up to the failure still accumulate in the `Err((fatal, warnings))` tuple. `Severity::Info` is not on the diagnostic channel — informational signals flow through `tracing` per `30 §6` (see §3.6 below).

- **`FailOnWarning`.** Any `Diagnostic<K> { severity: Warning, .. }` produced by a stage is re-emitted as the stage's fail-fast `Err((fatal, warnings))` tuple — the warning that triggered escalation lands in the `fatal` slot (kind preserved); warnings observed up to that point ride alongside in the `warnings` slot. Variant identity of the underlying `*ErrorKind` is preserved across the escalation: pattern-matching on the kind variant remains a stable predicate for machine triage.

- **`Strict`.** Same as `FailOnWarning`. There is no separate `Severity::Info` to trigger on — `Severity` was reduced to `{Error, Warning}` per `30 §5.2` and informational signals moved to `tracing`. Retained as a name for forward compatibility (it currently behaves identically to `FailOnWarning`); a future `Severity::Note` variant could differentiate.

### 5.4 Where the policy is applied

Warning-policy application is the LAST step on each stage's return path inside `SemStrait`: the stage-specific call returns its `Result<Output, StageErrors>`; `SemStrait` inspects the warnings slot; if the policy escalates, re-wraps into the stage's error carrier and returns `Err`; otherwise passes the original `Result` through unchanged. Stage implementations are unaware of `WarningPolicy` — the policy is strictly a boundary concern at `semstrait-api`.

### 5.5 Interaction with `compile_and_plan_and_adapt`

In the fused helper (`§7`), `WarningPolicy` applies at every stage boundary. If compile, plan, or adapt produces an escalation-eligible diagnostic, the helper returns `Err((Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>))` — the originating stage's typed-kind is wrapped into `SemStraitErrorKind` via `From` (§6.4); warnings from prior stages are preserved in the `Diagnostics<SemStraitErrorKind>` slot in source-stage order. All stages clean → `Ok((artifact, w1 ++ w2 ++ w3))` concatenated in source-stage order.

### 5.6 Invariants

No policy changes a diagnostic's kind variant, severity, or rendered message. `FailOnWarning` escalating a `CompileErrorKind::SchemaInferenceClamped` (Severity::Warning) into the fail-fast `fatal` slot keeps the `kind` variant unchanged; the only difference is the slot it occupies in the `Result` tuple. Pattern-matching on the underlying `*ErrorKind` variant remains a stable predicate for distinguishing escalated warnings from intrinsic errors — consumers may additionally inspect `Diagnose::severity()` to confirm the original severity was Warning.

---

## 6. `SemStraitErrorKind`

> **Migration note.** Prior drafts of this document used a `SemStraitError`
> enum that wrapped per-stage *Errors* carriers (`ParseErrors`, `ValidateErrors`,
> `CompileErrors`, `PlanErrors`, `AdaptError`, `RepositoryError`, `CatalogError`).
> Those carriers and the wrapping enum are **retired** as part of the
> workspace-wide typed-kind transition (`30 §5` / `31 §3`). The replacement
> is `SemStraitErrorKind`: a typed-kind enum whose variants carry the
> upstream `*ErrorKind` enums directly, with `From` impls for ergonomic `?`
> propagation. The unified shape is keyed by variant identity, not by a
> numeric subsystem prefix; consumers route on variant matching.

### 6.1 Purpose

`SemStraitErrorKind` is the unified typed-error kind for every
`SemStrait` method whose body crosses two or more stage boundaries —
`compile_from_yaml`, `compile_from_model`, `plan` (planner + optimizer),
`compile_and_plan_and_adapt`, `save_manifest`, `load_manifest`,
`validate_manifest`. The single-stage delegate `SemStrait::adapt`
preserves the native `AdaptErrorKind` so callers using just adapt see
the same surface they would using `semstrait-adapter` directly.

This **multi-stage methods unify; single-stage methods passthrough**
rule is the §3.3 method-roster contract. The `From` impls in §6.3 are
the wrapping mechanism — every wrapping site is a single `?` at a stage
boundary inside the orchestrator. No per-stage helper carriers
(retired with the migration note above) and no upstream `*ErrorKind`
mutation: each stage's kind stays owned by its crate, and `38` only
adds a sum on top.

### 6.2 Enum shape

```rust
/// Unified typed-error kind for `semstrait-api` orchestration entry points.
/// Identification by variant identity per `30 §5`. Variants wrap upstream
/// `*ErrorKind` enums; severity propagates through `Diagnose::severity()`
/// of the inner kind, except for the configuration variants
/// (`BuilderInvalid`, `NoRepositoryConfigured`) which are intrinsic to `38`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SemStraitErrorKind {
    // -- Stage kinds (wrappers over upstream *ErrorKind) --
    Parse              (ParseErrorKind),
    Validate           (ValidateErrorKind),
    Compile            (CompileErrorKind),
    Plan               (PlanErrorKind),
    Optimize           (OptimizeErrorKind),
    Adapt              (AdaptErrorKind),

    // -- Transport kinds (also wrappers; surfaced at I11b call sites) --
    Repository         (RepositoryErrorKind),
    CatalogProvider    (CatalogProviderErrorKind),
    FileSystem         (FileSystemErrorKind),

    // -- Configuration kinds (intrinsic to `38`) --
    /// `SemStraitBuilder::build` was called without a required field;
    /// `missing` is the field name (matching the chain-method name minus
    /// `with_`).
    BuilderInvalid     { missing: &'static str },

    /// A repository-bound method (`save_manifest` / `load_manifest`) was
    /// called on a `SemStrait` whose builder did not configure a
    /// `Repository`.
    NoRepositoryConfigured,
}

impl semstrait_core::diagnostic::Diagnose for SemStraitErrorKind {
    fn message(&self) -> std::borrow::Cow<'_, str> {
        use SemStraitErrorKind::*;
        match self {
            Parse(k)            => k.message(),
            Validate(k)         => k.message(),
            Compile(k)          => k.message(),
            Plan(k)             => k.message(),
            Optimize(k)         => k.message(),
            Adapt(k)            => k.message(),
            Repository(k)       => k.message(),
            CatalogProvider(k)  => k.message(),
            FileSystem(k)       => k.message(),
            BuilderInvalid { missing } =>
                format!("builder invalid: missing required field {missing}").into(),
            NoRepositoryConfigured =>
                "no repository configured on this SemStrait".into(),
        }
    }
    fn severity(&self) -> semstrait_core::Severity {
        use SemStraitErrorKind::*;
        match self {
            Parse(k)            => k.severity(),
            Validate(k)         => k.severity(),
            Compile(k)          => k.severity(),
            Plan(k)             => k.severity(),
            Optimize(k)         => k.severity(),
            Adapt(k)            => k.severity(),
            Repository(k)       => k.severity(),
            CatalogProvider(k)  => k.severity(),
            FileSystem(k)       => k.severity(),
            BuilderInvalid { .. } | NoRepositoryConfigured =>
                semstrait_core::Severity::Error,
        }
    }
}
```

### 6.3 `From` impls

```rust
impl From<ParseErrorKind>            for SemStraitErrorKind { /* Parse(k)            */ }
impl From<ValidateErrorKind>         for SemStraitErrorKind { /* Validate(k)         */ }
impl From<CompileErrorKind>          for SemStraitErrorKind { /* Compile(k)          */ }
impl From<PlanErrorKind>             for SemStraitErrorKind { /* Plan(k)             */ }
impl From<OptimizeErrorKind>         for SemStraitErrorKind { /* Optimize(k)         */ }
impl From<AdaptErrorKind>            for SemStraitErrorKind { /* Adapt(k)            */ }
impl From<RepositoryErrorKind>       for SemStraitErrorKind { /* Repository(k)       */ }
impl From<CatalogProviderErrorKind>  for SemStraitErrorKind { /* CatalogProvider(k)  */ }
impl From<FileSystemErrorKind>       for SemStraitErrorKind { /* FileSystem(k)       */ }
```

The fused helper relies on these impls so per-stage `?`-propagation lands
on the right `SemStraitErrorKind` variant without explicit conversion at
each call site. Each `From` is **lossless**: variant identity of the inner
kind is preserved; consumers that need the inner kind for fine-grained
matching destructure `SemStraitErrorKind::Compile(CompileErrorKind::…)`.

### 6.4 Variant-to-origin map

| Variant                  | Origin doc | Inner `*ErrorKind`            | Notes |
|--------------------------|------------|-------------------------------|-------|
| `Parse`                  | `32`       | `ParseErrorKind`              | YAML / expression-DSL parse errors |
| `Validate`               | `32`       | `ValidateErrorKind`           | structural-preconditions failures |
| `Compile`                | `33`       | `CompileErrorKind`            | name / catalog / schema / binding resolution |
| `Plan`                   | `34`       | `PlanErrorKind`               | planner failures |
| `Optimize`               | `34`       | `OptimizeErrorKind`           | optimizer-pass failures |
| `Adapt`                  | `36`       | `AdaptErrorKind`              | adapter / emission failures |
| `Repository`             | `33`       | `RepositoryErrorKind`         | persistence I/O |
| `CatalogProvider`        | `37`       | `CatalogProviderErrorKind`    | catalog-provider transport / drift |
| `FileSystem`             | `37`       | `FileSystemErrorKind`         | filesystem transport |
| `BuilderInvalid`         | `38`       | (none — configuration)        | builder missing required field |
| `NoRepositoryConfigured` | `38`       | (none — configuration)        | I11b call without `Repository` injected |

A caller writing a log-router pattern-matches first on the outer
`SemStraitErrorKind` variant (origin) and then, where needed, on the inner
`*ErrorKind` variant (specific failure). Neither alone is sufficient.

### 6.5 `Diagnose` impl semantics

`SemStraitErrorKind` implements `Diagnose` per `31 §3`. The wrapper variants
delegate `message()` and `severity()` to the inner kind so the rendered
diagnostic is identical regardless of whether a caller observes the inner
kind directly (per-stage method) or unified through `SemStraitErrorKind`
(fused helper). Configuration variants (`BuilderInvalid`,
`NoRepositoryConfigured`) render their own messages and report
`Severity::Error`.

The retired `IntoDiagnostic` trait of earlier drafts is not used; the
typed-kind discipline emits `Diagnostic<K>` directly via the construction
helpers in `31 §3.4` (e.g. `Diagnostic::error(kind)`,
`Diagnostic::warning(kind).with_location(loc)`).

### 6.6 `StageOrigin`

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOrigin {
    Parse, Validate, Compile, Plan, Optimize, Adapt,
    Repository, CatalogProvider, FileSystem, Builder,
}

impl SemStraitErrorKind {
    pub fn origin(&self) -> StageOrigin;
}
```

Provided so callers can branch on origin without pattern-matching the
whole enum (typical use: error messaging, log routing). `Optimize` /
`CatalogProvider` / `FileSystem` are added to match the variant set above.

### 6.7 Stability

`SemStraitErrorKind` and `StageOrigin` are `#[non_exhaustive]`; variant
additions are MINOR (`30 §2.1`). Inner kind shapes inherit stability from
their owning docs (renames MAJOR; field additions on `#[non_exhaustive]`
variant payloads MINOR). `BuilderInvalid.missing` carries a `&'static str`
matching a field name in `§4.3`; a new required field without default is
MAJOR; with a default is MINOR.

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
) -> Result<
    (EngineArtifact, Diagnostics<SemStraitErrorKind>),
    (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>),
>;
```

**Semantics.** Runs the full pipeline:

1. Parse+validate+compile (`compile_from_yaml` logic).
2. Plan+optimize (`plan` logic, internally synchronous).
3. Adapt (`adapt` logic, internally synchronous).

Async because step 1 is async (I11a); steps 2 and 3 execute synchronously inside the same future, polled to completion before the future yields. There is no `.await` point between step 2 and step 3 — callers observe this as a single async call that internally switches from async (I/O-bearing) to sync (hot-path) execution.

**Success arm.** Returns the final `EngineArtifact` and the accumulated warning vector in source-stage order (parse → validate → compile → plan → adapt). Empty vector if every stage was clean.

**Error arm.** Returns `Err((Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>))`. The fatal is the first stage's failure, with its kind wrapped via `From` into `SemStraitErrorKind` (§6.3); warnings produced up to that point live in the `Diagnostics<SemStraitErrorKind>` slot per `30 §7`. The outer `SemStraitErrorKind` variant identifies which stage failed; destructuring the inner `*ErrorKind` identifies what exactly went wrong.

### 7.3 When NOT to use the fused helper

- **Streaming / incremental.** Callers reusing a `SemanticManifest` across many requests prefer the compile-once pattern (`§8`).
- **Per-stage inspection.** Callers that need to inspect the `SemanticManifest` (e.g. to write it to a cache, to log the schema, to emit a dataflow diagram) must use the per-stage methods.
- **Custom error routing.** Callers that want to route parse errors differently from adapt errors inspect the `SemStraitErrorKind` variant on `Err`, which is identical cost to using per-stage methods.

---

## 8. Streaming / incremental use

### 8.1 Compile-once, plan-many

The expected pattern for services answering many requests against a stable model:

```rust
// Once at startup:
let (manifest, _build_warnings) = semstrait.compile_from_yaml(&yaml).await?;
let manifest: Arc<SemanticManifest> = Arc::new(manifest);

// Per request:
let (plan, _plan_warnings) = semstrait.plan(&manifest, request)?;
let (artifact, _adapt_warnings) = semstrait.adapt(adapter, &plan, &manifest)?;
```

`SemanticManifest` is `Send + Sync` and cheap to `Arc`-share (`33 §7.6`). The `plan` + `adapt` hot path is synchronous (I6), so a service that holds an `Arc<SemanticManifest>` answers requests on many threads without re-entering async.

### 8.2 Lifecycle expectations

- **SemanticManifest freshness.** A `SemanticManifest` captures a catalog snapshot at compile time (`33 §13.2`). Long-lived manifests SHOULD be periodically re-validated via `SemStrait::validate_manifest` (I11b); a `Breaking` `DriftStatus` means re-compile.
- **No implicit caching.** `SemStrait` does NOT cache manifests, plans, or artifacts. Callers that want caching hold `Arc<SemanticManifest>` themselves.
- **Per-request construction.** `Request` (`31 §14`) is a value type; `SessionContext` may shift per request without forcing re-compile.
- **Fan-out.** `SemStrait` is `Send + Sync + Clone` (cheap `Arc`-clone); fan out via `Arc<SemStrait>` into worker tasks.

### 8.3 Compile-and-persist

```rust
let (manifest, _w_compile) = semstrait.compile_from_yaml(&yaml).await?;
let (id, _w_save)          = semstrait.save_manifest(&manifest).await?;
// Later, in a different process:
let (manifest, _w_load)    = semstrait.load_manifest(id).await?;
let (plan, _w_plan)        = semstrait.plan(&manifest, request)?;
```

`SemanticManifestId` round-trips; identical `(model, catalog snapshot)` pairs produce the same id (`33 §3.5`), so save/load is content-addressable.

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

Inside `compile_and_plan_and_adapt`, the last `.await` is in the compile step; `plan` and `adapt` execute synchronously on the same future. From the caller's perspective, a single async call internally transitions from I/O-bearing execution to pure CPU work once the SemanticManifest lands. This is the I6 "hot path is synchronous" discipline surfaced at the fused-helper level.

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

Within each stage block, source-order. Across stage blocks, pipeline order. No interleaving. This guarantees that two identical runs produce byte-identical diagnostic vectors (subject to non-determinism in the catalog — which `33 §13.2` requires to be stabilized into the SemanticManifest).

### 10.3 De-duplication

`semstrait-api` does NOT de-duplicate diagnostics. Two reasons:

1. Duplication across stages is **informative**: a missing column reported once at validate and again at compile is a signal that the author saw the early error message but did not act on it, and the redundant later report helps downstream tooling present a consolidated view.
2. De-duplication requires a deterministic equivalence relation on `Diagnostic<K>` (what makes two diagnostics equal? same kind variant? same kind + location? same kind + location + rendered message?) that has not been ratified in `30` and would need its own `Q-API-*` to land.

Callers that want de-duplicated output apply their own predicate to the returned `Diagnostics<SemStraitErrorKind>`. A conservative default (matching `kind` variant + matching `location.span()` = same diagnostic) is sketched in `Q-API-004`.

### 10.4 Severity ordering is not enforced

The accumulated vector may interleave `Severity::Warning` and `Severity::Error` diagnostics in any order — `Severity` is reduced to `{Error, Warning}` in `30 §5.2`, with informational signals on `tracing` per `30 §6`. Callers that want to surface errors first apply `.sort_by_key(|d| d.severity().sort_rank())` or an equivalent. `Severity` does not implement `Ord` in `31` (per `30 §5.2`, the enum is `#[non_exhaustive]`); a caller-supplied rank function is the idiomatic pattern.

### 10.5 Warning-policy escalation

When `WarningPolicy ∈ {FailOnWarning, Strict}` escalates a warning into a stage-fatal, the escalated `Diagnostic<K>` (variant identity preserved) lands in the `Err` tuple's fatal slot. Prior warnings in the same stage are preserved in the same tuple's warnings slot. Warnings from prior stages travel forward in the helper's `Diagnostics<SemStraitErrorKind>` slot (re-keyed via the `From` impls of §6.3), keeping the cross-stage history intact. See `§5.3` for the escalation mechanics.

## 11. Stability

### 11.1 Crate-level stability

Per `30 §13`, `semstrait-api` is **Stable in v1**. The `SemStrait` / `SemStraitBuilder` method rosters, the `WarningPolicy` variants, the `SemStraitErrorKind` variant set, and the re-export list (`§2.2`) all participate in the workspace's MAJOR / MINOR / PATCH discipline.

### 11.2 Per-item stability

All `SemStrait` methods, `SemStraitBuilder` chain methods, `WarningPolicy`, `SemStraitErrorKind`, and `StageOrigin` are **Stable** in v1. `PipelineOutcome` is **Provisional** pending `Q-API-002`. Adding a new `SemStrait` method is MINOR (struct, not trait); adding a `SemStraitBuilder` field with a default is MINOR; a new required field is MAJOR. `#[non_exhaustive]` enums (`WarningPolicy`, `SemStraitErrorKind`, `StageOrigin`) admit MINOR variant additions, with the same MINOR/MAJOR rules cascading from upstream `*ErrorKind`s reachable via the wrapping variants.

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
| New per-crate `*ErrorKind`                       | **NO.** `SemStraitErrorKind` is a thin sum over upstream kinds plus two intrinsic API-only variants (`BuilderInvalid`, `NoRepositoryConfigured`); no parallel kind taxonomy. |
| SQL emission                                     | **NO.** `adapt` returns `EngineArtifact`; SQL comes from `EngineAdapter::emit` on the adapter. |
| Parse / validate / compile / plan / adapt algorithms | **NO.** All delegated to `32` / `33` / `34` / `36`. |
| Direct I/O                                       | **NO.** Every I/O call routes through `Arc<dyn CatalogProvider>`, `Arc<dyn FileSystem>`, `Arc<dyn Repository>`. No network, disk, or process-call lives inside `semstrait-api`. |
| Catalog-type branching (I3)                      | **NO.** Every dispatch over `&dyn CatalogProvider`; concrete type is not inspected. |
| Function-registry construction                   | **NO.** `function_registry()` from `31` is the sole source of the global registry; the builder's field accepts a pre-built `&'static FunctionRegistry` only. |
| SemanticManifest mutation                                | **NO.** `SemanticManifest` is sealed per `33 §7.4`. `SemStrait::save_manifest` takes `&SemanticManifest` and produces a `SemanticManifestId`; it does not mutate. |
| Plan mutation                                    | **NO.** `SemanticPlan` is `#[non_exhaustive]` but treated as immutable downstream of the planner. |
| Async runtime pinning                            | **NO.** `tokio`-neutral per `30 §9`. |
| Warning-policy enforcement at stages              | **NO.** Stages are unaware of `WarningPolicy`; enforcement is strictly at `semstrait-api`'s return boundary (`§5.4`). |
| Cross-stage dependency reasoning                 | **NO.** `SemStrait::plan` does not inspect the `SemanticManifest`'s internal structure; it hands the `SemanticManifest` + `Request` to the planner and consumes the result. |
| One-shot convenience                             | **YES.** `compile_and_plan_and_adapt` is owned here. |
| Warning aggregation at stage boundaries          | **YES.** `WarningPolicy` + diagnostic concatenation live here. |
| Builder ergonomics                               | **YES.** `SemStraitBuilder` is owned here. |
| Unified error kind                               | **YES.** `SemStraitErrorKind` (sum + intrinsic variants) is owned here, with `Diagnose` and `From` impls. |

The rightmost "YES" row is narrow: orchestration, warning policy, builder, unified error. Everything else is delegation.

---

## 14. Cross-references

- Overview: `00 §4.2 verbs`, `00 §5 pipeline`.
- Invariants: `00 §9 I3, I6, I7, I10, I11 (I11a + I11b), I12`.
- API contracts: `30 §4 (non-exhaustive policy)`, `30 §5–§6 (Diagnostic + error codes)`, `30 §7 (stage return shapes)`, `30 §8 (trait rules)`, `30 §9 (per-crate async matrix)`, `30 §10 (feature-flag policy)`, `30 §13 (stability tiers)`.
- Shared primitives: `31 §5 (FunctionRegistry)`, `31 §7 (Diagnostic)`, `31 §14 (Request / SessionContext)`.
- Parse / validate: `32 §9 (parse signature)`, `32 §11 (validate signature)`.
- Compile: `33 §9 (compile signature)`, `33 §10 (CompileErrorKind / RepositoryErrorKind)`, `33 §11 (Repository)`, `33 §12 (I11b drift-check caller)`.
- Plan / optimize: `34 §10 (PlanErrorKind / OptimizeErrorKind)`.
- IR types: `35 §3 (SemanticPlan)`, `35 §5 (EngineArtifact)`, `35 §10 (IrErrorKind)`.
- Adapt: `36 §3 (EngineAdapter::adapt)`, `36 §9 (EngineArtifact / SqlArtifact shape)`, `36 §10 (AdaptErrorKind)`.
- Catalog / FileSystem: `37 §3 (CatalogProvider)`, `37 §5 (FileSystem)`, `37 §8 (CatalogProviderErrorKind / FileSystemErrorKind)`, `37 §9 (I11b drift gate)`.
- Downstream: `39 (semstrait-facade)` — zero-config wrapper over this crate.

---

