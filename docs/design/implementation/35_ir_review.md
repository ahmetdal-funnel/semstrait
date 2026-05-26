---
title: IR Implementation Review — Findings & Iteration Tracker
status: in-review
review_target: crates/semstrait-ir (worktree feature+ir-impl)
spec: docs/design/apis/35_semstrait_ir.md
---

# IR Implementation Review — Findings & Iteration Tracker

Review of `crates/semstrait-ir` rework against `docs/design/apis/35_semstrait_ir.md`,
synthesized from multi-agent code review (Rust quality / software design /
data-engineering perspectives), deduplicated, and verified by spot-reading source.

19 findings grouped into 5 iteration buckets by **coupling**, not severity. We
work one iteration at a time so decisions in earlier groups can inform the
later ones.

## Conventions

Severity:

- **B** = Block — gates Phase B planning; resolve before merge.
- **S** = Should-fix — meaningful correctness/design issue; fix before broader rollout.
- **P** = Polish — hygiene/clarity; deferrable.

Each item carries four iteration fields:

- **Status** — `pending` / `decided` / `applied` / `dropped` / `superseded`
- **Research needed** — what we need to verify before committing
- **Decision** — the option taken, recorded after ratification
- **Notes** — anything surfaced during discussion (counter-options, follow-ups, links)

A directional pick (option letter, "yes to shape") is **not** authorization for
derived implementation clauses (validation rules, error codes, edge cases).
Each derived clause is its own decision per CLAUDE.md workstyle rule 4.

---

## Iteration 1 — Aggregate kernel architecture

Cluster: **B-1, B-2, P-9**. Gates Phase B planning. Items here couple tightly:
the choice on B-1 determines the shape of B-2, which folds P-9 in.

### B-1 — `AggregationOp` closed-five locks out 8 registry aggregates

**Severity:** Block

**Finding.** `crates/semstrait-ir/src/expr_kinds.rs:60` defines:

```rust
#[non_exhaustive]
pub enum AggregationOp {
    Sum, Avg, Count, Min, Max,
}
```

But `crates/semstrait-ir/src/functions/builtins/aggregate.rs` registers 8
*additional* aggregates: `stddev`, `stddev_pop`, `variance`, `var_pop`,
`median`, `string_agg`, `percentile_cont`, `approx_count_distinct`.

Because `Expr::Aggregate { op: AggregationOp, … }` and `AggregateExpr.aggregation:
AggregationOp` are typed against this closed enum, the IR cannot represent any
of those 8 — the v1 catalog claims 13 aggregates but the IR carries 5.

The moment the planner lifts any non-five aggregate during Phase B planning,
it has nowhere to put the operator name. This is not a polish issue; it is a
structural mismatch between the catalog and the carrier.

**Options.**

- **A** — Extend `AggregationOp` to all 13 v1 aggregates. Stays closed-enum;
  adapters get exhaustiveness checking. Cost: every new aggregate requires
  a MINOR enum addition (acceptable per `30 §2.2` `#[non_exhaustive]`).
- **B** — Drop `AggregationOp`. Use `Expr::Aggregate { fn_ref: CanonicalFn, … }`
  string-keyed, same shape as scalar `FnApply`. Catalog is the single source
  of truth; no enum-vs-catalog sync to maintain.
- **C** — Keep `AggregationOp` for the v1-canonical five (Sum/Avg/Count/Min/Max);
  add a wrapper `AggregateKind { Builtin(AggregationOp), Extension(CanonicalFn) }`
  so the canonical five get exhaustive matching while the rest flow as
  string-keyed extensions.

**Recommendation.** **B**. The catalog is already the source of truth —
`FunctionSpec` carries `Additivity`, `ReturnTypeRule`, signature; mirroring
that into a closed enum forces dual maintenance and gains nothing the
catalog doesn't already give us. Adapters dispatch on `CanonicalFn` for
scalars; aggregates can use the same machinery.

**Status:** pending
**Research needed:** confirm whether any adapter codepath exhaustively matches
on `AggregationOp` today and would lose checking under B; check whether `15`
or `16` calls out the closed-five as a normative invariant.
**Decision:**
**Notes:**

### B-2 — `AggregateExpr.input_expr: PhysicalExpr` is too narrow for 2-ary aggregates

**Severity:** Block (coupled to B-1)

**Finding.** `crates/semstrait-ir/src/primitives.rs:214` declares:

```rust
pub struct AggregateExpr {
    pub aggregation: crate::expr_kinds::AggregationOp,
    pub input_expr: PhysicalExpr,                  // single
    pub distinct: bool,
    pub filter: Option<PhysicalExpr>,
    pub inferred_type: DataType,
}
```

The doc-comment cites Q-IR-NEW-003 ("every v1 canonical aggregate is 1-ary").
That holds for the closed five. It does **not** hold for the registered
aggregates: `string_agg(expr, delimiter)` and `percentile_cont(expr, fraction)`
are 2-ary. With B-1 unresolved, these can't even be expressed; with B-1
resolved (any option), the 2-ary aggregates need somewhere for the second arg
to live.

**Options.**

- **Implied by B-1.A** — Add `args_tail: Vec<PhysicalExpr>` (default empty;
  MINOR per `30 §2.2`). Single-arg lift contract preserved for the canonical
  five.
- **Implied by B-1.B** — Replace `input_expr` with `args: Vec<PhysicalExpr>`.
  Symmetrical with `Expr::FnApply`; lift contract becomes "lift all of
  `Expr::Aggregate.args`" instead of "lift the singleton."
- **Implied by B-1.C** — `AggregateKind::Builtin` keeps single `input_expr`;
  `AggregateKind::Extension` carries `args: Vec<PhysicalExpr>`. Asymmetric
  but explicit.

**Recommendation.** **`args: Vec<PhysicalExpr>`** (the natural pairing with
B-1.B). Lift contract simplifies; the doc-comment about "single PhysicalExpr"
becomes "all args lifted in order."

**Status:** pending (decision implied by B-1 outcome)
**Research needed:** what `19 §7` says about lift contract — does it normatively
mandate single-arg, or is that a Q-IR-NEW-003 simplification we can revisit?
**Decision:**
**Notes:**

### P-9 — Aggregate kernel cohesion

**Severity:** Polish (folds into B-1/B-2 outcome)

**Finding.** With `AggregationOp`, `AggregateExpr`, `Expr::Aggregate`, and
`FunctionRegistry` aggregate entries all carrying overlapping aggregate
metadata (op identity, additivity, return-type rule), there's no single owner.
After B-1/B-2 land, audit what's left and consolidate so each aggregate fact
has exactly one home.

**Status:** pending (revisit after B-1/B-2 decided)
**Research needed:** post-B-1/B-2 inventory of where aggregate metadata lives.
**Decision:**
**Notes:**

---

## Iteration 2 — Leaf typing + validate completeness

Cluster: **S-2, S-3, S-4, S-5, S-6**. S-3 depends on S-2 (type-match needs
typed literals). S-4/S-5/S-6 are independent validate-pass holes.

### S-2 — `Literal` carries no inferred type

**Severity:** Should-fix

**Finding.** `crates/semstrait-ir/src/expr_kinds.rs` `Literal` variants are
value-only: `Literal::Decimal(rust_decimal::Decimal)`, `Literal::Int64(i64)`,
etc. Validators and downstream lift logic cannot determine the literal's
schema-side `DataType` without re-running inference. The IR claims "no
re-inference downstream" (Q-IR-NEW-003) but `Literal` forces it for any
type-matching pass.

**Recommendation.** Approve. Carry an `inferred_type: DataType` field on
literals where the value alone doesn't determine it (notably `Decimal`,
`Time`, `Timestamp` precision). The exact carrier shape is a derived clause
to ratify once direction is approved.

**Status:** pending
**Research needed:** for each `Literal` variant, list the cases where the
Rust value alone *doesn't* fully determine the canonical `DataType`
(precision, scale, time-unit). Decide carrier shape: per-variant fields,
sidecar `(Literal, DataType)` tuple, or new `TypedLiteral` struct.
**Decision:**
**Notes:**

### S-3 — Join key resolution + type-match validation

**Severity:** Should-fix (depends on S-2)

**Finding.** `JoinNode.on: Vec<KeyPair>` carries `Name` for each side.
`crates/semstrait-ir/src/plan/validate.rs` does **not** verify that:

1. Each `KeyPair.left` resolves to a column in the left child's schema,
   and `right` in the right child's schema.
2. The two columns' `DataType`s match (or are coercion-compatible per a
   defined rule).

(1) is structural and can be enforced today. (2) requires typed literals
(S-2) before it can be done generally — but for column refs only, types
are already in the child schemas, so the simpler "column→column type-match"
can land independently.

**Recommendation.** Two-step approval:

- **a)** Resolution check (both names exist in respective child schemas) —
  approve now.
- **b)** Type-match check — approve in shape; concrete coercion rules are a
  derived clause to land once S-2 is decided.

**Status:** pending
**Research needed:** what `16 §5.1` says about coercion at join keys — exact
match required, or some allowed widening?
**Decision:**
**Notes:**

### S-4 — `ValuesNode` row arity not validated

**Severity:** Should-fix

**Finding.** `ValuesNode { schema: Schema, rows: Vec<Vec<Literal>> }`
(Q-IR-NEW-002). Validator does not enforce `rows[i].len() == schema.fields.len()`
for all `i`. A malformed `ValuesNode` flows past structural validate.

**Recommendation.** Approve. Add an arity check to `validate.rs`. With S-2 in
flight, also add a per-cell type-match against `schema.fields[j].data_type`
once typed literals land.

**Status:** pending
**Research needed:** none (mechanical addition to validate.rs).
**Decision:**
**Notes:**

### S-5 — `AggNode` output-name uniqueness not validated

**Severity:** Should-fix

**Finding.** `AggNode { keys, aggregates }` produces output columns from
`keys` (carried as `Name`s) and `aggregates` (also producing `Name`s).
Validator does not reject collisions like `keys = ["region"]` and an
aggregate aliased to `"region"`.

**Recommendation.** Approve. Add uniqueness check across the union of key
names and aggregate output names.

**Status:** pending
**Research needed:** confirm the aggregate's output name field on
`AggNode.aggregates[i]` (which struct field holds the alias).
**Decision:**
**Notes:**

### S-6 — Outer-join nullability widening

**Severity:** Should-fix

**Finding.** Outer joins (`JoinType::Left`, `Right`, `Full`) widen the
nullability of the unmatched side's columns in the output schema. Today the
plan tree doesn't make this explicit — either the planner sets it correctly
upstream, or downstream consumers must re-derive.

**Options.**

- **a)** Auto-derive — `JoinNode`'s output schema is computed from
  child schemas + `JoinType` inside the IR (validator consumes the derivation,
  consumers read derived schema).
- **b)** Document expectation — planner is responsible for widening before
  building `JoinNode`; validator checks consistency (output schema matches
  the widened expectation given children + join type).

**Recommendation.** **(a)** if the IR exposes a uniform `node.output_schema()`
accessor; **(b)** if schema is set explicitly by the planner. Need to inspect
how `PlanNode` exposes schemas today before committing.

**Status:** pending
**Research needed:** does `PlanNode` carry an explicit `output_schema` field
per node, or is it derived on demand? Match the chosen pattern.
**Decision:**
**Notes:**

---

## Iteration 3 — Error layering + DSL + surface

Cluster: **S-1, S-7, S-8, P-4, P-7**. Mostly design-choice items about how
the IR talks to its callers. Independent of iterations 1–2.

### S-1 — `SemanticExprAccessorExt` uses `debug_assert!(false)` then silent passthrough

**Severity:** Should-fix

**Finding.** `crates/semstrait-ir/src/expr/expr_fn.rs:350-600` defines
8 accessor methods (`first`, `last`, `previous`, `next`, `delta`,
`percent_change`, `lag`, `lead`) that all follow this pattern:

```rust
debug_assert!(false, "<...>");
self  // silent passthrough in release
```

This is the worst of both worlds: panics in tests, silently corrupts in
production. Either the operation is unimplemented (should panic OR return
`Result`) or it's a no-op (should be documented as such, no `debug_assert!`).

**Options.**

- **A** — Convert to `Result<SemanticExpr, IrErrorKind>` returning a
  not-implemented error variant. Caller decides.
- **B** — Document as documented-no-op (remove `debug_assert!`, keep
  passthrough, say "v1 stub; semantics TBD"). Same runtime behavior, honest
  signal.
- **C** — Remove the methods entirely if no caller depends on them; reintroduce
  with real semantics when needed.

**Recommendation.** **B** if these methods have callers that expect them to
exist syntactically (DSL surface). **C** otherwise. **A** only if there's
already a `Result`-returning surface in this trait — introducing one for
8 methods to flag a stub is more friction than warning.

**Status:** pending
**Research needed:** are these methods called anywhere in the planner/model
code today, or only in tests? If only in tests, **C** is the cleanest.
**Decision:**
**Notes:**

### S-7 — Registry bootstrap uses `assert!` macros

**Severity:** Should-fix

**Finding.** `crates/semstrait-ir/src/functions/registry.rs:50-63` uses
`assert!()` for bootstrap invariants. These fire during static initialization,
panic the process, and don't surface to a structured error channel. Bootstrap
errors are recoverable in principle (skip bad entry, return `Result`) but
not under `assert!`.

**Options.**

- **A** — Convert to `Result<FunctionRegistry, BootstrapError>` propagated
  to the call site. Caller decides whether to panic.
- **B** — Keep `assert!` but document "bootstrap failure = compile-time bug,
  not runtime concern." Acceptable if all bootstrap data is hard-coded.
- **C** — Bundle with TD-REGISTRY-EXTENSION-WIRING — when the extension
  wiring lands (P-7), the bootstrap path becomes pluggable; that's the
  natural moment to switch to `Result`.

**Recommendation.** **C**. Today's `assert!` covers hard-coded invariants
only; switching to `Result` now means designing an error type that's
underspecified until extensions can fail too. Bundle into TD-REGISTRY-EXTENSION-WIRING.

**Status:** pending
**Research needed:** is TD-REGISTRY-EXTENSION-WIRING already a tracked debt,
or do we file it now?
**Decision:**
**Notes:**

### S-8 — `ValidateError::StructuralViolation` payload thinness

**Severity:** Should-fix

**Finding.** `ValidateError::StructuralViolation` (the catch-all for
plan-shape failures) likely carries a string-only payload. Adapters and
diagnostics surfaces want structured info — node id, field, expected vs
actual.

**Options.**

- **A** — Enrich with structured fields: `{ node_id: NodeId, field: &'static str,
  reason: StructuralReason }` where `StructuralReason` is a closed enum.
- **B** — Keep clean, single-string payload; rely on rich `Diagnostic<K>`
  envelope around it for context.

**Recommendation.** **B**. The IR's job is to flag the violation; structured
diagnostic context (file path, span, surrounding code) belongs in the
`Diagnostic<K>` envelope built upstream by the planner. Adding parallel
structure inside `ValidateError` duplicates the diagnostic responsibility.

**Status:** pending
**Research needed:** read current `Diagnostic<K>` shape to confirm it carries
the structured fields B relies on.
**Decision:**
**Notes:**

### P-4 — `lib.rs` re-exports

**Severity:** Polish

**Finding.** `crates/semstrait-ir/src/lib.rs` exports a flat namespace mixing
public surface (`SemanticPlan`, `Expr`, `DataType`) with internal-but-not-quite
items (registry internals, validate-internal helpers).

**Recommendation.** Approve A+B split:

- **A)** Curate a `pub use` block listing the *public* surface (the names
  consumers should use).
- **B)** Move anything not in that list behind `pub(crate)` or `mod`-private.

**Status:** pending
**Research needed:** enumerate current top-level `pub use`s; flag each as
"public" or "internal."
**Decision:**
**Notes:**

### P-7 — De-export `RegistryExtension` until wired

**Severity:** Polish

**Finding.** `crates/semstrait-ir/src/functions/extension.rs` defines
`RegistryExtension`, marked TD-REGISTRY-EXTENSION-WIRING (not yet wired).
It is currently re-exported. Consumers can name a trait that does nothing.

**Recommendation.** Approve. Hide behind `pub(crate)` (or remove from re-exports)
until wiring lands.

**Status:** pending
**Research needed:** none.
**Decision:**
**Notes:**

---

## Iteration 4 — Function catalog completeness

Cluster: **P-2, P-3**. Catalog hygiene; validates against `35 §14a` /
`19 §7`. Independent of all other iterations.

### P-2 — Dead binding in temporal builtins

**Severity:** Polish

**Finding.** `crates/semstrait-ir/src/functions/builtins/temporal.rs:12-13`:

```rust
let time_default = DataType::Time { precision: 6 };
let _ = time_default;
```

The binding is constructed and immediately discarded. Either it was meant
to be used (and a function entry is missing/incomplete), or it's leftover.

**Options.**

- **A** — Use it. Identify which entry was supposed to reference it;
  complete that entry.
- **B** — Delete the binding.
- **C** — Something else (defer, comment, …).

**Recommendation.** **B**. The binding has no comment explaining intent; the
14 temporal entries above and below don't reference any `time_default`. Most
likely a leftover from refactoring.

**Status:** pending
**Research needed:** git blame the lines to confirm — was this added with
an entry that got deleted, or is it dead-on-arrival?
**Decision:**
**Notes:**

### P-3 — Decimal overloads missing in math/aggregate catalog

**Severity:** Polish

**Finding.** Spot-checking against `35 §14a` and `19 §7`: math functions
(`abs`, `round`, `floor`, `ceil`, …) and aggregates (`sum`, `avg`) advertise
support for decimal, but the registered overloads don't always cover decimal
inputs. Spec says supported; catalog says no overload exists.

**Recommendation.** Approve. Audit each catalog file (`math.rs`, `aggregate.rs`)
against `35 §14a` and add missing decimal overloads.

**Status:** pending
**Research needed:** complete cross-reference table of `§14a` claimed support
vs catalog reality. Determines exact list of overloads to add.
**Decision:**
**Notes:**

---

## Iteration 5 — Trait shape + hygiene

Cluster: **P-1, P-5, P-6, P-8**. Independent polish items; can fan out in
parallel once decided.

### P-1 — Tests verifying Rust language semantics, not IR invariants

**Severity:** Polish

**Finding.** Some tests in the `semstrait-ir` crate assert things like
"`Clone` produces equal values" or "`Ord` orders consistently" — properties
guaranteed by the derive macros, not by IR design. They burn test budget
without testing IR-specific behavior.

**Recommendation.** Approve. Delete tests that verify only what `derive(Clone)`
/ `derive(Ord)` guarantees; keep tests that exercise IR invariants
(round-trip serde, hash-consistency for `Name`-as-`HashMap`-key, etc.).

**Status:** pending
**Research needed:** enumerate the tests in question (concrete file:line list)
before deletion so the user can audit.
**Decision:**
**Notes:**

### P-5 — `Tree::transform` refactor

**Severity:** Polish

**Finding.** `Tree::transform` (the rewriter dispatch on the universal `Tree`
trait) bundles concerns that could be split: matching, child-transform,
node-rebuild. Refactoring would clarify visitor/rewriter symmetry.

**Recommendation.** Approve in principle; concrete shape is a derived clause
that needs the refactor sketch on the table first.

**Status:** pending
**Research needed:** sketch the refactored shape — what does the new method
signature look like, what helpers shake out?
**Decision:**
**Notes:**

### P-6 — `ReturnTypeRule::Custom` breaks `PartialEq` reflexivity

**Severity:** Polish

**Finding.** `crates/semstrait-ir/src/functions/spec.rs` manual `PartialEq`
for `ReturnTypeRule` returns `false` for `(Custom, Custom)` — even when
both refer to the same closure. This breaks `a == a` for `Custom`-bearing
specs.

**Options.**

- **A** — Drop `PartialEq` on `ReturnTypeRule` entirely. Force callers to
  compare on the discriminant (the variant-without-Custom subset *can*
  derive `PartialEq`; `Custom` participates via discriminant equality only,
  not closure equality).
- **B** — Document the quirk: "`PartialEq` is structural for non-`Custom`
  variants; `Custom == Custom` is `false`."

**Recommendation.** **A**. Reflexivity-breaking `PartialEq` is a footgun. If
specs need to be compared, expose a method that does what callers actually
want (likely "do these two specs name the same canonical fn"); drop the
trait-derived equality.

**Status:** pending
**Research needed:** does anything *actually* compare `ReturnTypeRule` values
today? If not, drop is trivial.
**Decision:**
**Notes:**

### P-8 — `FunctionSpec.description` field

**Severity:** Polish

**Finding.** `FunctionSpec` carries a `description` field — human-readable
help text. It's not used at runtime (no caller reads it for behavior), but
it costs memory per spec and adds noise to bootstrap declarations.

**Options.**

- **A** — Remove. Documentation lives in spec files (`docs/design/apis/`,
  `35 §14a`); the registry doesn't need a copy.
- **B** — Keep. Useful for debug tooling (registry dumps, CLI inspection).

**Recommendation.** **A**. The catalog should hold what *runtime* needs;
descriptions belong in spec docs and rustdoc on the bootstrap functions.

**Status:** pending
**Research needed:** check whether any tooling (CLI, error messages) reads
`description`. If yes, weigh removal cost.
**Decision:**
**Notes:**

---

## Cross-cutting follow-ups

Items that emerge once the above are ratified:

- **README sync** — every clause that changes types/abstractions in
  `semstrait-ir` triggers a `crates/semstrait-ir/README.md` update per
  CLAUDE.md "Documentation Update Rule."
- **Spec sync** — anything that contradicts `35_semstrait_ir.md` either
  reshapes the spec (and lands as a spec PR first) or gets reworked to
  match. Per workstyle: spec is source of truth in spec-driven-dev mode.
- **Multi-agent code review** — at the end of each iteration, run the
  3-perspective review (Rust / software-design / data-engineering) on the
  diff before declaring the iteration applied.

---

## Iteration ordering rationale

1. **Iteration 1 first** — block-severity, gates Phase B; B-1's choice
   ripples into B-2 and P-9.
2. **Iteration 2 next** — S-2 (typed literals) is a foundational data-shape
   change; later validate-pass items (S-3.b) depend on it.
3. **Iteration 3** — design-choice cluster, can run in parallel with 4 if
   review bandwidth allows.
4. **Iteration 4** — catalog completeness, mostly mechanical once §14a
   cross-reference is built.
5. **Iteration 5** — independent polish; fan out once 1–4 settle.

---

## Second-pass review — agent synthesis (2026-05-26)

Five parallel review agents audited the worktree against current code and the
seven-principle vision (see `35_ir_review_plan.md` §1). Each agent's
verifications, recommendations, and new findings are reconciled below. Items
not reconciled here remain at their iteration-tracker status; agent output
augments the **Status / Notes** fields, not the **Decision** field — decisions
remain user-only per workstyle rule 3.

### Reconciled verifications

| ID | Pre-review | Post-review | Citation | Adjustment |
| --- | --- | --- | --- | --- |
| B-1 | pending | **VERIFIED** | `expr_kinds.rs:60-66` (closed-five) vs `functions/builtins/aggregate.rs` 8 entries | No exhaustive `match AggregationOp` exists workspace-wide; `#[non_exhaustive]` already prohibits external exhaustive matching. **Normative cite:** `35 §11.7:1370` and `14a §3.6:263` ratify closed-five as carrier rule. |
| B-2 | pending | **VERIFIED** | `primitives.rs:213-220` single `input_expr` | `string_agg` and `percentile_cont` are 2-ary in catalog (`aggregate.rs:48-60`); spec §11.7 normatively types `input_expr: PhysicalExpr` (singular) on premise of closed-five. |
| P-9 | pending | **VERIFIED** with structural finding | `expr_kinds.rs:60`, `spec.rs`, `expr/tree.rs:147-152`, `primitives.rs:213-220` | Closed-five `Additivity` and `ReturnTypeRule` are spec-only — NOT stored in IR. Phase B Strategy hard-codes them. Catalog 8 carry both via `FunctionSpec`. Duplication of `distinct`/`filter` between `Expr::Aggregate` and `AggregateExpr` is intentional (lift contract). |
| S-1 | pending | **VERIFIED + caller search complete** | `expr/expr_fn.rs:386, 403, 420, 437, 454, 471-474, 499, 524` (8 sites) | Workspace caller search returns **zero non-test, non-doc callers** for any of the 8 methods (`first`, `last`, `previous`, `next`, `delta`, `percent_change`, `lag`, `lead`). Authors construct accessors via `Dimension { name, accessor: Some(...) }` literal. |
| S-2 | pending | **PARTIAL** — value-vs-type ambiguity scoped | `expr_kinds.rs:148-172` | `Decimal { precision, scale }`, `Time { precision }`, `Timestamp { precision }` already self-describe. Only `Null`, `Integer(i64)`, `Float(f64)` lack `data_type` recovery. Scope is narrower than prior text suggested. |
| S-3 | pending | **SPLIT (3a CLOSED, 3b reframed)** | `validate.rs:148-174` already raises `DanglingReference` per side | 3a is implemented today. 3b: `16 §5.1` and `35 §13.5` say type-match is enforced **at compile** (Phase A semantic→physical lowering); IR-validate carries `JoinKeyTypeMismatch` only as a residual-trust diagnostic (`§16.3`). Tracker entry needs split. |
| S-4 | pending | **CLARIFIED + GAP confirmed** | `node.rs:438-442` `rows: Vec<Vec<PhysicalExpr>>` (NOT `Vec<Vec<Literal>>`) | Prior tracker text wrong type. `validate.rs:108` short-circuits Values without arity loop. Spec §10.9 mandates row arity = schema length. |
| S-5 | pending | **VERIFIED** — one-line fix | `validate.rs:130-145` HashSet only seeded from `aggregates`; `keys` not folded in | Spec §13.6 requires output names unique across `keys ∪ aggregates`. Trivial extension; new `IrErrorKind::DuplicateAggOutputName` already in §16.3 vocabulary. |
| S-6 | pending | **VERIFIED — option (b) selected by structure** | `meta.rs:70-101` `output_schema: Arc<Schema>` is planner-set; `validate.rs:227-245` `check_pass_through` runs only for Filter/Sort/Fetch | Today's structure fits option (b): planner sets `output_schema`, validator widens-then-equality-checks. New `IrErrorKind::JoinNullabilityMismatch` per §16.3 vocabulary. |
| S-7 | pending | **VERIFIED** | `registry.rs:50-63` three asserts: empty signatures, reserved AST tag collision, duplicate canonical name | `Result`-migration depends on `RegistryExtension` shape (TD). Today's data is hard-coded. |
| S-8 | pending | **VERIFIED** + envelope evidence | `error.rs:172-176` `StructuralViolation { kind: &'static str, reason: String }`. `Diagnostic<K>` (`semstrait-common/src/diagnostic.rs:65-134`) carries `kind/severity/Location(SourceId+Span)/notes` — **NO** node-id/field-name/expected/actual | Q-PLAN-14 (2026-05-25) explicitly scoped 11 of 14 §16.3 variants OUT of IR (upstream-of-IR concerns). Current thin payload is intentional. |
| P-2 | pending | **VERIFIED** — dead-on-arrival | `temporal.rs:11-13`; git-log shows file appears in single commit (`59e4600`) — was not orphaned by deletion | Trivial removal. |
| P-3 | pending | **PARTIAL** — gap table built | math/aggregate cross-reference vs `35 §14a` | **Decimal gaps:** `round`, `ceil`, `floor`, `median`. **Important constraint:** `ceil`/`floor` need scale-changing return type (`Decimal(p,s) → Decimal(p,0)`); current `ReturnTypeRule` lacks this — needs `Custom` or new variant. **Float-vs-Double drift** observed (catalog has both, spec writes `Float`); marked INSUFFICIENT EVIDENCE — separate question. |
| P-4 | pending | **PARTIAL** — surface mostly aligned | `lib.rs` 9 `pub mod` + ~80 `pub use` largely match `35 §1.1` / §20 | Demote candidates: `RegistryExtension` (P-7), possibly internal helpers in `plan` submodule (needs second pass). |
| P-5 | pending | **VERIFIED** — refactor sketch ready | `tree.rs:128-145` 3-line transform bundles clone+rebuild+callback | Helper-method split sketched; non-trivial change for derived clauses (helper signatures, naming). |
| P-6 | pending | **VERIFIED** + caller search complete | `spec.rs:70-81`, `(Custom, Custom) == false` at L77 | **Zero external `ReturnTypeRule` comparisons** workspace-wide. Internal hits in `spec.rs` are inside the manual impl itself. Tests use `matches!()`, not `==`. **Caveat:** `FunctionSpec` derives `PartialEq` (L15) — dropping `ReturnTypeRule: PartialEq` cascades. Tests don't compare full `FunctionSpec`, so cascade is safe. |
| P-7 | pending | **VERIFIED** | `lib.rs:70` re-exports `RegistryExtension` despite TD-REGISTRY-EXTENSION-WIRING | Single-line `pub` → `pub(crate)`. |
| P-8 | pending | **VERIFIED** + caller search complete | `spec.rs:22` `description: &'static str`; **zero runtime reads** workspace-wide; 47 struct-literal write sites | Removal cascade: field on `FunctionSpec`, `description` parameter on `scalar()` / `aggregate()` helpers (`builtins/mod.rs:40, 57`), 47 description literals in 5 builtin files, fixtures in `extension.rs:60` and `spec.rs:245`. **Foundations sync:** `14a_function_catalog.md:65` declares this field — spec edit required. |
| P-1 | pending | **VERIFIED** — 273 tests classified | enumerated module-by-module | ~85 DERIVE-DRIVEN tests (verify `derive(Clone)` / `derive(Hash)` / `derive(Eq)` semantics) recommended for deletion; ~155 IR-INVARIANT keep; ~33 BOTH keep. |

### New findings (added to tracker scope)

#### N-2 — Substrait expression coverage (Iter-3 / vision-6)

**Severity:** Polish (declaration-only; no code changes for v1)

**Finding.** `Expr<L>` 14 variants vs Substrait `Expression` proto:
- COVERED: Literal, FieldReference, ScalarFunction, WindowFunction, IfThen
  (via Case), SingularOrList (via InList), Cast, Aggregate.
- INTENTIONALLY ABSENT (declared in spec): Subquery family (`14 §11`),
  Nested struct/list/map (`35 §4.1` / `13 §2.5`), Enum.
- GAP — not declared: SwitchExpression, MultiOrList. Both are losslessly
  lowerable to `Case` / repeated `InList`-of-Or; recommend declaring as
  TD-IR-EXPR-LOWERINGS in `35 §17.1`. Author + Phase A pre-lowers.

**Recommendation.** **Declare** (no code change). One-line spec entries.

**Status:** pending
**Notes:** Vision-6 says "cover all plan abstractions — or reserve them … or
justify their absence." Declaration is the third arm.

#### N-3 — `PhysicalExpr` "never evaluated" enforcement (Iter-3 / vision-5)

**Severity:** Polish

**Finding.** `ExprLeaf` (`tree.rs:142-146`) is **not sealed**. External
crates can implement `ExprLeaf` for custom leaves and reuse `Expr<MyLeaf>`,
`Tree::transform`, etc. Today's vision-5 protection is "no `evaluate` method
exists in the trait surface, none on `Expr<L>`, none on any leaf" — robust
**only as long as** no future iteration adds one.

**Options.**
- **A — Doc-only.** Add a single-line module-doc to `expr/mod.rs` and
  `lib.rs` stating the contract; optionally add CI grep guard against
  forbidden method names (`eval`, `evaluate`, `compute`).
- **B — Seal `ExprLeaf`.** Private super-trait via `mod sealed`. Prevents
  external implementers; preserves crate-internal `PhysicalLeaf` /
  `SemanticLeaf` and the test-only `TestLeaf` (`tree.rs:509`) still compile.
- **C — Status quo.** Rely on rustdoc only.

**Recommendation.** **A** if `TestLeaf` and any planner-side stub leaves are
genuine downstream use; **B** if v1 leaf set is truly closed.

**Status:** pending
**Notes:** Multiple interpretations — present, do not pick silently.

#### N-4 — `NodeMeta.output_schema: Arc<Schema>` sharing invariant (Iter-2)

**Severity:** Polish

**Finding.** `meta.rs:70-101` declares `output_schema: Arc<Schema>` and
provides no in-place mutator. Replacement requires constructing a new
`NodeMeta`. Phase B traversal materializes new metas rather than mutating
shared ones. Sharing is therefore "value-shared, never-mutated" — but this
invariant is implicit; rustdoc does not state it.

**Recommendation.** Document the invariant in `meta.rs` rustdoc. No structural
change.

**Status:** pending

#### N-5 — `Capability` adapter read protocol (Iter-3)

**Severity:** Polish

**Finding.** `artifact.rs:193-201` `Capability` is `#[non_exhaustive]` with 5
v1 variants. `Dialect::capabilities()` rustdoc (L168-170) says "readers
SHOULD NOT pattern-match exhaustively; the set is additive" — directional
but doesn't tell consumers what to DO with an unknown variant.

**Options.**
- **a)** Treat unknown as **present/supported** — additive read posture; if
  engine advertises a newer cap the planner doesn't know about, planner
  doesn't gate.
- **b)** Treat unknown as **absent/unsupported** — conservative; planner
  refuses to emit anything requiring the unknown cap.

**Recommendation.** Direction-bearing question; vision-6 leans toward (a)
for additive evolution but the answer depends on whether `Capability`
advertises requirements vs gates. **Multiple interpretations — present, do
not pick silently.**

**Status:** pending

#### N-1 — Substrait Rel coverage (Iter-3 / vision-6)

**Severity:** Polish (declaration-only)

**Finding.** Substrait `Rel oneof` vs `PlanNode` 9 variants:
- COVERED: read, filter, fetch (with offset), aggregate, sort, join (equi),
  project.
- COVERED — intentionally absent (`35 §1.4`): exchange, partitioning,
  nested_loop_join, merge_join, hash_join (physical concerns); update / DDL
  / mutation ops (read-only IR scope).
- RESERVED via `#[non_exhaustive]` and declared in `35 §17.1`: cross_rel,
  Window-PlanNode, Distinct, Unnest, TopN; LEFT_SEMI / RIGHT_SEMI /
  LEFT_ANTI / RIGHT_ANTI on JoinType; extension_*_rel (delegated to
  `FunctionRegistry` per `35 §1.6 R4`).
- **GAP — not declared, not reserved by name in spec:**
  - **N-1.1** SetOp variants beyond Union (Intersect / Except / multiset
    variants). `UnionNode { distinct: bool }` covers two Substrait variants
    only; remaining six are not declared deferred.
  - **N-1.2** Sort `CLUSTERED` direction-free clustering. `SortDir` always
    pins Asc/Desc; CLUSTERED is unrepresentable.
  - **N-1.3** `expand_rel` / Pivot / Unpivot / GroupingSets.
  - **N-1.4** `reference_rel` / CTE / shared sub-plan. Trees are strictly
    owning (`Box<PlanNode>`).
  - **N-1.5** see N-2 for the expression-level lowerings.

**Recommendation.** **Declare** all 5 in `35 §17.1` as TD entries
(TD-IR-SET-OPS, TD-IR-SORT-CLUSTERED, TD-IR-PIVOT, TD-IR-CTE-REFERENCE,
TD-IR-EXPR-LOWERINGS). No code change. `PlanNode` is `#[non_exhaustive]`,
admitting future MINOR additions; the spec just needs to acknowledge them so
vision-6 ("OR justify their absence") is met.

**Note on N-1.1.** If the future shape is `SetNode { op: SetOp, distinct,
inputs }`, that is a MAJOR rename of the `Union` variant — declaration is
the only correct v1 disposition.

**Status:** pending

### Tracker mechanical corrections

- **S-4 wording** — prior text says `rows: Vec<Vec<Literal>>`. Actual type
  is `Vec<Vec<PhysicalExpr>>` (`node.rs:438-442`). Decision branch shifts:
  - (a) **Narrow** to `Vec<Vec<Literal>>` — tightens contract; aligns with
    v1 "constants only" expectation; type-match per-cell straightforward.
    Breaks any test exercising non-literal cells.
  - (b) **Keep** `PhysicalExpr` — admits constant-folded exprs; per-cell
    validation needs full type-inference path.
- **S-3 split** — close 3a as IMPLEMENTED; reframe 3b as "compile-time
  primary check; IR carries `JoinKeyTypeMismatch` as residual-trust
  diagnostic only."
- **B-1 recommendation re-tilt** — agent (1) recommends **Option C
  (hybrid)** over the pre-review **B (drop entirely)** based on normative
  spec cite (`35 §11.7:1370`, `14a §3.6:263`) declaring closed-five carrier
  rule. Option B requires spec amendment; Option C does not.

### Cross-iteration impacts surfaced

- **B-1 = C ↔ P-9** — closed-five hard-coded `Additivity` / `ReturnTypeRule`
  table becomes single source of truth for `AggregateKind::Builtin`;
  duplication-by-omission is structural, not redundant.
- **P-3 ↔ P-6** — adding `Custom` rules for `ceil` / `floor` Decimal
  amplifies P-6's broken `(Custom, Custom) == false` quirk. Reinforces P-6
  Option A (drop `PartialEq`).
- **P-8 ↔ foundations** — `14a:65` declares `description` field. Removing
  the field cascades to a foundations spec edit (clause-level approval per
  workstyle rule 4).
- **S-1 ↔ spec §6.3** — removing 8 stub methods + the `SemanticExprAccessorExt`
  trait surface cascades to a spec edit that drops the surface declaration.

### Open meta-questions for user (clause-level decisions before edits)

1. **B-1 direction.** Confirm Option C (hybrid `AggregateKind` wrapper) over
   prior B (drop enum). Spec normatively pins closed-five carrier; C
   preserves invariant while admitting catalog-8.
2. **B-2 shape (contingent on B-1).** If C: change `AggregateExpr.input_expr`
   to `args: Vec<PhysicalExpr>`. Confirm.
3. **S-1 disposition.** Caller search shows zero non-test callers — confirm
   willingness to delete the 8 methods + `SemanticExprAccessorExt` trait
   (Option C) vs preserve as documented no-op (Option B).
4. **S-4 type direction.** Narrow `ValuesNode.rows` to `Vec<Vec<Literal>>`
   (a), or keep `PhysicalExpr` and add full per-cell validation (b).
5. **S-6 nullability widening.** Confirm Option (b): planner-set, validator
   equality-checks against locally-widened expectation.
6. **N-3 enforcement.** Doc-only (A) or seal `ExprLeaf` (B)?
7. **N-5 unknown-`Capability` posture.** Treat unknown as present/supported
   (a) or absent/unsupported (b)?
8. **P-3 `ReturnTypeRule` for ceil/floor Decimal.** Use `Custom` (couples
   with P-6 footgun footprint) or add new variant (e.g.
   `DecimalScaleZero`)?
9. **P-5 / N-2 / N-4 / N-1.x scope.** Do these polish/declaration items
   belong in this review's apply-pass, or deferred to follow-up?

A surgical change plan has been drafted in
`docs/design/implementation/35_ir_review_changes.md` covering only the items
above. Per workstyle rule 3, **no code edits will be applied until each
clause is approved.**

---

## Third-pass review — post-implementation delta (2026-05-26)

Three implementation waves landed against the change plan. Four parallel
review agents (Wave 1 / Wave 2 / Wave 3 / cross-cutting) verified the result
against the same 7 vision principles + 16 review criteria used in the
original review. Cumulative state at the time of review:

- `cargo check -p semstrait-ir` — pass
- `cargo test -p semstrait-ir --lib` — **303 passed; 0 failed**
- `cargo clippy -p semstrait-ir --all-targets -- -D warnings` — clean
- `cargo fmt -p semstrait-ir -- --check` — **63 fmt diffs** (XC-3, below)

Test count progression: baseline 338 → Wave 1 347 → Wave 2 346 → Wave 3 303.
Net delta −35 = 41 derive-driven test deletions (P-1) + 6 new behavior tests
(closed-five lookup, integer/float-width serde, decimal overload presence,
n-ary aggregate, nullability widening, agg-uniqueness across group_by ∪
aggregates). No behavior tests were lost.

### Clause-level verdicts (applied as planned)

| Clause | Verdict | Notes |
|---|---|---|
| B-1 | ✓ | `AggregateKind::{Builtin, Extension}` adopted; `#[non_exhaustive]`; `expr_kinds.rs:72-77` |
| B-2 | ✓ | `AggregateExpr.args: Vec<PhysicalExpr>`; n-ary extensions exercised in tests |
| S-1 | ✓ | `SemanticExprAccessorExt` deleted; zero workspace hits remain |
| S-2 | ✓ | Typed `Literal::{Null, Integer, Float}` + `IntegerWidth` + `FloatWidth`; NaN-driven non-`Eq` regression test added |
| S-4 | ✓ | `ValuesNode.rows: Vec<Vec<Literal>>` + arity validation (`values_row_arity`) |
| S-5 | ✓ | Single-pass `HashSet<&Name>` over group_by ∪ aggregates |
| S-6 | ✓ | `widen_for_join` rewrites *type metadata only*; structural-only V-7 preserved |
| P-2 | ✓ | Dead `time_default` removed |
| P-3 | ✓ | `ReturnTypeRule::DecimalScaleZero` + `ParamType::DecimalFamily` + ceil/floor/round/median Decimal overloads |
| P-6 | ✓ | `PartialEq` dropped from `ReturnTypeRule` and `FunctionSpec` |
| P-7 | ⚠ partial | Removed from crate root, **but `functions/mod.rs:28` still re-exports `RegistryExtension`** — see W2-N-1 |
| P-8 | ✓ | `description` field removed; helper signatures updated; foundations spec edit (D-P8.1) deferred |
| P-9 | ✓ | `closed_five_additivity` + `closed_five_return_type` + `promote_numeric` added; `#[allow(dead_code)]` until Phase B consumes |
| N-4 | ✓ | `Arc<Schema>` value-shared, never-mutated invariant doc on `NodeMeta.output_schema` |
| P-1 | ✓ | 41 derive-driven tests removed across 9 files; behavior tests retained |

### V-5 / V-7 audit (critical principles)

- **V-5 (PhysicalExpr never evaluated).** No new code path calls `eval` /
  `compute` / value inspection. Grep clean across `crates/semstrait-ir/src/`.
- **V-7 (structural validation only).** All three new validation rules
  (`values_row_arity`, `agg_duplicate_name`, `join_nullability`) read shape
  metadata only — `row.len()`, `Name` strings, `Schema.columns[i].nullable`.
  No literal payload, no expression interior, no constant-folding.

### NEW findings (post-implementation, surface only — no clause-level edits applied)

- **W1-N-2** `expr/expr_fn.rs:35-60` — `IntoLiteral for f32` not implemented.
  `FloatWidth::Float` is reachable only via direct struct construction.
  Suggested follow-up clause `D-S2.4`: decide whether `IntoLiteral for f32`
  should exist and surface `FloatWidth::Float`, or whether the variant is
  parser-side-only.
- **W1-N-3** Spec drift: `docs/design/apis/35_semstrait_ir.md:1370-1371` and
  `:320-336` still type `aggregation: AggregationOp`, `input_expr:
  PhysicalExpr` (singular), and `Literal::{Integer(i64), Float(f64), Null}`.
  Wave 1 code now diverges; spec amendment is a separate clause-level
  decision (parent's D-B1.x, D-B2.2, S-2-spec).
- **W2-N-1** `functions/mod.rs:28` — `pub use extension::RegistryExtension;`
  still exposes the trait at `semstrait_ir::functions::RegistryExtension`.
  P-7's intent ("hide until wired") is only half-applied. Choose `pub(crate)
  use` or removal to complete the clause.
- **W2-N-2** `plan/validate.rs:194-207` — `widen_for_join` allocates a fresh
  `Schema` (Vec of `SchemaColumn`) for each side per Join validation. Inner
  joins clone unnecessarily. m10-performance hint: a borrow-comparing
  variant (`expected_nullable_at(i, side, jt)`) avoids the clones at high
  plan-tree fanout. Polish severity.
- **W3-N-1** `functions/builtins/math.rs:31-44` — ceil/floor declare three
  signature overloads (Float, Double, Decimal); `DecimalScaleZero` rule
  documents only the Decimal case. Resolution semantics for Float/Double
  inputs are implicit. Spec 14a §4.3 line 194-195 lists `(Float) -> Float,
  (Decimal(p,s)) -> Decimal(p,0)` — the catalog adds a Double overload not
  in the spec. Two issues: (a) catalog/spec drift on Double, (b)
  `DecimalScaleZero` resolver semantics for Float/Double need either a doc
  clause or a Custom-rule fallback. Polish severity.
- **W3-N-2** `docs/design/foundations/14a_function_catalog.md:65` still
  declares `pub description: &'static str` on `FunctionSpec`. P-8 was
  approved as code-only this wave; foundations edit was flagged as derived
  clause D-P8.1 awaiting separate approval.
- **XC-1** `functions/builtins/mod.rs:32-61` — `closed_five_additivity` and
  `closed_five_return_type` are `pub(crate)` with **0 production call
  sites** (test sites only), gated `#[allow(dead_code)]` "reserved for
  Phase B". Helper exists ahead of consumer. Acceptable if Phase B is the
  next-landing phase; otherwise consider gating behind a feature flag or
  lifting into `phase-b` once it lands. Tracked as `D-P9.2` in change plan.
- **XC-2** `functions/builtins/mod.rs:64-74` — `promote_numeric` has exactly
  one call site (`closed_five_return_type`). Could be inlined; minor.
- **XC-3** `cargo fmt -p semstrait-ir -- --check` reports 63 mechanical
  diffs across 18 files. Recommend running `cargo fmt -p semstrait-ir`
  before merge.
- **XC-4** Spec drift bundle (W1-N-3 + W3-N-1 + W3-N-2) — five spec
  amendments need clause-level approval before the next consumer crate
  reads `35_semstrait_ir.md` as authoritative: AggregateKind variant
  shape, AggregateExpr.args plural, typed Literal variants,
  IntegerWidth/FloatWidth, ParamType::DecimalFamily +
  ReturnTypeRule::DecimalScaleZero, FunctionSpec.description removal.

### Status summary

15 of 15 tactical clauses landed cleanly with one partial (P-7 — see
W2-N-1). All vision principles V-1 through V-7 hold. Test sufficiency:
strong (behavior-focused, no derive-driven smoke). Cargo state: green
except for `cargo fmt --check` (XC-3, mechanical only).

Per workstyle rule 3, the new findings (W1-N-2, W2-N-1, W2-N-2, W3-N-1,
W3-N-2, XC-1, XC-2, XC-3, XC-4) are surfaced for clause-level approval
before any further code edits.
