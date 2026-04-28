---
doc: design/questions/open/36_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/36_semstrait_adapter.md`
depends-on:
  - apis/36_semstrait_adapter.md
  - apis/35_semstrait_ir.md
  - apis/34_semstrait_planner.md
  - apis/31_semstrait_core.md
  - apis/30_api_contracts.md
  - foundations/14_expressions.md
  - foundations/14a_function_catalog.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
  - registry/functions_mapping.md
  - registry/types_mapping.md
---

# Open Questions — `apis/36_semstrait_adapter.md`

> Items surfaced during Round-1 drafting of the `semstrait-adapter` public API contract. Each entry restates the question, lists its ratified references, and records the Round-1 default `36` currently uses. Entries migrate out of this file as downstream drafts (`34`'s planner wiring, per-adapter crate `3x` appendices, and `30` amendments) confirm or amend the defaults. None of these items block the headline ratifications in `36 §17`.

---

## Q-ADAPT-001 — `EngineAdapter::adapt` return shape: `Result<EngineArtifact, AdaptError>` vs `Result<(EngineArtifact, Vec<Diagnostic>), AdaptError>`

**Question.** `36 §3.1` ratifies `adapt(&SemanticPlan, &SemanticManifest) -> Result<EngineArtifact, AdaptError>`. `30 §7`'s stage-result pattern recommends `Result<(Output, Vec<Diagnostic>), ...>` so warnings propagate alongside successful output. Should `adapt` also carry `Vec<Diagnostic>` on success (e.g. "structural rewrite for `string_agg` applied on Spark")?

**Refs.**
- `30 §7` — fail-fast stage-result pattern.
- `36 §3.1` — current bare-`Result` signature.
- `35 §3.1` — `SemanticPlan.diagnostics` carries planner diagnostics already; adapters MAY append per `35`'s note.

**Arguments for bare `Result<EngineArtifact, AdaptError>` (current Round-1 default).**
- Simpler call-site ergonomics; matches the current-code `adapt` shape.
- Adapter diagnostics are already accommodated by `SemanticPlan.diagnostics` append (`35 §3.1`) — adding a second channel duplicates the surface.
- `ADAPT_W_*` codes are reserved but unused in v1 (`36 §10.2`); no v1 producer exists.

**Arguments for `Result<(EngineArtifact, Vec<Diagnostic>), AdaptError>`.**
- Matches `30 §7`'s documented stage pattern; uniform across stages.
- Decouples the diagnostic channel from the plan (a plan consumed twice by two adapters should produce two independent diagnostic lists, not both mutating the plan's list).
- Future `ADAPT_W_*` codes get a natural home without changing the signature again.

**Current position in `36`.** Bare `Result`. Diagnostic appendage on `SemanticPlan.diagnostics` permitted but not required.

**Next step.** Revisit at `34`'s planner-wiring draft. If `34` adopts the tuple pattern uniformly for `plan` / `optimize`, `36` follows.

---

## Q-ADAPT-002 — `AdapterCapabilities` consultation site: pre-`adapt` vs inside `adapt`

**Question.** `36 §6.2` describes `AdapterCapabilities` as consumed by the planner (`34`) and by callers pre-`adapt` (`semstrait-api`). Should `adapt` itself also consult `capabilities()` as an early-fail gate (e.g. "plan uses `Capability::AsOfJoin`; adapter does not advertise it — fail immediately")?

**Refs.**
- `36 §3.2` I-ADAPT-5 — error-first fallback on unsupported features.
- `36 §6.2` — consumer list: planner + API, not adapter-internal.
- `30 §7` — fail-fast stage returns.

**Arguments for current placement (planner + API only).**
- Avoids redundant checks — if the planner already gated, re-checking inside `adapt` is wasted work.
- Keeps `adapt`'s contract tighter: "given a well-formed plan, emit or fail with a specific code", not "re-validate feasibility".
- `AdapterCapabilities` is a declarative approximation; the authoritative check is the emitter path itself (via `AdaptError::Unsupported*`).

**Arguments for adding an adapter-internal check.**
- Defends against misuse (e.g. a caller that bypassed `semstrait-api` and called `adapt` directly without pre-checking).
- Cheap — a single `capabilities().supports(cap)` per offending `PlanNode` variant.
- Matches the `RegistryExtension` registry's posture of checking at every boundary.

**Current position in `36`.** Planner + API only. Emitter-path failure (`AdaptError::Unsupported*`) is the authoritative check.

**Next step.** Decide at per-adapter-crate drafting. If per-adapter crates find themselves repeating capability-check logic across emitters, hoist to a shared wrapper in `36`.

---

## Q-ADAPT-003 — `DialectEmit` split from `Dialect`: placement in `35` vs `36`

**Question.** `36 §4.1` / `§4.2` splits `Dialect` (structural surface, ratified in `35 §6.5`: `ID` + `capabilities()`) from `DialectEmit` (operational surface: `type_name`, `quote_identifier`, `emit_asof_join`, ...). Should the split be ratified in `35` (so `semstrait-ir` carries the full trait) or stay in `36` (so `semstrait-ir` remains dependency-free of String-formatting concerns)?

**Refs.**
- `35 §6.5` — ratifies the structural `Dialect` trait.
- `36 §4.2` — operational `DialectEmit` extension.
- `35 §1.3` — `semstrait-ir` is "pure, sync, canonical" (no engine identity, but String formatting is engine-identity-adjacent).

**Arguments for current split (current Round-1 default).**
- Keeps `semstrait-ir` free of emission-layer concerns; the structural `Dialect` is an identity trait (`ID` + `capabilities()`) and nothing more at that layer.
- `DialectEmit`'s method set grows as adapters add rendering idioms; keeping it in `36` lets growth happen without touching `35`.
- Matches `35 §1.3`'s "zero emission" posture.

**Arguments for unifying in `35`.**
- Fewer traits to learn / impl for adapter authors (one trait instead of two).
- Eliminates the `DialectEmit: Dialect` supertrait chain, which callers must understand.
- Substrait-only dialects that never emit SQL still impl `Dialect` today without `DialectEmit`; unification would force them to impl the String-formatting methods for compile-time completeness, but the defaults handle this cleanly.

**Current position in `36`.** Split ratified as-is.

**Next step.** Revisit with per-adapter-crate drafting. If the supertrait chain creates consistent boilerplate (e.g. every impl has to write `impl Dialect for Foo { … } impl DialectEmit for Foo { … }`), collapse to a single trait.

---

## Q-ADAPT-004 — `EngineAdapter::debug_sql` removal vs retention

**Question.** Current code exposes `EngineAdapter::debug_sql(&self, plan)` as a trait method with a default impl (ANSI-SQL render). `36 §9.4` demotes it to a free function `debug_sql(plan, manifest)`. Is the demotion worth the call-site migration cost?

**Refs.**
- `36 §9.4` — current position (free function).
- Current code: `crates/semstrait-adapter/src/traits.rs` — `debug_sql` is a trait default method.
- `30 §4.1` — removing a trait method is MAJOR.

**Arguments for free-function demotion (current Round-1 default).**
- The method is adapter-independent (every default impl is identical).
- Reduces `EngineAdapter`'s method count, keeping the trait tight.
- Free functions are easier for third-party adapters to opt into (no override needed).

**Arguments against (retain the trait method).**
- Adapters CAN override — e.g. `DataFusionAdapter::debug_sql` could render with DataFusion-specific type names for better debugging fidelity.
- Removal is a MAJOR change per `30 §4.1`; current-code consumers migrate.
- `&self` access lets an adapter consult its own state when rendering (e.g. a dialect-preference field, a caller-supplied verbosity flag).

**Current position in `36`.** Free function. `[TD-ADAPTER-DEBUG-SQL-FREE-FN]` tracks the migration.

**Next step.** Decide at the first per-adapter-crate draft. If any adapter wants dialect-specific debug rendering (DataFusion's `TIMESTAMP(9)` vs ANSI's `TIMESTAMP(9)` — identical in practice, probably not), keep the trait method. Otherwise demote.

---

## Q-ADAPT-005 — `AdapterRegistry` initialization: `OnceLock` vs caller-supplied

**Question.** `36 §11.2` ratifies `adapter_registry() -> &'static AdapterRegistry` with lazy `OnceLock` initialization, matching `function_registry()`'s posture. An alternative is caller-supplied — `Session::new_with_registry(registry: Arc<AdapterRegistry>)` — so different sessions can hold different adapter sets.

**Refs.**
- `36 §11.2` — current `OnceLock` posture.
- `31 §9.1` — `function_registry()` uses `OnceLock`.
- `00 §9` I7 — strict DAG; process-global singletons SHOULD be carefully considered.

**Arguments for `OnceLock` (current Round-1 default).**
- Matches `function_registry()` — one global-singleton pattern, not two.
- Simpler call-site: `adapter_registry().get(id)` vs session-threaded.
- Third-party adapter registration via `ctor` at binary load works naturally.

**Arguments for caller-supplied.**
- Different `Session`s can pick different adapter sets (e.g. a "read-only analytics" session with only `duckdb-sql`; a "dev" session with every adapter including debug ones).
- Test isolation — no shared global state across tests.
- Enables plugin-style adapter loading at runtime (load adapter crate, register in a session, unload).

**Current position in `36`.** `OnceLock` global. `semstrait-api::Session` MAY layer per-session allowlists on top, but the underlying registry is global.

**Next step.** Revisit if `semstrait-api` drafts a multi-tenant scenario where adapter sets need per-session control.

---

## Q-ADAPT-006 — Per-adapter-crate version pin vs float

**Question.** Per `30 §13` and `36 §12.3`, each `semstrait-adapter-<engine>` crate is versioned independently. When a per-adapter crate depends on `semstrait-adapter`, should it pin to a specific version (`semstrait-adapter = "=0.1.0"`) or float within a MINOR band (`semstrait-adapter = "^0.1"`)?

**Refs.**
- `30 §13` — per-adapter-crate independent versioning.
- `36 §12.3` — per-adapter crates Provisional; expected to evolve faster than `semstrait-adapter` itself.
- Cargo conventions — `^0.1` floats through MINORs; `=0.1.0` is an exact match.

**Arguments for floating (`^0.1`).**
- Adapter authors pick up bug fixes in `semstrait-adapter` automatically.
- Matches the workspace coordinated-release posture of `30 §3`.
- Makes it easier to keep adapters in sync with each other across a release.

**Arguments for pinning (`=0.1.0`).**
- Deterministic build; no surprise from a transitive MINOR bump.
- Per-adapter stability tiers stay decoupled — a `semstrait-adapter` MINOR that breaks an adapter's assumptions fails loudly at the adapter's own release, not in a user's build.
- Matches the `Provisional` stability tier — "don't rely on this to be stable; pin exactly".

**Current position in `36`.** Unspecified. Per-adapter crates will pick per their own tier.

**Next step.** Ratify at the first per-adapter-crate draft (`semstrait-adapter-datafusion`'s `3x` appendix).

---

## Q-ADAPT-007 — `Capability` roster placement: `35` vs `36` (carried from `Q-IR-010`)

**Question.** `35 §6.6` exposes `Capability` as a `#[non_exhaustive]` enum re-exported from `36`. `36 §6.1` uses it as the type of `AdapterCapabilities.capabilities`. Should the roster-addition mechanism (which adapter ratifies which new variant) live in `35` (where the enum is defined) or in `36` (where it's consumed)?

**Refs.**
- `35 §6.6` / `35_questions Q-IR-010` — current split (enum in `35`, roster ownership in `36`).
- `36 §6.1` — uses the enum as a struct field type.
- `30 §4` — enum-variant addition is MINOR.

**Arguments for current placement.**
- Planners in `34` need the enum at plan time (capability-gated rules) without linking `36`.
- `36` ratifies new variants in its own amendments; `35` re-exports mechanically.

**Arguments for full `36` ownership.**
- Adapter-specific; each adapter knows what it can do. `35` is plan-layer.
- Reduces `35`'s dependency surface.

**Current position in `36`.** Enum in `35`; roster in `36`. `35 Q-IR-010` confirms.

**Next step.** Confirm at `34` planner-wiring draft — if `34` can reach the Capability roster transitively through `36` (via `semstrait-adapter`'s re-export chain) without direct `36` dependency, the split stays.

---

## Q-ADAPT-008 — `SubstraitAdapter` function-reference anchor mechanism

**Question.** `36 §5.5` notes that `SubstraitAdapter` needs to map canonical functions to Substrait-catalogued function URNs. V1 uses `CanonicalFn::as_str()` as the Substrait anchor, which works for every v1 canonical entry (they all have SQL-standard names that appear in Substrait's standard function extensions). A future canonical function whose SQL name does NOT match a Substrait extension would need a dedicated anchor (`CanonicalFn::SUBSTRAIT_ANCHOR: &str`). Should the anchor mechanism be introduced now or deferred?

**Refs.**
- `36 §5.5` — current Round-1 posture.
- `31 §5.1` — `CanonicalFn` newtype; `pub const` extensibility.
- `14a §4` — v1 canonical function roster.

**Arguments for deferring (current Round-1 default).**
- Every v1 canonical name matches its Substrait anchor directly; no divergence exists today.
- Introducing `SUBSTRAIT_ANCHOR` prematurely adds per-entry ceremony with no consumer.
- `[TD-ADAPTER-SUBSTRAIT-ANCHOR]` tracks the deferral.

**Arguments for introducing now.**
- Every `CanonicalFn` const would ship with its URN baked in from the start; no "v1.1 migration" for the first divergent entry.
- Substrait URN format is already stable; adding a const is pure metadata.

**Current position in `36`.** Deferred. `CanonicalFn::as_str()` as the default anchor; dedicated anchors added per-function-entry only when a divergence lands.

**Next step.** Revisit at `14a` amendment if a new canonical function's Substrait URN diverges from its SQL name.

---

## Q-ADAPT-009 — `UnsupportedFeatureKind` classifier: sub-variant vs top-level variants

**Question.** `36 §10.1` models unsupported-feature errors as `AdaptError::UnsupportedFeature { feature: UnsupportedFeatureKind, name, adapter, location }` — one top-level variant with a sub-classifier. An alternative is separate top-level variants (`UnsupportedFunction`, `UnsupportedJoinType`, `UnsupportedDataType`, `UnsupportedPlanNode`, `UnsupportedAnnotation`). Which is preferable for diagnostic rendering and test-harness matching?

**Refs.**
- `36 §10.1` — current sub-classifier design.
- `14a §6.3` — references `AdaptError::UnsupportedFunction` (separate variant) in the canonical-layer narrative.
- `30 §6.1` — one stable code per top-level variant.

**Arguments for current sub-classifier (single top-level variant).**
- Fewer top-level variants; `AdaptError` stays tight at 16 variants.
- The classifier enum `UnsupportedFeatureKind` is already `#[non_exhaustive]`; adding a classifier is MINOR.
- Single stable code (`ADAPT_E_0302`) for every unsupported-feature flavor; downstream code-range maintenance is simpler.

**Arguments for separate top-level variants.**
- Each flavor gets its own stable code (e.g. `ADAPT_E_0302 UnsupportedFunction`, `ADAPT_E_0310 UnsupportedJoinType`), which helps diagnostic consumers that key on code alone.
- Matches `14a §6.3`'s variant naming (`AdaptError::UnsupportedFunction`).
- Pattern-matching at consumer sites is cleaner (`AdaptError::UnsupportedFunction { … }` vs `AdaptError::UnsupportedFeature { feature: UnsupportedFeatureKind::Function, … }`).

**Current position in `36`.** Sub-classifier (single variant).

**Next step.** Decide at the first consumer-side integration test. If test-harness matching feels awkward at the sub-classifier level, split into top-level variants (MINOR per `30 §4.2`).

---

## Q-ADAPT-010 — Audit seam placement: in-crate vs separate `semstrait-adapter-audit`

**Question.** `36 §14.6` ratifies a release-level audit over pathological `Name` / `LiteralValue` inputs. Should the audit live in `semstrait-adapter`'s own test suite, or in a separate `semstrait-adapter-audit` crate?

**Refs.**
- `36 §14.6` — current in-crate posture.
- `30 §9` — CI-enforced contract auditing.

**Arguments for in-crate (current Round-1 default).**
- Single crate; simpler CI configuration.
- Audit runs alongside the adapter's own unit tests.
- Test fixtures ship in `semstrait-adapter`'s `testcases/` — visible to adapter-crate authors from day one.

**Arguments for separate crate.**
- Decouples audit growth from `semstrait-adapter` releases.
- Allows audit to depend on per-adapter-crate builds (e.g. a cross-adapter audit that verifies every adapter produces safely-quoted output for the same fixture).
- Separate CI job enables faster feedback for adapter PRs (audit runs in parallel).

**Current position in `36`.** In-crate.

**Next step.** Revisit if the audit suite grows beyond ~100 fixtures or starts requiring multi-adapter coordination. Split-out is MINOR — no contract change, just a layout reorganization.

---
