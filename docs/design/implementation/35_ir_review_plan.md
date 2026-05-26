---
title: IR Implementation Review — Plan, Verification & Agent Dispatch
status: planning
review_target: crates/semstrait-ir (worktree feature+ir-impl)
spec: docs/design/apis/35_semstrait_ir.md
sibling: 35_ir_review.md
---

# IR Implementation Review — Plan, Verification & Agent Dispatch

This is the **planning artifact** for the second-pass review of `crates/semstrait-ir`
on `worktree-feature+ir-impl`. The findings tracker (decision-bearing) lives in
`35_ir_review.md`; that document is for clause-level ratification per CLAUDE.md
workstyle rule 4. **This file** captures:

1. Verification of the 19 prior findings against the current source.
2. Agent dispatch plan (5 parallel reviewers, scoped by category).
3. Synthesis approach — how findings flow back into the tracker.
4. Workstyle anchors — what needs explicit user approval and when.

No code changes are proposed here. All concrete edits land via the tracker
(`35_ir_review.md`), and only after clause-level approval per workstyle rule 3.

---

## Vision anchors (review reference frame)

The seven principles that define what `semstrait-ir` is FOR — every finding,
recommendation, and dispatch brief must trace back to one of these:

1. **Plan-construction toolkit on top of `semstrait-model`.** Core abstractions
   for manipulating / building semantic plan or query plan.
2. **Canonical-first.** Aggregates compute-engine knowledge into engine-agnostic
   plan-building primitives (no per-dialect leakage).
3. **Provider, not consumer.** Provides canonical classes / objects /
   structures / APIs / structural validation. Other crates consume.
4. **Two-form Expr.**
   - `PhysicalExpr` — canonical, engine-agnostic, operates on columns / projections.
   - `SemanticExpr` — semantic sugar on top, operates with fields / dims / keys /
     metrics / measures.
5. **Computation contract.** `PhysicalExpr` is **never evaluated** in semstrait —
   only expresses computation. `PhysicalExpr` is adaptable; `SemanticExpr` is
   not fully adaptable (it lowers to `PhysicalExpr` at adapt-phase boundary).
6. **Substrait-inspired completeness.** Must cover all plan abstractions — or
   reserve them with `#[non_exhaustive]` placeholders — in lightweight form.
7. **Validation = structural / logical.** Semstrait doesn't compute; only
   produces plans. Validation is shape-correctness only.

---

## Review constraints (operational)

Hard rules from the task brief and CLAUDE.md (verbatim where given):

- **Workstyle.** "1. Root cause analysis 2. Present results 3. Wait for
  confirmation -- do NOT apply code changes until the human approves the fix
  4. Design / spec edits — clause-level approval."
- "Don't add features, refactor, or introduce abstractions beyond what the task
  requires."
- "Don't add error handling, fallbacks, or validation for scenarios that can't
  happen."
- "Ensure that code is not panicing, using Result enum pattern and panic is
  the only last thing when handling is not possible."
- "If multiple interpretations exist, present them - don't pick silently, ask
  for clarification."
- "Don't assume - check indexing and provided documentation specs - look for
  answer, if cant find - ask."
- No verbose comments. No docstring-padding for behavior already obvious from
  identifiers.
- No abstractions for single-use shapes.
- Avoid copying — `Cow` / borrow over clone where shape allows.
- Per `m15-anti-pattern` and `coding-guidelines` from rust-skills bundle.

---

## §1 — Verification of prior findings against current code

Each item below cites the current source state (worktree `feature+ir-impl`,
last commit `7304559`) and marks the prior finding as:

- **VERIFIED** — finding holds against current code as written.
- **CLARIFIED** — finding holds but the prior text mis-states a detail; the
  clarification is in the body.
- **REFUTED** — finding does not hold; current code does not exhibit the issue.
- **OBSOLETE** — code has changed such that the finding no longer applies.

### Iteration 1 — Aggregate kernel architecture

| ID | Status | Citation | Note |
| --- | --- | --- | --- |
| B-1 | **VERIFIED** | `expr_kinds.rs:60-66` `AggregationOp { Sum, Avg, Count, Min, Max }` (closed-five) vs `functions/builtins/aggregate.rs` registers 8 additional entries. | The carrier-vs-catalog mismatch is real. Adapter codepath exhaustiveness over `AggregationOp` to be confirmed during dispatch agent (1). |
| B-2 | **VERIFIED** | `primitives.rs:213-220` `AggregateExpr.input_expr: PhysicalExpr` (single, not `Vec`). Rustdoc cites Q-IR-NEW-003. | Single-arg lift contract holds for closed-five only. `string_agg(expr, delim)` and `percentile_cont(expr, frac)` are 2-ary — cannot be expressed today. |
| P-9 | **DEPENDENT** | n/a — meta-finding | Re-evaluate after B-1 / B-2 land. |

### Iteration 2 — Leaf typing + validate completeness

| ID | Status | Citation | Note |
| --- | --- | --- | --- |
| S-2 | **VERIFIED** | `expr_kinds.rs:148-172` `Literal` enum is value-only — no `inferred_type` field. | `Decimal(rust_decimal::Decimal)`, `Time`, `Timestamp` precisions cannot be recovered from the value alone. |
| S-3 | **VERIFIED** | `plan/validate.rs` — `JoinNode.on` is structurally walked but no resolution-against-schema or type-match check is performed. | (a) resolution check independent; (b) type-match coupled to S-2. |
| S-4 | **CLARIFIED** | `plan/validate.rs:108` `PlanNode::Scan(_) \| PlanNode::Values(_) => Ok(())` — Values is short-circuited. **Type clarification:** `ValuesNode.rows: Vec<Vec<PhysicalExpr>>` (per `plan/node.rs:436-452`), **not** `Vec<Vec<Literal>>` as prior review states. Arity check still missing; per-cell type-match is wider scope than prior text suggests because cells can be arbitrary `PhysicalExpr`. | Update tracker to reflect `PhysicalExpr` cell type. |
| S-5 | **VERIFIED** | `plan/validate.rs:119-146` only walks aggregate-vs-aggregate name uniqueness within a single pass; key names and aggregate output names are not jointly checked. | Confirm exact field name on `AggNode.aggregates[i]` during agent (2). |
| S-6 | **VERIFIED** (open between options a/b) | `PlanNode` does not expose a uniform `output_schema()` accessor today; `NodeMeta.schema: Arc<Schema>` is set explicitly per node by the planner (per `plan/meta.rs`). | Pattern is (b) — planner-set schema; validator should check consistency rather than auto-derive. |

### Iteration 3 — Error layering + DSL + surface

| ID | Status | Citation | Note |
| --- | --- | --- | --- |
| S-1 | **VERIFIED** | `expr/expr_fn.rs:386, 403, 419, 437, 453, 471, 499, 524` — 8 `debug_assert!(false)` + silent passthrough sites in `SemanticExprAccessorExt`. | Methods: `first`, `last`, `previous`, `next`, `delta`, `percent_change`, `lag`, `lead`. Caller search needed to choose between B and C. |
| S-7 | **VERIFIED** | `functions/registry.rs:50, 56, 60` — `assert!()` macros in `bootstrap()`. | Coupled to TD-REGISTRY-EXTENSION-WIRING. |
| S-8 | **VERIFIED** | `error.rs:165-207` — `IrErrorKind::StructuralViolation { kind: &'static str, reason: String }`. Payload is thin (kind+reason); no node id or field. | Diagnostic envelope shape to be checked during agent (3). |
| P-4 | **VERIFIED** | `lib.rs` — flat `pub use` namespace mixes plan-tree types, registry internals, and trait-helpers. | Curation pass needed. |
| P-7 | **VERIFIED** | `lib.rs:69` re-exports `RegistryExtension` despite TD-REGISTRY-EXTENSION-WIRING. | Hide behind `pub(crate)`. |

### Iteration 4 — Function catalog completeness

| ID | Status | Citation | Note |
| --- | --- | --- | --- |
| P-2 | **VERIFIED** | `functions/builtins/temporal.rs:12-13` — `let time_default = DataType::Time { precision: 6 }; let _ = time_default;` dead binding. | Recommend deletion. |
| P-3 | **PARTIAL** | Cross-reference table not yet built; spot-checks against `35 §14a` show plausible gaps in math/aggregate decimal overloads. | Full table is agent (3) deliverable. |

### Iteration 5 — Trait shape + hygiene

| ID | Status | Citation | Note |
| --- | --- | --- | --- |
| P-1 | **PARTIAL** | Tests-verify-derive list not yet enumerated. Sample: `primitives.rs:283-289` `name_hash_is_deterministic`, `primitives.rs:308-314` `source_ref_serde_json_roundtrip` — these test serde/Hash invariants which are partially derive-driven but also exercise the wrapper layer. Borderline. | Concrete file:line list is agent (4) deliverable. |
| P-5 | **VERIFIED** | `expr/tree.rs` and `plan/traversal.rs` — `Tree::transform` bundles match/child-walk/rebuild in single fn. | Refactor sketch is agent (4) deliverable. |
| P-6 | **VERIFIED** | `functions/spec.rs:70-81` — manual `PartialEq` returns `false` for `(Custom, Custom)` (line 77). | Reflexivity break confirmed. |
| P-8 | **VERIFIED** | `functions/spec.rs:22` — `description: &'static str` field on `FunctionSpec`. No runtime caller; cost is per-spec memory + bootstrap noise. | Spot-check during agent (3) for any tooling read. |

---

## §2 — New findings to surface during dispatch

Items the prior review didn't cover but that fall inside the seven vision
principles. Each agent's brief asks for additions in this list.

### N-1 candidate — Substrait coverage parity

Vision principle 6 ("Substrait-inspired completeness") is not currently
audited. The 9 `PlanNode` variants (Scan, Filter, Project, Agg, Join, Union,
Sort, Fetch, Values) cover the v1 minimum, but Substrait covers ~20 relational
operators (Cross, NestedLoopJoin, MergeJoin, Window, Set ops, Reference,
ExtensionLeaf/Single/Multi, Update/Delete, …). For each gap, the IR can either:

- (a) reserve via `#[non_exhaustive]` extension point for post-v1, or
- (b) document why it's intentionally absent in v1 (out-of-scope for semstrait).

Agent (5) builds the gap matrix.

### N-2 candidate — `Expr` 14-variant completeness vs Substrait expression set

`Expr<L>` carries 14 variants (Leaf, BinaryOp, UnaryOp, FunctionCall, Cast,
Case, InList, Between, Like, IsNull, Coalesce, NullIf, Aggregate, Window).
Substrait `Expression` covers ~12 forms but has subtle additions: `Switch`
(distinct from generic `Case`), `IfThen` (specialization), `SubqueryExpr`
(scalar/in/exists/comparison). Agent (1) audits.

### N-3 candidate — `PhysicalExpr` "never evaluated" enforcement

Vision principle 5 says `PhysicalExpr` is never evaluated. Today the type
system does not encode this — there is no `Sealed` marker preventing an
out-of-crate consumer from writing `match expr { … }` and trying to interpret.
Whether to add a marker (sealed trait variant on the leaf side) or rely on
documentation is the question. Agent (4) raises.

### N-4 candidate — Cardinality / NodeMeta schema sharing under `Arc`

`NodeMeta.schema: Arc<Schema>` shares Arcs across plan nodes. Vision principle
"avoid copying" is honored, but there is no explicit invariant about whether
two siblings can share the same Arc when their projection differs. If the
planner ever shares an Arc that one side then logically narrows, the validator
won't catch it. Agent (2) raises.

### N-5 candidate — `Capability` openness vs adapter contract

`Capability` enum has 5 v1 variants and is `#[non_exhaustive]`. Adapters that
match on it must include a wildcard arm — fine in principle, but if a future
MINOR addition adds a capability that breaks an existing artifact's
assumptions, the wildcard hides the gap. Document the read protocol: how does
an adapter declare "I don't yet know about this capability"? Agent (3) raises.

---

## §3 — Agent dispatch plan

Five parallel reviewers. Each gets:

- Vision principles (§ above, full text).
- Operational constraints (§ above, full text).
- The 19 prior findings (link to `35_ir_review.md`).
- Verification status (this file §1).
- The N-* candidates relevant to its scope.
- Specific files/lines to verify and the question to answer.

**Output format expected from each agent (under 800 words):**

```
## Agent N — <category>

### Verifications
- <ID>: <one-line status, file:line citation, any clarification>

### Recommendations (per-finding)
- <ID>: <Option A/B/C, rationale in 1-3 sentences, derived clauses flagged>

### New findings (if any)
- <N-x or fresh ID>: <severity, finding, file:line, recommendation>

### Cross-iteration impact
- <if a recommendation here forces a re-decision elsewhere, list it>
```

### Agent (1) — Expr-tree, leaves, DSL design

**Scope.** Iteration 1 (B-1, B-2, P-9) + Iteration 3 S-1 + N-2 + N-3.

**Files.**
- `crates/semstrait-ir/src/expr.rs`
- `crates/semstrait-ir/src/expr_kinds.rs`
- `crates/semstrait-ir/src/expr/tree.rs`
- `crates/semstrait-ir/src/expr/leaves.rs`
- `crates/semstrait-ir/src/expr/parameter.rs`
- `crates/semstrait-ir/src/expr/expr_fn.rs`
- `crates/semstrait-ir/src/primitives.rs` (for `AggregateExpr` only)

**Question.** Does the two-form Expr design (`PhysicalExpr` / `SemanticExpr`)
faithfully realize vision principles 4 & 5? Are aggregates structurally
correct given the v1 catalog has 8 non-five aggregates? Are the 8
`debug_assert!(false)` accessor methods (S-1) actually called from
`semstrait-model` or only from tests?

**Brief excerpt.** "Verify that the closed `AggregationOp` in `expr_kinds.rs`
matches the registered builtins; if not, recommend B-1 option (A/B/C) with
rationale anchored in vision principle 2 (canonical-first). For B-2, propose
the `args` shape that aligns with the chosen B-1 option. Search across the
worktree for callers of `SemanticExprAccessorExt::{first, last, previous,
next, delta, percent_change, lag, lead}` (S-1). Report whether they have any
non-test callers."

### Agent (2) — Plan validate + node completeness

**Scope.** Iteration 2 S-3, S-4, S-5, S-6 + N-4.

**Files.**
- `crates/semstrait-ir/src/plan/node.rs`
- `crates/semstrait-ir/src/plan/validate.rs`
- `crates/semstrait-ir/src/plan/meta.rs`
- `crates/semstrait-ir/src/plan/traversal.rs`
- `crates/semstrait-ir/src/types.rs` (for `Schema` / `SchemaColumn` only)

**Question.** Does `validate.rs` enforce structural correctness across all 9
`PlanNode` variants? List per-variant invariants that the spec (35 §10–§12)
requires; mark which are checked vs unchecked. Confirm S-4 row-cell type
(`PhysicalExpr` not `Literal`); decide whether per-cell validation is in
scope for v1.

**Brief excerpt.** "For S-6, inspect how `NodeMeta.schema` is set today and
whether the planner is the source of truth (option b) or the IR derives it
(option a). Recommend the simpler pattern given vision principle 7
(structural validation only). For S-3, `KeyPair` carries `Name`; trace the
schema-resolution machinery available to `validate.rs`."

### Agent (3) — Functions, types, error layering

**Scope.** Iteration 3 S-7, S-8 + Iteration 4 P-2, P-3 + Iteration 5 P-6, P-8
+ N-5.

**Files.**
- `crates/semstrait-ir/src/functions/spec.rs`
- `crates/semstrait-ir/src/functions/registry.rs`
- `crates/semstrait-ir/src/functions/extension.rs`
- `crates/semstrait-ir/src/functions/builtins/{aggregate,math,string,temporal,logical}.rs`
- `crates/semstrait-ir/src/error.rs`
- `crates/semstrait-ir/src/artifact.rs` (for `Capability` / `DialectId` only)

**Question.** Build the §14a × catalog cross-reference table for P-3.
Quantify P-8 (`description` field) memory cost. Trace P-6
`ReturnTypeRule::Custom` callers — is anyone actually comparing? For S-7 and
S-8, propose the error-channel architecture that fits the planner's
`Diagnostic<K>` envelope (read it; quote the structured fields).

**Brief excerpt.** "Read `Diagnostic<K>` in `semstrait-common` to confirm
whether structured fields (node-id, field, expected/actual) live there. If
yes, S-8 option B (single-string IR payload) holds; if no, recommend option A
with the exact `StructuralReason` enum members."

### Agent (4) — Rust quality, anti-patterns, hygiene

**Scope.** Iteration 5 P-1, P-5 + Iteration 3 P-4, P-7 + N-3.

**Files.**
- `crates/semstrait-ir/src/lib.rs`
- All test modules in `crates/semstrait-ir/src/`
- `crates/semstrait-ir/src/expr/tree.rs` (`Tree::transform`)
- `crates/semstrait-ir/src/plan/traversal.rs`
- Any `pub(crate)`-vs-`pub` audit candidates

**Question.** Apply `m15-anti-pattern` checklist. Enumerate all tests in
`semstrait-ir` and classify each as "tests IR invariant" or "tests derived
trait." Sketch the `Tree::transform` refactor. Inventory `lib.rs` re-exports
and propose a curated public surface.

**Brief excerpt.** "For each item in `lib.rs`'s `pub use` list, decide
public/internal. Items that consumers (`semstrait-model`, future
`semstrait-adapter-*`) genuinely need = public; everything else = `pub(crate)`.
Use the spec contract in `35 §1.5` (`PUBLIC API SURFACE`) as the authority."

### Agent (5) — Substrait coverage parity & vision-principle audit

**Scope.** N-1 + cross-cutting vision audit. No prior-finding ID assigned.

**Files.**
- `crates/semstrait-ir/src/plan/node.rs` (all 9 variants)
- `crates/semstrait-ir/src/expr.rs` (all 14 variants)
- `crates/semstrait-ir/src/artifact.rs`
- `docs/design/apis/35_semstrait_ir.md` (for spec-side contract)
- `docs/design/foundations/` (for cross-cutting architecture)

**Question.** Build the Substrait-vs-semstrait-IR coverage matrix. For each
Substrait relational operator, mark: covered, reserved (`#[non_exhaustive]`),
or intentionally absent (with rationale). Same for Substrait `Expression`
forms. Identify any vision-principle-6 violations (capability gaps with no
extension point).

**Brief excerpt.** "Use the Substrait spec as the reference (operators listed
in `algebra.proto` Rel oneof). For each, locate the equivalent in
`PlanNode` or note its absence. Where `35_semstrait_ir.md` already declares
something out-of-scope, cite the spec section. Surface anything that is
silently missing — not declared and not implemented."

---

## §4 — Synthesis approach

After agents return:

1. **Reconcile.** Where agents disagree on a finding's resolution, surface the
   disagreement to the user; do not pick silently (per "if multiple
   interpretations exist, present them").
2. **Merge into tracker.** Update each finding in `35_ir_review.md`'s
   Status / Decision / Notes fields with: agent IDs touching the finding, the
   verification citation, and the recommended option.
3. **Surgical change plan.** For approved findings only (post-user-decision),
   produce a per-clause edit list:

   ```
   <ID>: <file:line range> — <one-line surgical description>
   - rationale: <vision principle invoked>
   - blast radius: <what else changes>
   ```

4. **Workstyle gate.** Surgical change plan is presented; user approves
   clause-by-clause; only then code edits begin. **No edits before approval.**

---

## §5 — Out-of-scope (explicit non-goals for this review)

To prevent scope creep:

- Performance benchmarking. Vision principle 7 says we don't compute; we don't
  benchmark IR construction under this review.
- Adapter wiring beyond `Capability` / `DialectId` audit. `semstrait-adapter-*`
  crates are out of this worktree.
- `semstrait-model` review. `semstrait-ir` is the surface; model usage is a
  caller-side concern.
- Spec edits. If a finding implies a spec change, raise it as a candidate;
  the spec edit lands as a separate ratification per
  `docs/design/STATUS.md` discipline.
- Doc-comment polish that doesn't change behavior or API surface. Trim during
  apply-phase, not during review.

---

## §6 — Open meta-questions for the user

Surface items that need explicit direction before agents can finalize:

1. **Aggregate kernel direction (B-1).** The recommendation is option B (drop
   `AggregationOp`, string-key via `CanonicalFn`). This is a structural shape
   choice. Confirm before agent (1) finalizes B-2 / P-9.
2. **`SemanticExprAccessorExt` stub methods (S-1).** Agent (1) will return a
   caller search; if there are no non-test callers, recommend option C
   (delete). Confirm willingness to delete vs preserve for syntax surface.
3. **`Tree::transform` refactor (P-5).** Whether to do this in this pass at
   all, or defer until the visitor/rewriter contract is exercised by a real
   consumer.
4. **Substrait coverage matrix (N-1).** Whether silent gaps should be filled
   in this pass with `#[non_exhaustive]` reservations, or deferred to
   post-v1 with the matrix as the deferral artifact.

These questions are tracked here for visibility; they are **not** decided
without user input.
