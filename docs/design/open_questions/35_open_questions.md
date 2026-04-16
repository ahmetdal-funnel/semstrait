---
doc: design/open_questions/35_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/35_semstrait_ir.md`
depends-on:
  - apis/35_semstrait_ir.md
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/34_semstrait_planner.md
  - apis/36_semstrait_adapter.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/14a_function_catalog.md
  - foundations/14b_expression_resolution.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
---

# Open Questions — `apis/35_semstrait_ir.md`

> Items surfaced during Round-1 drafting of the `semstrait-ir` public API contract. Each entry restates the question, lists its ratified references, and records the Round-1 default `35` currently uses. Entries migrate out of this file as later docs (`34`, `36`, and downstream amendments to `30`) make decisions that either confirm or amend `35`'s defaults. None of these open items block the headline ratifications in `35 §15`.

---

## Q-IR-001 — `IR_E_35xx` subsystem-prefix registration in `30 §6.2`

**Question.** `35 §10.2` reserves the `IR_E_3500`–`IR_E_3599` range for `semstrait-ir` errors. `30 §6.2`'s ratified code-range table does NOT currently list an `IR` subsystem prefix. Two parallel questions:
- (a) Should `IR` be added to `30 §6.2`'s table as a new subsystem?
- (b) Should the numeric offset (`3500` rather than `0001` within the IR subsystem) be preserved, despite `30 §6.2`'s convention of 4-digit codes starting low within each subsystem?

**Refs.**
- `30 §6.1` — format: `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` with 4-digit zero-padded `NUMBER`.
- `30 §6.2` — reserved ranges table; no `IR` row today.
- `30 §6.6` — reserved future prefixes (`REG`, `IO`, `ENG`); `IR` is not mentioned.
- `35 §10.1`–`§10.2` — 14 variants defined under `IR_E_3500`..`IR_E_3513`.

**Arguments for `IR_E_35xx` (current Round-1 default).**
- The `35xx` offset matches the doc number (`35`), giving consumers a strong visual association between code and doc.
- Lexically distinct from `PLAN_E_05xx`, `ADAPT_E_03xx`, `COMP_E_01xx` at a glance.
- The numeric space is 4-digit per `30 §6.1`; `3500`–`3599` satisfies that format.

**Arguments for amending to `IR_E_0001`–`0099` (would align with the sub-range convention of `30 §6.2`).**
- `30 §6.2`'s existing subsystems all start low (`PARSE_E_0001`, `VALID_E_0100`, `COMP_E_0100`, `EXPR_E_0001`, `PLAN_E_0500`, `OPT_E_0100`, `ADAPT_E_0300`). The convention is "low numbers, sub-ranged by category"; `3500`–`3599` breaks the pattern.
- Future sub-range explosion (e.g. if IR grows tree-shape errors, wire-format errors, validator errors separately) is harder inside a 100-wide band than inside a 1000-wide one.

**Current position in `35`.** `IR_E_3500`–`3599` per the prompt's explicit specification. An amendment item `[TD-IR-CODE-TABLE-AMEND]` is recorded against `30 §6.2` to add the row.

**Next step.** Decide during `30`'s next amendment pass. If the amendment reshapes all subsystem ranges to a uniform convention (e.g. every subsystem gets a 1000-wide band starting at `doc_number * 100`), `35` updates its codes mechanically. Until then, `35 §10`'s values are placeholders in the ratified structural sense (shape / severity / name frozen), with the numeric literal tracked as a migration item.

---

## Q-IR-002 — `NodeId` stability across planner invocations

**Question.** `35 §5.1` / `§11.2` states `NodeId` is NOT stable across planner invocations — two runs of the planner over the same Manifest + Request MAY produce different UUIDs. Is this the right design, or should `NodeId` be derived deterministically (e.g. from a tree-shape hash of the subtree rooted at the node)?

**Refs.**
- `35 §5.1` — `NodeId` is a newtype over `Uuid::new_v4()`; opaque to external consumers.
- `35 §11.2` — declared internal / not stable.
- `00 §9` I4 — Manifest determinism; does not directly require plan-tree determinism but supports it as a design goal.

**Arguments for random `NodeId` (current Round-1 default).**
- Simple, fast, never collides within a run.
- Keeps `semstrait-ir` free of the tree-hash machinery a deterministic derivation would require.
- Consumer use cases so far (optimizer rule-trace, adapter diagnostic correlation) only need per-run uniqueness, not cross-run stability.

**Arguments for deterministic `NodeId` (content-hash derived).**
- Two identical plans produce identical `NodeId`s, enabling plan-diff tooling ("what changed between yesterday's plan and today's?") without a custom tree-walker.
- Matches `00 §9` I4's determinism spirit at the plan layer.
- Test harnesses become more deterministic (snapshot tests no longer need to redact UUIDs).

**Current position in `35`.** Random UUID per `Uuid::new_v4()`. Deterministic derivation is a MINOR upgrade — consumers who treat `NodeId` as opaque (per `§5.1`) are unaffected.

**Next step.** Revisit with `34`'s final draft. If any optimizer pass in `34` needs cross-run stable node identity (e.g. for plan-cache lookup by plan-shape fingerprint), switch to a hash-derived form; otherwise defer.

---

## Q-IR-003 — `JoinNode.residual` for non-equi-join predicates

**Question.** `35 §4.6` ratifies `JoinNode.on: Vec<KeyPair>` for equijoin only. Non-equi-join predicates (range joins, inequality joins, theta joins in the general sense) are deferred as `[TD-IR-NON-EQUI-JOIN]`. Should the v1 shape already carry a reserved `residual: Option<PhysicalExpr>` field (always `None` in v1), or should the field be added only when a concrete use case arrives?

**Refs.**
- `35 §4.6` — current shape without `residual`.
- `16 §5` — JoinType vocabulary; no non-equi constraint stated.
- `30 §4` — field additions inside a `#[non_exhaustive]` struct are MINOR.

**Arguments for reserving `residual: Option<PhysicalExpr>` now.**
- Adds the field at v1 shape stabilization without breaking serde on a future addition.
- Keeps the `JoinNode` shape aligned with Substrait's `JoinRel.expression` (which carries an optional general-purpose condition beyond the equijoin keys).
- Planners needing non-equi joins (e.g. temporal `AsOf` once `17` ratifies, range-partition joins in analytical workloads) have a canonical slot.

**Arguments against (current Round-1 default).**
- YAGNI — no v1 use case consumes a non-equi-join today.
- `30 §4` makes adding the field MINOR; the shape can grow when needed.
- Reserving the field now invites speculative usage ("put it in `residual` when not sure if it's an equijoin key").

**Current position in `35`.** Not reserved. Adding `residual` is a documented MINOR upgrade via `[TD-IR-NON-EQUI-JOIN]`.

**Next step.** Revisit at `17 TemporalShape` ratification. If `17`'s `AsOf` join lands, the shape grows the residual field; otherwise defer.

---

## Q-IR-004 — `AggregateExpr.filter` field reservation

**Question.** `35 §5.7` reserves `AggregateExpr.filter: Option<PhysicalExpr>` for aggregate-filter clauses (Substrait `AggregateFunction.filter`, SQL `FILTER (WHERE ...)`). `14 §3.2` does not ratify an `Expr::Aggregate.filter` field; the semstrait-core-side expression does not carry one. Is the field reservation on the plan-layer `AggregateExpr` appropriate, or should the filter be lowered into the `AggregateExpr.input_expr` as a `Case { condition, result } -> Null` rewrite?

**Refs.**
- `14 §3.2` — `Expr::Aggregate { aggregation, expr, distinct }`; no filter field.
- `14 §3.3` — design notes on closed Aggregation enum.
- Substrait — `AggregateFunction.filter` is optional.
- SQL — `COUNT(*) FILTER (WHERE cond)` is supported by most engines.

**Arguments for reserving the field at plan layer.**
- Substrait round-trip fidelity (the filter survives a `SemanticPlan ↔ substrait::proto::Plan` cycle).
- Avoids lowering that inflates the inner expression (wrapping every `input_expr` in a `Case` when only some aggregates need a filter makes plan trees noisier than necessary).

**Arguments for Case-rewriting.**
- Keeps the plan-layer shape aligned with the core-layer `Expr::Aggregate` shape (no extra field).
- The lowered form works on every engine (`CASE WHEN cond THEN x END` with a final aggregate is universally supported); the filter form is Substrait- and some-SQL-specific.

**Current position in `35`.** Reserved as `Option<PhysicalExpr>`, always `None` in v1 (no `34` pass populates it). The field is ratified as structurally present; whether a planner uses it vs. lowers to `Case` is `34`'s choice per `[TD-IR-AGG-FILTER]`.

**Next step.** Decide at `34` drafting. If `34` consumes aggregate-filter as a plan-node-level concept, the field stays. If `34` always lowers, the field is dropped (MINOR — absent field reintroduction is MINOR, but removing a reserved field is MAJOR per `30 §4`; defer the add until `34` decides).

---

## Q-IR-005 — `Dialect` trait sealed vs. non-sealed

**Question.** `35 §6.5` declares `Dialect` a non-sealed trait so third-party adapter crates can implement it. Should it instead be sealed (with a semstrait-workspace-private witness) to preserve in-workspace control over dialect identity?

**Refs.**
- `35 §6.5` — current non-sealed declaration.
- `30 §8` — sealed-trait pattern policy.
- `31 §5.8` — `RegistryExtension` is non-sealed for the same reason (third-party adapter support).

**Arguments for non-sealed (current Round-1 default).**
- Matches `RegistryExtension` posture (`31 §5.8`): third-party adapters (e.g. a community `semstrait-adapter-clickhouse`) must be able to impl without a sealing workaround.
- Dialect identity is inherently adapter-facing; hiding it behind a seal would create a different seam at the `DialectId` level (which is a newtype anyway).

**Arguments for sealed.**
- Keeps in-workspace control over the dialect-capability-enumeration surface. Adapter crates outside the workspace can still impl a `Dialect`, but the capability vocabulary (`Capability` in §6.6) must be ratified here.
- Reduces risk of third-party adapters shipping "incomplete" dialects that mis-declare capabilities.

**Current position in `35`.** Non-sealed, matching `RegistryExtension`.

**Next step.** Revisit with `36`'s final capability-roster draft. If `36` grows a capability-registration mechanism similar to `14a`'s adapter function extension, `Dialect` may become sealed with an `AdapterWitness`-style escape hatch on `Capability` alone.

---

## Q-IR-006 — `Schema` placement: `semstrait-ir` vs. `semstrait-core`

**Question.** `35 §5.1` exposes `Schema { fields: Vec<Field> }` at the plan layer. A plan-level `Schema` is structurally identical to (a subset of) the Manifest-level `ResolvedBinding.sources[*].columns` shape in `15 §4.2`. Should `Schema` live in `semstrait-core` (shared between Manifest, IR, and whoever needs it) or in `semstrait-ir` (plan-layer-specific)?

**Refs.**
- `35 §5.1` — current placement inside `semstrait-ir::plan::NodeMeta`.
- `15 §4.2` — Manifest-layer column shape on `ResolvedPhysicalSource`.
- `31 §2` — `semstrait-core` module roster does NOT list a `schema` module today.

**Arguments for placing in `semstrait-core` (shared).**
- Single source of truth; avoids near-duplicate shapes in `semstrait-manifest` and `semstrait-ir`.
- Downstream tools (catalog inspectors, drift detectors, schema diff tools) can import just `semstrait-core` without linking `semstrait-ir`.

**Arguments for keeping in `semstrait-ir` (current Round-1 default).**
- The plan-level `Schema` is a *derived* artifact of the plan tree — every plan node's `output_schema` is a re-computable function of the node and its children. The Manifest-level schema is the *authored* artifact.
- Keeping them separate respects the layering: Manifest is input to `plan`, `Schema` is output of each plan node.
- Future `[TD-IR-SCHEMA-SHARING]` — if `semstrait-planner` starts sharing schema machinery with `semstrait-manifest`, consolidation can happen then.

**Current position in `35`.** Plan-layer `Schema` lives in `semstrait-ir`. A convergence with the Manifest-layer `Schema` is a `[TD-IR-SCHEMA-SHARING]` item.

**Next step.** Decide at `33` (Manifest) drafting time. If `33` ratifies `Schema` at `semstrait-core` as a shared type, `35` re-exports it and drops the local definition.

---

## Q-IR-007 — `SemanticPlan::diagnostics` vs. a separate `PlanResult`

**Question.** `35 §3.1` places a `diagnostics: Vec<Diagnostic>` field on `SemanticPlan` itself. An alternative is a `PlanResult { plan: SemanticPlan, diagnostics: Vec<Diagnostic> }` at the planner-output seam (in `34`), keeping `SemanticPlan` diagnostic-free.

**Refs.**
- `10 §3.4` — planner errors fail-fast; warnings / notes propagate.
- `30 §7` — fail-fast stage returns carry warnings alongside output.
- `34` (pending) — planner entry-point signature.

**Arguments for diagnostics on `SemanticPlan` (current Round-1 default).**
- Keeps warning / note provenance attached to the plan it describes; a `SemanticPlan` serialized to disk and later re-inflated carries its original diagnostics without a separate bookkeeping envelope.
- Adapters consuming a plan from disk (future cache / replay scenarios) see the planner's context without re-running the planner.

**Arguments against.**
- Couples plan identity to diagnostic content — two otherwise-identical plans compare unequal if their warning lists differ. Plan-cache keys must strip `diagnostics` to hash.
- `34`'s per-stage signature (`Result<(Plan, Vec<Diagnostic>), ...>` per `30 §7`) already carries the same information at the stage seam; duplicating on `SemanticPlan` is redundant.

**Current position in `35`.** Diagnostics on `SemanticPlan`. Equivalence / hashing over `SemanticPlan` explicitly excludes `diagnostics` via `PartialEq` / `Hash` impls that skip the field.

**Next step.** Revisit at `34` drafting. If `34` settles on the `Result<(Plan, Vec<Diagnostic>), ...>` pattern from `30 §7` and serialization round-trip is never required, drop `diagnostics` from `SemanticPlan` (a MAJOR for the current doc). Otherwise keep.

---

## Q-IR-008 — Visitor shape: single `visit` vs. enter/exit pair

**Question.** `35 §8.1` defines `PlanVisitor` with a single `visit(&mut self, &PlanNode) -> Output` method, matching the shape of `ExprVisitor` in `31 §3.6`. Is a single-method shape sufficient for the analyses `34` and optimizer passes will write (optimizer-rule matching, plan-tree audit, schema re-check, cost-model population)?

**Refs.**
- `35 §8` — current shape.
- `31 §3.6` — `ExprVisitor` is single-method; parallel Q5 in `31`'s open questions.
- `34` (pending) — optimizer rule-engine consumer.

**Concern.** Some analyses need both pre-order and post-order hooks on the same pass (e.g. schema derivation at post-order + predicate-collection at pre-order). The single-method shape forces each pass to implement its own traversal state machine.

**Proposed alternatives.**
- Add `enter` / `exit` default methods alongside `visit`, with `walk_both` dispatching both.
- Provide two blanket traits `PrePlanVisitor` / `PostPlanVisitor` over the single `visit` with traversal-order implicit in the trait.

**Current position in `35`.** Single-method matching `31 §3.6`'s `ExprVisitor`. Parallel evolution with `ExprVisitor` — any amendment in `31` should propagate to `35`.

**Next step.** Revisit with `34`'s first concrete rule-engine draft. If the rule engine needs enter/exit separation, amend both traits together.

---

## Q-IR-009 — `EnginePlan::Substrait` boxing vs. direct

**Question.** `35 §6.2` boxes `substrait::proto::Plan` inside `EnginePlan::Substrait(Box<substrait::proto::Plan>)`. `substrait::proto::Plan` is large (many kilobytes typical). Should the variant be boxed (current), unboxed (`Substrait(substrait::proto::Plan)`), or `Arc`-wrapped for cheap sharing across consumers?

**Refs.**
- `substrait::proto::Plan` — generated by `prost` from the Substrait proto definitions; size is implementation-detail but easily exceeds 100 bytes.
- `35 §6.2` — current boxed form.

**Arguments for `Box` (current).**
- Keeps `EnginePlan` enum size moderate regardless of `substrait::proto::Plan`'s size growth.
- Standard Rust idiom for large enum variants.

**Arguments for unboxed.**
- One fewer allocation; `EnginePlan` is typically produced once by the adapter and consumed once by the executor.

**Arguments for `Arc`.**
- If multiple consumers want to read the same `EnginePlan` without cloning (e.g. a test harness that runs the same plan through multiple backends), `Arc` enables it cheaply.
- Matches `NodeMeta.output_schema: Arc<Schema>` posture.

**Current position in `35`.** `Box`. If a concrete consumer pattern benefits from `Arc`, the shape can grow; `Box` ↔ `Arc` is MAJOR per `30 §4`, so the decision should be made before v1 stabilizes.

**Next step.** Decide at `36` draft based on concrete consumer patterns.

---

## Q-IR-010 — `Capability` roster placement: `35` vs. `36`

**Question.** `35 §6.6` re-exports `Capability` so `SemanticPlan` consumers can interrogate an adapter, but declares "`Capability` roster ownership is `36`'s". Should the enum itself live in `35` (so planners can consult it without linking `36`) or in `36` (so adapters own their capability vocabulary)?

**Refs.**
- `35 §6.5` — `Dialect::capabilities() -> &'static [Capability]`.
- `36` (pending) — adapter capability roster owner.
- `00 §4.1` — `Capability` does not appear in the root vocabulary table today.

**Arguments for `35` (current Round-1 default).**
- Planners in `34` need the `Capability` vocabulary at plan time (to decide whether to emit an `Agg { distinct: true }` based on adapter support); linking `34` against `36` inverts the DAG.
- `Capability` is a stable vocabulary (like `JoinType` / `Cardinality`) whose additions are additive — `#[non_exhaustive]` protects.

**Arguments for `36`.**
- Adapter-specific; each adapter naturally knows which capabilities it has.
- Keeps `35` free of adapter-layer concerns.

**Current position in `35`.** Exposed in `35`; roster additions ratified in `36` and re-exported.

**Next step.** Confirm with `36` draft that the roster-addition path (`36` authors the new variant; `35` re-exports mechanically) is practical.

---

## Q-IR-011 — `SourceRef` opacity vs. `Display` / structured decomposition

**Question.** `35 §5.2` defines `SourceRef` as a newtype over `(BindingId, u32)` with crate-private construction and accessor methods (`binding_id()`, `source_index()`). Should it be fully opaque (no accessors), or should it expose `Display` so diagnostic messages can render it without an extra Manifest-lookup round-trip?

**Refs.**
- `35 §5.2` — current accessor methods.
- `31 §7.3` — parallel concern on `SourceId`; `Q6` in `31`'s open questions.
- `10 §5.1` — `Location` / `SourceId` handling model.

**Arguments for current accessors.**
- `binding_id()` / `source_index()` let the adapter resolve against the Manifest without exposing internal memory layout.
- Diagnostic rendering can look up the source via the accessors.

**Arguments for fully opaque.**
- Hides any future change to the internal `(BindingId, u32)` → `(BindingId, CatalogRef)` migration.
- Matches `SourceId`'s opaque-with-`as_str()` posture in `31 §7.3`.

**Arguments for `Display`.**
- Reduces the lookup round-trip for diagnostics.
- Matches a common Rust convention ("values you'd want to print should `Display`").

**Current position in `35`.** Accessors present, no `Display`. Consumers needing a renderable form pass the `SourceRef` + `Manifest` into a rendering helper.

**Next step.** Decide at `33` (Manifest) ratification. If the Manifest ratifies a `source_ref_to_display(manifest, source_ref) -> String` helper, `SourceRef::Display` is redundant; otherwise consider adding.

---

## Q-IR-012 — `UnionNode.distinct` vs. a separate `Distinct` `PlanNode` variant

**Question.** `35 §4.7` carries a `distinct: bool` on `UnionNode` to distinguish `UNION ALL` (`false`) from `UNION DISTINCT` (`true`). A future design question is whether `DISTINCT` (without an immediately-preceding union) warrants a first-class `PlanNode::Distinct` variant, or whether it is always lowered to `Agg { group_by: all_columns, aggregates: [] }`.

**Refs.**
- `35 §4.7` — current `UnionNode.distinct`.
- `35 §11.1` — deferred variants list `Distinct` as a candidate.
- SQL — `DISTINCT` is a projection-level or union-level modifier.

**Arguments for the current lowering.**
- `Agg { group_by: all, aggregates: [] }` is the canonical SQL translation of `DISTINCT`. Every engine supports it; no adapter needs a special case.
- Keeps the `PlanNode` catalog compact.

**Arguments for a dedicated `Distinct` variant.**
- Makes plan-tree diff / audit tools recognize "DISTINCT was intended" without inference.
- Maps directly to Substrait's `AggregateRel` with empty measures (no real savings) OR to a future Substrait primitive.

**Current position in `35`.** `Agg` lowering for DISTINCT. `UnionNode.distinct` flag is the one exception because `SET_OP_UNION_DISTINCT` is a primitive Substrait variant and the equivalent `Union + Agg` lowering loses round-trip fidelity.

**Next step.** Revisit if a concrete `34` pass needs to distinguish "author-intended DISTINCT" from "compile-synthesized DISTINCT".

---

## Q-IR-013 — `FetchNode` split into `Limit` + `Offset`

**Question.** `35 §4.9`'s `FetchNode` carries both `limit` and `offset` as `Option<u64>`. Substrait's `FetchRel` does the same, but some engines (PostgreSQL, DuckDB) support `OFFSET` independently of `LIMIT`. Should `FetchNode` be split into a `Limit` node and an `Offset` node?

**Refs.**
- `35 §4.9` — current combined shape.
- Substrait `FetchRel` — combined.
- `30 §4` — variant additions are MINOR.

**Arguments for combined (current Round-1 default).**
- Matches Substrait's shape — one-to-one mapping without restructuring.
- Typical use case (offset-paginated queries) combines them.

**Arguments for split.**
- More orthogonal; a plan with only `OFFSET` doesn't carry a trivially-unused `limit: None`.
- Optimizer rewrites can individually eliminate / merge each form.

**Current position in `35`.** Combined `FetchNode`. Splitting is a future MINOR if a concrete consumer benefits.

**Next step.** Monitor `34` optimizer rule development; if any rule specifically targets `Offset`-without-`Limit` or `Limit`-without-`Offset`, consider splitting.

---

## Q-IR-014 — `NodeMeta.annotations` stability as wire form

**Question.** `35 §5.1` carries `Vec<SemAnnotation>` on every `PlanNode`. `SemAnnotation` round-trips through Substrait's `AdvancedExtension.optimization` with URN `urn:semstrait:annotations:v1`. Is the annotation vocabulary stable enough for v1 wire form, given that `34` is still ratifying the annotation set?

**Refs.**
- `35 §5.1` — annotations field definition.
- `35 §9.2` — Substrait mapping.
- `34` (pending) — annotation-producer rules.

**Arguments for current posture (annotations on `NodeMeta` with serde-tagged enum).**
- The `SemAnnotation` enum is `#[non_exhaustive]`; new variants are MINOR per `30 §4`.
- Substrait's `AdvancedExtension.optimization` is an opaque byte blob per the Substrait spec; consumers that don't know a given annotation URN skip it. Forward-compatible.

**Arguments against.**
- The annotation vocabulary ratification is `34`'s; putting it on every `PlanNode` at v1 commits `35` to whatever annotation roster exists at v1 ratification. Future growth is additive, but the initial roster may be incomplete.

**Current position in `35`.** `Vec<SemAnnotation>` retained; annotation-roster ratification is `34`'s.

**Next step.** Confirm at `34` draft that the annotation-producer side has a migration story for new annotations (old consumers skipping unknowns gracefully). If not, narrow the initial roster to the absolutely-necessary set.

---
