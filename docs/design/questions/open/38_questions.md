---
doc: design/questions/open/38_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/38_semstrait_api.md`
depends-on:
  - apis/38_semstrait_api.md
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/32_semstrait_model.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
  - apis/36_semstrait_adapter.md
  - apis/37_semstrait_catalog.md
---

# Open Questions — `apis/38_semstrait_api.md`

> Ten questions remain open: Q-API-002, -004 through -012 (excluding -001 / -003 closed). Closed items moved to [`../closed/38_questions.md`](../closed/38_questions.md). Each entry restates the question, lists its ratified references, and records the Round-1 default `38` currently uses. None of these items block the headline ratifications in `38 §15`.

---

## Q-API-001 — Dedicated `API_E_*` subsystem prefix — CLOSED

> **Moved to [`../closed/38_questions.md`](../closed/38_questions.md#q-api-001--dedicated-api_e_-subsystem-prefix-for-structural-configuration-errors--closed--superseded-by-typed-kind-transition).** Superseded by the typed-kind discipline.

---

## Q-API-002 — `PipelineOutcome` struct vs fail-fast tuple return

**Question.** `38 §7.1`'s `compile_and_plan_and_adapt` returns the workspace-wide fail-fast tuple over `SemStraitErrorKind` — `Result<(EngineArtifact, Diagnostics<SemStraitErrorKind>), (Diagnostic<SemStraitErrorKind>, Diagnostics<SemStraitErrorKind>)>`. `§2.1` reserves a `PipelineOutcome` struct name, marked `Provisional`. Should the fused helper return a dedicated struct now (e.g. `PipelineOutcome { artifact, warnings, per_stage_timings }`), or keep the tuple and let `PipelineOutcome` land in a future MINOR?

**Refs.**
- `38 §2.1` — `PipelineOutcome` Provisional tier.
- `38 §7.1` — current tuple signature.
- `30 §4.2` — `#[non_exhaustive]` struct roster; `PipelineOutcome` would join this list.
- `30 §7` — fail-fast tuple shape that the current return shape adopts.

**Arguments for a `PipelineOutcome` struct (future default).**
- Room for additive fields: per-stage timings (`parse_duration`, `compile_duration`, etc.), adapter-id (already traceable via `SemanticPlan` but convenient to surface), manifest-id, request-id for observability.
- Named-field access is more discoverable than tuple-position access.
- MINOR-additive: callers who match on `pipeline.artifact` and `pipeline.warnings` never break when a new field appears.
- Pairs with `tracing` (`30 §6`) — the struct could carry a `Span` handle for correlating with the tracing event stream.

**Arguments against (current Round-1 default).**
- Premature abstraction — v1 callers only want the artifact and the warnings. Introducing a struct to hold two fields is API bloat.
- `(EngineArtifact, Diagnostics<K>)` is idiomatic Rust; callers destructure via `let (art, ws) = …`.
- Per-stage timings already ride on `tracing` events at `info` level (`38 §3.6`); duplicating them on a struct is redundant.

**Current position in `38`.** Tuple return; `PipelineOutcome` reserved but not used.

**Next step.** Revisit when an instrumentation surface lands (e.g. callers want timings as data, not as `tracing` events). The struct then holds more than two fields and justifies its own type.

---

## Q-API-003 — Stage-ownership of escalated warnings under `WarningPolicy` — CLOSED

> **Moved to [`../closed/38_questions.md`](../closed/38_questions.md#q-api-003--stage-ownership-of-escalated-warnings-under-warningpolicy).** Round-1 default ratified — preserve variant identity through escalation.

---

## Q-API-004 — Diagnostic de-duplication policy across stages

**Question.** `38 §10.3` explicitly does NOT de-duplicate diagnostics across stages. A missing column may be reported once at `validate` (with kind `ValidateErrorKind::ColumnNotInBindingSchema`) and again at `compile` (with kind `CompileErrorKind::BindingColumnNotInSchema`) over the same `Span`. Should `semstrait-api` fold these under a `(kind variant + Span)` predicate before returning?

**Refs.**
- `38 §10.3` — current "no de-duplication" policy.
- `30 §5` — typed-kind discipline: identification is by variant identity.
- `31 §3` — `Diagnostic<K>` carrier shape: `kind: K`, `severity`, `message` (rendered via `Diagnose::message()`), `location`, `context`.

**Arguments for de-duplication (future option).**
- Cleaner caller UX — a `cargo check`-style printer doesn't surface the same error twice.
- `(kind variant, Span)` pair is a natural equivalence class — but only across stages whose kinds use compatible variants. Cross-stage equivalence (e.g. `Validate::ColumnNotInBindingSchema` vs `Compile::BindingColumnNotInSchema`) requires an explicit equivalence map between upstream and downstream kinds.

**Arguments against (current Round-1 default).**
- Duplication across stages is informative: it means the earlier stage's warning was not acted on before the later stage re-encountered the root cause.
- No agreed-upon equivalence relation: is `(kind variant, Span)` sufficient, or does the rendered `message` matter? Two diagnostics with the same kind variant may carry different per-stage payloads (different field values); structural variant equality alone may collapse meaningfully different diagnostics.
- Cross-kind equivalence requires an explicit `is_equivalent_to(&CompileErrorKind, &ValidateErrorKind) -> bool` map that has not been ratified anywhere; that would be its own design item.
- De-duplication requires a canonical ordering of which copy to keep — is it the first-observed (stage-earliest) or the most-specific (stage-latest)?
- Callers with their own preferences apply their own predicate on the returned `Diagnostics<K>`.

**Current position in `38`.** No de-duplication in v1. Callers that want it implement `.dedup_by_key(|d| (variant_discriminant(&d.kind), d.location.span().cloned()))` or similar.

**Next step.** If a production UX surfaces strong demand, define the equivalence class (kind-variant identity, optionally with location-span) and the keep-which-copy rule in a `30` amendment, then implement here. Until then, this is a caller-side concern.

---

## Q-API-005 — Incremental-compile surface

**Question.** `38 §8.4` states model mutation forces a full re-compile. Should a Round-2 MINOR add a `SemStrait::recompile_with_changes(&manifest, ModelDiff)` surface that re-compiles only affected bindings?

**Refs.**
- `33 §7.4` — SemanticManifest is sealed; no in-place mutation.
- `33 §13` — determinism discipline (identical inputs → identical manifest bytes).
- `38 §8.4` — current no-incremental-compile stance.

**Arguments for incremental compile (future MINOR).**
- Model-file edits in a development loop (e.g. adding a new Grainset, fixing a typo) could re-compile in seconds vs tens of seconds for large workspaces.
- Services that dynamically mutate models (multi-tenant SaaS adjusting Semantics per tenant) would benefit.

**Arguments against (current Round-1 default).**
- Determinism story becomes harder: an incrementally-compiled SemanticManifest may not be byte-identical to a from-scratch compile for the same (model, catalog snapshot) pair (`33 §13.2`'s guarantee would need qualification).
- Implementation complexity: the dependency graph across bindings, data-kinds, relationships, and composition is dense; invalidation analysis is its own research project.
- Users who need fast incremental loops use the `FunctionRegistry`'s process-global handle + manifest caching — the compile itself becomes a cold-path operation amortized across many plans.

**Current position in `38`.** Not in v1. Callers who need fast iteration hold a manifest cache keyed on model hash.

**Next step.** Revisit after v1 if benchmarks show compile-stage latency dominating production workloads. A prerequisite is a well-defined `ModelDiff` type that `32` would own.

---

## Q-API-006 — Per-`SemStrait`-handle `FunctionRegistry`

**Question.** `38 §4.5` and `§1.3` state the `FunctionRegistry` is process-global per `31 §5.2`. Should `SemStraitBuilder::with_function_registry` accept a caller-built `&'static FunctionRegistry` constructed from a test-local `RegistryBuilder`, or remain pinned to `semstrait_core::function_registry()`?

**Refs.**
- `31 §5.2` — canonical registry is process-global and sealed.
- `31 §5.5` (via `questions/open/31 Q-CORE-005`) — whether a per-process isolated registry is feasible.
- `38 §4.5` — current position.

**Arguments for per-handle registries (future option).**
- Test isolation — a test that registers engine-specific extensions can do so without polluting the process-global state.
- Multi-tenant services that want different function sets per tenant (rare but plausible) gain a clean extension axis.

**Arguments against (current Round-1 default).**
- `CanonicalFn` identity is process-global per `31 §5.2` — two registries with different function rosters would create IDs that don't compose across them, violating I3 (stability of canonical IDs).
- Current `RegistryExtension` / `RegistryBuilder` surfaces don't support isolated instances cleanly; work in `31` is prerequisite.

**Current position in `38`.** The `function_registry` field on `SemStraitBuilder` (set via `with_function_registry`) accepts any `&'static FunctionRegistry`, but v1 ships only `semstrait_core::function_registry()`. The builder field shape accommodates future isolation without API change.

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

**Question.** `EngineAdapter::emit` (`36 §3`) renders SQL; callers accessing it via `SemStrait::adapt` must then call `adapter.emit(&plan, &manifest)` separately. Should `SemStrait::emit(&self, adapter, &plan, &manifest)` exist as a passthrough that returns `Result<(SqlArtifact, Diagnostics<AdaptErrorKind>), (Diagnostic<AdaptErrorKind>, Diagnostics<AdaptErrorKind>)>` (matching `adapt`'s passthrough discipline)?

**Refs.**
- `36 §3.1` — `EngineAdapter::emit` signature (returns the workspace-wide fail-fast tuple over `AdaptErrorKind`).
- `38 §1.3` — current stance: SQL is reached via the injected adapter, not via `SemStrait`.
- `38 §3.4` — `adapt` is a single-stage delegate that passes `AdaptErrorKind` through verbatim; an `emit` mirror would follow the same shape.

**Arguments for direct `emit` (future MINOR).**
- Ergonomic parity with `adapt` — callers who want SQL don't need to pattern-match on adapter types.
- Applies `WarningPolicy` at the `emit` boundary consistently with other stage entries.

**Arguments against (current Round-1 default).**
- SQL is an adapter concern (`36 §8`); surfacing it on `SemStrait` risks suggesting the crate owns SQL generation.
- `emit` is a strict subset of `adapt` on adapters that return SQL artifacts; adapters that emit non-SQL (Substrait protobuf) have no `emit` semantics.
- The wrapper would be three lines; caller composition is a better fit.

**Current position in `38`.** No direct `emit`; callers reach it through the adapter.

**Next step.** Revisit if `39_semstrait_facade.md` decides its ergonomic top-level includes an SQL-emission convenience; `38` would follow.

---

## Q-API-012 — Wrapping primitive for lifting `Diagnostic<K1>` into `Diagnostic<K2>`

**Question.** Multi-stage `SemStrait` methods (§3.4) and the fused helper (§7) need to lift `Diagnostic<K1>` / `Diagnostics<K1>` from a per-stage kind (e.g. `AdaptErrorKind`) into the unified `Diagnostic<SemStraitErrorKind>` / `Diagnostics<SemStraitErrorKind>`, given the §6.3 `From<K1> for K2` impls. Three candidate shapes are available; ratifying one governs the §31 diagnostic surface and the §38 fused-helper body.

**Refs.**
- `31 §3` — `Diagnostic<K>` / `Diagnostics<K>` primitives.
- `38 §6.3` — kind-level `From` impls (`From<AdaptErrorKind> for SemStraitErrorKind`, etc.).
- `38 §7.2` — the lift site that motivates this question.

**Option A — Blanket `impl<K1, K2> From<Diagnostic<K1>> for Diagnostic<K2> where K2: From<K1>` on `31`'s primitive.**

- Most idiomatic Rust: `?` and `.into()` lift through the wrap automatically.
- `Diagnostic<K1>: Into<Diagnostic<K2>>` and `Diagnostics<K1>: Into<Diagnostics<K2>>` come for free given any `K2: From<K1>` already declared.
- Coherence-clean because `Diagnostic<K>` is owned by `31` (the blanket impl lives where the type is defined).
- Trade-off: the blanket touches every `Diagnostic<*>` in the workspace; future kind designs that want to opt-out cannot.

**Option B — Explicit `cast_kind::<K2>(self)` adapter method on `Diagnostic<K1>` / `Diagnostics<K1>`.**

- Discoverable: `diag.cast_kind::<SemStraitErrorKind>()` makes the lift explicit at the call site.
- Less coherence-fragile: no blanket impl, so future kind-pair-specific behavior (e.g. enriching the `context` during lift) is straightforward.
- Trade-off: more verbose at the call site; doesn't compose with `?` natively unless paired with a `From` impl on the kind anyway.

**Option C — Per-element rewrap left to callers.**

- No new primitive on `31`; multi-stage methods do explicit `.into_iter().map(|d| Diagnostic::new(d.kind.into(), …))` inline.
- Trade-off: verbose, duplicates logic across multi-stage methods, and exposes `Diagnostic` field access where a prebuilt helper would not.

**Current position in `38`.** Forward-references whichever shape lands. The §7.2 example shows the lift site abstractly without committing to a specific primitive.

**Next step.** Pick A or B during the next `31` revision; update §7.2 and any other lift-site prose to match. Option A is the structural default for typed wrappers; Option B is the conservative choice if `31`'s blanket-impl posture is contested.
