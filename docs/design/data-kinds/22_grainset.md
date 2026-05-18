---
prereqs: [00, 11, 13, 14, 15, 16, 17, 18, 20, 21, 23, 26, 32]
authoritative-for:
  - the `Grainset` variant's authoring surface — which fields are explicit (authored) vs. implicit (compile-derived); cross-references the type-level shape in `32 §3.3` (`Grainset` / `NestedGrainset` concrete forms), `32 §3.2` (`GrainsetBody`), and `32 §4` (`ComplexExtras`)
  - the V1 strict TemporalShape kind equivalence rule across children + the "≥ 2 unique grains" requirement (stronger than R3's "≥ 2 children")
  - the cascade-up rule for `TemporalShape.kind` only — `grain` is leaf-only and stays per-child (per SR-E-7 / SR-E-8)
  - the same-grain pre-merge mechanism — same-grain children at the same Grainset level are folded into an implicit Unionset (`mode: All`, **non-strict** NullFilling) at compile; the Grainset's effective routing units are one per distinct grain
  - the cross-grain composition mechanism — when no single effective routing unit covers the Request, the planner builds a `ComposedSemanticInterface` (per `16 §5`) and emits a LEFT OUTER JOIN tree (driver = most-covering unit; attached units equi-joined on shared Keys per `18 §2.5`)
  - the v1 inference-only per-child Coverage model — no authored per-child override; fold from each child's interface + Binding-level Coverage; default behavior is non-strict (a child not providing a Semantics is skipped for Requests touching that Semantics, or NullFilled if it survives via same-grain pre-merge)
  - plan-time observable invariants for Grainset resolution — single-unit delegation in the simple case; cross-grain LEFT OUTER JOIN tree in the partial-coverage case; rollup wrapper added when chosen unit's grain is finer than Request's grain
  - the Grainset-specific Precondition surfaces — `VALID_E_2200`–`2299` (structural), `COMP_E_2200`–`2299` (compile), `PLAN_E_2200`–`2299` (plan)
refined-by:
  - 16 (`foundations/16_composition.md` — broadened `ComposedSemanticInterface` to cover both Joinset per-hop and Grainset cross-grain composition; `CompositionKind` shrunk to `{Grainset, Joinset}`)
  - 17 (`foundations/17_temporal_shape.md` — shape × grain rollup matrix; `RollupPolicy` semantics; per-shape pin / anchor mechanics)
  - 23 (`data-kinds/23_unionset.md` — same-grain pre-merge produces an implicit Unionset shaped per `23 §4`)
  - 24 (`data-kinds/24_joinset.md` — Joinset's per-hop composition shares the broadened `ComposedSemanticInterface` shape with Grainset's cross-grain tree)
  - 25 (`data-kinds/25_applicability_matrix.md` — per-variant consumption cells for Grainset)
  - 33 (`apis/33_semstrait_manifest.md` — `ResolvedGrainset` family persistence; cross-grain JOIN-tree storage; same-grain implicit-Unionset hash-id assignment)
  - 34 (`apis/34_semstrait_planner.md` — concrete `plan` entry point and `GrainsetStrategy` algorithm body)
  - 35 (`apis/35_semstrait_ir.md` — `PlanNode::{Project, Filter, Agg, Union, Join}` variants emitted by `GrainsetStrategy`)
  - 36 (`semstrait-adapter` — engine rendering of LEFT OUTER JOIN + DATE_TRUNC + per-shape pin / anchor mechanics)
---

# 22. Grainset (`ComplexDataKind`)

> **Reconciliation (post-thirteenth-pass cascade rebase, 2026-05-03).** The v1 authoring-layer canonical shape for `Grainset` is ratified across:
>
> - [`20_taxonomy.md §2`](./20_taxonomy.md) — sealed trait hierarchy (`DataKind`, `ComplexDataKind`, `PublicDataKind`, `NestedDataKind`); `Grainset` implements `ComplexDataKind + PublicDataKind`, `NestedGrainset` implements `ComplexDataKind + NestedDataKind`.
> - [`../apis/32_semstrait_model.md §3.2`](../apis/32_semstrait_model.md) — `GrainsetBody { base: DataKindBase<ComplexExtras>, datasets, unionsets, joinsets }`. Children are inlined `Nested*` structs (no `DataKindRef` mechanism for cross-Complex child references in v1).
> - [`../apis/32_semstrait_model.md §3.3`](../apis/32_semstrait_model.md) — `Grainset` and `NestedGrainset` concrete forms.
> - [`../apis/32_semstrait_model.md §4`](../apis/32_semstrait_model.md) — `ComplexExtras { temporal: Option<TemporalShape> }`; Grainset authors only `temporal:` in `extras` (no `catalog` / `storage` / `semantic_mapping` per R-6 / SR-5; no `grain` per SR-E-7).
> - [`../foundations/18_entities.md`](../foundations/18_entities.md) — canonical entity types consumed by `GrainsetBody`'s SemanticInterface (Dimensions / Measures / Metrics / **Keys** / Filters); SR-E-7 (no `grain` on Complex), SR-E-8 (Grainset child MUST author `extras.temporal.grain:` explicitly), Keys per `18 §2.5` (the cross-grain JOIN surface).
> - [`26_nesting_matrix.md`](./26_nesting_matrix.md) — nesting matrix; R1 (leaves don't nest), R2 (no same-variant self-nesting; type-level field absence in `GrainsetBody`), R3 (`ComplexDataKind` requires ≥ 2 children).
> - [`../foundations/16_composition.md §5`](../foundations/16_composition.md) — `ComposedSemanticInterface` (broadened in this rebase to cover both Joinset per-hop and Grainset cross-grain composition).
>
> `22` retains authority for:
>
> - The `Grainset` authoring surface — explicit (authored) vs. implicit (compile-derived) fields, with cross-references to the type-level shape in `32 §3` / `32 §4`.
> - The V1 strict TemporalShape kind equivalence rule across children (mixed shapes is a hard compile error).
> - The "≥ 2 unique grains" requirement (stronger than R3's ≥ 2 children).
> - The cascade-up rule for `kind` only — grain stays per-child (per SR-E-7 / SR-E-8).
> - The same-grain pre-merge mechanism (implicit Unionset per `23 §4`).
> - The cross-grain LEFT OUTER JOIN composition mechanism (driver = most-covering effective routing unit; attached units equi-joined on shared Keys per `18 §2.5`).
> - Per-child Coverage inference (no authored override in v1).
> - Plan-time observable invariants of Grainset resolution.
> - Variant-specific Precondition surfaces — `VALID_E_2200`–`2299`, `COMP_E_2200`–`2299`, `PLAN_E_2200`–`2299`.
>
> **Algorithm body relocated.** The `GrainsetStrategy` algorithm body (effective-routing-unit assembly, single-unit delegation, cross-grain JOIN-tree construction, rollup wrapper emission) lives in [`../apis/34_semstrait_planner.md §<GrainsetStrategy>`](../apis/34_semstrait_planner.md) (forthcoming). This rebase parked the body in [`../_drafts/34_grainset_strategy.md`](../_drafts/34_grainset_strategy.md) pending the `34` drafting pass.
>
> **Concepts retired or repositioned in this rebase.**
>
> - `RollupPolicy` (`ShapeDefault` / `PinOnly` / `PreferFinest`) — implemented internally as a planner knob, NOT exposed in V1 authoring grammar (per G-4). Default behavior follows shape rules in `17`; future fine-tuning hooks may surface in `34`.
> - The pre-cascade `MixedShapeAdvisoryChildren` warning (`PLAN_W_2202` in pre-rebase `22`) — retired. Mixed shapes are now a hard compile error per V1 strict equivalence (§5.2).
> - `DataKindRef`-based child references (`GrainsetChild.constituent: DataKindRef`) — retired. Children are inlined `Nested*` structs per `32 §3.2`.
> - YAML `binding:` / `column_mapping:` wrappers (pre-thirteenth-pass spelling) — dissolved into `extras.storage:` / `extras.semantic_mapping:` per `32 §4`.
> - `CompositionKind::Grainset` discriminator — survives in `16 §5.3`'s shrunken `CompositionKind { Grainset, Joinset }` enum, but with broadened semantics (Grainset's `ComposedSemanticInterface` carries the cross-grain JOIN-tree shape, distinct from Joinset's relationship-walk shape).

## 1. Purpose and Scope

### 1.1 What `22` ratifies

`22` is the per-variant chapter for `Grainset` (one of three `ComplexDataKind` variants in v1, alongside `Unionset` (`23`) and `Joinset` (`24`)). It owns five things: (a) the **authoring surface** — explicit vs. implicit fields the author / consumer sees on the type — with cross-references to the type-level shape in `32 §3` (forms + body) and `32 §4` (`ComplexExtras`); (b) the **temporal-shape rules** specific to Grainset — kind equivalence across children, ≥ 2 unique grains, cascade-up of kind only; (c) the **same-grain pre-merge mechanism** — implicit Unionset assembly at compile per `23 §4`; (d) the **cross-grain composition mechanism** — when no single effective routing unit covers a Request, the planner emits a LEFT OUTER JOIN tree on shared Keys per `18 §2.5`, mediated by a `ComposedSemanticInterface` per `16 §5`; (e) **Grainset-specific Preconditions** in the per-variant code bands `VALID_E_2200`–`2299`, `COMP_E_2200`–`2299`, `PLAN_E_2200`–`2299`.

### 1.2 What `22` does NOT ratify

The sealed trait hierarchy and shared `DataKind` invariants live in `20`. The Rust struct / YAML shape lives in `32 §3` / `32 §4`. Child-DataKind structural rules (R1 / R2 / R3, addressing scheme) live in `26`. SR-E-7 / SR-E-8 (grain-on-complex forbidden; grainset-child-grain-required) live in `18 §3` / `18 §11`. Per-child Binding mechanics live in `15`. The `ComposedSemanticInterface` Rust shape, `UnifiedSemantics` merge, `FieldProvenance` roster, and `CompositionCoverage` fold all live in `16 §5`–`§8`. The same-grain pre-merge implicit Unionset's algorithm body lives in `23 §4` + `_drafts/34_unionset_strategy.md`. The `GrainsetStrategy` algorithm body lives in `34 §<GrainsetStrategy>` (forthcoming; parked in `_drafts/34_grainset_strategy.md`). `TemporalShape` enum variants, SCD subtype catalog, shape × grain rollup matrix, `RollupPolicy` semantics, per-shape pin / anchor mechanics live in `17`. `PlanNode` shapes live in `35`. Engine rendering lives in `36`. Peer-vocabulary references (Cube.js `preAggregations` with `granularity`, dbt MetricFlow `time_spine`, OLAP aggregate-awareness / Kimball) live in `00_overview.md` only.

### 1.3 Guardrails — how `22` upholds `00 §9` invariants

| Invariant | Where `22` keeps it |
|---|---|
| **I1** — no raw SQL in canonical layer | The emitted plan tree carries `PlanNode::{Project, Filter, Agg, Union, Join}` with `PhysicalExpr` payloads; no SQL-shaped strings. Adapter rendering (`36`) is the only SQL site. |
| **I2** — physical types via adapters only | Cross-grain JOIN type-reconciliation uses canonical `DataType` per `13 §2`; no engine types. |
| **I3** — no engine branching | Zero engine-identity checks in `22`. Every decision reads either resolved manifest indices or per-unit Coverage (compile-derived). |
| **I4** — SemanticManifest determinism | Effective-routing-unit ordering follows YAML author-declared order across the `datasets:` / `unionsets:` / `joinsets:` arrays (per `32 §6` ordering table). Same-grain pre-merge produces implicit Unionsets in declaration order per `23 §3.1`. Driver selection is most-covering with declaration-order tie-break; attached ordering follows declaration order (G-2c). |
| **I5** — compile-time resolution | Per-unit Coverage is folded once at compile (§3.2); cross-grain JOIN-tree shape is built at compile from Keys + Coverage; per-Request routing is O(\|units\| × \|Semantics\|) lookups. |
| **I6** — synchronous hot path | No I/O at any stage of Grainset resolution. |
| **I8** — planner-complete SemanticManifest | `ResolvedGrainset` carries the effective-routing-unit list, per-unit Coverage projection, the per-pair JOIN-key index, and the `ComposedSemanticInterface`. |
| **I10** — non-exhaustive public sum types | `Grainset` and `NestedGrainset` are `#[non_exhaustive]` per `32 §3.3`. The internal `RollupPolicy` (per G-4) is `#[non_exhaustive]` even though not authored. |
| **I12** — first-class Diagnostics | Every error code in §§7–9 is stable and carries a `Diagnostic.location`. |

## 2. Authoring-Time Surface

### 2.1 Explicit vs. implicit fields

The `Grainset` author touches the following surface. Type-level shape lives in `32 §3.3` (`Grainset` / `NestedGrainset` concrete forms), `32 §3.2` (`GrainsetBody`), and `32 §4` (`ComplexExtras`); `22` enumerates obligations and cross-references foundations for substance.

| # | Axis | Explicit (authored) | Implicit (compile-derived) | Cite |
|---|---|---|---|---|
| A | Identity | `body.base.name` (Public form: globally unique across top-level `data_kinds:` per SR-3; Nested form: scoped to parent's nested-kind scope per `26 §4`). | `data_kind_id` (compile-assigned per `33 §<DataKindId>`); same-grain implicit Unionsets created during pre-merge get content-derived hash-ids per `33 §<implicit-unionset-id>` (forthcoming). | `32 §3.1`, `32 §3.3`, `26 §4`, `23 §3.3` |
| B | Description | `description: Option<String>` on `Grainset` (Public form only; absent on `NestedGrainset` per `26 §3` "Nested-form structural-only" rule). | None. | `32 §3.3` |
| C | Semantic interface | `semantic_interface: SemanticInterface` on `Grainset` (Public form only; absent on `NestedGrainset`). Authors Dimensions, Measures, Metrics, **Keys** (the cross-grain JOIN surface — see §3.4), Filters per `18 §1`–`§2`. Components flatten in YAML per `#[serde(flatten)]`. | Per-Semantics inferred-type fields; typed-leaf substitution for `- ref: <name>` entries; Metric expansion; Computed-Dimension expression resolution per `19 §3`. | `32 §3.3`, `18 §1`–`§2`, `19 §3` |
| D | AI context | `ai_context: Option<AiContext>` on `Grainset` (Public form only). | None. | `32 §3.3`, `18 §6` |
| E | Children | `body.{datasets, grainsets, joinsets}: Vec<Nested*>` — children are **inlined** `NestedDataset` / `NestedUnionset` / `NestedJoinset` structs per `26`'s nesting matrix (Grainset admits all three Complex variants except itself per R2). Each child MUST author `extras.temporal.grain:` explicitly per SR-E-8. | Per-child Coverage inference (§3.2); same-grain pre-merge into implicit Unionsets (§3.3); cross-grain JOIN-tree shape via `ComposedSemanticInterface` (§3.4). | `32 §3.2`, `26 §1`, `26 §2`, `18 §3.4` (SR-E-8) |
| F | TemporalShape | `extras.temporal: Option<TemporalShape>` on `ComplexExtras` — carries only `kind` (incl. SCD subtype). Per the V1 strict equivalence rule (§5.2), the Grainset's effective `kind` is the cascade-up of children's kinds (which must all be equivalent). `grain` is type-level forbidden on `ComplexExtras` per SR-E-7. | Cascade-up from children's `extras.temporal.kind`. The Grainset's grain axis is the **set** of children's distinct grain values (≥ 2 per §5.2). | `32 §4`, `17`, `18 §3.3` (SR-E-7), `26 §3.1` |

**Public vs. Nested form distinction.** Rows B, C, D apply only to the Public form (`Grainset`). The Nested form (`NestedGrainset`) carries only `body` (which contains `base.name`, `extras: ComplexExtras`, and the nested child arrays); fields B/C/D are structurally absent per `26 §3`. The `name` at nested scope is the structural anchor in the parent's nested-kind scope (`26 §4` addressing).

**Internal-only fields (NOT authored).** `RollupPolicy { ShapeDefault, PinOnly, PreferFinest }` is implemented internally as a planner knob (per G-4 ratification, 2026-05-03). YAML carries no `rollup_policy:` / `rollup_override:` field. The default policy follows shape rules in `17`; future fine-tuning hooks may surface in `34`.

### 2.2 Nesting interaction

A `Grainset`:

- **MAY** appear at Root scope as a top-level public DataKind (per `32 §2.1`'s `grainsets:` plural tag).
- **MAY** be nested inline as a `NestedGrainset` under a `Unionset` or `Joinset` (per `26 §1`'s matrix). Nested form is structural-only (no `description` / `semantic_interface` / `ai_context`); the nested `name` is the structural anchor.
- **MUST NOT** be nested under another `Grainset` (R2; type-level — no `grainsets:` field on `GrainsetBody` per `26 §2.2`).
- **MUST** carry **≥ 2 children** across `datasets:` / `unionsets:` / `joinsets:` combined (R3 per `26 §2.3`; structural-rule diagnostic owned by `26` / `32`).
- **MUST** carry **≥ 2 unique `extras.temporal.grain:` values** across the child set (V1 strict; §5.2). Cardinality check is at validate stage; same-grain duplicates are legal individually (they pre-merge per §3.3) but the distinct-grain count must be ≥ 2.

### 2.3 Authoring example — full SemanticInterface surface

A worked YAML showing every authoring slot a `Grainset` exposes (focus is **completeness of the surface**, not plan emission — see §10 for the plan-shape worked example):

```yaml
grainsets:
  - name: paid_media_rollups
    description: "Paid-media spend across grains; cross-grain composition supported."
    dimensions:
      - name: report_date
        data_type: Timestamp
      - name: campaign_id
        data_type: String
    measures:
      - name: cost
        agg: sum
        expr: cost_cents
        data_type: Long
      - name: clicks
        agg: sum
        expr: click_count
        data_type: Long
    metrics:
      - name: cpc
        expr: cost / clicks
        data_type: Decimal
    keys:
      - name: campaign_id
        kind: foreign
    filters:
      - name: paid_only
        expr: cost > 0
    extras:
      temporal:
        snapshot:
          snapshotted_at: report_date
        # NOTE: no `grain:` here — SR-E-7 forbids grain at any Complex level;
        #       grain lives on each child's LeafExtras only (SR-E-8).
    datasets:
      - name: paid_media_daily
        extras:
          catalog: marketing_warehouse
          storage: { format: Parquet, paths: ["s3://b/daily/*.parquet"] }
          semantic_mapping:
            report_date: snapshot_date
            campaign_id: campaign_pk
            cost_cents: daily_cost
            click_count: daily_clicks
          temporal:
            snapshot:
              snapshotted_at: snapshot_date
              grain: Day
      - name: paid_media_monthly
        extras:
          catalog: marketing_warehouse
          storage: { format: Parquet, paths: ["s3://b/monthly/*.parquet"] }
          semantic_mapping:
            report_date: month_start
            campaign_id: campaign_pk
            cost_cents: monthly_cost
            # note: no `click_count` mapping — child does not provide `clicks`
          temporal:
            snapshot:
              snapshotted_at: month_start
              grain: Month
```

Notes on this example:

- `dimensions:` / `measures:` / `metrics:` / `keys:` / `filters:` are **flat** at the Grainset level per `#[serde(flatten)]` on the SemanticInterface (per `32 §3.3`).
- `keys:` declares `campaign_id` as a foreign Key — this is the cross-grain JOIN surface. When a Request needs `clicks` (only on the daily child) AND comes at grain `Month` (only directly served by the monthly child), the planner builds a LEFT OUTER JOIN tree using `campaign_id` as the equi-join key. See §3.4 + §10.
- `extras.temporal:` on the Grainset carries only `kind` (here `snapshot:`); `grain:` is type-level forbidden on `ComplexExtras` (SR-E-7).
- Each child authors its own `extras.temporal.grain:` explicitly (SR-E-8) — `Day` for daily, `Month` for monthly. Two distinct grains; satisfies the "≥ 2 unique grains" rule (§5.2).
- All children share `kind: snapshot` (V1 strict equivalence per §5.2). A child with `kind: events` would be a hard compile error.
- `paid_media_monthly` does NOT map `click_count` — its Coverage of `clicks` is `NullFill`. Routing for Requests touching `clicks` skips the monthly child unless cross-grain JOIN composition kicks in (§3.4).

## 3. Per-Child Contract

### 3.1 Children list shape and ordering

The `GrainsetBody` carries three child arrays — `datasets: Vec<NestedDataset>`, `unionsets: Vec<NestedUnionset>`, `joinsets: Vec<NestedJoinset>` (per `32 §3.2`). The **canonical child sequence** for plan emission is:

1. all `datasets:` entries in YAML author order, then
2. all `unionsets:` entries in YAML author order, then
3. all `joinsets:` entries in YAML author order.

This order is stable (per `32 §6` ordering table) and is the order:

- Same-grain children are merged (declaration order within the same grain determines their order inside the implicit Unionset's `inputs` per `23 §3.1`).
- Driver selection's tie-break runs (most-covering first; declaration-order tie-break per G-2b).
- Attached units are added in cross-grain JOIN-tree construction (declaration-order per G-2c — deterministic).

R2 (no same-variant self-nesting) is type-level absence per `32 §3.2` — a `grainsets:` field does not exist on `GrainsetBody`. R3 (≥ 2 children) is the validate-stage rule per `26 §2.3`. Nested-child name uniqueness within a single Grainset's combined child set is the Grainset author's responsibility; collisions surface via the `26 §4` addressing scheme as ambiguous addresses.

### 3.2 Per-child Coverage — v1 inference-only

V1 carries **no authored per-child Coverage override** (parallels `23 §3.2`'s ratification). Per-child Coverage is inferred at compile from each child's interface plus the child's Binding-level Coverage (Simple children) or the child's resolved interface (Complex children).

**Per-Semantics inference rule.** For each surface Semantics `s` on the Grainset's own `SemanticInterface`, and for each child `i`:

- If child `i`'s interface declares a Semantics with the same name `s` (or a `- ref: s` to a root-pool entry shared by the Grainset) AND the child can produce a value (per `15 §10` for Simple, per the child's own resolution for Complex), then child `i` **provides** `s`.
- Otherwise, child `i` **does not provide** `s` — its Coverage entry is `NullFill`.

**Coverage-completeness check.** Unlike Unionset, a Grainset surface Semantics that NO child provides is NOT auto-rejected: it could still be unreachable at runtime if no Request touches it. But because such a Semantics is unanswerable, it is a structural mistake — `COMP_E_2202 GrainsetCoverageIncomplete` fires at compile (§8). Authors fix by adding the Semantics to ≥ 1 child or removing it from the Grainset's surface.

### 3.3 Same-grain pre-merge — implicit Unionset (`mode: All`, non-strict)

When two or more children share the same `extras.temporal.grain:` value, they are folded at compile into an **implicit Unionset** (`mode: All`, **non-strict** Coverage discipline — i.e. NullFilling per `23 §3.2` rather than the Dataset-multi-source strict discipline of `21 §3.2`). The implicit Unionset becomes one **effective routing unit** at that grain (§4.1). Per G-1 (2026-05-03 ratification).

**Identification.** Same-grain implicit Unionsets get content-derived hash-ids per `33 §<implicit-unionset-id>` (forthcoming) — same mechanism as `21 §3.2`'s multi-source-Dataset implicit Unionsets. They carry no `name:` (authors don't see them).

**Discipline contrast with Dataset multi-source.** Dataset's multi-source implicit Unionset is **strict** (`21 §3.2` `COMP_E_2106` on missing coverage); Grainset's same-grain implicit Unionset is **non-strict** (NullFill at the seam per `23 §4.3`). Reason: same-grain Grainset children are typically authored as variants of the same data slab with intentional roster differences (e.g. two daily snapshots from different business units, each carrying its own subset of Measures), whereas Dataset multi-source represents the same data sliced by physical layout (paths / partitions) where roster heterogeneity signals an authoring mistake.

### 3.4 Cross-grain composition — `ComposedSemanticInterface` + LEFT OUTER JOIN tree

When no single effective routing unit (per §4.1) covers all of a Request's Semantics at the eligible grain, the planner builds a **cross-grain LEFT OUTER JOIN tree**, mediated by a `ComposedSemanticInterface` (per `16 §5`) constructed at compile. Per G-2 (2026-05-03 ratification).

**Build-time (compile)**:

1. Build per-routing-unit Coverage projection of the Grainset's surface (per §3.2; effective routing units after same-grain pre-merge per §3.3).
2. For each pair of effective routing units, identify **shared Keys** — Keys (per `18 §2.5`) declared on both units' interfaces. The Keys must agree on `data_type` (under `13 §7`'s widening rules).
3. Construct the `ComposedSemanticInterface`:
   - `composition_kind = CompositionKind::Grainset` (per `16 §5.3`).
   - `constituents: Vec<DataKindRef>` — the effective routing units (one per distinct grain).
   - `interface: UnifiedSemantics` — the Grainset's own `SemanticInterface` lifted into the composition shape per `16 §6`.
   - `provenance: FieldProvenance` — per-Semantics ownership across units per `16 §7`.
   - `coverage: CompositionCoverage` — per-(unit, Semantics) Coverage entries per `16 §8`.
   - `traversed_paths: Vec<RelationshipPath>` — empty (Grainset cross-grain composition uses Keys, not Relationships; cross-grain JOIN-tree details live in `ResolvedGrainset` per `33`, not on `ComposedSemanticInterface`).
4. Pre-build the cross-grain JOIN-key index: for each pair of units that share Keys, record the equi-join condition.

**Plan-time (per Request)** — see §4.3.

**Hard failure modes (compile)**:

- **No shared Keys between two units the planner needs to join** — `COMP_E_2204 GrainsetCrossGrainKeysAbsent { driver, attached, missing_via }` (§8). Per G-2d ratification: hard compile error rather than runtime fallback. Author fixes by adding the missing Key declaration to one or both units' interfaces.
- **Key `data_type` disagreement between units** — `COMP_E_2205 GrainsetCrossGrainKeyTypeMismatch` (§8).

## 4. Plan-Time Observable Behavior

A Grainset's plan-time observable behavior is the realization of `GrainsetStrategy::resolve` (algorithm body in `34 §<GrainsetStrategy>`, parked in `_drafts/34_grainset_strategy.md` pending `34` drafting). This section ratifies the observable invariants an author or consumer can rely on; the algorithm body is referenced only for derivation.

### 4.1 Effective routing units

An **effective routing unit** is the planner's routing primitive — one per distinct child `extras.temporal.grain:` value:

- **Single-child case** — exactly one child sits at that grain → the unit is that child (NestedDataset / NestedUnionset / NestedJoinset, resolved per its own Strategy).
- **Multi-child case** — two or more children share that grain → the unit is the implicit Unionset folded at compile per §3.3. The unit's coverage is the per-Semantics fold over the Unionset members (any one provides → unit provides; non-providers NullFill at the seam per `23 §4.3`).

The Grainset's effective routing units are **one per distinct grain** value across the child set (V1 ≥ 2 per §5.2). Routing decisions (eligibility, driver selection, JOIN-tree construction) operate at the routing-unit granularity, not at the raw-child granularity.

### 4.2 Single-unit delegation (simple case)

When a Request's Semantics set is fully covered by exactly one effective routing unit (after grain-eligibility filtering), `GrainsetStrategy` delegates to that unit's own Strategy and wraps the result in a rollup wrapper if needed:

1. Identify grain-eligible units (units whose `grain ≤ request.grain` per `13 §3.2`'s coarseness order).
2. Filter to units that fully cover the Request's Semantics set (Coverage `Native` / `Derived` per §3.2).
3. If exactly one such unit (or multiple with the most-covering equal — declaration-order tie-break per G-2b) → that's the chosen unit.
4. Run the chosen unit's Strategy on a unit-narrowed Request.
5. Wrap with `Project (DATE_TRUNC) + Agg` if `chosen_unit.grain` is finer than `request.grain` AND the rollup is shape-legal per `17 §4`.

The "splice" pattern from pre-cascade `22 §10.5` is preserved: there is no `PlanNode::Grainset`. The chosen unit's subplan is the Grainset's contribution; the rollup wrapper (when needed) is built from shared `PlanNode::{Project, Agg}` nodes.

### 4.3 Cross-grain JOIN delegation (partial-coverage case)

When no single grain-eligible unit fully covers the Request's Semantics set, the planner builds a LEFT OUTER JOIN tree using the `ComposedSemanticInterface` (§3.4). Per G-2a / G-2b / G-2c (2026-05-03 ratification).

**Algorithm**:

1. Filter to grain-eligible units.
2. Identify the **driver** = unit covering the largest subset of the Request's Semantics (Native / Derived). Tie-break: declaration order (G-2b).
3. Greedily add **attached** units in declaration order (G-2c — deterministic over greedy-by-coverage-delta). Each attached unit must:
   - Cover ≥ 1 Semantics not yet covered by driver-or-already-attached units.
   - Share at least one Key (per `18 §2.5`) with the driver (or with an already-attached unit reachable through the JOIN tree).
4. After all attached units are added, every Request Semantics must be covered. If any remains uncovered → `PLAN_E_2202 GrainsetSemanticsNotCoverableByJoin` (§9).
5. Emit the JOIN tree:
   - Driver as the LEFT side.
   - Each attached unit added via `PlanNode::Join { join_type: LeftOuter }` (per G-2a) on the equi-join condition over shared Keys.
6. Above the JOIN tree, emit the Request's `Project + Agg + Filter` per the standard observable pipeline.

**Observable plan shape** (worked example in §10.4):

```
PlanNode::Project       (final shape — Request fields)
  PlanNode::Agg         (Request's group_by + aggregates; conditional per Measures)
    PlanNode::Filter    (Request filters above the join)
      PlanNode::Join { LeftOuter, on: campaign_id = campaign_id }
        <driver unit's subplan, rolled up to Request grain if needed>
        <attached unit 1's subplan, rolled up to Request grain if needed>
      [+ additional joins for further attached units]
```

Each unit's subplan is produced by its own Strategy (the same Strategy invoked in the single-unit case). For implicit-Unionset units (§4.1 multi-child case), that's `UnionsetStrategy` per `_drafts/34_unionset_strategy.md`.

### 4.4 Rollup mechanics — shape-gated per `17`; `RollupPolicy` is internal-only

When a Request's grain is coarser than a routing unit's grain, the chosen unit's subplan is wrapped with a rollup transformation:

- **`Timeseries` / `Events` shapes** — bucket-then-aggregate via `Project (DATE_TRUNC(axis, request_grain))` + `Agg`. Always legal.
- **`Snapshot` shape** — pin policy required (per `17`); without a pin policy, rolling a snapshot up across periods is semantically unsafe. Internal `RollupPolicy::PinOnly` is the planner's default for `Snapshot` units when no pin policy is declared.
- **`SCD` shape** — as-of anchoring required (per `17`); the Request must carry an as-of timestamp. Without one, rollup fails.

The `RollupPolicy { ShapeDefault, PinOnly, PreferFinest }` enum is implemented in the planner but NOT authored in V1 YAML (per G-4). Default behavior:

- `Snapshot` units → `PinOnly` (no rollup unless pin policy declared per `17`).
- `Timeseries` / `Events` units → `ShapeDefault` (free rollup via DATE_TRUNC + Agg).
- `SCD` units → `PinOnly` semantically (pin to as-of anchor).

Future fine-tuning hooks (e.g. exposing `PreferFinest` for data-quality workflows where the finer source is preferred even at higher cost) may surface in `34`.

### 4.5 Coverage-driven pruning

Routing units that NullFill every Semantics named in the Request contribute nothing and are pruned before JOIN-tree construction. Pruning collapses the candidate set for both single-unit delegation (§4.2) and cross-grain composition (§4.3); no advisory is emitted (the prune is transparent because Grainset's routing model is "pick covering units", not "merge all units").

## 5. TemporalShape Interaction

### 5.1 Authoring placement and cascade-up of `kind` only

A Grainset's effective `TemporalShape.kind` is **inherited from its children** under the V1 strict equivalence rule (§5.2). Authoring at the Grainset level is permitted via `extras.temporal:` (`ComplexExtras`) and must agree with children's; silent authoring at the Grainset level reads the (unanimous) children's kind as the effective value.

Per SR-E-7 (`18 §3.3`) and the `32 §4` cascade table, only `temporal.<variant>:` (the shape kind) may appear on `ComplexExtras`. **`grain` is type-level forbidden** at the Grainset level — it lives on each child's `LeafExtras` only (per `32 §4`'s cascade table) and per SR-E-8 (`18 §3.4`) MUST be authored explicitly on every Grainset child. Ambient grain inheritance from the Grainset to children is structurally impossible (the `ComplexExtras` shape has no `grain` field).

### 5.2 V1 strict equivalence rule + ≥ 2 unique grains

In V1, all children of a Grainset MUST have:

1. **Equal `kind`** — `Timeseries`, `Events`, `Snapshot`, or `Scd` — must match across all children. Mismatch is `COMP_E_2201 GrainsetChildShapeKindMismatch` (§8). Single consolidated code per the parallel C1 ratification used for Unionset.
2. **Equal `ScdType`** when `kind = Scd` — `Scd(Type1) + Scd(Type2)` is rejected; subtype must match.
3. **Children's grain values must include ≥ 2 unique values** — stronger than R3's `≥ 2 children`. A Grainset with `[Day, Day]` (2 children, 1 unique grain) is degenerate (it's just an implicit Unionset at Day with no distinct routing). Validated via `VALID_E_2202 GrainsetInsufficientUniqueGrains` (§7).

**Future direction.** Smart alignment of non-equivalent shapes (e.g. `Events + Snapshot` mixing) is post-V1 and tracked as `[TD-GRAINSET-SHAPE-MIX]`. The pre-cascade `MixedShapeAdvisoryChildren` warning is retired.

### 5.3 Scope boundary with `17`

`22` ratifies only:

- the `extras.temporal:` field carriage on `ComplexExtras` (cross-ref `32 §4`),
- the V1 strict equivalence rule across children (§5.2),
- the cascade-up direction (children → Grainset, `kind` only).

`22` does NOT ratify the `TemporalShape` enum variants, the SCD subtype catalog, the shape × grain rollup matrix, the per-shape pin / anchor mechanics, or the `RollupPolicy` enum's default-derivation logic. All of these live in `17`.

## 6. Grain Interaction

### 6.1 Per-child grain authoring (mandatory per SR-E-8)

`grain` lives on each leaf descendant's `LeafExtras` (per `32 §4`); Grainset itself does not author `grain` (SR-E-7). Per SR-E-8 (`18 §3.4`), every Grainset child MUST author its own `extras.temporal.grain:` explicitly — there is no inheritance-from-ancestor mechanism. Validated post-parse at validate stage; diagnostic `validate.grainset-child-grain-required` per `26 §6`.

The Grainset's "grain axis" is the **set** of distinct grain values across children (≥ 2 per §5.2). It is not an authored field on the Grainset; it is derived from children's `extras.temporal.grain:` declarations.

### 6.2 Plan-time grain consultation

Two observable invariants involve `grain`:

- **Grain eligibility** (§4.1). For a Request at grain `G`, an effective routing unit is grain-eligible iff its grain `≤ G` per `13 §3.2`'s coarseness order.
- **Rollup wrapping** (§4.4). When a chosen unit's grain is finer than the Request's grain, the planner emits a rollup wrapper (`Project (DATE_TRUNC) + Agg`) shape-gated per `17 §4`.

If no routing unit has grain ≤ Request grain → `PLAN_E_2201 GrainsetNoMatchingUnitByGrain` (§9). Authors should add a coarser routing unit or restructure the Request.

### 6.3 Scope boundary with `13` and `17`

- `13 §5` ratifies the `Grain` enum and its coarseness order.
- `17` ratifies the `TemporalShape × Grain` legality matrix (which rollups are shape-legal).
- `17` ratifies the per-shape pin / anchor mechanics.

## 7. Validation Preconditions — `VALID_E_2200`–`2299`

Structural well-formedness for Grainsets is partially upstreamed to:

- **R1 / R2** (parse-stage type-level absence per `26 §2.1` / `§2.2`; diagnostics `parse.unknown-field` and `parse.illegal-self-nesting`).
- **R3** (validate-stage walk per `26 §2.3`; diagnostic `validate.complex-data-kind-insufficient-children`).
- **SR-E-7** (no `grain` on Complex; type-level absence on `ComplexExtras`).
- **SR-E-8** (Grainset-child grain required; diagnostic `validate.grainset-child-grain-required` per `26 §6`).
- **SR-3** name uniqueness, **SR-4** / **SR-5** field validity, **SR-7** unknown-field rejection — all in `32 §6`.

The Grainset-specific structural checks below run at validate stage:

| Code | Variant | Trigger |
|---|---|---|
| `VALID_E_2201` | `GrainsetDuplicateNestedChildName { grainset, name, indices }` | Two nested children of the same Grainset (across `datasets:` / `unionsets:` / `joinsets:`) declare the same `body.base.name`. |
| `VALID_E_2202` | `GrainsetInsufficientUniqueGrains { grainset, distinct_grains }` | Children's `extras.temporal.grain:` values include fewer than 2 distinct values (per §5.2 V1 strict). |

**Extensibility.** Codes `2203`–`2299` reserved; MINOR per `30 §6.3`.

## 8. Compile Preconditions — `COMP_E_2200`–`2299`

Compile-stage checks run after `validate` has passed and child resolution has produced per-child interface + Coverage data + `ComposedSemanticInterface` construction.

| Code | Variant | Trigger |
|---|---|---|
| `COMP_E_2201` | `GrainsetChildShapeKindMismatch { grainset, children: Vec<(NestedDataKindName, TemporalShape)> }` | One or more children's `TemporalShape.kind` (or SCD subtype) diverges from the others. Single consolidated code; covers kind mismatches and SCD subtype mismatches per §5.2. Fail-fast. |
| `COMP_E_2202` | `GrainsetCoverageIncomplete { grainset, semantics }` | A Semantics declared on the Grainset's own `SemanticInterface` is provided by NO child (every child's Coverage is `NullFill`). The Semantics is unanswerable at any grain — authoring mistake. Fail-fast. |
| `COMP_E_2203` | `GrainsetTemporalShapeKindMismatchWithGrainset { grainset, grainset_kind, child_kinds }` | The Grainset author wrote `extras.temporal:` at the Grainset level AND the kind disagrees with the (unanimous) children's kind. Cascade-up rule per §5.1 requires agreement. |
| `COMP_E_2204` | `GrainsetCrossGrainKeysAbsent { grainset, driver_unit, attached_unit, missing_via }` | The cross-grain JOIN-tree construction (§3.4) needs to attach `attached_unit` to the JOIN tree, but no shared Keys exist between `attached_unit` and any unit already in the tree (the driver or an earlier attached unit). Per G-2d ratification: hard compile error. |
| `COMP_E_2205` | `GrainsetCrossGrainKeyTypeMismatch { grainset, key_name, units_with_types: Vec<(NestedDataKindName, DataType)> }` | Two units both declare a Key with the same name but with `DataType`s that cannot be reconciled under `13 §7`'s widening rules. The equi-join would be type-unsafe. |
| `COMP_E_2206` | `GrainsetCrossGrainNoEquiJoinPath { grainset, request_semantics, driver_unit, missing_units }` | Compile-stage check: for at least one structurally-feasible Request (one that touches Semantics in `request_semantics`), the cross-grain JOIN tree cannot be assembled because some required attached unit has no Key reachable from the driver. (Subset of `COMP_E_2204`; surfaced for static analysis.) |

**Extensibility.** Codes `2207`–`2299` reserved; MINOR per `30 §6.3`.

## 9. Plan-Stage Rules — `PLAN_E_2200`–`2299`

Plan-stage checks run against a specific `Request` and the resolved Grainset.

| Code | Severity | Variant | Trigger |
|---|---|---|---|
| `PLAN_E_2201` | Error | `GrainsetNoMatchingUnitByGrain { grainset, request_grain, unit_grains }` | No effective routing unit has `unit.grain ≤ request_grain`. Every unit is strictly coarser than the Request. Authors should add a coarser-grain unit or restructure the Request. |
| `PLAN_E_2202` | Error | `GrainsetSemanticsNotCoverableByJoin { grainset, request_semantics, uncovered }` | After cross-grain JOIN-tree construction (§4.3), some Semantics in the Request's `select` / `group_by` / `filters` remain uncovered. Authors fix by adding a unit covering the missing Semantics or removing them from the Request. |
| `PLAN_E_2203` | Error | `GrainsetSnapshotRollupWithoutPin { grainset, unit, unit_grain, request_grain }` | A `Snapshot`-kind unit was selected for a Request requiring rollup but `17`'s pin policy did not declare a coarser-grain aggregation rule. (Internal `RollupPolicy::PinOnly` rejects.) |
| `PLAN_E_2204` | Error | `GrainsetSCDRollupWithoutAsOf { grainset, unit, unit_grain }` | An `Scd`-kind unit was selected for a Request lacking an as-of anchor. Per `17`'s SCD mechanics. |
| `PLAN_W_2201` | Warning | `GrainsetUnitPrunable { grainset, unit, reason }` | A routing unit was eliminated from candidacy (Coverage-pruned per §4.5, or grain-ineligible). Advisory; informs authors of unused units. |
| `PLAN_W_2202` | Warning | `GrainsetCrossGrainJoinFanout { grainset, attached_unit, expected_fanout }` | Cross-grain JOIN may fanout (e.g. monthly-attached joined onto daily-driver introduces a 1-to-N relationship via the equi-join Key). Advisory; authors verify the join doesn't multiplicatively inflate row counts. |

**Extensibility.** Codes `2205`–`2299` (errors) and `2203`–`2299` (warnings) reserved; MINOR per `30 §6.3`.

**Codes retired in this rebase:**

- Pre-cascade `PLAN_W_2202 MixedShapeAdvisoryChildren` — now `COMP_E_2201` (mixed shapes is a hard compile error per V1 strict equivalence).
- Pre-cascade `PLAN_E_2200 GrainsetNoMatchingChildByGrain` — renamed to `PLAN_E_2201 GrainsetNoMatchingUnitByGrain` (operates at the routing-unit level, not the raw-child level).

## 10. Worked Example

### 10.1 YAML

(Re-uses the §2.3 YAML — `paid_media_rollups` Grainset with two children at `Day` and `Month` grains, both `kind: snapshot`.)

### 10.2 Request A — single-unit case

```
Request {
  from: paid_media_rollups,
  fields:  [ report_date (rollup Month), campaign_id, cost ],
  filters: [ paid_only ],
}
```

Effective routing units: `Day` (single child `paid_media_daily`), `Month` (single child `paid_media_monthly`). Both are grain-eligible at `Month` (Day rolls up; Month direct).

Coverage: `cost` is provided by both. `Day` unit covers `cost` Natively; `Month` unit covers `cost` Natively.

Most-covering at Month: tie (both cover {report_date, campaign_id, cost}). Tie-break: declaration order → `Day` (it appears first; `paid_media_daily` is index 0).

Wait — but `Day` requires rollup; `Month` is direct. The "most-covering" tie-break breaks before the cost-of-rollup is consulted. V1 picks `Day` (declaration order). Future cost-aware extensions may flip this.

Single-unit delegation (§4.2): `Day` chosen; rollup wrapper added.

```
Project          (final shape — report_date (month), campaign_id, cost)
  Agg            (group_by: [DateTrunc(snapshot_date, Month), campaign_pk], aggregates: [Sum(daily_cost) AS cost])
    Filter: paid_only
      Project    (rename per Day unit's seam)
        Scan     (s3://b/daily/*.parquet)
```

### 10.3 Request B — cross-grain JOIN case

```
Request {
  from: paid_media_rollups,
  fields:  [ report_date (rollup Month), campaign_id, cost, clicks ],
  filters: [],
}
```

Coverage:

- `Day` unit (`paid_media_daily`): covers `cost`, `clicks`, `campaign_id`, `report_date`. (4/4 — full)
- `Month` unit (`paid_media_monthly`): covers `cost`, `campaign_id`, `report_date`. Does NOT cover `clicks`. (3/4)

Single-unit case: `Day` covers all 4 → §4.2 single-unit delegation, no JOIN needed. Plan shape similar to §10.2 but projecting `clicks` too.

To illustrate the cross-grain JOIN case (§4.3), assume `paid_media_daily` lacks `clicks` but instead has it on a hypothetical `paid_media_quarterly_clicks` unit (kind `snapshot`, grain `Quarter`, only carries `clicks` + `campaign_id`):

Coverage:

- `Day` unit (`paid_media_daily`): covers `cost`, `campaign_id`, `report_date`. (3/4)
- `Month` unit (`paid_media_monthly`): covers `cost`, `campaign_id`, `report_date`. (3/4)
- `Quarter` unit (`paid_media_quarterly_clicks`): covers `clicks`, `campaign_id`, `report_date`. (Quarterly Coverage; rolls up to Month? Month is finer than Quarter → Quarter ineligible at Month grain. Restructure: assume the Request is at grain `Quarter`.)

Reframed Request at `Quarter`:

- All three units grain-eligible (Day → Quarter, Month → Quarter, Quarter → Quarter).
- Most-covering at Quarter: `Day` and `Month` tied at 3 (both cover {cost, campaign_id, report_date}). Declaration-order tie-break → `Day` is driver.
- `clicks` uncovered by driver → need attached unit. Greedy declaration order: `Month` is next, but doesn't cover `clicks`. Skip to `Quarter`: covers `clicks` AND shares `campaign_id` Key with `Day`. Attach.
- After driver + 1 attached, all 4 Semantics covered.

Observable plan shape:

```
Project           (final shape — report_date (quarter), campaign_id, cost, clicks)
  Agg             (group_by: [DateTrunc(report_date, Quarter), campaign_id])
    Join { LeftOuter, on: campaign_id = campaign_id }
      <Day unit's subplan, rolled up to Quarter via DateTrunc + Agg>
      <Quarter unit's subplan, no rollup needed>
```

### 10.4 Reading key

- Effective routing units = one per distinct grain. Same-grain children would pre-merge into an implicit Unionset (`mode: All`, non-strict NullFill per §3.3); the Unionset would be the routing unit.
- §4.2 single-unit delegation is the common case. Cross-grain JOIN (§4.3) fires only when no single unit covers the full Request.
- LEFT OUTER JOIN preserves driver's row set. Attached units contribute additional Measures via equi-join on shared Keys (per `18 §2.5`). Per G-2a ratification.
- Driver = most-covering at the Request grain; declaration-order tie-break (G-2b). Attached added in declaration order (G-2c — deterministic).
- `RollupPolicy` is internal-only in V1 (per G-4). Default rollup behavior follows shape rules in `17`.
- Algorithm body — exact column ordering, `DateTrunc` placement, partial-aggregate staging, JOIN-key index lookup — lives in `34 §<GrainsetStrategy>` (parked in [`../_drafts/34_grainset_strategy.md`](../_drafts/34_grainset_strategy.md)).

## 11. Open Items

Round-2 review (post-thirteenth-pass cascade rebase, 2026-05-03) closed three previously-open items and re-scoped a fourth; details in [`../questions/closed/22_questions.md`](../questions/closed/22_questions.md). Items remaining open in [`../questions/open/22_questions.md`](../questions/open/22_questions.md):

- (none in V1; all Round-1 questions closed or moved to deferred per the rebase)

Closed in this rebase:

- **Q-GRN-001** (inheritance default for child grain) — moot under SR-E-8 (every Grainset child MUST author `extras.temporal.grain:` explicitly; no inheritance mechanism in V1).
- **Q-GRN-002** (cross-child partial coverage) — ratified at G-2 as cross-grain LEFT OUTER JOIN composition via `ComposedSemanticInterface` + Keys per `18 §2.5`.
- **Q-GRN-005** (mixed-shape warning vs. error) — moot under V1 strict equivalence rule (§5.2); mixed shapes are now `COMP_E_2201` hard error.

Deferred (post-V1):

- **Q-GRN-003** — Cost-function pluggability hook site (planner trait vs. adapter trait vs. separate `CostEstimator`). Tracked in [`../questions/deferred/22_questions.md`](../questions/deferred/22_questions.md).
- `[TD-GRAINSET-SHAPE-MIX]` — smart alignment of non-equivalent shapes (e.g. `Events + Snapshot`).
- `[TD-GRAINSET-COST-STATS]` — stats-backed cost function for driver / attached selection.
- `[TD-GRAINSET-NESTED]` — Grainset-of-Grainset nesting (closed via R2 in `26 §2.2`; future matrix relaxation post-V1).

---

**End of document.** Ratified decisions are inline throughout §§1–9; closed items in [`../questions/closed/22_questions.md`](../questions/closed/22_questions.md); deferred items in [`../questions/deferred/22_questions.md`](../questions/deferred/22_questions.md).
