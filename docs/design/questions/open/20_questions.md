---

## doc: design/questions/open/20_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `data-kinds/20_taxonomy.md`
depends-on:
  - data-kinds/20_taxonomy.md
  - foundations/10_resolution_pipeline.md
  - foundations/11_names_and_scopes.md
  - foundations/12_nesting_policy.md
  - foundations/15_mapping_and_binding.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
  - apis/30_api_contracts.md
  - apis/34_semstrait_planner.md

# Open Questions — `data-kinds/20_taxonomy.md`

> Items surfaced during Round-1 drafting of the DataKind-taxonomy doc. Each entry restates the question, lists its ratified references, documents the Round-1 default `20` currently uses, and names the expected resolution point. Entries migrate out of this file when `21`–`25`, `33`, or `34` make a decision that confirms or amends `20`'s default. None of these items block the four shared-invariant ratifications (D1–D9) in `20 §2`–`§5`.

---

## Q-KIND-001. `Strategy` trait openness — sealed vs open for third-party implementers

**Section anchor:** `20 §5.2`

**Context.** `20 §5.2` ratifies the `Strategy` trait surface every variant's resolution strategy implements. The trait is public — it lives in `semstrait-planner` and its signatures participate in `34`'s public-API surface. The question is whether the trait is **open** (any crate in the ecosystem may implement a new `Strategy` and register it with the dispatch site) or **sealed** (implementation is restricted to the `semstrait-planner` crate; third-party extension requires an upstream PR).

The stakes parallel `I10` (`#[non_exhaustive]`): `DataKind::Complex` is already non-exhaustive, so the *variant* side is open. But a new variant without a strategy is useless — someone has to write the `Strategy` impl. If `Strategy` is sealed, variant extension requires in-tree coordination; if open, third parties can extend both the variant and the strategy independently.

**Options.**

- **A — Sealed.** Apply the sealed-trait pattern (`pub trait Strategy: sealed::Sealed + …`). All strategies live in `semstrait-planner`. A new `ComplexDataKind` variant requires:
  1. Adding the variant to `semstrait-model` (`#[non_exhaustive]` permits it).
  2. Adding a parallel `ResolvedComplexDataKind` arm in `semstrait-manifest` (`33`).
  3. Adding a `Strategy` impl + registry method in `semstrait-planner` (which needs a sealing-trait grant).
  4. Extending the `dispatch_strategy` match arm in `20 §5.3`.
  All four steps are in-tree, single PR, single review.
  **Pros:** Matches `30 §4`'s sealed-vs-open guidance for public traits where the invariants the trait carries are non-trivial. Keeps strategy dispatch provably total. No "mystery strategy" in third-party crates misbehaves silently. `I4` SemanticManifest determinism is trivially upheld (no non-deterministic strategy can inject itself).
  **Cons:** Third parties cannot extend the DataKind taxonomy without patching semstrait. Ergonomic-only con — no known use case exists today.
- **B — Open.** Publish `Strategy` as a fully public trait. Third-party crates may implement `MyVendorStrategy: Strategy` and register it via a `StrategyRegistry::register_for_variant(kind_tag, Box::new(…))` constructor. The dispatch site in `20 §5.3` still has a compile-time-total `match`, but the `StrategyRegistry` lookup per variant is a `HashMap<VariantTag, Box<dyn Strategy>>` instead of a direct reference-returning match.
**Pros:** Permits ecosystem extension. Matches the `CatalogProvider` / `EngineAdapter` open-trait pattern already ratified in `30 §4`.
**Cons:** Variant-to-strategy binding becomes runtime (a missing registration is a `PLAN_E_2051 StrategyMissingForVariant`, not a compile-time total-match). `I4` determinism depends on third-party strategies behaving determinically — a contract not structurally enforceable. Makes it harder to provide compile-time guarantees that a new variant has a strategy.
- **C — Sealed with explicit extension hook.** Sealed by default, but add a `StrategyRegistry::override_strategy(variant_tag, Box::new(…))` method intended for test doubles and benchmarking — never for production variant extension. Third-party variant extension still requires an upstream PR to `semstrait-planner`.
**Pros:** Test ergonomics of option B, safety of option A.
**Cons:** Two code paths (sealed + override) instead of one. Test-double needs can also be satisfied by constructing a `StrategyRegistry` with test values at test time (no override mechanism needed).

**Drafter recommendation.** **Option A (sealed).** Rationale:

- The `Strategy` trait's contract is load-bearing: it carries I4 determinism, I5 "no resolution at plan time", I6 "synchronous hot path", and I8 "SemanticManifest is planner-complete". A third-party strategy that violates any of these invariants silently corrupts the planner's guarantees.
- The `DataKind` taxonomy is deliberately small — four variants in v1, with `#[non_exhaustive]` only to permit future in-tree additions. There is no extension point today that an open Strategy trait would unlock.
- The `CatalogProvider` / `EngineAdapter` open-trait pattern exists because the ecosystem *requires* open extension — catalogs are vendor-specific, engines are vendor-specific. DataKind variants are not vendor-specific.

**Blocking?** No. The `20 §5.2` sketch is openness-agnostic; either option A or option B can be ratified by `34` without amending `20`. Tracked against `34`'s public-API surface.

**Expected resolution point.** `34 §`* (public `Strategy` trait surface).

---

## Q-KIND-002. Subsystem-prefix allocation within the `2000`–`2099` range

> **Moved to `[../closed/20_questions.md](../closed/20_questions.md#q-kind-002-subsystem-prefix-allocation-within-the-20002099-range)`.** Superseded by typed-kind diagnostic discipline; numeric subsystem-prefix governance is not a v1-blocking decision surface.

---

## Q-KIND-003. Interface exposure for a single-DataKind Request — bare vs degenerate-composed at planner entry

**Section anchor:** `20 §4.4` / `§5.3`

**Context.** `20 §4.4`'s Invariant D5 ratifies that `Simple` exposes `SemanticInterface` (bare) and every `Complex` variant exposes `ComposedSemanticInterface` — this mapping is variant-determined.

But the planner dispatch site (§5.3) treats the `InterfaceView` uniformly only if every strategy accepts `&ComposedSemanticInterface`. If a `SimpleStrategy` accepts `&SemanticInterface` directly, the dispatch site must branch on interface type, not just variant tag — a minor but real complication.

The question is whether `Simple`'s `Strategy::resolve` should receive the bare `SemanticInterface` (matching D5 literally) or a **degenerate** `ComposedSemanticInterface` (one constituent, `FieldProvenance::Native` everywhere) for uniform dispatch.

**Options.**

- **A — Strict D5 — Simple gets bare.** `SimpleStrategy::resolve` receives `&SemanticInterface`; every Complex strategy receives `&ComposedSemanticInterface`. Dispatch at `§5.3` branches on the interface type via the `InterfaceView` enum.
**Pros:** Literal D5. No synthetic `ComposedSemanticInterface` allocation per Simple plan. Matches `16 §5.1`'s "distinct type" ratification.
**Cons:** Two dispatch paths. Code that walks a mixed tree of `Simple` + `Complex` children needs to handle both interface types (though the `SemanticsView` trait from `16 §5.1` does factor the common accessors).
- **B — Degenerate composition — Simple gets a trivially-composed view.** Simple's `InterfaceView` is still `Bare(_)` at the SemanticManifest layer (D5 is preserved), but the planner's `PlannerCtx` synthesizes a degenerate `ComposedSemanticInterface` at plan time, and `SimpleStrategy` consumes that instead. Uniform dispatch in `§5.3`.
**Pros:** Uniform planner code — every strategy consumes the same interface type. Simplifies recursive dispatch when a Complex strategy delegates into a Simple child.
**Cons:** Per-plan allocation of a degenerate `ComposedSemanticInterface`. Muddies the D5 boundary — consumers may start thinking of Simple as "just a one-constituent composition", which `16 §5` explicitly rejects.
- **C — Hybrid: trait-level only.** Both interface types implement a shared `SemanticsView` trait (already ratified in `16 §5.1`). Strategies consume `&dyn SemanticsView` — no type-level branching, no per-plan allocation. The dispatch site chooses which concrete type to pass based on the variant, but the strategy never sees the difference.
**Pros:** Best of both. No allocation, no branching visible inside strategies. Respects D5.
**Cons:** Hides the concrete interface type behind a trait object — in rare cases, a strategy needs the concrete type (e.g. a `JoinsetStrategy` that walks the composed-interface's `constituents:` field). For those cases, downcasting or a separate concrete-typed method is required.

**Drafter recommendation.** **Option C (hybrid via trait).** Rationale:

- `16 §5.1` already ratifies `SemanticsView` as the shared accessor trait. It's the intended abstraction.
- No per-plan allocation overhead.
- Strategies that need the concrete type (e.g. Joinset's `constituents:` walk) can require `&ComposedSemanticInterface` directly; a `SimpleStrategy` that needs neither degeneration nor composition would simply take `&dyn SemanticsView`.

**Blocking?** No. The trait surface in `20 §5.2` uses `&RequestSlice`, not `&SemanticInterface` / `&ComposedSemanticInterface` directly — the interface is accessed via `ctx` / `manifest` lookups. The question bites at implementation time when concrete-type dependencies are drafted.

**Expected resolution point.** `34 §`* when it pins down `PlannerCtx` field shapes, and `22` / `24` when they specify which concrete interface shape their strategies consume.

---

## Q-KIND-004. Shared-vs-per-variant partition of structural Preconditions

**Section anchor:** `20 §4` / `§6.2`

**Context.** `20 §6.2` lists the `validate`-stage Preconditions that apply to every variant ("shared") and defers per-variant Preconditions to `21`–`24`. But some Preconditions have a gray area — they appear shared at first glance but have per-variant exceptions, or vice versa.

Concrete gray cases surfaced during drafting:

1. **"≥2 children" rule.** Applies to Unionset (`12 §3.2`) and Grainset (`12 §4.3`). Does NOT apply to Joinset (v1: exactly 2; `12 §5.3`) or to Simple (no children). Is this a shared-with-exception rule, or three per-variant rules?
2. **Structural label uniqueness.** Applies to every `ComplexDataKind` (a parent's children must have unique labels). Does NOT apply to Simple. `VALID_E_2004` in `20 §8.2` treats this as shared-Complex. Should it live in `20`, or be split across `22` / `23` / `24`?
3. **Interface exposure check.** D5 (`§4.4`): `Simple` returns `Bare(_)`, `Complex` returns `Composed(_)`. `VALID_E_2002 InterfaceTypeMismatch` is an implementer-bug check (should never fire for Model-authored content). Is it worth a code in `20`'s shared roster, or better a debug-only `assert!` that never surfaces as a diagnostic?

**Options.**

- **A — Shared-only-if-applies-to-all-four. Anything with a per-variant exception lives in the variant docs.** "≥2 children" goes to `22` + `23`; structural-label uniqueness goes to `22` + `23` + `24`; InterfaceTypeMismatch stays in `20` (applies to all four via the trait contract).
**Pros:** Crisp boundary. Readers of `20 §4` / `§6.2` see only invariants with no exceptions.
**Cons:** Duplication across `22` / `23` / `24` for rules that differ only in numeric parameters (min_children = 2). The `NestingCapability` struct from `§2.2` already factors the parameter — duplicating the diagnostic text is noise.
- **B — Shared-when-structurally-symmetric. Rules like "≥2 children for Unionset and Grainset" live in `20` with the variant-specific parameter noted inline.** `VALID_E_2006 InsufficientChildCount` fires from `20`'s `validate_structure` default behavior driven by `NestingCapability.min_children`.
**Pros:** No duplication. The diagnostic code is stable across variants. The shared `DataKindOps::validate_structure` can call a shared helper that reads `NestingCapability` and emits the shared diagnostic.
**Cons:** Readers of `22` / `23` have to chase back to `20 §6.2` for the diagnostic. Extra indirection.
- **C — Hybrid. Shared roster for the *diagnostic code*, per-variant roster for the *triggering rule explanation*.** `VALID_E_2006 InsufficientChildCount` lives in `20 §8.2`; the *rule text* ("`Unionset` requires ≥2 branches per `12 §3.2`") lives in `22 §`* / `23 §`*, with a back-reference to `20 §8.2`'s code.
**Pros:** Best of both. Avoids duplication of diagnostic text; keeps per-variant rule exposition local.
**Cons:** Readers must follow the reference. Mild cognitive overhead.

**Drafter recommendation.** **Option C (hybrid).** Apply it retroactively to `20 §6.2` and `20 §8.2`:

- Add `VALID_E_2006 InsufficientChildCount` to `20 §8.2` (currently reserved in `VALID_E_2010`–`2029`).
- `22` / `23` / `24` each cite `VALID_E_2006` in their per-variant validation rosters with the variant-specific parameter.
- `VALID_E_2002 InterfaceTypeMismatch` stays in `20` as a shared-trait-contract check (applies to all four).
- `VALID_E_2004 StructuralLabelCollision` stays in `20` as a shared-Complex check, with the diagnostic's `location` field naming the offending parent + child labels.

**Blocking?** No. The option-A approach currently followed by `20 §6.2` / `§8.2` produces a correct roster; option C is a refinement that can land as a doc edit once `22` / `23` / `24` draft their per-variant tables and observe the duplication.

**Expected resolution point.** First draft of `22` or `23` — whichever ratifies the "≥2 children" rule first will either cite option-A (and `20 §8.2` stays as-is) or recommend option-C (and `20 §8.2` gets `VALID_E_2006` moved out of reserved).

---

## Not Opened as Questions

Items considered during drafting but not elevated to open-question status. Each already has a ratified answer cited inline in `20`:

- **Two-level `DataKind` sum type structure.** Ratified in `§2.1`. `00 §4.1` already pairs `SimpleDataKind` / `ComplexDataKind` explicitly; `20` just mirrors the vocabulary.
- `**Binding` uniqueness on Simple.** Ratified in `15 §2.1` and restated as Invariant D1.
- **Same-variant self-nesting ban.** Ratified in `12 §2.1`.
- **Nested-kind scope has no interface.** Ratified in `11 §2` / `§10`.
- `**ComposedSemanticInterface` is a distinct type from `SemanticInterface`.** Ratified in `16 §5.1`.
- `**Joinset` is the explicit composition mechanism; implicit composition is planner-synthesized.** Ratified in `16 §9` / `§10`.
- **Strategies are per-variant, not per-aspect or per-something-else.** Ratified in `§5.1` with Invariant D9.

---

**End of `20_questions.md`.**