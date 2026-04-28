---

## doc: design/questions/open/16_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `foundations/16_composition.md`
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

# Open Questions — `foundations/16_composition.md`

> Items surfaced during Round-1 drafting of the composition foundations doc. Each entry restates the question, lists its ratified references, and records the Round-1 default `16` currently uses. Entries migrate out of this file as later docs (`17`, `23`, `32`, `33`, `34`) make decisions that either confirm or amend `16`'s defaults. None of these open items block the three headline ratifications (`Q1`/`Q2`/`Q3` in `16 §16`).

---

## Q-COMP-001 — `MAX_IMPLICIT_COMPOSITION_DEPTH` value

**Question.** `16 §9.1` bullet 3 ratifies a depth limit on implicit composition. Round-1 sets `MAX_IMPLICIT_COMPOSITION_DEPTH = 4`. Is `4` the right value for v1?

**Refs.**

- `16 §9.1` — boundary ratification.
- `16 §14.3` — `PLAN_E_0502 CompositionDepthExceeded`.
- `34` (pending) — planner entry point; exposes the limit as a constant.

**Proposed (Round 1):** `4` hops. Covers common star / snowflake patterns (fact → dim → outrigger → …) without endorsing arbitrarily deep anonymous walks. Authors wanting deeper compositions are nudged toward explicit `Joinset`.

**Arguments for `4`.**

- Kimball-style analytic workloads rarely exceed 3-hop dim outrigger chains.
- Larger values admit implicit compositions authors likely did not mean; smaller values reject legitimate patterns.
- Matches the industry-folklore "keep dim hierarchies shallow" guidance.

**Arguments for a larger value (say, `8`).**

- Some graph-oriented models (e.g. organizational hierarchies, social-network-style aggregations) genuinely need deeper walks.
- Larger limit defers the "switch to explicit `Joinset`" moment.

**Arguments for a smaller value (say, `2`).**

- Encourages explicit modeling earlier.
- Safer default for Round-1 when the implicit-composition algorithm has no real-world usage.

**Current position in `16`.** `4` hops. The constant lives in `semstrait-planner` and is not author-configurable in v1.

**Next step.** Revisit at `34` drafting with real-Model sample data; if early usage consistently runs into `PLAN_E_0502` on sane models, raise to `6` or `8`. If early usage indicates authors are accidentally constructing deep compositions, lower to `3`.

---

## Q-COMP-002 — Ambiguous-path tie-breaking: could there be a deterministic heuristic?

**Question.** `16 §11.4` / `§9.1` bullet 2 ratifies "ties → error, no heuristic choice." Alternatives exist — e.g. prefer the path whose `RelationshipId`s are lexically smallest, or prefer the path with fewer `ManyToMany` edges. Should v1 adopt any such heuristic?

**Refs.**

- `16 §11.4` — BFS determinism via neighbor order.
- `16 §9.4` bullet 2 — rationale for error-on-tie.
- `16 §14.3` — `PLAN_E_0500 AmbiguousImplicitComposition`.

**Proposed (Round 1):** Error on ties. Authors must either declare an explicit `Joinset` (pinning the path) or remove the ambiguity by deleting / narrowing one of the candidate `Relationship`s.

**Arguments for error.**

- I4 determinism: the author can always predict the answer because "tie = error" is a clear rule.
- Heuristics inject judgment authors cannot trace without reading the planner internals.
- Explicit `Joinset` is the escape hatch and is designed for exactly this case.

**Arguments for a heuristic.**

- Rejects legitimate queries on Models with natural cross-join-key structures (e.g. a fact with two FKs to the same dim table via different semantic roles).
- Authors may prefer "some answer" over a compile error for exploratory queries.

**Current position in `16`.** Error on ties. Authors disambiguate by declaring a `Joinset`.

**Next step.** Revisit post-v1 if user studies show the error fires on legitimate work more than it rejects malformed queries. A candidate heuristic (lexically-smallest `RelationshipId`) could be added as an opt-in planner flag.

---

## Q-COMP-003 — Implicit-composition Steiner-tree solver sophistication

**Question.** `16 §11.4`'s "multi-target BFS" is a simplified Steiner-tree approximation: find a subgraph connecting all owning kinds with minimum total hop count. Should v1 use an optimal Steiner-tree solver (NP-hard in general) or is the BFS approximation sufficient?

**Refs.**

- `16 §11.4` — multi-target BFS.
- `16 §11.5` — synthesis consumer.

**Proposed (Round 1):** BFS approximation. Graph size is small enough (10s–100s of `Relationship`s) that exhaustive enumeration of candidate cover trees up to `MAX_IMPLICIT_COMPOSITION_DEPTH` is feasible. For typical Models the approximation coincides with the optimum.

**Arguments for the approximation.**

- Round-1 scale (10s of DataKinds, 10s–100s of Relationships) is well within brute-force enumeration.
- Exact Steiner tree is NP-hard; investing in a sophisticated solver is premature.
- Determinism is easy to maintain with enumeration.

**Arguments for a sophisticated solver.**

- Pathological Models (thousands of kinds, dense Relationship graphs) would benefit.
- If the planner's implicit-composition time becomes a hot path, better algorithms matter.

**Current position in `16`.** Brute-force BFS enumeration within the depth bound.

**Next step.** Revisit when / if profiling shows implicit composition as a hot path. Well-established 2-approximation algorithms for Steiner tree exist and can be dropped in without changing the surface contract.

---

## Q-COMP-004 — Should implicit composition produce a `Joinset`-style surface or a bespoke `Relationship`-kind surface?

**Question.** `16 §5.3` introduces `CompositionKind::Relationship` as the implicit-composition discriminator, distinct from `CompositionKind::Joinset`. Arguably, implicit compositions could return a `CompositionKind::Joinset` with the `Joinset` being planner-synthesized. Rejected in `16` because an unnamed surface has no YAML-level name; keeping it distinct avoids the planner faking a `Joinset` identity.

**Refs.**

- `16 §5.3` — `CompositionKind` roster.
- `16 §13.5` — Joinset-reuse open item (`[TD-COMPOSITION-JOINSET-REUSE]`).

**Proposed (Round 1):** Keep `CompositionKind::Relationship` distinct from `CompositionKind::Joinset`. Implicit compositions do not carry a name; they are request-local.

**Arguments for distinct.**

- Clean mental model: named / persisted → `Joinset`; anonymous / request-local → `Relationship`.
- The planner dispatches differently: `Joinset` strategies may assume an author-declared anchor; implicit `Relationship` strategies work from the traversed path.
- `Joinset` may have author-declared overrides (join-type, traversal order); implicit `Relationship` has none.

**Arguments for unifying.**

- Fewer discriminator branches in the planner.
- Implicit compositions could "promote" to a named `Joinset` lazily if the author later declares one covering the same kinds — `[TD-COMPOSITION-JOINSET-REUSE]`.

**Current position in `16`.** Distinct.

**Next step.** Revisit at `[TD-COMPOSITION-JOINSET-REUSE]` realization. If the planner's Joinset-reuse optimization ships, the discriminator may collapse; until then, distinct.

---

## Q-COMP-005 — Scope of `PLAN_W_0501 FanoutAdvisory`: error-by-default under `strict` mode?

**Question.** `16 §14.4` ratifies `PLAN_W_0501` as a Warning (planner proceeds, advises). Should a future `strict` planner mode promote it to an Error?

**Refs.**

- `16 §3.3.2` — fanout-safe rewrite description.
- `16 §14.4` — advisory ratification.
- `30 §12` — deprecation / lifecycle policy.

**Proposed (Round 1):** Warning only in v1. `strict` mode deferred.

**Arguments for deferring strict mode.**

- v1 planner has no configuration surface; adding one is out of scope.
- Authors who want strict behavior can post-process the Diagnostic list.

**Arguments for strict mode.**

- Analytic-engineering teams may want fanout-triggering queries to fail compile rather than ship silently with a rewrite.
- Aligns with "surprising runtime semantics are worse than compile-time rejection" stance from `15 §9.4`.

**Current position in `16`.** Warning only; strict mode deferred.

**Next step.** Consider in `34` if user feedback requests it. A strict-mode planner flag is an additive change (MINOR).

---

## Q-COMP-006 — Cross-composition-kind chaining (the §9.1 bullet 5 prohibition)

**Question.** `16 §9.1` bullet 5 prohibits chaining implicit composition with already-composed surfaces of another `CompositionKind`. Is the prohibition too strict for v1?

**Refs.**

- `16 §9.1` bullet 5 — rule.
- `16 §9.4` — rationale.
- `16 §13.5` — explicit `Joinset` coexistence with implicit compositions.

**Proposed (Round 1):** Keep the prohibition. A composed surface from one pass is never fed into another as a constituent within the same Request.

**Arguments for the prohibition.**

- Preserves the mental model: implicit composition is a flat graph walk, not recursive synthesis.
- Avoids correctness hazards (e.g. recomputing `UnifiedSemantics` over already-unified surfaces).
- Authors with cross-composition needs can declare an explicit `Joinset` / `Unionset` that names the full composition.

**Arguments for relaxing.**

- Some Models would benefit from "query a `Unionset`, then pull in a related dimension from an outside kind via a `Relationship`." Prohibiting this forces declaring a dedicated `Joinset` over the `Unionset`.
- The BFS algorithm could be extended to handle the case.

**Current position in `16`.** Prohibition. Authors declare explicit surfaces for cross-kind-kind compositions.

**Next step.** Revisit at `34` drafting if common use-cases demonstrate the prohibition is over-broad. Extension would be additive (lift the check, add a new `CompositionKind::Mixed` variant for the result — MINOR per I10).

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

## Q-COMP-011 — `traversed_paths` on implicit compositions: single path vs path per leg

**Question.** `16 §11.5` step 2 says multi-target BFS may produce a tree, flattened to `Vec<RelationshipPath>`, one per "leg." Should an implicit composition instead carry a single canonical path that visits all constituents (via arbitrary ordering)?

**Refs.**

- `16 §5.2` — `traversed_paths: Vec<RelationshipPath>`.
- `16 §11.5` — synthesis.

**Proposed (Round 1):** Vec per leg. A tree cover is more general than a single path; Requests over 3+ owning kinds may genuinely need a tree shape.

**Arguments for Vec per leg.**

- General.
- Tree-covers are real for `>2` owning kinds.

**Arguments for single path.**

- Simpler.
- Only works for `<=2` owning kinds;  `3+` needs a tree.

**Current position in `16`.** Vec per leg.

**Next step.** No change expected. Shape is forward-compatible.

---

## Q-COMP-012 — `Joinset` reuse optimization (`[TD-COMPOSITION-JOINSET-REUSE]`)

**Question.** When an implicit-composition request spans exactly the kinds of a declared `Joinset`, could the planner reuse the `Joinset`'s pre-built `ComposedSemanticInterface` instead of synthesizing a new one?

**Refs.**

- `16 §13.5` — rule: do not reuse (Round 1).
- `16 §9.1` bullet 1 — `CompositionKind` identity mismatch.

**Proposed (Round 1):** Do not reuse. Implicit and explicit compositions are distinct instances even when they cover the same kinds.

**Arguments for deferring reuse.**

- Simplifies the planner.
- `Joinset` may carry author-declared join-type overrides (§13.3) that an implicit-composition request did not ask for; reusing blindly would change query semantics.
- Authors seeking the `Joinset`'s semantics can write `from: "<joinset-name>"` explicitly.

**Arguments for reuse.**

- Compile-time savings.
- Coherent user story: "the `Joinset` is the canonical composition for these kinds; why synthesize another?"

**Current position in `16`.** Do not reuse (`[TD-COMPOSITION-JOINSET-REUSE]`).

**Next step.** Revisit if the reuse optimization's value becomes apparent in `34` planner benchmarks. If the `Joinset`'s overrides and the implicit composition's expected defaults align, reuse is safe; if they disagree, it is not.

---

## Q-COMP-013 — Should explicit `Relationship`s between composed kinds (e.g. `Joinset → Simple`) be permitted?

**Question.** `16 §2.1` says a `Relationship` is "between two top-level DataKinds." Top-level kinds include `Simple`, `Unionset`, `Grainset`, `Joinset`. Should `Relationship` between, say, a `Joinset` and a `Simple` be permitted?

**Refs.**

- `16 §2.1` — placement.
- `12 §2` — nesting matrix; `Joinset` can nest other kinds but is itself top-level.
- `16 §9.1` bullet 5 — prohibition on cross-composition-kind chaining (for implicit walks).

**Proposed (Round 1):** Permitted. A `Relationship` declares joinability between any two top-level kinds, including composed ones. Implicit composition (§9.1 bullet 5) prohibits chaining different `CompositionKind`s in one walk, but authoring a `Relationship` that references a `Joinset` as a side is not implicit chaining; it's a first-class edge.

**Arguments for permitting.**

- Maximum expressive power.
- `Joinset → Simple` is a common pattern: "a canonical cross-platform view joined to an outrigger dim."

**Arguments against.**

- The composed kind's surface is large; `KeyPair.left` referencing a namespaced name from within the composed surface is unergonomic.
- Implicit walks cannot chain anyway, so what does the Relationship buy?

**Current position in `16`.** Permitted. Author writes `KeyPair.left` as a namespaced `SemanticsName` (e.g. `"paid_media.campaign_id"`) to reference into the composed surface.

**Next step.** Revisit at `32` YAML surface design — the author-facing authoring shape may make or break the feature's ergonomics.

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

## Q-COMP-016 — `Cardinality::ManyToMany` — warn-only or reject by default?

**Question.** `16 §3.3.4` permits `ManyToMany` with a `PLAN_W_0502` advisory nudging authors toward junction-table modeling. Should v1 reject `ManyToMany` outright and force junction-table modeling?

**Refs.**

- `16 §3.3.4` — per-variant semantics.
- `16 §14.4 PLAN_W_0502`.

**Proposed (Round 1):** Permit with advisory. Some legitimate Models need `ManyToMany` (e.g. a tag system where tags and articles are a genuine many-to-many without a modeled junction).

**Arguments for permitting.**

- Expressive completeness.
- Authors can opt into the fanout consequences knowingly.

**Arguments for rejection.**

- Forces clearer modeling.
- Reduces correctness surprises.

**Current position in `16`.** Permit with advisory.

**Next step.** Revisit if advisory fatigue sets in. A strict mode (Q-COMP-005) could promote the advisory to an error.

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

---

## Q-COMP-018 — `ComposedSemanticInterface.keys` on implicit compositions: empty vs derived?

**Question.** `16 §6.5` says implicit compositions have no composed-surface keys. Could the planner derive keys (e.g. from the anchor constituent's keys) when useful?

**Refs.**

- `16 §6.5` — rule.
- `16 §11.5` — synthesis.

**Proposed (Round 1):** Empty. Implicit compositions do not claim keys; author-addressable keys require an explicit surface (`Joinset`).

**Arguments for empty.**

- Implicit compositions are request-scoped; the "key" of the composed surface is meaningful only for the planner's grouping logic, not for the author's future reference.
- Explicit key declaration is the author's privilege on explicit surfaces.

**Arguments for deriving.**

- Some planner strategies benefit from a key on the composed surface (e.g. deduplication with a pinned key column).
- Author never sees the derived key; it's internal.

**Current position in `16`.** Empty. Planner derives internally-needed keys from `Cardinality` / anchor constituent on a per-strategy basis, outside the `ComposedSemanticInterface.keys` field.

**Next step.** Revisit at `34` if planner strategies consistently need to decorate composed surfaces with derived keys. Extension would be a MINOR field on `ComposedSemanticInterface`.

---