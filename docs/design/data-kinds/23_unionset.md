---
prereqs: [00, 11, 13, 14, 15, 16, 17, 18, 20, 21, 26, 32]
authoritative-for:
  - the `Unionset` variant's authoring surface — which fields are explicit (authored) vs. implicit (compile-derived); cross-references the type-level shape in `32 §3.3` (`Unionset` / `NestedUnionset` concrete forms), `32 §3.2` (`UnionsetBody`), and `32 §4` (`ComplexExtras`)
  - the variant-local `UnionMode` v1 roster (`{All, Unique}`, default `All`, `#[non_exhaustive]`) — cross-referenced from `32 §3.2`
  - the v1 inference-only per-child Coverage model — no authored per-child override; fold from each child's own interface + Binding-level Coverage; default behavior is non-strict (a Semantics not provided by some child is NullFilled in that child's branch)
  - the canonical fan-out mechanism shared between explicit Unionsets and implicit Unionsets (multi-source `Dataset` per `21 §3.2`) — per-branch sub-aggregation + UNION/UNION ALL + conditional final-aggregation, with literals + per-source `Metadata` driving disjointness elision
  - the V1 strict TemporalShape equivalence rule — children's shape (kind including SCD subtype + grain) must be equivalent; cascade-up from children to the Unionset's effective `extras.temporal:`; future smart alignment is post-v1
  - plan-time observable invariants for Unionset resolution — per-source pre-aggregation always emitted when Measures / Metrics are requested; NULL-fill projection at the seam; column-type reconciliation
  - the Unionset-specific Precondition surfaces — `VALID_E_2300`–`2399` (structural), `COMP_E_2300`–`2399` (compile), `PLAN_E_2300`–`2399` (plan)
refined-by:
  - 22 (Grainset — composes Dataset / Unionset-level resolution; shares the per-source pre-aggregation discipline at level boundaries)
  - 24 (Joinset — composes Dataset / Unionset / Grainset-level resolution; Joinset is the authoritative carrier of per-hop composition mechanics that `23` deliberately does not author)
  - 25 (Applicability matrix — per-variant consumption cells citing `23`'s consumer contract)
  - 33 (`semstrait-manifest` — `ResolvedDataKind` family persistence; `Unionset`'s resolved shape and implicit-Unionset hash-id assignment live there)
  - 34 (`semstrait-planner` — concrete `plan` entry point and `UnionsetStrategy` algorithm body)
  - 35 (`semstrait-ir` — `PlanNode::{Project, Filter, Agg, Union}` variants emitted by `UnionsetStrategy`; `UnionMode` projection to engine semantics)
  - 36 (`semstrait-adapter` — engine rendering of `UNION` / `UNION ALL` semantics)
---

# 23. Unionset (`ComplexDataKind`)

> **Reconciliation (post-thirteenth-pass cascade, 2026-05-03).** The v1 authoring-layer canonical shape for `Unionset` is ratified across:
>
> - `[20_taxonomy.md §2](./20_taxonomy.md)` — sealed trait hierarchy (`DataKind`, `ComplexDataKind`, `PublicDataKind`, `NestedDataKind`); `Unionset` implements `ComplexDataKind + PublicDataKind`, `NestedUnionset` implements `ComplexDataKind + NestedDataKind`.
> - `[../apis/32_semstrait_model.md §3.2](../apis/32_semstrait_model.md)` — `UnionsetBody { base: DataKindBase<ComplexExtras>, datasets, grainsets, joinsets, mode: UnionMode }`. Children are inlined `Nested`* structs (no `DataKindRef` mechanism for cross-Complex child references in v1).
> - `[../apis/32_semstrait_model.md §3.3](../apis/32_semstrait_model.md)` — `Unionset` and `NestedUnionset` concrete forms.
> - `[../apis/32_semstrait_model.md §4](../apis/32_semstrait_model.md)` — `ComplexExtras { temporal: Option<TemporalShape> }`; Unionset authors only `temporal:` in `extras` (no `catalog` / `storage` / `semantic_mapping`, per R-6 / SR-5).
> - `[../foundations/18_entities.md](../foundations/18_entities.md)` — canonical entity types consumed by `UnionsetBody`'s SemanticInterface: Dimensions / Measures / Metrics / Keys / Filters; inline-vs-`ref` grammar for entity-level reuse.
> - `[26_nesting_matrix.md](./26_nesting_matrix.md)` — nesting matrix; R1 (leaves don't nest), R2 (no same-variant self-nesting; type-level field absence in `UnionsetBody`), R3 (`ComplexDataKind` requires ≥ 2 children).
>
> `23` retains authority for:
>
> - The `Unionset` authoring surface — explicit (authored) vs. implicit (compile-derived) fields, with cross-references to the type-level shape in `32 §3` / `32 §4`.
> - The variant-local `UnionMode { All, Unique }` enum (per `32 §3.2`'s deferral note).
> - Per-child Coverage inference (no authored override in v1).
> - The canonical fan-out mechanism shared with multi-source `Dataset` (per `21 §3.2`).
> - V1 strict TemporalShape equivalence rule (kind incl. SCD subtype + grain across all children).
> - Plan-time observable invariants of Unionset resolution.
> - Variant-specific Precondition surfaces — `VALID_E_2300`–`2399`, `COMP_E_2300`–`2399`, `PLAN_E_2300`–`2399`.
>
> **Algorithm body relocated.** The `UnionsetStrategy` algorithm body (per-branch wrapping, NULL-fill projection emission, column-type reconciliation, branch pruning, conditional final-aggregation predicate) lives in `[../apis/34_semstrait_planner.md §<UnionsetStrategy>](../apis/34_semstrait_planner.md)` (forthcoming). This rebase parked the body in `[../_drafts/34_unionset_strategy.md](../_drafts/34_unionset_strategy.md)` pending the `34` drafting pass.
>
> **Concepts retired in this rebase.** `CompositionKind`, `ComposedSemanticInterface` (as a Unionset-level concept; `ComposedSemanticInterface` survives as a Joinset-only per-hop join-path-search artifact per `24`), `ChildCoverageOverride { provides }`, the YAML `coverage:` block on child entries, `DataKindRef`-based child references — all were pre-cascade artifacts not authored by the user. They have no slot in the v1 authoring surface or in `23`'s contract.

## 1. Purpose and Scope

### 1.1 What `23` ratifies

`23` is the per-variant chapter for `Unionset` (one of three `ComplexDataKind` variants in v1, alongside `Grainset` (`22`) and `Joinset` (`24`)). It owns four things: (a) the **authoring surface** — explicit vs. implicit fields the author / consumer sees on the type — with cross-references to the type-level shape in `32 §3` (forms + body) and `32 §4` (`ComplexExtras`); (b) the variant-local `**UnionMode { All, Unique }`** enum (per `32 §3.2`'s deferral); (c) the **plan-time observable invariants** of Unionset resolution — what an author or consumer can rely on from the emitted plan, *not* the algorithm body; (d) **Unionset-specific Preconditions** in the per-variant code bands `VALID_E_2300`–`2399` (validate), `COMP_E_2300`–`2399` (compile), `PLAN_E_2300`–`2399` (plan).

### 1.2 What `23` does NOT ratify

The sealed trait hierarchy and shared `DataKind` invariants live in `20`. The Rust struct / YAML shape lives in `32 §3` / `32 §4`. Child-DataKind structural rules (R1 / R2 / R3, addressing scheme) live in `26`. Per-child Binding mechanics live in `15`. The `UnionsetStrategy` algorithm body lives in `34 §<UnionsetStrategy>` (forthcoming; parked in `_drafts/34_unionset_strategy.md`). `TemporalShape` enum variants, SCD subtype catalog, shape × grain rollup matrix live in `17`. `PlanNode` shapes live in `35`. Engine rendering lives in `36`. Peer-vocabulary references (Cube.js `unionAll`, dbt MetricFlow, LookML, DataFusion `LogicalPlan::Union`, Substrait `SetRel`) live in `00_overview.md` only.

### 1.3 Guardrails — how `23` upholds `00 §9` invariants


| Invariant                                  | Where `23` keeps it                                                                                                                                                                                                                                                                                                                            |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **I1** — no raw SQL in canonical layer     | The emitted plan tree carries `PlanNode::Union` (with the `UnionMode` flag projected), per-branch `PlanNode::Project` (carrying `PhysicalExpr::Cast(Null, T)` for NullFilled columns), and `PlanNode::Agg` for terminal re-aggregation; no SQL-shaped strings. Adapter rendering (`36`) is the only SQL site.                                  |
| **I2** — physical types via adapters only  | Column-type reconciliation (§4.3) uses canonical `DataType` per `13 §2`; LUB widening uses `13 §7`'s cast matrix. No engine types.                                                                                                                                                                                                             |
| **I3** — no engine branching               | Zero engine-identity checks in `23`. Every decision reads either resolved manifest indices or per-child Coverage (compile-derived).                                                                                                                                                                                                            |
| **I4** — SemanticManifest determinism      | Child ordering follows **name order** across the `datasets:` / `grainsets:` / `joinsets:` collections (id-keyed, name-ordered projection per `32 §7`, post id-first rework). NULL-fill projections walk the Unionset's own `SemanticInterface` in name order. The implicit-Unionset (multi-source `Dataset`) per-source order follows `15 §3.6`'s lexical resolution. |
| **I5** — compile-time resolution           | Per-child Coverage is folded once at compile (§3.2); column-type LUB is computed once at compile (§4.3); per-branch projection rosters are pre-built. Plan-time is O(1) lookups per Semantics.                                                                                                                                                 |
| **I6** — synchronous hot path              | No I/O at any stage of Unionset resolution.                                                                                                                                                                                                                                                                                                    |
| **I8** — planner-complete SemanticManifest | `Unionset` resolution touches only the resolved manifest; no YAML, no catalog. Implicit-Unionset hash-ids (per `33 §<implicit-unionset-id>`, forthcoming) are compile-assigned and stable.                                                                                                                                                     |
| **I10** — non-exhaustive public sum types  | `UnionMode` is `#[non_exhaustive]` per `32 §3.2`; `Unionset` and `NestedUnionset` are `#[non_exhaustive]` per `32 §3.3`. Adding `UnionMode::Intersect` or `UnionMode::ByName` is MINOR per `30 §6.3`.                                                                                                                                          |
| **I12** — first-class Diagnostics          | Every error code in §§7–9 is stable and carries a `Diagnostic.location`.                                                                                                                                                                                                                                                                       |


## 2. Authoring-Time Surface

### 2.1 Explicit vs. implicit fields

The `Unionset` author touches the following surface. Type-level shape lives in `32 §3.3` (`Unionset` / `NestedUnionset` concrete forms), `32 §3.2` (`UnionsetBody`), and `32 §4` (`ComplexExtras`); `23` enumerates obligations and cross-references foundations for substance.


| #   | Axis               | Explicit (authored)                                                                                                                                                                                                                                                                                                                                                                                        | Implicit (compile-derived)                                                                                                                                                                                                                                                                                          | Cite                                     |
| --- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| A   | Identity           | `body.base.name` (Public form: globally unique across top-level `data_kinds:` per SR-3; Nested form: scoped to parent's nested-kind scope per `26 §4`).                                                                                                                                                                                                                                                    | `id: EntityId` for explicit Unionsets (the model `id` per `33 §6.1`); `id` for **implicit** Unionsets (a deterministic content-derived id over the parent `Dataset`'s structural identity + per-source list, per `33 §9.1`). Implicit Unionsets carry no `name` and live as **top-level** entries in `data_kinds: BTreeMap<EntityId, DataKind>` with `origin = DataKindOrigin::Implicit` (per C9.4 + C9.5); they are **not** carried inside any `NestedDataKind`. | `32 §3.1`, `32 §3.3`, `26 §4`, `21 §3.2` |
| B   | Description        | `description: Option<String>` on `Unionset` (Public form only; absent on `NestedUnionset` per `26 §3` "Nested-form structural-only" rule).                                                                                                                                                                                                                                                                 | None.                                                                                                                                                                                                                                                                                                               | `32 §3.3`                                |
| C   | Semantic interface | `semantic_interface: SemanticInterface` on `Unionset` (Public form only; absent on `NestedUnionset` per `26 §3`). Authors Dimensions, Measures, Metrics, Filters, Keys per `18 §1`–`§2`. Components flatten in YAML per `#[serde(flatten)]`.                                                                                                                                                               | Per-Semantics inferred-type fields, typed-leaf substitution for `- ref: <name>` entries, Metric expansion, Computed-Dimension expression resolution per `19 §3`.                                                                                                                                                       | `32 §3.3`, `18 §1`–`§2`, `19 §3`           |
| D   | AI context         | `ai_context: Option<AiContext>` on `Unionset` (Public form only).                                                                                                                                                                                                                                                                                                                                          | None.                                                                                                                                                                                                                                                                                                               | `32 §3.3`, `18 §6`                       |
| E   | Mode               | `body.mode: UnionMode` (default `All`; v1 roster `{All, Unique}` per `§4.1`).                                                                                                                                                                                                                                                                                                                              | None.                                                                                                                                                                                                                                                                                                               | `32 §3.2`, `§4.1`                        |
| F   | Children           | `body.{datasets, grainsets, joinsets}: BTreeMap<EntityId, Nested*>` — children are **inlined** `NestedDataset` / `NestedGrainset` / `NestedJoinset` structs per `26`'s nesting matrix (Unionset admits all three Complex variants except itself per R2). Cross-Complex `DataKindRef` references are not part of v1; entity-level `- ref:` for shared root-pool Dimensions / Measures / Metrics is unaffected (`18 §1.1`). | Per-child Coverage inference (§3.2); per-child resolved interface (Simple → `15 §10`; Complex → child's own resolution).                                                                                                                                                                                            | `32 §3.2`, `26 §1`, `26 §2`, `18 §1.1`   |
| G   | TemporalShape      | `extras.temporal: Option<TemporalShape>` on `ComplexExtras`. Per the V1 strict equivalence rule (§5.2), the Unionset's effective `temporal.kind` is the cascade-up of children's shapes (which must all be equivalent); explicit authoring at the Unionset level is permitted and must agree with children.                                                                                                | Cascade-up from children: when authoring is silent at the Unionset level, the effective shape is read from the (unanimous) children. `grain` is leaf-only (`LeafExtras` carries it; `ComplexExtras` does not — per `32 §4` table).                                                                                  | `32 §4`, `17`, `18 §3.3`, `26 §3.1`      |


**Public vs. Nested form distinction.** Rows B, C, D apply only to the Public form (`Unionset`). The Nested form (`NestedUnionset`) carries only `body` (which contains `base.name`, `extras: ComplexExtras`, `mode: UnionMode`, and the nested child arrays); fields B/C/D are structurally absent per `26 §3`. The `name` at nested scope is the structural anchor in the parent's nested-kind scope (`26 §4` addressing).

> **Phase 3 confirmation (2026-05-28; cascade from C6.5 + C9.4 + C9.5).** Row A's "implicit Unionsets carry a content-derived hash `data_kind_id`" claim is reaffirmed for the manifest-resident shape: implicit Unionsets (multi-source `Dataset` auto-synthesis per `21 §3.2`) live as **top-level entries** in `data_kinds: BTreeMap<EntityId, DataKind>` per [`../apis/33_semstrait_manifest.md §4.1`](../apis/33_semstrait_manifest.md), tagged with `origin: DataKindOrigin::Implicit` per C9.5, and structurally identical to explicit `DataKindVariant::Unionset` (per `33 §6.5`). They are **not** inlined as `NestedDataKind` (C6.5). When a Grainset level needs to reference an implicit Unionset (for the ≥2 same-grain children case per C8.1), it does so via `RoutingUnitRef::Synthesized(EntityId)` rather than `RoutingUnitRef::Inline(NestedDataKind)`. C9.4 confirms the persistence is unchanged from C6's closing note — the manifest top-level placement is canonical, not under elaboration.

### 2.2 Nesting interaction

A `Unionset`:

- **MAY** appear at Root scope as a top-level public DataKind (per `32 §2.1`'s `unionsets:` plural tag).
- **MAY** be nested inline as a `NestedUnionset` under a `Grainset` or `Joinset` (per `26 §1`'s matrix). Nested form is structural-only (no `description` / `semantic_interface` / `ai_context`); the nested `name` is the structural anchor.
- **MUST NOT** be nested under another `Unionset` (R2; type-level — no `unionsets:` field on `UnionsetBody` per `26 §2.2`).
- **MUST** carry **≥ 2 children** across `datasets:` / `grainsets:` / `joinsets:` combined (R3 per `26 §2.3`; structural-rule diagnostic owned by `26` / `32`).

### 2.3 Authoring example — full SemanticInterface surface

A worked YAML showing every authoring slot a `Unionset` exposes (focus is **completeness of the surface**, not plan emission — see §10 for the plan-shape worked example):

```yaml
unionsets:
  - name: paid_media
    description: "Cross-platform paid media activity, normalized."
    mode: all
    dimensions:
      - name: ordered_at
        data_type: Timestamp
      - name: source_platform
        data_type: String
      - name: campaign_id
        data_type: String
    measures:
      - name: cost
        agg: sum
        expr: cost_cents
        data_type: Long
      - name: impressions
        agg: sum
        expr: impressions_count
        data_type: Long
    metrics:
      - name: cpm
        expr: cost / impressions * 1000
        data_type: Decimal
    keys:
      - name: campaign_id
        kind: foreign
    filters:
      - name: paid_only
        expr: cost > 0
    extras:
      temporal:
        events:
          occurred_at: ordered_at
    datasets:
      - name: adwords_daily
        extras:
          catalog: marketing_warehouse
          storage:
            format: Parquet
            paths: ["s3://b/adwords/*.parquet"]
          semantic_mapping:
            ordered_at: order_date
            source_platform: { literal: "adwords" }
            campaign_id: campaign_pk
            cost_cents: total_cost
            impressions_count: total_impressions
          temporal:
            events:
              occurred_at: order_date
              grain: Day
      - name: facebook_daily
        extras:
          catalog: marketing_warehouse
          storage:
            format: Parquet
            paths: ["s3://b/facebook/*.parquet"]
          semantic_mapping:
            ordered_at: ts
            source_platform: { literal: "facebook" }
            campaign_id: cmpg_id
            cost_cents: spend_cents
            impressions_count: imps
          temporal:
            events:
              occurred_at: ts
              grain: Day
```

Notes on this example:

- `dimensions:` / `measures:` / `metrics:` / `keys:` / `filters:` are **flat** at the Unionset level per `#[serde(flatten)]` on the SemanticInterface (per `32 §3.3`).
- `mode: all` is the default and may be omitted; shown explicitly here for clarity.
- Each nested `datasets:` entry inlines a `NestedDataset` per `32 §3.2`. Each authors its own `extras.semantic_mapping:` to project its physical columns onto the Unionset's surface Semantics.
- `source_platform` is mapped to a per-source **literal** in each child — this is the per-source distinguishing metadata mechanism that drives disjointness elision at the final-aggregation layer (§4.4). Per-source `Metadata` recipes (path-token extraction per `15 §8`) are an alternative carrier.
- Children's `temporal:` blocks must be equivalent per the V1 strict equivalence rule (§5.2) — same `events:` shape, same `grain: Day`. The Unionset's `extras.temporal.events:` cascades up to match.

## 3. Per-Child Contract

### 3.1 Children list shape and ordering

The `UnionsetBody` carries three child maps — `datasets: BTreeMap<EntityId, NestedDataset>`, `grainsets: BTreeMap<EntityId, NestedGrainset>`, `joinsets: BTreeMap<EntityId, NestedJoinset>` (per `32 §3.2`). The **canonical child sequence** for plan emission is:

1. all `datasets:` entries in name order, then
2. all `grainsets:` entries in name order, then
3. all `joinsets:` entries in name order.

This order is stable (per `32 §7` ordering table — collections are id-keyed but projected name-ordered) and is the order branches appear in the emitted `PlanNode::Union`'s `inputs` (§4.2). The intra-variant ordering basis changed from YAML author order to name order with the id-first model rework (`32 §7`); it remains deterministic.

R2 (no same-variant self-nesting) is type-level absence per `32 §3.2` — a `unionsets:` field does not exist on `UnionsetBody`. R3 (≥ 2 children) is the validate-stage rule per `26 §2.3` (diagnostic `validate.complex-data-kind-insufficient-children`); `23` does not re-codify. Nested-child name uniqueness within a single Unionset's combined child set is the Unionset author's responsibility; collisions surface via the `26 §4` addressing scheme as ambiguous addresses.

### 3.2 Per-child Coverage — v1 inference-only

V1 carries **no authored per-child Coverage override**. The `coverage:` block on child entries (and the `ChildCoverageOverride { provides }` shape) from pre-cascade `23` are retired. Per-child Coverage is inferred at compile from each child's own interface plus the child's Binding-level Coverage (for Simple children) or its resolved composed interface (for Complex children).

**Per-Semantics inference rule.** For each composed-surface Semantics `s` declared on the Unionset's own `SemanticInterface`, and for each child `i`:

- If child `i`'s interface declares a Semantics with the same name `s` (or a `- ref: s` to a root-pool entry shared by the Unionset) AND the child can produce a value (per `15 §10` for Simple, per the child's own resolution for Complex), then child `i` **provides** `s`.
- Otherwise, child `i` **NullFills** `s` — `s` is absent from the child's projection and a typed-NULL constant is emitted at the seam (§4.3).

This is the **default non-strict** behavior: any Semantics provided by ≥ 1 child is queryable on the Unionset; non-providers emit NULL at the seam.

**Coverage-completeness check.** A composed-surface Semantics that NO child provides yields `COMP_E_2302 UnionsetCoverageIncomplete` (§8) — the column would be always-NULL across every branch, indicating an authoring mistake.

### 3.3 Implicit-vs-explicit Coverage discipline

A second branch of the Unionset contract exists for **implicit Unionsets** — created by the planner when a `Dataset` has multiple `PhysicalSource`s (currently the only origin per `21 §3.2`). The implicit case differs from the explicit case in one respect:

- **Explicit Unionset** — non-strict per-child Coverage as above. Per-child branches NullFill missing Semantics.
- **Implicit Unionset** — **strict** per-source SemanticMapping coverage. Every per-source pseudo-leaf must serve every Semantics in the parent `Dataset`'s `extras.semantic_mapping:`; absence is `COMP_E_2106 SimpleMultiSourceIncompatibleNullFill` per `21 §3.2`.

The strict-vs-non-strict distinction is an internal planner discriminator (the `strict` flag); it is **never authored** in YAML. Authors who need NullFill across heterogeneous physical sources wrap their `Dataset`s in an explicit Unionset per `21 §3.2`'s remediation.

## 4. Plan-Time Observable Behavior

A Unionset's plan-time observable behavior is the realization of `UnionsetStrategy::resolve` (algorithm body in `34 §<UnionsetStrategy>`, parked in `_drafts/34_unionset_strategy.md` pending `34` drafting). This section ratifies the observable invariants an author or consumer can rely on; the algorithm body is referenced only for derivation.

### 4.1 `UnionMode { All, Unique }`

The variant-local enum, deferred from `32 §3.2`:

```rust
#[non_exhaustive]
pub enum UnionMode {
    /// Preserve duplicates across branches.
    /// Engine rendering: `UNION ALL`.
    All,

    /// Deduplicate across branches via the engine's natural sort-merge or
    /// hash-aggregate mechanism. Engine rendering: `UNION` (without `ALL`).
    /// Three-valued-logic NULL semantics apply: rows differing only in
    /// NullFilled positions do NOT dedupe (NULL ≠ NULL); see §4.3.
    Unique,
}
```

`All` is the default — declared via `32 §3.2`'s field comment and surfaced in §2.1 row E. `Unique` corresponds to the engine's natural `UNION` default (sort-merge / hash-aggregate dedupe per the engine survey in `00_overview.md`). `#[non_exhaustive]` per I10; future variants (`Intersect`, `ByName`, etc.) tracked as MINOR additions per `30 §6.3` and Q-UNI-008.

### 4.2 Per-source pre-aggregation — always emitted when Measures requested

When a Request names a Measure or Metric on the Unionset's surface, `UnionsetStrategy` emits **per-branch sub-aggregation** under every branch's `Project` (the seam wrapper). This is unconditional: pre-aggregation is the canonical path, not an optimizer judgment, because unioning raw rows and then aggregating wastes compute relative to per-branch partial-aggregate then merge. The contract holds for both explicit Unionsets and implicit Unionsets (`21 §3.2`'s multi-source `Dataset` case).

The aggregate expressions are placed by their `ExprLayer` (`19 §6.0`): `Aggregate`-layer Measures lift into the per-branch `Agg`; a `PostAggregate`-layer Metric's residual stays above the final aggregation (its constituent aggregates pre-aggregate per branch). Pre-aggregate eligibility and the re-aggregation operator come from **function-derived** `Additivity` in v1 (`19 §6.5`, `14a §3.6.2`) — `Additive` Measures (`Sum`/`Count`/`Min`/`Max`) pre-aggregate then re-aggregate; `NonAdditive` (`Avg`) does not.

A Request that names only Dimensions (no Measures, no Metrics) skips per-branch aggregation — the branches' `Project`s flow directly into the `PlanNode::Union`, with a final post-Union de-duplication implicit in `UnionMode::Unique` or absent in `UnionMode::All`.

### 4.3 NULL-fill projection at the seam

For each composed-surface Semantics `s` that a child does NOT provide (per §3.2), the child's branch `Project` (the wrap-for-union seam) emits `PhysicalExpr::Cast(PhysicalExpr::Literal(LiteralValue::Null), unified_type(s))` at the column position. The `unified_type(s)` is the column-type-reconciled type per §4.4. The typed NULL is carried as a `PhysicalExpr` tree (no SQL string per I1); adapter rendering (`36`) translates to the engine's NULL idiom.

**Branch projection ordering.** The wrap-for-union step orders columns according to a deterministic walk over the Unionset's own `SemanticInterface` (author-declared Semantics order). Every branch projects in the same order — required for `PlanNode::Union` to align inputs positionally.

`**UnionMode::Unique` interaction.** Two branches' rows that agree on every Native column but differ in NullFilled positions are NOT deduplicated under `Unique`, because three-valued-logic treats `NULL = NULL` as UNKNOWN (not TRUE) and `Unique` mode dedupes only when comparison is TRUE. Authors expecting full deduplication across partial-coverage branches see advisory `PLAN_W_2303 UnionsetUniqueThreeValuedNullCollision` (§9).

### 4.4 Column-type reconciliation

Per composed-surface Semantics `s`, the unified branch column type is derived once at compile:

- **Pass-through fast path** (per `14 §5.4`) — if every contributing child's `DataType` for `s` is identical, the unified type is that shared type; no `Cast` needed.
- **Widening** (per `13 §7`) — if contributors' types differ but are pairwise cast-compatible under `13 §7`'s widening rules, the unified type is the LUB. `UnionsetStrategy` wraps non-LUB-typed contributors in `Cast(<col>, <lub>)` inside the seam `Project`. LUB selection follows `14`'s promotion lattice.
- **Incompatible** — if any pair is not cast-compatible under `13 §7`, `COMP_E_2303 UnionsetCrossChildTypeDisagreement` fires at compile (§8).
- **Single contributor** — if exactly one child provides `s`, the unified type is that child's type. **No contributor** — caught earlier as `COMP_E_2302 UnionsetCoverageIncomplete` per §3.2.

**Nullability.** If any contributor reports the column as nullable OR the column is NullFilled in any branch, the unified column is nullable. Authors see advisory `COMP_W_2301 UnionsetNullabilityWidened` (§8) when widening occurs.

### 4.5 Re-aggregation policy — final aggregation conditional

After the per-branch sub-aggregation (§4.2) and the `PlanNode::Union`, a **final aggregation** above the Union is emitted **conditionally**:

- **Required** when the Request names Measures / Metrics AND the planner cannot prove from literals + per-source `Metadata` that branches' GROUP BY keys are disjoint.
- **Elided** when disjointness is provable — per-branch partial aggregates are already correct. Provability requires every branch to project a literal or per-source `Metadata` Dimension whose value is unique across branches (e.g. `source_platform: { literal: "adwords" }` in one branch and `{ literal: "facebook" }` in another).

For Dimensions-only Requests, no aggregation is emitted at any layer (no per-branch sub-agg, no final agg).

**Re-aggregation function inference.** Per-Measure aggregation function over the union'd partials follows the table:


| Per-branch `agg` | Final agg          | Correctness                                                                                           |
| ---------------- | ------------------ | ----------------------------------------------------------------------------------------------------- |
| `Sum`            | `Sum`              | Exact                                                                                                 |
| `Count`          | `Sum`              | Exact (sum of partial counts)                                                                         |
| `Min`            | `Min`              | Exact                                                                                                 |
| `Max`            | `Max`              | Exact                                                                                                 |
| `CountDistinct`  | `Sum`              | **Lossy** (overcounts when branches' row spaces overlap); `PLAN_W_2302` advisory                      |
| `Avg`            | (not decomposable) | `PLAN_E_2304 UnionsetReAggregationInfeasible` — author restructures as a Metric `Sum(num) / Sum(den)` |


The lossy `CountDistinct` case fires `PLAN_W_2302 UnionsetReAggregationLossy` (§9). The `Avg` case is a hard error directing authors to the Metric idiom; mathematically correct cross-branch averaging requires explicit numerator / denominator aggregates.

### 4.6 Coverage-driven branch pruning (advisory)

When Coverage analysis (§3.2) shows that a child contributes only NullFills for every Request-selected Semantics, that branch's contribution is rows of typed NULLs; the final `PlanNode::Union` would inflate row counts with all-NULL rows. `UnionsetStrategy` prunes the branch and emits advisory `PLAN_W_2301 UnionsetBranchPrunable` (§9). The pruned branch's subplan is not constructed.

**Exceptions.**

- A Request whose only Measure is `Count(*)` (i.e. row-counting) is sensitive to NULL rows; pruning would change the result. Pruning suppressed; the all-NULL branch is preserved.
- For `UnionMode::Unique`, pruning is always safe — an all-NULL branch collapses under deduplication to at most one row, which may be removed without altering the dedup result.

If pruning collapses the surviving branch set to zero, `PLAN_E_2303 UnionsetRequestTotallyNullFilled` (§9). This is exceptional; the Coverage-completeness check (§3.2) catches most instances at compile.

### 4.7 Implicit Unionset fan-out — `21 §3.2` cross-link

The mechanism described in §§4.2–4.6 is identical for implicit Unionsets created by multi-source `Dataset` resolution (`21 §3.2`). The differences from the explicit case:

- Children are per-source pseudo-leaves (one per `PhysicalSource` resolved from `extras.storage.paths:` / `extras.storage.tables:`), not user-authored `Nested*` structs.
- Mode is hard-coded `All` (the implicit `mode: all`).
- Coverage discipline is **strict** (`21 §3.2`'s `COMP_E_2106` rather than `23 §8`'s NullFill-tolerant inference).
- Identity is a content-derived hash-id (per `33 §<implicit-unionset-id>`, forthcoming) rather than an author-declared `name`.

The plan shape is structurally identical: per-source `Project` (with literals + per-source `Metadata` driving disjointness), `PlanNode::Union { mode: All }`, and conditional final-aggregation. `21 §10` carries the worked example for the implicit case.

## 5. TemporalShape Interaction

### 5.1 Authoring placement and cascade-up

A Unionset's effective `TemporalShape` is **inherited from its children** under the V1 strict equivalence rule (§5.2). Authoring at the Unionset level is permitted via `extras.temporal:` (`ComplexExtras`) and must agree with children's; silent authoring at the Unionset level reads the (unanimous) children's shape as the effective value.

Per `26 §3.1` and the `32 §4` cascade table, only `temporal.<variant>:` (the shape kind) cascades; `grain` is leaf-only and lives on each leaf descendant's `LeafExtras` (`32 §4`).

### 5.2 V1 strict equivalence rule

In V1, all children of a Unionset MUST have **equivalent** TemporalShape:

- **Equal `kind`** — `Timeseries`, `Events`, `Snapshot`, or `Scd` — must match across all children.
- **Equal `ScdType`** when `kind = Scd` — `Scd(Type1) + Scd(Type2)` is rejected; subtype must match.
- **Equal `grain`** when present on leaf children — `grain: Day` and `grain: Hour` cannot coexist.

Mismatch is a hard compile error: `COMP_E_2301 UnionsetChildShapeMismatch { unionset, children: Vec<(name, TemporalShape)> }` (§8). Single consolidated code (per C1 ratification, 2026-05-03) covering all equivalence violations (kind, subtype, grain).

**Future direction.** Smart alignment of non-equivalent shapes (e.g. `Scd + Snapshot` projecting `Snapshot` as a degenerate `Scd(Type1)`) is post-v1 and tracked as `[TD-UNIONSET-SHAPE-ALIGN]` in `open/23_questions.md`.

### 5.3 Multi-as-of `Snapshot` advisories — survive equivalence rule

Two `Snapshot` children with equivalent shape (same `kind`, same `grain`) but different as-of timestamps are equivalent under §5.2 — yet semantically heterogeneous (rows mix two distinct snapshot worlds). Two advisories survive:

- `COMP_W_2302 UnionsetSnapshotMultipleAsOf` — the composed surface exposes the snapshot's identifying Dimension (`snapshotted_at` per `18 §3.3`); rows are distinguishable but the Union mixes worlds.
- `COMP_W_2303 UnionsetSnapshotMultipleAsOfNoDiscriminator` — the composed surface does NOT expose `snapshotted_at`; rows are indistinguishable.

Both are warnings. (Per C2 ratification, 2026-05-03.)

### 5.4 Scope boundary with `17`

`23` ratifies only:

- the `extras.temporal:` field carriage on `ComplexExtras` (cross-ref `32 §4`),
- the V1 strict equivalence rule across children (§5.2),
- the cascade-up direction (children → Unionset).

`23` does NOT ratify the `TemporalShape` enum variants, the SCD subtype catalog, the shape × grain rollup matrix, the `AsOf` join variant, or the advisory-warning predicates beyond the multi-as-of cases above. All of these live in `17`.

## 6. Grain Interaction

### 6.1 Cascade — equal grain across children

`grain` lives on each leaf descendant's `LeafExtras` (per `32 §4`); Complex variants do not author `grain` directly. Under §5.2's strict equivalence rule, the union of every leaf descendant under a Unionset must yield a single common `grain`. Mismatch is `COMP_E_2301` (the same consolidated code; grain is part of TemporalShape per `17`).

### 6.2 Plan-time consultation

Two observable invariants involve `grain`:

- **Rollup coarsening.** When a Request rolls up a temporal Dimension on the Unionset's surface, the target grain must be at least as coarse as the children's common `grain`. A `Day`-grain Unionset rejects an `Hour`-target rollup — same code reused from `21 §6.2` (`PLAN_E_2102 RequestGrainFinerThanSource`); the diagnostic locator points at the Unionset rather than a single Dataset.
- **Shape-gated legality.** When `temporal.kind` is also declared, the rollup is shape-gated per `17`'s matrix.

### 6.3 Scope boundary with `13` and `17`

- `13 §5` ratifies the `Grain` enum and its coarseness order.
- `17` ratifies the `TemporalShape × Grain` legality matrix.
- `22` ratifies grain routing (the Grainset-specific dispatch across children at different grains; out of scope for `23`'s equivalent-grain rule).

## 7. Validation Preconditions — `VALID_E_2300`–`2399`

Structural well-formedness for Unionsets is upstreamed to:

- **R1 / R2** (parse-stage type-level absence per `26 §2.1` / `§2.2`; diagnostics `parse.unknown-field` and `parse.illegal-self-nesting`).
- **R3** (validate-stage walk per `26 §2.3`; diagnostic `validate.complex-data-kind-insufficient-children`).
- **SR-3** name uniqueness, **SR-4 / SR-5** field validity, **SR-7** unknown-field rejection — all in `32 §6`.

Consequently the `VALID_E_2300`–`2399` band is **largely empty** in v1. The single Unionset-specific structural check that does not fit cleanly upstream:


| Code           | Variant                                                        | Trigger                                                                                                                                                                                                                                                            |
| -------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `VALID_E_2301` | `UnionsetDuplicateNestedChildName { unionset, name, indices }` | Two nested children of the same Unionset (across `datasets:` / `grainsets:` / `joinsets:`) declare the same `body.base.name`. The `26 §4` addressing scheme (`unionsets[X].datasets[Y]`) requires nested-name uniqueness within the parent's combined child scope. |


**Extensibility.** Codes `2302`–`2399` reserved; MINOR per `30 §6.3`.

## 8. Compile Preconditions — `COMP_E_2300`–`2399`

Compile-stage checks run after `validate` has passed and child resolution has produced per-child interface + Coverage data.


| Code          | Variant                                                                                                           | Trigger                                                                                                                                                                                                                                  |
| ------------- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `COMP_E_2301` | `UnionsetChildShapeMismatch { unionset, children: Vec<(NestedDataKindName, TemporalShape)> }`                     | One or more children's `TemporalShape` (kind, SCD subtype, or grain) diverges from the others. Single consolidated code per C1 (2026-05-03); covers every equivalence violation (kind, subtype, grain). Fail-fast.                       |
| `COMP_E_2302` | `UnionsetCoverageIncomplete { unionset, semantics }`                                                              | A composed-surface Semantics declared on the Unionset's own `SemanticInterface` is provided by NO child (every child NullFills). The column would be always-NULL across every branch — almost certainly an authoring mistake. Fail-fast. |
| `COMP_E_2303` | `UnionsetCrossChildTypeDisagreement { unionset, semantics, children_types: Vec<(NestedDataKindName, DataType)> }` | Two or more children both provide a Semantics with logically incompatible `DataType`s that cannot be reconciled by `13 §7`'s widening rules. Fail-fast.                                                                                  |
| `COMP_E_2304` | `UnionsetUniqueModeIncompatibleType { unionset, semantics, data_type }`                                           | `mode: unique` declared but the Unionset's surface carries a Semantics whose `DataType` is non-comparable (e.g. `Binary`); the engine cannot dedupe. Fail-fast.                                                                          |


**Compile-stage warnings.**


| Code          | Variant                                                                         | Trigger                                                                                                                                                                              |
| ------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `COMP_W_2301` | `UnionsetNullabilityWidened { unionset, semantics }`                            | Per §4.4: at least one child reports `s` as non-nullable but another contributor (or NullFill) forces nullability; the unified column is nullable. Survives equivalence rule per C2. |
| `COMP_W_2302` | `UnionsetSnapshotMultipleAsOf { unionset, snapshot_identifier }`                | Per §5.3: two `Snapshot` children with equivalent shape but different as-of timestamps; `snapshotted_at` is on the surface (rows distinguishable). Survives equivalence rule per C2. |
| `COMP_W_2303` | `UnionsetSnapshotMultipleAsOfNoDiscriminator { unionset, snapshot_identifier }` | Per §5.3: same as `COMP_W_2302` but `snapshotted_at` is NOT on the surface (rows indistinguishable). Survives equivalence rule per C2.                                               |


**Extensibility.** Codes `2305`–`2399` (errors) and `2304`–`2399` (warnings) reserved; MINOR per `30 §6.3`.

## 9. Plan-Stage Rules — `PLAN_E_2300`–`2399`

Plan-stage checks run against a specific `Request` and the resolved Unionset.


| Code          | Severity | Variant                                                          | Trigger                                                                                                                                                                                                                                              |
| ------------- | -------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PLAN_E_2301` | Error    | `UnionsetRequestFieldNotCovered { unionset, semantics }`         | A Request references a Semantics on the Unionset's surface whose composition-level Coverage resolves to NullFill from every child. Edge case; `COMP_E_2302` catches most instances at compile, but pathological Request field sets may slip through. |
| `PLAN_E_2303` | Error    | `UnionsetRequestTotallyNullFilled { unionset, request_fields }`  | Coverage-driven branch pruning (§4.6) collapsed every branch; zero branches survive. Indicates a Request / manifest inconsistency.                                                                                                                   |
| `PLAN_E_2304` | Error    | `UnionsetReAggregationInfeasible { unionset, measure }`          | A requested Measure's re-aggregation is not decomposable (e.g. `Avg` per §4.5's table). Author restructures as a Metric.                                                                                                                             |
| `PLAN_W_2301` | Warning  | `UnionsetBranchPrunable { unionset, child_index, reason }`       | Per §4.6: a child's contribution to the Request's selected fields is pure NullFill; branch pruned (subject to exceptions). Advisory.                                                                                                                 |
| `PLAN_W_2302` | Warning  | `UnionsetReAggregationLossy { unionset, measure, reason }`       | Per §4.5's table: the requested Measure's re-aggregation function is `Lossy` (e.g. `CountDistinct` summed across branches). Advisory; result may overcount.                                                                                          |
| `PLAN_W_2303` | Warning  | `UnionsetUniqueThreeValuedNullCollision { unionset, semantics }` | Per §4.3's `Unique` mode note: rows differing only in NullFill positions are NOT deduplicated under three-valued logic. Advisory for authors expecting full dedup.                                                                                   |


**Extensibility.** Codes `2305`–`2399` (errors) and `2304`–`2399` (warnings) reserved; MINOR per `30 §6.3`.

**Note.** `PLAN_E_2302` (mixed-grain rollup error from pre-cascade `23`) is removed under V1 strict equivalence — mixed grains across children fail at compile (`COMP_E_2301`) and never reach plan stage.

## 10. Worked Example

### 10.1 YAML

(Re-uses the §2.3 YAML — full SemanticInterface surface + per-source-distinguishing literals.)

### 10.2 Request

```
Request {
  from: paid_media,
  fields:  [ ordered_at (rollup Month), source_platform, cost ],
  filters: [ paid_only ],
}
```

### 10.3 Observable plan shape

Both branches' `source_platform` is a per-source literal (`"adwords"` in branch 0, `"facebook"` in branch 1). The disjointness predicate (§4.5) recognizes the literals are distinct → final aggregation **elided**. Per-branch sub-aggregation always emitted because `cost` is a Measure.

```
Union { mode: All }                       (final aggregation elided — disjoint literals)
├─ Project                                (branch 0 — adwords; rename + literal projection)
│  ├─ ordered_at AS ordered_at,
│  │  Literal("adwords") AS source_platform,
│  │  cost AS cost
│  └─ Agg                                 (per-branch sub-aggregation; SimpleStrategy under)
│     ├─ group_by: [DateTrunc(order_date, Month)],
│     ├─ aggregates: [Sum(total_cost) AS cost]
│     └─ Filter: paid_only
│        └─ Project (rename) + Scan       (adwords/*.parquet)
└─ Project                                (branch 1 — facebook; same shape)
   ├─ ordered_at AS ordered_at,
   │  Literal("facebook") AS source_platform,
   │  cost AS cost
   └─ Agg
      ├─ group_by: [DateTrunc(ts, Month)],
      ├─ aggregates: [Sum(spend_cents) AS cost]
      └─ Filter: paid_only
         └─ Project (rename) + Scan       (facebook/*.parquet)
```

### 10.4 Reading key

- The literals `"adwords"` / `"facebook"` projected at each branch's `Project` make `source_platform` source-distinguishing; per §4.5 the disjointness predicate elides the final aggregation. Branches' partial aggregates are already correct because no branch contributes a row at the same `(month, platform)` key as another.
- Per-branch `Agg` runs unconditionally because `cost` is a Measure (per §4.2 — pre-aggregation is the canonical path).
- `Filter: paid_only` is pushed below per-branch `Agg` because it references no aggregate; placement is `34`'s concern.
- Per-branch `Project + Scan` is the child `SimpleStrategy`'s output (per `_drafts/34_simple_strategy.md`); `UnionsetStrategy` does not reach into the child.
- `mode: All` projects to the engine's `UNION ALL`; the (absent) final aggregation would have been the Unionset-level merge layer if literals were not source-distinguishing.

### 10.5 Implicit Unionset reference

The implicit-Unionset case (multi-source `Dataset`) is structurally identical — see `[21 §10](./21_dataset.md)`. Only the children are different (per-source pseudo-leaves vs. user-authored nested DataKinds) and the discipline is strict (`21 §3.2`).

## 11. Open Items

Round-2 review (post-thirteenth-pass cascade, 2026-05-03) closed six previously-open items and re-scoped a seventh; details in `[../questions/closed/23_questions.md](../questions/closed/23_questions.md)`. Five items remain open in `[../questions/open/23_questions.md](../questions/open/23_questions.md)`:

- **Q-UNI-004** — `Avg` re-aggregation: hard error stands; promote `Sum(num) / Sum(den)` Metric pattern in docs.
- **Q-UNI-006** — Single-child collapse after pruning: skip the `PlanNode::Union`; final shape per `35`'s `PlanNode::Union { inputs: Vec<_> }` validation rules.
- **Q-UNI-012** — `PlanNode::Union` mode-flag projection: defer to `35` for the exact field shape (`mode: UnionMode` vs `distinct: bool` vs richer enum).
- **Q-UNI-013** — Nested aggregation collapse: defer to `34`'s optimizer pass; `23` does no collapsing.
- **Q-UNI-014** — Engine-specific UNION nullability handling: defer to `36`'s adapter trait.

Closed in this rebase: Q-UNI-003 (no per-child override in v1; inference-only), Q-UNI-005 (moot under V1 strict equivalence), Q-UNI-007 (moot under V1 strict equivalence), Q-UNI-008 (`{All, Unique}` v1 roster ratified; future variants tracked as `[TD-UNIONSET-FUTURE-MODES]`), Q-UNI-010 (re-scoped — `CompositionKind` retired; Unionset-level computed Semantics author per `19 §3` like Dataset), Q-UNI-011 (collapsed via Q-UNI-003).

---

**End of document.** Ratified decisions are inline throughout §§1–9; open items live in `[../questions/open/23_questions.md](../questions/open/23_questions.md)`; historical resolutions in `[../questions/closed/23_questions.md](../questions/closed/23_questions.md)`.