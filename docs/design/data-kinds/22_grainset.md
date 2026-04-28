---
prereqs: [20, 13, 14, 15, 16, 17]
authoritative-for:
  - `GrainsetDataKind` model-layer struct shape (children roster, grain-axis binding, rollup policy)
  - `ResolvedGrainsetDataKind` manifest-layer counterpart and its per-child indices
  - `GrainsetChild` — constituent reference, natural grain, per-child Coverage projection
  - the request-grain extraction algorithm (from grouped Dimensions + temporal filter) consumed at plan time
  - child-eligibility predicate (grain ≤ request-grain AND Coverage ⊇ requested Semantics) and its deterministic subset check
  - rollup legality per `TemporalShape` (gated forward-ref to `17`): Timeseries / Events / Snapshot / SCD rules
  - the v1 cost function (source-count proxy) and the tie-break order (declaration order)
  - plan-shape contract: Grainset emits a single chosen-child delegation, not a union across candidates
  - `VALID_E_2200`–`2299`, `COMP_E_2200`–`2299`, `PLAN_E_2200`–`2299` error-code allocations
refined-by:
  - 17 (`foundations/17_temporal_shape.md` — shape-specific rollup rules, snapshot-pin policies, as-of anchoring for SCD)
  - 23 (`data-kinds/23_unionset.md` — Grainset children may themselves be Unionsets; field-first resolution across composed surfaces)
  - 24 (`data-kinds/24_joinset.md` — Grainset children may be Joinset members; Relationship walks through a Grainset's chosen child)
  - 25 (`data-kinds/25_applicability_matrix.md` — cross-kind cell for Grainset, including "Grainset-of-Grainset" nesting policy)
  - 33 (`apis/33_semstrait_manifest.md` — `ResolvedGrainsetDataKind` persistence, per-child index layout)
  - 34 (`apis/34_semstrait_planner.md` — planner entry-point that dispatches Grainset strategy)
  - 35 (`apis/35_semstrait_ir.md` — `PlanNode` subtree produced by Grainset delegation)
---

# 22. Grainset

> **Reconciliation (Phase-3, 2026-04-17).** The v1 authoring-layer canonical shape for `Grainset` is ratified across:
>
> - [`../apis/32_semstrait_model.md §3`](../apis/32_semstrait_model.md) — top-level YAML tag (`grainsets:`), `GrainsetBody` struct shape.
> - [`../foundations/18_entities.md`](../foundations/18_entities.md) — shared entity types consumed by `GrainsetBody`.
> - [`26_nesting_matrix.md`](./26_nesting_matrix.md) — nesting rules. Notably **R3** (every `ComplexDataKind` requires ≥ 2 children, auto-closing `Q-GRN-006`) and **R2** (no Grainset-of-Grainset self-nesting, auto-closing `Q-GRN-004`).
> - **SR-E-8** (`18 §11`): every Grainset child MUST author its own `extras.temporal.grain:` explicitly — no inheritance from the Grainset parent.
>
> This document retains authority for:
>
> - The request-grain extraction algorithm (§4) and child-eligibility / child-selection rules.
> - The v1 cost function (source-count proxy) and declaration-order tie-break.
> - `GrainsetStrategy` plan-shape contract — single chosen-child delegation, not a union across candidates.
> - `VALID_E_22NN` / `COMP_E_22NN` / `PLAN_E_22NN` error-code allocations.
>
> Rust-struct and YAML-surface body sections predate `18` (formerly `32c` before the 2026-04-17 promotion); read them as historical. `ColumnMapping` → `SemanticMapping` rename per `18 §10`.

---

## 1. Purpose and Scope

### 1.1 What `22` ratifies

`22` is the per-variant specification for `Grainset` — the `ComplexDataKind` variant that composes N children exposing the same `SemanticInterface` at different grains, and routes a `Request` at plan time to **exactly one** child: the cheapest one whose natural grain and Coverage cover the Request. The variant is introduced in `00 §4.1` and enters the taxonomy through `20`.

Concretely, `22` ratifies:

- **§2** — the `GrainsetDataKind` / `ResolvedGrainsetDataKind` struct shapes, the per-child `GrainsetChild` record, and the composition into `16`'s `CompositionKind::Grainset`.
- **§3** — the YAML authoring surface (`children:` list; per-child `grain`, `coverage` projection, optional `rollup:` override).
- **§4** — the child-selection algorithm: request-grain extraction, eligibility predicate, rollup legality gate, cost function, deterministic tie-break.
- **§5** — how `TemporalShape` (forward-ref to `17`) gates which children are planner-eligible at request time (Timeseries rolls freely, Snapshot pins at its source grain, SCD requires as-of anchoring, Events rolls by bucket-then-aggregate).
- **§6** — how Grainset consumes `15 §6`'s Binding-level Coverage and `16 §8`'s composition-level Coverage to drive both eligibility and fallback.
- **§7**–**§9** — the Precondition rosters at each pipeline stage (`validate` / `compile` / `plan`) with stable codes in the `22xx` sub-ranges.
- **§10** — the plan shape: Grainset emits a delegation to a single chosen child's sub-strategy (Simple / Unionset / Joinset), **not** a Unionset-style UNION ALL across candidates. The chosen child's sub-plan is spliced into the Grainset's owning position in the `SemanticPlan`.

### 1.2 What `22` does NOT ratify (forward-refs)

- **Field-first resolution that spans multiple Grainsets.** The rule "a Request whose Semantics span two Grainsets resolves via the Relationship graph" is `16 §11`'s implicit-composition path; `22` only governs selection *within* a single Grainset.
- **The full `TemporalShape` matrix and SCD subtypes.** `17` owns the shape taxonomy and the planner-eligibility gating rules. `22 §5` records the shape-of-the-interaction; it does not redefine the shapes.
- **Stats-backed cost.** The v1 cost function uses source-count as a proxy (§4.4). A stats-backed cost (row counts, partition counts, estimated scan sizes) is tracked as `[TD-GRAINSET-COST-STATS]` and deferred.
- **Grainset-of-Grainset nesting rules.** Whether a Grainset may nest another Grainset as a child is an invariant in `12 §2`'s nesting matrix, surfaced per-variant in `25`. `22` consumes the matrix but does not re-litigate it.
- **Non-temporal grain axes.** `13 §3.4` reserves non-temporal grain axes (geographic, entity) as a future extension; `22` is written against `Grain`'s current temporal-only variant set and trivially generalizes when `13` expands.
- **The `Relationship` traversal when a Grainset is a constituent of an implicit composition.** `16 §11.5`'s synthesis step consumes a Grainset's `ComposedSemanticInterface`; the child-selection inside the Grainset is still `22 §4`. The two layers compose by delegation.

### 1.3 Relationship to prerequisite docs

- **`00 §4.1`** — introduces `Grainset` as one of the three `ComplexDataKind` variants ("routes queries to the cheapest child covering the requested grain"). `22` is the full spec.
- **`13 §3`** — `Grain` enum with 7 temporal variants and a total coarseness order; `22 §4.2`'s eligibility predicate uses `Grain::coarseness()` verbatim.
- **`14` / `14a` / `14b`** — expressions resolved at `compile`; `22`'s per-child selection consumes pre-resolved `PhysicalExpr`s through the child's `ResolvedBinding`s (for Simple children) or `ComposedSemanticInterface` (for Complex children). No expression work at plan time.
- **`15 §6`** — Binding-level Coverage is the X-axis of source selection; `22`'s eligibility predicate uses Coverage to answer "does this child serve every requested Semantics?"
- **`16 §5–§8`** — `Grainset` is one of the four `CompositionKind` variants; `22 §2.3` wires the struct into `ComposedSemanticInterface`. `16 §8`'s `CompositionCoverage` is consumed by `22 §6` when the Grainset's children are Complex kinds.
- **`17`** — shape-gated rollup legality (§5) is a forward-reference; `17` is drafted in parallel.
- **`20`** — taxonomy-level invariants (one `Binding` per Simple; no Binding on Complex; `ComplexDataKind` composes children without introducing new Semantics beyond the composed surface) are consumed here. `20 §*` is cited where the invariant is used.

### 1.4 Invariants `22` directly upholds

| Invariant | Where `22` keeps it |
|---|---|
| **I1** — no raw SQL | Per-child selection is structural; the delegated sub-plan is a `SemanticPlan` subtree assembled from `PlanNode`s and `PhysicalExpr`s, never a string. |
| **I4** — SemanticManifest determinism | The SemanticManifest's `ResolvedGrainsetDataKind` records children in **declaration order** (§2.2). The child-selection algorithm is a deterministic function of `(SemanticManifest, Request)`: eligibility is a set membership check, cost is arithmetic, ties break by declaration order. Identical SemanticManifest + Request → identical chosen child → identical plan. |
| **I5** — resolution at compile time | Every child's `SemanticInterface` / `ComposedSemanticInterface`, Coverage, and natural grain is pre-resolved at `compile`. Plan-time selection is index lookup + arithmetic. |
| **I6** — synchronous plan hot path | `22 §4`'s algorithm has no I/O, no `.await`, no catalog call. All inputs are already in the `SemanticManifest`. |
| **I8** — SemanticManifest planner-complete | `ResolvedGrainsetDataKind` carries everything the planner needs: pre-sorted child roster, per-child `natural_grain`, per-child Coverage projection, pre-built Semantics → candidate-children index. |
| **I10** — non-exhaustive | `GrainsetChild`, `RollupPolicy`, and the composition-kind tag are `#[non_exhaustive]`. |
| **I12** — first-class diagnostics | `22`'s error set lives in `VALID_E_22xx` / `COMP_E_22xx` / `PLAN_E_22xx`. §§7–9 enumerate. |

### 1.5 Peer landscape

- **Cube.js `preAggregations` with `granularity`.** The closest analog: the author declares rolled-up pre-aggregations at different grains; Cube's query router picks the coarsest pre-aggregation that answers the query. `22`'s child-selection is structurally equivalent, re-expressed in canonical vocabulary (children are top-level DataKinds with Bindings; Grain is the Y-axis; Coverage is the X-axis; rollup legality is shape-gated per `17`).
- **dbt MetricFlow `time_spine` + dimension grains.** MetricFlow declares dimension `granularity` on each model; a Request at a coarser grain rolls up from a finer-grain model when no native-grain model is available. `22` adopts the same "cheapest-child-that-covers" discipline; the differences are (i) semstrait routes to one child (no multi-child merge within a Grainset), (ii) semstrait's Coverage captures the horizontal (per-Semantics) axis that MetricFlow does not surface as a first-class concept.
- **OLAP aggregate-awareness (Kimball).** The idea — materialize multiple cubes at different grains and route to the cheapest — is decades old. `22`'s contribution is the canonical-layer encoding: grain is a typed `13 §3` enum, Coverage is a per-Semantics subset check, cost is a pluggable function.

---

## 2. The `Grainset` variant

### 2.1 Model-layer struct

```rust
/// The Grainset variant of `ComplexDataKind`. Composes N children
/// exposing the same semantic surface at different grains and
/// routes a Request to exactly one child at plan time.
///
/// Cannot be a leaf (per `20 §*`); MUST declare at least one child
/// (§7 `VALID_E_2201`).
#[non_exhaustive]
pub struct GrainsetDataKind {
    /// Children in declaration order. Order is significant: it is
    /// the tie-break axis in §4.5.
    pub children: Vec<GrainsetChild>,

    /// The grain-axis `Dimension` name that every child's natural
    /// grain is measured against. The named Dimension MUST be
    /// declared on the Grainset's `SemanticInterface` with
    /// `DimensionType::Temporal` (`13 §4.2`) and MUST be the axis
    /// carried by each child's `grain:` declaration.
    pub grain_axis: SemanticsName,

    /// Rollup policy for the Grainset. Governs which children the
    /// planner may roll up from when no native-grain child exists
    /// for the Request. See §4.3.
    pub rollup_policy: RollupPolicy,
}

/// One child of a Grainset.
#[non_exhaustive]
pub struct GrainsetChild {
    /// Reference to a top-level `DataKind` (Simple, Unionset,
    /// Grainset, or Joinset — subject to `12 §2`'s nesting matrix).
    /// Resolved at `compile` to a `DataKindRef`.
    pub constituent: DataKindRef,

    /// Natural grain of this child on the Grainset's `grain_axis`.
    /// When omitted in YAML, inherited from the child's own
    /// temporal-Dimension `grains:` declaration — §3.2 details the
    /// inheritance.
    pub grain: Grain,

    /// Per-child Coverage of the Grainset's composed surface:
    /// which `SemanticsName`s this child natively provides.
    /// Populated at `compile` from `16 §8`'s `CompositionCoverage`
    /// fold; not author-declared.
    pub coverage: CompositionCoverage,

    /// Per-child rollup override. When `None`, inherits
    /// `GrainsetDataKind.rollup_policy`. See §4.3.
    pub rollup_override: Option<RollupPolicy>,
}

/// Rollup policy. Governs whether the planner may roll a finer-grain
/// child up to a coarser request grain, and — for shapes that need
/// it — how to anchor the rollup.
#[non_exhaustive]
pub enum RollupPolicy {
    /// Default. The planner may roll up per `TemporalShape` rules
    /// (§5): Timeseries / Events roll freely; Snapshot pins at its
    /// source grain; SCD requires as-of anchoring.
    ShapeDefault,

    /// Suppress rollup. A child is only eligible when its natural
    /// grain matches the request grain exactly. Useful for pinned
    /// snapshots where rollup is never correct.
    PinOnly,

    /// Force rollup. The planner MAY roll up even when a native-grain
    /// child exists. Rare; used for data-quality workflows where the
    /// finer-grain source is preferred even at higher scan cost.
    PreferFinest,
}
```

### 2.2 SemanticManifest-layer counterpart

Per `20 §*` and the `Resolved*` prefix convention in `00 §4.1`:

```rust
/// SemanticManifest-layer Grainset. Planner-optimized: children are indexed
/// by natural grain and pre-sorted for the eligibility scan.
pub struct ResolvedGrainsetDataKind {
    pub children: Vec<ResolvedGrainsetChild>,
    pub grain_axis: SemanticsName,
    pub rollup_policy: RollupPolicy,

    /// Index from a requested `SemanticsName` to the set of child
    /// indices that natively cover it (Coverage in {Native, Derived}).
    /// Populated at `compile`; O(1) per-Semantics probe at plan time.
    pub semantics_to_covering_children: HashMap<SemanticsName, Vec<usize>>,

    /// Children pre-sorted by natural grain coarseness ascending
    /// (`13 §3.2`). The index-into-`children` sequence; not a
    /// re-ordering of `children` itself.
    pub children_by_grain_ascending: Vec<usize>,

    /// The composed semantic interface exposed by the Grainset —
    /// per `16 §5`, with `composition_kind = CompositionKind::Grainset`.
    pub interface: ComposedSemanticInterface,
}

pub struct ResolvedGrainsetChild {
    pub constituent: DataKindRef,
    pub grain: Grain,
    pub coverage: CompositionCoverage,
    pub rollup_override: Option<RollupPolicy>,
}
```

`children` preserves the author's declaration order (§4.5 tie-break axis). `children_by_grain_ascending` is the plan-time fast path for "walk children coarsest-to-finest at or below the request grain"; it does not re-order the source `children`.

### 2.3 Composition wiring

A `ResolvedGrainsetDataKind` composes into `16`'s `ComposedSemanticInterface` via the `CompositionKind::Grainset` discriminator:

- `constituents` — the child `DataKindRef`s in declaration order.
- `interface: UnifiedSemantics` — every child exposes the same logical Semantics surface; the `UnifiedSemantics` merge per `16 §6` is trivial for Grainset (the promotion is always `Shared` when all children declare the field, or `Native` with `FieldOwnership::Native(only_child)` when only some do).
- `provenance: FieldProvenance` — per-field ownership; `FieldOwnership::Shared` for fields every child covers, `FieldOwnership::Native(child)` for singletons.
- `coverage: CompositionCoverage` — folded from each child's Binding-level or composition-level Coverage per `16 §8.4`. Entries with `CoverageVariant::NullFill` are **legal on a Grainset** (a child that lacks a specific Semantics may still be eligible when the Request does not name that Semantics; §4.2 is precise).
- `traversed_paths` — empty. Grainset composition is not path-walk-derived.
- `composition_kind = CompositionKind::Grainset`.

### 2.4 What Grainset is NOT

- **Not a Unionset.** A Unionset produces rows from every eligible branch and merges them with UNION ALL (per `23`). A Grainset selects **one** child and delegates. The two compose: a Grainset child may be a Unionset, and a Unionset branch may be a Grainset.
- **Not a Joinset.** Grainset children are not joined; they are alternatives. Selection is exclusive.
- **Not a materialized view.** The Grainset's role is to pre-declare where the author has arranged rolled-up sources; the planner reads that declaration and picks. Creation and refresh of those rolled-up sources is outside semstrait (per `00 §10`).

---

## 3. Child declaration

### 3.1 YAML surface

A Grainset is authored under a top-level `grainsets:` block (the full Model grammar lives in `32`):

```yaml
grainsets:
  - name: paid_media_rollups
    grain_axis: report_date
    rollup_policy: shape_default      # optional; default shown
    dimensions:
      - name: report_date
        data_type: date
        type:
          temporal:
            grains: [day, week, month, quarter, year]
      - name: campaign_id
        data_type: string
    measures:
      - name: cost
        data_type: decimal(18, 4)
        aggregation: sum
      - name: clicks
        data_type: long
        aggregation: sum
    children:
      - kind: paid_media_daily_events    # DataKindRef
        grain: day
      - kind: paid_media_daily_snapshot
        grain: day
      - kind: paid_media_monthly_snapshot
        grain: month
        rollup_override: pin_only
```

The Grainset's `SemanticInterface` block (dimensions / measures / metrics / keys / filters) is authored on the Grainset itself — this is the **composed surface**. Each child's own interface must be a superset of the composed surface on the Semantics it claims to cover (§6).

### 3.2 Per-child `grain:` inheritance

When a child's `grain:` key is omitted, the natural grain is inherited from the child's own `grain_axis` temporal Dimension declaration:

- If the child is a `Simple` kind, the child's `SemanticInterface` must declare the `grain_axis` Dimension with `type: temporal` and a non-empty `grains:` list. The inherited natural grain is the **finest** grain in that list (per `13 §3.2`'s total order).
- If the child is a `ComplexDataKind`, the inherited natural grain is the finest grain in the child's composed-surface `grain_axis` Dimension's `grains:` list.
- If the child's `grain_axis` declaration has no `grains:` (the "temporal only in data type, not for rollup" case per `13 §4.2`), inheritance **fails**: the child MUST declare `grain:` explicitly. Failure: `VALID_E_2204 GrainsetChildGrainUnresolvable`.

**Rationale for "finest as default".** The finest grain is the most permissive: a child at finer grain can be rolled up to any coarser request grain (shape permitting, §5); a child at coarser grain cannot be dis-aggregated. Defaulting to finest reflects the common case (the author wrote the underlying Binding against a fact-level source) and fails loudly when ambiguous.

### 3.3 `Coverage` is compile-time-derived, not author-declared

The per-child `coverage: CompositionCoverage` field on `GrainsetChild` is **not** declared in YAML. It is folded at `compile` from the child's Binding-level Coverage (`15 §6`) for Simple children, or from the child's composition-level Coverage (`16 §8`) for Complex children, projected onto the Grainset's composed-surface Semantics set.

Concretely, for each Semantics name `S` on the Grainset's composed surface, the compile-time fold computes the child's coverage of `S` per `16 §8.4`'s coverage-fold rules:

- `Native` — the child directly provides `S`.
- `Derived` — the child computes `S` via a `PhysicalExpr` referencing columns present on (at least one of) its sources.
- `Metadata` — the child provides `S` as a compile-resolved metadata literal (e.g. path-token) per `15 §5.5` / `15 §8`. Folds identically to `Native` / `Derived` (per `16 §8.4`).
- `NullFill` — the child does not provide `S` at all.

`Native`, `Derived`, and `Metadata` entries participate in the eligibility predicate (§4.2); `NullFill` entries participate only under the partial-coverage fallback rule (§4.2 step 3).

### 3.4 Constituent references

`GrainsetChild.constituent` is a `DataKindRef` resolved at `compile` to a `DataKindId`. The reference MUST name a top-level DataKind; inline-declared sub-kinds are not permitted here (authors declare each child as a top-level kind and reference it by name, consistent with `11 §5`'s Scope rules).

`12 §2`'s nesting matrix governs which DataKind variants may be children. Round-1 cells:

| Child kind | Permitted as Grainset child? |
|---|---|
| `Simple` (`Dataset`) | Yes (common case). |
| `Unionset` | Yes (per-grain Unionset of multi-source bindings). |
| `Joinset` | Yes (per-grain Joinset of fact × dimension). |
| `Grainset` | **Deferred** — `[TD-GRAINSET-NESTED]`. `12 §2` ratifies the current cell; `25`'s applicability matrix surfaces the decision. |

When a child is itself a Complex kind, the Grainset delegates through the child's composed surface; the child's own strategy handles everything below (union merging for Unionset, join path for Joinset).

---

## 4. Child selection algorithm

The core planner strategy for Grainset. Runs per-Request, synchronously, against `ResolvedGrainsetDataKind` and the `SemanticManifest` indices.

### 4.1 Request grain extraction

**Inputs:** the `Request` (canonical-layer), the Grainset's `grain_axis`, the Grainset's `grain_axis` Dimension declaration.

**Output:** `RequestGrain` — one of `Some(Grain)` or `None` (grain-axis not queried).

The extraction rule:

```text
REQUEST_GRAIN_EXTRACT(request, grainset):
  axis ← grainset.grain_axis

  // (a) Explicit grain selector on the grouped Dimension.
  //     Authors write `group_by: [{ name: report_date, grain: month }]`
  //     (YAML surface is `32`'s); at the canonical layer, the Request
  //     carries the grain selector on the grouped Dimension.
  for each grouped_dim in request.group_by:
    if grouped_dim.name == axis and grouped_dim.grain is Some(g):
      return Some(g)

  // (b) Temporal filter narrowing the grain implicitly.
  //     E.g. `WHERE DATE_TRUNC(report_date, 'month') = '2024-01-01'`
  //     — the DATE_TRUNC fixes the request grain at Month.
  for each filter in request.filters:
    if filter references axis via a DATE_TRUNC / similar canonical
       function whose grain argument is a literal `g`:
      return Some(g)

  // (c) Bare grouping by the axis without a grain selector.
  //     The request grain is implicitly the finest grain declared
  //     on the Grainset's grain_axis Dimension.
  for each grouped_dim in request.group_by:
    if grouped_dim.name == axis:
      return Some(finest(grainset.grain_axis.grains))

  // (d) Grain-axis not grouped and not filtered by grain.
  //     The planner has no grain constraint; selection defaults
  //     to "coarsest available" (the cheapest child). See §4.4.
  return None
```

The `REQUEST_GRAIN_EXTRACT` function is deterministic: the first-hit wins, in the order (a) → (b) → (c) → (d). When a Request's shape yields `None`, §4.4's cost function still picks a child (the cheapest by coverage); the Grainset does not require the request to name a grain.

**Cross-kind grain extraction.** When a Grainset participates in an implicit composition (`16 §11`) or as a Joinset member (`24`), the containing composition's grain-axis determination defers to each constituent Grainset's own `REQUEST_GRAIN_EXTRACT` pass — the function runs once per Grainset on the full Request, not once per composition.

### 4.2 Eligibility

A child is **eligible** for a Request iff:

1. **Grain admissibility.** `child.grain <= request.grain` on `Grain::coarseness()` (`13 §3.2`), OR `request.grain` is `None` (no grain constraint).
2. **Coverage admissibility.** Every `SemanticsName` named in the Request — in `group_by`, `aggregations`, `filters`, `order_by`, or `select` — has a Coverage entry on the child in `{Native, Derived}`.
3. **Rollup legality.** Per-child shape (`17`) permits rollup from `child.grain` to `request.grain`. §4.3 details.

The coverage admissibility check is a **deterministic subset check**: `RequestedSemantics ⊆ NativeOrDerivedSemantics(child)`. The SemanticManifest's `semantics_to_covering_children` index (§2.2) serves as the reverse-lookup: for each requested Semantics, enumerate the child indices that cover it Natively or Derivedly; intersect across all requested Semantics; the result is the eligibility set before grain and shape filtering.

```text
ELIGIBILITY(request, grainset):
  requested ← extract_requested_semantics(request)
  request_grain ← REQUEST_GRAIN_EXTRACT(request, grainset)

  candidates ← full set of child indices
  for each s in requested:
    covering ← grainset.semantics_to_covering_children.get(s)
    if covering is empty:
      return Err(PLAN_E_2202 NoChildCoversSemantics(s))
    candidates ← candidates ∩ covering

  candidates ← candidates.filter(|i|
    request_grain is None
    || grainset.children[i].grain <= request_grain
  )

  candidates ← candidates.filter(|i|
    ROLLUP_LEGAL(grainset.children[i], request_grain)   // §4.3
  )

  return candidates
```

**Partial-coverage fallback.** If the coverage intersection is empty AND the Grainset's `rollup_policy` permits (default `ShapeDefault`), the planner MAY split the Request across children: each child serves a subset of the requested Semantics, and the Grainset emits a per-subset delegation. Round 1 **does not** ratify this mode — it collapses Grainset's "one child, one plan" semantics into Unionset territory. The Round-1 behavior is `PLAN_E_2201 NoEligibleChild`; cross-child splitting is tracked as `[TD-GRAINSET-PARTIAL-COVERAGE]` (see open question `Q-GRN-002`).

### 4.3 Rollup legality

Per-child shape-gated check. Full matrix lives in `17`; `22` records the per-shape rule and the error surface.

```text
ROLLUP_LEGAL(child, request_grain):
  policy ← child.rollup_override.unwrap_or(grainset.rollup_policy)
  match policy:
    PinOnly:
      return request_grain == Some(child.grain)
    PreferFinest:
      return request_grain is None || child.grain <= request_grain
    ShapeDefault:
      return SHAPE_ROLLUP_LEGAL(child.temporal_shape, child.grain, request_grain)
```

Where `SHAPE_ROLLUP_LEGAL` is defined by `17` and summarized in `22 §5`. The key rule: **Snapshot pins at its source grain**; rolling a Snapshot up without a pin policy is `PLAN_E_2205 SnapshotRollupWithoutPin`.

The `ShapeDefault` evaluation consults the child's `TemporalShape` as recorded on the child's DataKind (at the `ResolvedDataKind` level; see `20 §*` for the shape-field placement). For Simple children, the shape is authored directly; for Complex children, the shape is derived per `17`'s composition rules (e.g. a Unionset's shape is the common shape of its branches, or `None` if mixed).

### 4.4 Cost function (v1 = source-count proxy)

After §4.2 produces an eligible-candidate set, the planner ranks candidates by **cost**. The v1 cost function is a deliberate proxy:

```text
COST(child, request):
  // v1: estimated number of PhysicalSources the child's sub-plan
  //     will scan for this request. For Simple children, this is
  //     the cardinality of the child's ResolvedBinding.sources list
  //     after Coverage pruning (per `15 §6`). For Complex children,
  //     the sum of the same over each constituent's ResolvedBinding.
  n ← count_covered_sources(child, request)
  return n
```

**Rationale.** Stats-free: we do not have row counts, partition-file sizes, or histogram data at the canonical layer (`00 §10` defers cost-based optimization). Source count is the only count-like property every adapter agrees on and every Binding carries.

**Tiebreaker interaction.** Source-count cost is intentionally coarse — many real SemanticManifests will have multiple children with equal source counts. Ties at §4.4 are resolved by the §4.5 deterministic order.

**Forward-compatibility.** When stats-backed cost lands (`[TD-GRAINSET-COST-STATS]`), `COST` becomes a strategy-injectable function; the rest of §4 is unchanged. The hook will live on the planner's `Planner` trait per `34` (open question `Q-GRN-003`).

### 4.5 Tie-break deterministic order

When §4.4's cost-rank produces multiple children with equal cost, the tiebreaker is **declaration order** — the `Vec<GrainsetChild>` index:

```text
CHOOSE(candidates, costs):
  min_cost ← min(costs.values())
  tied ← candidates.filter(|i| costs[i] == min_cost)
  return min(tied)      // smallest index wins
```

Declaration order is the author's ergonomic knob: putting the most-preferred child first makes it the tiebreaker winner. The tie is **not an error** — it is a standard selection outcome. `PLAN_E_2203 AmbiguousChildChoice` is reserved for pathological cases where two children have identical grain, identical Coverage, identical cost, AND identical declaration position in a scenario the Round-1 surface forbids (specifically: a Grainset constructed by `compile` merging two author-written Grainsets — not a v1 feature). In v1 this error is never emitted; it is reserved for `[TD-GRAINSET-MERGE]`.

---

## 5. Interaction with `TemporalShape`

The shape-gated rollup legality matrix. Full ratification lives in `17`; this table is the `22`-facing summary. `17` is drafted in parallel; the cells below are the forward-reference contract `22` assumes.

| `TemporalShape` | Natural grain semantics | Rollup legality (via `ShapeDefault`) | Required anchoring | `22`-visible error (on illegal rollup) |
|---|---|---|---|---|
| `Timeseries` (dense indexed series) | The child's declared `grain` is the dense series period. | Rolls up freely to any coarser grain via bucket-then-aggregate at plan time. | None. | — (always legal when grain ≤ request-grain) |
| `Events` (sparse discrete occurrences) | The child's declared `grain` is the bucket period the author pre-rolled to (or the raw `Minute` for untrimmed events). | Rolls up freely: group by `DATE_TRUNC(axis, request_grain)`, aggregate. | None. | — |
| `Snapshot` (periodic full-state capture) | The child's declared `grain` IS the snapshot period. Snapshots are pinned: each row represents the entity state at exactly that snapshot time; aggregation across snapshots is **semantically unsafe** without a per-snapshot pin policy. | Only legal when `request_grain == child.grain` **OR** a pin policy declares how to aggregate (latest-snapshot-per-period, first-snapshot-per-period, all-snapshots-additive). | `SnapshotPinPolicy` per `17`. | `PLAN_E_2205 SnapshotRollupWithoutPin` |
| `SCD` (slowly-changing dimension with history) | The child has **no intrinsic grain**; `grain` declared on an SCD child is the grain at which the as-of anchor is evaluated. | Not rollable in the Timeseries/Events sense. Must be resolved via as-of anchoring against a Request-carried as-of timestamp. | `AsOfAnchor` per `17`. | `PLAN_E_2206 SCDRollupWithoutAsOf` |
| `None` / unspecified | No shape declared. | Legal per grain-coarseness alone; no pin, no anchor, no bucket. | None. | — |

**The `PinOnly` short-circuit.** Regardless of shape, a child with `rollup_override: PinOnly` is only ever eligible when `request_grain == child.grain` exactly. This is the Round-1 safety valve for authors who know their data cannot correctly roll up (e.g. a Snapshot without a defined pin policy) — they can declare `PinOnly` and the eligibility check excludes rolled-up candidates without needing to involve `17`'s pin machinery.

**Mixed-shape Grainsets.** A Grainset MAY declare children of different shapes (`PLAN_E_2207 MixedShapeAdvisoryChildren` is a **warning**, not an error — per §9). Eligibility per child is evaluated per-shape; the cost function compares them uniformly (source count). The Round-1 caveat: authors writing such Grainsets should ensure each shape's pin / anchor policies are explicit, or the resulting plans may surface confusing `PLAN_E_2205` / `PLAN_E_2206`.

---

## 6. Interaction with `Coverage`

### 6.1 Per-child projection from Binding / composition Coverage

`22`'s per-child Coverage is a **projection** onto the Grainset's composed surface:

```text
PROJECT_COVERAGE(child, grainset_semantics) -> CompositionCoverage:
  result ← CompositionCoverage::empty()
  for each s in grainset_semantics:
    variant ← if child is Simple:
      fold child's Binding-level Coverage (`15 §6.2`) for s
        across child's sources per `15 §6.2` / `16 §8.4` rules
    else if child is Complex:
      fold child's composition-level Coverage (`16 §8.4`) for s
        across child's constituents
    result.insert((child_ref, s), variant)
  return result
```

The Semantics set `grainset_semantics` is the Grainset's composed-surface name set (§2.3's `UnifiedSemantics`'s names). Names that appear on the child but not on the composed surface are ignored (the Grainset exposes only what it declares). Names on the composed surface absent from the child's interface register as `NullFill`.

### 6.2 Admissibility check

Per §4.2 step 2, the admissibility check is `RequestedSemantics ⊆ NativeOrDerivedSemantics(child)`. The SemanticManifest's `semantics_to_covering_children` index pre-computes, at `compile`, the reverse mapping: `SemanticsName → Vec<child_index>` filtered to children whose Coverage of the name is `Native` or `Derived`.

At plan time:

```text
NATIVELY_COVERED(grainset, s) -> Vec<child_index>:
  return grainset.semantics_to_covering_children
    .get(s)
    .unwrap_or(&empty)
```

Cost: O(1) HashMap probe per requested Semantics. The intersection across multiple requested Semantics is O(|requested| * |candidates|) in the worst case; for typical Grainsets (≤10 children, ≤50 Semantics) this is sub-microsecond.

### 6.3 Fallback when no child natively covers

If `ELIGIBILITY` (§4.2) returns an empty candidate set, the planner MUST emit `PLAN_E_2201 NoEligibleChild` without falling back to:

- Anonymous union across children (that's Unionset, §2.4).
- Cross-child column-wise composition (deferred, `[TD-GRAINSET-PARTIAL-COVERAGE]`).
- Schema-inferred joins between children (explicitly out of scope per `16 §9.3`).

The author's options are:

1. Add a child whose Coverage is a superset of the requested Semantics at the needed grain.
2. Express the query against a different DataKind (a Unionset, perhaps, or a Joinset).
3. Reduce the Request (drop a Semantics to satisfy `⊆ NativeOrDerived`).

---

## 7. Validation Preconditions

Run by `validate` against a `SemanticModel`. Accumulate all failures per `10 §3.2` before returning. Code range: `VALID_E_2200`–`2299` per `30 §6.2`.

| Code | Variant | Condition |
|---|---|---|
| `VALID_E_2200` | `GrainsetMissingGrainAxis { grainset }` | `grain_axis` field is empty or unparsable. |
| `VALID_E_2201` | `GrainsetNoChildren { grainset }` | `children:` list is empty. A Grainset with zero children is structurally nonsensical. |
| `VALID_E_2202` | `GrainsetChildDuplicateReference { grainset, constituent, count }` | The same `DataKindRef` appears as a child more than once. Round 1 forbids duplicates (no useful semantics for the planner). |
| `VALID_E_2203` | `GrainsetChildMalformedGrain { grainset, child_index, value }` | Child's inline `grain:` value is not a valid `Grain` variant per `13 §3.1`. |
| `VALID_E_2204` | `GrainsetChildGrainUnresolvable { grainset, child_index }` | Child omits `grain:` AND the child's own grain-axis temporal Dimension declaration has no `grains:` list to inherit from. §3.2. |
| `VALID_E_2205` | `GrainsetGrainAxisNotTemporal { grainset, axis }` | The Grainset's declared `grain_axis` names a Dimension whose `type:` is not `temporal` (per `13 §4.2`). |
| `VALID_E_2206` | `GrainsetInvalidRollupPolicy { grainset, value }` | Authored `rollup_policy:` / `rollup_override:` string does not match any `RollupPolicy` variant. |
| `VALID_E_2207` | `GrainsetChildSelfReference { grainset, child_index }` | Child's `DataKindRef` is the Grainset itself. Direct self-reference is forbidden; nested Grainset-of-Grainset is deferred (`[TD-GRAINSET-NESTED]`). |

### 7.1 Typed-variant surface

Per `10 §4`, `validate`-stage failures surface as `ValidateError` variants on `32`'s public API. The mapping above is the canonical variant name; `32` may attach additional Location / SourceSpan context per `30 §5`.

---

## 8. Compile Preconditions

Run during `compile` after `validate` passes. Fail-fast per `10 §3.3`. Code range: `COMP_E_2200`–`2299` per `30 §6.2`.

| Code | Variant | Condition |
|---|---|---|
| `COMP_E_2200` | `GrainsetChildUnresolved { grainset, child_index, name }` | A child's `DataKindRef` does not resolve to any top-level DataKind in the SemanticManifest. |
| `COMP_E_2201` | `GrainsetGrainAxisMissingOnInterface { grainset, axis }` | `grain_axis` names a Semantics not on the Grainset's own interface. |
| `COMP_E_2202` | `GrainsetGrainAxisMissingOnChild { grainset, child_index, axis }` | A child's interface lacks the Grainset's `grain_axis` Semantics (the child does not expose the grain axis at all). |
| `COMP_E_2203` | `GrainsetChildGrainNotInAxis { grainset, child_index, grain, axis_grains }` | Child's declared `grain` is not a member of the Grainset's `grain_axis.grains:` list. Every child's grain must be one of the grains the Grainset declares. |
| `COMP_E_2204` | `GrainsetSemanticShapeConflict { grainset, child_index, semantics, expected, found }` | Child's `SemanticInterface` / `ComposedSemanticInterface` disagrees with the Grainset's composed surface on a shape axis (`DataType`, `DimensionType`, `Constraint` — full list per `11 §5` / `13 §2.4`). |
| `COMP_E_2205` | `GrainsetCoverageUnion { grainset, missing_semantics }` | The union of all children's Coverage fails to natively cover (at `Native` or `Derived`) a Semantics on the Grainset's composed surface. If no child ever provides `s`, `s` cannot be answered at any grain — configuration bug. |
| `COMP_E_2206` | `GrainsetGrainAxisTypeConflict { grainset, child_index, child_type, axis_type }` | Grain-axis `DataType` on the child does not unify with the Grainset's axis type under `13 §2.4`. |
| `COMP_E_2207` | `GrainsetNestedGrainsetChild { grainset, child_index, child_ref }` | Child `DataKindRef` resolves to another `Grainset`; deferred (`[TD-GRAINSET-NESTED]`, see §3.4). |
| `COMP_E_2208` | `GrainsetChildShapeUnknown { grainset, child_index }` | `rollup_policy: ShapeDefault` applies and the child has no `TemporalShape` declared AND the Request-time shape fallback rule (`17`) does not apply. Reported at `compile` because the shape is known statically. |
| `COMP_E_2209` | `GrainsetChildrenGrainAxisDivergent { grainset, child_indices, axes }` | Two children reference different Semantics names as their grain axis, OR the axis resolves to different canonical Dimensions in different children (after name-resolution collisions). |

### 8.1 SemanticManifest-index build

After Preconditions pass, `compile` builds the SemanticManifest indices consumed by §4.2 / §4.4:

1. Sort `children` by natural grain coarseness ascending → populate `children_by_grain_ascending`.
2. For each `SemanticsName` on the Grainset's composed surface, enumerate child indices with `CoverageVariant ∈ {Native, Derived}` → populate `semantics_to_covering_children`.
3. Fold each child's coverage per §6.1 → populate `GrainsetChild.coverage`.
4. Build `ComposedSemanticInterface` per `16 §5` → attach to `ResolvedGrainsetDataKind.interface`.

Index build is deterministic (I4): the sort is stable, the map is populated in declaration order, the interface follows `16`'s construction rules.

---

## 9. Plan-stage rules

Run during `plan` per `10 §3.4`. Synchronous, fail-fast. Code ranges: `PLAN_E_2200`–`2299` (errors) and `PLAN_W_2200`–`2299` (advisories) per `30 §6.2`.

### 9.1 Errors (`Severity::Error`)

| Code | Variant | Condition |
|---|---|---|
| `PLAN_E_2200` | `GrainsetNoMatchingChildByGrain { grainset, request_grain, child_grains }` | No child has `child.grain <= request_grain`. Every child is strictly coarser than the Request. |
| `PLAN_E_2201` | `NoEligibleChild { grainset, request, reasons }` | Post-§4.2, candidate set is empty. `reasons` enumerates per-child why (grain, coverage, shape). |
| `PLAN_E_2202` | `NoChildCoversSemantics { grainset, semantics }` | A requested Semantics has no child with `{Native, Derived}` coverage — compile's `COMP_E_2205` should have caught this for top-level surfaces, but may surface at plan time when the Semantics was added by an upstream composition layer. |
| `PLAN_E_2203` | `AmbiguousChildChoice { grainset, candidates, costs }` | Reserved for the compile-merged-Grainset case; never emitted in v1 (see §4.5, `[TD-GRAINSET-MERGE]`). |
| `PLAN_E_2204` | `GrainsetChildSubplanFailed { grainset, child_index, cause }` | The chosen child's sub-strategy failed to produce a plan. `cause` carries the child's own error. |
| `PLAN_E_2205` | `SnapshotRollupWithoutPin { grainset, child_index, child_grain, request_grain }` | `ShapeDefault` rollup of a `Snapshot`-shape child requires a pin policy (per `17`), and none is declared. |
| `PLAN_E_2206` | `SCDRollupWithoutAsOf { grainset, child_index, child_grain }` | `ShapeDefault` selection of an `SCD`-shape child requires an as-of anchor in the Request, and none is present. |
| `PLAN_E_2207` | `RequestGrainNotInAxisGrains { grainset, request_grain, axis_grains }` | The Request-extracted grain is not a member of the Grainset's `grain_axis.grains:` list. Authors cannot request a grain the Grainset did not declare. |
| `PLAN_E_2208` | `GrainsetPartialCoverageNotSupported { grainset, request, per_child_coverage }` | The Request requires cross-child column-wise composition; deferred per `[TD-GRAINSET-PARTIAL-COVERAGE]`. Raised instead of silently falling back. |

### 9.2 Advisories (`Severity::Warning`)

| Code | Variant | Condition |
|---|---|---|
| `PLAN_W_2200` | `GrainsetRollupUnusedChild { grainset, unused_child_indices }` | One or more children were eligible but a coarser-grain child won the cost rank. Informational: the author may want to prune unused children or understand why finer-grain children cost more. |
| `PLAN_W_2201` | `GrainsetTieBrokenByOrder { grainset, tied_children, chosen }` | Cost tie resolved by declaration-order tiebreak. Flags an opportunity for the author to make intent explicit (by reordering children or splitting the Grainset). |
| `PLAN_W_2202` | `MixedShapeAdvisoryChildren { grainset, shape_counts }` | Mixed `TemporalShape`s across children; planner surfaces and proceeds. |
| `PLAN_W_2203` | `RequestGrainAbsentUsingCoarsest { grainset, chosen_grain }` | Request carried no grain selector; the planner picked the coarsest-grain child via default cost logic. |

### 9.3 Severity policy

Per `30 §7`, errors abort planning; advisories flow through to the `Diagnostic` list on the resulting `SemanticPlan`. Advisories are the author's feedback channel for ergonomic issues that are not bugs.

---

## 10. Plan shape

Grainset's plan contribution is a **delegation**: the chosen child's sub-strategy produces a `PlanNode` subtree, and the Grainset splices it into the position in the overall `SemanticPlan` where the Grainset was queried.

### 10.1 Single-child delegation (common case)

For a Grainset directly queried via `Request.from = Some(grainset_ref)`:

```text
plan(request, grainset):
  candidates     ← ELIGIBILITY(request, grainset)        // §4.2
  if candidates.is_empty():
    return PLAN_E_2201 NoEligibleChild
  costs          ← candidates.map(|c| COST(c, request))  // §4.4
  chosen         ← CHOOSE(candidates, costs)             // §4.5
  sub_request    ← REWRITE_FOR_CHILD(request, chosen)    // §10.2
  sub_plan       ← plan(sub_request, chosen.constituent) // recurse
  return WRAP_WITH_GRAIN_ROLLUP(sub_plan, chosen.grain, request.grain)
```

`WRAP_WITH_GRAIN_ROLLUP` is a no-op when `chosen.grain == request.grain`, and wraps the sub-plan with a `Project(DATE_TRUNC(axis, request_grain))` + `Agg(group_by: request.group_by, aggregations: request.aggregations)` when `chosen.grain < request.grain` and the shape permits (`17`'s rollup rules; `Timeseries` / `Events` — the common case).

### 10.2 Request rewrite for the child

Before delegation, the Request is rewritten onto the child's surface:

- **`from:`** → the child's `DataKindRef`. The child takes over as the planner's target.
- **`group_by:`** → preserved; the child's own strategy handles the grain-axis binding.
- **`aggregations:` / `filters:` / `order_by:` / `select:`** → preserved verbatim; the child's Coverage must have already admitted every referenced Semantics per §4.2 step 2.
- **Grain selector on the grouped axis** → preserved; the `WRAP_WITH_GRAIN_ROLLUP` step handles the `DATE_TRUNC` if the child's grain is finer than the request grain.

### 10.3 ASCII plan trees

**Single-child Simple delegation, native grain** (no rollup needed):

```
PlanNode::Project [measures: cost, dims: report_date, campaign_id]
└─ PlanNode::Agg [group_by: (report_date, campaign_id), aggs: SUM(cost)]
   └─ PlanNode::Scan [source: paid_media_daily_snapshot, grain: day]
```

**Single-child Simple delegation, rolled up** (child grain = day, request grain = month):

```
PlanNode::Project [measures: cost, dims: report_date, campaign_id]
└─ PlanNode::Agg [group_by: (report_date, campaign_id), aggs: SUM(cost)]
   └─ PlanNode::Project [report_date := DATE_TRUNC(report_date, 'month'),
                         campaign_id, cost]
      └─ PlanNode::Scan [source: paid_media_daily_events, grain: day]
```

**Single-child Complex (Unionset) delegation, native grain**:

```
PlanNode::Project [measures: cost, dims: report_date]
└─ PlanNode::Agg [group_by: (report_date), aggs: SUM(cost)]
   └─ PlanNode::Union [inputs: {adwords_daily, facebook_daily, tiktok_daily},
                       distinct: false]
      ├─ PlanNode::Project [... NULL-fill per Unionset Coverage (`23`) ...]
      │  └─ PlanNode::Scan [adwords_daily]
      ├─ PlanNode::Project [...]
      │  └─ PlanNode::Scan [facebook_daily]
      └─ PlanNode::Project [...]
         └─ PlanNode::Scan [tiktok_daily]
```

In the Unionset-child case, the Grainset's work is entirely above the Union node — it picked the child (the Unionset), and the Unionset's own strategy (`23`) produced the Union subtree.

### 10.4 What Grainset does NOT emit

- **No UNION across Grainset children.** Child selection is exclusive; the non-chosen children contribute no rows.
- **No Join across Grainset children.** Children are alternatives, not joinable.
- **No implicit composition traversal.** If the Request names Semantics outside the Grainset's composed surface, `PLAN_E_0506 RequestOutOfSurface` (per `16 §14.3`) fires — the Grainset does not silently extend via a Relationship walk.

### 10.5 Splicing into an enclosing plan

When a Grainset is queried indirectly (as a constituent of a Joinset, as a child of another Grainset — once `[TD-GRAINSET-NESTED]` lands, as a Unionset branch), the chosen-child's sub-plan is the value the Grainset returns to the enclosing strategy. The enclosing strategy treats the sub-plan as an opaque `PlanNode` subtree and composes it per its own rules (`23`'s UNION ALL, `24`'s join path, `22` recursive delegation, etc.).

The splice is structural, not syntactic: there is no "Grainset node" in the `PlanNode` enum. The Grainset is a **planner strategy**, not a node type. The output is the chosen child's subtree, optionally wrapped with `Project`/`Agg` for rollup per §10.1.

---

## 11. Worked example

### 11.1 Model

```yaml
# Top-level Simple kinds (children of the Grainset)
datasets:
  - name: paid_media_hourly_events
    temporal_shape: events
    binding:
      sources:
        - glob: "s3://lake/events/year=*/month=*/day=*/hour=*/*.parquet"
        - format: parquet
      column_mapping:
        report_date: event_timestamp
        campaign_id: campaign_id
        cost: cost_cents
        clicks: click_count
    dimensions:
      - name: report_date
        data_type: timestamp(0)
        type:
          temporal:
            grains: [minute, hour, day, week, month, quarter, year]
      - name: campaign_id
        data_type: string
    measures:
      - name: cost
        data_type: decimal(18, 4)
        aggregation: sum
      - name: clicks
        data_type: long
        aggregation: sum

  - name: paid_media_daily_snapshot
    temporal_shape: snapshot
    binding:
      sources:
        - table: "warehouse.paid_media.daily_rollup"
      column_mapping:
        report_date: snapshot_date
        campaign_id: campaign_id
        cost: daily_cost
        clicks: daily_clicks
    dimensions:
      - name: report_date
        data_type: date
        type:
          temporal:
            grains: [day, week, month, quarter, year]
      - name: campaign_id
        data_type: string
    measures:
      - name: cost
        data_type: decimal(18, 4)
        aggregation: sum
      - name: clicks
        data_type: long
        aggregation: sum

  - name: paid_media_monthly_snapshot
    temporal_shape: snapshot
    binding:
      sources:
        - table: "warehouse.paid_media.monthly_rollup"
      column_mapping:
        report_date: month_start
        campaign_id: campaign_id
        cost: monthly_cost
    dimensions:
      - name: report_date
        data_type: date
        type:
          temporal:
            grains: [month, quarter, year]
      - name: campaign_id
        data_type: string
    measures:
      - name: cost
        data_type: decimal(18, 4)
        aggregation: sum
      # note: no `clicks` measure on the monthly snapshot

# The Grainset composing the three
grainsets:
  - name: paid_media_rollups
    grain_axis: report_date
    rollup_policy: shape_default
    dimensions:
      - name: report_date
        data_type: date
        type:
          temporal:
            grains: [day, week, month, quarter, year]
      - name: campaign_id
        data_type: string
    measures:
      - name: cost
        data_type: decimal(18, 4)
        aggregation: sum
      - name: clicks
        data_type: long
        aggregation: sum
    children:
      - kind: paid_media_hourly_events
        # grain: inherited — finest of [minute..year] is minute
      - kind: paid_media_daily_snapshot
        # grain: inherited — day
        rollup_override: pin_only   # this snapshot must be queried at its declared grain or rolled up only if 17's pin policy applies
      - kind: paid_media_monthly_snapshot
        # grain: inherited — month
        rollup_override: pin_only
```

### 11.2 Request A — daily cost

```
Request {
  from: Some(paid_media_rollups),
  group_by: [{ name: report_date, grain: day }],
  aggregations: [sum(cost)],
  filters: [],
}
```

**Walkthrough:**

1. `REQUEST_GRAIN_EXTRACT` → `Some(Day)` (rule (a)).
2. **Coverage admissibility.** Requested Semantics: `{report_date, cost}`. All three children cover both Natively. Candidates: `[0, 1, 2]`.
3. **Grain admissibility.** `hourly_events.grain = Minute` ≤ Day. `daily_snapshot.grain = Day` ≤ Day. `monthly_snapshot.grain = Month` > Day → **excluded**. Candidates: `[0, 1]`.
4. **Rollup legality.**
   - Child 0 (`hourly_events`, Events shape, `ShapeDefault`) → rolls freely via bucket-then-aggregate. Legal.
   - Child 1 (`daily_snapshot`, Snapshot shape, `PinOnly`) → `request_grain == child.grain == Day`. Legal.
5. **Cost.**
   - `hourly_events`: ~365 sources per year (hourly-partitioned parquet files) for a 1-day filter, but without a filter, the full glob. Suppose 8760.
   - `daily_snapshot`: 1 source (a table). Cost 1.
6. **CHOOSE.** Cost-min → `daily_snapshot` (index 1). No tie.

**Plan:**

```
PlanNode::Project [measures: cost, dims: report_date]
└─ PlanNode::Agg [group_by: (report_date), aggs: SUM(cost)]
   └─ PlanNode::Scan [source: warehouse.paid_media.daily_rollup, grain: day]
```

### 11.3 Request B — monthly cost

```
Request {
  from: Some(paid_media_rollups),
  group_by: [{ name: report_date, grain: month }],
  aggregations: [sum(cost)],
  filters: [],
}
```

**Walkthrough:**

1. `REQUEST_GRAIN_EXTRACT` → `Some(Month)`.
2. **Coverage admissibility.** `{report_date, cost}`: all three children cover Natively. Candidates: `[0, 1, 2]`.
3. **Grain admissibility.** All three children have `grain <= Month`. Candidates: `[0, 1, 2]`.
4. **Rollup legality.**
   - Child 0 (`hourly_events`, Events, `ShapeDefault`) → rolls freely. Legal.
   - Child 1 (`daily_snapshot`, Snapshot, `PinOnly`) → `request_grain = Month`, `child.grain = Day`. `PinOnly` requires equality → **excluded**.
   - Child 2 (`monthly_snapshot`, Snapshot, `PinOnly`) → `request_grain == child.grain == Month`. Legal.
5. **Cost.**
   - `hourly_events`: 8760 sources.
   - `monthly_snapshot`: 1 source.
6. **CHOOSE.** `monthly_snapshot` (index 2) wins.

**Plan:**

```
PlanNode::Project [measures: cost, dims: report_date]
└─ PlanNode::Agg [group_by: (report_date), aggs: SUM(cost)]
   └─ PlanNode::Scan [source: warehouse.paid_media.monthly_rollup, grain: month]
```

### 11.4 Request C — monthly cost + clicks

```
Request {
  from: Some(paid_media_rollups),
  group_by: [{ name: report_date, grain: month }],
  aggregations: [sum(cost), sum(clicks)],
  filters: [],
}
```

**Walkthrough:**

1. `REQUEST_GRAIN_EXTRACT` → `Some(Month)`.
2. **Coverage admissibility.** `clicks` is NOT natively covered by `monthly_snapshot`. `semantics_to_covering_children[clicks] = [0, 1]`. Candidates intersected: `[0, 1]` (index 2 excluded by coverage).
3. **Grain admissibility.** `hourly_events` → Month: ok. `daily_snapshot` → Month: ok.
4. **Rollup legality.** `daily_snapshot` is `PinOnly`, request is Month: excluded. Candidates: `[0]`.
5. **Cost.** `hourly_events` is the only candidate.
6. **CHOOSE.** `hourly_events`.

**Plan:**

```
PlanNode::Project [measures: cost, clicks, dims: report_date]
└─ PlanNode::Agg [group_by: (report_date), aggs: SUM(cost), SUM(clicks)]
   └─ PlanNode::Project [report_date := DATE_TRUNC(report_date, 'month'),
                         campaign_id, cost, clicks]
      └─ PlanNode::Scan [source: s3://lake/events/...,
                         grain: minute, shape: events]
```

Advisory `PLAN_W_2200 GrainsetRollupUnusedChild(unused_child_indices: [1, 2])` flows through — the snapshots were ineligible once `clicks` was requested, not because they were too coarse-grained. The advisory informs the author that the monthly snapshot's lack of `clicks` forced a scan of the full Events source; they may want to add `clicks` to the monthly rollup.

### 11.5 Request D — hourly cost

```
Request {
  from: Some(paid_media_rollups),
  group_by: [{ name: report_date, grain: hour }],
  aggregations: [sum(cost)],
  filters: [],
}
```

**Walkthrough:**

1. `REQUEST_GRAIN_EXTRACT` → `Some(Hour)`.
2. **Coverage admissibility.** Candidates: `[0, 1, 2]`.
3. **Grain admissibility.** `hourly_events.grain = Minute ≤ Hour`: ok. `daily_snapshot.grain = Day > Hour`: excluded. `monthly_snapshot.grain = Month > Hour`: excluded.
4. **Rollup legality.** `hourly_events`: legal.
5. **Cost.** Only candidate.
6. **CHOOSE.** `hourly_events`.

Had `hourly_events` not existed, step 3 would leave Candidates empty → `PLAN_E_2200 GrainsetNoMatchingChildByGrain` (no child at or below Hour).

---

## 12. Round-1 open items

Parked in `docs/design/questions/open/22_questions.md`. Each entry restates the question, lists its refs, and records `22`'s Round-1 default. Entries migrate out of the file as later docs (`17`, `20`, `25`, `33`, `34`) ratify decisions that confirm or amend `22`'s defaults.

Summary of titles:

- **Q-GRN-001** — Inheritance default for child `grain`: finest vs declared (§3.2).
- **Q-GRN-002** — Cross-child partial coverage: error in v1, or split-and-delegate? (`[TD-GRAINSET-PARTIAL-COVERAGE]`, §4.2 / §9.1 `PLAN_E_2208`).
- **Q-GRN-003** — Cost function pluggability hook site: planner trait or adapter hook? (`[TD-GRAINSET-COST-STATS]`, §4.4).
- **Q-GRN-004** — Grainset-of-Grainset nesting (`[TD-GRAINSET-NESTED]`, §3.4 / `COMP_E_2207`).
- **Q-GRN-005** — Mixed-shape Grainsets: warning vs error (§5 / `PLAN_W_2202`).
- **Q-GRN-006** — Single-child Grainset degeneracy: lint or accept? (§7 open item).

---

## 13. Cross-References

- `00 §4.1` — `Grainset` row in the canonical vocabulary.
- `00 §9` — I1, I4, I5, I6, I8, I10, I12 (all upheld per §1.4).
- `10 §3.2` / `§3.3` / `§3.4` — per-stage contracts; §§7–10 fit each.
- `11 §5` — Scope rules; `DataKindRef` resolution for `GrainsetChild.constituent`.
- `12 §2` — nesting matrix; the Grainset-child permitted-kinds cell.
- `13 §3.1` / `§3.2` — `Grain` enum and coarseness order; `13 §4.2` — `TemporalDimension.grains`.
- `14` / `14a` / `14b` — expressions; `22` consumes pre-resolved `PhysicalExpr`s via child Bindings.
- `15 §6` — Binding-level Coverage fold; §6.1 projection consumes it.
- `16 §5` — `ComposedSemanticInterface`; `16 §6` `UnifiedSemantics`; `16 §7` `FieldProvenance`; `16 §8` `CompositionCoverage` fold.
- `17` *(parallel)* — shape-gated rollup legality matrix, SCD / Snapshot anchoring policies.
- `20` *(parallel)* — taxonomy-level Complex-DataKind invariants; `ResolvedComplexDataKind` envelope.
- `23` — Unionset; Grainset children may be Unionsets.
- `24` — Joinset; Grainset children may be Joinsets, and a Grainset may participate in a Joinset path.
- `25` — per-kind applicability matrix; Grainset cell.
- `30 §5` / `§6.2` / `§7` — `Diagnostic` shape, `22xx` code-range allocation, severity policy.
- `33` — `SemanticManifest` / `ResolvedGrainsetDataKind` persistence.
- `34` — planner entry-point dispatching Grainset strategy.
- `35` — `PlanNode` subtree composition.

---

**End of document.** Open items in `docs/design/questions/open/22_questions.md`.
