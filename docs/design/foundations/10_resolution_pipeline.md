---
prereqs: [00]
authoritative-for:
  - per-stage pipeline contract (inputs, outputs, invariants upheld, error types)
  - compile-time vs query-time boundary
  - I/O permission matrix per stage (refines I11)
  - sync/async posture per stage (refines I6)
  - typed-error-per-stage model; Diagnostic as user-facing render of typed errors
  - EngineAdapter's cross-stage role (injection points vs terminal transform)
refined-by:
  - 11 (names and scopes — details of name/scope resolution inside compile)
  - 13 (types and grain — types referenced across stages)
  - 14 (expressions — ExprSource→Expr compilation inside compile)
  - 15 (mapping and binding — compile-time `Binding` process and `SemanticMapping` resolution inside compile)
  - 16 (composition — Relationship resolution and ComposedSemanticInterface construction inside compile)
  - 17 (temporal shape — shape resolution inside compile; shape-gating inside plan)
  - 20–25 (DataKind strategy dispatch inside plan)
  - 31 (semstrait-core — shared-types crate role, pending)
  - 32–37 (per-crate public surfaces for each stage)
---

# 10. Resolution Pipeline — Per-Stage Contract

> **Note.** Root-shape authoritative spec: [`../apis/32_semstrait_model.md`](../apis/32_semstrait_model.md) + [`../data-kinds/26_nesting_matrix.md`](../data-kinds/26_nesting_matrix.md) + [`../apis/32b_catalogs_yaml.md`](../apis/32b_catalogs_yaml.md). This document predates that spec and is pending refactor.

## 1. Purpose and Scope

This document ratifies the per-stage contract of the canonical pipeline shown in `00_overview.md §5`. For each verb (`parse`, `validate`, `compile`, `plan`, `optimize`, `adapt`) it specifies:

- input type and output type (by vocabulary name — structural shapes live in 11–17 / 20–25 / 31–37),
- which `00` invariants it upholds,
- which typed-error enum carries its failures,
- which I/O surfaces (if any) it is permitted to touch,
- its sync/async posture,
- the forward-reference doc that ratifies its structural detail.

`10` does **not** re-specify concept shapes (those live in the foundations and data-kind docs) and does **not** specify per-crate public surface (that lives in the `3x` API docs). It is the contract matrix that ties the pipeline to the vocabulary.

**Two canonical data types bracket compile-time** (00 §4.1 `SemanticModel`, `Manifest`):

- `SemanticModel` — post-parse, in-memory, typed.
- `Manifest` — post-compile, planner-complete, denormalized.

No intermediate `ResolvedModel` type exists; all resolution-type work (name lookup, catalog metadata fetch, glob expansion, Relationship resolution, `ExprSource` → `Expr` compilation, index construction) happens inside `compile`. The word *resolve* is retained as descriptive English ("compile resolves references") and as the `Resolved*` type-name prefix for manifest-layer types that diverge structurally from model-layer counterparts (00 §4.1 naming note); it is no longer a top-level pipeline verb.

**`emit` is not a pipeline stage.** Per `00 §4.2`, `emit` is the name for the SQL-specific form `adapt` takes when the adapter is SQL-emitting (producing `SqlArtifact`). It is a vocabulary-level handle for SQL-emission specifics in `36`, not a distinct stage downstream of `adapt`. The pipeline terminates at `adapt`.

**Out of scope for this doc:**

- structural layouts of inputs/outputs (`SemanticModel`, `Manifest`, `SemanticPlan`, `EngineArtifact`) — see 11–17, 33, 35,
- planner strategy dispatch per DataKind variant — see 20–25,
- adapter implementation specifics per engine — see 36,
- public crate API shape — see 31–39.

## 2. Stage Index

Six stages. Compile-time = `parse`, `validate`, `compile`. Query-time = `plan`, `optimize`, `adapt`.

| # | Stage | Input | Output | Owner (forward-ref) |
|---|---|---|---|---|
| 1 | `parse` | YAML bytes | `SemanticModel` | 32 |
| 2 | `validate` | `&SemanticModel` | `Result<(), Vec<ValidateError>>` (pure predicate) | 32 |
| 3 | `compile` | `SemanticModel` + `CatalogProvider` + `FileSystem` | `Manifest` | 33 (orchestrator), 37 (metadata), 32 (AST source) |
| 4 | `plan` | `&Manifest` + `Request` + optional injected `EngineAdapter` hooks | `SemanticPlan` | 34, 36 |
| 5 | `optimize` | `SemanticPlan` + optional injected `EngineAdapter` hooks | `SemanticPlan` | 34, 36 |
| 6 | `adapt` | `SemanticPlan` | `EngineArtifact` (`Sql(SqlArtifact)` via `emit` sub-form, or `Plan(EnginePlan)`) | 36 |

```mermaid
flowchart LR
    subgraph CT["Compile-time (compile is async)"]
        direction LR
        Y[Model YAML] --> P(parse)
        P --> SM[SemanticModel]
        SM --> V(validate)
        V --> C(compile)
        C --> M[(Manifest)]
    end

    subgraph QT["Query-time (synchronous, I6 hot path)"]
        direction LR
        R[Request] --> PL(plan)
        M --> PL
        PL --> SP1[SemanticPlan]
        SP1 --> O(optimize)
        O --> SP2[SemanticPlan]
        SP2 --> A(adapt)
        A --> EA[EngineArtifact]
    end

    CPT{{CatalogProvider}} -.->|async I/O| C
    FST{{FileSystem}} -.->|async I/O| C

    EAD{{EngineAdapter}} -.->|injection mode| PL
    EAD -.->|injection mode| O
    EAD -->|terminal adapt| A
```

**Notes on the diagram.**

- `SemanticModel` is shown once; `validate` borrows it and returns `Result<(), Vec<ValidateError>>` (pure predicate, no new type). The `V → C` edge represents pipeline ordering (validation-passed precedes compile), not a data transformation.
- `CatalogProvider` and `FileSystem` are hexagons (traits per 00 §7.2 legend) and appear as dashed async-I/O dependencies on `compile` only — no other stage touches them.
- `EngineAdapter` has two arrow styles: dashed (injection mode — optional, replaces canonical choices in `plan`/`optimize` for near-canonical engines) and solid (terminal — every adapter owns `adapt`).
- Two I/O entry points from §6 are **not** on this diagram because they are outside the pipeline stages; see the phase-boundary diagram in §7.

## 3. Per-Stage Contract

### 3.0 Stage template

Every subsection in §3 uses this fixed template. Fields are authoritative for the stage; any deviation must be flagged and resolved in that stage's subsection, not elsewhere.

- **Purpose** — one sentence, imperative.
- **Input** — vocabulary-name type (forward-ref doc).
- **Output** — vocabulary-name type (forward-ref doc).
- **Owning crate** — the crate implementing the stage's entry point (see §4 for the layering rule).
- **Invariants upheld** — list of `00 §9` invariants this stage is directly responsible for maintaining. (Invariants merely not-violated but not upheld are omitted.)
- **Error type** — the typed Rust enum that carries this stage's failures. Convertible to `Diagnostic` at the nearest public API boundary (see §5).
- **Error policy** — `fail-fast` or `accumulate` (see §5).
- **I/O permitted** — explicit list of trait methods this stage is allowed to invoke. Everything not listed is forbidden. (Refines I11.)
- **Sync/async** — one of `sync`, `async`. Justification required for `async`. (Refines I6.)
- **EngineAdapter interaction** — whether this stage accepts injected adapter hooks, and which hooks.
- **Forward-refs** — docs that ratify structural detail produced or consumed here.

### 3.1 `parse`

- **Purpose** — Deserialize Model YAML text into a typed, in-memory `SemanticModel`.
- **Input** — YAML text (bytes or `&str`). A single-file Model (multi-file / include-directive support is deferred — §9).
- **Output** — `SemanticModel` (structural shape ratified in `32`).
- **Owning crate** — `semstrait-model`.
- **Invariants upheld** —
  - Preserves `ExprSource` verbatim (inline DSL strings and declarative YAML blocks). `parse` does **not** compile expressions; the `ExprSource` → `Expr` transition is `compile`'s boundary (I1).
  - Does not perform any name resolution. Cross-references (between DataKinds, between Semantics, between Bindings and sources) remain as unresolved identifiers in the `SemanticModel` (prefigures I5).
- **Error type** — `ParseError`. Typical variant families (exact list ratified in `32`):
  - YAML syntax errors (malformed document, unexpected token, bad indentation).
  - Schema tag errors (unknown top-level key, unknown discriminator variant, invalid type tag).
  - Structural errors (duplicate declaration of a name within its scope, missing required field, wrong field shape).
- **Error policy** — `accumulate` (per §5).
- **I/O permitted** — none.
- **Sync/async** — `sync`.
- **EngineAdapter interaction** — none.
- **Forward-refs** — `32` (`SemanticModel` structural shape, `ParseError` variants), `14` (`ExprSource` surface forms).

### 3.2 `validate`

- **Purpose** — Run structural Preconditions over a `SemanticModel` and accumulate all violations.
- **Input** — `&SemanticModel` (borrowed; `validate` does not consume or mutate).
- **Output** — `Result<(), Vec<ValidateError>>`. Pure predicate; returns the same `SemanticModel` surface to the caller (the caller passes it to `compile`).
- **Owning crate** — `semstrait-model`. Preconditions are model-level rules, co-located with the Model spec that defines what a valid model is.
- **Invariants upheld** —
  - The `Precondition` framework (00 §4.1): every structural Precondition defined in the vocabulary is evaluated here. Catalog-dependent Preconditions (function-registry coverage checks that depend on resolved `CanonicalFn` identities, reference checks that require resolved Bindings) are **not** run here — they run inline during `compile`.
- **Error type** — `ValidateError`. Typical variant families (exact list ratified in `32`):
  - Disallowed nesting (violates `12`'s container-in-container matrix).
  - Missing mandatory field (structural Precondition).
  - Arity violation (e.g. a `Joinset` with fewer than the required number of constituents, a `Grainset` with an empty grain set).
  - Malformed reference (a syntactically invalid identifier, independent of whether the target exists).
  - Shape-incompatible declarations (e.g. `SCD` subtype mismatched with supplied temporal columns — structural aspects only, gated by `17`).
  - Relationship well-formedness (Relationship block's structural shape, independent of target existence — gated by `16`).
- **Error policy** — `accumulate`. Batch-reporting every Precondition failure is the whole point of the pass.
- **I/O permitted** — none.
- **Sync/async** — `sync`.
- **EngineAdapter interaction** — none.
- **Forward-refs** — `32` (`ValidateError` variants, Precondition catalog), `11` (reference well-formedness rules), `12` (nesting matrix), `16` (Relationship structural rules), `17` (shape-gated structural rules).

### 3.3 `compile`

- **Purpose** — Produce a planner-complete `Manifest` from a validated `SemanticModel`, performing all resolution, catalog fetching, expression compilation, and indexing in a single pass.
- **Input** — owned `SemanticModel` + `impl CatalogProvider` + `impl FileSystem`. (The caller has already invoked `validate`; `compile` does not re-run structural Preconditions.)
- **Output** — `Manifest` (structural shape ratified in `33`).
- **Owning crate** — `semstrait-manifest` (orchestrator) + `semstrait-catalog` (metadata I/O) + `semstrait-model` (AST source).
- **Work subsumed** — this is the work that would have been split across a separate `resolve` stage and a downstream `compile` stage in an engine-style pipeline; here it is a single coherent pass:
  1. Name resolution — lexical scope traversal, reference expansion, alias expansion across DataKinds, Semantics, Bindings, and Relationships (I5).
  2. Catalog metadata fetch — via `CatalogProvider` for each Binding's declared source: schema (columns + types), catalog-supplied metadata.
  3. `FileSystem` glob expansion — a single declared Binding pattern can resolve to **one or more** `PhysicalSource`s (00 §4.1 `PhysicalSource`).
  4. Reference-level Preconditions — function-registry coverage (every `CanonicalFn` used exists in the registry with compatible arity and argument types), target existence (every referenced DataKind / Semantics / source / column actually exists in the resolved context).
  5. `ExprSource` → `Expr` compilation — using the `FunctionRegistry` for function identity, the resolved name scope for identifier resolution, and the resolved schema for type inference (I1 boundary).
  6. Relationship graph resolution — resolving the top-level `Relationship` block's endpoints, validating `Cardinality`, preparing the graph for `ComposedSemanticInterface` construction (I5, `16`).
  7. `TemporalShape` resolution — binding shape-specific columns (`valid_from`, `valid_to`, `occurred_at`, `snapshotted_at`, …) to physical columns; shape-gated resolution rules (`17`).
  8. Manifest index construction — name indices, Coverage indices, `Relationship` adjacency, per-DataKind Semantics lookup tables (I8).
  9. Denormalization and `Resolved*` type construction — manifest-layer types that diverge structurally from model-layer counterparts (00 §4.1 naming note, I8).
- **Invariants upheld** —
  - I1 — the `ExprSource` → `Expr` boundary. After `compile`, no `ExprSource` survives into the Manifest or anything downstream.
  - I4 — canonical IR only in the output (`Expr`, `DataType`, `CanonicalFn`).
  - I5 — all name resolution is performed here and captured in the Manifest as direct references; nothing resolvable is deferred to `plan`.
  - I8 — the Manifest is planner-complete; every pre-computed index, every flattened denormalization, every `Resolved*` type that the planner expects is constructed here.
  - I11 — all I/O is through the permitted provider traits (`CatalogProvider`, `FileSystem`), never direct filesystem or network calls.
- **Error type** — `CompileError`. Typical variant families (exact list ratified in `33`):
  - `UnresolvedReference` — a name reference did not resolve in its scope.
  - `AmbiguousReference` — multiple candidates matched a name reference.
  - `SourceNotFound` — a declared source (path or catalog entry) does not exist.
  - `CatalogUnavailable` — `CatalogProvider` returned an unrecoverable error.
  - `SchemaResolutionFailed` — catalog returned a schema incompatible with the Model's claims.
  - `GlobExpansionFailed` — `FileSystem` expansion produced zero matches, or failed to list.
  - `UnknownFunction` — an `ExprSource` referenced a function not in the `FunctionRegistry`.
  - `TypeInferenceFailure` — local type inference could not derive a type for a Semantics element (bare untyped `Null` at a Semantics boundary with no `data_type:` declared, unresolved column/entity ref, or an expression shape with no applicable inference rule). Semstrait does **not** validate cross-operand type compatibility — per `14 §5.6` operand / comparison / argument compatibility is deferred to the engine, not checked by compile.
  - `CircularRelationship` — Relationship graph contains a cycle of a kind disallowed by `16`.
  - `IndexBuildFailed` — invariant violated during Manifest index construction (indicates a bug in the compile pass; should not arise from well-formed input).
- **Error policy** — `fail-fast`. Resolution, catalog I/O, and Expr compilation form dependency chains: continuing past a failure produces unreliable cascades (e.g. a downstream `TypeMismatch` caused by an earlier `UnresolvedReference` is not a real error, just noise).
- **I/O permitted** —
  - `CatalogProvider::*` — all methods (metadata fetch, schema lookup, catalog-specific metadata). Exact trait surface ratified in `37`.
  - `FileSystem::{list, read, exists}` — read-side only; no writes. `compile` never mutates external state.
  - `Repository` — forbidden (Manifest persistence is the caller's concern, not a compile-time I/O entry).
- **Sync/async** — `async`. This is the pipeline's only async stage. All I/O via `CatalogProvider` and `FileSystem` is async; the orchestrator awaits everything before returning the `Manifest`.
- **EngineAdapter interaction** — none. The adapter is a query-time concern; its injection points are at `plan` / `optimize` / `adapt`. `compile` does not know about engines.
- **Forward-refs** — `33` (`Manifest` structural shape, `CompileError` variants, orchestration details), `37` (`CatalogProvider` and `FileSystem` trait surfaces), `11` (name resolution mechanics), `14` (`ExprSource` → `Expr` compilation rules, `FunctionRegistry`), `15` (compile-time `Binding`, `SemanticMapping`, `PhysicalSource` resolution), `16` (Relationship graph and `ComposedSemanticInterface` construction), `17` (`TemporalShape` resolution).

### 3.4 `plan`

- **Purpose** — Construct a canonical `SemanticPlan` from a `Manifest` and a `Request`, performing Constraint evaluation, from-resolution, per-DataKind strategy dispatch, PlanNode construction, and SessionContext materialization.
- **Input** — `&Manifest` + owned `Request` (which carries an embedded `SessionContext`). Optional injected `EngineAdapter` in **injection mode** (see §3.4.1 below).
- **Output** — `SemanticPlan` (structural shape ratified in `35`; strategy-specific construction rules in `20`–`25`; algorithm detail in `34`).
- **Owning crate** — `semstrait-planner`.
- **Sub-steps (contract level).** Enumerated; exact algorithms deferred to `34` and the DataKind strategy docs.
  1. **Constraint check (pre-resolution, step 0).** `ConstraintValidator::check()` (per `11 §8.6`) runs as the planner's first action — BEFORE any of the sub-steps numbered below. For v1 realized carriers (Measure, Metric per `11 §8.4`), it evaluates every `constraints:` block on each Measure / Metric named in the Request against the Request's *query scope* (`request.dimensions` ∪ filter-field Dimensions). Failure returns `PlannerError::ConstraintViolation { entity, message }` immediately — fail-fast. This precedes dataset routing, Relationship traversal, and PlanNode construction by design; Constraints can forbid combinations ("this Measure cannot be grouped by that Dimension") that would otherwise make all downstream work meaningless. (Future reserved carriers per `11 §8.5` may select other stages; the per-carrier + per-kind stage matrix is `11 §8.6`.) The sub-steps below assume this check has already passed.
  2. **From-resolution.** If `Request.from` is set, the target DataKind is looked up directly (from-first). If `Request.from` is omitted, **field-first resolution** runs: the planner maps each requested Semantics back to its owning DataKind(s) via Manifest indices and, if the fields span multiple DataKinds, traverses the Relationship graph to form a `ComposedSemanticInterface` over the constituents (00 §4.1 `Request`, `ComposedSemanticInterface`; detailed rules in `16` and `34`).
  3. **Strategy dispatch.** The resolved target DataKind's variant selects the planner strategy: Simple (`21`), Grainset (`22`), Unionset (`23`), Joinset (`24`). `Compose`d targets dispatch per `16` / `24` rules. Each strategy's PlanNode construction is documented in its own data-kind spec; `10` treats the dispatch as opaque here.
  4. **PlanNode construction.** The strategy emits a canonical PlanNode tree (`35`). Adapter **injection hooks** (§3.4.1) may override specific nodes at defined extension points.
  5. **Expression inlining.** Semantics-level `Expr`s (computed Dimensions, Metrics, Filters) are inlined into their use sites on the PlanNode tree. Expressions are already typed (compiled from `ExprSource` in `compile`); no type inference happens at plan time.
  6. **SessionContext materialization.** Time-sensitive values from `SessionContext` (query clock, caller timezone) are substituted into the PlanNode tree as concrete literals at this stage, not threaded through. After this sub-step, the `SemanticPlan` is self-contained and SessionContext-free (b5).
  7. **SemanticPlan assembly.** Final PlanNode tree + Manifest reference + optional lineage metadata are packaged into the returned `SemanticPlan`.

- **Invariants upheld** —
  - I4 — canonical IR only in the output (`PlanNode`, `Expr`, `DataType`, `CanonicalFn`); no engine-specific types leak in.
  - I5 — planner performs only **Semantics lookup** via Manifest indices; no name resolution, no scope walking. Any identifier unknown to the index is `PlanError::UnknownReference`.
  - I6 — synchronous; no `.await`.
  - I8 — operates through the `Manifest` alone; no YAML parsing, no catalog queries (except the I11 out-of-band drift check, which is done **before** `plan` begins by the caller).
  - Determinism — given `(Manifest, Request)`, `plan`'s output is deterministic. SessionContext-sourced values are materialized as concrete literals (§3.4 sub-step 6), so any non-determinism is bounded to the SessionContext supplied by the caller; the planner itself introduces none.
- **Error type** — `PlanError`. Typical variant families (exact list ratified in `34`):
  - `ConstraintViolation { entity, message }` — a `constraints:` block on a requested Measure / Metric rejected the Request (v1 realized carriers per `11 §8.4`). Single typed variant; the free-form `message` field encodes which rule (`one_of` / `none_of` / `all` / `allowed` / `prohibited`) fired. Typed enum fan-out per rule is deferred per `11 §8.7` (`[TD-CONSTRAINT-ERROR-FANOUT]`).
  - `UnknownReference` — a `Request`-side identifier (field, from, filter target) does not match any Manifest index.
  - `AmbiguousFieldFirstResolution` — field-first resolution found multiple unrelated target DataKinds and the Relationship graph does not connect them.
  - `UnsupportedRequestShape` — the `Request` asks for a combination the strategy cannot satisfy (e.g. ordering over a Metric the target DataKind does not expose).
  - `StrategyDispatchFailed` — internal; indicates a bug in strategy dispatch (should not arise from well-formed inputs).
- **Error policy** — `fail-fast`.
- **I/O permitted** —
  - None directly. May call **injected** `EngineAdapter` hooks per §3.4.1; those hooks themselves must be pure (no I/O).
- **Sync/async** — `sync`. I6 hot path.
- **EngineAdapter interaction** — see §3.4.1.
- **Forward-refs** — `34` (`PlanError` variants, algorithm detail, injection-hook exact method list), `35` (`SemanticPlan` and `PlanNode` shape), `20`–`25` (per-DataKind strategy detail), `16` (composition / field-first resolution detail), `36` (adapter injection-hook trait surface).

#### 3.4.1 EngineAdapter injection (plan phase)

`EngineAdapter` has two distinct interaction modes with the pipeline; `plan` (and `optimize`, §3.5.1) is where **injection mode** applies.

**Two adapter modes — shape class**:

- **Injection mode** (substrait-compatible and near-canonical engines). The adapter registers rewrite hooks that the planner invokes at defined extension points inside `plan` sub-step 4 (PlanNode construction) and at specific points inside `optimize` (§3.5.1). The hooks **replace** canonical default choices with engine-aware choices, still producing canonical IR (I4). Use cases: choosing a canonical plan shape that maps naturally to the target engine when several equivalent canonical shapes exist (e.g. preferring one of two semantically equivalent aggregate forms because the target engine handles it efficiently); rewriting one `CanonicalFn` into a semantically equivalent `CanonicalFn` that the target engine renders better; re-parenthesizing canonical expressions to match engine-preferred patterns. Shape class: **visitor-style rewriter** — the adapter implements a trait with node-typed extension methods that the planner/optimizer invoke at known points. Exact method list ratified in `36`.
- **Conversion mode** (non-canonical-compatible engines). The adapter does not register injection hooks; the canonical `SemanticPlan` is built unmodified, and the adapter's terminal `adapt` call (§3.6) converts the whole plan end-to-end — either to a structured `EnginePlan` (full plan rewrite) or to a `SqlArtifact` via the `emit` sub-form. Use case: DuckDB SQL emission, Spark SQL emission, a non-Substrait structured plan.

**Required vs optional**:

- Every `EngineAdapter` provides terminal `adapt` (§3.6). Required.
- Injection hooks are **optional**. A pure-conversion adapter contributes nothing during `plan`/`optimize`. A pure-injection adapter still needs `adapt` — in its case, `adapt` can be an identity or a thin wrapper (since the plan has already been adjusted during `plan`/`optimize`).
- An adapter MAY provide both: a partially-canonical plan via injection followed by a final conversion pass in `adapt`.

**Invariant contract for injected hooks**:

- Pure (no I/O).
- Typed (produce canonical IR output only — `PlanNode`, `Expr`, `DataType`; engine-native types are forbidden in the hook's output, per I4).
- Deterministic (same input → same output; hooks may not read `SessionContext` — it has already been materialized by sub-step 6 in an ordering sense, though in practice hooks at sub-step 4 run before materialization; hooks depending on session state indicate an adapter-design bug).

Detailed hook surface, extension-point list, and engine-facing adapter trait are ratified in `36`.

### 3.5 `optimize`

- **Purpose** — Apply rule-based rewrites to a `SemanticPlan`, producing a semantically equivalent but more efficient plan. In the initial design, this is a near-identity pass; the stage exists so the pipeline shape is stable and rewrite passes can be added without re-plumbing.
- **Input** — owned `SemanticPlan`. Optional injected `EngineAdapter` rewrite passes (see §3.5.1).
- **Output** — `SemanticPlan` (semantically equivalent to the input).
- **Owning crate** — `semstrait-planner`.
- **Initial-design scope.** The canonical pass registry is intentionally minimal — or empty — in the first milestone. The project focus is "logical plan first, SQL emission second"; rule-based optimization (constant folding, metadata-dimension substitution, predicate simplification, pass registration — see 00 §4.2 `optimize` row) is deferred to a later milestone. `optimize` is nonetheless a **mandatory named stage**: the canonical pipeline entry point always invokes it, even when the registry is empty. An identity pass is a valid state, not a bypass.
- **Invariants upheld** —
  - Semantic equivalence: for every pass `p`, `execute(optimize(plan))` produces the same result set as `execute(plan)`. This is a contract on every registered pass; `optimize` as a stage enforces it only by accepting passes that claim it.
  - I4 — canonical IR preservation: passes MUST produce canonical IR; passes that emit engine-specific types violate the contract and are forbidden.
  - I6 — synchronous; no `.await`.
  - Determinism: given the same `SemanticPlan` and pass registry, `optimize`'s output is deterministic.
- **Error type** — `OptimizeError`. Typical variant families (exact list ratified in `34`):
  - `PassFailed` — a registered pass returned an error (pass-specific payload preserved).
  - `InvalidRewrite` — a pass produced a structurally invalid `SemanticPlan` (internal; should not arise from a correctly-implemented pass).
- **Error policy** — `fail-fast`. A failing pass aborts `optimize`; the caller receives the error rather than a partially-optimized plan.
- **I/O permitted** — none directly. May call **injected** `EngineAdapter` rewrite passes per §3.5.1; those passes must be pure.
- **Sync/async** — `sync`. I6 hot path.
- **EngineAdapter interaction** — see §3.5.1.
- **Forward-refs** — `34` (`OptimizeError` variants, pass registry mechanics, initial canonical pass set when defined), `36` (adapter pass-registration surface), `35` (`SemanticPlan` invariants passes must preserve).

#### 3.5.1 EngineAdapter injection (optimize phase)

Adapter injection at `optimize` is shape-classified as **pass registration**. An adapter operating in injection mode (§3.4.1) may contribute zero or more rewrite passes to the optimizer's pass registry. These passes run after canonical passes in a documented order (ratified in `36`). Each adapter-registered pass obeys the same invariant contract as canonical passes: pure, deterministic, semantic-equivalence-preserving, canonical-IR-producing. Adapters in pure conversion mode do not register optimize passes.

### 3.6 `adapt`

- **Purpose** — Transform a `SemanticPlan` into an `EngineArtifact` that a specific engine can execute, producing either a structured plan (`EnginePlan`) or an emitted SQL text (`SqlArtifact`) as the adapter's implementation type dictates.
- **Input** — owned `SemanticPlan`.
- **Output** — `EngineArtifact = Sql(SqlArtifact) | Plan(EnginePlan)` (00 §4.1 `EngineArtifact`, `SqlArtifact`, `EnginePlan`).
- **Owning crate** — `semstrait-adapter`.
- **`emit` as sub-form.** `emit` (00 §4.2) is the name for `adapt`'s output when the adapter is SQL-emitting — it returns `EngineArtifact::Sql(SqlArtifact)`. `emit` is not a separate pipeline stage; it is the specialization of `adapt` for the SQL variant, kept as a named handle so SQL-emission specifics have an anchor in the adapter docs (`36`).
- **Variant choice.** The adapter implementation's type determines which `EngineArtifact` variant(s) it can produce:
  - Most adapters are **single-variant**: either always `Sql` (e.g. DuckDB, Spark SQL emitters) or always `Plan` (e.g. a pure Substrait adapter).
  - A **multi-variant** adapter is permitted: it may return `Sql` or `Plan` per call based on its own decision logic (e.g. Substrait for supported shapes, SQL fallback otherwise). Every adapter MUST produce at least one variant; the `Sql`-only and `Plan`-only cases are the common ones.
  - The caller chooses the adapter; the adapter choose the variant. 10 does not prescribe which adapter produces which variant — that is an adapter-implementation concern documented per-adapter in `36`.
- **Invariants upheld** —
  - I1 — no raw SQL in the canonical layer. SQL text is produced **only** inside `adapt` (in the `Sql` variant), at the adapter-to-engine boundary. The SemanticPlan input never contains SQL text.
  - I2 — physical types appear only inside the adapter (if at all). The `SemanticPlan` input uses canonical `DataType` only; any translation to engine-specific physical types happens inside `adapt` and is invisible to upstream layers.
  - I3 — engine-specific behavior is confined to the adapter. `adapt` is the one place engine identity drives code paths.
  - I4 — input is canonical IR; output is explicitly engine-targeted (the `EngineArtifact` boundary is where I4 stops applying).
  - I6 — synchronous; no `.await`.
- **Error type** — `AdaptError`. Typical variant families (exact list ratified in `36`):
  - `UnsupportedFeature` — the `SemanticPlan` uses a feature the target engine / adapter does not support (specific node kind, specific `CanonicalFn`, specific grain).
  - `DialectUnsupported` — for SQL adapters, the selected `DialectId` does not correspond to a known dialect.
  - `AdaptationFailed` — the adapter's internal transformation failed (pass-specific payload preserved).
  - `EmitFailed` — SQL text generation failed (`Sql` variant path).
- **Error policy** — `fail-fast`.
- **I/O permitted** — none. `adapt` is a pure transformation; network/filesystem I/O to the target engine is the caller's concern, **not** part of `adapt`.
- **Sync/async** — `sync`. I6 hot path.
- **EngineAdapter interaction** — this stage IS the adapter. There is no "injected" adapter here — the adapter owns the call. If the adapter also ran in injection mode during `plan` / `optimize`, the plan has already been shaped; `adapt` is either a thin wrapper (for near-canonical substrait-compatible adapters) or a full conversion/emission (for pure-conversion adapters).
- **Forward-refs** — `36` (`AdaptError` variants, per-adapter trait surface, `emit` SQL-specific rules, dialect handling, adapter classification), `35` (`EngineArtifact` shape).

## 4. Crate Ownership Matrix

This section cements the layering implied by §3's per-stage Owning-crate fields. It is the pipeline-level view of which crate is the entry point for which verb. Per-crate public surface is ratified in the `3x` docs.

| Stage | Owning crate | Role |
|---|---|---|
| `parse` | `semstrait-model` | owns the Model specification and YAML parsing |
| `validate` | `semstrait-model` | Preconditions are model-level rules, co-located with the Model spec; `validate` is a pure predicate over `SemanticModel` |
| `compile` | `semstrait-manifest` (orchestrator) + `semstrait-catalog` (metadata I/O) + `semstrait-model` (AST source) | produces the `Manifest` from a validated `SemanticModel`: fetches catalog metadata, resolves references and names, expands globs, compiles `Expr`s, builds indices |
| `plan` | `semstrait-planner` | constructs the `SemanticPlan`; may call injected `EngineAdapter` hooks |
| `optimize` | `semstrait-planner` | canonical optimization passes; may call injected `EngineAdapter` hooks |
| `adapt` | `semstrait-adapter` | transforms `SemanticPlan` → `EngineArtifact`; `emit` is the SQL-specific form (`Sql(SqlArtifact)`) |

**Supporting crates (consumed across stages, not owners of any verb):**

- `semstrait-ir` — canonical representation types (`Expr`, `PlanNode`, `EngineArtifact`, etc.). Consumed by `compile`, `plan`, `optimize`, `adapt`.
- `semstrait-core` — **role pending ratification in `31_semstrait_core.md`**. Expected to be a shared-types crate for cross-cutting primitives (logical `DataType`, identifier types, shared error traits). `10` treats it as a supporting crate with no stage-ownership claim.

**Injection vs ownership.** `EngineAdapter` is owned by `semstrait-adapter` (for `adapt`) but can be **injected** into `semstrait-planner` to hook `plan` and `optimize` (for example, rewriting plan fragments for substrait-compatible engines that accept the canonical plan with minor adjustments, or for dialect-specific expression rewrites). This does not transfer ownership — the planner still owns the stage; the adapter contributes through the hook surface ratified in `36_semstrait_adapter.md`.

## 5. Error Model

**Internal carrier: typed per-stage error enums.** Each stage has its own Rust `enum` named `<Stage>Error`: `ParseError`, `ValidateError`, `CompileError`, `PlanError`, `OptimizeError`, `AdaptError`. Variants carry enough structured context to render a human-readable message and to be matched programmatically.

**User-facing form: `Diagnostic`.** At public API boundaries, typed errors convert into `Diagnostic` (see 00 §4.1). `Diagnostic` is a render of a typed error — it flattens type-specific payload into `{severity, location, message, source-chain}` for uniform consumption by callers that do not want to pattern-match on the internal enum. Field layout is ratified in §5.1.

**No centralized code registry.** Stages do not share a global error-code namespace. Each typed enum is self-contained. Reporting tools key on the enum variant itself; human-readable messages are produced by each enum's `Display` impl.

**Error policy per stage.** Two policies exist. Each stage commits to exactly one in its §3 contract:

- **`accumulate`** — the stage collects all independent errors in one pass and returns them together as `Vec<<Stage>Error>` (or a named wrapper). Best for author-facing passes where surfacing every problem at once improves authoring UX.
- **`fail-fast`** — the stage returns the first error and stops. Best when dependency chains make continuation unreliable (downstream work depends on the failing artifact's validity).

Uniform policy across stages:

| Stage | Policy | Rationale |
|---|---|---|
| `parse` | `accumulate` | YAML syntax errors are independent; batch-reporting improves authoring UX |
| `validate` | `accumulate` | Preconditions are independent structural checks; batch-reporting is the whole point of validate |
| `compile` | `fail-fast` | resolution, catalog I/O, and Expr compilation form dependency chains; continuing past a failure produces unreliable cascades |
| `plan` | `fail-fast` | the plan is a cohesive structure; partial plans are meaningless |
| `optimize` | `fail-fast` | same as plan |
| `adapt` | `fail-fast` | artifact production is atomic |

### 5.1 `Diagnostic` layout

`Diagnostic` is the user-facing render of a typed stage error. Layout is fixed here so every API boundary across crates surfaces errors in a uniform shape; typed internals remain crate-local.

**Struct.**

```rust
/// User-facing render of a typed stage error. Uniform across all stages.
struct Diagnostic {
    /// Stable, kebab-case identifier derived from the originating stage and
    /// enum variant by convention, e.g. "parse.unknown-top-level-key",
    /// "validate.disallowed-nesting", "compile.unresolved-reference",
    /// "plan.constraint-violation", "adapt.unsupported-feature".
    ///
    /// The `<stage>` prefix is one of: parse, validate, compile, plan,
    /// optimize, adapt. The suffix is the enum variant's kebab-case name.
    /// No central code registry; this string is a derivation, not an
    /// enumeration.
    code: String,

    severity: Severity,

    /// Where in the input the error originated. `None` is valid for errors
    /// that are context-free (e.g. a catalog-unavailable error has no
    /// source-document location).
    location: Option<Location>,

    /// Human-readable message; produced by the typed error's `Display` impl
    /// at conversion time.
    message: String,

    /// Nested causes, most-specific-first. Example: a
    /// `CompileError::ExprCompilationFailed` may carry an inner diagnostic
    /// for the underlying `FunctionRegistry::UnknownFunction`.
    source_chain: Vec<Diagnostic>,
}
```

**`Severity`.**

```rust
enum Severity {
    /// The stage cannot proceed. In fail-fast stages, the first Error
    /// aborts. In accumulate stages, Errors are collected and the stage
    /// returns all of them at end-of-pass.
    Error,

    /// The stage proceeds; the condition is surfaced to the caller but
    /// does not halt compilation or planning. Warnings MUST NOT be
    /// silently dropped at API boundaries — they travel in the same
    /// `Vec<Diagnostic>` the caller receives.
    Warning,

    /// Informational; not a problem. Used sparingly for default
    /// substitutions, auto-applied migrations, and similar caller-visible
    /// decisions the stage made.
    Note,
}
```

Initial design: every typed error variant carries `Severity::Error`. `Warning` and `Note` exist in the enum so future non-halting surfaces (e.g. deprecation notices during `parse`, coverage-gap hints during `plan`) can be added without widening the Diagnostic shape.

**`Location`.**

```rust
/// Minimal source-level location. Shape is deliberately narrow at the
/// 10 layer; richer location types (YAML line/column, expression span,
/// JSON pointer) live in the stage-owning crate and convert into this
/// form at Diagnostic construction.
struct Location {
    source: SourceId,   // identifies the document/buffer; shape in 32
    span: ByteSpan,     // byte offsets into the source
}
```

`SourceId` and `ByteSpan` exact shapes are ratified in `32` (since Model YAML is the primary source of locations). For errors originating later in the pipeline (e.g. a `CompileError` on a specific Binding), the location is derived from the originating `SemanticModel` node's recorded location.

**Conversion contract.**

```rust
/// Every typed stage error converts into a Diagnostic at the public API
/// boundary. Implementations are straightforward and mechanical:
///   - `code` = kebab-case of the enum variant, prefixed by the stage name.
///   - `severity` = Error (initial design).
///   - `location` = variant-specific span (None when context-free).
///   - `message` = Display output.
///   - `source_chain` = nested converted errors, if any.
trait IntoDiagnostic {
    fn into_diagnostic(self) -> Diagnostic;
}
```

`IntoDiagnostic` is implemented for each `<Stage>Error` enum and for `Vec<<Stage>Error>` (via a blanket impl that maps each element). Crates that publish API surface (`32`, `33`, `34`, `36`) convert at the public boundary; internal callers continue to match on the typed enum when they need to.

**Placement.** The `Diagnostic`, `Severity`, and `Location` types are **crate-layer primitives**; their canonical home is almost certainly `semstrait-core` (pending ratification in `31`). Stage crates import and construct them; they do not define competing local types.

## 6. I/O Permission Matrix

This matrix refines `00 §9 I11`. Every cell is either a permitted trait method or `∅` (forbidden).

| Stage | `CatalogProvider` | `FileSystem` | `Repository` | `EngineAdapter` (injected) |
|---|---|---|---|---|
| `parse` | ∅ | ∅ | ∅ | ∅ |
| `validate` | ∅ | ∅ | ∅ | ∅ |
| `compile` | all methods (metadata fetch, schema lookup, drift-check helpers) | read/list (for glob expansion of source bindings) | ∅ | ∅ |
| `plan` | ∅ | ∅ | ∅ | planner-hook subset ratified in 36 |
| `optimize` | ∅ | ∅ | ∅ | planner-hook subset ratified in 36 |
| `adapt` | ∅ | ∅ | ∅ | terminal `adapt` method (owned by the adapter implementation — not "injected"; this is the adapter's own call) |

**Out-of-pipeline I/O entries** (per I11, outside every `§3.x` stage):

- `Repository::load` — fetching a Manifest from persistent storage before the first `Request`. Awaited before `plan` begins. Not a pipeline stage.
- `CatalogProvider::check_schema_drift` — narrow drift validation against a previously-compiled Manifest. Awaited before `plan` begins. Not a pipeline stage.

Both entries are explicit, synchronous from the caller's perspective, and outside the `plan → optimize → adapt` synchronous chain.

**Deferred.** Per-trait method-level sub-tables (which specific `CatalogProvider` / `FileSystem` / `EngineAdapter` methods each stage may call) are blocked on finalizing those trait surfaces in `36` and `37`. Once those docs ratify their trait signatures, this section will be extended with tables keyed on method names. Until then, the trait-level matrix above is authoritative: any method on a trait permitted in a stage is in-scope for that stage; any method on a trait marked `∅` is out of scope entirely.

## 7. Compile-Time vs Query-Time Boundary

The pipeline is split into two phases with distinct posture:

- **Compile-time phase** — `parse` → `validate` → `compile`. Async permitted at `compile` (catalog / filesystem I/O). Produces the Manifest. Runs once per Model revision.
- **Query-time phase** — `plan` → `optimize` → `adapt`. Fully synchronous (I6). Runs once per `Request`. Consumes the Manifest by reference.

The boundary artifact is the `Manifest`. Once it is in memory (either freshly compiled or loaded via `Repository::load`), the query-time phase is guaranteed synchronous and I/O-free except for the two I11 out-of-band entries listed in §6.

```mermaid
flowchart TD
    subgraph P1["Compile-time phase (async permitted; once per Model revision)"]
        direction LR
        Y[Model YAML] --> parse(parse)
        parse --> SM[SemanticModel]
        SM --> validate(validate)
        validate --> compile(compile)
        compile --> M[(Manifest - fresh)]
        CP{{CatalogProvider}} -.->|async| compile
        FS{{FileSystem}} -.->|async| compile
    end

    subgraph OB["Out-of-band I/O (outside pipeline stages; awaited before plan)"]
        direction LR
        M -.->|Repository persist| STORE[(persistent store)]
        STORE -.->|Repository load| MEM[(Manifest - in memory)]
        MEM -.->|optional| DRIFT{{CatalogProvider check-schema-drift}}
    end

    subgraph P2["Query-time phase (synchronous - I6; once per Request)"]
        direction LR
        REQ[Request] --> plan(plan)
        MEM --> plan
        plan --> optimize(optimize)
        optimize --> adapt(adapt)
        adapt --> EA[EngineArtifact]
    end

    M -.->|same-process fast path| MEM
```

**Notes on the diagram.**

- The boundary artifact is the `Manifest`. Everything upstream of it (parse / validate / compile, plus the permitted I/O at compile) is Phase 1. Everything downstream (plan / optimize / adapt) is Phase 2 and is strictly synchronous.
- Two handoff paths into Phase 2 exist:
  - **Fresh compile path** — the Manifest produced by a just-completed compile is used directly, no serialization or I/O in between. Shown as the dashed `same-process fast path` edge.
  - **Load path** — a previously-persisted Manifest is fetched via `Repository::load` (dashed), optionally followed by `CatalogProvider::check_schema_drift`. Both are awaited before `plan` begins.
- The Repository persist / load and the drift check are the **only** two I/O touchpoints outside the pipeline stages (I11). They are not stages; they are framing operations the caller performs around the pipeline. They may be async from the caller's perspective, but from the pipeline's perspective, they complete before `plan` begins, preserving the synchronous Phase 2 chain.
- `CatalogProvider` appears twice in the diagram: once as the compile-time metadata source (inside Phase 1), and once as the out-of-band drift checker (between phases). These are the same trait; different call sites with different method signatures (ratified in `37`).

## 8. Sync/Async Posture

| Stage | Posture | Justification |
|---|---|---|
| `parse` | sync | pure transformation |
| `validate` | sync | pure predicate over in-memory `SemanticModel` |
| `compile` | **async** | network / filesystem I/O via `CatalogProvider` / `FileSystem` |
| `plan` | sync | I6 hot path |
| `optimize` | sync | I6 hot path |
| `adapt` | sync | I6 hot path |

The async boundary is exactly one stage: `compile`. Everything downstream is synchronous from the moment the Manifest is produced. The two I11 out-of-band I/O entries (`Repository::load`, `CatalogProvider::check_schema_drift`) are awaited **before** `plan` begins and do not break the `plan → optimize → adapt` synchronous chain.

## 9. Non-Goals / Deferred

- Per-stage public API shape (deferred to `3x`).
- Internal algorithm detail for `plan` and `optimize` (deferred to `20`–`25`, `34`).
- Adapter hook surface specification (deferred to `36`).
- Streaming / incremental variants of any stage (not in the initial design; re-evaluate post-`42_migration_notes.md`).
- Parallelism / concurrency at stage boundaries (each stage is a single-threaded function in the initial design; orchestration is caller's concern).
- Multi-file / include-directive model support (initial design is single-file Model; future extension).

## 10. Cross-References

- `00_overview.md §5` — pipeline diagram and verb vocabulary (parent).
- `00_overview.md §9 I6, I11` — invariants refined by this doc.
- `11`–`17` — structural specifications of inputs and outputs.
- `20`–`25` — DataKind strategy dispatch inside `plan`.
- `31` — `semstrait-core` role (pending).
- `32`–`37` — per-crate public surface for each stage.
