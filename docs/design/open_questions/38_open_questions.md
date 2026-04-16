---
doc: design/open_questions/38_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/38_semstrait_api.md`
depends-on:
  - apis/38_semstrait_api.md
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
  - apis/36_semstrait_adapter.md
  - apis/37_semstrait_catalog.md
---

# Open Questions — `apis/38_semstrait_api.md`

> Items surfaced during Round-1 drafting of the `semstrait-api` public API contract. Each entry restates the question, lists its ratified references, and records the Round-1 default `38` currently uses. Entries migrate out of this file as later docs (principally `34`'s planner, `39`'s facade, and amendments to `30`) confirm or amend the defaults. None of these items block the headline ratifications in `38 §15`.

---

## Q-API-001 — Dedicated `API_E_*` subsystem prefix for structural configuration errors

**Question.** `38 §6.2` adds two configuration-level `SemStraitError` variants (`BuilderInvalid`, `NoRepositoryConfigured`) that do not correspond to any stage. `38 §6.5` assigns them diagnostic code `COMP_E_0101` (the name-resolution-class slot) as a placeholder. Should these variants receive a dedicated `API_E_*` subsystem prefix registered in `30 §6.2`, or continue to re-use `COMP_E_*` codes?

**Refs.**
- `30 §6.1`–`§6.2` — reserved subsystem prefix table; `API` is not currently listed.
- `30 §6.6` — reserved-but-unpopulated prefixes (`REG`, `IO`, `ENG`). `API` is not among them.
- `38 §6.3` — current position: no new prefix; configuration errors piggyback on `COMP_E_*`.
- `38 §6.5` — placeholder `COMP_E_0101` for both structural variants.

**Arguments for dedicated `API_E_*` (proposed amendment).**
- Lexical distinctness at grep time: `API_E_0101` (builder invalid) vs `COMP_E_0101` (name-resolution fatal) disambiguates upstream ("wrong wiring") from downstream ("wrong model") failures.
- Future growth: a batch-request surface (`Q-API-009`), a diagnostics-routing policy, or a SaaS-style async wrapper would all want stable codes that aren't compile-stage.

**Arguments against (current Round-1 default).**
- Two variants do not justify a whole subsystem. `REG` and `ENG` are reserved but unpopulated after months of design; adding another unpopulated prefix is churn.
- Configuration errors are caller-setup failures, not pipeline-stage failures — they're rare in production (caught at integration time) and don't need the same stability discipline as stage errors.
- Using `COMP_E_0101` "misclassifies" the diagnostic in code-range tooling but a `SemStraitError::BuilderInvalid` match is the real route a caller takes.

**Current position in `38`.** `BuilderInvalid` and `NoRepositoryConfigured` emit `COMP_E_0101`. Shape, variant names, severity, and discipline are ratified; only the literal digits may shift.

**Next step.** Decide during `30`'s next amendment pass. If an `API_E_*` prefix is adopted, allocate at minimum `0100`–`0199` with sub-ranges mirroring `CAT_E_*` / `FS_E_*` (config / setup / runtime-policy). Tracked as amendment item `[TD-API-CODE-TABLE-AMEND]`.

---

## Q-API-002 — `PipelineOutcome` struct vs `(EngineArtifact, Vec<Diagnostic>)` tuple return

**Question.** `38 §7.1`'s `compile_and_plan_and_adapt` returns `(EngineArtifact, Vec<Diagnostic>)`. `§2.1` reserves a `PipelineOutcome` struct name, marked `Provisional`. Should the fused helper return the dedicated struct now, or keep the tuple and let `PipelineOutcome` land in a future MINOR?

**Refs.**
- `38 §2.1` — `PipelineOutcome` Provisional tier.
- `38 §7.1` — current tuple signature.
- `30 §4.2` — `#[non_exhaustive]` struct roster; `PipelineOutcome` would join this list.

**Arguments for a `PipelineOutcome` struct (future default).**
- Room for additive fields: per-stage timings (`parse_duration`, `compile_duration`, etc.), adapter-id (already traceable via `SemanticPlan` but convenient to surface), manifest-id, request-id for observability.
- Named-field access is more discoverable than tuple-position access.
- MINOR-additive: callers who match on `pipeline.artifact` and `pipeline.diagnostics` never break when a new field appears.

**Arguments against (current Round-1 default).**
- Premature abstraction — v1 callers only want the artifact and the diagnostics. Introducing a struct to hold two fields is API bloat.
- `(EngineArtifact, Vec<Diagnostic>)` is idiomatic Rust; callers destructure via `let (art, diags) = …`.
- Per-stage timings belong in an instrumentation channel (`Q-API-004`-adjacent), not the outcome type.

**Current position in `38`.** Tuple return; `PipelineOutcome` reserved but not used.

**Next step.** If `Q-API-004` (diagnostic de-duplication) or an instrumentation open item lands, revisit at that point — the struct then holds more than two fields and justifies its own type.

---

## Q-API-003 — Stage-ownership of escalated warnings under `WarningPolicy`

**Question.** When `WarningPolicy::FailOnWarning` escalates a compile-stage warning to a fatal, `38 §5.3` wraps it as `SemStraitError::CompileStage(CompileErrors { fatal: <warning-diag>, warnings: [...] })`. Should the outer variant be `CompileStage` (preserving origin) or a dedicated `SemStraitError::ApiEscalated { origin: StageOrigin, diag: Diagnostic }` variant (clarifying the escalation)?

**Refs.**
- `38 §5.3` — current escalation rewraps into the stage's own carrier.
- `38 §5.6` — invariant: code/severity/message unchanged by escalation.
- `38 §6.6` — `StageOrigin` enum already distinguishes stage-of-origin.

**Arguments for stage-origin preservation (current Round-1 default).**
- Caller code that pattern-matches on `SemStraitError::CompileStage` sees no difference between "compile emitted an error" and "compile emitted a warning escalated by policy" — both are legitimate reasons for compile to have halted from the caller's perspective. The inner `Diagnostic.severity` is the sole bit that differs.
- Keeps the variant set small; adding `ApiEscalated` is a new discriminator to pattern-match on.
- Aligns with `30 §7`'s per-stage fail-fast rule: the stage returns its own carrier regardless of how the fatal arose.

**Arguments for `ApiEscalated`.**
- Explicit is better than implicit. Callers that want to distinguish "compile intrinsically errored" from "compile emitted a warning and the policy escalated" gain a structural discriminator.
- Error-reporting UX often wants to say "the policy fired" rather than "compile failed" — an explicit variant makes that phrasing easy.

**Current position in `38`.** Preserve stage origin; escalation leaves the outer variant unchanged. Callers distinguish via `Diagnostic.severity`.

**Next step.** Revisit if the facade crate (`39`) surfaces a "pretty error-report printer" that benefits from a dedicated escalation discriminator.

---

## Q-API-004 — Diagnostic de-duplication policy across stages

**Question.** `38 §10.3` explicitly does NOT de-duplicate diagnostics across stages. A missing column may be reported once at `validate` (`VALID_E_0101`) and again at `compile` (`COMP_E_0101` over the same span). Should `semstrait-api` fold duplicates under a `(code, span)` predicate before returning?

**Refs.**
- `38 §10.3` — current "no de-duplication" policy.
- `30 §5.1` — canonical `Diagnostic` shape: `code`, `severity`, `message`, `location`, `context`.

**Arguments for de-duplication (future option).**
- Cleaner caller UX — a `cargo check`-style printer doesn't surface the same error twice.
- `(code, span)` pair is a natural equivalence class.

**Arguments against (current Round-1 default).**
- Duplication across stages is informative: it means the earlier stage's warning was not acted on before the later stage re-encountered the root cause.
- No agreed-upon equivalence: is `(code, span)` sufficient, or does `(code, span, message)` matter? Messages may interpolate per-stage context; equal codes/spans can carry different messages.
- De-duplication requires a canonical ordering of which copy to keep — is it the first-observed (stage-earliest) or the most-specific (stage-latest)?
- Callers with their own preferences apply their own predicate on the returned vector.

**Current position in `38`.** No de-duplication in v1. Callers that want it implement `.dedup_by_key(|d| (d.code, d.location.clone()))` or similar.

**Next step.** If a production UX surfaces strong demand, define the equivalence class and the keep-which-copy rule in a `30` amendment, then implement here. Until then, this is a caller-side concern.

---

## Q-API-005 — Incremental-compile surface

**Question.** `38 §8.4` states model mutation forces a full re-compile. Should a Round-2 MINOR add a `SemStrait::recompile_with_changes(&manifest, ModelDiff)` surface that re-compiles only affected bindings?

**Refs.**
- `33 §7.4` — Manifest is sealed; no in-place mutation.
- `33 §13` — determinism discipline (identical inputs → identical manifest bytes).
- `38 §8.4` — current no-incremental-compile stance.

**Arguments for incremental compile (future MINOR).**
- Model-file edits in a development loop (e.g. adding a new Grainset, fixing a typo) could re-compile in seconds vs tens of seconds for large workspaces.
- Services that dynamically mutate models (multi-tenant SaaS adjusting Semantics per tenant) would benefit.

**Arguments against (current Round-1 default).**
- Determinism story becomes harder: an incrementally-compiled Manifest may not be byte-identical to a from-scratch compile for the same (model, catalog snapshot) pair (`33 §13.2`'s guarantee would need qualification).
- Implementation complexity: the dependency graph across bindings, data-kinds, relationships, and composition is dense; invalidation analysis is its own research project.
- Users who need fast incremental loops use the `FunctionRegistry`'s process-global handle + manifest caching — the compile itself becomes a cold-path operation amortized across many plans.

**Current position in `38`.** Not in v1. Callers who need fast iteration hold a manifest cache keyed on model hash.

**Next step.** Revisit after v1 if benchmarks show compile-stage latency dominating production workloads. A prerequisite is a well-defined `ModelDiff` type that `32` would own.

---

## Q-API-006 — Per-`SemStrait`-handle `FunctionRegistry`

**Question.** `38 §4.5` and `§1.3` state the `FunctionRegistry` is process-global per `31 §5.2`. Should `SemStraitBuilder::function_registry` accept a caller-built `&'static FunctionRegistry` constructed from a test-local `RegistryBuilder`, or remain pinned to `semstrait_core::function_registry()`?

**Refs.**
- `31 §5.2` — canonical registry is process-global and sealed.
- `31 §5.5` (via `open_questions/31 Q-CORE-005`) — whether a per-process isolated registry is feasible.
- `38 §4.5` — current position.

**Arguments for per-handle registries (future option).**
- Test isolation — a test that registers engine-specific extensions can do so without polluting the process-global state.
- Multi-tenant services that want different function sets per tenant (rare but plausible) gain a clean extension axis.

**Arguments against (current Round-1 default).**
- `CanonicalFn` identity is process-global per `31 §5.2` — two registries with different function rosters would create IDs that don't compose across them, violating I3 (stability of canonical IDs).
- Current `RegistryExtension` / `RegistryBuilder` surfaces don't support isolated instances cleanly; work in `31` is prerequisite.

**Current position in `38`.** `function_registry` field accepts any `&'static FunctionRegistry`, but v1 ships only `semstrait_core::function_registry()`. The builder field shape accommodates future isolation without API change.

**Next step.** Depends on `Q-CORE-005` resolution. If `31` adopts per-process isolation, `38` picks it up with no signature change.

---

## Q-API-007 — Sync wrappers for non-async callers

**Question.** Callers without an async runtime (CLI tools, synchronous test harnesses, WASM without async support) need a synchronous compile path. Should `semstrait-api` ship `compile_from_yaml_blocking(&self, yaml: &str)` that wraps `block_on(self.compile_from_yaml(yaml))`?

**Refs.**
- `30 §9` — async surfaces are executor-agnostic; callers choose the runtime.
- `38 §9.5` — current position: callers bridge via their runtime's `block_on`.

**Arguments for sync wrappers (future MINOR).**
- Caller ergonomics — CLI tools would prefer `compile_from_yaml_blocking(yaml)?` over `tokio::runtime::Runtime::new()?.block_on(…)`.
- `_blocking` is a Rust ecosystem idiom (`reqwest::blocking`, `rusoto`'s sync facades).

**Arguments against (current Round-1 default).**
- Requires a runtime dependency (`tokio`, `async-std`, or `futures::executor`) in `semstrait-api`, violating the "executor-agnostic" stance of `30 §9`.
- Gated feature flag (e.g. `blocking`) adds per-crate feature matrix complexity against `30 §10.5`.
- `semstrait-facade` (`39`) is the more-natural home for sync convenience; `38` stays purely async.

**Current position in `38`.** Not in v1. Callers who want sync bridge via their runtime.

**Next step.** Consider for `39_semstrait_facade.md` where "one-shot-no-configuration" might pair well with "one-shot-no-async".

---

## Q-API-008 — `SemStrait::builder` as `const fn`

**Question.** `38 §11.4` states `SemStrait::builder` is not `const fn` because `Vec::new()` and `Arc<dyn ...>` construction inside the builder are not const-compatible. Should the builder be refactored (e.g. lazy allocation at `build`) to enable `const fn builder()`?

**Refs.**
- `38 §4.7` — `SemStraitBuilder::new` is `const fn`; `SemStrait::builder` is not.
- `30 §8.1` — `const fn` is preferred where possible.

**Arguments for making it `const fn` (future refactor).**
- Enables `static SEMSTRAIT_BUILDER: SemStraitBuilder = SemStrait::builder();` declarations.
- Unifies the `const fn` discipline across the builder's entry points.

**Arguments against (current Round-1 default).**
- Requires refactoring `SemStraitBuilder` to defer allocation (the `Vec<Box<dyn OptimizerPass>>` cannot be `const` initialized).
- Callers needing a `const` builder hold a `SemStraitBuilder` in `const` directly (via `SemStraitBuilder::new()`, already `const fn`).
- Net ergonomic gain is small.

**Current position in `38`.** `SemStrait::builder` is not `const`; `SemStraitBuilder::new` is.

**Next step.** Revisit only if the caller pattern shows widespread use of `static` builder declarations.

---

## Q-API-009 — Batch `compile_and_plan_and_adapt(Vec<Request>)`

**Question.** Should `38 §7.1`'s fused helper accept a batch of requests to avoid repeated plan-and-adapt boilerplate for callers issuing many requests against the same manifest?

**Refs.**
- `38 §7.1` — current single-request signature.
- `38 §8.1` — compile-once / plan-many is already the documented pattern.

**Arguments for batch variant (future MINOR).**
- Caller ergonomics when a model + catalog is already cached and only requests vary.
- Opens the door to parallel plan/adapt across requests (`SemStrait` is `Sync`; the planner and adapter are both sync-pure).

**Arguments against (current Round-1 default).**
- Callers who want batch write a trivial loop; the savings are 2-3 lines.
- Batch-error semantics are contentious: fail on first? accumulate per-request errors? partial results? Each policy is a separate API decision.
- The compile-once / plan-many pattern in `§8.1` already expresses this pattern more clearly than a batch signature would.

**Current position in `38`.** Not in v1. Callers loop.

**Next step.** Revisit if service-side observability shows many callers writing identical batch loops.

---

## Q-API-010 — `validate_manifest` aggregation policy

**Question.** `38 §3.4` aggregates per-source drift reports into a single `DriftReport` using max-severity ordering. Should the API surface the per-source reports directly (`Vec<DriftReport>`) instead?

**Refs.**
- `37 §9.2` — `DriftReport` shape.
- `37 §9.4` — caller-policy table keys on `DriftStatus`.
- `38 §3.4` — current aggregation policy.

**Arguments for per-source vector (future option).**
- Preserves all information; a caller debugging a drift failure wants to see which specific source drifted.
- Matches the `37 §9.4` caller-policy table (which acts on a per-source basis conceptually).

**Arguments against (current Round-1 default).**
- Callers typically act on aggregate: if ANY source drifted breaking, abort. `DriftStatus::Breaking` at the top-level is sufficient.
- Per-source vector requires the caller to re-aggregate for decision-making.
- The per-source details already live in `DriftReport.details: Vec<DriftKind>` via concatenation; the information is preserved.

**Current position in `38`.** Max-severity aggregation; per-source details concatenated into `DriftKind` vector.

**Next step.** Revisit if diagnostics UX shows callers consistently wanting to trace which source contributed which drift.

---

## Q-API-011 — Direct `emit` on `SemStrait` for SQL-emission convenience

**Question.** `EngineAdapter::emit` (`36 §3`) renders SQL; callers accessing it via `SemStrait::adapt` must then call `adapter.emit(&plan, &manifest)` separately. Should `SemStrait::emit(&self, adapter, &plan, &manifest) -> Result<SqlArtifact, AdaptError>` exist?

**Refs.**
- `36 §3.1` — `EngineAdapter::emit` signature.
- `38 §1.3` — current stance: SQL is reached via the injected adapter, not via `SemStrait`.

**Arguments for direct `emit` (future MINOR).**
- Ergonomic parity with `adapt` — callers who want SQL don't need to pattern-match on adapter types.
- Applies `WarningPolicy` at the `emit` boundary consistently with other stage entries.

**Arguments against (current Round-1 default).**
- SQL is an adapter concern (`36 §8`); surfacing it on `SemStrait` risks suggesting the crate owns SQL generation.
- `emit` is a strict subset of `adapt` on adapters that return SQL artifacts; adapters that emit non-SQL (Substrait protobuf) have no `emit` semantics.
- The wrapper would be three lines; caller composition is a better fit.

**Current position in `38`.** No direct `emit`; callers reach it through the adapter.

**Next step.** Revisit if `39_semstrait_facade.md` decides its ergonomic top-level includes an SQL-emission convenience; `38` would follow.
