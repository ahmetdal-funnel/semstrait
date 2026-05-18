---
status: working-notes (transient)
purpose: Compaction-safe state of the open architectural question on Expr<L> placement vs ExprBlock duplication
extracted-from: chat session 2026-05-18 (feature/expr-tree branch)
destination: incorporate decision into `14`/`19`/`32` as part of the expression-spec consolidation pass; then delete this file
---

# Expression IR placement — working notes

## The open question

Should `semstrait-model` **import** `Expr<L>` from `semstrait-ir` (current state, and conventional), or should it have its own **parallel `ExprBlock` AST** that gets lowered to `Expr<L>` in `semstrait-manifest`?

Human's instinct: dependency is unavoidable because model needs expression vocabulary to even *describe* what a Measure carries. Duplication via `ExprBlock` looks redundant.

## State of decisions already made in this session

- **Error naming refactor (Comment 1):** drop `Kind` suffix on `*ErrorKind` / `*WarningKind` enums in expression docs (`14`/`14a`/`19`); leave global rename as deferred follow-up. **Agreed in principle; pending final go.**
- **Scope discipline (Comment 3):** this pass touches expressions only. Binding (`15`), manifest structure (`33`), planner internals (`34`) are out. Even if existing code becomes invalid, the refactor is a redesign of the parse/compile boundary. **Agreed.**
- **No accessor-enum move to `semstrait-core`:** human is uncertain about `semstrait-core`'s future; nothing new lands there in this pass. **Agreed.**

## The architectural shift I proposed (now under question)

Two-tier:

1. `semstrait-model` owns `ExprBlock` + Rust DSL + model-side accessor enums.
2. `semstrait-ir` keeps `Expr<L>` + `SemanticLeaf` + `PhysicalLeaf` + ir-side accessor enums.
3. `semstrait-manifest` runs an ExprBlock → `Expr<SemanticLeaf>` lowering pre-pass (function-name resolution, `Aggregate` synthesis from `(agg:, expr:)`, accessor mapping).
4. Duplicate accessor enums (4 small enums each in model and ir) with mechanical mapping.

**Benefit claimed:** model fully decoupled from ir; cleaner DAG (`core → {model, ir} → manifest → planner → adapter`).

**Cost:** parallel structural variants in `ExprBlock` and `Expr<L>`; duplicate accessor enums; mapping boilerplate in manifest.

## Options actually on the table

| ID | Shape | Decoupling | Complexity | Notes |
|---|---|---|---|---|
| **A** | model imports ir's `Expr<L>` directly (current state) | model→ir coupled | Lowest | What `14 §9.2`/`§9.3` say today |
| **B** | model has `ExprBlock`; ir has `Expr<L>`; manifest lowers | Fully decoupled | Highest (parallel variants, mapping) | My earlier proposal — human pushing back on this |
| **C** | Expr<L> + leaves + accessors in a new shared crate (e.g., `semstrait-ast`) | Decoupled via shared dep | Medium (new crate) | Adds a crate |
| **D** | Expr<L> in `semstrait-core` | Decoupled via shared dep | Medium | **Rejected** — human said don't touch core |
| **E** | Option A + properly document `ExprBlock` shape as a sub-form/transient view of `Expr<SemanticLeaf>` | model→ir coupled | Low | Hybrid: keep coupling, document parse boundary clearly |

## Constraints / inputs to a recommendation

1. **Semstrait is canonical-first.** Parse produces an authored shape; compile produces canonical IR; adapt produces engine artifact. Two conversion boundaries.
2. **Type-level invariants matter** (`14 §3.7`): `PhysicalExpr` cannot contain `Field`/`Dimension`/`Measure`/`Metric`/`Key`. This is upheld by the leaf-set distinction. **This invariant is the load-bearing piece of `Expr<L>`'s parameterization.**
3. **Single source of truth (DOCS_MAINTENANCE §1):** parallel variant rosters violate the spirit.
4. **I7 strict DAG:** any coupling must be acyclic; current `model → ir` is acyclic.
5. **Rust DSL** (`dim()`, `measure().previous()`, `std::ops`) lives somewhere and produces *some* type. Whichever type that is determines author ergonomics.
6. **`semstrait-core`'s future is uncertain;** treat it as frozen for this pass.
7. **YAML serde derives** need to attach to *something* with reasonable variant shape. `Expr<L>` with `CanonicalFn` (an interned newtype) inside `FunctionCall` is awkward to derive `Deserialize` for directly because `CanonicalFn::new` does validation.

## Precedents to compare against (research target)

- **sqlparser-rs / datafusion** — sqlparser owns the parsed AST; datafusion has `LogicalPlan` / `Expr`. Two-tier with explicit conversion.
- **Apache Calcite** — `SqlNode` (parsed) vs `RelNode` (relational). Two-tier with named lowering.
- **Polars** — `Expr` (lazy AST, both parsed and engine consumed). Single-tier.
- **dbt** — Python; ref/source macros lower to compiled SQL. Different paradigm.
- **Cube.js** — JS; cubes have schema, queries have separate AST. Different paradigm.
- **Apache Iceberg's expression module** — Java; `Expression` interface with literal/bound/unbound variants. Single AST with type-state-like tagging.
- **rustc** — `ast::Expr` (parsed) vs `hir::Expr` (post-resolution) vs `mir::*`. Multi-tier; explicit lowering per stage.

The pattern is **mostly two-tier in production systems**. The single-tier (Polars) works when the AST has no parse-time vs compile-time semantic distinction.

For semstrait, the parse vs compile semantic distinction is **real**:
- Parse: typed-semantic leaves carry unresolved `SemanticsName`; no function-registry validation; no kind resolution; no cycle detection.
- Compile: typed-semantic leaves are substituted; functions are registry-resolved; kind-checked; cycle-free.

This is structurally identical to rustc's `ast::Expr` vs `hir::Expr` distinction.

## My pre-research bias

Lean toward **Option A or E**, not B.

Reasoning:

- The cost of model importing ir is small in practice — ir is itself thin (no I/O, no async, no engine deps per `35 §1.3`).
- "Lightweight" should mean "no heavy runtime deps + clean DAG" — not "zero cross-crate types". model→ir is one acyclic edge; that's fine.
- Two-tier (Option B) introduces real duplication that violates DOCS_MAINTENANCE §1 (single source of truth) and forces drift-management.
- The `Expr<L>` design already gives us the parse-vs-compile distinction via *leaf set* (SemanticLeaf vs PhysicalLeaf) without needing a parallel structural enum. That's elegant; throwing it away for "decoupling" is over-engineering.

What I previously framed as "ExprBlock in model" was conflating two things:
- (a) The **serde wire form** of an expression (which is just `Expr<SemanticLeaf>` with serde derives — IS just `Expr<L>`)
- (b) A **structurally different AST** for the parse stage (which is what I drew, and which is unnecessary)

(a) is fine and is just "use `Expr<L>` with serde". (b) is the duplication trap.

So **the proposal should be Option E**: keep model→ir; properly document that model uses `Expr<SemanticLeaf>` as both the parse target *and* the in-memory authored form. There's no separate `ExprBlock` — the YAML serde shape IS `Expr<L>` deserialized.

## What the research subagent should validate

1. **In Rust ecosystem precedents**, is two-tier actually load-bearing, or is it cargo-cult?
2. **In semstrait's current `crates/`**, how is `ExprSource`/`Expr<L>` actually wired today? (`STATUS.md` mentions `P1a — ExprBlock archive`: the typed AST moved to `expr_ast.rs #[doc(hidden)]` and `ExprSource::Declarative` carries `serde_yaml::Value` opaque pass-through.) Does the current code already split or merge?
3. Are there semstrait-specific concerns that make two-tier (B) actually worth it (e.g., the Inline DSL — but that's deferred per `14 §6.3`)?
4. Is **Option E** (keep model→ir; document `ExprBlock` as `Expr<SemanticLeaf>` + serde) the cleanest path?

## Decision rubric for the subagent's recommendation

Score each option on:

- **DAG cleanliness** (I7): all options pass since all are acyclic
- **Duplication** (DOCS_MAINTENANCE §1)
- **Parse/compile semantic distinction enforcement** (I5): does the type system uphold "no semantic-leaf in PhysicalExpr"?
- **Author ergonomics** (Rust DSL site)
- **Serde / parse implementation cost**
- **Maintenance burden** (variant additions)
- **Cost to flip later** if we get it wrong

Then recommend.

## Pending after research

Once subagent returns:

1. Update this file with the recommendation + confirmed option letter.
2. Re-draft the architectural-shift section of the plan accordingly.
3. Present to human for go/no-go.
4. Execute the 6 spec edits.
5. Self-verify (grep + lint + cross-ref checks).
6. Hand off to codex model for external review.

## Items locked in (regardless of which option wins)

- Error naming refactor in expression docs (`Kind` suffix dropped).
- Dead-ref cleanup: `14 §5.6` cluster → new `14 §5.4`; `14 §4.3` retargets; `14 §7` "see §12" residue; `19 §10` "`14a §3.1`" stale; `LikeKind::Escape(c)` typo.
- Missing expression-error variants in `19 §8.1`.
- `Aggregate` synthesis from `(agg:, expr:)` documented (location depends on Option A/B/E).
- `STATUS.md` item P + deferred-global-rename note.

## Items deferred (per scope discipline)

- Binding-selection algorithm for cross-kind splice (Blocker 3).
- `LoweringCtx` visibility + inner-type definitions.
- Auto-mapping `name == column` clarification.
- Cross-kind join-key column harvesting rule.
- Anything in `15` (binding) / `33` (manifest structure beyond cited variants) / `34` (planner beyond cited variants).

---

## Research findings (inline, 2026-05-18 evening)

### Current code state — a critical surprise

The actual `crates/` graph does **NOT** match either the current spec OR my two-tier proposal. It's a third arrangement:

| Crate | Internal deps | Where `Expr` lives |
|---|---|---|
| `semstrait-core` | none (leaf) | **owns `Expr`** (`src/expr.rs`, 1216 lines — "the single expression representation from YAML parsing through planning, IR, SQL emission, and Substrait serialization") |
| `semstrait-model` | `semstrait-core` only | imports `Expr` from `semstrait-core::expr` (model's `expr_ast.rs` is the dormant typed-AST archive — currently inert, `ExprSource::Declarative` carries `serde_yaml::Value` pass-through per STATUS P1a) |
| `semstrait-ir` | `semstrait-core` only | **re-exports `Expr` from `semstrait-core`** via `pub use plan::node::{… Expr};` — does NOT define its own `Expr`. Owns `PlanNode` / `LogicalPlan` / Substrait serialization only. |
| `semstrait-manifest` | `core` + `model` + `catalog` (NOT `ir`) | uses `core::Expr` through `model`'s types |
| `semstrait-planner` | `core` + `ir` + `manifest` + `catalog` | uses `core::Expr` via `ir`'s re-export |
| `semstrait-adapter` | `core` + `ir` | same |

So the code today is **Option D in disguise** (`Expr` already in core, used everywhere through re-exports). The spec (`14 §9.2/§9.3`, `35 §3`) ratifies a TARGET state (`Expr<L>` in ir, parameterized, with PhysicalLeaf/SemanticLeaf split, model depends on ir). The migration hasn't happened.

Notably:

- The current `Expr` is a **flat unified enum** (`Column`, `Literal`, `EntityRef`, `Aggregate`, `BinaryOp`, …). No `Expr<L>` parameterization. No leaf-set distinction. `EntityRef` is a variant of the SAME enum that `PhysicalLeaf`-equivalent variants (`Column`, `Literal`) live in. So the spec's load-bearing `14 §3.7` invariant ("PhysicalExpr cannot contain Field/Dimension/Measure/Metric/Key") is **NOT type-enforced in current code** — it doesn't exist there at all.
- `semstrait-ir::Expr` is `semstrait-core::expr::Expr`. They're the same type.
- `semstrait-manifest` does not depend on `semstrait-ir` (manifest goes core+model only); per the spec, manifest should consume IR types, so this is also a divergence.

The "spec-vs-code gap" is acknowledged by `CLAUDE.md` ("current code does not yet match spec") and tracked via `implementation/40_refactor_plan.md`. We're in spec-driven-dev mode; specs lead, code follows.

### Ecosystem patterns

- **sqlparser-rs + DataFusion** — sqlparser owns the parsed AST; DataFusion has `LogicalPlan` + a separate `Expr`. Two-tier with explicit lowering through `SqlToRel`. **Forces:** sqlparser is dialect-flexible and serializable; DataFusion's `Expr` is typed for execution. Two ASTs because parse and execution have genuinely different concerns (locations + raw tokens vs typed bound exprs).
- **Apache Calcite** — `SqlNode` (parsed) vs `RelNode` (relational). Two-tier. **Forces:** same as DataFusion — `SqlNode` is parser output, `RelNode` is post-validation typed relational algebra.
- **Polars** — single `Expr` for both authoring (DSL) and engine consumption. **Forces:** Polars's DSL has no semantic-name resolution stage; the DSL constructs typed Exprs directly. No parse/compile distinction → no need for two ASTs.
- **rustc** — multi-tier (`ast::Expr` → `hir::Expr` → `thir::Expr` → `mir::*`). **Forces:** each tier discards information the next doesn't need; this is the textbook compiler-tower pattern. Justified by sheer scale.
- **Apache Iceberg's expression module** — `Expression` interface with `bound` vs `unbound` variant pattern (single enum, state tagged). **Forces:** Iceberg uses an `Expression.bind(schema)` operation that returns the same type with bound state. Their concerns map to a single AST with phase-state in the data.

**Pattern that emerges:** two-tier is justified when *parse and compile have genuinely different concerns that don't share structure*. Polars and Iceberg get away with single-tier because the DSL/builder produces ready-to-execute exprs directly.

**semstrait's case:** the parse→compile transition is a **leaf substitution** (typed semantic leaves → resolved physical leaves), not a structural rewrite. The structural variants (`BinaryOp`, `Case`, `Aggregate`, …) are identical pre- and post-compile. This is what `Expr<L>` parameterization captures: **same structural skeleton, different leaf sets**. That's the elegant insight in `14 §3`'s design — semstrait is naturally single-skeleton, two-leaf-set, NOT two-tier.

### Option scorecard

| Option | DAG | Duplication | I5 enforce (no semantic-leaf in PhysicalExpr) | Ergonomics | Serde cost | Maintenance | Flip cost | Verdict |
|---|---|---|---|---|---|---|---|---|
| **A** spec-as-written (`Expr<L>` in ir; model→ir) | acyclic; thin edge | none | type-level via leaf-set parameterization | Rust DSL produces `Expr<SemanticLeaf>` (in ir) | normal — serde derives on `Expr<L>` work | low — one variant catalog | already what spec ratified | ✓ recommended |
| **B** two-tier (`ExprBlock` in model + `Expr<L>` in ir) | acyclic; model & ir parallel | high — parallel structural variants + duplicate accessor enums | type-level via separate types | DSL has to pick a target type; ambiguity | parallel serde derives | high — every Expr variant addition touches both | very high (one-way) | ✗ over-engineered |
| **C** new shared crate `semstrait-ast` | adds a crate | none | type-level | DSL in shared crate | normal | low | medium | 🟡 viable but adds a crate for no gain over A |
| **E** current state (`Expr` in core, flat enum, no parameterization) | acyclic but core is overloaded | none | NOT enforced — `EntityRef` and `Column` are siblings in one enum | DSL produces `core::Expr` | normal | low | low (status quo) | 🟡 sacrifices I5 type-enforcement; relies on runtime checks |

### Recommendation: **Option A** — keep the spec as-written

This is what the spec already ratifies (`14 §9.2`, `14 §9.3`, `35 §3`, `35 §17.2`). My earlier two-tier proposal was over-engineering: it added structural duplication to escape a one-edge `model → ir` dependency that costs nothing in practice (ir is itself thin — no I/O, no async, no engine deps per `35 §1.3`).

The human's instinct ("dependency is unavoidable — to build model we need to know how expressions look") is **correct and is exactly what the spec already encodes**. `Expr<L>` is the canonical expression vocabulary; model consumes it; ir owns it. One source of truth.

The two-tier proposal (Option B) was reaching for a "lightweight model" goal that's better served by ir being lightweight — which `35 §1.3` already ratifies (no I/O, no async, single upstream `semstrait-core`). model→ir is a thin acyclic edge; that's fine.

The current code (Option E in disguise) puts `Expr` in `semstrait-core`. Per the human's earlier note ("don't put accessors in core; uncertain about core's future"), the direction of travel is **out of core, not deeper into core**. So the spec's target (move `Expr<L>` to ir) is also the right code-migration direction; the migration just hasn't landed yet (tracked under `implementation/40_refactor_plan.md` as `[TD-IR-ABSORB-EXPR]`).

### Concrete consequences for the spec edits (this pass)

- **`14 §9.2/§9.3` stays as-is** — `semstrait-ir` owns `Expr<L>`, leaf sets, accessor enums, `Parameter`, `CanonicalFn`, `FunctionRegistry`; `semstrait-model` owns `ExprSource` + parse-site dispatch and depends on `semstrait-ir` (no change to these subsections — they already say this).
- **No `ExprBlock` parallel AST in model.** What I previously called `ExprBlock` is just `Expr<SemanticLeaf>` with serde derives. The serde derives already live on `Expr<L>` per `35 §14.1`.
- **Accessor enums stay in `semstrait-ir`.** They're authoring vocabulary, but model uses them via the `semstrait-ir` re-export. No duplication.
- **`19 §3.1` substep order stays exactly as ratified.** No new pre-pass. The input to `resolve` is `SemanticExpr` (which IS `Expr<SemanticLeaf>`), authored directly via the parser or the Rust DSL.
- **Blocker 1's resolution (parser API in `32`)** becomes simpler: it's just "document `parse_semantic` and `parse_physical` from `14 §6.2` in `32`'s crate-API section; carry `ExprSource` as it already is; deserialize `Block(...)` via serde derives that walk the reserved-tag catalog from `14 §6.4.1`". No new types in model.
- **Blocker 2's resolution (Aggregate synthesis)** lands at parse-time in `semstrait-model` — when the Measure / Metric builder collects `agg:` and `expr:` together, it wraps into `Aggregate { op, args: [parsed_expr], distinct, filter }`. Document in `32 §<synth>` with a one-line note in `19 §3.1` saying "Aggregate-wrapping from `(agg:, expr:)` happens at parse-time per `32 §<synth>`; by the time `resolve` runs, the `SemanticExpr` already carries the Aggregate root if a Measure / Metric authored it".

### Risks / what could make this recommendation wrong

If the Rust DSL ergonomics get awkward because `expr_fn` (in `semstrait-ir::expr::expr_fn`) needs to be available to model-layer test fixtures and authoring code, AND if the implementer hits friction with serde derives on the `Expr<L>` newtype-with-CanonicalFn shape, we might want to lift `expr_fn` (the DSL constructors) to `semstrait-model` so authoring stays in the model crate while ir holds the IR types. That's a minor placement tweak inside Option A, not a return to two-tier.

The other risk: the **code migration** to move `Expr` from `core` to `ir` is non-trivial (1216 lines in core; ~2300 lines of dormant `expr_ast.rs` in model; full DAG re-plumbing in manifest/planner/adapter). This is **out of scope** for this spec pass — it's a separate `[TD-IR-ABSORB-EXPR]` implementation work item. For now, the spec ratifies the target; the code follows when the next implementation phase opens.

### Recommendation summary

**Option A. Single architecture: `Expr<L>` lives in `semstrait-ir`; `semstrait-model` depends on `semstrait-ir`; no `ExprBlock` parallel AST; accessor enums stay in ir.** This is what the spec already ratifies; my earlier two-tier proposal was over-engineering. Revised plan: keep all 6 spec edits from before, but DROP the "new `§2.2 ExprBlock lowering pre-pass`" addition (no longer needed) and DROP the "duplicate accessor enums in model" addition. Everything else (error rename, dead refs, missing error variants, Aggregate synthesis docs, `32` forward note, STATUS item P) stays.
