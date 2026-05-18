---
prereqs: [20, 11, 13, 14, 15, 16, 17]
authoritative-for:
  - `Joinset` canonical ComplexDataKind shape (`JoinsetDataKind` struct sketch, `DataKind::Joinset` variant)
  - anchor specification — mandatory single root child, "FROM-clause" semantics, fan-out reference frame
  - explicit join-path specification (`ExplicitPath`, `JoinHop`, per-hop `RelationshipRef` citation)
  - implicit join-path selection rules (anchor-biased specialization of `16 §11`'s field-first algorithm)
  - `JoinsetStrategy` — the planner contract that lowers a resolved `Joinset` into `PlanNode::Join` sequences
  - per-hop `Cardinality` propagation, fan-out accumulation, and multi-fanout advisory emission
  - as-of activation rules — temporal-shape-gated `JoinType::AsOf` consumption (forward-ref `17 §5`)
  - v1 binary-arity restatement (per `12 §5.2`), `TD-JOINSET-NARY` tech-debt marker
  - validate / compile / plan error rosters:
    - `VALID_E_2400`–`2499` — structural validate-stage `Joinset` failures
    - `COMP_E_2400`–`2499` — compile-stage path-resolution failures
    - `PLAN_E_2400`–`2499` — plan-stage `Joinset` usage failures
    - `PLAN_W_2400`–`2499` — plan-stage `Joinset` advisories
refined-by:
  - 17 (`foundations/17_temporal_shape.md` — `TemporalShape`, as-of activation matrix for `JoinType::AsOf`)
  - 25 (cross-kind strategy catalog — Joinset × Grainset, Joinset × Unionset composition rules)
  - 32 (`apis/32_semstrait_model.md` — YAML surface for the `joinsets:` top-level block and `path:` sub-block)
  - 33 (`apis/33_semstrait_manifest.md` — `ResolvedJoinset`, the SemanticManifest entry carrying the materialized `ComposedSemanticInterface`)
  - 34 (`apis/34_semstrait_planner.md` — `JoinsetStrategy` implementation, path-resolve entry points)
  - 35 (`PlanNode::Join` consumption of `Joinset`-derived join sequences; `JoinNode.from_relationship` + `from_joinset` tagging)
---

# 24. Joinset

> **Reconciliation (Phase-3, 2026-04-17; relationship-block rebase 2026-05-12).** The v1 authoring-layer canonical shape for `Joinset` is ratified across:
>
> - `[../apis/32_semstrait_model.md §3](../apis/32_semstrait_model.md)` — top-level YAML tag (`joinsets:`), `JoinsetBody` struct shape.
> - `[../foundations/18_entities.md §2](../foundations/18_entities.md)` — canonical `Relationship` struct (unified across root-level `relationships:` and `JoinsetBody.relationships`). Key decisions:
>   - `JoinsetBody.relationships: Vec<Relationship>` — no separate `JoinRelationship` type. The unified struct is **semantic-first**, carrying `{ name, from, to, keys, filter?, cardinality, integrity, optional?, cross_filter?, ai_context?, description? }`.
>   - `JoinType` is **derived** at compile from `Relationship.optional` per `18 §2.9`, not authored. v1 roster: `{Inner, Left, Right, Full}`, `#[non_exhaustive]`. `AsOf` is descoped for v1 (post-v1 deferred per `17`).
>   - Join keys shape: `keys: [{from: <SemanticExpr>, to: <SemanticExpr>}, …]` equi-pair list + optional `filter: <SemanticExpr>` residual predicate.
>   - `cardinality:` is required at every Relationship authoring site (SR-E-4). `optional:` and `cross_filter:` are required on `OneToOne` / `ManyToMany` (SR-E-13); directional `cross_filter` is forbidden on `ManyToMany` (SR-E-14).
>   - The earlier `directionality:` field and `Directionality` enum are **retired (2026-05-12)** — every Relationship is bidirectional by construction; authors who need to forbid auto-synthesized reverse traversal declare an explicit Joinset.
>   - The earlier per-hop **`JoinTypeOverrides` / `HopPosition`** carriage on `JoinsetDataKind` is **retired (2026-05-12)**. A Joinset that needs different join semantics declares a **scope-local `Relationship`** in its own `relationships:` block; the scope-shadow rule in `18 §2.10` resolves visibility. See `16 §13.3` for the sole permitted mechanism.
> - `[26_nesting_matrix.md](./26_nesting_matrix.md)` — nesting rules. Notably **R3** (every `ComplexDataKind` requires ≥ 2 children).
> - `[../questions/deferred/24_questions.md](../questions/deferred/24_questions.md)` — Q-24-09 (`JoinAssociativity`) + Q-24-10 (star / snowflake / 3NF shape-tag vocabulary), folded from the former `joinset_shape_semantics.md` sidecar on 2026-04-17.
>
> This document retains authority for:
>
> - The **anchor** contract (§3) — mandatory single root child, FROM-clause semantics, fan-out reference frame.
> - The **join-path** contract (§4) — explicit-path vs implicit-path authoring modes and their interaction with the Relationship graph.
> - `JoinsetStrategy` planner contract (§5) — lowering to `PlanNode::Join` sequences in anchor-outward order; per-hop `Cardinality` propagation; per-hop `JoinType` is derived from each traversed Relationship's `optional` field per `18 §2.9` (no override surface).
> - `VALID_E_24NN` / `COMP_E_24NN` / `PLAN_E_24NN` / `PLAN_W_24NN` error-code allocations.
>
> Rust-struct and YAML-surface body sections predate `18` (formerly `32c`); `JoinRelationship` / `JoinTypeOverrides` / `HopPosition` in legacy body text are pre-unification or pre-rebase vocabulary — read the `Relationship` shape from `18 §2`. `AsOf` sections are forward-reference only; v1 does not emit `AsOf` joins. `ColumnMapping` → `SemanticMapping` rename per `18 §10`.

## 1. Purpose and Scope

### 1.1 What `24` ratifies

`24` is the canonical-layer specification for the `**Joinset`** variant of `DataKind`. It fills in what `16 §13` deferred:

- **§2** — the `JoinsetDataKind` canonical struct shape: `{ anchor, members, path, relationships, interface, ... }`, its placement as a `ComplexDataKind` variant, and how it composes via `CompositionKind::Joinset`.
- **§3** — the **anchor** contract: exactly one root child, which member plays the FROM-clause role, which member drives the fan-out reference frame. Rationale for mandating the anchor (determinism, fanout predictability, and traversal ambiguity resolution).
- **§4** — the **join-path** contract: the two authoring modes (implicit path = name the members, let the planner compute; explicit path = pin a `RelationshipId`-indexed traversal), their selection rules, and their interaction with `16`'s Relationship graph.
- **§5** — the `**JoinsetStrategy`** planner contract: how the planner lowers a resolved `Joinset` into `PlanNode::Join` nodes in anchor-outward order, how per-hop `JoinType` is derived from each traversed Relationship's `optional` field (per `18 §2.9`; scope-local Relationship shadow per `18 §2.10` is the sole mechanism for divergent join semantics), how `Cardinality` propagates, and how post-join Project nodes reconcile the unified surface per `16 §6`.
- **§6** — `Joinset` as a **consumer** (not declarer) of `Relationship`s. `Joinset`'s `path` references Relationships by id or by citing the Relationship graph; it never introduces a new Relationship.
- **§7** — the `**TemporalShape`** interaction: as-of join gating — `JoinType::AsOf` is legal only when the hop's `to`-side carries a temporal shape that `17 §5` sanctions (`Events ↔ Snapshot`, `Events ↔ SCD`, etc.). `17` is parallel-drafted; `24` fixes the integration points so the hookup is mechanical once `17` lands.
- **§8** — the `**ComposedSemanticInterface`** the `Joinset` publishes: how `UnifiedSemantics` (`16 §6`), `FieldProvenance` (`16 §7`), and `CompositionCoverage` (`16 §8`) apply under `CompositionKind::Joinset`.
- **§§9–11** — the validate-, compile-, and plan-stage error rosters specific to `Joinset`, allocated against the `*_E_24NN` / `PLAN_W_24NN` ranges.
- **§12** — two worked examples: a star-schema shape (§12.1) illustrating anchor + implicit path; a multi-path graph (§12.2) illustrating explicit-path pinning.

### 1.2 What `24` does NOT ratify (forward-refs)

- `**Relationship` shape, `Cardinality`, `JoinType`** — all three live canonically in `16 §§2–4`. `24` cites and consumes; it never redefines.
- **Implicit composition for `Request.from = None`** — that is `16 §11`'s field-first lookup over compile-time-enumerated compositions (`16 §10.4`). Both implicit and explicit Joinsets produce `CompositionKind::Joinset`; the distinguishing axis is `Origin` per `16 §5.6` (`Origin::Explicit` for author-declared, `Origin::Implicit { id }` for compile-enumerated). §1.3 below sharpens the boundary.
- `**TemporalShape` variants and the `Additivity × TemporalShape` matrix** — `17`. `24` only records the integration point for `JoinType::AsOf` activation.
- **N-ary `Joinset` authoring shape** — deferred. `12 §5.2` ratifies binary-arity for v1 under `TD-NESTING-NARY-JOIN`; `24 §2.5` restates the constraint and tracks the semantic shape the N-ary extension will take. The canonical struct in §2.2 is written for N-ary to keep the MINOR lift-to-N-ary purely structural (adding `path: Vec<JoinHop>` rather than swapping in a new struct).
- **YAML surface for `joinsets:`** — `32`. The `path.on.left` / `path.on.right` structural-label-dotted-column form declared in `12 §5.1` is the v1 YAML convention; `24` reasons about the canonical-layer shape (post-parse, `ResolvedJoinset`).
- `**ResolvedJoinset` / `PlanNode::Join` exact field rosters** — `33` / `35`. `24` fixes the carriage contract (which fields each layer needs) without freezing struct layout.

### 1.3 Sharpening the implicit-vs-`Joinset` boundary

Every Joinset could, in principle, be expressed by declaring the constituent DataKinds plus declaring the required Relationships, and then issuing a `Request.from = None` — `16 §11`'s field-first algorithm would synthesize a `ComposedSemanticInterface` over the same constituents. The two are **not** the same object, and Round-1 design preserves that distinction:


| Dimension                   | Implicit (`16 §11`)                                     | `Joinset` (`24`)                                                  |
| --------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------- |
| `CompositionKind`           | `Relationship`                                          | `Joinset`                                                         |
| Named / addressable         | No (anonymous, Request-scoped)                          | Yes (a `DataKindRef`; valid `Request.from`)                       |
| Anchor                      | None (BFS is symmetric over the Semantics owners)       | Mandatory exactly-one root (§3)                                   |
| `JoinType` per hop          | Derived from each Relationship's `optional` field       | Same derivation; divergent semantics via scope-local Relationship shadow (`§5.3.2`) |
| SemanticManifest lifecycle  | Synthesized at `plan` (`16 §10.1`)                      | Materialized at `compile` (`16 §10.1`)                            |
| Depth bound                 | `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` (`16 §9.1`)        | None (authoring a `Joinset` is the escape hatch)                  |
| Ambiguous paths             | `PLAN_E_0500 AmbiguousImplicitComposition` (`16 §14.3`) | Author pins via explicit path (§4.2); never an error at plan time |


A `Joinset` is what the author reaches for when they need (a) a named surface, (b) a pinned path, (c) divergent join semantics on a hop (via scope-local `Relationship` shadow per `§5.3.2`), or (d) a composition deeper than 4 hops. Everything else an author might otherwise reach for a `Joinset` for is served by implicit composition + declared Relationships.

### 1.4 Invariants `24` directly upholds

- **I1 — canonical layer.** A `Joinset`'s anchor, members, and path references are `DataKindRef` / `RelationshipId` handles; never SQL column names. Resolution to physical join predicates happens during `JoinsetStrategy` → `PlanNode::Join` lowering, which delegates to `15`-resolved `Binding.column_mapping` at each hop.
- **I4 — determinism.** For a fixed SemanticManifest and a fixed `Joinset`, the materialized `ComposedSemanticInterface` is bit-identical; the planner's `JoinsetStrategy` emits the same `PlanNode::Join` sequence on every invocation. Implicit-path resolution uses `16 §11.4`'s deterministic neighbor order (extended for anchor bias in §4.1.3).
- **I5 — compile-time resolution.** `Joinset.path` (whether implicit or explicit) is fully resolved to `Vec<RelationshipId>` at `compile`. The `plan` stage never re-walks the Relationship graph for a `Joinset`.
- **I7 — strict crate DAG.** `Joinset` resolution lives in `semstrait-manifest`; `JoinsetStrategy` lives in `semstrait-planner`; `PlanNode::Join` construction consumes the resolved `Joinset` without re-resolution.
- **I8 — SemanticManifest is planner-complete.** A `ResolvedJoinset` carries everything the planner needs: resolved anchor, resolved `RelationshipId` sequence with forward/reverse direction per hop, the resolved Relationship at each hop (root-level or scope-local shadow per `18 §2.10`) from which `JoinType` is derived at plan emission, resolved `ComposedSemanticInterface`.
- **I10 — non-exhaustive extensibility.** `JoinsetDataKind`, `JoinHop`, `ExplicitPath`, `JoinsetStrategy`'s inputs / outputs are all `#[non_exhaustive]`. N-ary lift (`TD-NESTING-NARY-JOIN`) and `JoinType::AsOf` activation (pending `17`) are MINOR additions.
- **I12 — first-class diagnostics.** Every `Joinset` Precondition has a stable error code in the `*_E_24xx` / `PLAN_W_24xx` ranges (§§9–11).

## 2. The `Joinset` Variant

### 2.1 Placement in the DataKind taxonomy

`Joinset` is one of the four `CompositionKind` variants ratified in `16 §5.3`:

```rust
// (From `16 §5.3`, quoted for reference only.)
#[non_exhaustive]
pub enum CompositionKind {
    Relationship,   // implicit (16 §11)
    Unionset,       // explicit — `23_unionset.md`
    Grainset,       // explicit — `22_grainset.md`
    Joinset,        // explicit — this document
}
```

The author-declared top-level kind that lowers to `CompositionKind::Joinset` is the `DataKind::Joinset(JoinsetDataKind)` variant, sketched below.

### 2.2 The canonical struct

```rust
/// Author-declared `Joinset` in canonical-layer (post-parse, pre-bind)
/// form. `21`'s Unionset and `22`'s Grainset peers share the
/// `ComplexDataKind` shape without further coupling.
#[non_exhaustive]
pub struct JoinsetDataKind {
    /// Human-readable canonical name; globally unique per `11 §3`.
    pub name: DataKindName,

    /// The root child — the member that plays the FROM-clause
    /// role and anchors fanout analysis. See §3. MUST be an element
    /// of `members`. Exactly one.
    pub anchor: DataKindRef,

    /// The named subset of DataKinds composed by this Joinset.
    /// Contains `anchor` plus zero-or-more other members. v1 arity
    /// per `12 §5.2` is binary (`members.len() == 2`); §2.5 restates
    /// the constraint. The struct is intentionally N-ary-ready so
    /// `TD-NESTING-NARY-JOIN` lifts without a struct swap.
    pub members: Vec<DataKindRef>,

    /// The traversal specification. `None` → implicit path (planner
    /// computes via §4.1); `Some(ExplicitPath)` → author-pinned path
    /// (§4.2).
    pub path: Option<ExplicitPath>,

    /// Scope-local `Relationship` declarations. The struct shape,
    /// validation rules, and defaults matrix are identical to the
    /// root-level `relationships:` block per `18 §2`. A scope-local
    /// entry that shares its name with a root-level Relationship
    /// shadows the root-level one within this Joinset's scope only
    /// (`18 §2.10`). This is the sole mechanism for varying join
    /// semantics inside a Joinset — there is no per-hop override
    /// surface.
    #[serde(default)]
    pub relationships: Vec<Relationship>,

    /// The Joinset's own interface — the declared Dimensions /
    /// Measures / Metrics / Filters / Keys that live at the Joinset
    /// scope. Per `11 §2`, nested members contribute their semantics
    /// through the unified surface; the `Joinset` itself may declare
    /// composition-level semantics (e.g. a composite key, a derived
    /// dimension).
    pub interface: JoinsetInterface,
}
```

Every field is required at the canonical layer except `path`, `relationships`, and (post-compile) the derived `ComposedSemanticInterface`. The struct is `#[non_exhaustive]` per I10; MINOR additions (`fanout_strategy`, `parallelism_hint`, ...) carry explicit defaults.

> **Retired (2026-05-12).** Earlier drafts of this struct carried `overrides: JoinTypeOverrides` and a companion `HopPosition` newtype for per-hop join-type override carriage. Both are removed in v1 in favour of the scope-local `Relationship` shadow model (`16 §13.3`). See `[../questions/closed/24_questions.md](../questions/closed/24_questions.md)` for the closure record.

### 2.3 What `ExplicitPath` carries

```rust
#[non_exhaustive]
pub struct ExplicitPath {
    /// The ordered sequence of hops from anchor outward. A single-
    /// element `Vec` is the binary-Joinset case. N-ary (`TD-NESTING-
    /// NARY-JOIN`) extends this to a spanning-tree walk; v1 keeps it
    /// linear.
    pub hops: Vec<JoinHop>,
}

#[non_exhaustive]
pub struct JoinHop {
    /// The Relationship traversed by this hop. MUST be a
    /// `RelationshipId` declared in the SemanticManifest's `relationships:`
    /// top-level block per `16 §2.1`.
    pub relationship: RelationshipId,

    /// The traversal direction. `Forward` walks
    /// `Relationship.from → Relationship.to`; `Reverse` walks the
    /// opposite. Every `Relationship` is bidirectional in v1
    /// (`16 §2.4`), so either direction is always permissible.
    pub direction: HopDirection,

    /// The DataKindRef that this hop lands on. Redundant with the
    /// underlying Relationship's endpoint under `direction`, but
    /// stored for validation symmetry (compile cross-checks the
    /// match; see §10.1 `COMP_E_2402`).
    pub to: DataKindRef,
}

#[non_exhaustive]
pub enum HopDirection {
    Forward,
    Reverse,
}
```

Round 1 uses `HopDirection` explicitly rather than inferring direction from the ordered `relationship.from` / `relationship.to` fields because the same `RelationshipId` can be walked either direction on a bidirectional edge, and explicit direction makes validation (§§10.1–10.2) straightforward.

### 2.4 Composition via `CompositionKind::Joinset`

Per `16 §10.1`, an explicit `ComplexDataKind` is materialized at `compile` as a `ResolvedComplexDataKind` carrying its `ComposedSemanticInterface`. For `Joinset`:

```text
compile(JoinsetDataKind) → ResolvedJoinset {
    name, anchor, members, hops: Vec<ResolvedJoinHop>, relationships,
    interface: ComposedSemanticInterface {
        composition_kind: CompositionKind::Joinset,
        constituents: <members, canonicalised anchor-first>,
        interface: UnifiedSemantics <per §8.2>,
        provenance: FieldProvenance <per §8.3>,
        coverage: CompositionCoverage <per §8.4>,
        traversed_paths: Vec<RelationshipPath> <recorded for audit/debug>,
    },
}
```

`33` ratifies the exact `ResolvedJoinset` struct roster. `24` fixes only the semantic content: the `ComposedSemanticInterface` carried by a `ResolvedJoinset` MUST have `composition_kind == CompositionKind::Joinset` and MUST populate `constituents` in anchor-first order (see §3.2 for why).

### 2.5 v1 arity — binary only (restatement)

`12 §5.2` ratifies binary-only Joinsets in v1: exactly two members, `path.hops.len() == 1`. `24` does not relax this constraint and does not redefine it; the canonical struct in §2.2 is N-ary-ready to make `TD-NESTING-NARY-JOIN`'s MINOR lift a struct-preserving extension.

Enforcement:

- **Validate.** `members.len() == 2` or `VALID_E_2400 JoinsetArityV1Violation` per §9.1. (`12 §7` also emits its own nesting-shape variant `NV-V5`, ratified there; `24 §9.1` provides the canonical-layer twin so non-YAML construction paths are covered.)
- **Compile.** `hops.len() == 1` for an explicit path, or `implicit_path_result.hops.len() == 1` for an implicit path. N-ary inputs were rejected at validate; this is a defense-in-depth check against mis-compilation (`COMP_E_2408 JoinsetArityV1Invariant`).

For v1 the implicit-path algorithm (§4.1) always produces exactly one hop when it succeeds, or fails with `COMP_E_2401 JoinsetImplicitNoPath` (§10.1).

### 2.6 Nested Joinset interfaces

A `JoinsetDataKind` declares its own `JoinsetInterface` — Dimensions / Measures / Metrics / Filters / Keys at the Joinset scope (per `11 §2`). The canonical-layer rules are:

- **Keys.** MAY be declared on the Joinset (typically echoing the anchor's key tuple per `16 §6.5`). Declared keys participate in `UnifiedSemantics.keys` (§8.2).
- **Dimensions / Measures / Metrics.** MAY be declared at the Joinset scope as composition-level semantics. They are recorded in `UnifiedSemantics` with `FieldOwnership::Derived` (for computed) or `FieldOwnership::Native(anchor)` (for delegation to the anchor member), per `16 §7.3`.
- **Filters.** MAY be declared; they apply at the composed surface, post-join.

The majority of authored Joinsets declare no Joinset-level semantics and rely entirely on the member surfaces unified per §8.2.

## 3. Anchor Specification

### 3.2 Rules for the anchor


| Rule                                                                      | Enforcement                                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The anchor MUST be non-empty.                                             | `VALID_E_2401 JoinsetAnchorMissing` — validate-stage structural check.                                                                                                                                                                                              |
| The anchor MUST be a member of `members`.                                 | `VALID_E_2402 JoinsetAnchorNotMember` — validate-stage.                                                                                                                                                                                                             |
| Exactly one anchor per Joinset.                                           | Enforced by `anchor: DataKindRef` being a scalar; multi-anchor declarations are rejected at YAML parse per `32` (the canonical layer only exposes the one-anchor shape).                                                                                            |
| The anchor MUST be reachable by every other member via the resolved path. | `COMP_E_2403 JoinsetAnchorUnreachable { unreachable_member }` — compile-stage, after path resolution. In v1 with binary arity this is degenerate (the other member is directly reached by the single hop); the rule is stated for forward-compatibility with N-ary. |
| The anchor MUST NOT itself be a `Joinset` at the YAML authoring level.    | Same-kind ban per `12 §2` matrix. `Joinset` is not a legal child of `Joinset` at any role, including anchor. Enforced by `12 §7`'s nesting-matrix validate pass; `24` does not re-enforce.                                                                          |


### 3.3 FROM-clause semantics

The FROM-clause role is made explicit in `JoinsetStrategy`'s plan-tree output. For a resolved Joinset with anchor `A` and members `[A, B]`:

```text
PlanNode::Project(<unified surface>)
  └── PlanNode::Join {
          join_type: <per §5.3>,
          left: PlanNode::Scan(A),     // anchor = left
          right: PlanNode::Scan(B),    // reached member = right
          keys: <from Relationship.keys, resolved via `15`>,
          from_relationship: Some(<hop_0.relationship>),
          from_joinset: Some(<Joinset.name>),
      }
```

The `left`-slot identification of the anchor is the plan-tree manifestation of §3.1's row-set-reference-frame rule. Even when the hop's underlying `Relationship.from / to` is reversed (i.e. `JoinHop.direction == Reverse`), the anchor remains the plan-tree left: `JoinsetStrategy` flips the `KeyPair` orientation as needed but preserves anchor-left structurally.

### 3.4 Anchor and fan-out

Fanout analysis per `16 §3.3` operates on **walked** cardinality. For a Joinset with hop `H` from the anchor to member `M`:

- If `Relationship.cardinality == ManyToOne` and `H.direction == Forward`: no fanout; anchor rows lookup-join single-row matches on `M`.
- If `Relationship.cardinality == OneToMany` and `H.direction == Forward`: **anchor rows fan out** — each anchor row matches zero-or-more `M` rows.
- If `Relationship.cardinality == ManyToOne` and `H.direction == Reverse`: fanout (effective cardinality mirrors to `OneToMany` per `16 §3.4`).
- If `Relationship.cardinality == OneToOne`: no fanout, direction-independent.
- If `Relationship.cardinality == ManyToMany`: fanout in both directions; `PLAN_W_2402 JoinsetManyToManyHopAdvisory` per §11.2.

Multi-hop fanout (N-ary Joinsets) accumulates per §5.4; v1's single-hop case is one of the cases above.

## 4. Join-Path Specification

### 4.1 Implicit path

#### 4.1.1 Authoring shape

The author declares `members` (including the `anchor`) and omits `path`. Compile derives a `ResolvedJoinset.hops` list by running an **anchor-rooted** variant of `16 §11.4`'s `RELATIONSHIP_BFS`.

#### 4.1.2 Algorithm (binary v1)

```text
JOINSET_IMPLICIT_PATH(anchor, members, manifest) -> Vec<ResolvedJoinHop> | CompileError:
  1. other_members ← members \ { anchor }
  2. if |other_members| != 1:
        return COMP_E_2408 JoinsetArityV1Invariant  // defense in depth; validate
                                                      // should have caught this already
  3. target ← unique(other_members)
  4. paths ← RELATIONSHIP_BFS_ANCHORED(anchor, target, manifest)
        where:
          - source = anchor
          - target = target
          - depth limit = unbounded (Joinset is the escape hatch; 16 §9.1's
            MAX_IMPLICIT_COMPOSITION_DEPTH does NOT apply here)
          - neighbor order = RelationshipId ascending (I4, same as 16 §11.4)
          - direction constraints = none; every Relationship is bidirectional
            per 16 §2.4
  5. if paths is empty:
        return COMP_E_2401 JoinsetImplicitNoPath { anchor, target }
  6. if |paths| > 1 AND shortest-path length is not unique:
        return COMP_E_2402 JoinsetImplicitAmbiguousPaths { anchor, target, candidates }
                                                      // author must pin via explicit path
  7. selected ← the unique shortest path
  8. return Vec<ResolvedJoinHop> constructed from `selected` with each hop
           carrying its RelationshipId, walked direction, and landed-on DataKindRef.
```

#### 4.1.3 Extension of `16 §11.4`

`16 §11`'s field-first algorithm visits **multiple** target kinds (one for each requested Semantics's owning kind). `Joinset`'s implicit-path variant visits exactly the other member(s) explicitly listed in `members`, with the anchor as the fixed source. The extension is:

1. **Anchor pinning.** Source is the `anchor`; BFS walks outward from it. In `16 §11.4` BFS walks from the full set of `OwningKinds` inward (Steiner-tree-style); here BFS walks from a single known source.
2. **No depth limit.** `16 §9.1`'s `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` does NOT apply. Declaring a `Joinset` is exactly what an author does when their path exceeds the implicit depth cap; imposing the same cap on the escape hatch would be pointless.
3. **Ambiguity handling deferred to compile.** Implicit composition surfaces ambiguity at plan time (`PLAN_E_0500`). Joinset's implicit path surfaces ambiguity at **compile** time (`COMP_E_2402`), because the Joinset is a named surface whose path must be deterministic before plan-time queries land.

Algorithmic I4-determinism is preserved: same SemanticManifest + same anchor + same target → same resolved hops.

#### 4.1.4 When to use implicit path

Authors prefer implicit when:

- The SemanticManifest has a single shortest Relationship path between anchor and target. Typical for star-schema shapes where the fact → dimension hop is unique.
- The Relationship path is expected to stabilize; new Relationships that create alternative paths would be caught at the `compile` that introduces them (`COMP_E_2402`), giving the author a chance to pin.

Authors prefer explicit (§4.2) when:

- Multiple equal-length paths exist and the author has a specific one in mind.
- The author needs divergent join semantics on a specific hop: declare the divergence on a scope-local Relationship (`§5.3.2`) whose `name` matches the hop's traversed Relationship, and pin the explicit path so the scope-local shadow lands on the intended hop.
- The `Joinset` will grow in the future (e.g. when `TD-NESTING-NARY-JOIN` lifts), and the author wants the traversal shape pinned regardless of later Relationship additions.

### 4.2 Explicit path

#### 4.2.1 Authoring shape

The author declares `path: Some(ExplicitPath { hops: Vec<JoinHop> })`. Each `JoinHop` cites a `RelationshipId` and a `HopDirection`. For v1 binary Joinsets, exactly one hop.

#### 4.2.2 Validation pipeline

Explicit path validation runs across validate and compile stages:


| Stage    | Check                                                                                                                                   | Error                                                                                                                                                                                           |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| validate | `hops` non-empty                                                                                                                        | `VALID_E_2403 JoinsetExplicitPathEmpty`                                                                                                                                                         |
| validate | (v1) `hops.len() == 1`                                                                                                                  | `VALID_E_2400 JoinsetArityV1Violation`                                                                                                                                                          |
| compile  | each `hop.relationship` resolves to a SemanticManifest `RelationshipId`                                                                 | `COMP_E_2404 JoinsetExplicitPathUnknownRelationship { position, relationship }`                                                                                                                 |
| compile  | `hop_0`'s walked source == `anchor` (given `direction`, the "source" endpoint matches the anchor DataKind)                              | `COMP_E_2405 JoinsetExplicitPathAnchorMismatch { expected_anchor, actual_source }`                                                                                                              |
| compile  | `hop_i.to` matches the Relationship's walked target endpoint (given `direction`)                                                        | `COMP_E_2406 JoinsetExplicitPathEndpointMismatch { position, declared_to, computed_to }`                                                                                                        |
| compile  | `hop_{i+1}`'s source (given direction) == `hop_i.to` (chain continuity) — no-op for v1 binary, but kept for N-ary forward-compatibility | `COMP_E_2407 JoinsetExplicitPathDiscontinuity { position }`                                                                                                                                     |
| compile  | (binary v1) exactly the `members` set is covered by the explicit path's endpoints                                                       | `COMP_E_2410 JoinsetExplicitPathUncoveredMembers { uncovered }`                                                                                                                                 |


#### 4.2.3 Self-joins and cyclic paths

- `Relationship.from == Relationship.to` (self-joins) is forbidden per `16 §12.4 COMP_E_0403`; a Joinset CANNOT reference a self-referential Relationship because none exist in v1. Tracked as `[TD-COMPOSITION-SELFJOIN]` in `16 §12.4`; `24` inherits.
- Cyclic explicit paths (visiting the same DataKind twice) are rejected at `VALID_E_2405 JoinsetExplicitPathCyclic`. Binary v1 cannot produce a cycle (one hop), so the rule is N-ary-forward-looking.

#### 4.2.4 Scope-local Relationship resolution

When a Joinset declares a scope-local `Relationship` (per `§5.3.2`) and an explicit path, compile resolves each `hop.relationship` `RelationshipId` against the Joinset's scope first, then falls back to root-level. A scope-local Relationship with the same `name` as a root-level one shadows it within this Joinset (`18 §2.10`); the path's `RelationshipId` is rebound to the scope-local entry. With an implicit path, scope resolution runs after path enumeration: BFS walks the root-level Relationship graph to enumerate paths, then each resolved hop's Relationship is rebound to its scope-local shadow (if any) before `EFFECTIVE_JOIN_TYPE` runs.

### 4.3 Mode-selection precedence


| `path` value                           | Resolved mode                                                                               |
| -------------------------------------- | ------------------------------------------------------------------------------------------- |
| `None`                                 | Implicit (§4.1).                                                                            |
| `Some(ExplicitPath { hops: vec![] })`  | Rejected at validate (`VALID_E_2403`). Empty explicit paths are never an implicit-fallback. |
| `Some(ExplicitPath { hops: [_, ..] })` | Explicit (§4.2).                                                                            |


Round 1 forbids hybrid modes ("use these specific hops plus let the planner fill in the rest"). Hybrid modes are tracked as `[TD-JOINSET-HYBRID-PATH]` in `questions/closed/24_questions.md` (Q-24-02 — closed; TD marker carries the post-v1 reactivation).

## 5. `JoinsetStrategy`

### 5.1 Path resolution

`JoinsetStrategy` is the planner-side contract that lowers a `ResolvedJoinset` to `PlanNode::Join` nodes. Path resolution runs at `compile` (per §4); `JoinsetStrategy` consumes the already-resolved `hops: Vec<ResolvedJoinHop>` without re-walking the Relationship graph.

```rust
#[non_exhaustive]
pub struct JoinsetStrategy<'m> {
    pub joinset: &'m ResolvedJoinset,
    pub manifest: &'m SemanticManifest,
}

impl<'m> JoinsetStrategy<'m> {
    /// Lower the Joinset into a `PlanNode` tree. Caller passes the
    /// Request's projection list; strategy returns the post-join
    /// Project node. `34` ratifies the exact signature.
    pub fn lower(self, projection: &Projection, advisories: &mut DiagnosticSink)
        -> Result<PlanNode, PlannerError>;
}
```

`34` ratifies the exact signature and the `DiagnosticSink` shape. `24` fixes the contract: resolution has already happened; `lower` emits nodes and advisories.

### 5.2 `PlanNode::Join` emission

For each `ResolvedJoinHop` in anchor-outward order:

1. **Left operand.** Hop 0's `left` is the anchor's `PlanNode::Scan`. Hop `i > 0`'s `left` is the join-tree built up through hop `i - 1`. (For v1 binary, only hop 0 exists.)
2. **Right operand.** Hop `i`'s `right` is the `PlanNode::Scan` of `hops[i].to`.
3. **Keys.** From `Relationship.keys` (`16 §2.3`), resolved to physical columns via `15`-resolved `Binding.column_mapping`. Direction-aware swap: if `hop.direction == Reverse`, `KeyPair.left` and `KeyPair.right` are swapped so the plan-tree's left-side key matches the anchor-side of the hop.
4. `**JoinType`.** Per §5.3's derivation from the resolved Relationship's `optional` field.
5. `**from_relationship`.** `Some(hop.relationship)`, carrying the originating Relationship for debug / audit.
6. `**from_joinset`.** `Some(self.joinset.name)` — a new field on `JoinNode` per `35`'s parallel draft, carrying the resolved Joinset's `DataKindName` (which is the synthetic `__implicit_joinset_<8-hex>` for `Origin::Implicit { id }` Joinsets per `16 §5.7`, or the author-declared name for `Origin::Explicit` Joinsets). The two are uniform downstream — strategy emission does not distinguish on `Origin`. (`24` records the requirement; `35` ratifies the struct.)

ASCII plan tree for a binary Joinset `OrdersWithCustomers` anchor-left:

```text
Project <unified surface>
  └── Join {
         join_type: <effective, §5.3>,
         from_relationship: Some(Rel(orders_to_customers)),
         from_joinset: Some(DataKindName("orders_with_customers")),
         keys: [KeyPair { left: "customer_id" (on orders), right: "customer_id" (on customers) }],
         left: Scan(orders) {
             <push-down projection / filters per 19 §3 and 34>
         },
         right: Scan(customers) {
             <same>
         }
     }
```

### 5.3 `JoinType` selection — derived from the traversed Relationship

#### 5.3.1 Default selection

Hop `i`'s `JoinType` is **derived** from the underlying `Relationship` at `hops[i].relationship`: the planner reads `Relationship.optional` and applies the `18 §2.9` derivation table (`None → Inner`, `Left → Left`, `Right → Right`, `Both → Full`). Direction-agnostic at the canonical layer: `Reverse` walks read the same canonical `optional` value; the planner substitutes the mirror form (`Left ↔ Right`) at plan emission (per `16 §2.4.1`).

Pseudocode:

```text
EFFECTIVE_JOIN_TYPE(hop, manifest) -> JoinType:
  rel ← manifest.relationship_for_hop(hop)        // applies scope-local shadow per 18 §2.10
  derived ← derive_join_type(rel.optional)        // 18 §2.9 table
  if hop.direction == Reverse:
    return mirror(derived)                        // Left ↔ Right; Inner/Full unchanged
  else:
    return derived
```

#### 5.3.2 Varying join semantics inside a Joinset

There is **no per-hop override surface** in v1. A Joinset that needs different join semantics for a given hop declares a **scope-local `Relationship`** in its own `relationships:` block that shadows the root-level one (`18 §2.10`, `16 §13.3`). The scope-local Relationship is a full Relationship: it must declare `cardinality:`, and `optional:` / `cross_filter:` per SR-E-13 / SR-E-14. The derivation table then runs against the scope-local `optional` value during `EFFECTIVE_JOIN_TYPE` resolution.

Example (semi-formal):

```yaml
# Root-level:
relationships:
  - name: orders_to_customers
    optional: none                    # derives Inner
    # ...

# Joinset wanting NULL-padded enrichment for facts:
joinsets:
  - name: orders_with_optional_customer
    relationships:
      - name: orders_to_customers     # scope-local shadow
        optional: left                # derives Left for this Joinset only
        # ...
```

The root-level Relationship's derived `JoinType` is unchanged for all other consumers (implicit composition, other Joinsets). Only this Joinset sees the shadowed entry.

> **Retired (2026-05-12).** Earlier drafts of this section carried §5.3.2 *Override application*, §5.3.3 *Override-legality matrix*, and §5.3.4 *Override interaction with fanout*, with a permitted-override matrix gated on the underlying Relationship's declared `JoinType`. The override surface is removed in favour of the scope-local-shadow rule above. The associated `COMP_E_2411 JoinsetIllegalJoinTypeOverride` and `PLAN_W_2401 JoinsetJoinTypeOverrideAdvisory` are retired (see `[../questions/closed/24_questions.md](../questions/closed/24_questions.md)`); their numeric codes are reserved for forward-compat. Fanout interactions (e.g. anchor preservation under `OneToMany`) carry through verbatim — the scope-local Relationship simply declares the divergent `optional`, and `§5.4`'s fanout accounting reads the same derived `JoinType` it always did.


### 5.4 `Cardinality` propagation

For a single-hop (v1 binary) Joinset, the composed surface's effective cardinality is whatever the single hop produces:


| Walked cardinality            | Composed surface cardinality             | Fanout              |
| ----------------------------- | ---------------------------------------- | ------------------- |
| `OneToOne`                    | anchor-cardinality preserved             | none                |
| `OneToMany` (anchor → target) | anchor rows fan out by 0..N              | yes                 |
| `ManyToOne` (anchor → target) | anchor-cardinality preserved             | none                |
| `ManyToMany`                  | anchor rows fan out; target rows fan out | yes (bidirectional) |


For N-ary Joinsets (`TD-NESTING-NARY-JOIN`), per-hop walked cardinalities accumulate:

```text
CUMULATIVE_FANOUT(hops, anchor) -> FanoutProfile:
  profile ← FanoutProfile::new_unit()
  running_kind ← anchor
  for hop in hops:
    walked ← effective_cardinality(hop.relationship, hop.direction)
    profile ← profile.compose(walked)
    running_kind ← hop.to
  return profile
```

`profile.compose(walked)` is the fanout-accumulation function (defined in `25` / `17` — parallel drafts). Binary v1 needs only the single-hop reading; the composition rule is fixed forward-compatibly.

#### 5.4.1 Multi-fanout advisory (N-ary forward-looking; v1-degenerate)

If more than one hop in the Joinset introduces fanout (accumulated `profile` has `> 1` fan-out edges), `JoinsetStrategy` emits `PLAN_W_2403 JoinsetMultiFanoutAdvisory`. In v1 binary this can only fire when a single `ManyToMany` hop is present (counted as two fan-out edges under `profile.compose`'s accounting). N-ary lift extends the advisory to cross-hop multi-fanout without a surface change.

### 5.5 Post-join projection

After the last `PlanNode::Join`, `JoinsetStrategy` emits a `PlanNode::Project` that:

1. Renames per-member columns to their `UnifiedName` (`16 §6.2`): bare name for single-contributor fields, namespaced (`orders.total`) for shape-incompatible collisions.
2. Applies `FieldProvenance` (`16 §7`) per column: `Native` fields select from the single contributor's projected columns; `Shared` fields (uncommon in a Joinset but possible on a join-key column, per `16 §6.3`'s `customer_id` example) are emitted once; `Derived` Joinset-level fields are computed post-join.
3. Applies `CompositionCoverage` (`16 §8`): fields with coverage variant `NullFill` on any member emit `NULL AS {name}` on that member's projection branch (this is primarily a Unionset concern; for Joinset the NULL-fill is carried by the `JoinType`'s outer-join NULL behavior rather than by `FieldOwnership::NullFill` per `16 §7.3.3`).
4. Applies any Joinset-level Filter declarations (`§2.6`) as a `PlanNode::Filter` atop the Project.

The final plan-tree shape for a binary Joinset with anchor `A`, member `B`:

```text
[Filter(joinset.filters)?]
  └── Project(unified surface)
        └── Join { anchor-left, per-§5.1–§5.3 }
              ├── Scan(A) with per-member pushdown
              └── Scan(B) with per-member pushdown
```

Exact pushdown semantics (predicate pushdown, projection pruning) are `19 §3` / `25` / `34`'s concern.

## 6. Interaction with `Relationship`

`Joinset` **consumes** top-level `Relationship`s declared in the Model's `relationships:` block (per `16 §2.1`, YAML surface per `32`) and MAY declare **scope-local `Relationship`s** in its own `relationships:` block that shadow root-level entries within the Joinset's scope only (per `18 §2.10`). `Joinset` does NOT:

- Modify root-level Relationships. A root-level Relationship's `cardinality`, `integrity`, `optional`, `cross_filter`, `keys` remain authoritative for implicit composition (`16 §11`) and for every other Joinset that does not shadow it. A scope-local shadow affects only the Joinset that declares it; the root-level entry on the SemanticManifest's `ResolvedRelationship` is unchanged.
- Participate in root-level Relationship-graph validation. `16 §12`'s well-formedness (duplicate detection, key-type agreement, self-reference, etc.) runs on the root-level `relationships:` block independently of any `Joinset` that may consume the resulting edges. Scope-local Relationships run the same structural rules (`18 §11`, SR-E-4 / SR-E-13 / SR-E-14) within the Joinset's scope.

The reverse direction holds: `Relationship`s are unaware of which `Joinset`s consume them. A Relationship with no consumers (no implicit-composition walks, no `Joinset.path` references) is a perfectly valid SemanticManifest entry; authors routinely declare Relationships defensively for future use.

### 6.1 `Joinset`-side references to Relationships


| Reference                                           | Purpose                            | Validation stage                                |
| --------------------------------------------------- | ---------------------------------- | ----------------------------------------------- |
| `ExplicitPath.hops[i].relationship: RelationshipId` | Author-pinned path citation.       | compile (`COMP_E_2404`–`COMP_E_2410`).          |
| Implicit-path resolution (`§4.1.2 step 4`)          | BFS over the Relationship graph.   | compile (`COMP_E_2401`–`COMP_E_2402`).          |
| `JoinsetStrategy`'s `effective_join_type` (§5.3)    | Per-hop `JoinType` derivation from resolved Relationship's `optional`. | compile + plan; scope-local Relationship shadowing resolved at compile. |
| `JoinsetStrategy`'s key-pair emission (§5.2 step 3) | Plan-node `KeyPair` carriage.      | plan (lookup only; no validation).              |


## 7. Interaction with `TemporalShape`

`17_temporal_shape.md` is a parallel draft. `24` fixes the integration points so `17`'s ratification lands as mechanical uptake.

### 7.1 What `17` will ratify (forward-ref)

Per `16 §4.4.2` and `00 §4.1`'s `TemporalShape` row:

- `TemporalShape` is a per-`DataKind` classification: `Timeseries`, `Events`, `Snapshot`, `Scd`, …
- `JoinType::AsOf` is the temporal-proximity join variant that, for each row on the anchor side, picks the most-recent row on the target side satisfying an as-of predicate (typically a `valid_from` / `valid_to` range).
- `AsOf` activation is gated on the pair of endpoint TemporalShapes. The activation matrix — which endpoint-shape pairs permit `AsOf` — lives in `17 §5`.

### 7.2 `Joinset`'s contract re `AsOf` (forward-ref, v1 descoped)

`JoinType::AsOf` is descoped for v1; the rules below are forward-ref scaffolding for the post-v1 MINOR that lands `AsOf` after `17` ratifies. The integration points are:

1. **Implicit activation.** If a Joinset hop walks between two DataKinds whose `TemporalShape` pair **mandates** as-of joining (per `17 §5`'s matrix; canonical case: `Events ↔ Snapshot` and `Events ↔ Scd`), the hop's effective `JoinType` is `AsOf` regardless of the canonical derivation from `Relationship.optional`. `JoinsetStrategy` activates silently and emits `PLAN_W_2404 JoinsetAsOfActivation` as an informational advisory so the author sees the activation.
2. **Missing temporal key.** `JoinType::AsOf` requires the `Relationship.keys` to include a time-typed `KeyPair` (`16 §4.4.2`). If absent, `COMP_E_2414 JoinsetAsOfMissingTemporalKey`. This is a compile-stage check; `17 §5` ratifies the temporal-key schema.

Author opt-out and shape-mismatch handling for `AsOf` are folded into the scope-local-Relationship model: a Joinset that needs to *prevent* `AsOf` activation declares a scope-local Relationship that diverges in shape (e.g. drops the temporal `KeyPair`), and the temporal pair no longer "mandates" `AsOf`.

### 7.3 Activation matrix skeleton (to be filled by `17 §5`)

`17 §5` will fill in the precise matrix; `24`'s integration point is the two buckets:

- **Mandated `AsOf`** (bullet 7.2.1): the derived `JoinType` is overridden to `AsOf` at plan emission. Advisory emitted.
- **Permitted `AsOf` but not mandated**: the canonical derivation from `optional` wins; no special handling.

When `17` ratifies, this section is updated to cite `17 §5.X` tables directly.

## 8. Interaction with `ComposedSemanticInterface`

A `Joinset`'s output surface is a `ComposedSemanticInterface` with `composition_kind == CompositionKind::Joinset`. This section specializes `16 §§5–8` for the Joinset case; canonical definitions remain in `16`.

### 8.1 `constituents` and ordering

- `constituents` contains the `members` of the Joinset, with the **anchor canonicalized to position 0** followed by the order hops reach the other members. In binary v1: `constituents = [anchor, hops[0].to]`.
- This ordering is stable per I4 and is used by `FieldProvenance` and `CompositionCoverage` to refer to members positionally (though the maps are keyed by `DataKindRef`, not by index).
- The anchor-first ordering is contract: planner code that relies on "constituent 0 is the FROM-clause member" is correct for Joinsets.

### 8.2 `UnifiedSemantics` merge rules

Per `16 §6`, `UnifiedSemantics` merges constituent `SemanticInterface`s into a single queryable surface. For Joinsets:


| Case                                                                       | Shared-compatible?    | `UnifiedSemantics` treatment                                                                                                                                                                                                             |
| -------------------------------------------------------------------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A dimension appears on exactly one member.                                 | Trivially.            | Bare name on the unified surface; `FieldOwnership::Native(owning_member)`.                                                                                                                                                               |
| A dimension appears on multiple members, shape-compatible per `16 §6.2.3`. | Yes.                  | Shared; bare name; `FieldOwnership::Shared(members)`. Typical for the **join key column** (`customer_id` when joining Orders ↔ Customers), which is the same `SemanticsName` on both sides by construction (§16 `KeyPair.left / right`). |
| A dimension appears on multiple members, shape-**in**compatible.           | No.                   | Namespaced (`orders.customer_id`, `returns.customer_id`); bare name is ambiguous.                                                                                                                                                        |
| A measure appears on multiple members with incompatible aggregation.       | No (per `16 §6.2.4`). | Namespaced; planner emits `PLAN_E_0505 AmbiguousCompositionReference` if the Request uses the bare form.                                                                                                                                 |
| The Joinset declares its own derived dimension / measure / metric.         | n/a                   | `FieldOwnership::Derived(physical_expr)` on the unified surface. Contributes to `UnifiedSemantics` alongside per-member contributions.                                                                                                   |


The Joinset's own declared keys (§2.6) appear in `UnifiedSemantics.keys`; absent explicit declaration, the unified surface has no composed-level keys and the anchor's keys remain per-constituent.

### 8.3 `FieldProvenance`

Per `16 §7`, `FieldProvenance` records per-field ownership on the composed surface:

- `**Native(DataKindRef)`** — the field exists on exactly one member. Typical for dimensions / measures specific to a single side.
- `**Shared(Vec<DataKindRef>)**` — the field exists with compatible shape on multiple members; typical for the join-key column.
- `**NullFill(Vec<DataKindRef>)**` — per `16 §7.3.3`, this variant is produced ONLY for `CompositionKind::Unionset`. For `CompositionKind::Joinset`, missing-side fields under an outer join (`Left`, `Right`, `Full`) are carried by the JoinType's NULL-fill in SQL emission, NOT by `FieldOwnership::NullFill`. A Joinset's `FieldProvenance` therefore never contains `NullFill` entries.
- `**Derived(PhysicalExpr)**` — Joinset-level computed fields per §2.6.

### 8.4 `CompositionCoverage`

Per `16 §8.4`, `CompositionCoverage.entries` is the per-`(constituent, unified-name)` fold of binding-level `Coverage`. For Joinsets, the fold is:

```text
JOINSET_COVERAGE_FOLD(joinset) -> CompositionCoverage:
  entries ← {}
  for member in joinset.constituents:
    for unified_name in joinset.interface.interface.all_names():
      entries[(member, unified_name)] ←
        fold_binding_coverage(member, unified_name)
          // returns Native, Derived, NullFill, or Metadata per `16 §8.4`
  return CompositionCoverage { entries }
```

Consumers (planner advisories, SQL adapters) read this map to know, for each `(member, name)` pair, whether the member natively provides the column, computes it, or NULL-fills it.

### 8.5 `traversed_paths`

`16 §5.2`'s `ComposedSemanticInterface.traversed_paths: Vec<RelationshipPath>` is populated for Joinsets with a single-element `Vec<RelationshipPath>` containing the single hop's `[RelationshipId]` (v1 binary). For N-ary Joinsets, multiple paths may be present (one per leg of the spanning tree). The field is diagnostic — it lets `34` / `35` print the actual traversed edges in plan explanations without re-resolving.

### 8.6 Identity and equality (specialization of `16 §5.4`)

Two `ResolvedJoinset`s are equal iff:

- Their `name` match.
- Their `anchor` match.
- Their `constituents` sequences match (order-significant).
- Their `hops` sequences match (order-significant; each hop's `(relationship, direction, to)` triple).
- Their scope-local `relationships` rosters match (per-name canonical-form equality of every shadow entry; root-level resolution at each hop yields identical Relationships).
- Their `ComposedSemanticInterface.interface` match (`UnifiedSemantics` roster).

Per I4 the resolution pipeline is deterministic; two compiles of the same Model produce identical `ResolvedJoinset`s.

## 9. Validation Preconditions

`VALID_E_2400`–`2499` are reserved for validate-stage Joinset errors. Validate runs structural checks that do not require Semantics type resolution (per `10 §3.3`).

### 9.1 The roster


| Code           | Variant                                                       | Condition                                                                                                                                                                                                           |
| -------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `VALID_E_2400` | `JoinsetArityV1Violation { joinset, member_count }`           | `members.len() != 2` in v1. Also surfaced by `12 §7 NV-V5`; `24` provides the canonical-layer twin.                                                                                                                 |
| `VALID_E_2401` | `JoinsetAnchorMissing { joinset }`                            | `anchor` is absent (canonical-layer only; YAML surface forces the field present per `32`, but programmatic construction paths need the guard).                                                                      |
| `VALID_E_2402` | `JoinsetAnchorNotMember { joinset, anchor, members }`         | `anchor` is not in `members`.                                                                                                                                                                                       |
| `VALID_E_2403` | `JoinsetExplicitPathEmpty { joinset }`                        | `path == Some(ExplicitPath { hops: vec![] })`. An empty explicit path is not an implicit-fallback (§4.3).                                                                                                           |
| `VALID_E_2404` | `JoinsetMembersEmpty { joinset }`                             | `members` is empty (no anchor, no other member). Degenerate; covered-by-2401 in practice but kept as a distinct code for clarity when authoring tools surface the error.                                            |
| `VALID_E_2405` | `JoinsetExplicitPathCyclic { joinset, revisited_member }`     | Explicit path visits the same member twice. Binary v1 cannot trigger; N-ary-forward-looking.                                                                                                                        |
| `VALID_E_2406` | `JoinsetDuplicateMember { joinset, duplicated }`              | `members` contains the same `DataKindRef` twice.                                                                                                                                                                    |


### 9.2 Severity and stage

All `VALID_E_24xx` are `Severity::Error`; they fail validate. Per `30 §7`, validate-stage errors block the pipeline from proceeding to compile.

### 9.3 Interaction with `11` and `12`

Several `24` validate errors share territory with `11` / `12`:

- `VALID_E_2400 JoinsetArityV1Violation` ≡ `12 §7 NV-V5` nesting-shape check on the YAML side.
- `VALID_E_2402 JoinsetAnchorNotMember` is a canonical-layer-only check (YAML authoring does not have an anchor-is-member distinction; the anchor IS a member by YAML structure).
**Retired (2026-05-12).** `VALID_E_2407 JoinsetOverridesForNonexistentHop` is retired — the override surface (`overrides.per_hop` / `HopPosition`) no longer exists on `JoinsetDataKind` (`§2.2`). Code reserved for forward-compat.

Double-emission (a single programming-level error triggering both a `12` and a `24` diagnostic) is avoided at the dispatch level in `34`; when both would fire, the more-specific `24` variant is preferred.

## 10. Compile Preconditions

`COMP_E_2400`–`2499` are reserved for compile-stage Joinset errors. Compile runs type-sensitive and graph-walking checks (per `10 §3.4`).

### 10.1 The roster


| Code          | Variant                                                                                        | Condition                                                                                                                                                                                                                                                         |
| ------------- | ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `COMP_E_2401` | `JoinsetImplicitNoPath { joinset, anchor, target }`                                            | Implicit-path BFS found no Relationship path from `anchor` to the other member (§4.1.2 step 5). Author must declare the Relationship or an explicit path.                                                                                                         |
| `COMP_E_2402` | `JoinsetImplicitAmbiguousPaths { joinset, anchor, target, candidates: Vec<RelationshipPath> }` | Implicit-path BFS found multiple equal-length shortest paths (§4.1.2 step 6). Author must pin via explicit path.                                                                                                                                                  |
| `COMP_E_2403` | `JoinsetAnchorUnreachable { joinset, unreachable_member }`                                     | Resolved path does not connect the anchor to one of the members. v1 binary-degenerate (the single hop either connects the two or fails at `COMP_E_2401`); N-ary-forward-looking.                                                                                  |
| `COMP_E_2404` | `JoinsetExplicitPathUnknownRelationship { joinset, position, relationship }`                   | An explicit `JoinHop.relationship` does not resolve to a SemanticManifest `RelationshipId`.                                                                                                                                                                       |
| `COMP_E_2405` | `JoinsetExplicitPathAnchorMismatch { joinset, expected_anchor, actual_source }`                | Hop 0's walked source (given `direction`) is not the Joinset's declared anchor.                                                                                                                                                                                   |
| `COMP_E_2406` | `JoinsetExplicitPathEndpointMismatch { joinset, position, declared_to, computed_to }`          | `hop.to` does not match the Relationship's walked target endpoint (given `direction`).                                                                                                                                                                            |
| `COMP_E_2407` | `JoinsetExplicitPathDiscontinuity { joinset, position }`                                       | Hop `i+1`'s walked source != hop `i`'s `to`. v1 binary-degenerate; N-ary-forward-looking.                                                                                                                                                                         |
| `COMP_E_2408` | `JoinsetArityV1Invariant { joinset, hops_count }`                                              | Resolved hops count `!= 1`. Defense-in-depth guard against validate failing to catch an N-ary input.                                                                                                                                                              |
| `COMP_E_2410` | `JoinsetExplicitPathUncoveredMembers { joinset, uncovered: Vec<DataKindRef> }`                 | The explicit path's endpoints do not cover every `member`. Binary v1 cannot trigger; N-ary-forward-looking.                                                                                                                                                       |
| `COMP_E_2414` | `JoinsetAsOfMissingTemporalKey { joinset, position, relationship }`                            | A hop's effective `JoinType == AsOf` but the underlying `Relationship.keys` contains no time-typed `KeyPair` (§7.2 bullet 2). Forward-ref only — v1 does not emit `AsOf`.                                                                                         |
| `COMP_E_2415` | `JoinsetMemberNotTopLevel { joinset, member }`                                                 | A `members` entry refers to a `DataKindRef` that is not a top-level DataKind in the Model. (Nested-`Simple` binding-only leaves are not queryable on their own, per `12 §6.2`.)                                                                                   |


### 10.2 Severity and stage

All `COMP_E_24xx` are `Severity::Error`; they fail compile. Per `30 §7`, compile-stage errors block `plan`.

### 10.3 Ordering

Path-resolution errors (`COMP_E_2401`–`COMP_E_2410`) fire before any temporal-shape-gated errors (`COMP_E_2414`): the hops must resolve before their effective `JoinType`s can be evaluated. `COMP_E_2415` fires before path resolution (member-resolution is a prerequisite).

**Retired (2026-05-12).** `COMP_E_2409 JoinsetReverseForwardOnlyRelationship` is retired — every Relationship is bidirectional per `16 §2.4`; reverse traversal is always legal. `COMP_E_2411 JoinsetIllegalJoinTypeOverride`, `COMP_E_2412 JoinsetAsOfDowngradeForbidden`, and `COMP_E_2413 JoinsetAsOfShapeMismatch` are retired with the per-hop override surface removal (`§2.2`); divergent join semantics surface through structural rules on the scope-local `Relationship` itself (SR-E-4 / SR-E-13 / SR-E-14 in `18 §11`). All four codes are reserved for forward-compat.

## 11. Plan-Stage Rules

`PLAN_E_2400`–`2499` and `PLAN_W_2400`–`2499` are reserved for plan-stage Joinset usage. Plan-stage errors surface when a `Request` interacts with an already-resolved Joinset in an illegal way.

### 11.1 `PlannerError` additions


| Code          | Variant                                                                  | Condition                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PLAN_E_2400` | `JoinsetRequestOutOfSurface { joinset, name }`                           | A Request with `from: Some(joinset)` names a `Semantics` not on the Joinset's `ComposedSemanticInterface`. Specialization of `16 §14.3 PLAN_E_0506 RequestOutOfSurface`; the specialized code carries the Joinset's name for a more actionable diagnostic. The planner MAY emit `PLAN_E_0506` OR `PLAN_E_2400`; `24`'s convention is to prefer `PLAN_E_2400` when the surface is a `CompositionKind::Joinset`. |
| `PLAN_E_2401` | `JoinsetAmbiguousReferenceOnSurface { joinset, name, candidates }`       | A bare `SemanticsName` on the Joinset's composed surface is ambiguous per `16 §6.2.3`'s namespacing rule. Specialization of `16 §14.3 PLAN_E_0505`. Same dispatch convention as `PLAN_E_2400`.                                                                                                                                                                                                                 |
| `PLAN_E_2402` | `JoinsetNonAdditiveRollupRequired { joinset, measure, hop_cardinality }` | A non-additive measure requires a fanout-safe rewrite over the Joinset's fanout shape, but the Request's shape (or `17`'s `TemporalShape × Additivity` matrix) forbids the rewrite. Joinset-specialized variant of `16 §14.3 PLAN_E_0506 CompositionAggregationConflict`.                                                                                                                                      |


### 11.2 `PlannerError` advisories


| Code          | Variant                                                                      | Condition                                                                                                                                                                                                                                                                                                                                                |
| ------------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PLAN_W_2400` | `JoinsetFanoutAdvisory { joinset, position, cardinality }`                   | A hop introduces fanout; `JoinsetStrategy` inserted a fanout-safe rewrite. Joinset-specialized intent advisory; the cross-DataKind parent (`PLAN_W_0501 FanoutAdvisory`) was retired in `16 §14.4` (2026-04-29) per Q-COMP-005's intent-advisory deferral. Retention vs retirement of the Joinset-specialized variant is `[TD-JOINSET-FANOUT-ADVISORY]`. |
| `PLAN_W_2402` | `JoinsetManyToManyHopAdvisory { joinset, position, relationship }`           | A hop's underlying `Relationship.cardinality == ManyToMany`; consider junction-table modeling (`16 §3.3.4`). Joinset-specialized intent advisory; the cross-DataKind parent (`PLAN_W_0502 ManyToManyFanoutAdvisory`) was retired in `16 §14.4` (2026-04-29). Retention covered by `[TD-JOINSET-FANOUT-ADVISORY]`.                                        |
| `PLAN_W_2403` | `JoinsetMultiFanoutAdvisory { joinset, profile }`                            | The Joinset has more than one fan-out edge (v1 binary: a single `ManyToMany` hop; N-ary: cross-hop accumulation). `JoinsetStrategy` proceeds but the author should consider restructuring.                                                                                                                                                               |
| `PLAN_W_2404` | `JoinsetAsOfActivation { joinset, position, declared, activated }`           | `17 §5`'s matrix mandated `JoinType::AsOf` on a hop whose declared `JoinType` was non-`AsOf`. Informational so the author sees the activation.                                                                                                                                                                                                           |


### 11.3 Severity

All `PLAN_E_24xx` are `Severity::Error`; they fail plan. All `PLAN_W_24xx` are `Severity::Warning`; plan proceeds with the diagnostic emitted.

**Retired (2026-05-12).** `PLAN_W_2401 JoinsetJoinTypeOverrideAdvisory` is retired with the override-surface removal (`§2.2` / `§5.3.2`). Divergent join semantics are now declared structurally via a scope-local `Relationship` and do not warrant a per-resolution advisory. Code reserved for forward-compat.

### 11.4 Dispatch convention

`16 §14`'s composition errors (`PLAN_E_05xx`) are the generic, composition-kind-agnostic entries. `24`'s `PLAN_E_24xx` / `PLAN_W_24xx` are the Joinset-specialized entries. The planner's convention:

- If the error is clearly Joinset-specific (e.g., `PLAN_W_2403 JoinsetMultiFanoutAdvisory`), emit the `24` code.
- If the error is a generic composition concern that happens to apply to a Joinset (e.g., a fanout advisory on an implicit composition that coincidentally covers the Joinset's members — see `16 §13.5`), emit the `16` code.
- Dispatch happens in `34`; `24` fixes the taxonomy without hard-coding dispatch rules.

## 14. Cross-References

- `00 §4.1` — `Joinset` row (one of the four `CompositionKind` variants); `00 §4.2` — canonical vocabulary; `00 §9` — invariants I1, I4, I5, I7, I8, I10, I12.
- `10 §3.3, §3.4` — `validate` and `compile` stage contracts; `24` respects both without reopening.
- `11 §2, §3, §6.5, §9, §10.3` — scope chain, global identity, `Constraint::Key`, cross-kind references, structural labels used by `12 §5.1`'s YAML path shape.
- `12 §2, §5` — nesting matrix (Joinset ⊄ Joinset, member kinds); binary-v1 arity; YAML-level path shape (`path.on.left` / `path.on.right`).
- `13 §5` — `DataType` compatibility, consumed by `KeyPair` type-agreement checks `16 §12.2`.
- `14 §N` — expression grammar used by Joinset-level derived dimensions / filters; `19 §3.4.5` — `PathSignature` subsumption interaction (§11's `PLAN_E_0509` remains the canonical code for Request-path / expression-path mismatches, even on Joinset surfaces).
- `15 §4, §6` — `Binding`, `ColumnMapping`, `Coverage`; consumed when `JoinsetStrategy` resolves per-hop `KeyPair`s to physical columns.
- `16 §§2–14` — `Relationship`, `Cardinality`, `JoinType`, `ComposedSemanticInterface`, `UnifiedSemantics`, `FieldProvenance`, `CompositionCoverage`, implicit composition algorithm, composition error codes. `24` cites; `24` does NOT redefine any of `16`'s canonical types.
- `17 §5` (pending) — `TemporalShape` activation matrix for `JoinType::AsOf`.
- `20_taxonomy.md` (parallel draft) — data-kinds taxonomy.
- `23_unionset.md` (parallel draft) — `CompositionKind::Unionset` peer.
- `22_grainset.md` (parallel draft) — `CompositionKind::Grainset` peer.
- `23_dataset.md` (parallel draft, numbering per current plan) — `Simple` / Dataset leaf.
- `25` (pending) — cross-kind strategy catalog (Joinset × Grainset, Joinset × Unionset interactions).
- `30 §4, §5, §6` — `#[non_exhaustive]` policy, `Diagnostic` shape, error-code range governance; `24` allocates `VALID_E_2400`–`2499`, `COMP_E_2400`–`2499`, `PLAN_E_2400`–`2499`, `PLAN_W_2400`–`2499` under `30 §6.2`'s per-DataKind-doc scheme.
- `32` (pending) — YAML surface for the `joinsets:` top-level block, `path:` sub-block, and Joinset-scoped `relationships:` sub-block (scope-local Relationship shadow shape).
- `33` (pending) — SemanticManifest surface: `ResolvedJoinset`, `ResolvedJoinHop`.
- `34` (pending) — planner surface: `JoinsetStrategy` impl, dispatch rules between `16` generic codes and `24` specialized codes.
- `35` (pending) — `PlanNode::Join` with `from_relationship` and `from_joinset` tagging fields.
- `questions/closed/24_questions.md` — Round-1 ratified Q-24-02..Q-24-08; `questions/deferred/24_questions.md` — Q-24-01 / Q-24-09 / Q-24-10 (deferred for post-v1).
- `questions/open/16_questions.md` — composition-level deferrals that touch Joinset (`[TD-COMPOSITION-JOINSET-REUSE]`, `[TD-COMPOSITION-SELFJOIN]`, `[TD-COMPOSITION-ASOF]`).
- Legacy: `docs/JOINSET.md` — early reference; superseded by this document and `16 §13`.

