---
title: IR Implementation Review — Surgical Change Plan
status: awaiting-approval
review_target: crates/semstrait-ir (worktree feature+ir-impl)
spec: docs/design/apis/35_semstrait_ir.md
related: 35_ir_review.md, 35_ir_review_plan.md
---

# IR Implementation Review — Surgical Change Plan

This file lists every concrete code or spec edit that follows from the second-pass
review (see `35_ir_review.md` second-pass section + `35_ir_review_plan.md`).
**No edit listed here is authorized.** Each clause must be approved by the
user before it is applied (workstyle rule 3, clause-level approval).

Format per clause:

- **ID** — finding ID from `35_ir_review.md`.
- **Surgical edit(s)** — file:line range + one-line description.
- **Rationale** — vision principle + spec cite.
- **Blast radius** — what else changes in the same edit pass.
- **Derived clauses** — independent decisions inside this edit, each its
  own approval gate.

---

## §1 — Block-severity (Iteration 1: aggregate kernel)

### Clause B-1 — Aggregation operator carrier

**Status:** awaiting direction (Option A / B / C).

**Recommendation (agent 1):** Option **C** — hybrid `AggregateKind`.

**Surgical edit (Option C):**

- `crates/semstrait-ir/src/expr_kinds.rs:60-66` — keep `AggregationOp` closed
  five.
- `crates/semstrait-ir/src/expr_kinds.rs` (new) — add:
  ```rust
  #[non_exhaustive]
  #[derive(Debug, Clone, PartialEq, Eq, Hash)]
  pub enum AggregateKind {
      Builtin(AggregationOp),
      Extension(CanonicalFn),
  }
  ```
- `crates/semstrait-ir/src/expr/tree.rs:147-152` — change
  `Expr::Aggregate { op: AggregationOp, … }` to
  `Expr::Aggregate { op: AggregateKind, … }`.
- `crates/semstrait-ir/src/primitives.rs:213-220` — change
  `AggregateExpr.aggregation: AggregationOp` to `AggregateExpr.aggregation:
  AggregateKind`.
- All construction sites in `crates/semstrait-ir/src/` — wrap existing
  `AggregationOp::Sum` as `AggregateKind::Builtin(AggregationOp::Sum)`.
  (Estimated ~15-20 sites; mechanical.)

**Rationale.** Vision-2 (canonical-first), vision-6 (Substrait
completeness). `35 §11.7:1370` and `14a §3.6:263` ratify closed-five carrier
as normative; Option B would amend spec normative text. Option C admits the
8 catalog extensions without breaking the spec invariant.

**Blast radius.** `expr_kinds.rs`, `expr/tree.rs`, `primitives.rs`,
`functions/registry.rs` (lookup paths), `functions/builtins/aggregate.rs`
(no change — these were already extensions), all test fixtures constructing
`Expr::Aggregate`. `semstrait-model` is downstream and out of scope (Q2=B).

**Derived clauses (each own decision):**
- D-B1.1: arity enforcement boundary — at `AggNode::new` constructor or in
  `tree.rs::check_well_formed`?
- D-B1.2: should `AggregateKind::Extension` carry `Additivity` resolved at
  construction or at compile?
- D-B1.3: rustdoc on `AggregationOp` — describe its closed-five role within
  `AggregateKind::Builtin`.

### Clause B-2 — `AggregateExpr` arg list (contingent on B-1)

**Status:** awaiting direction.

**Recommendation (if B-1 = C):** change `input_expr` to `args:
Vec<PhysicalExpr>`.

**Surgical edit:**

- `crates/semstrait-ir/src/primitives.rs:213-220` — replace `input_expr:
  PhysicalExpr` with `args: Vec<PhysicalExpr>`.
- `crates/semstrait-ir/src/primitives.rs:196-211` — rewrite doc-comment;
  drop Q-IR-NEW-003 cite (or amend the question's text upstream).
- All construction sites — wrap single-arg as `vec![expr]`.
- `docs/design/apis/35_semstrait_ir.md:1361-1376` (§11.7) — **spec edit**:
  field name `input_expr` → `args`; arity statement `singular` → `1..=N`
  with constraint `args.len() == arity_of(aggregation)`.

**Rationale.** Vision-2; admits 2-ary `string_agg`, `percentile_cont` from
catalog without re-touching the lift codepath later.

**Blast radius.** `primitives.rs`, all `AggregateExpr` construction (~6
sites in tests and Phase B lift paths), spec §11.7. Spec edit needs its own
ratification.

**Derived clauses:**
- D-B2.1: arity invariant — where is `args.len() == arity_of(aggregation)`
  enforced?
- D-B2.2: spec amendment for `35 §11.7` text — separate clause-level
  approval per workstyle rule 4.

### Clause P-9 — Aggregate metadata cohesion (contingent on B-1)

**Status:** awaiting direction.

**Recommendation (agent 1):** No code change for this iteration. Document
that closed-five `Additivity` and `ReturnTypeRule` are **spec-only** (Phase
B Strategy lookups, not stored on `Expr::Aggregate` / `AggregateExpr`). If
B-1 = C lands, place a hard-coded table in `crates/semstrait-ir/src/functions/builtins/mod.rs`
so Phase B / adapters don't re-hardcode.

**Surgical edit:**

- `crates/semstrait-ir/src/functions/builtins/mod.rs` (new helper) — add
  closed-five additivity + return-type table; expose lookup fn.
- `docs/design/apis/35_semstrait_ir.md` §11.7 doc-comment expansion —
  **spec edit**: state that closed-five Additivity is spec-only.

**Blast radius.** Single helper module; consumers update at convenience.
Spec edit clause-level.

---

## §2 — Should-fix (Iteration 2: leaf typing + validate)

### Clause S-2 — Typed literals

**Status:** awaiting direction (option a/b/c).

**Recommendation (agent 2):** Option **(b)** — per-variant disambiguation.

**Surgical edit (Option b):**

- `crates/semstrait-ir/src/expr_kinds.rs:148-172` — change:
  ```rust
  Null  →  Null { data_type: DataType }
  Integer(i64)  →  Integer { value: i64, width: IntegerWidth }
  Float(f64)  →  Float { value: f64, width: FloatWidth }
  ```
  Add `IntegerWidth { Int, Long }` and `FloatWidth { Float, Double }` enums.
- All construction sites for these three variants (~30 sites in tests +
  catalog tests) — provide explicit type/width.

**Rationale.** Vision-7 (structural validation) — validators need types to
check shape; ambiguous literals force adapter inference. `35 §13.5`
mandates literal types round-trip without re-inference.

**Blast radius.** `expr_kinds.rs`, all `Literal::{Null, Integer, Float}`
construction, `expr_kinds.rs` tests, `serde` round-trip tests.

**Derived clauses:**
- D-S2.1: `IntegerWidth` and `FloatWidth` enum names — separate naming
  approval.
- D-S2.2: serde representation (does `Null { data_type }` get a tagged or
  untagged form?).
- D-S2.3: existing `Literal::Decimal { precision, scale, value }` shape
  stays; verify alignment with the new pattern.

### Clause S-3a — Join key resolution check (CLOSED)

**Status:** **already implemented**. `validate.rs:148-174` already raises
`DanglingReference` per side.

**Action:** mark CLOSED in tracker; no code edit.

### Clause S-3b — Join key type-match check (REFRAMED)

**Status:** awaiting direction.

**Recommendation (agent 2):** Reframe — type-match is enforced **at
compile** (Phase A semantic→physical lowering) per `16 §5.1` and `35 §13.5`.
IR carries `JoinKeyTypeMismatch` (`§16.3`) only as a **residual-trust
diagnostic** at adapter ingress, not as a primary gate.

**Surgical edit:**

- `crates/semstrait-ir/src/plan/validate.rs` (`validate_join_node` body) —
  optional residual check: walk `KeyPair`s, look up types in child schemas,
  compare; if mismatch, emit `IrErrorKind::JoinKeyTypeMismatch`. **Skip if
  S-3b reframed as compile-only.**

**Blast radius.** `validate.rs` only.

**Derived clauses:**
- D-S3b.1: residual check on/off — if on, the IR validator is a
  trust-but-verify layer; if off, it relies entirely on Phase A.

### Clause S-4 — `ValuesNode` row arity

**Status:** awaiting direction (option a/b).

**Tracker correction.** `rows: Vec<Vec<PhysicalExpr>>` (not
`Vec<Vec<Literal>>`).

**Recommendation (agent 2):** Option **(a)** — narrow to
`Vec<Vec<Literal>>` for v1.

**Surgical edit (Option a):**

- `crates/semstrait-ir/src/plan/node.rs:438-442` — change `rows:
  Vec<Vec<PhysicalExpr>>` to `rows: Vec<Vec<Literal>>`.
- `crates/semstrait-ir/src/plan/validate.rs:108` — replace short-circuit;
  add per-row arity loop:
  ```rust
  PlanNode::Values(v) => {
      for (i, row) in v.rows.iter().enumerate() {
          if row.len() != v.schema.fields.len() {
              return Err(ValidateError::structural(
                  "Values.row_arity",
                  format!("row {} has {} cells, schema has {} fields",
                      i, row.len(), v.schema.fields.len())
              ));
          }
      }
      Ok(())
  }
  ```
- After S-2 lands, add per-cell `data_type` match.

**Rationale.** Vision-7 (structural validation). Narrowing aligns with v1
"constants only" expectation.

**Blast radius.** `node.rs`, `validate.rs`, any tests constructing
`ValuesNode` with non-literal cells (likely zero per current spec scope).

**Derived clauses:**
- D-S4.1: error message text + `kind` discriminator string (`"Values.row_arity"`).
- D-S4.2: per-cell type-match — coupled to S-2 landing.

### Clause S-5 — `AggNode` output-name uniqueness

**Status:** awaiting direction.

**Recommendation (agent 2):** approve. One-line extension to existing HashSet.

**Surgical edit:**

- `crates/semstrait-ir/src/plan/validate.rs:130-145` —
  ```rust
  let mut seen: HashSet<&Name> = HashSet::new();
  for k in &agg.keys {
      if !seen.insert(k) {
          return Err(ValidateError::structural(
              "Agg.output_name", format!("duplicate name {}", k.as_str())
          ));
      }
  }
  for (name, _) in &agg.aggregates {
      if !seen.insert(name) {
          return Err(ValidateError::structural(
              "Agg.output_name", format!("duplicate name {}", name.as_str())
          ));
      }
  }
  ```
- `crates/semstrait-ir/src/error.rs` — confirm `IrErrorKind::DuplicateAggOutputName`
  exists in `§16.3` vocabulary; if not, add (per `35 §16.3` ratification).

**Rationale.** Vision-7. `35 §13.6` mandates output names unique across
`keys ∪ aggregates`.

**Blast radius.** `validate.rs`. Possibly `error.rs` if new variant needed.
New tests under `validate.rs` test module.

### Clause S-6 — Outer-join nullability widening

**Status:** awaiting direction (option a/b).

**Recommendation (agent 2):** Option **(b)** — planner-set, validator
equality-checks against locally-widened expectation.

**Surgical edit (Option b):**

- `crates/semstrait-ir/src/plan/validate.rs` — add helper:
  ```rust
  fn widen_for_join(side: JoinSide, schema: &Schema, jt: JoinType) -> Schema { ... }
  ```
  that flips `nullable: true` on the matching side per `JoinType`.
- `crates/semstrait-ir/src/plan/validate.rs` — extend `validate_join_node`
  to compute expected union schema (left-widened ++ right-widened) and
  equality-check against `JoinNode.meta.output_schema`.
- `crates/semstrait-ir/src/error.rs` — add `IrErrorKind::JoinNullabilityMismatch`
  per `§16.3`.

**Rationale.** Vision-7. `35 §13.7` requires Left/Right/Full to widen the
matching side's `nullable` to true. Option (b) keeps planner as authority
(matches today's `Arc<Schema>` pattern) while adding the trust-but-verify
gate.

**Blast radius.** `validate.rs` (one helper + extension), `error.rs` (new
variant). Tests in `validate.rs` test module.

**Derived clauses:**
- D-S6.1: `JoinSide` enum or bool? Recommend bool (`is_left`).
- D-S6.2: error variant naming — `JoinNullabilityMismatch` per §16.3.

---

## §3 — Should-fix (Iteration 3: error layering + DSL)

### Clause S-1 — Drop `SemanticExprAccessorExt` 8 stub methods

**Status:** awaiting direction (option A/B/C).

**Recommendation (agent 1):** Option **C** — delete entirely. **Caller
search confirms zero non-test, non-doc callers.**

**Surgical edit (Option C):**

- `crates/semstrait-ir/src/expr/expr_fn.rs:285-?` — remove `impl
  SemanticExprAccessorExt for SemanticExpr` block (8 methods, ~140 lines).
- `crates/semstrait-ir/src/expr/expr_fn.rs` — remove `trait
  SemanticExprAccessorExt` declaration.
- `crates/semstrait-ir/src/expr/accessor.rs:44, 46` — remove rustdoc
  references to `delta` / `percent_change` accessor builders if they cite
  the removed methods.
- `crates/semstrait-ir/src/expr/expr_fn.rs:855-921` (test module) — remove
  the 6 unit tests calling these 8 methods.
- `docs/design/apis/35_semstrait_ir.md` §6.3 — **spec edit**: drop the
  best-effort accessor-builder surface declaration.

**Rationale.** Vision-3 (provider, not consumer). 8 methods of pure
silent-passthrough cost ~140 lines of trait surface that no caller uses.
`debug_assert!(false, …) → self` is a textbook anti-pattern (panic-hiding-
under-guard).

**Blast radius.** `expr_fn.rs` (removal), `accessor.rs` rustdoc, spec §6.3.

**Derived clauses:**
- D-S1.1: spec amendment for `35 §6.3` — separate clause approval.
- D-S1.2: rustdoc cleanup in `accessor.rs:44, 46`.

### Clause S-7 — Bootstrap `assert!` migration

**Status:** awaiting direction (option A/B/C).

**Recommendation (agent 3):** Option **C** — bundle into TD-REGISTRY-EXTENSION-WIRING.

**Surgical edit (Option C):**

- `crates/semstrait-ir/src/functions/registry.rs:50-63` — add comment:
  ```rust
  // SAFETY: bootstrap data is hard-coded; collision = build-time bug
  //   per §7.2 (TD-REGISTRY-EXTENSION-WIRING converts to Result<_, BootstrapError>
  //   when extension wiring lands).
  ```
- No `assert!` removal until the extension surface is shaped.

**Rationale.** Hard rule "no panics" vs vision-7 "structural-only" — today's
3 asserts cover compile-time invariants over hard-coded data. `Result`
migration prematurely shapes an error type that `RegistryExtension` will
reshape. Bundle the migration with TD-REGISTRY-EXTENSION-WIRING.

**Blast radius.** `registry.rs` comment-only.

### Clause S-8 — `StructuralViolation` payload (KEEP THIN)

**Status:** awaiting direction (option A/B).

**Recommendation (agent 3):** Option **B** — keep thin, file
`[TD-IR-STRUCTURAL-PAYLOAD]`.

**Surgical edit (Option B):**

- Tech-debt tracker entry `TD-IR-STRUCTURAL-PAYLOAD` (filed in the
  spec-tree tech-debt index): "If `Diagnostic<K>` (semstrait-common) ever
  grows structured plan-tree fields (node-id, expected/actual), revisit
  IR's `StructuralViolation` payload."
- No code edit on `error.rs`.

**Rationale.** `Diagnostic<K>` carries source-text Location only — no plan-
tree coordinates. Q-PLAN-14 (2026-05-25) explicitly scoped 11 of 14 §16.3
variants OUT of IR; current thin payload is intentional. Adding parallel
structure would duplicate planner's diagnostic responsibility.

**Blast radius.** Tracker entry only.

### Clause P-4 — `lib.rs` re-export curation

**Status:** awaiting direction.

**Recommendation (agent 4):** approve A+B split (curate vs demote).

**Surgical edit:**

- `crates/semstrait-ir/src/lib.rs` — for each `pub use`:
  - If listed in `35 §1.1` PUBLIC API SURFACE → keep `pub`.
  - If not listed → demote to `pub(crate)` or delete the re-export.
- Confirmed demote candidate: `RegistryExtension` (P-7).
- Possible additional demotes: validate.rs internal helpers, traversal helpers.

**Rationale.** Vision-3 (provider) + spec §1.1 PUBLIC API SURFACE as
authority.

**Blast radius.** `lib.rs` re-export block. Downstream `semstrait-model` is
out of scope (Q2=B); other consumers will bind only to the curated surface.

**Derived clauses:**
- D-P4.1: per-symbol public/internal classification — needs an explicit
  list, not bulk approval.

### Clause P-7 — De-export `RegistryExtension`

**Status:** awaiting direction.

**Recommendation (agent 4):** approve.

**Surgical edit:**

- `crates/semstrait-ir/src/lib.rs:70` — `pub use functions::extension::RegistryExtension;`
  → `pub(crate) use …` or remove the re-export.

**Blast radius.** `lib.rs` one line.

---

## §4 — Polish (Iteration 4: catalog completeness)

### Clause P-2 — Remove dead `time_default` binding

**Status:** awaiting direction (option A/B/C).

**Recommendation (agents 1, 3):** Option **B** — delete.

**Surgical edit (Option B):**

- `crates/semstrait-ir/src/functions/builtins/temporal.rs:11-13` — remove
  the binding lines.

**Rationale.** Verified dead-on-arrival via `git log` (single-commit
introduction; no entry references it).

**Blast radius.** `temporal.rs` 3 lines.

### Clause P-3 — Decimal overloads in math/aggregate catalog

**Status:** awaiting direction (per-function list).

**Recommendation (agent 3):**

- `round`: add `(Decimal(p,s)) -> Decimal(p,s)` (SameAsFirstArg) and
  `(Decimal(p,s), Int) -> Decimal(p,s)`.
- `ceil`: add `(Decimal(p,s)) -> Decimal(p,0)` — **needs `ReturnTypeRule::Custom`
  or new variant**.
- `floor`: same shape as `ceil`.
- `median`: add `(Decimal(p,s)) -> Decimal(p,s)` (SameAsFirstArg).

**Surgical edit:**

- `crates/semstrait-ir/src/functions/builtins/math.rs:21-30, 31-39, 40-48`
  — add Decimal overloads to `round`/`ceil`/`floor` `signatures`.
- `crates/semstrait-ir/src/functions/builtins/aggregate.rs:40-46` — add
  Decimal overload to `median`.
- **For `ceil`/`floor`**: either use existing `ReturnTypeRule::Custom`
  (couples with P-6 footgun footprint) **OR** add new
  `ReturnTypeRule::DecimalScaleZero` variant — this is a **derived
  clause** requiring spec ratification.

**Rationale.** `35 §14a` claims Decimal support for these functions; catalog
currently doesn't deliver.

**Blast radius.** `math.rs`, `aggregate.rs`, `spec.rs` (`ReturnTypeRule`
variant if new), tests verifying signatures, spec §14a alignment audit.

**Derived clauses:**
- D-P3.1: `Custom` vs new `DecimalScaleZero` variant — separate decision.
  **Recommendation:** new variant (couples with P-6 Option A; reduces
  `Custom` proliferation).
- D-P3.2: Float-vs-Double drift between catalog and `35 §14a` — flagged
  INSUFFICIENT EVIDENCE. Treat as separate question; do not edit blindly.

---

## §5 — Polish (Iteration 5: trait shape + hygiene)

### Clause P-1 — Delete derive-driven tests

**Status:** awaiting direction.

**Recommendation (agent 4):** approve. ~85 candidates identified.

**Surgical edit:**

- Delete tests classified as `DERIVE-DRIVEN` per agent (4)'s
  module-by-module classification (full table in `35_ir_review.md` second-
  pass section). Approximate breakdown:
  - `expr/leaves.rs`: ~12 `*_equality_and_clone` tests.
  - `expr/parameter.rs`: ~9 `*_equality_and_hash`, `parameter_key_is_copy`.
  - `expr/accessor.rs`: ~7 `*_equality_and_hash`.
  - `expr_kinds.rs`: ~21 `*_equality_and_hash`, `*_round_trip_via_debug`.
  - `primitives.rs`: 5 `*_hash_is_deterministic`, `*_clone`.
  - `types.rs`: ~4 schema-hash, type-class-clone.
  - `error.rs`: ~6 `*_equality_and_clone`.
  - `plan/node.rs`: 2 `plan_node_clone_preserves_structure`.
  - `plan/meta.rs`: ~4 `node_id_is_copy`, derive tests.
  - Others: ~15.

**Rationale.** Vision-7 + maintenance — tests verifying `derive(Clone)` /
`derive(Hash)` properties duplicate Rust std guarantees. ~30% test corpus
reduction.

**Blast radius.** Test files only. No production code change. Test count
drops from 273 to ~188.

**Derived clauses:**
- D-P1.1: per-test approval — user audits the file:line list before
  deletion.
- D-P1.2: `BOTH`-classified tests stay; classification edge cases need
  case-by-case review.

### Clause P-5 — `Tree::transform` refactor

**Status:** awaiting direction.

**Recommendation (agent 4):** sketch ready; defer until visitor/rewriter
contract is exercised by a real consumer.

**Surgical edit (if approved now):**

- `crates/semstrait-ir/src/expr/tree.rs:128-145` — split
  `Tree::transform` into three helpers (`transform`, `transform_children`,
  `clone_children_transformed`).

**Rationale.** Vision-3. Single-concern helpers for visitor/rewriter
symmetry per `m05-type-driven`.

**Blast radius.** `tree.rs` — added helper methods on the `Tree` trait;
existing `transform` callers unaffected (signature unchanged).

**Derived clauses:**
- D-P5.1: helper method names — `transform_children` /
  `clone_children_transformed` are sketch names; final naming needs
  separate approval.

### Clause P-6 — Drop `ReturnTypeRule: PartialEq`

**Status:** awaiting direction (option A/B).

**Recommendation (agent 3):** Option **A** — drop. **Caller search confirms
zero external comparisons.**

**Surgical edit (Option A):**

- `crates/semstrait-ir/src/functions/spec.rs:70-81` — delete the manual
  `impl PartialEq for ReturnTypeRule` block.
- `crates/semstrait-ir/src/functions/spec.rs` — remove `PartialEq` from
  any `derive` on enums depending on `ReturnTypeRule`'s equality (e.g.
  `FunctionSpec` if it derives via `ReturnTypeRule`).
- `crates/semstrait-ir/src/functions/spec.rs:15` — drop `PartialEq` from
  `FunctionSpec` derive (or remove `return_type` from the derived equality
  via manual impl).

**Rationale.** Reflexivity-breaking `PartialEq` is a footgun. Zero callers
compare `ReturnTypeRule` (verified by Grep across workspace).

**Blast radius.** `spec.rs` only. Tests use `matches!()`, not `==`, so no
test breakage expected.

**Derived clauses:**
- D-P6.1: cascade to `FunctionSpec` — drop entire `PartialEq` derive
  (simpler) or keep `FunctionSpec` partial equality via manual impl that
  excludes `return_type`?

### Clause P-8 — Remove `FunctionSpec.description`

**Status:** awaiting direction (option A/B).

**Recommendation (agent 3):** Option **A** — remove. **Caller search
confirms zero runtime reads.**

**Surgical edit (Option A):**

- `crates/semstrait-ir/src/functions/spec.rs:22` — remove
  `pub description: &'static str` field.
- `crates/semstrait-ir/src/functions/builtins/mod.rs:40, 57` — remove
  `description: &'static str` parameter from `scalar()` and `aggregate()`
  helpers.
- `crates/semstrait-ir/src/functions/builtins/{string,math,temporal,logical,aggregate}.rs`
  — drop 47 description string literals from spec construction.
- `crates/semstrait-ir/src/functions/extension.rs:60`,
  `crates/semstrait-ir/src/functions/spec.rs:245` — update test fixtures.
- `docs/design/foundations/14a_function_catalog.md:65` — **spec edit**:
  remove `description: &'static str` from `FunctionSpec` declaration.

**Rationale.** Vision-1 (toolkit minimalism). Zero callers; `35 §14a` is
the canonical doc home. Removal saves per-spec memory and bootstrap noise.

**Blast radius.** Crate-wide builtin spec construction. Foundations spec
edit. Tests.

**Derived clauses:**
- D-P8.1: foundations spec edit — separate clause approval per workstyle
  rule 4.

---

## §6 — New findings (declaration-only or doc-only)

### Clause N-1 — Substrait Rel coverage declarations

**Status:** awaiting direction.

**Recommendation (agent 5):** declare-only; no code change.

**Surgical edit:**

- `docs/design/apis/35_semstrait_ir.md` §17.1 — add 5 TD entries:
  - `TD-IR-SET-OPS` (Intersect / Except / multiset variants beyond Union).
  - `TD-IR-SORT-CLUSTERED` (direction-free CLUSTERED).
  - `TD-IR-PIVOT` (`expand_rel` / Pivot / Unpivot / GroupingSets).
  - `TD-IR-CTE-REFERENCE` (`reference_rel` / shared sub-plan).
  - `TD-IR-EXPR-LOWERINGS` (Switch → Case-of-Eq, MultiOrList → Or-of-And).

**Rationale.** Vision-6: "cover all plan abstractions — or reserve them
(`#[non_exhaustive]`) — or justify their absence." Spec acknowledgment is
the third arm.

**Blast radius.** Spec only. No code change.

### Clause N-2 — Substrait Expression coverage declarations

Subsumed by N-1's `TD-IR-EXPR-LOWERINGS`.

### Clause N-3 — `PhysicalExpr` "never evaluated" enforcement

**Status:** awaiting direction (option A/B/C).

**Recommendation (agent 1):** **A** if `TestLeaf` and any planner-side stub
leaves are genuine downstream use; **B** if v1 leaf set is closed.

**Surgical edit (Option A):**

- `crates/semstrait-ir/src/expr/mod.rs` — add module-doc:
  ```rust
  //! Vision-5 contract: PhysicalExpr and SemanticExpr express computation;
  //! no evaluation method (eval, evaluate, compute) may be added to this
  //! crate. Consumers requiring evaluation operate on EngineArtifact (`36`).
  ```
- `crates/semstrait-ir/src/lib.rs` — same doc-comment at crate root.
- (Optional) CI grep guard against forbidden method names.

**Surgical edit (Option B — sealed):**

- `crates/semstrait-ir/src/expr/tree.rs` — add `mod sealed { pub trait
  Sealed {} }`; bound `pub trait ExprLeaf: sealed::Sealed + …`.
- Internal `impl sealed::Sealed for PhysicalLeaf {}`,
  `impl sealed::Sealed for SemanticLeaf {}`,
  `impl sealed::Sealed for TestLeaf {}` (in test module only).

**Rationale.** Vision-5.

**Blast radius.** Doc-only (A) — `expr/mod.rs`, `lib.rs`. Sealed (B) —
`tree.rs`, leaf trait users.

### Clause N-4 — Document `NodeMeta.output_schema` Arc invariant

**Status:** awaiting direction.

**Recommendation (agent 2):** approve. Doc-only.

**Surgical edit:**

- `crates/semstrait-ir/src/plan/meta.rs:70-101` — add rustdoc to
  `NodeMeta.output_schema`:
  ```rust
  /// `Arc<Schema>` is value-shared and never-mutated. Replacement requires
  /// constructing a new `NodeMeta`; in-place mutation is not exposed. Phase
  /// B traversal materializes new metas rather than mutating shared ones.
  ```

**Blast radius.** `meta.rs` doc-comment only.

### Clause N-5 — `Capability` adapter read protocol

**Status:** awaiting direction (option a/b).

**Surgical edit:**

- `crates/semstrait-ir/src/artifact.rs:193-201` (or `Dialect::capabilities()`
  rustdoc at L168-170) — add:
  ```rust
  /// Read protocol: when a consumer encounters a `Capability` variant
  /// unknown to its compiled-in roster, it MUST treat the variant as
  /// {present | absent}. Variant additions are MINOR (`30 §2.2`).
  ```

**Direction needed:** present-when-unknown (a) vs absent-when-unknown (b).
**Multiple interpretations exist; no silent pick.**

**Blast radius.** `artifact.rs` doc-comment only.

---

## §7 — Apply order (after approval)

When clauses are approved, apply in this order to minimize churn:

1. **B-1, B-2, P-9** — aggregate kernel reshape; large blast radius.
2. **S-2** — typed literals; foundational.
3. **S-3a, S-4, S-5, S-6** — validate-pass extensions (S-3b reframed; no
   code).
4. **S-1, P-2, P-7, P-8, P-6** — small mechanical removals.
5. **P-3** — Decimal overloads (depends on `ReturnTypeRule` decision —
   could fold into P-6 in this batch).
6. **P-4** — `lib.rs` re-export curation.
7. **P-1** — delete derive-driven tests (after all behavior changes land,
   so test corpus reflects final shape).
8. **N-1, N-2, N-4, N-5** — doc / spec entries.
9. **N-3, P-5** — last (or defer); independent of above.

**S-7, S-8** require no code change in this pass (recommendations C and B
respectively).

---

## §8 — README sync trigger

Per CLAUDE.md "Documentation Update Rule":

- Every clause that changes types or abstractions in `semstrait-ir`
  triggers a `crates/semstrait-ir/README.md` update.
- Cross-cutting changes (e.g., B-1 `AggregateKind`, S-2 `Literal` typing,
  P-8 `description` removal) also touch foundations specs (`14a`,
  `35 §11.7`, `35 §13.5`, etc.).

These are **not** separate clauses; they ride the corresponding code
change. But each foundations-spec edit is its own approval per workstyle
rule 4.

---

## §9 — Workstyle gate (final reminder)

**No edit listed in this file is authorized.** Each clause needs explicit
direction (option letter or "yes to shape") from the user. A directional
pick on a parent clause (e.g., B-1 = C) does **NOT** authorize the derived
clauses (D-B1.1, D-B1.2, D-B1.3) — each derived clause is its own decision.

After approval, apply edits in the order from §7. After each iteration
applied, run multi-agent review per CLAUDE.md "Code Review" section before
declaring the iteration complete.
