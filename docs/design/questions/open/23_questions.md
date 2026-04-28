---
doc: design/questions/open/23_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `data-kinds/23_unionset.md`
depends-on:
  - data-kinds/23_unionset.md
  - data-kinds/20_complex_datakinds.md
  - data-kinds/22_grainset.md
  - data-kinds/24_joinset.md
  - data-kinds/25_applicability_matrix.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/15_mapping_and_binding.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
  - apis/30_api_contracts.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
  - apis/35_semstrait_ir.md
---

# Open Questions — `data-kinds/23_unionset.md`

> Items surfaced during Round-1 drafting of the Unionset specification. Each entry restates the question, lists its ratified references, and records the Round-1 default `23` currently uses. Entries migrate out of this file as later docs (`17`, `20`, `24`, `25`, `30`, `33`–`35`) make decisions that confirm or amend `23`'s defaults.

> **Status summary (2026-04-17).** Q-UNI-002 (`UnionMode::Distinct` v1 or deferred) and Q-UNI-009 (single-child Unionset acceptance) are **CLOSED**. Q-UNI-002 is ratified by `foundations/18_entities.md §2` with v1 roster `{All, Unique}` (the former `Distinct` renamed to `Unique`); Q-UNI-009 is ratified by `data-kinds/26_nesting_matrix.md §R3` (every `ComplexDataKind` requires ≥ 2 children). Both retain their original bodies for resolution context.

---

## Q-UNI-001 — Error-code allocation: `*_E_23NN` per doc vs `30 §6.2` cross-subsystem ranges

**Question.** `23` allocates its error codes in the per-doc sub-range (`VALID_E_2300`–`2399`, `COMP_E_2300`–`2399`, `PLAN_E_2300`–`2399`), matching the pattern established by `21` and `22`. `30 §6.2` ratifies a DIFFERENT cross-subsystem allocation scheme (`COMP_E_0300-0399` for schema/binding, `COMP_E_0400-0499` for composition/relationship, `PLAN_E_0500-0599` for general planner errors). Which scheme is authoritative?

**Refs.**
- `23 §§8, 9, 10` — Round-1 default: `*_E_23NN` per-doc range.
- `21 §§8, 9, 10` — same per-doc convention (`*_E_21NN`).
- `22 §§8, 9` — same per-doc convention (`*_E_22NN`).
- `30 §6.2` — cross-subsystem allocation scheme (currently listing `COMP_E_03xx` / `04xx`; `PLAN_E_05xx`).
- `16 §14` — composition errors assigned to `04xx` under `30`'s scheme.
- `15 §11` — binding errors assigned to `03xx`.

**Arguments for per-doc ranges (Round-1 default).**
- The per-doc convention (`21xx`, `22xx`, `23xx`, ...) is crisp: a reader sees a code `COMP_E_2304` and knows immediately "this is Unionset territory, doc `23`." Cross-subsystem ranges require a lookup table.
- Matches the pattern the other data-kind docs in this wave (`21`, `22`, `24` pending) are using.
- Scales naturally: each future DataKind gets its own 100-entry range without re-negotiating with the cross-subsystem table.

**Arguments for `30 §6.2`'s cross-subsystem scheme.**
- `30` is authoritative for API contracts, including the error-code format. A per-doc convention that diverges from `30 §6.2` creates an inconsistent code-space.
- Grouping codes by SUBSYSTEM (binding vs. composition vs. planner) rather than by DataKind makes tooling simpler — a diagnostic filter "show me every composition-layer error" is a range-check in `30`'s scheme but not in the per-doc scheme.

**Current position in `23`.** Per-doc `*_E_23NN`, with `[TD-UNIONSET-CODERANGE]` explicitly tagged in §8's header.

**Next step.** Resolve at `30` ratification / revision. If `30` adopts the per-doc scheme, `23` is already aligned. If `30` retains the cross-subsystem scheme, `23` / `21` / `22` each migrate their codes; the migration is a MAJOR because error-code numbers appear in downstream tooling. Decide BEFORE Round-2.

---

## Q-UNI-002 — `UnionMode::Distinct` in v1 or deferred?

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `foundations/18_entities.md §2` (adjacency reference): v1 `UnionMode` roster is **`{All, Unique}`**, `#[non_exhaustive]`, default `All`. The previous spelling `Distinct` is renamed to `Unique` (authors who need SQL-`DISTINCT` semantics at Union-level select `Unique`; the three-valued-logic NULL-collision caveat documented in `23 §4.3` still applies as `PLAN_W_2303`). The "defer Distinct to post-v1" alternative is rejected; `Unique` is supported in v1.

**Question.** `23 §2.1` ratifies a `UnionMode` enum with two variants: `All` (default) and `Distinct`. Distinct semantics in the presence of NULL-fill are subtle (per `23 §4.3`'s three-valued-logic note): rows from different children differing only in NULL-fill positions do NOT dedupe under `DISTINCT`. Should Round 1 ship `Distinct` at all, or defer it to a later milestone?

**Refs.**
- `23 §2.1, §4.1, §4.3` — Round-1: `UnionMode::Distinct` supported, with advisory `PLAN_W_2303` for the NULL × NULL non-collision case.
- `UNIONSET.md` (legacy) — distinguishes `UNION ALL` vs. `UNION DISTINCT`.
- Cube.js `unionAll` — does not expose a DISTINCT mode at the cube level.
- dbt MetricFlow — does not have a direct Unionset analog.
- `[TD-UNIONSET-DISTINCT-SEMANTICS]` — deferred subsection under §4.3.

**Arguments for including `Distinct` in v1 (Round-1 default).**
- Shipping both modes keeps `UnionMode` extensible from day one; adding `Distinct` later is MINOR under I10 but costs a version bump.
- Authors coming from SQL expect `UNION` vs. `UNION ALL` as a primary choice.
- The NULL-collision advisory `PLAN_W_2303` surfaces the subtlety clearly; authors who don't need it can use `UnionMode::All`.

**Arguments for deferring `Distinct`.**
- The three-valued-logic interaction is confusing; shipping `Distinct` without a crisp story risks authoring errors.
- Cube.js's decision to not expose DISTINCT at the composition level is evidence that the feature is rarely needed; UNION ALL + explicit deduplication at the Request level (a DISTINCT Aggregate) is the more common pattern.
- Deferring frees up `UnionMode::Distinct` to carry refined semantics (e.g. "distinct treating NULL as equal") when it lands.

**Current position in `23`.** Both modes supported in v1; `UnionMode::All` is the default.

**Next step.** If Round-2 adoption feedback shows `Distinct` is rarely used OR frequently mis-used, consider demoting it to a post-v1 feature. A demotion is a feature-removal (MAJOR); keep in v1 unless strong negative signal.

---

## Q-UNI-003 — Per-child Coverage override shape: whitelist `provides` vs per-Semantics tri-variant

**Question.** `23 §3.2` ratifies `ChildCoverageOverride { provides: BTreeSet<SemanticsName> }` — an opt-in whitelist. Any name NOT in `provides` that the Binding-level fold would cover is forced to `FieldOwnership::NullFill` at the composition level. Is this the right shape, or should authors be able to distinguish per-Semantics between `Native` / `Derived` / `NullFill` overrides?

**Refs.**
- `23 §3.2` — Round-1: whitelist-only.
- `23 §5.4` — override-compose-with-fold rule.
- `15 §6.1` — `CoverageVariant { Native, NullFill, Derived, Metadata }`; the richer four-way distinction at Binding level (`Metadata` ratified 2026-04-27 per `15 §13 R22 / R44–R47`).
- `16 §8` — `CompositionCoverage`; the same four variants at composition level (per `16 §8.2 / §8.3`).

**Arguments for whitelist-only (Round-1 default).**
- Simplest mental model: "these fields, the child provides; everything else is NULL-filled."
- Most common override use case is suppressing a field the child happens to have but shouldn't contribute (e.g. a legacy column the author doesn't want surfaced); the whitelist handles this directly.
- Authors who want `Derived` at the composition level can express it on the Unionset's own `SemanticInterface` via a composition-level computed expression (per `16 §7.3.4`); the override doesn't need to carry that axis.

**Arguments for tri-variant per-Semantics.**
- Full expressive power: an author could declare, per-(child, name), the exact variant the composition-level Coverage should carry — e.g. "child A provides `revenue` Natively; child B Derives `revenue` from `subtotal + tax`." Currently, child B would have to declare the Derivation on its own interface, which is more distant from the Unionset context.
- Allows override-specific CAST declarations (per `13 §7`) without leaking into the child's own interface.

**Current position in `23`.** Whitelist-only.

**Next step.** Revisit if authors need per-Semantics Derivation at the composition level. The whitelist is forward-compatible: adding a `derives: BTreeMap<SemanticsName, Expr>` field to `ChildCoverageOverride` is MINOR under I10. Do not add it prematurely.

---

## Q-UNI-004 — `Avg` re-aggregation: default `Error` (`PLAN_E_2304`) or `Lossy` warning (`PLAN_W_2302`)?

**Question.** `23 §4.5`'s re-aggregation inference table marks `Avg` as "not-decomposable-directly," suggesting authors restructure as an explicit Metric with `Sum(num) / Sum(den)`. Round-1 emits `PLAN_E_2304 UnionsetReAggregationInfeasible` for an `Avg` Measure across a Unionset. Should the default be an error, or a `Lossy`-warning with a fallback to "sum the averages and re-average" (which is mathematically wrong on heterogeneous row counts, but provides a result)?

**Refs.**
- `23 §4.5` — Round-1 default: `PLAN_E_2304` for `Avg`.
- `23 §10.1` (`PLAN_E_2304`) — error roster.
- `22 §?` — parallel Grainset `Avg` decision; if the two docs diverge, cross-source consistency is undermined.
- `14a` — `FunctionRegistry`; `Avg` signature.
- `11 §6` — Measure's `Aggregation` variant roster.

**Arguments for error (Round-1 default).**
- `Avg` across Unioned children is almost always mathematically wrong (the average of averages ≠ the true average unless row counts are equal). Failing loudly is safer than silent wrong answers.
- Authors can opt into a `Sum(num) / Sum(den)` Metric declaration if they want correct semantics; the error message points at this remediation.

**Arguments for warning-with-fallback.**
- Some adapters (e.g. OLAP engines) compute `AVG` in a way that's meaningful over pre-aggregated rows (carrying a count-weight); the engine-side fallback may be correct in those specific cases.
- Matches SQL's permissive behavior: a `SELECT AVG(x) FROM (unioned)` in SQL doesn't fail; authors may expect the same.

**Current position in `23`.** Error (`PLAN_E_2304`).

**Next step.** Keep as error; promote `Avg` to `Metric` pattern in example documentation. If adapter-side weighted-average becomes a thing, revisit as `[TD-UNIONSET-AVG-WEIGHTED]` and introduce a `PLAN_W_23xx` variant.

---

## Q-UNI-005 — Strict-mode posture for `TemporalShape`-mismatch advisories

**Question.** `23 §6.1` lists a matrix of cross-child `TemporalShape` combinations and emits warnings (`COMP_W_2302`–`2305`) for every mismatch. Should there be a strict-mode flag (e.g. `--strict-unionset-shapes`) that promotes these warnings to errors? Should the decision live on the Unionset, on the Model root, or on the compile invocation?

**Refs.**
- `23 §6.1` — Round-1: warnings only.
- `17` (parallel) — `TemporalShape` vocabulary owner; may ratify per-pair compatibility.
- `10` — stage contracts; doesn't currently expose a strictness flag.
- `30 §6.5` — `Severity` enum; adjustable at the Diagnostic site but not currently globally promotable.

**Arguments for warnings-only (Round-1 default).**
- Shape mismatches are almost always intentional when they occur (e.g. unioning a pre-migration `Snapshot` source with a post-migration `Events` source); erroring would break legitimate use cases.
- `17` is parallel-drafted; its own authority on shape-pair legality may provide richer per-pair error / warning rules than `23` can pre-commit to.

**Arguments for a strict-mode flag.**
- CI / production builds may want to escalate warnings to errors, to force authors to review mismatches explicitly.
- Per-Unionset `strict_shapes: true` declarations would allow per-model opt-in.

**Current position in `23`.** Warnings only. Promotion mechanism (global flag? per-Unionset? per-shape-pair?) is deferred.

**Next step.** Defer to `17`'s Round-2 ratification. If `17` ratifies a per-pair legality matrix with its own error/warning allocation, `23 §6.1`'s warnings become a consumption of `17`'s rules rather than a `23`-owned set. `[TD-UNIONSET-STRICT-SHAPES]`.

---

## Q-UNI-006 — Post-prune single-child collapse: emit `PlanNode::Union` or skip?

**Question.** `23 §4.1` notes that Coverage-driven pruning (§4.6) may reduce the surviving child set to 1. In that case, should the emitted plan (a) retain a single-input `PlanNode::Union` for structural regularity, or (b) skip the `Union` node entirely and flow the surviving child's subplan directly into the terminal Aggregate (if any)?

**Refs.**
- `23 §4.1` — Round-1: skip the Union (defensive branch in the implementation).
- `23 §4.5` — terminal Aggregate skip-rule `§4.5 (3)` — "Single-child post-prune" skips both the Union and the Aggregate.
- `35` (pending) — `PlanNode::Union` field roster; may allow `inputs: Vec<PlanNode>` with `len == 1` or may require `len >= 2`.

**Arguments for skip (Round-1 default).**
- A single-input UNION is a no-op at the IR level and produces unnecessary plan tree depth.
- Matches the `§4.5 (3)` terminal-Aggregate skip rule: the two optimizations compose.
- Downstream adapters may not generate SQL like `SELECT ... UNION ALL ... (empty second branch)`; skipping avoids adapter complexity.

**Arguments for retain.**
- Structural regularity: every Unionset emits `PlanNode::Union` unconditionally; downstream plan-introspection tools don't need a special case.
- Makes the re-expansion of pruned branches (if a future optimizer re-admits them) more structurally uniform.

**Current position in `23`.** Skip the Union for single-child-post-prune cases.

**Next step.** Confirm at `35` ratification. If `35`'s `PlanNode::Union` requires `|inputs| >= 2`, the skip is mandated. If `35` allows `len == 1`, keep the Round-1 skip as an optimization and document the alternative retain path in `34`'s planner trait surface.

---

## Q-UNI-007 — Interaction with `17`'s as-of / snapshot-selection when children have heterogeneous shapes

**Question.** `23 §6.4` defers "shape-gated planner rewrites" to when `17`'s planner support is ratified. Specifically: when a Unionset has an `SCD(Type2)` child and a `Timeseries` child, and a Request carries an as-of timestamp (`valid_at: 2024-06-01`), how should the planner route the as-of filter to each child? Per-child, based on the child's shape? Globally, pushing the filter into every branch? Not at all (Round-1 default)?

**Refs.**
- `23 §6.4` — Round-1: no shape-gated rewrites; advisories only.
- `17` (parallel) — as-of filter semantics; parallel-drafted.
- `[TD-UNIONSET-SHAPE-PLANNING]` — deferred subsection tag.
- `23 §11.5` — worked example does not exercise mixed-shape Unionsets.

**Arguments for Round-1 default (no rewrites).**
- `17` is parallel-drafted; committing `23` to a specific as-of-routing strategy before `17` is ratified risks misalignment.
- The conservative default (emit advisories; let authors either (a) unify shapes at authoring time, or (b) manually filter per-child via pre-Union Requests) is safe.

**Arguments for aggressive shape-aware routing now.**
- Authors mixing shapes expect the planner to do the right thing automatically. Deferring to `17` leaves a gap where correct-but-complex mixed-shape Unionsets are not plannable.

**Current position in `23`.** Defer; advisories only; `[TD-UNIONSET-SHAPE-PLANNING]`.

**Next step.** Resolve at `17` ratification. Drafting of `23 §6.4`'s "shape-aware rewrite" subsection is tracked as `[TD-UNIONSET-SHAPE-PLANNING]`.

---

## Q-UNI-008 — Non-exhaustive `UnionMode`: future variants

**Question.** `UnionMode` is `#[non_exhaustive]` per I10. Which additional variants are plausibly in the MINOR-addition space?

**Refs.**
- `23 §2.1` — `UnionMode::{All, Distinct}`.
- I10 — `00 §9`.
- Similar patterns in query-engine IRs: DataFusion's `UnionType`, Substrait's `SetRelType`.

**Candidate future variants.**
- `UnionMode::ByName` — name-keyed alignment rather than positional. Would loosen the "deterministic column-order walk" requirement of `23 §4.3`; useful when children expose Semantics with subtly different authored names. Probably low priority (authors can rename in the Unionset's top-level `SemanticInterface`).
- `UnionMode::Intersect` — `INTERSECT` semantics. Substantial: changes the planner's NULL-fill story (every row must exist in every child). Probably its own DataKind variant rather than a UnionMode.
- `UnionMode::Except` — `EXCEPT` / `MINUS`. Same caveat as Intersect.
- `UnionMode::DistinctOnKeys(Vec<SemanticsName>)` — dedupe on a specific key subset rather than the full row. Useful for Unionsets with partial overlap where authors want to pick one-child-per-key.

**Current position in `23`.** `UnionMode::{All, Distinct}` only. All others are `[TD-UNIONSET-FUTURE-MODES]`.

**Next step.** Track usage signals. If `DistinctOnKeys` emerges as a repeated pattern, promote to MINOR. If `Intersect` / `Except` come up, create separate `ComplexDataKind` variants (`Intersectset` / `Exceptset` or similar) rather than overloading `UnionMode`.

---

## Q-UNI-009 — Single-child Unionset acceptance: reject (Round-1) vs silent accept vs warning

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `data-kinds/26_nesting_matrix.md §R3`: every `ComplexDataKind` (including `Unionset`) REQUIRES ≥ 2 children. Rejection stands (via R3 + `VALID_E_2302`); the "silent accept" alternative from `22 Q-GRN-006` (now also closed by R3) no longer creates asymmetry. All three Complex variants (`Unionset`, `Grainset`, `Joinset`) share the ≥ 2 children rule. `[TD-UNIONSET-SINGLE-CHILD]` is retired.

**Question.** `23 §8.1` fires `VALID_E_2302 UnionsetSingleChild` on a Unionset with exactly one child. The rationale: "semantically the child itself; authors should replace." Is rejection right, or should Round 1 be more permissive (silent accept or warning)?

**Refs.**
- `23 §8.1` — Round-1: rejection via `VALID_E_2302`.
- `12 §3.2` — `UnionsetMustHaveMultipleChildren`; structural minimum.
- `22 Q-GRN-006` (parallel) — Grainset ratifies silent accept for single-child. Asymmetric decisions across sibling docs.
- `24` (pending) — Joinset; what does a one-member Joinset do?

**Arguments for rejection (Round-1 default).**
- A one-child Unionset is semantically a no-op; it is safe to reject at validation.
- Matches `12 §3.2`'s structural check.

**Arguments for silent accept (parallel with `22`).**
- During Model evolution, a Unionset may temporarily shrink to one child while an author refactors; rejection forces a scaffold.
- Symmetric decision across sibling docs (Grainset and Unionset both accept single-child) simplifies the mental model.

**Arguments for warning.**
- "You probably meant to either add more children or replace with the underlying DataKind." Low-cost nudge without blocking.

**Current position in `23`.** Rejection.

**Next step.** Reconcile with `22`'s Round-1 decision at `25`-drafting. Whichever sibling doc decides first (likely `22` via its Q-GRN-006 resolution) sets the pattern; `23` / `24` align. Current asymmetry (`22` accept vs. `23` reject) is noted as `[TD-UNIONSET-SINGLE-CHILD]`.

---

## Q-UNI-010 — Composition-level `Derived` expressions: who declares them?

**Question.** `23 §4.3`'s NULL-fill-projection table mentions `FieldOwnership::Derived(expr)` as a valid variant (per `16 §7.3.3`). When a composed-surface Semantics is `Derived` at the Unionset level (i.e. computed from other composed-surface fields, not from any child), where does the author declare the derivation?

**Refs.**
- `23 §4.3` — treats `Derived` as "compute at terminal-wrapper; branches emit NULL at the join position."
- `16 §7.3.4` — composition-level `Derived` expressions declared via a dedicated field on the `ComposedSemanticInterface`.
- `14b` — expression resolution.
- `32` (pending) — YAML surface; does the Unionset's `measures:` / `dimensions:` block support an `expr:` clause for composition-level derivations?

**Arguments for `16 §7.3.4`'s path (declare on the composed interface).**
- Composition-level derivations are a `16` concern; `23` consumes, not re-ratifies.
- Clear authoring surface: the Unionset's own `SemanticInterface` carries an `expr:` for each `Derived` field.

**Arguments for a `23`-level declaration.**
- Some derivations are Unionset-specific (e.g. "CASE WHEN source_platform = 'adwords' THEN ...") and don't fit cleanly on the `ComposedSemanticInterface`'s uniform schema.

**Current position in `23`.** Defer to `16 §7.3.4`; `23 §4.3` only consumes.

**Next step.** Confirm at `16`'s and `32`'s ratification. If `16 §7.3.4` does not cover Unionset-specific derivations, open a `[TD-UNIONSET-DERIVED]` carve-out for `23`-level expression declaration.

---

## Q-UNI-011 — `CompositionCoverage` override: override-before-fold or override-after-fold?

**Question.** `23 §5.4` specifies the override composes with the fold: `override` acts as a post-fold mask. An alternative is that `override.provides` REPLACES the fold (the child's Binding-level `Coverage` is ignored; only `provides` matters). Which semantic is right?

**Refs.**
- `23 §3.2, §5.4` — Round-1 default: override-after-fold (override is a post-fold mask, with subsumption check).
- `15 §6` — Binding-level Coverage.
- `16 §8.4` — composition-level fold.

**Arguments for override-after-fold (Round-1 default).**
- Preserves Binding-level truth: an override cannot CLAIM a child provides what the Binding doesn't contain. The subsumption check (§9.3's `COMP_E_2305`) enforces this.
- Composable: overrides act as restrictions, not inventions.

**Arguments for override-replaces-fold.**
- Simpler semantic: "what I declare is what the composition uses; Binding-level is no longer relevant." Useful when authors know better than the default fold (e.g. Binding has `NullFill` but the author knows the physical data does have the column).
- More powerful; fewer compile errors.

**Current position in `23`.** Override-after-fold with subsumption check.

**Next step.** Keep Round-1. If authors report confusion around `COMP_E_2305` (claiming Binding-level doesn't have the column when they KNOW it does), investigate whether the Binding-level detection is too strict; don't loosen the override semantic.

---

## Q-UNI-012 — `PlanNode::Union` `distinct` flag: how do adapters map it?

**Question.** `23 §4.1` emits `PlanNode::Union { distinct: bool }`. `35` ratifies the node's exact roster. Adapter (`36`) responsibility: ANSI SQL `UNION ALL` vs `UNION`; but what about engine dialects without `UNION`?

**Refs.**
- `23 §4.1` — Round-1: emit `distinct: bool` on the `PlanNode::Union`.
- `35` (pending) — `PlanNode::Union` field roster.
- `36` — adapter trait; SQL generation.
- `37` — catalog-adapter; not involved here.
- Engine compatibility: DataFusion has `LogicalPlan::Union`; Substrait has `SetRel`; DuckDB / Postgres have ANSI `UNION`/`UNION ALL`; some older warehouses (Redshift, BigQuery Legacy) have dialectal differences.

**Current position in `23`.** Emit `distinct: bool`; adapter responsibility to translate.

**Next step.** Confirm at `35`-drafting. If `35` prefers a richer discriminator (`UnionKind` enum with more variants) rather than a plain `bool`, `23 §4.1` aligns.

---

## Q-UNI-013 — Re-aggregation wrapper: single-level or nested?

**Question.** `23 §4.5` emits a single terminal `PlanNode::Aggregate` wrapping the `PlanNode::Union`. For deep nesting (Unionset → Grainset → SimpleStrategy), each level may emit its own Aggregate, producing nested `Aggregate(Union(Aggregate(Scan), Aggregate(Aggregate(Scan))))`-style plans. Is nested aggregation acceptable, or should the planner collapse redundant levels?

**Refs.**
- `23 §4.5` — Round-1: single terminal Aggregate; each child's subplan may have its own internal Aggregates (per-`SimpleStrategy` / `GrainsetStrategy` choices).
- `35` (pending) — whether `PlanNode::Aggregate(Aggregate(...))` nesting is admitted.
- `34` (pending) — whether the planner has an optimizer pass for aggregate collapsing.

**Current position in `23`.** Single terminal Aggregate at the Union seam; per-child aggregates are the child-Strategy's concern. No collapsing pass at the `23` level.

**Next step.** If `34` introduces an optimizer pass for aggregate collapsing, `23` may benefit (reducing redundant work). Track as `[TD-UNIONSET-AGG-COLLAPSE]`.

---

## Q-UNI-014 — Adapter-level engine-specific UNION semantics

**Question.** Some engines treat `UNION ALL` with mismatched nullabilities differently (e.g. Spark ANSI mode rejects; Spark non-ANSI widens; DuckDB always widens; Postgres depends on column types). Should `23` ratify a normalized nullability-widening behavior that all adapters must match, or leave it adapter-dependent?

**Refs.**
- `23 §4.4` — Round-1: nullability widens if any contributor is nullable; advisory `COMP_W_2301`.
- `36` — adapter trait.
- I2 — logical types only in the canonical layer; `13`'s `DataType` does not carry nullability as a top-level axis (nullability is per-field on the SemanticInterface, per `13 §3.5`).

**Current position in `23`.** Canonical-layer rule: unified column nullability widens if any contributor's column is nullable or NULL-filled. Adapters translate to engine-specific nullability.

**Next step.** Confirm at `36`-drafting that the canonical-layer widening rule is adapter-emulable on every target engine. If an engine cannot express the widened type, adapters must add explicit `CAST` or issue a runtime diagnostic.

---
