---
prereqs: [20, 13, 14, 15, 17]
authoritative-for:
  - the `SimpleDataKind` Rust struct shape (name, interface, binding, optional `temporal_shape`, optional `grain`) and its `DataKind::Simple` discriminant
  - the one-`Binding`-per-`SimpleDataKind` rule at the `SimpleDataKind` level (the structural rule itself lives in `15 §2.1`; `21` ratifies the variant-specific consumer contract)
  - the `SimpleStrategy` 5-layer plan shape (L1 Scan, L2 Rename, L3 Expression, L4 Aggregate, L5 Project) as the canonical plan-emission shape for `DataKind::Simple`
  - per-layer emission rules, skip rules, and boundary-`Cast` placement for Simple plans
  - multi-`PhysicalSource` fan-out and re-aggregation semantics for a single `Simple` Binding whose glob expanded to N sources
  - Simple-specific Precondition surfaces: `VALID_E_2100`–`2199` (structural), `COMP_E_2100`–`2199` (compile), `PLAN_E_2100`–`2199` (plan)
  - how a `SimpleDataKind` carries `TemporalShape` and `Grain` declarations at the YAML surface and how each is consumed by the `SimpleStrategy` (cross-ref-only; full semantics in `17` and `13`)
refined-by:
  - 22 (Grainset — composes per-level `SimpleStrategy` sub-plans; grain routing lives above the per-Simple plan)
  - 23 (Unionset — composes per-branch `SimpleStrategy` sub-plans; NULL-fill emission and branch coverage)
  - 24 (Joinset — composes per-member `SimpleStrategy` sub-plans under a relationship-driven join path)
  - 25 (Applicability matrix — per-variant consumption cells citing `21`'s per-layer contract)
  - 33 (`semstrait-manifest` — `ResolvedSimpleDataKind` persistence)
  - 34 (`semstrait-planner` — concrete `plan` entry point dispatching to `SimpleStrategy`)
  - 35 (`semstrait-ir` — `PlanNode::{Scan, Project, Filter, Agg}` variants emitted by `SimpleStrategy`)
  - 36 (`semstrait-adapter` — engine rendering of the `SimpleStrategy` output)
---

# 21. SimpleDataKind (Dataset)

> **Reconciliation (Phase-3, 2026-04-17).** The v1 authoring-layer canonical shape for `Dataset` (a.k.a. `SimpleDataKind` in the taxonomy) is ratified across:
>
> - [`../apis/32_semstrait_model.md §3`](../apis/32_semstrait_model.md) — top-level YAML tag (`datasets:`), per-entry `DatasetBody` struct shape, `#[serde(flatten)]` composition.
> - [`../foundations/18_entities.md`](../foundations/18_entities.md) — canonical entity types consumed by `DatasetBody`: `SemanticInterface`, `Extras`, `TemporalShape`, `SemanticMapping`, `Keys`, `AiContext`, inline-vs-`ref` grammar for Dimensions / Measures / Metrics / filters.
> - [`26_nesting_matrix.md`](./26_nesting_matrix.md) — nesting rules (R1 / R2 / R3).
> - [`../apis/32b_catalogs_yaml.md`](../apis/32b_catalogs_yaml.md) — `CatalogRef` grammar consumed via `extras.catalog:`.
>
> This document retains authority for:
>
> - The `SimpleStrategy` 5-layer plan shape (§4) — L1 Scan → L2 Rename → L3 Expression → L4 Aggregate → L5 Project.
> - Per-layer emission / skip rules for the Dataset planner strategy.
> - Multi-`PhysicalSource` fan-out semantics when a single `Dataset`'s storage glob expands to N sources.
> - `VALID_E_21NN` / `COMP_E_21NN` / `PLAN_E_21NN` error-code allocations.
>
> Body sections that describe the Rust struct shape, YAML surface, or `Binding` / `ColumnMapping` vocabulary predate the `18` ratification (formerly `32c` before the 2026-04-17 promotion to the foundations layer); read those sections as historical context and cross-ref `32` / `18` for the v1 authoring shape. `ColumnMapping` → `SemanticMapping` / `ColumnMappingValue` → `SemanticMappingValue` rename per `18 §10`.

## 1. Purpose and Scope

### 1.1 What `21` ratifies

`21` is the per-variant specification for the `DataKind::Simple` arm. Where `20` ratifies the shared `DataKind` taxonomy, nesting invariants, and the SemanticManifest-layer `ResolvedDataKind` lifecycle (per `20 §*` — see shared invariants referenced below), `21` fills in the Simple-specific shape and plan-emission contract:

- **§2** — the Rust struct `SimpleDataKind`: its field roster, its `DataKind::Simple` discriminant, and its SemanticManifest-layer counterpart `ResolvedSimpleDataKind`.
- **§3** — the single-`Binding` rule as a `SimpleDataKind`-level consumer contract layered on top of `15 §2.1`'s mechanics. `21` does not re-specify Binding shape; it pins down the consumer rule "exactly one Binding per `SimpleDataKind`" and how glob-expanded multi-source Bindings are presented to the plan.
- **§4** — the 5-layer `SimpleStrategy` plan shape: L1 Scan, L2 Rename, L3 Expression, L4 Aggregate, L5 Project. This is the canonical Simple-plan form. Every layer's role is ratified here; per-layer skip rules are in §4.7.
- **§5** / **§6** — cross-reference-only interactions with `TemporalShape` (forward to `17`) and `Grain` (forward to `13 §5`). These sections pin the YAML surface carriage on the `SimpleDataKind` struct and enumerate which shape / grain fields affect the `SimpleStrategy` layers; the actual shape semantics live in `17`.
- **§7** / **§8** / **§9** — Simple-specific Preconditions across the three stages: `VALID_E_2100`–`2199` (structural, run by `validate`), `COMP_E_2100`–`2199` (compile-time, run by `compile`), `PLAN_E_2100`–`2199` (plan-stage, run by `plan` on a per-Request basis).
- **§10** — a minimal worked example: YAML `SimpleDataKind` + the corresponding `SimpleStrategy` plan tree in ASCII.

### 1.2 What `21` does NOT ratify

- **`Binding` shape and compile-time resolution mechanics** — `15`. `21` ratifies the consumer rule at the `DataKind::Simple` level; it does NOT restate `Binding.column_mapping`, `PhysicalSource`, or `Coverage` shapes.
- **Shared `DataKind` invariants** — `20 §*` (shared invariants; being drafted concurrently). `21` refines the Simple variant of those invariants without overriding any.
- **`TemporalShape` classification, SCD subtypes, as-of semantics, and snapshot selection** — `17`. `21` only mentions that a `SimpleDataKind` may carry a `temporal_shape:` declaration; all shape-gated rules are forward-referenced.
- **`Grain` rollup legality** — `13 §5` + `17`. `21` declares the YAML field and references the `SimpleStrategy` hook in L4 (§4.5); the legality matrix is cross-referenced, not restated.
- **Composition** — `16` and siblings `22` / `23` / `24`. A `SimpleDataKind` is composable under a `ComplexDataKind` per `12`'s nesting matrix, but the composition shape (how a Unionset picks up per-branch NULL-fill, how a Joinset threads the Simple's plan under an `Agg` rewrite) lives in those docs.
- **`PlanNode` variant shapes** — `35`. `21` names variants (`PlanNode::Scan`, `::Project`, `::Filter`, `::Agg`) to describe the emission shape; their field rosters live in `35`.
- **Engine rendering** — `36`. A `SimpleStrategy` output is engine-agnostic; adapter concerns are forward-referenced.

### 1.3 Design posture

`SimpleStrategy` is the **fast path**. Three properties drive the design:

- **Layered, not monolithic.** The 5-layer plan decomposes the "Scan → Rename → Compute → Aggregate → Project" pipeline into discrete stages the optimizer can rewrite independently (`34 §5`). Every Complex variant composes Simple-plans at exactly one of these layer boundaries (Unionset at L2 / L3 / L4; Grainset at L1; Joinset at L2 / L4 — ratified in `22`–`24`).
- **Every layer is optional except L1.** A pure "project three physical columns" query elides L3 (no computed Dimensions), L4 (no aggregation — e.g. a Dimensions-only request), and L5 (when identity with the layer above). Skip rules (§4.7) are the per-layer predicates.
- **No engine branching.** `SimpleStrategy` emits canonical `PlanNode`s (I3). Adapters decide how to render `Agg` / `Project` trees; the planner does not.

`SimpleStrategy` also owes its shape to two invariants:

- **I5 / I8** — Everything Simple needs is in the SemanticManifest (`ResolvedSimpleDataKind` + its `ResolvedBinding`). No catalog call, no schema fetch, no expression compilation at plan time.
- **I6** — Synchronous hot path. No `.await` in any layer's emission code.

### 1.4 Reference implementations

- **dbt MetricFlow.** The `semantic_model` block is the direct analog: a single source of truth, a set of measures / dimensions / identifiers, and a physical target. MetricFlow's "metrics are composed on top of semantic models" is the `22`/`23`/`24` story; `21`'s `SimpleDataKind` is the `semantic_model` layer.
- **Cube.js.** `cube { sql / sql_table }` + `dimensions` + `measures` is the peer shape. Cube's pre-aggregations are a Grainset concern (`22`); the bare `cube`-with-no-preAgg mode is `21`.
- **LookML.** `view.sql_table_name` + per-field `sql:` is a peer; LookML's PDTs (persistent derived tables) do not map cleanly here and are intentionally out of scope.

None of these override `00 §4` vocabulary: `Binding`, `SemanticMapping`, `PhysicalExpr`, `SemanticInterface`, `ResolvedDataKind`, `PlanNode` are authoritative. Peer vocabulary (`semantic_model`, `cube`, `view`) is cited only as structural precedent.

### 1.5 Guardrails — how `21` upholds `00 §9` invariants

| Invariant | Where `21` keeps it |
|---|---|
| **I1** — no raw SQL in canonical layer | `SimpleStrategy` emits `PlanNode`s carrying `PhysicalExpr` trees; no SQL-shaped strings leave the planner. Adapter rendering (`36`) is the only site where SQL is produced. |
| **I2** — physical types via adapters only | Every `PlanNode::Scan` projects columns typed by the resolved physical `Schema` (logical `DataType` per `15 §3.2` / `13 §2`). L2 CAST emission reconciles declared-vs-physical at the semantic boundary; the rendering of `CAST(... AS ...)` is still `36`'s job. |
| **I3** — no engine branching | Nothing in `SimpleStrategy` names an engine, a dialect, or an engine-specific operator. `PlanNode::Agg` carries canonical `Aggregation` variants; adapters decide emission. |
| **I4** — SemanticManifest determinism | `SimpleStrategy` is a pure function of `(ResolvedSimpleDataKind, Request)`. Multi-source fan-out order follows `15 §3.6`'s lexical `Binding.sources` index order. |
| **I5** — compile-time resolution | Every layer's input (`ResolvedColumnMapping`, `ResolvedExprTable`, per-source `Coverage`) is compile-built. Plan-time is O(1) lookups per Semantics. |
| **I6** — synchronous hot path | No I/O at any layer. |
| **I8** — planner-complete SemanticManifest | `SimpleStrategy` touches only `ResolvedSimpleDataKind` and its `ResolvedBinding`; no YAML, no catalog. |
| **I10** — non-exhaustive public sum types | `SimpleStrategy`'s strategy enum (`SimpleStrategyVariant`, if the planner ever exposes strategy selection) is `#[non_exhaustive]`. The `SimpleDataKind` struct itself is `#[non_exhaustive]` per `20 §*`. |
| **I12** — first-class Diagnostics | Every error code allocated in §§7–9 is stable and carries a `Diagnostic.location`. |

## 2. The `Simple` Variant

### 2.1 `DataKind::Simple` discriminant

`DataKind` is the top-level sum type ratified in `20 §*` (shared invariants). The Simple arm:

```rust
#[non_exhaustive]
pub enum DataKind {
    Simple(SimpleDataKind),
    Unionset(UnionsetDataKind),   // 23
    Grainset(GrainsetDataKind),   // 22
    Joinset(JoinsetDataKind),     // 24
    // non-exhaustive per I10
}
```

`SimpleDataKind` is the leaf in `12`'s nesting matrix: it may appear at `Root` scope (as a top-level `DataKind`) or nested under a `ComplexDataKind` per that matrix. It **cannot** contain another `DataKind` — it is structurally terminal.

### 2.2 Model-layer shape

```rust
#[non_exhaustive]
pub struct SimpleDataKind {
    pub name: DataKindName,
    pub interface: SemanticInterface,
    pub binding: Binding,
    pub temporal_shape: Option<TemporalShape>,
    pub grain: Option<Grain>,
}
```

Per-field semantics:

- `name: DataKindName` — the Model-unique identifier ratified in `11 §3`. At Root scope, `name` is globally unique across all top-level `DataKind`s. At nested scope, `name` is unique within its parent `ComplexDataKind`'s nested-kind scope per `11 §2.1`.
- `interface: SemanticInterface` — the Semantics surface (Dimensions, Measures, Metrics, Filters, Keys) per `11 §6`. `21` does not ratify the interface shape; `11` does.
- `binding: Binding` — exactly one `Binding` per `SimpleDataKind` per `15 §2.1` and §3 below. Field is not `Option<_>`; the Simple is ill-formed without a Binding (caught by `validate` — `VALID_E_2101`).
- `temporal_shape: Option<TemporalShape>` — optional `TemporalShape` declaration (`Timeseries` / `Events` / `Snapshot` / `SCD`, per `17`). `None` means the Simple is **shape-less** — its rows carry no author-declared historization axis. See §5.
- `grain: Option<Grain>` — optional `Grain` declaration (`13 §5`'s temporal enum or future non-temporal extensions). `None` means the Simple has no author-declared rollup level; rollup legality defaults per `17` (shape-gated). See §6.

**Field ordering is stable.** `21` pins the roster above; adding a field is MINOR per `30 §4` and follows `20`'s shared extension discipline for `DataKind` variants.

### 2.3 SemanticManifest-layer shape

```rust
#[non_exhaustive]
pub struct ResolvedSimpleDataKind {
    pub data_kind_id: DataKindId,
    pub name: DataKindName,
    pub interface: SemanticInterface,
    pub binding: ResolvedBinding,
    pub temporal_shape: Option<TemporalShape>,
    pub grain: Option<Grain>,
}
```

The SemanticManifest counterpart differs from the Model form in two ways:

- `binding: ResolvedBinding` — the compile-resolved form (`15 §7.6`), carrying `ResolvedColumnMapping` + `Vec<ResolvedPhysicalSource>` + per-source `Coverage`.
- `data_kind_id: DataKindId` — the compile-assigned identifier (per `20 §*`'s DataKind identity rules). SemanticManifest indices key on it.

`ResolvedSimpleDataKind` is structurally close to `SimpleDataKind`; the `Resolved*` prefix is retained (per `00 §4.1`'s naming convention) because the `binding` field diverges — the Model-layer form carries a `Binding`, the SemanticManifest-layer form carries a `ResolvedBinding`. Structural fidelity is not a goal (I8); the shape is planner-oriented.

Per `20 §*`'s ratified flow, the `ResolvedSimpleDataKind` is produced by the `compile` driver and spliced into the `SemanticManifest`'s `data_kinds: Vec<ResolvedDataKind>` with `ResolvedDataKind::Simple(ResolvedSimpleDataKind)` as the discriminant.

### 2.4 YAML surface sketch

A minimal Simple declaration (surface ratified in `32`; `21` reproduces the shape for orientation):

```yaml
data_kinds:
  - kind: simple
    name: orders
    interface:
      dimensions:
        - name: order_id
          data_type: String
        - name: ordered_at
          data_type: Timestamp
        - name: region
          data_type: String
      measures:
        - name: gross_amount
          agg: sum
          expr: amount_cents
          data_type: Long
    binding:
      sources:
        - path: "s3://bucket/orders/year=*/month=*/*.parquet"
      column_mapping:
        order_id: { column: order_id }
        ordered_at: { column: ordered_at }
        region: { metadata: { path: { token: 1 } } }
        amount_cents: { column: amount_cents }
    grain: Day
    temporal_shape:
      kind: Timeseries
      event_time: ordered_at
```

Fields `grain:` and `temporal_shape:` are optional at the YAML surface; their absence maps to `None` in §2.2's struct. Every other field is required.

### 2.5 Interaction with `12`'s nesting matrix

Per `12 §2`, a `SimpleDataKind`:

- **MAY** appear at Root scope (top-level `DataKind`).
- **MAY** be nested inline under a `ComplexDataKind` per `12`'s matrix (Unionset branches, Grainset levels, Joinset members).
- **MUST NOT** contain any `DataKind` child (it has no nested-kind scope).

A nested `SimpleDataKind`'s `name` is scoped to its parent Complex's nested-kind scope (`11 §2.1`); `21` does not re-ratify the scoping rule.

## 3. Binding

### 3.1 Single-Binding rule

**A `SimpleDataKind` owns exactly one `Binding`.** This is ratified at three levels:

- **Structurally** — `SimpleDataKind.binding: Binding` is a single value, not a `Vec` (§2.2).
- **At the parse layer** — `11 §5.3` and `15 §2.1` forbid multiple binding blocks on a single kind.
- **At the SemanticManifest layer** — `ResolvedSimpleDataKind.binding: ResolvedBinding` is a single value (§2.3).

The structural rule is `15 §2.1`'s; `21`'s job here is to pin the **consumer contract**: every `SimpleStrategy` invocation operates over exactly one `ResolvedBinding`. Plan composition above a Simple (Unionset branching, Grainset routing, Joinset joining) multiplies the number of Simple plans, not the number of Bindings per Simple.

### 3.2 Multi-`PhysicalSource` fan-out

A single `Binding` may resolve to **multiple** `PhysicalSource`s (per `15 §2.1` / `§3.5`): e.g. a glob over `year=*/month=*/*.parquet` expanding to N files. Every source in `Binding.sources` serves the same `SemanticInterface` — the `ColumnMapping` is one, not N; the `Coverage` table is what records per-source divergence (e.g. source 0 lacks a column → `NullFill`).

`SimpleStrategy`'s handling:

- **Single source (N=1).** The common path. The L1 scan targets the one source; no fan-out.
- **Multiple sources (N>1), uniform coverage.** Every source has `Native` Coverage for every referenced Semantics (no `NullFill`, no `Derived`-with-missing-columns). `SimpleStrategy` emits one `PlanNode::Scan` per source, unions them with `PlanNode::Union` (or inlines as a multi-path `Scan`; shape ratified in `35`), then proceeds through L2–L5 on the unified result. See §4.2 for the exact shape.
- **Multiple sources (N>1), heterogeneous coverage.** When any source has `NullFill` Coverage for a referenced Semantics, a bare `SimpleDataKind` consumer is a compile error per `15 §6.1` (`COMP_E_0310 UnusableNullFillInNonUnionContext`). To tolerate `NullFill`, the author wraps the Simple in a Unionset (`23`). `21` rejects heterogeneous coverage; it does not try to emit per-branch NULL-fill at the `SimpleStrategy` level — that is structurally Unionset territory.

**Re-aggregation after multi-source union.** When multiple `Scan`s are unioned at L1 and the Request's L4 aggregation reduces across rows from different sources, re-aggregation is always required. See §4.5 for the re-aggregation-skip rule when a metadata Dimension in `GROUP BY` carries source-distinguishing values.

### 3.3 Pointer to `15`

Everything else about the Binding — its shape, its `ColumnMapping` variants, its `PhysicalSource` expansion, its per-format schema resolution, its `Coverage` mechanics, its compile-time resolution flow — is ratified in `15`. `21` only enumerates the SimpleDataKind-specific consumer contract here. Readers working on Simple plan semantics should read `15 §§2–10` first; `21` assumes fluency in `15`'s vocabulary.

## 4. The Layered Plan Strategy (`SimpleStrategy`)

### 4.1 Overview

`SimpleStrategy` is the planner's resolution strategy for `DataKind::Simple`. Its output is a canonical `PlanNode` tree with up to five layers:

```
L5  Project          Final output shape; skipped when identity with L4.
L4  Aggregate        GROUP BY semantic Dimensions + declarative Measure
                     decomposition; skipped when Request has no aggregation.
L3  Expression       Computed Dimension / Measure evaluation (SemanticExpr
                     substituted per 14b); skipped when no computed fields.
L2  Rename           Physical -> semantic rename, literal / metadata
                     injection, boundary CAST per 14 §6.4 / 15 §9.1.
L1  Scan             Physical column scan(s) from Binding's resolved sources.
```

Every Simple plan is a `Project(Agg(Expression(Rename(Scan(...)))))`-shaped tree with optional layers elided per §4.7. The tree reads top-to-bottom as "final shape on top, physical scan at the bottom". In ASCII form:

```
PlanNode::Project         (L5, optional)
  PlanNode::Agg           (L4, optional)
    PlanNode::Project     (L3 as Project — computed columns, optional)
      PlanNode::Project   (L2 — rename / cast / literal / metadata)
        PlanNode::Scan    (L1, required; may itself be a Union of N Scans)
```

Filter placement is **in between** layers, not a numbered layer itself. A Request's filter list is decomposed into per-layer predicates by `SimpleStrategy`'s filter-pushdown sub-pass (not detailed here; ratified in `34 §5`): Semantics-referencing filters land above L2 (after rename), column-level filters sink into L1 where possible, aggregation-referencing filters (HAVING-equivalents) land above L4. `PlanNode::Filter` nodes are inserted where appropriate. `21` does not pin the exact placement; it only notes that `Filter` is a layer-agnostic node that `SimpleStrategy` interleaves.

**This shape is canonical.** Every Complex kind (`22`–`24`) composes `SimpleStrategy` sub-plans at specific layer boundaries; a Grainset's grain-routing decision is made above L4 of the chosen level's SimpleStrategy; a Unionset's branch assembly wraps each branch's SimpleStrategy at L2 or L3; a Joinset's join emission wraps each member's SimpleStrategy at L4 or L5. This composition discipline is why §4.1's shape is ratified — downstream composition relies on its regularity.

### 4.2 L1 — Scan

**Role.** Emit the minimal set of physical columns the Request's downstream layers need, from the Binding's `ResolvedPhysicalSource` list.

**Inputs.**
- `rb: &ResolvedBinding` — the Simple's resolved binding (§2.3).
- `needed_columns: Vec<ColumnName>` — the set of physical column names required downstream (see below).

**Algorithm.**
1. Compute `needed_columns` as the union of:
   - For every requested physical Dimension (`ColumnMappingValue::Column`): the mapped column name.
   - For every requested computed Dimension (`ColumnMappingValue::Computed`): `PhysicalExpr::referenced_columns` (per `14`'s compile-enriched field).
   - For every requested Measure's expression: its `PhysicalExpr::referenced_columns`, recursively through Metric expansion (Metric composition ratified in `11 §6.3`).
   - For every filter in the Request: its `PhysicalExpr::referenced_columns` (for column-level filter pushdown).
2. For each `src ∈ rb.sources`, emit a `PlanNode::Scan { source_ref: src, projected_columns: needed_columns }`.
3. If `rb.sources.len() > 1`, combine the per-source scans with `PlanNode::Union { branches: Vec<PlanNode::Scan> }`. (Ordering follows `15 §3.6`'s lexical `Binding.sources` index order; I4.)

**Output shape.** A `PlanNode` whose schema is the `needed_columns` physical-type-preserved projection of the Binding's physical surface. No semantic renaming has happened yet — columns carry their physical names.

**Metadata dimension columns.** Metadata-typed Semantics (per `15 §8`) do NOT contribute to `needed_columns` — their values are extracted from `PhysicalSource` metadata at L2 (Rename), not scanned. This is the `21`-specific rule: the L1 scan is minimal; the L2 layer is where injected columns appear.

**Literal dimension columns.** Likewise not in L1; literals are injected at L2.

**Interaction with `22` Grainset.** When a Simple is a Grainset level, the Grainset's grain-routing decision picks a single child; that child's `SimpleStrategy` runs a full L1 — no cross-level fan-out at L1. Ratified in `22`.

**Interaction with `23` Unionset.** A Unionset branch is a `SimpleDataKind` (or nested Complex). Each branch runs its own L1 independently; the Unionset's union-all happens at L2 / L3 / L4 per the branch's Coverage. Ratified in `23`.

### 4.3 L2 — Rename (Project)

**Role.** Transform the physical-named L1 output into a semantic-named row shape; inject literal and metadata values; emit boundary `Cast`s where the physical `DataType` disagrees with the declared Semantics type.

**Inputs.**
- L1 output (physical-named).
- `rb.column_mapping: ResolvedColumnMapping` — per `15 §7.2`'s four HashMaps + per-source `Coverage`.

**Algorithm.** Emit `PlanNode::Project { expressions: Vec<NamedPhysicalExpr> }` where each entry in `expressions` is produced per the `ResolvedColumnMapping` lookup (`15 §7.4`'s `resolve_semantics`):

| `ColumnMappingValue` | L2 emission |
|---|---|
| `Column { name }` | `(name_semantic, PhysicalExpr::Column(name_physical))` — a rename. |
| `Column { name }` with boundary cast (`15 §9.1`) | `(name_semantic, PhysicalExpr::Cast(Column(name_physical), declared_type))` — the `Cast` was wrapped at compile and lives in `ResolvedColumnMapping.computed` per `15 §7.2`; L2 reads from `computed` for this Semantics. |
| `Literal { value, data_type }` | `(name_semantic, PhysicalExpr::Literal(value))` — materialized as a scalar broadcast. |
| `Metadata(MetadataDimensionRecipe)` | `(name_semantic, PhysicalExpr::Literal(per_source_value))` — the **extraction is performed at compile time**, not at plan time (per `15 §5.5` / §10.5). The planner reads each source's pre-resolved `LiteralValue` from `ResolvedPhysicalSource.metadata_values[name_semantic]` (`15 §7.6`) and emits it as a scalar broadcast. In a multi-source scan (§4.2 step 3), each source's Rename project emits its own per-source literal because the `metadata_values` map's value can differ across sources (the recipe is global to the Binding; the resolved `LiteralValue` is per-source). v1 scope is path-token only (`15 §8.0`). |
| `Computed { expr }` | Deferred to L3. L2 passes through the columns `expr.referenced_columns` needs unchanged. |

**Output shape.** Every non-Computed Semantics is named and typed per the `SemanticInterface`. Computed Semantics are not yet materialized — their `referenced_columns` are carried through as physical columns.

**Per-source variation in multi-source scans.** When L1 emits a `Union` of N `Scan`s, L2's Rename project is emitted per-branch (inside the Union) so metadata values, which vary per source, are per-branch literal constants. The final tree shape:

```
Union(
  Project[rename_source_0](Scan[source_0]),
  Project[rename_source_1](Scan[source_1]),
  ...
)
```

This is the one place in `SimpleStrategy` where per-source divergence bleeds into the plan shape; below L2, the union'd rows are semantic-named and uniform.

**Narrowing-cast warning at compile.** Any `Cast` emitted at L2 was already diagnosed at compile (`COMP_W_0302 ImplicitNarrowingCast` per `15 §9.1`). L2 does not re-emit the warning.

### 4.4 L3 — Expression (Project)

**Role.** Materialize computed Dimensions and computed Measure-base expressions; `SemanticExpr` substitution has already happened at compile (`14b`), so L3 projects already-resolved `PhysicalExpr` trees.

**Inputs.**
- L2 output (semantic-named row shape, minus computed Semantics).
- `rb.column_mapping.computed: HashMap<SemanticsName, PhysicalExpr>` — per `15 §7.2`.

**Algorithm.** For every requested Semantics that maps to `ColumnMappingValue::Computed`:

1. Look up its `PhysicalExpr` in `rb.column_mapping.computed[name]` (per `15 §7.4`). The expression is `EntityRef`-free (14b has substituted every `EntityRef` during compile) and `inferred_type`-annotated.
2. Emit a `PlanNode::Project` entry `(name_semantic, expr)` alongside pass-through of every L2-produced column that downstream layers (L4 / L5 / filters) need.

The resulting `Project` has the shape `(all_pass_through_columns ++ computed_columns)`.

**Cross-reference to `14b`.** `14b §3` specifies the substitution algorithm; `21` reads the output. The `PhysicalExpr` stored in `rb.column_mapping.computed[name]` is semantically equivalent to "inline the computation as of the SemanticInterface's definition at the time of `compile`." No plan-time recomputation.

**Skip rule.** If the Request references no computed Semantics on this Simple, L3 is elided. See §4.7.

### 4.5 L4 — Aggregate

**Role.** Apply `GROUP BY` over the requested semantic Dimensions and evaluate the requested Measures and Metrics via declarative decomposition.

**Inputs.**
- L3 output (or L2 output when L3 is skipped).
- `interface: &SemanticInterface` — to look up per-Measure aggregation shape (`agg: Sum` / `Count` / `Avg` / etc. per `14 §3.2`'s `Aggregation` enum) and per-Metric composition.

**Algorithm.**

1. **GROUP BY clause.** The `GROUP BY` keys are the requested Dimensions (their semantic names). Includes both physical-mapped and computed Dimensions — both are semantic columns by L4's input. For temporal Dimensions with a Request-level grain rollup (e.g. "rollup `ordered_at` to Week"), wrap in `DateTrunc(<sem_col>, Grain::Week)` per `14 §3.2`'s `DateTrunc` variant. The legality of the rollup is shape-gated (§6; `17`).
2. **Aggregate expressions.** For each requested Measure `M`:
   - `M` declares `agg: Sum` + `expr: amount_cents` (a physical Measure over one column). L4 emits `PhysicalExpr::Aggregate { aggregation: Sum, expr: Column(amount_cents), distinct: false }` under the name `M`.
   - `M` declares `agg: Count` + `expr` omitted (Count-star-like). L4 emits `Aggregate { aggregation: Count, expr: Literal(1), distinct: false }`.
   - `M` carries a measure-level filter (`filter: expr_F` per `11 §6.2`). L4 emits `Aggregate { aggregation: Sum, expr: Case { when: [{condition: expr_F, result: Column(amount_cents)}], else_expr: None }, distinct: false }` — conditional aggregation via `Case`, equivalent to `SUM(CASE WHEN ... THEN ... END)` at the adapter level.
3. **Metric decomposition.** A requested Metric `X` is recursively expanded into its constituent Measures per `11 §6.3`'s Metric composition rule. Every Measure surfaces as its own L4 aggregate expression; the Metric's composing expression lives in L5 as a post-aggregation Project term.
4. **Re-aggregation over multi-source Scans.** If L1 emitted a Union of N sources and L4's `GROUP BY` keys include a metadata Dimension, the Request may be satisfiable with no re-aggregation — see §4.5.1 below. In the general case, L4 runs twice conceptually: once per-source (as a per-source partial aggregate, pushed down inside the Union's branches) and once as a "merge aggregate" above the Union. The per-source-partial pushdown is an optimizer decision (`34 §5`); `21` ratifies the shape but not the pushdown predicate.

**Output shape.** A `PlanNode::Agg { group_by: Vec<PhysicalExpr>, aggregates: Vec<NamedAggregate> }` whose schema is `(group_by_columns ++ aggregate_columns)`.

**Skip rule.** If the Request asks for no Measures or Metrics (a Dimensions-only Request), L4 is elided — the plan returns the L3 (or L2) Project unchanged. See §4.7.

#### 4.5.1 Re-aggregation skip when metadata is source-distinguishing

When a multi-source `Binding` is consumed at L4 and the `GROUP BY` includes a metadata Dimension whose values are **distinct per source** (e.g. `path.token: 0` extracting `year_dir = "year=2024"` on source 0 vs `"year=2025"` on source 1, per `15 §8.1`; v1 path-only scope per `15 §8.0`), the per-source partial aggregation is already "complete" — no two sources contribute rows that share a `GROUP BY` key. In that case, the re-aggregation above the Union can be skipped, and the plan shape simplifies to:

```
Union(
  Agg[partial_source_0](... L3 source_0 ...),
  Agg[partial_source_1](... L3 source_1 ...),
  ...
)
```

`SimpleStrategy` checks this predicate via a sub-pass (functional name proposed: `has_source_distinguishing_metadata`). The predicate is **conservative** — if any metadata Dimension in `GROUP BY` yields the same value across two sources, the re-aggregation is retained. Determinism (I4) is preserved: the predicate is a pure function of the `ResolvedBinding`'s per-source metadata literals.

**Lossy re-aggregation warning.** When re-aggregation is not skippable and the per-source partial aggregate uses `COUNT_DISTINCT` or `AVG`, the merge-aggregate cannot recover the exact value without the underlying rows. `SimpleStrategy` falls back to emitting the full aggregation only at the merge layer (no per-source partial). A `PLAN_W_2101 LossyMultiSourceReaggregation` advisory is emitted when this fallback fires and the Binding has multiple sources.

### 4.6 L5 — Project

**Role.** Produce the final output row shape the Request expects — typically a reordering, a Metric's post-aggregation expression, or an identity pass-through.

**Inputs.**
- L4 output (or the layer below if L4 was skipped).

**Algorithm.**
1. For every requested field (Dimension, Measure, or Metric) in the Request's output list, emit a `PhysicalExpr` under its semantic name:
   - **Dimension** — `PhysicalExpr::Column(sem_name)` (pass-through from L4's `GROUP BY`).
   - **Measure** — `PhysicalExpr::Column(sem_name)` (pass-through from L4's aggregate output).
   - **Metric** — the Metric's composing expression, built over the Measures that L4 computed. E.g. a ratio Metric `revenue_per_user = revenue / user_count` becomes `PhysicalExpr::BinaryOp { op: Divide, left: Column("revenue"), right: Column("user_count") }`.
2. Honor the Request's ordering of output fields.

**Output shape.** `PlanNode::Project { expressions: Vec<NamedPhysicalExpr> }` whose schema matches the Request's expected output row.

**Skip rule (identity elision).** If every L5 projection expression is `PhysicalExpr::Column(name)` and the `name` sequence matches the L4 output schema field-for-field (same names, same order), L5 is elided. The resulting plan ends at L4 (or wherever the prior-layer elision left it).

### 4.7 Skip rules

Consolidated per-layer skip predicates. Applied top-to-bottom after the logical plan is built; each layer's emission is conditioned on its predicate.

| Layer | Skip predicate |
|---|---|
| L5 Project | Every projection expression is `Column(name)` AND `name` sequence matches L4 output schema exactly. |
| L4 Agg | Request asks for zero Measures AND zero Metrics (pure Dimensions query). |
| L3 Expr | Request references zero computed Semantics on this Simple (zero `Computed` entries in `rb.column_mapping.computed`). |
| L2 Rename | Never skipped. Even when every Semantics maps to `Column { name }` with no `Cast` and no renaming, L2 is emitted — renaming from physical `ColumnName` to semantic `SemanticsName` is always required. |
| L1 Scan | Never skipped. |

**Interaction with multi-source fan-out.** §3.2's multi-source case does not change skip rules; each per-source branch applies the same predicates uniformly.

**Interaction with filter pushdown.** `PlanNode::Filter` nodes (interleaved between layers per §4.1) are skipped when the Request carries no filter. When they exist, they are additionally subject to their own pushdown placement — column-level filters sink to the right of L2; aggregation-referencing filters surface above L4. Ratified in `34 §5`.

## 5. Interaction with `TemporalShape`

### 5.1 YAML surface

A `SimpleDataKind` (a `Dataset` leaf) carries an optional `extras.temporal:` block per `18 §3`. The v1 shape kinds are `Timeseries`, `Events`, `Snapshot`, and `Scd` (collapsed wrapper — `extras.temporal.<variant>:` per `18 §3.2`); the v1 `ScdType` roster is **`{Type1, Type2}`** per `18 §3.3`. Each variant carries its own identifying-Dimension field (`occurred_at:` for `Timeseries` / `Events`, `snapshotted_at:` for `Snapshot`, `valid_from:` / `valid_to:` for `Scd`). `Type0` / `Type3` / `Type4` / `Type5` / `Type6` in `17`'s Kimball SCD taxonomy are **post-v1 deferred** — not in the v1 authoring surface.

Example:

```yaml
- kind: simple
  name: web_events
  interface:
    dimensions: [...]
    measures: [...]
  binding: [...]
  temporal_shape:
    kind: Events
    event_time: occurred_at
```

### 5.2 How `SimpleStrategy` consumes shape

`SimpleStrategy`'s layers consult `SimpleDataKind.temporal_shape` at three sites:

- **L4 (§4.5) — rollup legality.** When the Request rolls up a temporal Dimension, the rollup target is compared against the shape's rollup matrix (`17 §*`). `Snapshot` has a fixed source grain; rolling up `snapshotted_at` from its native grain requires explicit additivity information. `SCD` has no intrinsic grain; rolling up `valid_from` is shape-gated. The exact matrix lives in `17`; `21` only identifies the hook.
- **L4 (§4.5) — advisory warnings when `TemporalShape` and `Additivity` are inconsistent.** Per `00 §4.1`'s `TemporalShape` row and `11 §7`'s `Additivity` row, the two axes are independent but related. The planner MAY emit advisory warnings (shape ratified in `17`). `SimpleStrategy` surfaces the warning at L4 emission; the predicate lives in `17`.
- **Per-Request `temporal:` block (DEFERRED).** A Request with an `as_of:` timestamp or a temporal range must be shape-compatible: `SCD` types answer as-of queries; `Snapshot` answers point-in-time; `Timeseries`/`Events` reject `as_of` in favor of bucket-scoped queries. Planner support for these is DEFERRED per `00 §4.1` and `17`; `21` does not emit shape-aware plans yet in v1. `temporal_shape:` declarations are stored on the SemanticManifest and become consequential when `17`'s planner support lands.

### 5.3 Shape-less Simples

A `SimpleDataKind` with `temporal_shape: None` has no author-declared historization axis. Its `SimpleStrategy` runs with no shape-specific rules activated — no rollup gating, no advisory warnings, no as-of support. This is the common path for staging / flat-fact tables without a ratified temporal structure.

### 5.4 Scope boundary with `17`

`21` ratifies ONLY:
- the YAML field `temporal_shape:` carriage on the `SimpleDataKind` struct (§2.2),
- the three consumption sites in `SimpleStrategy` above (cross-references only).

`21` does NOT ratify:
- the `TemporalShape` enum variants and their per-variant fields (`17 §*`),
- the SCD subtype catalog (`17 §*`),
- the shape × grain rollup matrix (`17 §*`),
- the `AsOf` join variant (`17 §*` + `16`),
- the advisory-warning predicates (`17 §*`).

Forward-references to `17` are loose in Round 1; as `17` ratifies, `21` tightens its cross-references.

## 6. Interaction with `Grain`

### 6.1 YAML surface

A `SimpleDataKind` carries an optional `grain:` declaration. `Grain` is ratified in `13 §5` as a 7-variant temporal enum (`Second`, `Minute`, `Hour`, `Day`, `Week`, `Month`, `Year`) with a total coarseness order. Non-temporal grains (geographic, entity) are an extensibility note in `13`; `21` inherits the extensibility posture (I10).

Example:

```yaml
- kind: simple
  name: daily_snapshots
  ...
  grain: Day
```

### 6.2 How `SimpleStrategy` consumes grain

The `grain:` declaration is the **author's declared rollup level** of the Simple's rows. `SimpleStrategy`'s layers consult it at two sites:

- **L4 (§4.5) — rollup coarsening.** When the Request rolls up a temporal Dimension, the target grain must be **at least as coarse** as the Simple's declared `grain`. A `grain: Day` Simple cannot be queried at `Hour`; a Request that tries is a plan error (`PLAN_E_2102 RequestGrainFinerThanSource`). Rolling up to `Week` / `Month` / `Year` is legal; `DateTrunc` at the requested grain is emitted at L4 (§4.5 step 1).
- **L4 (§4.5) — shape-gated legality.** When `temporal_shape:` is also declared, the rollup is shape-gated per `17`'s matrix. A `Snapshot` with `grain: Day` does not roll up to `Month` without advisory warnings; an `Events` Simple with `grain: Hour` rolls up freely to `Day` / `Week` / etc. Full matrix in `17`.

### 6.3 Grain-less Simples

A `SimpleDataKind` with `grain: None` has no author-declared rollup level. `SimpleStrategy` makes two simplifying assumptions:

- Any Request-side `DateTrunc` is accepted structurally (subject to shape-gated legality per `17` if `temporal_shape:` is present).
- Advisory warnings about mixing finer and coarser sources in a Unionset / Grainset (ratified in `22` / `23`) are silent.

### 6.4 Scope boundary with `13` and `22`

- **`13 §5`** ratifies the `Grain` enum, its variants, and its total coarseness order.
- **`22`** ratifies grain routing — the Grainset variant that dispatches a Request across children with different `grain:` declarations. `21` does not ratify routing; a bare `SimpleDataKind` has ONE declared grain.
- **`17`** ratifies the `TemporalShape × Grain` legality matrix.

## 7. Validation Preconditions — `VALID_E_2100`–`2199`

`21` allocates its `VALID_E` sub-range for Simple-variant-specific structural checks. `validate` runs these against the `SemanticModel` (per `10 §3.2`). Every check accumulates (per `10 §3.3`) — `validate` collects all failures before returning.

| Code | Variant | Trigger |
|---|---|---|
| `VALID_E_2101` | `SimpleMissingBinding { data_kind }` | A `SimpleDataKind` declares no `binding:` block at all. (Authored `binding: None` is structurally impossible since the Model's YAML-surface type requires the key — this error fires when the YAML omits it entirely, caught by the Model-level parse or by post-parse structural validation.) |
| `VALID_E_2102` | `SimpleMultipleBindings { data_kind, count }` | A `SimpleDataKind`'s YAML carries multiple `binding:` blocks (duplicate key handled at the YAML level; if a future `bindings:` plural surface ever appears, this code fires). Today this is structurally unreachable; reserved. |
| `VALID_E_2103` | `SimpleNestedInSelf { data_kind }` | A `SimpleDataKind`'s `binding.column_mapping` (via a `Computed` entry) references itself cyclically through an intermediate Semantics — the cycle is detected structurally per `11 §7` / `14b §5`. Reserved here because the underlying cycle-detection emits `EXPR_E_0206` (from `14b`); `21` does not duplicate the code, but the VALID-range reservation notes the Simple-structural character of the error. |
| `VALID_E_2104` | `SimpleInterfaceEmpty { data_kind }` | A `SimpleDataKind`'s `SemanticInterface` has zero Semantics of every kind (no Dimensions, no Measures, no Metrics, no Filters, no Keys). A Simple with no queryable surface is pointless; caught at validate as advisory-plus-error. |
| `VALID_E_2105` | `SimpleGrainOnUnsupportedShape { data_kind, shape, grain }` | A `SimpleDataKind` declares both `temporal_shape:` and `grain:`, but the shape is one that has an intrinsic grain (e.g. `Snapshot` per `17`). The shape-intrinsic grain conflicts with the author-declared one. Reserved; the authoritative gating lives in `17`. |
| `VALID_E_2106` | `SimpleBindingReferencesSelf { data_kind, semantics }` | A `Binding.column_mapping` `Column { name }` uses the `SimpleDataKind`'s own semantic name as a column. Semantics-to-column naming overlaps are user errors at the structural level; `VALID_E_2106` flags the obvious case (the column name is a semantic name declared in the same kind's interface). Cross-kind overlap is not caught here. |
| `VALID_E_2107` | `SimpleTemporalShapeMissingIdentifier { data_kind, shape }` | A `temporal_shape:` declaration omits its shape-required identifying field (e.g. `Events` without `event_time:`, `Snapshot` without `snapshotted_at:`, `SCD` without `valid_from:`/`valid_to:`). The shape-specific field catalog lives in `17`; `21` reserves the code for the boundary check. |
| `VALID_E_2108` | `SimpleTemporalShapeIdentifierNotInInterface { data_kind, shape, identifier }` | A `temporal_shape:` declaration's identifying field (e.g. `event_time: occurred_at`) names a Semantics that is not present in the `SimpleDataKind`'s interface. |
| `VALID_E_2109` | `SimpleTemporalShapeIdentifierWrongType { data_kind, shape, identifier, declared_type }` | A `temporal_shape:` identifier resolves to a Semantics whose `data_type:` is not a temporal type (`Date` / `Time` / `Timestamp`). Shape-kind-specific — `Events`/`Snapshot` require `Timestamp`; `SCD` `valid_from/valid_to` requires `Date` or `Timestamp`. Detailed matrix in `17`. |

**Extensibility.** The 100-code sub-range reserves ample headroom for future Simple-structural checks. Codes `2110`–`2199` are free; adding a variant is MINOR per `30 §6.3`.

**Re-surfaced errors from `11` / `12` / `13` / `14` / `15`.** Structural issues under Simples that belong to other foundations docs (e.g. a duplicate Dimension name — `11 §3`; a mismatched `data_type:` across occurrences — `11 §5.1`; a `Binding`'s ill-formed `ColumnMapping` — `15 §5.6`) are reported by the owning doc's code ranges; `21` does not re-codify them. Diagnostics fill in the `data_kind` location to contextualize.

## 8. Compile Preconditions — `COMP_E_2100`–`2199`

`21` allocates its `COMP_E` sub-range for Simple-variant-specific compile-time checks. `compile` runs these after `validate` has passed (per `10 §3.3`). Fail-fast per `10 §3.3` — the first compile error aborts the Simple's resolution.

| Code | Variant | Trigger |
|---|---|---|
| `COMP_E_2101` | `SimpleBindingResolutionFailed { data_kind, cause }` | A wrapper re-surface: Binding-level resolution (per `15 §10`) raised any of `COMP_E_02xx` / `COMP_E_03xx`. `21` attaches the `data_kind` context but does not re-codify the inner cause. |
| `COMP_E_2102` | `SimpleSourceGlobEmpty { data_kind, pattern }` | The Simple's Binding's source glob expanded to zero sources. Semantically equivalent to `COMP_E_0301 NoSourcesMatched` per `15 §3.5.7`; `21` re-surfaces under the Simple's diagnostic context for the common "the author meant this Simple's glob, show the author which Simple" pattern. (Whether `21` re-surfaces or passes through verbatim is the Q-DS-002 open item.) |
| `COMP_E_2103` | `SimpleSchemaInferenceRequiredForBinding { data_kind, source }` | A Simple's Binding has a CSV or JSON source with no `declared_schema:` and inference would violate I4 for the Simple's declared grain / shape combination (e.g. a `grain: Hour` + `temporal_shape: Events` Simple demands column-level timestamp typing; inference yielding `String` is not recoverable into `Timestamp` without explicit authoring). Advisory upgrade to error for Simples with temporal declarations. |
| `COMP_E_2104` | `SimpleTemporalIdentifierTypeMismatch { data_kind, identifier, declared, physical }` | The identifying column for a `temporal_shape:` declaration resolves (via `ColumnMapping`) to a `PhysicalExpr` whose inferred type disagrees with the shape's required type (e.g. `event_time: occurred_at` but `occurred_at`'s physical column is `String` with no cast to `Timestamp` declared). Distinct from `COMP_E_0315 IncompatiblePhysicalType` (`15 §9.1`) because the shape context narrows the required type beyond what `15`'s pass-through cast policy enforces. |
| `COMP_E_2105` | `SimpleGrainNotSupportedBySource { data_kind, declared_grain, source }` | A Simple declares `grain: Hour` but a source's declared partition-transform (via `37`) reports coarser-than-Hour granularity. The declared grain cannot be satisfied by the physical partitioning. The authoritative legality matrix lives in `22` / `17`; this error is reserved for the Simple-level pre-routing check. |
| `COMP_E_2106` | `SimpleMultiSourceIncompatibleNullFill { data_kind, source_index, semantics }` | Wrapper re-surface of `15 §6.1`'s `COMP_E_0310 UnusableNullFillInNonUnionContext`, with the Simple's name attached for operator context. The Simple does not tolerate `NullFill` Coverage; the author's fix is to wrap in a Unionset (`23`). |
| `COMP_E_2107` | `SimpleComputedReferencesUnresolvableEntity { data_kind, semantics, unresolved }` | Wrapper re-surface of `EXPR_E_0201 EntityRefNotResolved` (from `14b`), attached to the Simple's diagnostic context. A `Computed` Dimension / Measure's `SemanticExpr` referenced an `@other` that does not exist in the Simple's `SemanticInterface` and has no `Relationship` path (`14b §4`). |

**Extensibility.** Codes `2108`–`2199` are free for future Simple-specific compile-time checks; MINOR per `30 §6.3`.

**Wrapper vs pass-through.** Several codes above (`2101`, `2102`, `2106`, `2107`) are re-surfaces of codes owned by `14` / `14a` / `14b` / `15`. The Q-DS-002 open item discusses whether the wrapper discipline should be universal (every Simple-context error gets a `21`-level code with the inner cause) or verbose-unhelpful (leave the original code visible, no wrapping). Round-1 default: wrap only when the Simple-level context materially aids debugging; pass-through the rest.

## 9. Plan-Stage Rules — `PLAN_E_2100`–`2199`

`21` allocates its `PLAN_E` sub-range for Simple-specific checks that cannot be resolved until Request-time. Most Request-level checks live in `34`; `21`'s range covers the narrow slice that specifically depends on `SimpleDataKind` shape.

| Code | Severity | Variant | Trigger |
|---|---|---|---|
| `PLAN_E_2101` | Error | `SimpleRequestReferencesUnknownSemantics { data_kind, semantics }` | A Request's fields list or filter list references a Semantics name not present in the Simple's `SemanticInterface`. (For Requests with explicit `from: <simple>`, this is a hard error; for field-first-resolved Requests per `16 §11`, the routing check is in `34` and this code does not fire.) |
| `PLAN_E_2102` | Error | `RequestGrainFinerThanSource { data_kind, requested, declared }` | A Request rolls a temporal Dimension finer than the Simple's declared `grain:`. See §6.2. |
| `PLAN_W_2101` | Warning | `LossyMultiSourceReaggregation { data_kind, measure }` | Multi-source Binding + re-aggregation-cannot-be-skipped (§4.5.1) + Measure uses `COUNT_DISTINCT` or `AVG`. Advisory; the plan still executes but is lossy. |
| `PLAN_W_2102` | Warning | `ShapeAdditivityMismatch { data_kind, measure, shape, additivity }` | A Measure's `Additivity` and the Simple's `TemporalShape` appear inconsistent per the advisory matrix (ratified in `17`). Advisory; plan proceeds. |
| `PLAN_E_2103` | Error | `SimpleNullFillAtPlan { data_kind, source_index, semantics }` | A multi-source Binding whose Coverage includes `NullFill` reached plan-time under a bare Simple consumer. This should have been caught at compile (`COMP_E_0310` / `COMP_E_2106`); if the SemanticManifest was hand-constructed or loaded from a stale artifact, the planner's defensive check fires this error. |
| `PLAN_E_2104` | Error | `SimpleEmptyPlanRequested { data_kind }` | A Request asks for zero fields (empty projection list). A Simple plan with zero output columns is ill-formed; rejected. Reserved for the structural check. |
| `PLAN_E_2105` | Error | `SimpleAggregationWithoutMeasure { data_kind, grouping }` | A Request includes `GROUP BY`-level Dimensions but no Measure / Metric. This is structurally a Dimensions-DISTINCT query; `SimpleStrategy` elides L4 per §4.7. If the Request carries aggregation explicitly (via a future API), the absence of Measures fires this error. Reserved. |

**Extensibility.** Codes `2106`–`2199` reserved; MINOR per `30 §6.3`.

**Scope note.** Request-routing errors (which DataKind owns which Semantics, field-first vs explicit-`from` resolution, composition synthesis) are `34`'s concern. `21` only ratifies the narrow slice where Simple-variant shape is the root cause.

## 10. Worked Example

### 10.1 YAML

A minimal single-source Simple:

```yaml
data_kinds:
  - kind: simple
    name: orders
    interface:
      dimensions:
        - name: ordered_at
          data_type: Timestamp
        - name: region
          data_type: String
        # Metadata Dimension — author the recipe on the Dimension type
        # (per `13 §4.7` / `18 §4`); compile synthesizes the corresponding
        # `SemanticMappingValue::Metadata(...)` entry — no `semantic_mapping:`
        # entry for this Dimension. v1 path-only per `15 §8.0`.
        - name: year_dir
          data_type: String
          type:
            metadata:
              source:
                path:
                  token: 0       # 0-indexed, scheme-stripped — `15 §8.1`
      measures:
        - name: gross_revenue
          agg: sum
          expr: amount_cents
          data_type: Long
    binding:
      sources:
        - path: "s3://b/orders/year=*/*.parquet"
      semantic_mapping:
        ordered_at:   ordered_at        # Variant 1 — bare column
        region:       region
        amount_cents: amount_cents
        # `year_dir` has NO entry here — its recipe lives on the Dimension
        # type above and is compile-synthesized into the SemanticMapping
        # before the §5.6 completeness check (`15 §10.4` step 4.0).
    grain: Day
    temporal_shape:
      kind: Events
      event_time: ordered_at
```

### 10.2 Request

```
Request {
  from: orders,
  fields: [ ordered_at (rollup Month), region, gross_revenue ],
  filters: [ region = 'EU' ],
  order_by: [],
  limit: None,
}
```

### 10.3 Pre-plan resolution

- `Binding.sources` expands the glob. Assume lexical order: `year=2024/data.parquet`, `year=2025/data.parquet`. `sources.len() == 2`.
- Each source's schema: `{ordered_at: Timestamp, region: String, amount_cents: Long}`. Uniform; no `NullFill`.
- `ResolvedColumnMapping`:
  - `columns: { ordered_at -> ordered_at, region -> region, amount_cents -> amount_cents }`
  - `literals: {}`
  - `computed: {}`
  - `metadata: { year_dir -> MetadataDimensionRecipe { extraction: Path { token: 0 }, data_type: String } }` — v1 path-only scope per `15 §8.0`; partition extraction deferred to v2.
- `year_dir`'s per-source resolved metadata literal (eagerly evaluated at compile per `15 §10.5`, stored on `ResolvedPhysicalSource.metadata_values` per `15 §7.6`): `"year=2024"` on source 0, `"year=2025"` on source 1. The raw segment is what `path_token` returns (`15 §8.1.1`); a downstream `Computed` Dimension can `substring_after(@year_dir, '=')` if a `"2024"`-style value is needed.

### 10.4 `SimpleStrategy` plan emission

Layer-by-layer derivation:

- **L1 Scan.** `needed_columns = {ordered_at, region, amount_cents}`. Two sources → Union of two `Scan`s.
- **L2 Rename.** Per-source Project injecting the metadata literal for `year_dir` (read from `ResolvedPhysicalSource.metadata_values` per `15 §7.6`) and renaming physical to semantic (identity-mapped here).
- **L3 Expression.** Zero `Computed` entries on the referenced Semantics. **Skipped.**
- **Filter injection.** The `region = 'EU'` filter references a semantic name; placed above L2.
- **L4 Agg.** `GROUP BY = {DateTrunc(ordered_at, Month), region}`; `Aggregate(Sum, amount_cents)` as `gross_revenue`. `year_dir` is NOT in the Request's fields list, so it is not a `GROUP BY` key. The source-distinguishing-metadata skip predicate (§4.5.1) examines `GROUP BY` only; `DateTrunc(ordered_at, Month)` and `region` carry values that can repeat across sources → re-aggregation is **not** skipped → per-source partial `Agg` inside each Union branch + merge `Agg` above.
- **L5 Project.** Requested output shape: `(ordered_at, region, gross_revenue)`. L4 output schema: `(ordered_at_month, region, gross_revenue)` — the `GROUP BY` key `DateTrunc(ordered_at, Month)` is materialized under the name `ordered_at_month`, not `ordered_at`. Output requires renaming to `ordered_at`. **Not skipped.**

### 10.5 Final ASCII plan

```
PlanNode::Project                              (L5 — rename ordered_at_month -> ordered_at)
  expressions:
    ordered_at:    Column(ordered_at_month)
    region:        Column(region)
    gross_revenue: Column(gross_revenue)
  |
  +-- PlanNode::Agg                            (L4 — merge)
        group_by:
          DateTrunc(Column(ordered_at), Month) AS ordered_at_month
          Column(region)                       AS region
        aggregates:
          Sum(Column(partial_revenue))         AS gross_revenue
        |
        +-- PlanNode::Union                    (multi-source concatenation)
              |
              +-- PlanNode::Agg                (L4 — per-source partial, source 0)
              |     group_by:
              |       DateTrunc(Column(ordered_at), Month) AS ordered_at_month
              |       Column(region)                       AS region
              |     aggregates:
              |       Sum(Column(amount_cents))            AS partial_revenue
              |     |
              |     +-- PlanNode::Filter       (region = 'EU')
              |           predicate: Eq(Column(region), Literal("EU"))
              |           |
              |           +-- PlanNode::Project  (L2 — rename + metadata injection)
              |                 expressions:
              |                   ordered_at:    Column(ordered_at)
              |                   region:        Column(region)
              |                   year_dir:      Literal("year=2024") # source 0 — raw path token per `15 §8.1.1`
              |                   amount_cents:  Column(amount_cents)
              |                 |
              |                 +-- PlanNode::Scan    (L1 — source 0)
              |                       source: File{path=".../year=2024/..."}
              |                       projected: [ordered_at, region, amount_cents]
              |
              +-- PlanNode::Agg                (L4 — per-source partial, source 1)
                    group_by: <same>
                    aggregates: <same>
                    |
                    +-- PlanNode::Filter       (region = 'EU')
                          |
                          +-- PlanNode::Project  (L2 — source 1)
                                expressions:
                                  ordered_at:    Column(ordered_at)
                                  region:        Column(region)
                                  year_dir:      Literal("year=2025") # source 1 — raw path token per `15 §8.1.1`
                                  amount_cents:  Column(amount_cents)
                                |
                                +-- PlanNode::Scan    (L1 — source 1)
                                      source: File{path=".../year=2025/..."}
                                      projected: [ordered_at, region, amount_cents]
```

### 10.6 Reading key

- **L1** materializes one `Scan` per resolved `PhysicalSource`. Every source projects the same physical column set (`ordered_at, region, amount_cents`) because `needed_columns` is Request-derived and uniform across sources.
- **L2** renames (identity here) and injects the per-source `year_dir` metadata literal (read from `ResolvedPhysicalSource.metadata_values` per `15 §7.6`; pre-resolved at compile per `15 §10.5`). `year_dir` is NOT referenced by the Request (neither in fields nor filters); `SimpleStrategy` still emits it because the metadata-column emission is unconditional at the layer (per Q-DS-003). A future optimizer pass (`34 §5`) elides `year_dir` from L2 when no downstream layer reads it; `21` ratifies the layer structure, not the elision.
- **Filter** sits between L2 and L4, targeting the semantic-named `region`.
- **L4** runs as a per-source partial inside each branch of the Union, and as a merge aggregate above. The per-source partial is an optimizer-driven form (pushed-down from `34 §5`); the merge is the Request's actual aggregate.
- **L5** projects the three requested fields, dropping the internal `partial_revenue` staging name and selecting the output order.

### 10.7 Simpler example — single source, no grain rollup, no aggregation

For a Request of `fields: [order_id, ordered_at, region]`, `from: orders`, no filter — a pure Dimensions-only query against a single-source Simple:

```
PlanNode::Project                              (L2 only — L5 elided by identity)
  expressions:
    order_id:    Column(order_id)
    ordered_at:  Column(ordered_at)
    region:      Column(region)
  |
  +-- PlanNode::Scan                           (L1)
        source: File{path="..."}
        projected: [order_id, ordered_at, region]
```

L3 / L4 / L5 elided per skip rules. This is `SimpleStrategy`'s irreducible minimum.

## 11. Round-1 Open Items

Round-1 drafting surfaced five open questions. Each is parked in `docs/design/questions/open/21_questions.md` with context, options, and a Round-1 working default. Summary:

- **Q-DS-001 — `SimpleDataKind` nested-kind structural label under a Complex.** When a Simple is nested inside a Complex (per `12`), it may carry a structural label separate from its `name:` (per `11 §10`). Does `SimpleDataKind`'s struct need a separate `label: Option<StructuralLabel>` field, or does `name:` double as the label at nested scope? Round-1 default: `name:` doubles.
- **Q-DS-002 — Wrapper code discipline for re-surfaced errors.** Should every compile-time error raised inside a Simple's compile flow be wrapped under a `21`-level `COMP_E_21xx` code (for diagnostic context), or should pass-through to the owning-doc code (`COMP_E_03xx`, `EXPR_E_02xx`) be the norm? Round-1 default: wrap only when context materially aids debugging.
- **Q-DS-003 — Multi-source per-branch metadata emission at L2.** When L1 fans out to N sources and L2 injects per-source metadata literals, should the metadata emission be pruned if no downstream layer reads the metadata Dimension (the optimizer elision case in §10.6)? Or is the emission unconditional (simpler, but adds plan-size)? Round-1 default: unconditional at `SimpleStrategy`; optimizer elides in `34 §5`.
- **Q-DS-004 — Temporal-shape identifier scope on Computed Dimensions.** Can a `temporal_shape:` `event_time:` reference point at a Computed Dimension (rather than a physical-column Dimension)? E.g. `event_time: computed_occurred_at` where `computed_occurred_at` is a Computed Dimension with `expr: substring(timestamp_str, 1, 19)`. Round-1 default: yes, any Dimension with a `Timestamp` / `Date` / `Time` type regardless of its `ColumnMappingValue` variant.
- **Q-DS-005 — Does L4's re-aggregation-skip predicate consider Computed Dimensions in `GROUP BY`?** A Computed Dimension in `GROUP BY` whose expression is source-distinguishing (e.g. `Case WHEN source_metadata = 'A' THEN 1 ELSE 0 END`) is technically source-distinguishing, but the predicate in §4.5.1 only examines metadata Dimensions in v1. Should Computed Dimensions be extended in? Round-1 default: v1 only checks metadata Dimensions; computed extension deferred.

---

**End of document.** Round-1 ratified decisions are inline throughout §§2–9; open items parked in `docs/design/questions/open/21_questions.md`.
