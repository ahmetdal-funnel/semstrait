---
doc: design/questions/open/16_questions
status: Living
purpose: Open questions surfaced while drafting `foundations/16_composition.md`
depends-on:
  - foundations/16_composition.md
  - foundations/11_names_and_scopes.md
  - foundations/12_nesting_policy.md
  - foundations/14b_expression_resolution.md
  - foundations/15_mapping_and_binding.md
  - foundations/17_temporal_shape.md
  - data-kinds/23_joinset.md
  - apis/32_semstrait_model.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
---

# Open Questions — `foundations/16_composition.md`

> **Status (post-thirteenth-pass cascade rebase, 2026-05-03).** Nine framework-level questions remain open. New: Q-COMP-006 (post-rebase Unionset-retirement structural cleanup of §9.3 / §10.5 / §13 + downstream cross-refs). Carried-over: Q-COMP-007 (Directionality granularity), Q-COMP-008 (compile-time reverse-traversal detection), Q-COMP-009 (composite-key shape alternatives), Q-COMP-010 (CompositionCoverage serialization shape), Q-COMP-014 (PLAN_E_0505 candidate suggestions), Q-COMP-015 (FieldOwnership::Derived distinctness), Q-COMP-016 (ManyToMany reject-by-default), Q-COMP-017 (YAML default for JoinType). Closed items moved to [`../closed/16_questions.md`](../closed/16_questions.md). None of the items below block the headline ratifications in `16 §16` (note: §16 itself may need re-numbering when Q-COMP-006 lands).

---

## Q-COMP-006 — Post-rebase Unionset-retirement structural cleanup

**Question.** The post-thirteenth-pass cascade rebase (2026-05-03) shrunk `CompositionKind` from `{Joinset, Unionset, Grainset}` to `{Joinset, Grainset}` — Unionset uses its own bare `SemanticInterface` (per `23 §3.2`) and no longer participates in `ComposedSemanticInterface`. The `16 §5` ratification block at the top of `§5` documents this; `§5.3`'s `CompositionKind` enum was tightened. But three deeper sections still carry pre-rebase Unionset material that became inert:

- **§9.3 Implicit-Unionset enumeration sketch** — describes a coverage-overlap enumeration that no longer feeds `ComposedSemanticInterface`. In V1, implicit Unionsets surface inside Dataset (multi-source per `21 §3.2`) and Grainset (same-grain pre-merge per `22 §3.3`); both bake their own enumeration logic without `16`'s involvement.
- **§10.5 Implicit-`Unionset` enumeration** — full algorithm sketch. Same retirement rationale.
- **§13 Joinset section** + scattered cross-refs to `CompositionKind::Unionset` throughout §10 / §11 / §14.

Should these sections be:

- **A** — deleted outright (clean slate; no V1 Unionset enumeration via `16` at all);
- **B** — preserved with a "RETIRED" banner pointing readers at the variant-internal mechanisms;
- **C** — restructured as a generic "implicit-composition enumeration framework" with two V1 instances (Joinset; Grainset cross-grain JOIN-tree) and zero Unionset instances?

**Refs.**

- `16 §5` ratification block (top of §5) — current scope adjustment.
- `16 §5.3` — `CompositionKind { Joinset, Grainset }` (V1).
- `16 §9.3` — implicit-Unionset enumeration sketch (now inert).
- `16 §10.5` — implicit-Unionset enumeration (now inert).
- `21 §3.2` — Dataset multi-source implicit Unionset (strict NullFill discipline).
- `22 §3.3` — Grainset same-grain pre-merge implicit Unionset (non-strict NullFill discipline).
- `23 §3.2` — Unionset bare `SemanticInterface` ratification (post-cascade rebase).

**Proposed (preliminary).** Option B for the next pass (preserve with banner; minimum-disruption); promote to Option C in a Round-4 framework cleanup if the structural symmetry between Joinset enumeration and Grainset cross-grain JOIN-tree construction makes the framework worth extracting.

**Arguments for A (delete).**

- Cleanest. No vestigial inert text.
- Unionset enumeration logic now lives where it belongs (variant-internal: `21` / `22` / `23`).

**Arguments for B (banner).**

- Minimum disruption. Preserves doc cohesion and historical decision context.
- Readers who land in §9.3 / §10.5 from external references see the banner immediately.

**Arguments for C (restructure as generic framework).**

- Forward-compatible: Grainset cross-grain JOIN-tree construction is structurally similar to Joinset Steiner enumeration; both could share a "implicit-composition enumeration" abstraction.
- Heaviest lift; risks over-abstraction.

**Current position in `16`.** Pre-rebase text retained with the §5 ratification block flagging V1 scope (option B-equivalent, lightweight). Deeper structural cleanup deferred to this question.

**Next step.** Resolve before Round-4 (`16` framework cleanup pass). Decision should align with whether `34`'s Strategy chapter prefers per-variant or framework-level enumeration logic.

---

## Q-COMP-007 — `Directionality` granularity: per-`Relationship` vs per-direction

**Question.** `16 §2.4` ratifies `Directionality` as a per-Relationship field with variants `Bidirectional` / `Forward`. An alternative would be per-direction flags: `{ forward_walkable: bool, reverse_walkable: bool }`. Should v1 adopt the granular form?

**Refs.**

- `16 §2.4` — enum ratification.
- `16 §11.4` — BFS direction filtering.

**Proposed (Round 1):** Enum form with two variants. `Reverse` (reverse-only) is not in v1; if needed, authors can swap `from` / `to`.

**Arguments for the enum (current).**

- Simpler surface.
- Covers 99% of real-world needs (bidirectional is overwhelmingly common; forward-only is rare but real; reverse-only is re-expressible as forward-only with sides swapped).
- Extension to add a `Reverse` variant is MINOR per I10.

**Arguments for per-direction flags.**

- Fully general.
- Slightly more ergonomic for authors who think "which directions work" rather than "what category of edge is this."

**Current position in `16`.** Enum. `#[non_exhaustive]` per I10; variants may grow.

**Next step.** If authors consistently need `Reverse`-only or `Neither`, extend the enum. Otherwise keep as-is.

---

## Q-COMP-008 — `Directionality::Forward` — is reverse-traversal error the right surface?

**Question.** `16 §14.3 PLAN_E_0503 CrossCompositionForbidden` fires when the planner attempts to walk a `Forward` relationship in reverse. Should the error be raised at `validate` / `compile` (pre-planning) if the author's declared `Relationship`s cannot cover a needed direction?

**Refs.**

- `16 §2.4.2` — `Forward` use-cases.
- `16 §14.3` — error code.

**Proposed (Round 1):** Plan-time error. `validate` / `compile` cannot know which directions a Request will need.

**Arguments for plan-time (current).**

- The need for a reverse traversal depends on the Request (its `select:` shape); it is a request-specific error.
- Moving to compile-time would require proactively analyzing "what if a Request needs this?" — arbitrary.

**Arguments for compile-time.**

- Would catch some pathologies earlier.
- Unworkable in general (see above).

**Current position in `16`.** Plan-time. `PLAN_E_0503`.

**Next step.** No change expected. Documented for completeness.

---

## Q-COMP-009 — Composite keys: positional pairing vs named pairing

**Question.** `16 §2.3` ratifies "composite keys use multiple positional `KeyPair` entries; ordering is significant." An alternative would be a single `KeyPair` whose `left` / `right` are `Vec<SemanticsName>` — symmetric shape, explicit cardinality of N, no reliance on list ordering. Should the alternative be adopted?

**Refs.**

- `16 §2.3` — shape ratification.

**Proposed (Round 1):** Positional pairs (multiple `KeyPair` entries, one per column). Matches common foreign-key declaration style; YAML surface (`32`) is natural.

**Arguments for positional pairs (current).**

- Each `KeyPair` is self-contained; type-agreement check runs per-pair.
- Easier to emit as SQL predicates (`A.col_i = B.col_i`).
- YAML is a list of pairs; natural authoring.

**Arguments for single-entry with `Vec<SemanticsName>`.**

- Makes the composite nature explicit.
- One entry is clearer than "N entries that together form one key."
- Type-agreement check is bulkier but no harder.

**Current position in `16`.** Positional pairs.

**Next step.** Revisit at `32` YAML drafting if the shape turns out unergonomic. Migration would be MAJOR.

---

## Q-COMP-010 — `CompositionCoverage` — serialize per-constituent or collapsed?

**Question.** `16 §8.2`'s `CompositionCoverage` is keyed by `(DataKindRef, UnifiedName)` — one entry per constituent per name. An alternative would be a per-name entry with a `HashMap<DataKindRef, CoverageVariant>` value. Should Round-1 use the collapsed shape?

**Refs.**

- `16 §8.2` — keyed-by-tuple ratification.
- `15 §6` — Binding-level Coverage (flat `HashMap<(SourceIndex, SemanticsName), CoverageVariant>`).

**Proposed (Round 1):** Keyed by `(DataKindRef, UnifiedName)`. Matches `15 §6`'s shape for consistency.

**Arguments for tuple-keyed (current).**

- Symmetric with `15 §6`.
- Simple lookup.
- Empty entries cost nothing extra in a `HashMap`.

**Arguments for nested.**

- Slightly smaller on-wire footprint when serializing (one nested map vs flat tuples).
- "All constituents for this name" query is cheaper.

**Current position in `16`.** Tuple-keyed.

**Next step.** Revisit if SemanticManifest on-disk size becomes material (unlikely for the composition-coverage index specifically).

---

## Q-COMP-014 — Should `PLAN_E_0505 AmbiguousCompositionReference` include suggested qualifications?

**Question.** `16 §14.3` `PLAN_E_0505` fires when a bare name on a composed surface is ambiguous. Diagnostic currently carries `(name, candidates)`. Should the Diagnostic include the exact qualified-name forms the author can use (e.g. `orders.total`, `returns.total`)?

**Refs.**

- `16 §14.3` — variant definition.
- `30 §5.3` — `ContextLine` use for suggestions.

**Proposed (Round 1):** Yes — the `context: Vec<ContextLine>` on the emitted `Diagnostic` includes one `ContextLine` per candidate qualification, with a "use this form" label.

**Arguments for suggestions.**

- Classic "did you mean X?" UX.
- `30 §5` ratifies `ContextLine` for exactly this.

**Arguments against.**

- Slight overhead in error construction.

**Current position in `16`.** Suggestions included in context lines.

**Next step.** Implementation detail; no doc change needed.

---

## Q-COMP-015 — `FieldOwnership::Derived`: how common, and does it warrant its own variant?

**Question.** `16 §7.3.4`'s `FieldOwnership::Derived(PhysicalExpr)` captures fields that exist only on the composed surface. Is this common enough to warrant a dedicated variant, or could it be folded into `Native` (with the "native provider" being a synthetic `DataKindRef` for the composition itself)?

**Refs.**

- `16 §7.2` — `FieldOwnership` roster.
- `15 §6.3` — parallel concern on Binding-level Coverage.

**Proposed (Round 1):** Keep as a distinct variant. Composition-level-derived fields are structurally different from constituent-native fields — they carry a `PhysicalExpr` that only makes sense in the composed scope.

**Arguments for distinct.**

- Planner logic can quickly skip constituent scans for `Derived` fields (they don't need a constituent contribution).
- `PhysicalExpr` payload has no home on `Native`.

**Arguments for folding.**

- Fewer variants on a public enum.

**Current position in `16`.** Distinct.

**Next step.** Revisit at `34` if the planner's dispatch on `FieldOwnership` rarely branches on `Derived`.

---

## Q-COMP-016 — `Cardinality::ManyToMany` — permit or reject by default?

**Question.** `16 §3.3.4` permits `ManyToMany`. Should v1 reject `ManyToMany` outright and force junction-table modeling?

**Status (Round 2, 2026-04-29).** Framing simplified: the advisory dimension is moot now that `PLAN_W_0502 ManyToManyFanoutAdvisory` was retired with Q-COMP-005. The remaining axis is **permit vs reject** for `ManyToMany` itself.

**Refs.**

- `16 §3.3.4` — per-variant semantics for `ManyToMany`.

**Proposed (Round 1):** Permit. Some legitimate Models need `ManyToMany` (e.g. a tag system where tags and articles are a genuine many-to-many without a modeled junction).

**Arguments for permitting.**

- Expressive completeness.
- Authors can opt into the fanout consequences knowingly.

**Arguments for rejection.**

- Forces clearer modeling.
- Reduces correctness surprises.

**Current position in `16`.** Permit.

**Next step.** Revisit during a Round-3 model-discipline pass if real-world models reveal pervasive misuse.

---

## Q-COMP-017 — Should `Relationship.join_type` default differ from `JoinType::Inner`?

**Question.** `16 §2.2` lists `join_type` as required with no default. Author must pick. Should a YAML-surface default apply (e.g. `Inner`)?

**Refs.**

- `16 §2.2` — struct shape.
- `32` (pending) — YAML defaults.

**Proposed (Round 1):** Required at the canonical layer; YAML surface (`32`) MAY default to `JoinType::Inner` for ergonomics. The canonical struct always carries an explicit value.

**Arguments for YAML default.**

- Common case is `Inner`.
- Reduces authoring friction.

**Arguments against.**

- Explicit is better than implicit for semantically charged decisions.
- `Left` is arguably common for fact → dim patterns.

**Current position in `16`.** Required canonically; defaulted in YAML at `32`'s discretion.

**Next step.** `32` ratifies the YAML default.
