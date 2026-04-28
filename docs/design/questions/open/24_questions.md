---
doc: design/questions/open/24_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `data-kinds/24_joinset.md`
depends-on:
  - data-kinds/24_joinset.md
  - foundations/11_names_and_scopes.md
  - foundations/12_nesting_policy.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/15_mapping_and_binding.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
  - apis/30_api_contracts.md
  - apis/32_semstrait_model.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
---

# Open Questions — `data-kinds/24_joinset.md`

> Items surfaced during Round-1 drafting of the Joinset specification. Each entry restates the question, lists its ratified references, and records the Round-1 default `24` currently uses. Entries migrate out of this file as later docs (`17`, `25`, `32`, `33`, `34`, `35`) make decisions that either confirm or amend `24`'s defaults. None of these open items block the core `Joinset` ratifications in `24 §§2–11`.

---

## Q-24-01 — N-ary Joinset lift (`TD-NESTING-NARY-JOIN` / `TD-JOINSET-NARY`)

**Question.** `12 §5.2` ratifies binary-only Joinsets in v1; `24 §2.5` restates the constraint. The canonical struct in `24 §2.2` is N-ary-ready (`members: Vec<DataKindRef>`, `ExplicitPath { hops: Vec<JoinHop> }`). When does N-ary Joinset graduate from tech-debt to a MINOR release, and what is the exact authoring / compile contract that lifts?

**Refs.**
- `12 §5.2` — v1 binary arity, `TD-NESTING-NARY-JOIN`.
- `24 §2.2` — canonical struct sketch (N-ary-ready shape).
- `24 §4.2.2`, `24 §5.4` — explicit-path and fanout rules that are binary-v1-degenerate but N-ary-forward-looking.
- `24 §10.1` — `COMP_E_2403`, `COMP_E_2407`, `COMP_E_2410` are v1-unreachable; N-ary-forward-looking.
- `16 §13.3` — per-traversal `JoinType` overrides in explicit `Joinset` (already contemplates N-hop surface).

**Proposed (Round 1):** Deferred. The v1 Joinset is binary; N-ary is the recognized next step with the canonical struct already lifted to support it.

**Arguments for early lift (v1.1 MINOR).**
- The struct shape is already N-ary-ready; lifting requires only loosening `12 §5.3`'s arity check and extending the implicit-path BFS to handle N target members (Steiner-tree-style, as `16 §11.4` already sketches).
- Star-schema authoring is awkward when a single logical surface (`orders with customers, products, dates`) requires three separate binary Joinsets. N-ary removes the awkwardness.
- `14b §4.5`'s `PathSignature: BTreeSet<RelationshipPath>` already assumes multi-path traversal at the expression layer.

**Arguments for further deferral.**
- Multi-hop `Cardinality` accumulation (`24 §5.4`'s `profile.compose(walked)`) is non-trivial — `25` / `17` collaborate on the exact composition matrix, and neither is ratified.
- Multi-hop implicit-path BFS needs a Steiner-tree-esque solver; `16 §11.4`'s BFS is an approximation that has never been exercised on >2 target kinds.
- Override positioning becomes subtle: do overrides key on hop position, on endpoint-pair, or on the underlying `RelationshipId`? Round-1 chose `HopPosition`, which is unambiguous for binary and order-brittle for N-ary.

**Current position in `24`.** Binary-only in v1 per `12 §5.2`. Struct is N-ary-ready; MINOR lift requires only arity-check relaxation plus path-algorithm extension.

**Next step.** Revisit at `25` ratification; `25` specifies cross-kind composition rules including Joinset × Grainset, which is the first place multi-hop Cardinality accumulation becomes concrete. If `25` ratifies the composition matrix, N-ary Joinset becomes a mechanical uptake.

---

## Q-24-02 — Hybrid path mode — CLOSED (2026-04-28)

**Status: CLOSED.** v1 ratifies "no hybrid path" — `24 §4.3` makes `path` strictly `None` (implicit) or fully declared (explicit). `TD-JOINSET-HYBRID-PATH` carries the post-v1 reactivation. Round-1 framing retained for historical reference.

**Question.** `24 §4.3` ratifies that `path` is either fully implicit (`None`) or fully explicit (`Some(ExplicitPath)`). Could there be a hybrid mode where the author declares some hops and lets the planner fill the remainder via BFS?

**Refs.**
- `24 §4.3` — mode-selection precedence.
- `24 §4.1` — implicit-path algorithm.
- `24 §4.2` — explicit-path algorithm.

**Proposed (Round 1):** Prohibited. `path` is `None` (implicit) or fully declared (explicit); there is no partial mode.

**Arguments against hybrid (for Round-1 prohibition).**
- Authoring semantics become ambiguous: "what anchor is the implicit-fill starting from — the declared anchor or the last declared explicit hop's endpoint?"
- Override positioning (`HopPosition`) depends on a known hop count; partial paths make positions ambiguous until planner fills.
- Implicit-path failures (`COMP_E_2401`, `COMP_E_2402`) become harder to diagnose when the author has pinned some hops.

**Arguments for hybrid.**
- Authoring ergonomics: pin only the ambiguous hops; let the planner resolve the unambiguous ones.
- Gracefully degrades: if an intermediate Relationship is added, the hybrid path still works without editing.

**Current position in `24`.** Prohibited. Authors who need partial control either declare the full explicit path or declare multiple binary Joinsets, chaining via composition kinds that `12` permits.

**Next step.** Revisit post-N-ary-lift (Q-24-01); hybrid mode is strictly more useful on N-ary paths.

---

## Q-24-03 — Override reach: `Cardinality` overrides? — CLOSED (2026-04-28)

**Status: CLOSED.** v1 ratifies "no `Cardinality` override". The declared `Relationship.cardinality` is authoritative for all consumers; sophisticated authors restructure the Model (junction-table DataKind per `16 §3.3.4`) rather than override. Round-1 framing retained for historical reference.

**Question.** `24 §5.3`'s `JoinsetStrategy` override mechanism currently covers `JoinType` only. Should `overrides` also permit pinning a hop's effective `Cardinality` — e.g. declaring "treat this `ManyToMany` hop as `OneToMany` for this Joinset because the author knows the Joinset's slice satisfies the tighter constraint"?

**Refs.**
- `24 §5.3` — `JoinType` override matrix.
- `24 §5.4` — `Cardinality` propagation and fanout profile.
- `16 §3.5` — `Cardinality × Additivity` matrix (drives fanout rewrites).

**Proposed (Round 1):** No `Cardinality` overrides. The declared `Relationship.cardinality` is authoritative for all consumers.

**Arguments against Cardinality overrides.**
- `Cardinality` has unverifiable structural implications: "this is `OneToMany` on this slice" is a data-content claim, not a schema claim. The canonical layer avoids data-content claims; that is the SQL engine's territory.
- Overrides open a new class of subtle bugs: a wrong `Cardinality` override produces incorrect aggregation results silently (the planner's fanout-rewrite assumes the override).

**Arguments for Cardinality overrides.**
- Pre-aggregated or pre-deduplicated member tables often present `ManyToMany` declared relationships as effectively `OneToOne` on the aggregated surface; an override would avoid spurious fanout rewrites.
- Sophisticated authors sometimes know more than the `Relationship` declaration can express.

**Current position in `24`.** No `Cardinality` override. Sophisticated authors restructure the Model (e.g. declare a junction-table DataKind; see `16 §3.3.4`) rather than override.

**Next step.** Revisit post-v1 if pre-aggregated-member patterns become common.

---

## Q-24-04 — `AsOf` activation matrix — CLOSED (2026-04-28)

**Status: CLOSED.** `17 §5.2` ratifies the per-shape-pair legality matrix; `24 §7.3` cross-references it. `TD-COMPOSITION-ASOF` continues to track the planner-side `AsOf` implementation deferral, but the activation-matrix question itself is settled. Round-1 framing retained for historical reference.

**Question.** `24 §7` fixes the integration points for `TemporalShape × JoinType::AsOf` but defers the exact activation matrix to `17 §5`. What are the precise `TemporalShape` pairs that mandate / permit / forbid `AsOf`?

**Refs.**
- `24 §7.1–§7.3` — Joinset's contract re `AsOf`; error codes `COMP_E_2412`–`COMP_E_2414`.
- `16 §4.4.2` — `AsOf` deferral; `TD-COMPOSITION-ASOF`.
- `17 §5` (pending) — activation matrix.
- `00 §4.1` — `TemporalShape` vocabulary row.

**Proposed (Round 1):** Deferred to `17 §5`. `24` reserves codes and integration points; `17 §5` fills in the matrix.

**Sketch of the likely matrix (pending `17 §5` ratification).**

| anchor-side `TemporalShape` | target-side `TemporalShape` | `AsOf` activation |
|---|---|---|
| `Events` | `Snapshot` | Mandated (canonical as-of case: events-as-of-snapshot). |
| `Events` | `Scd` | Mandated (canonical as-of case: events-as-of-SCD). |
| `Timeseries` | `Snapshot` | Permitted, not mandated (author may prefer point-in-time lookup but also may want the declared `JoinType`). |
| `Snapshot` | `Snapshot` | Forbidden (both already time-point-indexed; as-of is ill-defined). |
| `Timeseries` | `Timeseries` | Forbidden (same-grain timeseries joins on time are equality, not as-of). |
| `Events` | `Events` | Forbidden (no stable reference frame for "most-recent"). |

**Current position in `24`.** Integration points fixed; exact matrix parked for `17 §5`.

**Next step.** `17 §5` ratifies. `24 §7.3` updates to cite `17 §5.X` table numbers.

---

## Q-24-05 — Joinset reuse by implicit composition — CLOSED (2026-04-28)

**Status: CLOSED.** v1 ratifies "no reuse" per `16 §13.5`. Authors must use `from: Some("joinset_name")` to target a pre-built Joinset surface; otherwise an implicit composition synthesizes a distinct `ComposedSemanticInterface` with `CompositionKind::Relationship`. `TD-COMPOSITION-JOINSET-REUSE` continues to track the post-v1 reactivation. Round-1 framing retained for historical reference.

**Question.** `16 §13.5` records that an explicit `Joinset` does NOT shadow implicit composition: a `Request` with `from: None` over the same constituents produces a **distinct** `ComposedSemanticInterface` with `CompositionKind::Relationship`, not the Joinset's `CompositionKind::Joinset`. Should the planner learn to recognize the coincidence and substitute the pre-built Joinset surface?

**Refs.**
- `16 §13.5` — current ratification (no reuse).
- `16 §10.4` — plan-local implicit-composition cache.
- `16 §10.1` — materialization boundary (explicit materialized, implicit synthesized).
- `TD-COMPOSITION-JOINSET-REUSE` — tracking marker in `16 §13.5`.

**Proposed (Round 1):** No reuse. The two surfaces are semantically distinct objects; conflation risks behavior drift when the Joinset's overrides differ from the implicit composition's defaults.

**Arguments against reuse.**
- Joinset may carry overrides (different `JoinType` per hop) that change the composed surface's semantics; substituting would silently change Request results.
- Joinset's `ComposedSemanticInterface` equality per `24 §8.6` includes overrides; an implicit composition would never match.

**Arguments for reuse.**
- Performance: if the planner already materialized the Joinset's surface at compile, re-synthesizing an equivalent implicit composition at plan is wasted work.
- User mental model: "I declared this Joinset to avoid re-walking the graph; the planner should notice it."

**Current position in `24`.** No reuse (inherited from `16 §13.5`). Author explicitly uses `from: Some("joinset_name")` to target the pre-built surface.

**Next step.** Revisit post-v1 once real-world profiling shows the implicit-synthesis cost is material.

---

## Q-24-06 — Self-referential Joinsets — CLOSED (2026-04-28)

**Status: CLOSED.** Forbidden in v1, transitively from `16 §12.4` (Relationship self-references forbidden) and the validate-layer rejection `VALID_E_2406 JoinsetDuplicateMember` (`24 §9.1`). `TD-COMPOSITION-SELFJOIN` continues to track the post-v1 lift. Round-1 framing retained for historical reference.

**Question.** Can a Joinset's anchor and target be the same DataKind (e.g. `employees` joined to itself along a `manager_id → id` relationship)?

**Refs.**
- `16 §12.4` — Relationship self-references forbidden in v1; `TD-COMPOSITION-SELFJOIN`.
- `24 §4.2.3` — restatement: since no self-referential Relationships exist in v1, self-referential Joinsets are structurally unreachable.
- `24 §9.1 VALID_E_2406 JoinsetDuplicateMember` — rejects `members = [X, X]` at the validate layer.

**Proposed (Round 1):** Forbidden, transitively from `16 §12.4`.

**Arguments against (for Round-1 prohibition).**
- Relationships cannot self-reference; a Joinset over `{employees, employees}` has nowhere to walk.
- Author-level workarounds exist (alias the DataKind via a YAML-level rename) per `16 §12.4`.

**Arguments for.**
- Hierarchical / graph-like DataKinds (org charts, bill-of-materials) are a real modeling case.
- Self-referential Joinsets would be a natural surface for recursive-join support.

**Current position in `24`.** Forbidden. Tracked jointly with `TD-COMPOSITION-SELFJOIN`.

**Next step.** Revisit when `16 §12.4`'s self-join deferral lifts. Until then, authors use the alias workaround in `32`.

---

## Q-24-07 — Per-hop filter pushdown annotations — CLOSED (2026-04-28)

**Status: CLOSED.** v1 ratifies "no per-hop filters". `JoinHop` carries only `relationship`, `direction`, `to` (`24 §4.2`). Filters are declared at the Joinset level (`§2.6`) and the planner pushes them where safe. Authors needing per-hop scoping push the filter into the member's own interface or declare a narrower member DataKind. Round-1 framing retained for historical reference.

**Question.** Should `ExplicitPath.hops[i]` permit a per-hop filter expression — e.g. "only join with `addresses` where `country = 'US'`" — declared at the Joinset level?

**Refs.**
- `24 §4.2` — `JoinHop` struct (no filter field in Round 1).
- `24 §2.6` — Joinset-level Filter declarations (applied post-join, not per-hop).
- `14` — expression grammar.
- `34` — planner pushdown; selection-pushdown already an optimizer concern.

**Proposed (Round 1):** No per-hop filters. Filters are declared at the Joinset level (`2.6`) and the planner pushes them where safe.

**Arguments against per-hop filters.**
- Filter-pushdown is a classic planner optimization; encoding it in the SemanticManifest risks double-application (Joinset-level filter + planner pushdown of the Joinset-level filter).
- Per-hop filters blur the line between "declarative Joinset" and "imperative query plan" — Joinset is meant to be the former.

**Arguments for per-hop filters.**
- Some joins are semantically defined with a filter (the Cube.js `sql_on:` pattern): "join with this table, where `type = 'active'`" is part of the join's meaning, not a query-time refinement.
- Pushdown from a Joinset-level filter may fail when the filter references only one constituent; per-hop filters are unambiguously scoped.

**Current position in `24`.** Joinset-level filters only. Authors needing per-hop scoping can push the filter into the member's own interface or declare a narrower member DataKind.

**Next step.** Revisit when `25` examines the Cube-style pattern in detail.

---

## Q-24-08 — Structural `NullFill` for outer-join Joinsets — CLOSED (2026-04-28)

**Status: CLOSED.** v1 ratifies "no structural NullFill for Joinset". `FieldOwnership::NullFill` is Unionset-only per `16 §7.3.3`; outer-join NULL-fill is the `JoinType` at the plan tree's responsibility (`24 §5.5` step 3). Round-1 framing retained for historical reference.

**Question.** `16 §7.3.3` ratifies that `FieldOwnership::NullFill` is produced ONLY for `CompositionKind::Unionset`. For a `Joinset` with a `Left` / `Right` / `Full` outer join, missing-side columns are NULL-filled by the JoinType's semantics rather than recorded structurally on `FieldProvenance`. Should Joinset-side outer-join NULL-fill be recorded structurally?

**Refs.**
- `16 §7.3.3` — `NullFill` is Unionset-only in Round 1.
- `24 §8.3` — Joinset's `FieldProvenance` consequences; Joinset `FieldProvenance` has no `NullFill` entries.
- `24 §5.5` step 3 — SQL-side NULL-fill handled by JoinType at emission time.

**Proposed (Round 1):** No structural `NullFill` for Joinset. JoinType at the plan tree is the single source of NULL-fill truth.

**Arguments against structural NullFill.**
- Duplication: `JoinType` already tells the planner which side NULL-fills; a parallel `NullFill` record is redundant.
- `FieldOwnership` is meant to be a purely surface-shape record; runtime nullability belongs to the plan-emission layer.

**Arguments for structural NullFill.**
- Symmetry with Unionset simplifies consumer code that walks `FieldProvenance` uniformly.
- Explicit records make diagnostic output clearer ("this field is NULL-filled from member X" without needing to reason about join semantics).

**Current position in `24`.** Follow `16 §7.3.3`. Join-side NULL-fill is a JoinType concern, not a FieldProvenance concern.

**Next step.** Revisit post-v1 if Unionset / Joinset consumers end up with substantial separate-code-path logic. A MINOR could promote `NullFill` into a universal `FieldOwnership` variant.

---

## Post-v1 shape-hint clusters (folded in 2026-04-17)

> The following two clusters were previously parked in a standalone sidecar `joinset_shape_semantics.md`. That file was folded into this one on 2026-04-17 as part of the documentation-consolidation pass (H6). Neither cluster is blocking for v1; both describe *authoring-shape hints* the v1 Joinset body deliberately omits. Historical references to `questions/open/24_questions.md` resolve into this section.

### Q-24-09 — `JoinAssociativity` hint (deferred) — `TD-JOINSET-ASSOCIATIVITY-PARK`

**Background.** Earlier drafts of the Joinset shape carried an `associativity:` field on the body — values such as `left`, `right`, `star`, `snowflake` — as a hint to the planner about how to order joins when the declared `relationships:` graph had multiple valid topological walks.

**v1 decision (ratified 2026-04-17).** Dropped. `JoinAssociativity` does NOT exist in the v1 Joinset body. `32 §14` records this as `TD-JOINSET-ASSOCIATIVITY-PARK`.

**v1 behavior.** The Joinset walks its `relationships:` in YAML author order (`32 §7` / `18 §2`). When there is a choice, the planner picks the canonical left-deep traversal starting from the first-referenced member in the first `Relationship` entry. No cross-cutting hint shapes the walk. For the explicit-binary-join case that is v1 Joinset's only ratified arity, shape choices are degenerate anyway — there is exactly one join to emit.

**Deferred sub-questions.**

| Sub-ID | Question |
|---|---|
| Q-24-09.a | When v1's binary-only restriction lifts (`TD-NESTING-NARY-JOIN` / Q-24-01), does `associativity:` re-enter as a field, or does an ordering convention on `relationships:` suffice? |
| Q-24-09.b | If re-introduced, what values are legal? Candidates: `left` (left-deep), `right` (right-deep), `bushy` (planner chooses), `hinted` (planner uses author order). |
| Q-24-09.c | Is associativity advisory (planner may override for cost) or binding (planner must respect)? |
| Q-24-09.d | Interaction with `AsOf` joins (`17 §*`, Q-24-04 deferred): does AsOf carry its own shape-hint axis, or is it orthogonal? |

**Reference implementations.**

- **Cube.js** — no shape hint; join graph discovered at query time from declared relationships.
- **dbt_metricflow** — join-path resolution hints via `join_path:` on Metric definitions; implicit associativity.
- **LookML** — explicit `join` block per pair; implicit left-deep walk from the base `view:`.

v1 aligns with the Cube.js / LookML pattern (implicit from author-order). Re-introducing an explicit hint is a future decision when N-ary Joinsets (Q-24-01) land.

**Current position in `24`.** No `associativity:` field in the v1 Joinset body; `TD-JOINSET-ASSOCIATIVITY-PARK` carries the deferral. Revisit bundled with Q-24-01 (`TD-NESTING-NARY-JOIN`).

**Next step.** Bundle the decision with the N-ary lift: a shape-dedicated session, once N-ary support is on deck, re-opens both this cluster and Q-24-10 jointly.

---

### Q-24-10 — Star / Snowflake / 3NF shape-tag vocabulary (deferred) — `TD-JOINSET-SHAPE-VOCAB`

**Background.** During Q&A on the joinset YAML, a candidate field was considered that would let authors mark a Joinset as a specific canonical analytical shape:

```yaml
joinsets:
  - name: fact_sales_star
    shape: star              # or: snowflake, 3nf, data_vault, galaxy
    relationships: [ ... ]
```

The intent was to let the planner apply shape-specific optimizations (fact-table detection, conformed-dimension lookup, etc.) without deriving the shape from the `relationships:` graph.

**v1 decision.** Deferred. Not in the v1 body.

**v1 behavior.** Shape is derived at plan time from the `relationships:` graph + per-member `Cardinality`, when a shape-optimizing pass needs it. The planner does not require a declared shape and emits the same `PlanNode::Join` tree regardless.

**Deferred sub-questions.**

| Sub-ID | Question |
|---|---|
| Q-24-10.a | Is the `shape:` tag advisory (for diagnostics / tooling) or operationally meaningful (drives a planner pass)? |
| Q-24-10.b | What shape vocabulary is canonical? Candidates: `star`, `snowflake`, `galaxy`, `3nf`, `data_vault`, `one_big_table`, `junk_dimension_hub`. Open-ended strings vs a closed enum. |
| Q-24-10.c | If authored, is it validated against the declared relationships (e.g. `star` requires exactly one `many_to_one`-spine from one member to all others)? |
| Q-24-10.d | Does shape interact with `constraints:` at the joinset level (e.g. a star schema implicitly allows certain dimension rollups)? |
| Q-24-10.e | Does shape interact with the applicability matrix (`25`) — e.g. specific shapes enable specific aggregation patterns that flat graphs do not? |

**Shape vocabulary — reference implementations.**

- **Star schema** — one fact, many conforming dimensions, single-depth joins from fact.
- **Snowflake** — like star, with dimensions normalized (multi-level dimension hierarchies).
- **Galaxy / fact constellation** — multiple fact tables sharing conforming dimensions.
- **3NF / normalized** — arbitrary graph over normalized entities.
- **Data Vault** — hubs / satellites / links canonical shape.
- **One Big Table (OBT)** — denormalized, single wide member with no joins.

For v1, the Joinset represents explicit joins over ≥ 2 members with a declared graph. Shape classification can be layered on post-v1 without breaking the v1 body.

**Current position in `24`.** No `shape:` field; `TD-JOINSET-SHAPE-VOCAB` carries the deferral. Coordination with Q-24-09.

**Next step.** Re-open jointly with Q-24-09 when the N-ary Joinset arity lifts (Q-24-01).

---

## Summary

| ID | Title | Round-1 default | Tracking marker |
|---|---|---|---|
| Q-24-01 | N-ary Joinset lift | Binary-only (per `12 §5.2`) | `TD-NESTING-NARY-JOIN` / `TD-JOINSET-NARY` |
| Q-24-02 | Hybrid path mode | **CLOSED (2026-04-28)** — Prohibited | `TD-JOINSET-HYBRID-PATH` |
| Q-24-03 | Cardinality override | **CLOSED (2026-04-28)** — No | — |
| Q-24-04 | `AsOf` activation matrix | **CLOSED (2026-04-28)** — `17 §5.2` matrix | `TD-COMPOSITION-ASOF` |
| Q-24-05 | Joinset reuse by implicit composition | **CLOSED (2026-04-28)** — No reuse | `TD-COMPOSITION-JOINSET-REUSE` |
| Q-24-06 | Self-referential Joinsets | **CLOSED (2026-04-28)** — Forbidden | `TD-COMPOSITION-SELFJOIN` |
| Q-24-07 | Per-hop filter annotations | **CLOSED (2026-04-28)** — No | — |
| Q-24-08 | Structural `NullFill` for outer joins | **CLOSED (2026-04-28)** — JoinType-only | — |
| Q-24-09 | `JoinAssociativity` hint | Dropped (no field in v1 body) | `TD-JOINSET-ASSOCIATIVITY-PARK` |
| Q-24-10 | Star / snowflake / 3NF shape-tag vocabulary | Deferred (no field in v1 body) | `TD-JOINSET-SHAPE-VOCAB` |
