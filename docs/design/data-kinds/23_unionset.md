---
prereqs: [20, 13, 14, 15, 16, 17]
authoritative-for:
  - the `Unionset` ComplexDataKind variant shape and its `CompositionKind::Unionset` discriminator contract with `16`
  - the `UnionMode` enum (`All`, `Distinct`) and its default
  - per-child declaration shape: child-reference by canonical `DataKindRef`, per-child Coverage declaration, coverage-inference rules
  - `UnionsetStrategy` — the planner strategy that emits `PlanNode::Union` over per-child subplans, with NULL-fill projection and column-type reconciliation
  - per-child NULL-fill projection semantics — the `Project[Cast(Null, T) AS name]` shape a child's branch carries for each composed-surface field the child does NOT provide
  - column-type reconciliation rules for the unified Union output (pass-through when child types agree per `14 §5.6`; explicit `Cast` layer per `13 §7` when types widen across children)
  - Coverage-driven branch pruning: a child whose every requested field is `NullFill` is pruned from the Union (advisory `PLAN_W_2301`)
  - re-aggregation after Union (shared with `22 Grainset` per `20 §5`) — the Unionset-specific rules for deciding when a terminal `Aggregate` follows the Union
  - `TemporalShape`-alignment advisory rules for Unionset children (forward-refs `17`)
  - `Grain`-alignment advisory rules for Unionset children and post-Union rollup shape
  - validation Preconditions `VALID_E_2300`–`2399` (empty-children, single-child, self-reference, cyclic nesting, conflicting shape fields, union-mode misuse)
  - compile Preconditions `COMP_E_2300`–`2399` (child-reference resolution, per-child Coverage consistency, cross-child type-reconciliation failures, composition-level coverage-completeness)
  - plan-stage rules `PLAN_E_2300`–`2399` (request references a Semantics not covered by any child; fanout interactions; re-aggregation infeasibility)
refined-by:
  - 25 (applicability matrix — per-variant strategy-consumption cells for Unionset)
  - 30 (`apis/30_api_contracts.md` — final stable placement of `VALID_E_23xx` / `COMP_E_23xx` / `PLAN_E_23xx` in the cross-subsystem code-range table)
  - 33 (`apis/33_semstrait_manifest.md` — `ResolvedUnionset` struct roster and persistence)
  - 34 (`apis/34_semstrait_planner.md` — `UnionsetStrategy` trait surface, re-aggregation helper)
  - 35 (`apis/35_semstrait_ir.md` — `PlanNode::Union` field roster)
---

# 23. Unionset

> **Reconciliation (Phase-3, 2026-04-17).** The v1 authoring-layer canonical shape for `Unionset` is ratified across:
>
> - [`../apis/32_semstrait_model.md §3`](../apis/32_semstrait_model.md) — top-level YAML tag (`unionsets:`), `UnionsetBody` struct shape.
> - [`../foundations/18_entities.md`](../foundations/18_entities.md) — shared entity types consumed by `UnionsetBody`.
> - **`UnionMode` v1 roster** is `{All, Unique}`, `#[non_exhaustive]`, default `All`. `UnionMode::Distinct` from this doc's authoring was renamed to `Unique` and `Q-UNI-002` (Distinct activation) is auto-closed. See `18 §2` adjacency references.
> - [`26_nesting_matrix.md`](./26_nesting_matrix.md) — nesting rules. Notably **R3** (every `ComplexDataKind` requires ≥ 2 children, auto-closing `Q-UNI-009`).
> - `union_mode` is authored as a direct field on `UnionsetBody` (not inside `extras`) per `32 §4.2`.
>
> This document retains authority for:
>
> - `UnionsetStrategy` plan-shape contract (per-child NULL-fill projection, column-type reconciliation, branch pruning).
> - Coverage-driven branch-pruning rules and the `PLAN_W_2301` advisory.
> - Re-aggregation-after-Union rules (shared with `22 Grainset` per `20 §5`).
> - `VALID_E_23NN` / `COMP_E_23NN` / `PLAN_E_23NN` error-code allocations.
>
> Rust-struct and YAML-surface body sections predate `18` (formerly `32c`); read them as historical. `Distinct` in body text = `Unique` in v1; `ColumnMapping` → `SemanticMapping` rename per `18 §10`.

---

## Table of Contents

1. [Purpose and Scope](#1-purpose-and-scope)
2. [The `Unionset` variant](#2-the-unionset-variant)
3. [Child declaration](#3-child-declaration)
4. [UNION ALL Strategy (`UnionsetStrategy`)](#4-union-all-strategy-unionsetstrategy)
5. [Coverage semantics](#5-coverage-semantics)
6. [Interaction with `TemporalShape`](#6-interaction-with-temporalshape)
7. [Interaction with `Grain`](#7-interaction-with-grain)
8. [Validation Preconditions](#8-validation-preconditions)
9. [Compile Preconditions](#9-compile-preconditions)
10. [Plan-stage rules](#10-plan-stage-rules)
11. [Worked example](#11-worked-example)
12. [Round-1 open items](#12-round-1-open-items)
13. [Cross-references](#13-cross-references)

---

## 1. Purpose and Scope

### 1.1 What `23` ratifies

`23` is the per-DataKind specification for the `Unionset` variant of `ComplexDataKind`. It closes three concerns that `20` (taxonomy) and `16` (composition) hand off to the per-variant docs:

- **(a) UNION ALL semantics with per-child Coverage.** The Unionset composes `N ≥ 2` children (Simple or Complex, per the `12 §2` nesting matrix) into a single queryable surface whose rows are the vertical concatenation of the children's rows, deduplicated or not per `UnionMode`. `23` fixes how the planner constructs that concatenation and how per-child Coverage (from `15 §6` at the Binding level; lifted to the composition level by `16 §8`) drives the per-child branch shape.
- **(b) NULL-fill for Semantics a child does not provide.** When a composed-surface field's `FieldOwnership` (per `16 §7.3.3`) is `NullFill` for a given child, the child's branch must emit a typed NULL constant at the join-column position so the UNION ALL's columns align. `23` ratifies the projection shape, its logical typing (via the unified column type from §4.4), and its `PhysicalExpr` form in the emitted `PlanNode`s.
- **(c) Per-child type-compatibility rules.** Children may expose the same Semantics with physical types that agree exactly, widen, or disagree. `23` ratifies the reconciliation rules at the composition level, in conjunction with `14 §5.6`'s expression-level pass-through typing and `13 §7`'s cast matrix.

Concretely, `23` ratifies:

- **§2** — the `Unionset` variant's Rust-level shape (Model-layer declaration, Manifest-layer `ResolvedUnionset`, composition tag per `16 §5.3`).
- **§3** — the YAML-surface shape for child entries and per-child Coverage overrides.
- **§4** — `UnionsetStrategy`: the planner strategy that emits `PlanNode::Union` over per-child subplans produced by each child's own Strategy (delegation per `20 §5`).
- **§5** — Coverage semantics at the Unionset level, including the fold from per-child Binding Coverage to per-child composition-level `FieldOwnership`.
- **§6** — `TemporalShape`-alignment advisory rules across children.
- **§7** — `Grain`-alignment advisory rules and the post-Union rollup shape.
- **§§8–10** — the three code-range allocations: `VALID_E_2300`–`2399` (validate), `COMP_E_2300`–`2399` (compile), `PLAN_E_2300`–`2399` (plan).
- **§11** — a worked example (heterogeneous event sources across time windows).

### 1.2 What `23` does NOT ratify (forward-refs)

- **The `ComposedSemanticInterface` shape itself.** Owned by `16 §5`. `23` consumes `CompositionKind::Unionset`; it does not redefine the composed-interface struct.
- **Per-child `Binding` resolution.** Owned by `15`. A Unionset child references a top-level or nested `DataKind`; that child's own `ResolvedBinding` (if `Simple`) or its child-of-a-child bindings (if `Complex`) are resolved by their respective specs (`21`–`24`) and `15`'s flow.
- **`PlanNode::Union` field roster.** Owned by `35`. `23` stipulates the planner emits `PlanNode::Union { distinct, inputs, ... }` and reads back the fields it populates; the exact roster is `35`'s.
- **Re-aggregation inference.** The "infer a re-aggregation function per measure" behavior (SUM over COUNT, etc.) is shared between Unionset and Grainset; the canonical rule-set lives in `20 §5` (taxonomy) and `22`'s Grainset-specific doc. `23 §4.5` cross-refs; it does not re-ratify the mapping table.
- **Authored YAML grammar details.** Top-level `unionsets:` grammar is ratified in `32`. `23 §3` describes the canonical-layer shape sufficiently for the planner and Manifest; `32` is authoritative on the YAML spellings.
- **`TemporalShape` planner-side logic.** `17` is a parallel draft in this wave; planner support for shape-aware planning is `17`'s purview. `23 §6` lists advisory-warning cases the Unionset-level checker emits, forward-referencing `17` for the `TemporalShape` vocabulary itself.

### 1.3 Design posture

`23`'s posture is **delegate per-child, reconcile at the seam**:

- **Delegate.** Each child's subplan is produced by the child's own Strategy — Simple children route through `21`'s `SimpleStrategy`, Grainset children through `22`'s `GrainsetStrategy`, Joinset children through `24`'s `JoinsetStrategy`. The Unionset does not reach inside a child's construction; it treats every child as a sealed producer of a `PlanNode` subtree whose output columns match a contract `UnionsetStrategy` negotiates.
- **Reconcile at the seam.** The UNION ALL seam is where column alignment, NULL-fill, and type reconciliation happen. `UnionsetStrategy` owns the projection wrapping that transforms each child's subplan output into the unified column roster. The seam is the only place the Unionset intervenes in a child's plan.
- **Per-child Coverage as the decision driver.** Every branch-shape decision — which child contributes which column Natively, which Derives, which NullFill-projects — reads the composition-level `CompositionCoverage` (per `16 §8`), which is itself a fold of per-child Binding `Coverage`s (per `15 §6.2`). `23` never re-computes coverage; it consumes the pre-built index.

### 1.4 Reference material — peer-group positioning

- **Cube.js: `unionAll: [...cubes]`.** Cube.js's closest direct analog. Semantically aligned: Cube authors a cube whose rows are the UNION ALL of several member cubes, with Cube auto-generating NULL-fills for columns missing on a member. Differences: Cube has no `UNION DISTINCT` mode at the cube level, and Cube does not declare per-child Coverage — it infers coverage from the member's schema. semstrait's `UnionMode::Distinct` and explicit per-child `Coverage` declarations are both additions.
- **dbt MetricFlow:** no direct Unionset analog. MetricFlow handles UNION via SQL — authors write a custom `data_source` whose SQL is the UNION. semstrait's I1 forbids raw SQL, so the Unionset is a first-class DataKind variant that the planner emits as `PlanNode::Union` (non-textual).
- **LookML:** no direct analog. Views declare `sql_table_name:` or `derived_table:` with SQL; UNION is expressed in the SQL. Same I1 constraint as MetricFlow.
- **Query-engine IRs (DataFusion, Substrait):** `PlanNode::Union` is structurally borrowed from these. `23`'s semantic layer over that node — Coverage-driven NULL-fill, union-mode selection, post-Union re-aggregation — is `23`'s own; engine IRs do not carry Coverage or a Semantics roster.

### 1.5 Guardrails — how `23` upholds `00 §9` invariants

| Invariant | Where `23` keeps it |
|---|---|
| **I1** — no raw SQL in the canonical layer | UnionsetStrategy emits `PlanNode::Union` (§4.1); NULL-fill projections carry `PhysicalExpr(Cast(Null, T))` per `14`'s tree shape (§4.3); no SQL text anywhere in a Unionset's Manifest or SemanticPlan. |
| **I2** — logical types only | §4.4's column-type reconciliation uses canonical `DataType` (per `13 §2`); no Arrow / Spark / engine types in the Unionset's resolution path. |
| **I3** — no engine branching | Zero engine-identity checks in `23`. Every decision reads either Manifest indices or the composition's `CompositionCoverage`. |
| **I4** — Manifest determinism | Child ordering is author-declared (YAML list order); the resolved `ResolvedUnionset` preserves that order. NULL-fill projections are emitted per a deterministic walk over the composed surface's `UnifiedSemantics` (sorted per `16 §6`). |
| **I5** — resolution at compile | Every child reference is resolved at `compile`. The planner's `UnionsetStrategy` walks `ResolvedUnionset.children` — no lookup, no name resolution, no catalog calls. |
| **I8** — Manifest is planner-complete | `ResolvedUnionset` holds a `ComposedSemanticInterface` (from `16`), the per-child `DataKindRef` list, per-child Coverage overrides (if any), and the `UnionMode`. No further resolution at plan time. |
| **I10** — non-exhaustive public sum types | `UnionMode` and every `23`-owned error-variant enum are `#[non_exhaustive]`. Adding `UnionMode::Intersect` or a new `PLAN_E_23xx` variant is MINOR. |
| **I12** — first-class diagnostics | Every precondition emits a `Diagnostic` with a stable code from the `2300`–`2399` sub-range (§§8–10). No raw `String`-typed errors anywhere in `23`'s contract. |

---

## 2. The `Unionset` variant

### 2.1 Model-layer shape

At the Model layer (post-`parse`, pre-`compile`), a Unionset is one variant of `ComplexDataKind`:

```rust
// Model-layer variant roster (per `20 §3`):
#[non_exhaustive]
pub enum ComplexDataKind {
    Unionset(UnionsetDecl),
    Grainset(GrainsetDecl),
    Joinset(JoinsetDecl),
}

#[non_exhaustive]
pub struct UnionsetDecl {
    /// Canonical name; top-level DataKind identity per `11 §5.1`.
    pub name: DataKindName,

    /// The Unionset's own `SemanticInterface`. Per `12 §3.3`, children
    /// do NOT declare Semantics — the Unionset's top-level interface is
    /// authoritative. Dimensions / Measures / Metrics / Filters / Keys
    /// live here.
    pub interface: SemanticInterface,

    /// Children — at least 2 (per `12 §3.2` and §8.1 `VALID_E_2301`).
    /// Mixed-shape children permitted: Simple / Grainset / Joinset per
    /// `12 §2`; another Unionset is forbidden (same-kind self-nesting
    /// ban per `12 §2.2` → `ParseError::IllegalNesting`).
    pub children: Vec<UnionsetChildDecl>,

    /// UNION ALL vs UNION DISTINCT. Defaults to `UnionMode::All`.
    pub mode: UnionMode,
}

#[non_exhaustive]
pub enum UnionMode {
    /// UNION ALL semantics — preserve duplicates across children;
    /// the planner emits `PlanNode::Union { distinct: false, ... }`.
    All,

    /// UNION DISTINCT semantics — deduplicate across children; the
    /// planner emits `PlanNode::Union { distinct: true, ... }`.
    Distinct,
}
```

The enum is `#[non_exhaustive]` per I10: future modes (`UnionByName` for name-keyed alignment, `Intersect` as a MINOR) may land without breaking authors.

### 2.2 Manifest-layer shape

At the Manifest layer (post-`compile`), the resolved counterpart materializes the composed interface (per `16 §5.5`'s "explicit compositions are materialized" rule):

```rust
#[non_exhaustive]
pub struct ResolvedUnionset {
    /// Identity — assigned from the Manifest's `DataKindId` counter
    /// (per `11 §5.1` / `20`). Stable within a Manifest per I4.
    pub data_kind_id: DataKindId,

    /// Author-declared canonical name.
    pub name: DataKindName,

    /// The pre-built composed interface. `16 §5` ratifies the shape;
    /// `23` is the consumer.
    pub composed_interface: ComposedSemanticInterface,
    // invariant: composed_interface.composition_kind == CompositionKind::Unionset

    /// Per-child reference + per-child Coverage override (if any).
    /// Ordering is author-declared; preserved by I4.
    pub children: Vec<ResolvedUnionsetChild>,

    /// Union mode (carried verbatim from the Model).
    pub mode: UnionMode,
}

#[non_exhaustive]
pub struct ResolvedUnionsetChild {
    /// The child DataKind. Resolved at `compile`; may be a Simple or
    /// a nested Complex per the `12 §2` matrix (not another Unionset).
    pub data_kind_ref: DataKindRef,

    /// Per-child explicit Coverage override (see §3.2). If `None`,
    /// the planner uses the inferred Coverage from the child's own
    /// interface (§5.2).
    pub coverage_override: Option<ChildCoverageOverride>,
}
```

Per `16 §10`, `ResolvedUnionset.composed_interface` is built at `compile` and stored in the Manifest alongside the `ResolvedUnionset`. The exact placement — whether on the `ResolvedUnionset` struct directly (as above) or in a sibling `Manifest::composed_interfaces` index — is a `33` implementation choice; `23`'s contract surface is the struct as shown.

### 2.3 Composition tag contract with `16`

`ResolvedUnionset.composed_interface.composition_kind` MUST be `CompositionKind::Unionset` (per `16 §5.3`). The Unionset variant is the only site in the canonical layer that produces that tag; the planner dispatches on it.

`ResolvedUnionset.composed_interface.constituents` mirrors `ResolvedUnionset.children` in the same order; the two are kept in sync at `compile` time. Downstream code reads `composed_interface.constituents` when walking the composition and `ResolvedUnionset.children` when needing the per-child Coverage override — both reads are O(1) index lookups.

### 2.4 Identity rules

- **Name uniqueness.** `name` is unique across the Model's top-level DataKind namespace (per `11 §5.1`). A Unionset named `paid_media` cannot coexist with a Simple DataKind named `paid_media`.
- **Child uniqueness.** No child `DataKindRef` may appear twice in `children` (per §8.3 `VALID_E_2303` — `DuplicateUnionsetChild`). Authors needing "union a source with itself" should instead model a single Simple with the appropriate Binding sources (`15 §3.5`'s glob expansion).
- **No self-reference.** The Unionset's own `DataKindRef` cannot appear in `children` (per §8.4 `VALID_E_2304` — `UnionsetSelfReference`). Cyclic chains (Unionset A → Grainset B → Unionset A) are caught by the cycle-detection pass at compile time (§9.2 `COMP_E_2301`).

---

## 3. Child declaration

### 3.1 YAML surface (canonical-layer view)

Per `12 §3.1`, children are grouped by their kind-container key (`datasets:`, `grainsets:`, `joinsets:`). For `23`'s canonical-layer ratification the grouping is immaterial — what the Unionset carries is a flat `Vec<UnionsetChildDecl>` of references (each tagged implicitly with the child's kind via its `DataKindRef`). The YAML-surface grouping is a parse-site convenience; `32` ratifies the authoring grammar.

```yaml
# Authored YAML (shape per `32`). Children are grouped by kind container;
# semstrait flattens the groups into a single ordered `children: Vec<...>`.
unionsets:
  - name: paid_media
    mode: all                    # default; explicit for clarity
    dimensions:
      - name: date
      - name: source_platform     # single platform column surfaced by the Unionset
      - name: campaign_id
    measures:
      - name: cost
        agg: sum
      - name: impressions
        agg: sum
    datasets:                    # Simple children
      - ref: adwords_daily       # reference to a top-level or nested Simple
      - ref: facebook_daily
    grainsets:
      - ref: tiktok_rollup       # a Grainset child (legal per `12 §2`)
```

**Rules the canonical layer enforces:**

- Each child entry is a reference (`ref:`) to a canonical `DataKindRef`, not an inline definition. Inlining is permitted by `12 §2` for structural children, but `23` treats the canonical-layer Unionset's children uniformly as refs: inline-declared children are hoisted to their owning scope and referenced by name at `compile` time (per `11 §10`'s structural labels).
- At least 2 children across all groups combined (§8.1 `VALID_E_2301`). A single-child Unionset is a `ValidateError`; authors should replace it with the child directly.
- Another Unionset as a child is a `ParseError::IllegalNesting` (`12 §2.2`), caught upstream of `23`.

### 3.2 Per-child Coverage declaration

Per-child Coverage is the central decision input for Unionset's NULL-fill emission (per `16 §8` and §5 below). Two authoring modes are permitted:

**(1) Inferred Coverage.** Default. No `coverage:` block on the child entry. `compile` derives the child's Coverage-at-composition-level from the child's own `ResolvedBinding.coverage` fold (§5.2).

**(2) Explicit Coverage override.** Advanced. The author declares, per-child, the set of composed-surface Semantics the child provides Natively / Derives / NullFills. Used when the inferred Coverage is too permissive (e.g. the child's Binding has a column named `source` that the author does not want surfaced as the Unionset's `source` Semantics — the override forces NullFill at the composition level).

```yaml
# Authored YAML with explicit per-child Coverage override:
unionsets:
  - name: paid_media
    mode: all
    dimensions: [date, source_platform, campaign_id]
    measures: [{ name: cost, agg: sum }]
    datasets:
      - ref: adwords_daily
        coverage:
          provides: [date, campaign_id, cost]
          # `source_platform` omitted → explicit NullFill
      - ref: facebook_daily
        coverage:
          provides: [date, source_platform, campaign_id, cost]
```

The canonical-layer shape of the override:

```rust
#[non_exhaustive]
pub struct ChildCoverageOverride {
    /// SemanticsNames the child natively provides (or Derives from an
    /// internal expression). Everything NOT in this set is forced to
    /// `FieldOwnership::NullFill` at the composition level for this
    /// child, regardless of what the child's own interface exposes.
    pub provides: BTreeSet<SemanticsName>,
}
```

**Subsumption rule.** An explicit `provides` set MUST be a subset of the child's own exposed interface (§9.3 `COMP_E_2303` — `ChildCoverageOverridesUnexposed`). Authors cannot claim a child provides a Semantics the child does not even declare.

**Interaction with Binding-level NullFill.** Binding-level `Coverage::NullFill` on a child (e.g. a multi-source Simple child whose sources partially agree) is orthogonal. The composition-level `provides` set is layered ON TOP of the Binding-level fold — if a Binding-level fold already yields `NullFill` for a Semantics, the composition-level Coverage is `NullFill` regardless of whether `provides` lists the name (since NullFill-at-Binding means "no source has it"). Details in §5.3.

### 3.3 Child-DataKind kinds admitted

Per the `12 §2` matrix:

| Child kind | Admitted by Unionset? | Notes |
|---|---|---|
| `Simple` (Dataset) | ✓ | Most common case. Each `Simple` contributes its single `ResolvedBinding`; Coverage is read directly. |
| `Unionset` | ✗ | Same-kind ban (§12 §2.2); `Union(Union(a, b), c) ≡ Union(a, b, c)`. Flatten at authoring. |
| `Grainset` | ✓ | A Grainset child exposes a pre-composed `ComposedSemanticInterface` of its own (per `22`'s spec, parallel draft). The Unionset treats it as a sealed producer: the Grainset's `GrainsetStrategy` handles level selection internally, and `UnionsetStrategy` branches on the output. |
| `Joinset` | ✓ | Same delegation posture as Grainset; the Joinset's `JoinsetStrategy` produces the branch subplan. |

When the child is Complex (Grainset or Joinset), the composition-level Coverage fold (§5.3) reads the child's already-composed `FieldProvenance` (per `16 §7`) rather than a raw Binding `Coverage`. The fold rule is uniform across Simple and Complex children.

### 3.4 Child ordering

`ResolvedUnionset.children` preserves YAML author-declared order. The planner emits per-child branch subplans in this order (§4.1). Ordering is semantically significant for three downstream concerns:

- **Determinism.** Author-declared order ensures byte-identical SemanticPlans across runs (I4).
- **Column-type reconciliation "first-wins" policy.** When two children disagree on a column type that both claim to provide Natively, the first child's type is the canonical target and later children are Cast to it (§4.4). The policy is stable and deterministic; authors who want a different reconciliation target simply reorder the children.
- **Diagnostic context.** Error messages cite children by index (`child_index: usize`) as well as name; the index matches the declared order.

---

## 4. UNION ALL Strategy (`UnionsetStrategy`)

`UnionsetStrategy` is the planner strategy dispatched for any Request whose resolved target is a `ResolvedUnionset` (per `20 §5`'s Strategy dispatch). It produces a sub-tree rooted at `PlanNode::Union`, with per-child subplans feeding into it and an optional terminal re-aggregation wrapper.

### 4.1 Emit `PlanNode::Union` over per-child subplans

The outermost decision `UnionsetStrategy` makes is emitting the `PlanNode::Union` itself. The shape is:

```text
PlanNode::Union {
    distinct: (mode == UnionMode::Distinct),
    inputs: [<child_0_subplan>, <child_1_subplan>, ..., <child_{N-1}_subplan>],
}
```

- **`distinct`** mirrors the author-declared `UnionMode` (§2.1). `All` → `distinct: false`; `Distinct` → `distinct: true`.
- **`inputs`** carries the per-child subplans in child-declared order.

**Single-child short-circuit.** The `12 §3.2` / §8.1 Precondition guarantees `|children| ≥ 2`, so a single-branch Unionset is a validation error, not a runtime case. Nevertheless, the planner's implementation carries a defensive branch: if the resolved Request-narrowed child set reduces to 1 (e.g. via Coverage-driven pruning in §4.6 — "a child whose every requested field is NullFill is pruned"), the `PlanNode::Union` is omitted and the single surviving child subplan flows directly into the terminal wrapper (§4.5). This is a planner optimization; the Manifest-layer `ResolvedUnionset` is unaffected.

**Zero-child post-prune.** If Coverage-driven pruning reduces the surviving child set to 0 (every child's entire contribution is NullFill for the Request's selected fields), the strategy emits `PLAN_E_2303 UnionsetRequestTotallyNullFilled` (§10.3). This is exceptional; it means the Request asks for Semantics the Unionset declares but no child covers — a Manifest-inconsistency that §9.5 catches at compile time in most cases, surviving to plan time only when the Request's field set is a pathological subset.

### 4.2 Each child's subplan is produced by the child's own Strategy

Per `20 §5`, the planner dispatches Strategy based on the DataKind variant. A Unionset child is itself a DataKind; its subplan is produced by its own Strategy:

```text
UnionsetStrategy::plan(request, resolved_unionset, manifest) {
    // Rewrite the incoming Request for each child's scope.
    per_child_requests = narrow_request_per_child(request, resolved_unionset)

    // Delegate to each child's Strategy.
    per_child_subplans = []
    for i in 0..resolved_unionset.children.len():
        child_ref = resolved_unionset.children[i].data_kind_ref
        child_request = per_child_requests[i]
        child_strategy = dispatch_strategy(manifest, child_ref)  // per `20 §5`
        child_subplan = child_strategy.plan(child_request, child_ref, manifest)
        // Wrap with Unionset's seam: NULL-fill + column reconciliation.
        branch_subplan = wrap_for_union(child_subplan, i, resolved_unionset, request)
        per_child_subplans.push(branch_subplan)

    // Combine.
    union_node = PlanNode::Union {
        distinct: resolved_unionset.mode == UnionMode::Distinct,
        inputs: per_child_subplans,
    }

    // Optional terminal wrapper.
    finalize(union_node, request, resolved_unionset)
}
```

The delegation matches `20 §5`'s Strategy-dispatch rule exactly: the Unionset knows nothing about how a Simple child scans its Binding sources, how a Grainset child selects a level, or how a Joinset child sequences its relationship walks — each child's Strategy encapsulates that work.

**Per-child Request narrowing (`narrow_request_per_child`).** The Unionset rewrites the incoming Request for each child's scope:

1. **Filter on covered fields.** Fields on the Request's `select:` that the child does not cover (composition-level `FieldOwnership::NullFill` per §5) are dropped from the child's version of the Request — the child does not "know about" the field, and its subplan does not scan for it. NULL-fill is injected at the wrap-for-union seam (§4.3), not inside the child.
2. **Preserve grouping invariants.** Any `Dimension` the Unionset's top-level Request groups by is either (a) covered by the child, in which case it's requested from the child; (b) NullFill for the child, in which case the child's branch emits a `CAST(NULL AS T)` expression at the seam, and the grouping at the post-Union Aggregate (§4.5) groups on that NULL value. The NULL-valued group row is semantically correct: the child contributes rows whose value for the dimension is "unknown."
3. **Filters on NullFill-ed dimensions.** A Request-level filter on a Semantics NULL-filled for a particular child is logically unsatisfiable on that child's rows (since the child's contribution always has NULL there, and `NULL = <value>` is FALSE in three-valued logic). The planner has two options: (a) push a `WHERE FALSE` clause into the child's subplan (trivially eliminating its contribution); (b) skip the child entirely via the §4.6 pruning advisory. §4.6 takes option (b) when the filter is the sole reason the child contributes nothing; for complex filter expressions that partially depend on NullFill and partially on Native columns, (a) is the fallback.

### 4.3 NULL-fill projection

For each composed-surface Semantics `s` that a child `i` does NOT provide (`FieldOwnership` for `(constituent_i, s)` is `NullFill`; see §5), the child's branch subplan must project a typed NULL at the join-column position. The `wrap_for_union(child_subplan, i, ...)` step adds a `PlanNode::Project` that carries both (a) the child's covered columns and (b) typed-NULL constants for the not-covered columns.

**Projection shape.** For each requested composed-surface Semantics `s`, the branch-`Project` entry is:

| `FieldOwnership` for `(child_i, s)` | Projection expression |
|---|---|
| `Native(child_i)` | `PhysicalExpr::Column("<s_child_side_name>")` — the child's own column name for `s`, resolved from the child's `ResolvedColumnMapping` or composed `FieldProvenance`. |
| `Shared([..., child_i, ...])` | Same as `Native` — the child contributes its own column, and the UNION ALL aligns the semantically-equivalent columns from every contributor. |
| `NullFill([providers])` (child_i is NOT in providers) | `PhysicalExpr::Cast(PhysicalExpr::Literal(LiteralValue::Null), unified_column_type(s))` — a typed NULL. The `unified_column_type(s)` is the composition-level reconciled type per §4.4. |
| `Derived(expr)` (composition-level) | `PhysicalExpr::Cast(PhysicalExpr::Literal(LiteralValue::Null), unified_column_type(s))` — the `Derived` variant computes at the composition level, so the child's branch simply emits NULL at the join position; the terminal wrapper (§4.5) applies the `Derived` expression post-Union. |

The typed NULL is carried as a `PhysicalExpr` tree (per `14 §3`) — `Cast(Null, T)` — never as a SQL string. Adapters (`36`) translate the tree into the engine's NULL-typing idiom (`CAST(NULL AS INTEGER)` in ANSI, `lit(null).cast("int")` in DataFusion, etc.).

**Column ordering.** The `wrap_for_union` step orders the projection columns according to a deterministic walk over the composed surface's `UnifiedSemantics` (alphabetical on `SemanticsName`, keys-first ordering per `16 §6`). Every child's branch Project emits columns in the same order — this is required for `PlanNode::Union` to align inputs positionally. The per-adapter SQL emission may reorder to match author expectations, but the semantic-layer contract is positional alignment on the deterministic order.

**`DISTINCT` mode interaction.** For `UnionMode::Distinct`, the NULL-fill projections DO participate in deduplication. Two rows from different children that agree on every Native column but differ in NULL-fill positions are NOT deduplicated by `UNION DISTINCT` — because NULL is not equal to NULL in standard three-valued logic (SQL `NULL = NULL` is UNKNOWN, not TRUE, and `DISTINCT` treats UNKNOWN as "not-equal"). Authors relying on DISTINCT for dedup across partial-coverage children should know this; §6 flags it as an advisory in the `TemporalShape`-mismatch context (where partial coverage is especially common).

### 4.4 Column-type reconciliation

The UNION ALL semantic requires every input in `PlanNode::Union.inputs` to expose the same column types at the same positions. When two children disagree on the type of a Semantics they both provide (Natively or via `Shared`), `UnionsetStrategy` reconciles at the seam.

**Unified-column-type derivation (`unified_column_type(s)`).** Per Semantics `s` on the composed surface:

1. Determine the set of contributing children: `contribs(s) = { child_i | FieldOwnership for (child_i, s) is Native or Shared }`.
2. If `|contribs(s)| == 0`, the Semantics is either `Derived` or fully-`NullFill` at the composition level. The declared Semantics `DataType` on the Unionset's own `SemanticInterface` is the unified type (since no child contributes, the author's declaration is authoritative).
3. If `|contribs(s)| == 1`, the unified type is that single child's `DataType` for `s`.
4. If `|contribs(s)| ≥ 2`:
   - **Pass-through fast path (per `14 §5.6`).** If every contributor's `DataType` for `s` is identical, no cast is needed; the unified type is that shared type. This is the common case for Unionsets of homogeneous sources.
   - **Widening reconciliation (per `13 §7`).** If contributors' types differ but all are pairwise cast-compatible under `13 §7`'s widening rules, the unified type is the least-upper-bound of the contributor types. `UnionsetStrategy` wraps each contributing child's branch `Project` in a `Cast(<child_col>, <lub>)` expression where needed. The LUB selection matches `14a`'s promotion lattice for binary operators (e.g. `Integer` + `Long` → `Long`; `Decimal(10, 2)` + `Decimal(12, 4)` → `Decimal(12, 4)` with the widest precision + scale).
   - **Incompatible types.** If any pair of contributor types is not cast-compatible under `13 §7` (e.g. `String` × `Integer` without a cast policy), `UnionsetStrategy` emits `COMP_E_2304 CrossChildTypeDisagreement` at compile time. Cross-child type-compatibility failures fail-fast during Manifest assembly (§9.4).

**"First-child-wins" default.** When multiple LUB candidates tie (a pathological case, e.g. two contributors with `Decimal(10, 2)` and `Decimal(10, 2)`), the first child's declared type is authoritative. This is a determinism tie-breaker; authors who want a specific target type should author it on the Unionset's own `SemanticInterface` `DataType:` declaration (which step 2 above promotes).

**Cast site.** The `Cast` is always wrapped around the child's own column read inside the `wrap_for_union` Project (the same Project that handles NULL-fill). A single `Project` per child carries both casts and NULL-fills:

```text
Project[
    covered_col_0 AS <unified_name_0>,
    Cast(covered_col_1, <lub_type>) AS <unified_name_1>,
    Cast(Null, <lub_type>) AS <nullfill_col_2>,
    ...
]
```

This keeps the Union's inputs homogeneous without extra plan nodes.

**Nullability reconciliation.** If contributors disagree on nullability (one reports NOT NULL, another reports NULLABLE), the unified column is NULLABLE. Because any `NullFill` child already forces nullability, and because declared-NOT-NULL columns that cross into a union with even one NULL-fillable counterpart logically cannot preserve NOT-NULL semantics, the rule is one-way. `COMP_W_2301 UnionsetNullabilityWidened` advises authors when the widening happens (informational-severity; not an error).

### 4.5 Re-aggregation terminal wrapper

When the Unionset's composed surface carries Measures / Metrics and the Request includes them, the planner wraps the `PlanNode::Union` in a terminal `PlanNode::Aggregate` that GROUPs BY every requested Dimension and re-aggregates each requested Measure.

**Why re-aggregate.** UNION ALL preserves row-identity per child; each child's rows are independent. If two children both contribute rows at `(date=2024-01-01, source_platform=adwords)` — perhaps because of late-arriving data overlap — the Union produces two rows. Without re-aggregation, a downstream `SUM(cost)` would see separate per-row entries; with re-aggregation, the two contributions sum together at the grouping key.

**Re-aggregation function inference.** Shared with Grainset (per `20 §5` / `22`). The mapping table (cross-referenced, not re-ratified here):

| Original child-side `Aggregation` | Re-aggregation function | Correctness |
|---|---|---|
| `Sum` | `Sum` | Exact |
| `Count` | `Sum` | Exact (sum of partial counts) |
| `Min` | `Min` | Exact |
| `Max` | `Max` | Exact |
| `CountDistinct` | `Sum` | Lossy (overcounts across children when children's row spaces overlap) |
| `Avg` | `Sum` over numerator + `Sum` over denominator, divided; or lossy `Sum` fallback | Not-decomposable-directly; authors should restructure as Metric with explicit num/den Measures |

Full table and rationale in `22 §?` (parallel draft). `23` cross-refs.

**Skip rules.** The terminal wrapper is SKIPPED in three cases:

1. **No Measures / Metrics requested.** The Request selects only Dimensions; re-aggregation is a no-op (every row is already distinct on the grouping key, by `UnionMode::Distinct`, or preserved-duplicates-are-wanted-by-author for `UnionMode::All`).
2. **Distinguishing metadata.** If every requested Dimension has a `Literal` or `Metadata(Path)` mapping whose value is distinct across all contributing children — e.g. a `source_platform` Dimension mapped as the literal `"adwords"` in one child and `"facebook"` in another — then no two rows from different children can share a grouping key, and re-aggregation is a no-op. `UnionsetStrategy` detects this case and skips the wrapper. (The same optimization is ratified for `22`'s multi-source Grainsets; the Unionset analog is the "has-source-distinguishing-metadata" skip.)
3. **Single-child post-prune.** If Coverage-driven pruning (§4.6) collapsed the Union to a single child, the wrapper's behavior is identical to the single child's own terminal wrapper — redundant work. `UnionsetStrategy` skips it.

In all other cases, the terminal Aggregate is emitted.

### 4.6 Coverage-driven branch pruning (advisory)

When a child's every contribution to the Request's selected-fields set resolves to `FieldOwnership::NullFill`, that child contributes only NULL rows to the Union — its columns are all `CAST(NULL AS T)`. Its sole effect is to inflate the row count with NULL-valued rows; the terminal re-aggregation (§4.5) then groups them under NULL-valued grouping keys.

Whether this is the author's intent or an oversight depends on context. `UnionsetStrategy` emits `PLAN_W_2301 UnionsetBranchPrunable` (§10.5) as an advisory and prunes the child from the Union. The pruned child's subplan is not constructed; the Manifest's `ResolvedUnionset.children` list is unchanged.

**Exceptions.**

- If the Request is a count-like query (`COUNT(*)` or equivalent) where NULL rows contribute to the count, the pruning would change the result semantics. Detection: the Request selects a `Measure` whose `Aggregation` is `Count` and the Measure's semantics is `Count(*)` / `Count(Key)` without filtering on the NullFill'd column. In this case, pruning is suppressed and the child's branch is emitted with `PhysicalExpr::Cast(Null, T)` columns — the row-count contribution is preserved.
- For `UnionMode::Distinct`, pruning is always safe: a child contributing only NULL rows collapses to at most one all-NULL row under deduplication, which either matches a NULL row from another child or stands alone; pruning removes at most one row and preserves DISTINCT semantics. The advisory is emitted nonetheless.

### 4.7 Emitted plan-tree shape

Summary ASCII (deterministic output shape for the canonical multi-Simple-child case):

```text
Aggregate                       [terminal re-aggregation; may be skipped per §4.5]
├─ group_by: [dim_0, dim_1, ...]
├─ aggregates: [Sum(m_0), Max(m_1), ...]
│
└─ Union                        [PlanNode::Union; distinct flag from UnionMode]
   ├─ distinct: false           [UnionMode::All]
   │
   ├─ Project                   [child 0 branch: cast + NULL-fill]
   │  ├─ cols: [c0_dim_0, Cast(c0_col_1, T), Cast(Null, U) AS dim_2, ...]
   │  └─ ...child 0's own subplan (SimpleStrategy / GrainsetStrategy / ...)
   │
   ├─ Project                   [child 1 branch]
   │  ├─ cols: [c1_dim_0, c1_col_1, c1_col_2, ...]
   │  └─ ...child 1's own subplan
   │
   └─ Project                   [child N-1 branch]
      ├─ cols: [...]
      └─ ...child N-1's own subplan
```

The sub-tree rooted at each `Project` (a child's branch) is the child's own Strategy's output. The `UnionsetStrategy` contributes only the top two layers (Aggregate, Union) plus the per-child Project wrappers at the seam.

---

## 5. Coverage semantics

### 5.1 Composition-level Coverage as the decision driver

Per `16 §8`, a `ComposedSemanticInterface` carries a `CompositionCoverage` keyed by `(DataKindRef, UnifiedName)` with `CoverageVariant` values from `15 §6.1` (`Native`, `NullFill`, `Derived`). For a Unionset, this coverage is the primary input the strategy reads.

`UnionsetStrategy` consumes `CompositionCoverage` at three sites:

- **§4.2 Request narrowing.** For each child, the strategy computes the subset of the Request's `select:` fields that the child covers (Native or Derived) and the subset that does not (NullFill). Only the covered subset is forwarded to the child's own Strategy.
- **§4.3 NULL-fill projection.** For each non-covered field, the strategy emits a typed NULL in the child's `wrap_for_union` Project.
- **§4.6 Branch pruning.** When every requested field is NullFill for a child, the child's branch is prunable.

### 5.2 Fold from per-child Binding Coverage (for Simple children)

When a Unionset child is `Simple`, its `ResolvedBinding.coverage` (per `15 §6`) is the direct input. The fold rule at compile time (per `16 §8.4`) for each `(child_i, composed_field)`:

1. Resolve `composed_field` (the `UnifiedName` on the Unionset's composed surface) to the child's own `SemanticsName`. This is identity when the child and the Unionset use the same name; it resolves through the Unionset's top-level `SemanticInterface`'s mapping when namespacing applies (per `16 §6.2`).
2. If the child's `SemanticInterface` does not declare the resolved name, the composition-level Coverage is `NullFill` (the child lacks the Semantics entirely).
3. Otherwise, fold the child's Binding-level `Coverage.entries` for the name across the child's Binding sources:
   - If ≥1 Binding source has `CoverageVariant::Native`, composition-level is `Native`.
   - Else if ≥1 has `Derived`, composition-level is `Derived`.
   - Else (all Binding sources are `NullFill`) composition-level is `NullFill`.

### 5.3 Fold from per-child composed Coverage (for Complex children)

When a Unionset child is `Complex` (Grainset or Joinset), the child's `ComposedSemanticInterface.coverage` is already a composition-level structure. The Unionset's fold consumes it directly:

1. Resolve `composed_field` on the Unionset's surface to the child's own composed-surface name (typically identity; namespacing applies when both kinds expose the same bare name with incompatible shape — per `16 §6.2.2`).
2. If the child's composed surface does not expose the name, composition-level Coverage is `NullFill`.
3. Otherwise, read the child's composed-level `FieldOwnership` for the name:
   - `Native(_)` or `Shared([...])` → Unionset's composition-level `Native`.
   - `Derived(expr)` → Unionset's composition-level `Derived` (the child computes; the Unionset treats it as a "came out of the child's pipeline" contribution).
   - `NullFill([...])` → Unionset's composition-level `NullFill`.

The fold is uniform across Simple and Complex children; the only difference is the input source (Binding-level vs. composed-level Coverage).

### 5.4 Explicit overrides compose with the fold

An explicit `ChildCoverageOverride { provides }` (§3.2) MASKS the fold. After the fold produces a per-`(child, name)` variant:

- If `name ∈ provides` and the fold yielded `NullFill`: `COMP_E_2305 ChildCoverageOverridesUncovered` (§9.3) — the author claims the child provides the name, but the child's interface does not even declare it (caught by §9.3's subsumption check) OR claims coverage a Binding-level fold denies (the Binding's sources lack the column). The override is authoritative, and the mismatch is a compile error rather than a silent NULL.
- If `name ∈ provides` and the fold yielded `Native` / `Derived`: the fold's variant is preserved.
- If `name ∉ provides` and the fold yielded `Native` / `Derived`: the override forces `NullFill` (authors explicitly want the child's coverage suppressed).
- If `name ∉ provides` and the fold yielded `NullFill`: already `NullFill`; no change.

The rule: `provides` is an opt-in whitelist. Absence from `provides` AND Binding-level presence still collapses to `NullFill`. Presence in `provides` AND Binding-level absence is an error.

### 5.5 `FieldProvenance::NullFill` as the composition-level record

Per `16 §7.3.3`, the composed interface's `FieldProvenance` for a Unionset (`CompositionKind::Unionset`) uses `FieldOwnership::NullFill(providers)` to record fields partially covered. The `providers` vector lists the children that DO cover the field; non-providers are inferred by set-difference against `constituents`.

The Unionset is the only `CompositionKind` that emits `FieldOwnership::NullFill` (per `16 §7.3.3`'s explicit note). Other composition kinds handle non-coverage via join-type semantics (e.g. `Joinset` uses `JoinType::Left` to carry NULL-fill for unmatched rows, which is expressed in the join's NULL-handling rather than in `FieldOwnership`). `23` is the authoritative consumer for `FieldOwnership::NullFill`.

### 5.6 Coverage-completeness check

A Unionset's composed `SemanticInterface` exposes a set of names; every name must be covered by at least one child (Natively, Derived, or via composition-level `Derived`). A composed-surface name that no child covers is an error (`COMP_E_2306 UnionsetFieldUnreachable` — §9.5), because its `FieldOwnership` would be `NullFill([])` (empty providers), producing a column that is always NULL for every row. The author either failed to declare a contributor or declared the Semantics on the wrong kind.

The check runs at `compile`, after the per-child Coverage fold. It is a Unionset-specific extension of `15 §5.6`'s Binding-level completeness check, lifted to the composition level.

---

## 6. Interaction with `TemporalShape`

`TemporalShape` is ratified in `17` (parallel draft in this wave). `23`'s interaction is advisory: the Unionset planner emits warnings when children's `TemporalShape`s suggest the UNION ALL is semantically ill-formed, but does not error. Authors with intentional cross-shape unions (e.g. merging an `Events` stream with a `Snapshot` cutover) are accepted, with the advisory attached to the Manifest's `Diagnostic` list.

### 6.1 Shape-alignment advisory rules

Children in a Unionset SHOULD share `TemporalShape` for the Union to be semantically well-formed. When they do not, `UnionsetStrategy` (at compile time, during composition-level fold) emits advisories:

| Child A shape | Child B shape | Advisory | Severity |
|---|---|---|---|
| `Timeseries` | `Timeseries` | none — aligned | — |
| `Events` | `Events` | none — aligned | — |
| `Snapshot` | `Snapshot` | none — aligned, but see §6.2 on snapshot-as-of times | — |
| `SCD(Type2)` | `SCD(Type2)` | none — aligned | — |
| `Timeseries` | `Events` | `COMP_W_2302 UnionsetShapeMismatchTimeseriesEvents` | Warning |
| `Timeseries` | `Snapshot` | `COMP_W_2303 UnionsetShapeMismatchTimeseriesSnapshot` | Warning |
| `Snapshot` | `Events` | `COMP_W_2304 UnionsetShapeMismatchSnapshotEvents` | Warning |
| `SCD(*)` | any non-SCD | `COMP_W_2305 UnionsetShapeMismatchScdNonScd` | Warning |

The advisories are not symmetric — the first-child shape is the "base"; later children are compared against it. The matrix is symmetric in its truth value but the emission is ordered for Diagnostic stability.

### 6.2 Snapshot-as-of across children

Two `Snapshot` children that serve different as-of timestamps produce semantically heterogeneous rows: Child A's row at `snapshotted_at=2024-01-01` and Child B's row at `snapshotted_at=2024-01-15` both contribute, but the Union's rows mix two distinct "worlds." The advisory `COMP_W_2306 UnionsetSnapshotMultipleAsOf` fires when the composed surface exposes `snapshotted_at` as a Dimension (so rows can be distinguished) — and `COMP_W_2307 UnionsetSnapshotMultipleAsOfNoDiscriminator` fires when it does not (rows are indistinguishable; querying the Union produces a mixed-world result without a way to filter).

Both are warnings, not errors. Authors with intentional mixed-world unions (e.g. a long-history snapshot plus a recent-snapshot tail) are accepted; authors making a modeling mistake see the advisory.

### 6.3 SCD × non-SCD unions

Unioning an `SCD` (any type) child with a non-SCD child (e.g. `Timeseries` or `Events`) is structurally legal but almost always a modeling error: SCD's history rows carry `valid_from` / `valid_to` windows that non-SCD rows do not, and the Union collapses those windows to NULLs on the non-SCD rows. `COMP_W_2305` advises authors.

The specific windows / `valid_from` / `valid_to` handling is `17`'s concern; `23` forwards the advisory and lets `17` refine.

### 6.4 Deferred: shape-gated planner rewrites

Per `17`'s DEFERRED status (planner-side shape-aware planning is deferred to a later milestone), `23` does not emit shape-specific plan-node rewrites (e.g. inserting a shape-aware join at the composition level, or routing SCD children through a dedicated as-of-timestamp branch). All shape interaction in `23`'s current scope is advisory at compile time. Planner-side rewrites will land when `17`'s planner support is ratified; at that point, `23 §6` will be extended with a "shape-aware rewrite" subsection, tracked as `[TD-UNIONSET-SHAPE-PLANNING]`.

---

## 7. Interaction with `Grain`

### 7.1 Grain-alignment advisory rules

Children SHOULD share a common `Grain` for Union to be semantically clean: a Union of a daily-grain child with an hourly-grain child mixes rows at two granularities, and a downstream aggregation over the unified surface may not produce what the author expects.

`UnionsetStrategy` emits compile-time advisories:

| Scenario | Advisory | Severity |
|---|---|---|
| All children share a single `Grain` | none | — |
| Children's grains are all rollable to a common coarsest grain (e.g. `Day` + `Hour` → `Day` via rollup legality per `17`) AND the Request requests at or coarser than that common grain | none | — |
| Children's grains are all rollable but the Request requests a finer grain than the common coarsest | `PLAN_E_2302 UnionsetGrainIncompatibleWithRequest` | Error |
| Children have incompatible grains (no common rollup per `17` / `13 §5`) | `COMP_W_2308 UnionsetGrainDivergent` | Warning |

The compile-time advisory is a warning because the Manifest-layer decision ("which grain is the Union's grain?") depends on the Request — which the Manifest doesn't know. The plan-time error fires when a Request demands a grain no child can serve.

### 7.2 Post-Union rollup shape

When children's grains differ but rollup is legal per `17`, the post-Union terminal `Aggregate` (§4.5) rolls up to the coarsest shared grain by default. The Request MAY request a specific output grain:

- **Request at the coarsest shared grain or coarser.** The terminal Aggregate's `group_by` set is the coarsest-shared-grain dimensions; the shape is legal per `17`.
- **Request at a grain finer than the coarsest shared grain.** The planner emits `PLAN_E_2302` — rollup cannot invent finer detail. Authors must either (a) request a coarser grain, or (b) authorthe Unionset so every child's grain covers the finer grain.

### 7.3 Grain rollup via children's own strategies

Each child contributes rows at its own grain. If a child has a finer grain than the Unionset's target, the child's own Strategy's plan-stage rollup (per `22` for Grainset children; per `21` for Simple children with coarser grains requestable) runs BEFORE the Union's per-child Project. This is the standard "push aggregation down" pattern; `UnionsetStrategy` does not duplicate the work.

If a child's grain is coarser than the target but the target is the child's grain or a further rollup, the child's contributions land at their native grain and the post-Union Aggregate performs the final rollup. This is the normal case.

### 7.4 `Grain`-aware re-aggregation for legal rollups

Per `13 §5` / `17`, rollup legality depends on the `TemporalShape` × `Additivity` combination of each Measure. A `SemiAdditive` Measure over a `Snapshot` child rolled up across a non-snapshot-time dimension (e.g. `customer` instead of `date`) is a semantic error (per `17`'s additivity rules), but summing the same Measure over `date` is safe. The terminal Aggregate's re-aggregation (§4.5) inherits these rules; the inference table in §4.5 is subject to per-Measure `Additivity` corrections per `17`.

---

## 8. Validation Preconditions

Validation Preconditions run at the `validate` stage (per `10 §3`). They check structural well-formedness that does not require type resolution or catalog metadata. The code range `VALID_E_2300`–`2399` is reserved for Unionset-specific validation errors; non-Unionset-specific structural errors (e.g. `12 §2`'s illegal-nesting) are reported at their own code ranges.

> **Note on code allocation.** The `2300`–`2399` sub-range is the per-DataKind convention for doc `23` (Unionset). This is a per-variant allocation pattern parallel to `15`'s `03NN` and `16`'s `04NN`; reconciliation against `30 §6.2`'s cross-subsystem ranges is tracked as `[TD-UNIONSET-CODERANGE]` in the open-items doc.

### 8.1 Code roster

| Code | Variant | Condition | Severity |
|---|---|---|---|
| `VALID_E_2301` | `UnionsetEmptyChildren { unionset }` | `children.is_empty()` — the `unionsets:` block has no children at all. | Error |
| `VALID_E_2302` | `UnionsetSingleChild { unionset, only_child }` | `children.len() == 1` — a one-child Unionset is semantically the child itself; authors should replace it. (This is `12 §3.2`'s `UnionsetMustHaveMultipleChildren`; `23` re-surfaces it at the Unionset's own code range for diagnostic clarity.) | Error |
| `VALID_E_2303` | `DuplicateUnionsetChild { unionset, child, indices }` | Two `children[i]` entries reference the same `DataKindRef`. Authors wanting "union a source with itself" should use a multi-source Simple DataKind instead. | Error |
| `VALID_E_2304` | `UnionsetSelfReference { unionset }` | `children[i].data_kind_ref` resolves to the Unionset's own name. | Error |
| `VALID_E_2305` | `UnionsetShapeFieldConflict { unionset, field, children_disagreeing }` | Two children declare shape-fields (`TemporalShape`, declared `Grain`) on the same Semantics with mutually-exclusive values that `validate` can detect structurally — i.e. before type resolution. This is the structural half; the composition-level type-resolution half fires as `COMP_E_2304` at compile. | Error |
| `VALID_E_2306` | `UnionsetModeIncompatibleWithChildren { unionset, mode, reason }` | `UnionMode::Distinct` declared but the Unionset's composed-surface has a non-comparable type (e.g. `Binary`) that DISTINCT cannot meaningfully compare. (Rare; reserved for edge cases of the future.) | Error |

### 8.2 Cyclic child-reference detection (structural half)

A Unionset's children may nest other DataKinds (Grainsets, Joinsets) that themselves reference further DataKinds. A cycle forms if the transitive-closure of child references reaches the Unionset itself or any ancestor. The structural half of cycle detection — where the cycle is identifiable from the Model's YAML structure alone without type resolution — fires at `validate` as `VALID_E_2304 UnionsetSelfReference` for direct self-reference. Transitive cycles require the full compile-time reference graph; they are caught at compile as `COMP_E_2301` (§9.2).

### 8.3 Validate-time error accumulation

Per `10 §3.3`, validate-stage errors accumulate (all failures collected in one pass) before failing. `VALID_E_2301`–`2306` follow this convention: a Unionset with multiple structural issues surfaces every issue in the `Diagnostic` list. Authors see all structural problems in one `validate` invocation.

### 8.4 Interaction with `12`'s structural checks

The `12 §2` nesting-matrix checks run BEFORE `23`'s validation. A `unionsets:` block nested inside another Unionset is rejected upstream as `ParseError::IllegalNesting` (at parse time, per `12 §2.2`); by the time `23`'s validation runs, nesting legality is assumed.

Similarly, `12 §3.2`'s `UnionsetMustHaveMultipleChildren` is the original structural Precondition for the two-child minimum. `23 §8.1` reports it as `VALID_E_2302` with richer context (`only_child: DataKindRef` so the author sees which single child they accidentally declared). The two codes represent the same rule; the richer `VALID_E_2302` is the canonical surface at Round-1 maturity.

---

## 9. Compile Preconditions

Compile Preconditions run at the `compile` stage (per `10 §3.3`). They check integrity that requires resolved names, resolved types, and Manifest-index access. The code range `COMP_E_2300`–`2399` is reserved.

### 9.1 Code roster

| Code | Variant | Condition | Severity |
|---|---|---|---|
| `COMP_E_2301` | `UnionsetCyclicChildReference { unionset, cycle }` | Transitive child-reference cycle detected (Unionset A → Grainset B → Unionset A, or similar). Fail-fast. | Error |
| `COMP_E_2302` | `UnionsetChildReferenceUnresolved { unionset, child_index, ref }` | A child's `DataKindRef` does not resolve to any declared top-level DataKind. Fail-fast. | Error |
| `COMP_E_2303` | `ChildCoverageOverridesUnexposed { unionset, child_index, semantics }` | An explicit `coverage.provides:` entry on a child references a `SemanticsName` the child's own interface does not declare. Authors cannot claim a child provides a Semantics the child cannot produce. | Error |
| `COMP_E_2304` | `CrossChildTypeDisagreement { unionset, composed_field, children_types }` | Two children both provide `composed_field` Natively, with logically incompatible `DataType`s that cannot be reconciled by `13 §7`'s cast matrix. `children_types: Vec<(DataKindRef, DataType)>`. | Error |
| `COMP_E_2305` | `ChildCoverageOverridesUncovered { unionset, child_index, semantics }` | An explicit `coverage.provides:` entry claims a name whose Binding-level fold yields `NullFill` — the child's Binding sources lack the column despite the child's interface declaring it. | Error |
| `COMP_E_2306` | `UnionsetFieldUnreachable { unionset, composed_field }` | A composed-surface Semantics is covered (Native / Derived / Shared) by NO child. The column would be always-NULL. | Error |
| `COMP_E_2307` | `UnionsetComposedInterfaceBuildFailed { unionset, cause }` | Internal: `16 §11` field-first composition building failed for the Unionset's declared interface. Wraps the underlying `16`-owned error as `cause`. Diagnostic-only; indicates a semstrait bug or a Model that slipped past validation. | Error |

### 9.2 Cyclic child-reference detection (transitive half)

At compile, the per-Unionset cycle detector walks the transitive-closure of child DataKindRefs. Detection uses the standard DFS-with-visited-set approach:

```text
detect_cycles(unionset, manifest):
  stack = [unionset.data_kind_id]
  visited = {unionset.data_kind_id}

  fn walk(current):
    for child in current.children:
      child_id = child.data_kind_ref
      if child_id in stack:
        return COMP_E_2301 with cycle = stack[stack.index(child_id):] + [child_id]
      if child_id in visited:
        continue              # already-proven-acyclic subgraph
      visited.insert(child_id)
      stack.push(child_id)
      walk(manifest.get(child_id))
      stack.pop()
    return Ok

  walk(unionset)
```

The DFS runs once per top-level Unionset at compile. Per-DataKind memoization (the `visited` set) makes repeated walks O(1); full Model-wide cycle detection is `O(|DataKinds|)` total.

### 9.3 Composition-level Coverage override subsumption

For each `ResolvedUnionsetChild` with `coverage_override = Some(override)`:

1. Check every `name ∈ override.provides` is in the child's `SemanticInterface`'s declared roster → else `COMP_E_2303`.
2. Fold the child's Binding-level Coverage per §5.2 (for Simple children) or §5.3 (for Complex). For every `name ∈ override.provides`:
   - If the fold yields `NullFill`: `COMP_E_2305`.
   - If the fold yields `Native` / `Derived`: consistent; preserved.
3. Apply the override mask (§5.4) to produce the final composition-level Coverage.

### 9.4 Cross-child type reconciliation

The §4.4 unified-column-type derivation runs at compile time as part of building the `ResolvedUnionset.composed_interface`. For each composed-surface Semantics `s`:

1. Collect `contribs(s) = { (child_i, data_type_of_child_i_for_s) | ... }`.
2. If `|contribs(s)| ≤ 1`, no reconciliation needed; use the single type or the declared type (§4.4 step 2 / 3).
3. If `|contribs(s)| ≥ 2`, attempt LUB per `14a`'s promotion lattice + `13 §7`'s cast matrix:
   - If every pair is cast-compatible: LUB computed; unified type selected.
   - If any pair is incompatible: `COMP_E_2304`. Fail-fast.

The reconciled `unified_column_type(s)` is stored on the `ResolvedUnionset.composed_interface` (via `UnifiedSemantics.dimensions[_].data_type` or `.measures[_].data_type` per `16 §6`) and is available at plan time.

### 9.5 Coverage-completeness check

After the fold produces composition-level Coverage, iterate every name in the Unionset's `SemanticInterface`:

- If NO child covers the name (every `(child, name)` is `NullFill`): `COMP_E_2306`. Fail-fast.
- Otherwise: record the composition-level `FieldOwnership` per `16 §7.3.3` (`Native` if exactly one provider; `Shared` if multiple compatible-shape providers; `NullFill(providers)` if partial).

### 9.6 Compile-stage error-handling posture

Per `10 §3.3` and `15 §10.8`: structural checks accumulate; reference-resolution and type-check checks fail-fast. `23`'s compile-stage convention:

- **Fail-fast:** `COMP_E_2301` (cycle, cannot proceed), `COMP_E_2302` (ref unresolved, cannot build), `COMP_E_2304` (type disagreement, cannot reconcile), `COMP_E_2307` (internal build failure).
- **Accumulate:** `COMP_E_2303`, `COMP_E_2305`, `COMP_E_2306` — all "override / completeness" checks iterate the full surface before failing, so the author sees every problem in one compile run.

---

## 10. Plan-stage rules

Plan-stage rules run at the `plan` stage (per `10 §3.4`). They check Request-specific integrity against the Manifest's `ResolvedUnionset`. The code range `PLAN_E_2300`–`2399` is reserved.

### 10.1 Code roster — errors

| Code | Variant | Condition | Severity |
|---|---|---|---|
| `PLAN_E_2301` | `UnionsetRequestFieldNotCovered { unionset, field }` | A Request references a `SemanticsName` that resolves to the Unionset's surface but has `FieldOwnership::NullFill([])` — no child covers it. (Edge case; §9.5 catches most instances at compile time, but pathological field sets may slip through.) | Error |
| `PLAN_E_2302` | `UnionsetGrainIncompatibleWithRequest { unionset, request_grain, children_grains }` | Request's grain is finer than any child's grain or finer than the coarsest-shared-grain. Rollup cannot invent detail. | Error |
| `PLAN_E_2303` | `UnionsetRequestTotallyNullFilled { unionset, request_fields }` | After Coverage-driven pruning (§4.6), zero children survive. Every child's contribution to the Request is pure NULL. Indicates a Request/Manifest inconsistency. | Error |
| `PLAN_E_2304` | `UnionsetReAggregationInfeasible { unionset, measure, reason }` | A requested Measure's re-aggregation rule (§4.5's inference table) cannot be satisfied — e.g. `Avg` without decomposable num/den Measures AND `UnionMode::Distinct` AND multiple contributing children. Authors must restructure the Measure or switch `UnionMode::All`. | Error |

### 10.2 Code roster — warnings

| Code | Variant | Condition | Severity |
|---|---|---|---|
| `PLAN_W_2301` | `UnionsetBranchPrunable { unionset, child_index, reason }` | Per §4.6; a child's branch was pruned from the Union because every Request-selected field was NullFill for that child. Advisory. | Warning |
| `PLAN_W_2302` | `UnionsetReAggregationLossy { unionset, measure, reason }` | A requested Measure's re-aggregation function is `Lossy` per §4.5's table (e.g. `CountDistinct` across children; `Avg` collapsed to `Sum`). Advisory that the cross-child aggregation may over-count or drop precision. | Warning |
| `PLAN_W_2303` | `UnionsetDistinctThreeValuedNullCollision { unionset, field }` | Per §4.3's `DISTINCT` mode note: two children contribute rows differing only in `NullFill` positions; DISTINCT does not dedupe them because NULL ≠ NULL. Advisory for authors expecting full dedup. | Warning |

### 10.3 Interaction with `16`'s planner errors

`16 §14.3` ratifies a set of planner errors for implicit composition (`PLAN_E_0500`–`0509`). A Unionset is an EXPLICIT composition — `Request.from` points at the Unionset's name — so most of `16`'s implicit-composition errors do not apply. The exception is `PLAN_E_0506 RequestOutOfSurface`: a Request naming a Semantics not on the Unionset's composed surface. `23` does not duplicate this error; the `16`-owned code fires at the planner's pre-dispatch surface-validation step, before `UnionsetStrategy` is called.

`23`'s own plan-stage errors (`PLAN_E_2301`–`2304`) fire AFTER the `16 §11.8` "explicit `from:` surface validation" passes — i.e. the requested name IS on the Unionset's surface, but the per-child-coverage fold leaves nothing to scan (`PLAN_E_2301`, `PLAN_E_2303`) or the grain / re-aggregation constraints are unsatisfiable (`PLAN_E_2302`, `PLAN_E_2304`).

### 10.4 Plan-stage error-handling posture

Per `10 §4`: planner errors fail-fast. `23`'s plan-stage errors are all `Severity::Error` (fail-fast) except the `PLAN_W_2301`–`2303` advisories, which accumulate on the `Diagnostic` list and the plan proceeds.

### 10.5 Advisory-warning emission sites

| Advisory | Emission site |
|---|---|
| `PLAN_W_2301 UnionsetBranchPrunable` | `UnionsetStrategy::wrap_for_union(child_i, ...)` when the child's Coverage-narrowed Request is empty and §4.6's pruning exceptions do not apply. |
| `PLAN_W_2302 UnionsetReAggregationLossy` | `UnionsetStrategy::finalize(...)` when inferring the re-aggregation function (§4.5) selects a `Lossy`-marked entry from the inference table. |
| `PLAN_W_2303 UnionsetDistinctThreeValuedNullCollision` | `UnionsetStrategy::plan(...)` when `UnionMode::Distinct` + `FieldOwnership::NullFill([some])` combine on a requested field. |

---

## 11. Worked example

A common Unionset use-case: heterogeneous event sources across time windows, where one source covers an older date range with a slightly different column set, and another source covers the current range.

### 11.1 Scenario

An analytics team tracks order events. The legacy events system (`orders_legacy`) was retired at the end of 2022; the new events system (`orders_new`) took over in 2023. The legacy source carries `customer_id`, `order_date`, `amount`; the new source adds `channel` (not present in legacy). Queries should see a unified event stream with the `channel` Dimension appearing as NULL for pre-2023 rows.

### 11.2 YAML

```yaml
# Top-level Model (shape per `32`)
datasets:
  - name: orders_legacy
    binding:
      sources:
        - path: "s3://bucket/events/legacy/*.parquet"
      column_mapping:
        customer_id: customer_id
        order_date: order_date
        amount: amount
    interface:
      dimensions:
        - { name: customer_id, type: identifier }
        - { name: order_date, type: temporal }
      measures:
        - { name: revenue, agg: sum, expr: amount }
      keys:
        - { name: customer_id, kind: foreign }

  - name: orders_new
    binding:
      sources:
        - path: "s3://bucket/events/new/*.parquet"
      column_mapping:
        customer_id: customer_id
        order_date: order_date
        amount: amount
        channel: channel
    interface:
      dimensions:
        - { name: customer_id, type: identifier }
        - { name: order_date, type: temporal }
        - { name: channel, type: categorical }
      measures:
        - { name: revenue, agg: sum, expr: amount }
      keys:
        - { name: customer_id, kind: foreign }

unionsets:
  - name: orders_all
    mode: all
    dimensions:
      - { name: customer_id, type: identifier }
      - { name: order_date, type: temporal }
      - { name: channel, type: categorical }   # present only in orders_new
    measures:
      - { name: revenue, agg: sum }
    keys:
      - { name: customer_id, kind: foreign }
    datasets:
      - ref: orders_legacy   # does not provide `channel` → inferred NullFill
      - ref: orders_new      # provides everything
```

### 11.3 Compile-time resolution

At `compile`:

1. **Child reference resolution.** Both `orders_legacy` and `orders_new` resolve successfully (§9.2 check passes).
2. **Coverage fold (§5.2).** For each composed-surface field:
   - `customer_id`: legacy Native, new Native → `Shared([legacy, new])`.
   - `order_date`: legacy Native, new Native → `Shared([legacy, new])`.
   - `channel`: legacy `NullFill` (no column), new Native → `NullFill([new])`.
   - `revenue`: legacy Native (via computed `Sum(amount)`), new Native → `Shared([legacy, new])`.
3. **Coverage completeness check (§9.5).** Every composed-surface Semantics has at least one provider → pass.
4. **Cross-child type reconciliation (§9.4).** `revenue` is `Sum(amount)` on both; both `amount` columns are `Decimal(12, 2)` (assume). LUB identity → unified type `Decimal(12, 2)`.
5. **Composed interface materialized.** `ResolvedUnionset { data_kind_id, name: "orders_all", composed_interface: ..., children: [...], mode: UnionMode::All }`.

### 11.4 Plan tree for a sample Request

```text
Request {
    from: Some("orders_all"),
    select: [
        dim: order_date,
        dim: channel,
        measure: revenue,
    ],
}
```

The emitted plan tree:

```text
Aggregate                                                    [terminal re-aggregation]
├─ group_by: [order_date, channel]
├─ aggregates: [Sum(revenue)]
│
└─ Union                                                     [PlanNode::Union]
   ├─ distinct: false                                        [UnionMode::All]
   │
   ├─ Project                                                [orders_legacy branch]
   │  ├─ cols: [
   │  │    Column("order_date") AS order_date,
   │  │    Cast(Literal(Null), Categorical) AS channel,      [NULL-fill per §4.3]
   │  │    Sum(Column("amount")) AS revenue
   │  │  ]
   │  └─ Aggregate                                           [orders_legacy's own SimpleStrategy]
   │     ├─ group_by: [order_date]
   │     ├─ aggregates: [Sum(amount)]
   │     └─ Scan
   │        ├─ source: orders_legacy Binding
   │        └─ columns: [order_date, amount]
   │
   └─ Project                                                [orders_new branch]
      ├─ cols: [
      │    Column("order_date") AS order_date,
      │    Column("channel") AS channel,
      │    Sum(Column("amount")) AS revenue
      │  ]
      └─ Aggregate                                           [orders_new's own SimpleStrategy]
         ├─ group_by: [order_date, channel]
         ├─ aggregates: [Sum(amount)]
         └─ Scan
            ├─ source: orders_new Binding
            └─ columns: [order_date, channel, amount]
```

Notes on the tree:

- **Per-child subplans.** Each child's sub-tree (Scan → Aggregate → Project) is the `SimpleStrategy`'s output for the child's narrowed Request. `UnionsetStrategy` does not reach into the sub-tree; it wraps the output at the `Project` seam.
- **NULL-fill at the seam.** The `orders_legacy` branch's Project emits `Cast(Literal(Null), Categorical) AS channel`. The type `Categorical` is the unified column type for `channel` (from `orders_new`'s declaration; per §4.4 step 3 "single contributor's type is unified type").
- **Pre-Union Aggregate inside each child.** `SimpleStrategy` per-child aggregation is a common optimization — it reduces the row count entering the Union. The per-child Aggregate's `group_by` is the child's view of the Request (e.g. `orders_legacy` groups by `order_date` only, since `channel` is NULL-filled at the seam and does not participate in the child's grouping). This is `21`'s `SimpleStrategy` concern; `23` consumes the result.
- **Terminal Aggregate (§4.5).** Re-groups by the Request's full dimension set (`order_date`, `channel`) and re-sums `revenue`. The `channel=NULL` rows from `orders_legacy` group together under NULL; the `channel=<value>` rows from `orders_new` group per value.
- **`PlanNode::Union.distinct: false`** mirrors `UnionMode::All`.

### 11.5 Diagnostics emitted

For this Model + Request, compile emits no errors. At plan time, no errors fire. Depending on the `TemporalShape` of the two children (not declared in the YAML above; assume both are `Events`), no `COMP_W_23xx` shape-mismatch advisory fires either.

---

## 12. Round-1 open items

Round-1 open questions surfaced while drafting `23` are parked in `docs/design/questions/open/23_questions.md`. Questions span:

- The error-code allocation scheme (`*_E_23NN` per doc vs. `30 §6.2`'s cross-subsystem reservation).
- ~~Whether `UnionMode::Distinct` should remain v1 or be deferred.~~ **CLOSED by `18 §2`** — v1 roster is `{All, Unique}`; the variant formerly named `Distinct` was renamed to `Unique` and kept in v1.
- Whether the composition-level Coverage override (`ChildCoverageOverride.provides`) is the right shape, vs. richer per-Semantics `Native`/`Derived`/`NullFill` declaration.
- Whether `Avg` re-aggregation should be an error (`PLAN_E_2304`) or a `Lossy` warning (`PLAN_W_2302`) by default.
- The strict-mode posture for the §6 `TemporalShape`-mismatch advisories (warnings vs. errors).
- Whether post-prune single-child collapse should short-circuit the terminal Aggregate.
- Interaction with `17`'s as-of / snapshot-selection when children have heterogeneous `TemporalShape`s.

Each entry there records the Round-1 default `23` currently uses.

---

## 13. Cross-references

- `00 §4.1` — `Unionset` vocabulary entry; `CompositionKind::Unionset` tag.
- `00 §9` — I1 (no raw SQL), I4 (determinism), I5 (compile-time resolution), I8 (Manifest planner-complete), I10 (non-exhaustive), I12 (diagnostics).
- `10 §3.3, §3.4` — `compile` and `plan` stage contracts.
- `11 §5.1, §6, §8` — name identity, Semantics roster, `Additivity` vocabulary.
- `12 §2.2, §3` — nesting matrix (no Unionset-in-Unionset); Unionset block shape + two-child minimum.
- `13 §5, §7` — `DataType` compatibility, cast matrix (consumed by §4.4).
- `14 §3, §5.6` — `Expr` / `PhysicalExpr` shape; pass-through typing.
- `14a` — `FunctionRegistry`, promotion lattice for LUB.
- `14b §3.8` — `Computed`-value expressions on `ColumnMapping`; consumed at §5.3.
- `15 §6, §6.4` — Binding-level Coverage; scope boundary with `16`.
- `16 §5, §7, §8` — `ComposedSemanticInterface`, `FieldProvenance`, `CompositionCoverage`.
- `16 §14.3` — planner-layer composition errors (PLAN_E_0500–0509); `PLAN_E_0506 RequestOutOfSurface` precedes `23`'s plan-stage errors.
- `17` (parallel draft) — `TemporalShape`; consumed by §6.
- `20` (parallel draft) — DataKind taxonomy, Strategy dispatch, shared re-aggregation inference.
- `21` (parallel draft) — `SimpleStrategy`; the per-child Strategy most Unionset children use.
- `22` (parallel draft) — `GrainsetStrategy`; shared re-aggregation table owner.
- `24` (future) — `JoinsetStrategy`; legal as a Unionset child per `12 §2`.
- `25` (future) — applicability matrix; per-variant cells for Unionset.
- `30 §6.2` — cross-subsystem code-range table; `[TD-UNIONSET-CODERANGE]` reconciliation.
- `32` (future) — YAML surface for `unionsets:` block.
- `33` (future) — `ResolvedUnionset` persistence; `Manifest` index placement for composed interfaces.
- `34` (future) — `UnionsetStrategy` trait surface; re-aggregation helper.
- `35` (future) — `PlanNode::Union` field roster.
- `questions/open/23_questions.md` — Round-1 deferred items.

---

**End of document.** Round-1 open reconciliation items are in `docs/design/questions/open/23_questions.md`.
