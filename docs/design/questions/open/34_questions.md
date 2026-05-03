---
doc: design/questions/open/34_questions
status: Living
purpose: Round-1 open items raised against `apis/34_semstrait_planner.md`
---

# 34 — Open Questions

Unresolved items arising while drafting `docs/design/apis/34_semstrait_planner.md`. Each entry restates the question, lists the relevant ratified references, and proposes a lean next step so a later decision pass can resolve without re-reading the whole doc.

> Eight questions remain open: Q-PLAN-001, -002, -004 through -007, Q-PLAN-009, Q-PLAN-010. Closed items moved to [`../closed/34_questions.md`](../closed/34_questions.md).

---

## Q-PLAN-001 — `Request.from` shape: `DataKindRef` scalar vs `DataKindPath`

**Question.** `34 §3.1` / §3.8 ratify `Request.from: Option<DataKindRef>`, where `DataKindRef` is a newtype over `DataKindName` (a single top-level DataKind name). A Request wanting to target one arm inside a nested Complex (e.g. the `ordersAPAC` branch of a Unionset of Unionsets) cannot express that with a flat `DataKindName`. Should the field widen to a `DataKindPath` (a `Vec<DataKindName>`-like discriminated path)?

**Refs.**
- `34 §3.1` / §3.8 — current `Option<DataKindRef>` shape.
- `16 §11.6` — explicit-routing semantics (Request.from).
- `23 §3` — Unionset-of-Unionsets admissibility.
- `22 §3` — Grainset nesting posture.

**Arguments pro `DataKindRef` scalar.**
- Matches the top-level-only routing intent the v1 API layer encodes.
- Keeps the Request flat and easy to emit from JSON / gRPC.
- Nested Complex targeting is rare enough that field-first resolution (§10) covers most needs.

**Arguments pro `DataKindPath`.**
- Preserves the escape hatch when a caller really does need to pin an interior arm.
- Future-proof against `17`'s temporal-shape decisions that may expose per-arm targeting.

**Current position in `34`.** Scalar `DataKindRef` per `34 §3.1`.

**Next step.** Keep the scalar form through v1. If users request interior targeting, add a sibling field (`Request.from_path: Option<DataKindPath>`) rather than widening `from`; this preserves the flat case and adds the nested case cleanly per `30 §2.2`'s MINOR posture.

---

## Q-PLAN-002 — `Strategy` trait openness: sealed vs non-sealed

**Question.** Should the `Strategy` trait be sealed (crate-private impls only) or open to third-party implementers? `34 §8.1` / §15.1 ratify **non-sealed** for Round 1.

**Refs.**
- `34 §8.1` — `Strategy` trait surface.
- `34 §15.1` — stability posture (open, with caveat).
- `20 §5.2` — structural ratification of the trait.
- `Q-KIND-001` in [`20_questions.md`](20_questions.md) — companion question about `ResolvedDataKind` variant openness.
- `30 §4.6` — sealed-vs-open guidance.

**Arguments pro sealed.**
- Collapses the per-variant strategy roster to exactly four (`21`–`24`). Third-party strategies become meaningless unless a new `ResolvedDataKind` variant is also added; requiring both in the workspace makes the coupling explicit.
- Simplifies the dispatch story — `dispatch_strategy` can remain a closed match without the `#[non_exhaustive]` hedge.

**Arguments pro open.**
- Adapter authors MAY need custom plan shapes for novel `Complex` variants (per I10).
- Matches `30 §4.6`'s default "extensible unless proven otherwise" posture for trait surfaces.

**Current position in `34`.** Non-sealed, gated by `Q-KIND-001` (in [`20_questions.md`](20_questions.md)). If Q-KIND-001 seals `ResolvedDataKind`, sealing `Strategy` follows mechanically.

**Next step.** Revisit jointly with Q-KIND-001 before v1.0. The default position is to follow whatever `ResolvedDataKind` does — sealing both together or keeping both open together.

---

## Q-PLAN-003 — `PLAN_E_0500` allocation conflict — CLOSED

> **Moved to [`../closed/34_questions.md`](../closed/34_questions.md#q-plan-003--plan_e_0500-allocation-conflict--closed--superseded-by-typed-kind-transition).** The typed-kind transition at `30 §6` retired the stable string-code subsystem; `PlanErrorKind::ConstraintViolation` and `PlanErrorKind::AmbiguousImplicitComposition` are now distinct typed variants with no numeric identifier collision.

---

## Q-PLAN-004 — `TemporalRequest` vocabulary: expose now vs defer

**Question.** `34 §3.9` exposes a reserved-shape `TemporalRequest` on `Request.temporal`. Planner consumption is DEFERRED per `17 §10`. Should the type be visible in v1 at all, or should `Request.temporal` stay a placeholder (e.g. `pub temporal: Option<()>`)?

**Refs.**
- `34 §3.9` — current reserved shape.
- `17 §6.1` — `TemporalRequest` shape ratification.
- `17 §10` — DEFERRED planner consumption.
- `30 §2.2` — MINOR-add discipline for `#[non_exhaustive]` additions.

**Arguments pro expose now.**
- The API layer (`semstrait-api`) can start populating the field in a forward-compatible way today.
- Callers can inspect the type for correctness (shape-aware serialization testing) without waiting for `17`'s planner milestone.

**Arguments pro defer.**
- Exposing a type that the planner ignores is a foot-gun: callers set `temporal` and wonder why it does nothing.
- Adding the field later is MINOR per `30 §2.2`; no compatibility cost.

**Current position in `34`.** Exposed as a reserved shape with advisory `PLAN_W_1702` on any populated consumption attempt.

**Next step.** Keep the current posture. If `semstrait-api` begins populating `TemporalRequest` before `17`'s planner milestone lands, escalate the advisory to a hard error to avoid the foot-gun — tracked as `[TD-TEMPORAL-GATE]`.

---

## Q-PLAN-005 — `SessionContext.feature_toggles` typing: free-form vs typed catalog

**Question.** `34 §4` ratifies `feature_toggles: BTreeMap<String, FeatureToggleValue>` — a free-form map. Should it instead be a closed enum (a typed catalog of known toggles) or an open enum with reserved prefixes?

**Refs.**
- `34 §4.1` — current shape.
- `34 §11.4` — `OptimizerPass` is the primary toggle consumer.
- `30 §3.5` — `#[non_exhaustive]` discipline.

**Arguments pro free-form.**
- Adapter authors and third-party passes can register their own toggles without a `34` crate bump.
- Matches industry convention for "feature-flag bag" APIs.

**Arguments pro typed catalog.**
- Typos become compile errors.
- Each toggle's semantics is documented in one place (the enum's doc comments).
- Cross-service toggle propagation is easier when the value space is closed.

**Current position in `34`.** Free-form `BTreeMap<String, FeatureToggleValue>`; the `semstrait.*` prefix is reserved for built-in toggles.

**Next step.** Keep free-form through v1. If the toggle surface exceeds ~10 stable entries, introduce a typed catalog alongside (not replacing) the free-form map — the typed view is a read-through helper, the map remains the source of truth.

---

## Q-PLAN-006 — `OptimizerPass` idempotence: proof obligation vs convention

**Question.** `34 §14.5` states that canonical v1 passes are idempotent and that third-party passes are "encouraged but not enforced" to be. Should the trait ratify idempotence as a proof obligation (e.g. a contract method `verify_idempotent`) or is convention enough?

**Refs.**
- `34 §14.5` — current posture.
- `34 §12.3` — soundness posture (no static enforcement).
- `10 §3.5` — `optimize`-stage contract.

**Arguments pro proof obligation.**
- Re-optimize becomes safe by construction; no "did the author remember?" risk.
- Makes the contract explicit in code.

**Arguments pro convention.**
- Idempotence cannot be verified statically by Rust's type system. Any `verify_idempotent` would be either a round-trip assertion (test-only) or a runtime check that costs one extra pass.
- Convention is cheaper and matches the `OptimizerPass` soundness posture (per `34 §12.3`).

**Current position in `34`.** Convention only.

**Next step.** Add an integration test (`semstrait-planner::tests::optimize::idempotence`) that applies the default pass chain twice to a corpus of plans and asserts byte-identical output. Keep convention for third-party passes but document the test harness so authors can self-check.

---

## Q-PLAN-007 — `ResolvedQueryRequest` visibility: `pub` vs `pub(crate)`

**Question.** `34 §5` exposes `ResolvedQueryRequest` as `pub` so `Strategy` impls (including third-party strategies per Q-PLAN-002) can consume it. Given that the only consumer is the `Strategy` trait's `plan` method, should it be `pub(crate)` instead and the trait's `plan` parameter use a sealed type?

**Refs.**
- `34 §5` — current `pub` posture.
- `34 §8.1` — `Strategy::plan` signature consuming `&ResolvedQueryRequest`.
- `30 §3.3` — public-surface discipline.

**Arguments pro `pub(crate)`.**
- Smaller public surface. Less for consumers to reason about.
- Eliminates the risk of callers constructing malformed `ResolvedQueryRequest` directly.

**Arguments pro `pub`.**
- Required for Q-PLAN-002's open-trait posture; if `Strategy` is open, its parameter types must be.
- Enables adapter-side debugging / tracing hooks to inspect the resolved form.

**Current position in `34`.** `pub`, with `pub(crate)` constructors (§5.1 lead-in).

**Next step.** Resolve jointly with Q-PLAN-002. If `Strategy` is sealed, move `ResolvedQueryRequest` to `pub(crate)`; if open, keep it `pub` with constructor-side discipline.

---

## Q-PLAN-008 — Field-first depth bound — CLOSED (2026-04-28)

> **Moved to [`../closed/34_questions.md`](../closed/34_questions.md#q-plan-008--field-first-depth-bound-max_implicit_composition_depth--closed-2026-04-28).** Mirrored from Q-COMP-001 at value `4`.

---

## Q-PLAN-009 — `StrategyRegistry` construction vs. mutation

**Question.** `34 §8.3` ratifies `StrategyRegistry` as construction-time only — no `replace_simple(...)` method. Is that the right discipline, or should the registry be mutable to support live A/B-testing of a replacement strategy?

**Refs.**
- `34 §8.3` — current shape.
- `34 §15.2` — built-in-strategies stability posture.
- `I4` — determinism.

**Arguments pro construction-only.**
- Determinism: the plan produced by a `StrategyRegistry` is a pure function of its construction. Mutating a registry mid-life creates non-deterministic plans for the same Request.
- Simpler thread-safety story.

**Arguments pro mutable.**
- Live A/B testing of a new strategy without re-starting the server.

**Current position in `34`.** Construction-only.

**Next step.** Keep construction-only through v1. For A/B testing, recommend spinning up a second registry and routing a traffic fraction through it at the API layer — the registry is cheap to construct.

---

## Q-PLAN-010 — `PlanError` per-variant wrapper vs flat enum

**Question.** `34 §13.1` ratifies the `PlanError` enum with per-variant wrappers for `21`–`24` (`Simple(SimpleError)`, `Grainset(GrainsetError)`, etc.). Should the variants be flattened into `PlanError` directly (one enum per variant per DataKind) or kept as wrapper types?

**Refs.**
- `34 §13.1` — current wrapper posture.
- `21 §7` / `22 §8` / `23 §10` / `24 §10` — per-DataKind error enums.
- `30 §6.2` — stable-code discipline.

**Arguments pro wrapper.**
- Keeps per-DataKind error modules self-contained.
- Each DataKind doc owns its error taxonomy (`21 §7`, `22 §8`, etc.).
- Adding a new error variant to one DataKind does not touch `34`.

**Arguments pro flat.**
- Pattern-match users see all planner errors in one enum.
- Simpler for `#[derive]` macros.

**Current position in `34`.** Wrapper per variant.

**Next step.** Keep wrapper posture. Revisit if downstream consumers complain about the extra match-nesting — at which point a `PlanError::all_codes() -> impl Iterator<Item = &'static str>` convenience can paper over the ergonomic cost.

---

## Cross-doc fixes flagged while drafting `34`

Tracked out of band; none block `34`'s ratification but should be resolved before v1.0.

| ID | Location | Fix |
|---|---|---|
| CDF-30-02 | `30 §6.2` `PLAN_E` row | Spell out the `PLAN_E_05xx` sub-bands explicitly (`0500` constraint, `0501`–`0509` composition, `0510`–`0519` request-shape, `0520`–`0529` filter-shape) once Q-PLAN-003 resolves. |
| CDF-21-01 | `21 §7` | Cross-reference `34 §13` as the aggregation surface (`PlanError::Simple(SimpleError)` wraps `PLAN_E_21xx`). |
| CDF-22-01 | `22 §8` | Same as CDF-21-01 for `PLAN_E_22xx` and `PlanError::Grainset(GrainsetError)`. |
| CDF-23-01 | `23 §10` | Same as CDF-21-01 for `PLAN_E_23xx` and `PlanError::Unionset(UnionsetError)`. |
| CDF-24-01 | `24 §10` | Same as CDF-21-01 for `PLAN_E_24xx` and `PlanError::Joinset(JoinsetError)`. |

## Deferred / known-gap migration items

Tracked in `implementation/40_refactor_plan.md`, not in open-questions:

- `[TD-PLANNER-SHAPE]` — rename `SemanticPlanner` struct → free `plan` / `optimize` functions at the crate root (per `34 §6` / §11).
- `[TD-SESSION-CONTEXT]` — legacy `SessionVariables: HashMap<String, String>` → typed `SessionContext` (per `34 §4`).
- `[TD-REQUEST-SHAPE]` — legacy `ResolvedQueryRequest` folds Request + SessionContext + partial resolution; v1 splits into `Request` (caller surface) + `ResolvedQueryRequest` (internal post-lookup form) per `34 §3` / §5.
- `[TD-ADHOC-INTO-FIELD-FIRST]` — legacy `AdHocJoin` dispatch path subsumed by field-first resolution (per `34 §9.5`).
- `[TD-PLANNER-NO-CATALOG]` — drop the optional `SemanticPlanner.catalog` field once ad-hoc-join paths migrate to field-first resolution (per `34 §16.4`).
- `[TD-CONSTRAINT-ERROR-FANOUT]` — v1 `ConstraintValidator` short-circuits on first violation; future work may accumulate multiple violations in one pass (per `34 §14.2`, originally flagged in `11 §8.7`).
