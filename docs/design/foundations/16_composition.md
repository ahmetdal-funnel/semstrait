---
prereqs: [00, 10, 11, 12, 13, 14, 14a, 14b, 15, 18]
authoritative-for:
  - `Relationship` **composition semantics** — placement (global top-level), scope visibility, traversal rules, per-variant fanout analysis (struct shape owned by `18 §2`)
  - `ComposedSemanticInterface` — the unified queryable surface presented to the planner
  - `CompositionKind` — discriminator for the four flavours of composed surface
  - `UnifiedSemantics` — namespace-aware merge of constituent `SemanticInterface`s
  - `FieldProvenance` / `FieldOwnership` — per-field ownership on a composed surface
  - `CompositionCoverage` — extends `15 §6`'s `Coverage` to the composition level
  - `RelationshipPath` — the composition-level chain of `RelationshipId` traversals
  - explicit vs implicit composition — the boundary, the authoring contract
  - materialization policy — what lives in the Manifest vs what the planner synthesizes
  - field-first resolution — the planner algorithm when `Request.from` is `None`
  - `Relationship` graph well-formedness preconditions (validate / compile stage)
  - new `CompileError` / `ValidateError` / `PlannerError` variants for composition
refined-by:
  - 17 (TemporalShape × Additivity × fanout-safe planning; ratifies `AsOf` join vocabulary)
  - 20 (`data-kinds/20_taxonomy.md` — shared DataKind invariants; strategies consume field-first resolution)
  - 21 (`data-kinds/21_dataset.md` — Simple / Dataset kind as trivial leaf constituent under composition)
  - 22 (`data-kinds/22_grainset.md` — Grainset grain-aware partial coverage under composition)
  - 23 (`data-kinds/23_unionset.md` — Unionset vertical composition; orthogonal to Relationship walks)
  - 24 (`data-kinds/24_joinset.md` — Joinset author-named Relationship-driven composition; anchor + path)
  - 25 (`data-kinds/25_applicability_matrix.md` — per-variant cross-cut for composition rules)
  - 32 (`apis/32_semstrait_model.md` — YAML surface for `Relationship` / `Joinset`)
  - 33 (`apis/33_semstrait_manifest.md` — persists `Relationship`, `Joinset`, `ComposedSemanticInterface`)
  - 34 (`apis/34_semstrait_planner.md` — planner consumes the field-first algorithm as its entry point)
  - 35 (`apis/35_semstrait_ir.md` — `PlanNode::Join` carriage of `JoinType` / `Cardinality`)
---

# 16. Composition

> **Struct ownership (2026-04-17 consolidation).** The `Relationship` struct, `RelationshipId` newtype, `JoinType`, `Cardinality`, `Directionality`, and `JoinKeyExprPair` are ratified in [`18_entities.md §2`](./18_entities.md#2-relationship). This doc owns the *composition semantics* on top — placement, scope, traversal, fanout analysis, `ComposedSemanticInterface` construction, field-first resolution. The struct-shape lands in `18`; what the planner does with it lands here. Where body sections below cite `KeyPair`, read `JoinKeyExprPair` (`18 §2.6`); where they cite `ColumnMapping`, read `SemanticMapping` (`18 §10`).
>
> This document ratifies how multiple `DataKind`s appear as a **single queryable
> surface**: the `Relationship` edge-type that binds top-level `DataKind`s,
> the `ComposedSemanticInterface` the planner works against, the
> explicit-vs-implicit composition boundary, and the field-first resolution
> algorithm the planner runs when a `Request` omits a `from:` clause.
>
> **Three open items from `00 §4.1` land here (in the `ComposedSemanticInterface`
> row).** `16` closes them:
>
> - **(i)** Structural shape of `ComposedSemanticInterface` vs bare
>   `SemanticInterface` — **ratified:** distinct type, with a shared
>   `SemanticsView` trait for the accessors both expose (`§5.4`, `§16 Q1`).
> - **(ii)** Whether composed interfaces are materialized in the Manifest or
>   synthesized by the planner on demand — **ratified:** explicit
>   compositions (`Unionset` / `Grainset` / `Joinset`) are **materialized**;
>   implicit `Relationship`-driven compositions are **synthesized on-demand**
>   (`§10`, `§16 Q2`).
> - **(iii)** Scope of implicit `Relationship`-driven composition vs required
>   explicit declaration — **ratified:** implicit composition is bounded to
>   chains of **declared** `Relationship`s, **unambiguous shortest-path** only,
>   **depth-limited** to `MAX_IMPLICIT_COMPOSITION_DEPTH` hops (`§9.1`, `§16 Q3`).
>
> **Status (Round 1 ratified).** All 17 framework decisions settled per `§16`'s
> Ratified Decisions Index. Open implementation choices (depth bound value,
> tie-breaker heuristics, solver sophistication) parked in
> `open_questions/16_open_questions.md`.

## 1. Purpose and Scope

`semstrait` composes data along two axes. Vertically, an author groups
equivalent-shape `DataKind`s into a `Unionset` (append) or a `Grainset`
(grain-sharded). Horizontally, an author declares `Relationship`s between
`DataKind`s with complementary `Semantics`, and either (a) names the
traversal as a `Joinset` or (b) leaves the planner to walk the graph
implicitly when a `Request`'s selected `Semantics` span multiple kinds.

`16` is the authoritative specification for the horizontal axis's **core
type machinery** (`Relationship`, `Cardinality`, `JoinType`,
`Directionality`, `ComposedSemanticInterface`, `UnifiedSemantics`,
`FieldProvenance`, `CompositionCoverage`), for the **boundary** between
explicit and implicit composition, and for the **field-first resolution
algorithm** the planner uses to synthesize an implicit composition at plan
time. Per-`DataKind` materialization strategies (`Unionset`, `Grainset`,
`Joinset` bodies), the YAML authoring surface, and the Manifest / Planner
IR that carry the ratified shapes are refined in the `refined-by` docs.

### 1.1 What `16` ratifies (index)

`16` ratifies: the `Relationship` struct + `KeyPair` + `Directionality`
(§2); `Cardinality` (§3); `JoinType` + `PlanNode::Join` carriage (§4);
`ComposedSemanticInterface` + `CompositionKind` + `SemanticsView` trait
(§5, **resolves (i)**); `UnifiedSemantics` merge logic (§6);
`FieldProvenance` + `FieldOwnership` (§7); `CompositionCoverage` extending
`15 §6` (§8); the explicit-vs-implicit composition boundary (§9,
**resolves (iii)**); the materialization policy (§10, **resolves (ii)**);
the field-first resolution algorithm (§11); `Relationship` graph
well-formedness preconditions (§12); `Joinset`'s role as named-subset
narrowing of implicit composition (§13); and new `CompileError` /
`ValidateError` / `PlannerError` variants with stable codes in the
`COMP_E_04xx` / `PLAN_E_05xx` / `PLAN_W_05xx` ranges (§14).

### 1.2 What `16` does NOT ratify

- **Per-`DataKind` planning strategies** — Scan / Join / Aggregate /
  Project lowering lives in `20`–`25` and `34`; `16` ratifies the
  type-of-surface the strategies plan against.
- **YAML authoring syntax** — `relationships:`, `joinsets:`,
  `directionality:` block shapes and defaults live in `32`.
- **`Manifest` serialization** — on-disk shape of
  `ResolvedRelationship`, `ResolvedComplexDataKind`, and the
  `RelationshipGraph` index lives in `33`.
- **Fanout-safe-rewrite algorithm shape** — the rewrite's plan-tree
  transformation lives in `20` / `34`; `16` ratifies the
  `Cardinality × JoinType` matrix that triggers it.
- **`JoinType::AsOf`** — deferred; gated on `17 TemporalShape` (§4.3).
- **Physical predicate lowering** — `34` / `36` for adapter rendering.
- **Diagnostic rendering** — `30 §5` owns the Diagnostic shape; `16`
  ratifies the stable codes and structured payloads.

### 1.3 Design posture

Four stances govern:

1. **Name, not column.** `Relationship.keys` pair `SemanticsName`s; per
   I1 physical resolution is `15`'s responsibility (§2.3).
2. **Declare the edges, let the planner pick the walk.** Authors
   declare pairwise `Relationship`s; the planner walks. Authors who
   want to pin a walk declare a `Joinset` (§9, §11).
3. **Materialize what's named; synthesize what's implicit.** Named
   `ComplexDataKind`s earn Manifest residence; anonymous walks pay a
   cheap per-Request synthesis cost (§10).
4. **Fail fast, disambiguate up.** Ambiguous implicit paths error;
   authors disambiguate by naming a `Joinset` (I4 determinism; §9.1,
   §14.3).

### 1.4 Guardrails upheld

- **I1 (canonical layer).** `Relationship.keys` are `SemanticsName`s;
  physical resolution is `15`'s responsibility.
- **I4 (determinism).** Ambiguous implicit-composition paths error
  (`PLAN_E_0500`); no heuristic tie-break.
- **I5 (compile-time resolution).** Name indices and the
  `RelationshipGraph` are pre-built at `compile`; the planner's walk
  is lookup, not resolution.
- **I8 (Manifest is planner-complete).** The planner reads indices and
  graph from the Manifest; no catalog fetch, no re-parse.
- **I10 (non-exhaustive public sums).** `Cardinality`, `JoinType`,
  `Directionality`, `CompositionKind`, and `FieldOwnership` all carry
  `#[non_exhaustive]`.
- **I12 (fail-fast).** `CompileError::*` composition variants abort
  `compile`; `PlannerError::*` composition variants abort `plan`.

## 2. The `Relationship`

A `Relationship` is a **pairwise, named connector** between two top-level
`DataKind`s declaring a joinable edge: a pair (or tuple) of `SemanticsName`s
on each side, the cardinality of the join, and the `JoinType` the planner
should use when traversing it. It is the semstrait type-system analogue of
a foreign-key association, lifted to the semantic layer so keys are
`Semantics`, not SQL columns.

> **Struct shape**: the `Relationship` struct itself — including its fields (`name`, `from`, `to`, `join_type`, `keys`, `filter`, `cardinality`, `directionality`, `description`), the companion `RelationshipId` newtype, the `JoinKeyExprPair` hybrid equi-key grammar, and the `JoinType` / `Cardinality` / `Directionality` enums — is defined in [`18_entities.md §2`](./18_entities.md#2-relationship). This doc ratifies the *composition semantics* on top (placement, scope, traversal, fanout analysis). Where the body prose below uses `KeyPair`, read `JoinKeyExprPair` per `18 §2.6`.

### 2.1 Placement — global, top-level

`Relationship`s live as **top-level blocks** in the `SemanticModel`, not
inside any `DataKind`. Per `11 §2` (scope chain), the `Root` scope owns
the `Relationship` list; `Kind` and `Nested-kind` scopes see but do not
declare `Relationship`s. This matters for three reasons:

1. **Symmetric participation.** A `Relationship` between `A` and `B` is a
   property of the pair, not a field on either side. Embedding
   `Relationship`s inside `A` would (falsely) imply `A` owns the edge.
2. **Scope visibility.** Per `11 §2.1`, `Relationship`s are visible at
   `Root` scope; `11 §9`'s cross-kind reference rule grants any `Kind`
   scope the ability to dereference a `SemanticsName` owned by another
   kind **iff** a `Relationship` path exists at `Root`.
3. **Authoring ergonomics.** Star-schema-style Models declare one
   `relationships:` block with N entries; snowflake patterns extend this
   linearly. Authors do not hunt through `DataKind` bodies to find edges.

A `Relationship` between a top-level `DataKind` and a `Nested-kind`
(child of a `Unionset` / `Grainset` / `Joinset`) is rejected at `validate`:
nested kinds do not have `Root` scope identity. Authors who need the
equivalent lift the nested kind to top-level first.

**Permitted constituents.** Both `from` and `to` must resolve to a
top-level `DataKind` — `Simple`, `Unionset`, `Grainset`, or `Joinset`.
A `Relationship` between two composed kinds is permitted (see
`open_questions/16_open_questions.md#Q-COMP-013`); its `KeyPair.left` or
`.right` may reference a namespaced name within the composed surface (e.g.
`"order_details.customer_id"`).

### 2.2 Structure

Fields:

- `id: RelationshipId` — assigned at `compile` in declared-iteration order,
  `u32` shape, Manifest-wide unique (`14b §4.2` owns the assignment). The
  ID is internal to one Manifest; not stable across recompiles (see
  `14b open_questions OQ-7`).
- `from: DataKindRef`, `to: DataKindRef` — named references to top-level
  `DataKind`s. `DataKindRef` is defined in `11 §4` as a newtype over
  `DataKindName`.
- `keys: Vec<KeyPair>` — non-empty; each entry declares one join-column
  pair. Composite keys (N > 1) are positional: `keys[0]` on the left
  side matches `keys[0]` on the right side, etc. (§2.3; open
  `Q-COMP-009`).
- `cardinality: Cardinality` — one of four variants (§3).
- `join_type: JoinType` — one of four variants (§4). Required at the
  canonical layer; YAML surface (`32`) MAY default (open `Q-COMP-017`).
- `directionality: Directionality` — governs traversal (§2.4).

**Conventional orientation.** `from` is the "owning" or "driving" side
and `to` is the "referenced" side, mirroring foreign-key narrative. For
a `ManyToOne` relationship (e.g. `orders → customers`), `from = orders`
and `to = customers`. Semantics do not depend on the orientation — the
planner walks the edge in either direction subject to `directionality` —
but the convention aids readability and drives `Cardinality`'s
per-variant naming (`ManyToOne` reads naturally as `from → to`).

**Stable identity.** `RelationshipId` is the primary key every downstream
layer uses: `14b §4.5`'s `PathSignature` carries `Vec<RelationshipId>`,
`PlanNode::Join` (per `35`) carries a `RelationshipId` as metadata, and
diagnostics reference `relationship_id` for precise blame. Names
(`from.to` style) are not used as identity — two `Relationship`s between
the same `DataKindRef` pair differ by `id` even if they declare the same
`from` / `to` / `keys` (but will be rejected by `§12.1`'s duplicate
check).

### 2.3 `KeyPair`

```rust
#[non_exhaustive]
pub struct KeyPair {
    pub left: SemanticsName,  // on Relationship.from — a Key or Dimension
    pub right: SemanticsName, // on Relationship.to — a Key or Dimension
}
```

**Name, not column (I1).** Both sides are `SemanticsName`s — the canonical
layer's identity for a semantic element. Physical resolution (what column
does `SemanticsName::new("customer_id")` resolve to on the `orders`
Binding?) is `15 §5`'s `ColumnMapping` job. `16` never looks at columns.

**Shape.** Both sides must resolve at `compile` to either:

- A `Key` declared in the referenced `DataKind`'s interface
  (`Key::Primary`, `Key::Foreign`, or `Key::Unique` per `11 §8.3`), or
- A `Dimension` declared in the interface.

`Measure`, `Metric`, and `Filter` names are rejected — those are aggregates
or guards, not join columns. `CompileError::RelationshipKeyNotJoinable`
(§14.1) fires if a `KeyPair` references one.

**Composite keys — positional pairing.** A `Relationship` with a composite
join condition declares multiple `KeyPair` entries:

```rust
Relationship {
    from: "order_lines", to: "product_variants",
    keys: vec![
        KeyPair { left: "product_id",    right: "product_id" },
        KeyPair { left: "variant_code",  right: "variant"    },
    ],
    // ...
}
```

Ordering is significant only for `PathSignature` canonicalization
(`14b §4.5`); the join condition itself is commutative (`A AND B == B AND A`).
`32`'s YAML surface emits `keys:` as a list of `{left, right}` pairs.
Positional vs single-entry `Vec<SemanticsName>` is tracked as
`Q-COMP-009`; Round 1 ratifies positional pairs.

**Type agreement.** Both sides' inferred `DataType` (per `14b`'s type
inference over the `Binding`'s `ColumnMapping` and any `Declarative`
expression) must be compatible under `13 §4`'s type-compatibility
relation. Numeric sides widen per `13 §5`'s widening rules; string sides
require exact match; temporal sides require equal precision and
nullability compatibility. Mismatch triggers
`CompileError::RelationshipKeyTypeMismatch` (§14.1). The check runs at
`compile`, not `validate`, because inferring the physical type requires
`14b`'s resolution.

**Nullability.** `semstrait` does not propagate NOT NULL as a relationship
precondition. If a `KeyPair.left` resolves to a nullable semantics,
`NULL = NULL` returns `UNKNOWN` per standard SQL and the row is dropped
on `Inner` / non-matched rows are `NULL`-padded on `Left` / `Right`.
Authors who need a NOT-NULL contract add a `Constraint::NotNull` in the
referenced `DataKind`'s interface (per `11 §8.4`); violations surface at
plan time when the planner emits the join.

### 2.4 `Directionality`

Enum defined in [`18 §2.5`](./18_entities.md#25-directionality). v1 variants: `Bidirectional` (default — forward and reverse both walkable) and `Forward` (forward only; reverse traversal errors at plan time).

Governs whether the planner may traverse the `Relationship` in both
directions (§2.4.3) or only the forward direction (`from` → `to`).
Bidirectional is the default; `Forward` is a deliberate restriction.

#### 2.4.1 Variants

- **`Bidirectional`** (default). The planner may use the `Relationship`
  as an edge in either direction. A query that requests `orders.revenue`
  and `customers.name` can walk `orders → customers`; a query that
  requests `customers.name` and the count of related `orders` can walk
  `customers → orders`. The `Cardinality` remains declared as-written —
  a `ManyToOne` walked in reverse is effectively a `OneToMany` for
  fanout-analysis purposes (`§3.3.2`), and the planner flips its view
  accordingly.
- **`Forward`**. The planner may only walk `from` → `to`. A Request that
  would require the reverse direction triggers
  `PlannerError::CrossCompositionForbidden` (§14.3 `PLAN_E_0503`).

Additional variants (`Reverse`, `Neither`) are **not** in v1; if a
genuine reverse-only need arises, the author swaps `from` and `to` in
the declaration (see open `Q-COMP-007`). The enum is `#[non_exhaustive]`
per I10.

#### 2.4.2 `Forward` use-cases

`Forward` is useful when the `Relationship`'s semantics are directional
in a way that does not survive inversion:

- **Event-log → Entity enrichments.** An events table (`page_views`,
  `Many`) joined to a user dimension (`users`, `One`) via
  `ManyToOne` / `Inner`. Walking `page_views → users` enriches events
  with user attributes. Walking `users → page_views` is a very different
  query (fanout of users by their events) that the author may not want
  the planner to synthesize without an explicit `Joinset`.
- **Slow-changing-dimension lookups.** A fact table joined to an SCD-II
  dim via a temporal-valid-range predicate. Forward direction is the
  point-in-time enrichment; reverse direction is a historical-churn
  query that the planner should not synthesize silently.
- **Degenerate-one-sided joins.** `Relationship` declared only to
  enable filter pushdown (e.g. a reference table of valid country
  codes); reverse walk has no analytic meaning.

When in doubt, authors pick `Bidirectional`. `Forward` is an opt-in
restriction.

#### 2.4.3 Symmetric traversal under `Bidirectional`

Under `Bidirectional`, forward and reverse walks share the same
`RelationshipId` — they are the same edge, walked in two directions. The
BFS traversal in `§11.4` normalizes direction at walk time: given a
`current_node` and an unvisited neighbour `target_node`, the step is
flagged `reverse: true` when `current_node == Relationship.to &&
target_node == Relationship.from`, and `reverse: false` otherwise. The
`PathSignature` (`14b §4.5`) records the `RelationshipId` alone; the
direction is reconstructed at plan time by matching `current_node`
against the stored `from` / `to`.

**`Cardinality` under reversal.** A `Relationship { cardinality: ManyToOne,
from: A, to: B }` walked in reverse (`B → A`) is read as `OneToMany`
for fanout-analysis purposes. The stored enum variant does not change;
the planner inverts the interpretation on reverse walks. Analogous for
`OneToMany` ↔ `ManyToOne`. `OneToOne` and `ManyToMany` are
inversion-symmetric — no mental flip needed.

**Bidirectionality and `JoinType`.** `JoinType::Left` walked in reverse
behaves as `JoinType::Right` (fills the reverse side with NULLs).
`JoinType::Right` reversed becomes `Left`. `Inner` and `Full` are
symmetric. The planner does not rewrite the declared `JoinType` — it
substitutes the effective form per-direction at plan emission.

## 3. `Cardinality`

Enum defined in [`18 §2.4`](./18_entities.md#24-cardinality--required-at-every-site). v1 variants: `OneToOne`, `OneToMany`, `ManyToOne`, `ManyToMany`.

Declared on every `Relationship`. `Cardinality` is **planning metadata**,
not a runtime enforcement — `semstrait` does not scan data to verify that
the declared multiplicity holds. Authors assert it; the planner trusts
it and shapes the plan accordingly. A Model that declares a `OneToOne`
where the physical data exhibits `ManyToMany` will produce arithmetically
incorrect aggregations (per-row duplication) without a runtime error.
This is an explicit trade-off: verification would require a scan, which
violates the compile-time-resolution posture (I5).

### 3.1 Enum — `#[non_exhaustive]`

Four variants cover the ratified case matrix. `#[non_exhaustive]`
(I10) reserves room for future variants (`ZeroOrOne` for SCDs,
`OneOrMore` for strict-existence joins) without semver breakage.

### 3.2 `Cardinality` vs key-level uniqueness

`Cardinality` declares the multiplicity of the **join**, not the
uniqueness of any particular side's keys. A `ManyToOne` `Relationship`
does not imply `from.key` is non-unique and `to.key` is unique — those
uniqueness claims are `11 §8.3`'s `Key::Primary` / `Key::Unique`
territory. Inconsistencies between the two (`ManyToOne` from a side with
`Key::Primary` on the left key) emit `PLAN_W_0503`
`RelationshipCardinalityKeyMismatch` advisory (§14.4).

### 3.3 Per-variant semantics

#### 3.3.1 `OneToOne`

Each row in `from` matches at most one row in `to`, and vice versa.

- **Fanout:** none. Measure aggregations over the composed surface
  behave as-if on a single relation.
- **Join-type compatibility:** any of `Inner` / `Left` / `Right` /
  `Full`. `Inner` drops rows with no match on either side;
  `Left` / `Right` preserve one side's rows and NULL-pad;
  `Full` preserves both.
- **Planning implication:** the planner may schedule aggregation either
  before or after the join without correctness risk. Cost-based choice
  drives ordering.

#### 3.3.2 `OneToMany`

One row in `from` matches zero-or-more rows in `to`.

- **Fanout:** potential. A `SUM(from.measure)` computed after joining
  would sum a `from` row once per matched `to` row, overstating the
  aggregate by the `to`-side fanout factor.
- **Fanout-safe rewrite.** When the planner composes a surface that
  requires a measure on the `from` side **and** a dimension from
  `to`-side (or a transitive `to`-side constituent), it rewrites the
  plan to aggregate `from` to its grain before the join. This is a
  `17 §*` × `Additivity` interaction: measures declared `Additive`
  under the join's grain are safely rewritten; `SemiAdditive` requires
  the grain axis match `11 §7`'s declared axes; `NonAdditive` measures
  trigger `PLAN_W_0501 FanoutAdvisory` (§14.4) and the planner emits
  the straightforward join (potentially yielding duplicated contributions
  the author is responsible for handling in their Request).
- **Join-type compatibility:** `Inner` with `OneToMany` is fine. `Left`
  from the `One` side with `OneToMany` produces one row per `to`-side
  match; zero-match `from` rows are preserved with NULL `to`-side
  fields. `Right` from the `One` side is equivalent to `Left` from the
  `Many` side; `Full` preserves unmatched rows on both.
- **Walked in reverse** (under `Bidirectional`): behaves as `ManyToOne`
  (see §3.3.3).

#### 3.3.3 `ManyToOne`

Many rows in `from` match exactly one row in `to` (typical fact → dim).

- **Fanout:** none on measures declared on `from`. Measures declared on
  `to` are subject to re-distribution: a `SUM(to.population)` grouped by
  a `from` attribute sums `to.population` once per `from` row matching
  that attribute — an author-intent mismatch the `PLAN_W_0501
  FanoutAdvisory` flags.
- **Join-type compatibility:** any. `Inner` drops unmatched `from` rows;
  `Left` preserves unmatched `from` rows with NULL `to` fields;
  `Right` / `Full` are less common but permitted.
- **Planning implication:** the planner may schedule aggregation after
  the join without correctness risk for measures on `from`; measures
  on `to` require either pre-join aggregation on `to` (if supported by
  the strategy) or the `PLAN_W_0501` advisory.

#### 3.3.4 `ManyToMany`

Multiple rows on each side match multiple rows on the other.

- **Fanout:** bilateral. Aggregations are at risk on both sides.
- **Canonical modeling.** `ManyToMany` without an intermediate junction
  `DataKind` is usually an anti-pattern in analytics. Authors are
  nudged toward declaring two `ManyToOne` `Relationship`s through a
  junction `DataKind`. `16 §16 Q16` tracks the "reject by default"
  alternative; Round 1 permits with advisory.
- **Planning advisory.** On every `ManyToMany` walked by an implicit
  or explicit composition, the planner emits `PLAN_W_0502
  ManyToManyFanoutAdvisory` (§14.4) nudging the author toward
  junction-table modeling. Queries proceed; correctness is the author's
  responsibility.
- **Join-type compatibility:** any. Deduplication — when the planner
  needs it to preserve cardinality of a primary-key side — is
  `DISTINCT`-based (plan-layer decision; `20`).

## 4. `JoinType`

Enum defined in [`18 §2.3`](./18_entities.md#23-jointype). v1 variants: `Inner`, `Left`, `Right`, `Full`. `Semi` / `Anti` are deferred (see §4.3 below). `AsOf` is deferred and gated on `17 TemporalShape` (see §4.3 and `17 §5`).

The join-kind carried by a `Relationship`. Lowers directly to
`PlanNode::Join`'s `join_type` field in `35`.

### 4.1 Enum — ratified variants

Four variants in v1: `Inner`, `Left`, `Right`, `Full`. `#[non_exhaustive]`
per I10. All four translate 1:1 to standard SQL `INNER JOIN`,
`LEFT JOIN`, `RIGHT JOIN`, `FULL OUTER JOIN`.

### 4.2 Per-variant semantics

- **`Inner`** — produces rows with a match on **both** sides of the
  `KeyPair`. Non-matching rows on either side are dropped. Combined
  with `Cardinality::OneToOne` and non-NULL join columns, `Inner` is
  the canonical "joined table" semantics.
- **`Left`** — preserves all rows from `Relationship.from`; NULL-pads
  the `to` side for unmatched `from` rows. Combined with `ManyToOne`,
  this is the canonical "enrich facts with dim, keep unmatched facts"
  pattern.
- **`Right`** — preserves all rows from `Relationship.to`; NULL-pads
  the `from` side for unmatched `to` rows. Less common in analytic
  workloads but symmetric.
- **`Full`** — preserves rows from both sides; NULL-pads whichever side
  lacks a match. Useful for union-like semantics that the shape of a
  `Unionset` would not express cleanly.

**Under `Bidirectional` reversal.** `Left` walked in reverse becomes
`Right` at plan emission (see §2.4.3). The canonical-layer `JoinType`
value does not change; the emission substitutes.

### 4.3 Deferred variants

**`Semi` / `Anti`.** Deferred from v1. Both are correctness-preserving
optimizations of `Inner` (with and without matching rows respectively)
that the adapter layer may emit as an engine-specific rewrite of an
`Inner` + dedup-and-project pattern. Authors explicitly wanting
`Semi` / `Anti` semantics in v1 write a `Filter` constraint referencing
an `EXISTS`-style semantics; a future v2 revision may introduce
`JoinType::Semi` / `JoinType::Anti` as canonical variants. Tracked as
`[TD-COMPOSITION-SEMI-ANTI]`.

**`AsOf`.** Deferred; gated on `17 TemporalShape`'s `SCD` and
`Snapshot` variants. An `AsOf` join's semantics require matching the
closest-preceding or closest-following timestamp in the `to` side to a
reference timestamp on the `from` side — which presupposes a ratified
temporal axis on both constituents. `17` is the gating document; `16`
does not reserve `JoinType::AsOf` in v1. Tracked as
`[TD-COMPOSITION-ASOF]`.

### 4.4 `PlanNode::Join` carriage

Per `35`'s `PlanNode::Join` variant:

```rust
pub enum PlanNode {
    // ...
    Join {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        join_type: JoinType,           // ratified in 16 §4
        predicate: PhysicalExpr,       // resolved from Relationship.keys via 15
        cardinality: Cardinality,      // ratified in 16 §3
        relationship_id: Option<RelationshipId>, // set for traversal-derived joins
        // ...
    },
    // ...
}
```

The `relationship_id` field is `Some(RelationshipId)` when the `Join`
node was emitted from a `Relationship` traversal (implicit composition
or `Joinset`) and `None` when emitted from a direct predicate (e.g. a
`Unionset`'s internal `UNION ALL` has no `Join` node, so this question
does not arise; a hand-authored `filter:` with an `EXISTS` construct
would, when reducible to a `Semi` rewrite by the adapter, lose
`relationship_id`). Adapters consume the trio (`join_type`, `cardinality`,
`relationship_id`) to emit engine-correct SQL / IR.

## 5. `ComposedSemanticInterface`

```rust
#[non_exhaustive]
pub struct ComposedSemanticInterface {
    pub composition_kind: CompositionKind,
    pub constituents: Vec<DataKindRef>,
    pub interface: UnifiedSemantics,      // §6 — namespace-aware merge
    pub provenance: FieldProvenance,      // §7 — per-field ownership
    pub coverage: CompositionCoverage,    // §8 — extends 15 §6
    pub traversed_paths: Vec<RelationshipPath>, // §5.2
}
```

The **unified queryable surface** the planner works against when a
Request's selected `Semantics` span multiple `DataKind`s. A
`ComposedSemanticInterface` is the composition-level analogue of a
`SemanticInterface` (`11 §6`), but its shape is materially different:
where `SemanticInterface` enumerates the `Semantics` of a single kind,
`ComposedSemanticInterface` enumerates the `Semantics` of N kinds
reconciled under `UnifiedSemantics`, with per-field ownership
(`FieldProvenance`) and per-constituent coverage (`CompositionCoverage`).

### 5.1 Structure

- `composition_kind: CompositionKind` — the origin discriminator (§5.3).
- `constituents: Vec<DataKindRef>` — the top-level `DataKind`s
  participating. Exactly the kinds that contribute at least one field
  or one edge to the composition. Order is significant for
  `Joinset` / `Unionset` / `Grainset` (author-declared); unspecified
  (but deterministic) for implicit `Relationship`-composition.
- `interface: UnifiedSemantics` — the merged semantic surface (§6).
- `provenance: FieldProvenance` — per-unified-name ownership (§7).
- `coverage: CompositionCoverage` — per-constituent per-name coverage
  (§8).
- `traversed_paths: Vec<RelationshipPath>` — the `Relationship`
  traversal that produced the composition (§5.2). Empty for `Unionset`
  and `Grainset`; non-empty for `Joinset` (author-declared path) and
  implicit `Relationship`-composition (planner-synthesized path).

### 5.2 `traversed_paths`

The `RelationshipPath` struct is owned by [`14b §4.5`](./14b_expression_resolution.md#45-pathsignature) — a `#[derive(Ord, PartialOrd, Eq, PartialEq)]` newtype over `Vec<RelationshipId>`. `16` consumes that shape; it does not redefine it.

For `CompositionKind::Joinset` and `CompositionKind::Relationship`, this
records the `RelationshipId` chain that produced the composition. Shape
is `Vec<RelationshipPath>`, not a single `RelationshipPath`, because a
multi-target BFS may yield a **tree cover** over 3+ constituents — one
`RelationshipPath` per "leg" of the tree.

For `CompositionKind::Unionset` and `CompositionKind::Grainset`, this
field is empty (vertical compositions do not traverse `Relationship`s).

`traversed_paths` is consistent with `14b §4.5`'s `PathSignature`: for a
cross-kind reference inside a composed Request, the per-expression
`PathSignature` is a subset of the composition's `traversed_paths`.

Single-path vs tree-cover shape is tracked as `Q-COMP-011`; Round 1
ratifies `Vec<RelationshipPath>`.

### 5.3 `CompositionKind`

```rust
#[non_exhaustive]
pub enum CompositionKind {
    Relationship, // implicit via declared Relationship(s); planner-synthesized
    Unionset,     // explicit — Unionset DataKind (vertical append)
    Grainset,     // explicit — Grainset DataKind (grain-sharded)
    Joinset,      // explicit — Joinset DataKind (named subset of Relationships)
}
```

Four variants, one per source of the composed surface:

- **`CompositionKind::Relationship`** — implicit composition. Emitted by
  the planner's field-first resolution (§11) when `Request.from = None`
  and selected `Semantics` span multiple `DataKind`s connected by
  declared `Relationship`s. Ephemeral: exists only for the duration of
  the planning call, never persisted.
- **`CompositionKind::Unionset`** — explicit vertical composition. Emitted
  by `compile` when the author declares a `Unionset` `ComplexDataKind`
  (per `12 §3`). Materialized in the Manifest as a
  `ResolvedComplexDataKind` with `composition_kind: Unionset` (§10.1).
- **`CompositionKind::Grainset`** — explicit grain-sharded composition.
  Emitted by `compile` when the author declares a `Grainset`
  `ComplexDataKind` (per `12 §4`). Materialized in the Manifest (§10.1).
- **`CompositionKind::Joinset`** — explicit named composition over
  `Relationship`s. Emitted by `compile` when the author declares a
  `Joinset` `ComplexDataKind` (per `12 §5`). Materialized in the
  Manifest (§10.1, §13).

The distinction between `Relationship` and `Joinset` (both
horizontal, both `Relationship`-mediated) is **materialization and
identity**: `Joinset` has a name, is author-declared, and is persisted;
`Relationship`-composition is anonymous, planner-synthesized, and
Request-scoped. See `§9` for the full boundary and
`open_questions Q-COMP-004` for the "should they merge?" debate.

### 5.4 Distinct type vs bare `SemanticInterface` — resolves open item (i)

**Ratified (Q1):** `ComposedSemanticInterface` is a **distinct Rust type**;
it is **not** a variant, wrapper, or subtype of `SemanticInterface`. The
two share a **common trait** (`SemanticsView`; §5.5) for the accessors
both expose, but otherwise have independent structures.

**Alternatives considered and rejected.**

- **Variant of a `SemanticInterface` enum.** Rejected because the two
  shapes share only a subset of their fields. `SemanticInterface` has no
  `constituents` or `provenance`; `ComposedSemanticInterface` has no
  owning `DataKindRef` (it has `Vec<DataKindRef>` instead).
  Shoehorning both into one enum forces every consumer to match on
  every variant, even when only one is semantically possible.
- **Wrapper over `SemanticInterface`.** Rejected because `UnifiedSemantics`
  is not a `SemanticInterface` — it reconciles N of them. A wrapper
  would suggest `ComposedSemanticInterface.inner: SemanticInterface`
  plus decorations, but no single `SemanticInterface` captures the
  unified surface.
- **Subtype via trait inheritance (`ComposedSemanticInterface: SemanticInterface`).**
  Rejected because Rust does not have subtype polymorphism for structs.
  A trait-based encoding is exactly what `SemanticsView` is.

**Rationale for distinct type.**

- **Pattern-matching clarity.** Planner code that needs to distinguish
  composed vs non-composed surfaces pattern-matches on the type or the
  source of the surface; a single enum would hide this distinction
  behind a variant check.
- **Field shape independence.** Future revisions to either surface
  (e.g. adding `materialization_mode` to `ComposedSemanticInterface`)
  do not force a matching field on `SemanticInterface`.
- **Trait-based convergence where useful.** For the few operations
  that make sense on both (query "does this surface expose a dimension
  named X?"), `SemanticsView` provides a uniform accessor (§5.5).
- **Serialization.** `SemanticInterface` serializes as a single-kind
  manifest entry; `ComposedSemanticInterface` serializes (when persisted
  — only for explicit compositions; §10) as a `ResolvedComplexDataKind`
  with materially different on-disk shape.

### 5.5 `SemanticsView` trait

```rust
pub trait SemanticsView {
    fn dimension_names(&self) -> &[SemanticsName];
    fn measure_names(&self)   -> &[SemanticsName];
    fn metric_names(&self)    -> &[SemanticsName];
    fn key_names(&self)       -> &[SemanticsName];
    fn filter_names(&self)    -> &[SemanticsName];
    fn has_semantics(&self, name: &SemanticsName) -> bool;
    fn semantics_role(&self, name: &SemanticsName) -> Option<SemanticsRole>;
}
```

Both `SemanticInterface` and `ComposedSemanticInterface` implement
`SemanticsView`. `SemanticsRole` (from `11 §6`) is the enum
`Dimension` / `Measure` / `Metric` / `Filter` / `Key`. This trait is
the planner's primary shape-query interface; strategy code that doesn't
need to distinguish composed vs non-composed surfaces operates generically
over `&dyn SemanticsView`.

Where strategies need to distinguish:

- `&SemanticInterface` directly exposes the owning `DataKindRef`.
- `&ComposedSemanticInterface` directly exposes `constituents`,
  `provenance`, `coverage`, `traversed_paths`.

## 6. `UnifiedSemantics`

```rust
#[non_exhaustive]
pub struct UnifiedSemantics {
    pub dimensions: BTreeMap<UnifiedName, Dimension>,
    pub measures:   BTreeMap<UnifiedName, Measure>,
    pub metrics:    BTreeMap<UnifiedName, Metric>,
    pub filters:    BTreeMap<UnifiedName, Filter>,
    pub keys:       BTreeMap<UnifiedName, Key>,
}

pub struct UnifiedName(pub String); // bare `SemanticsName` or `constituent.name`
```

The namespace-aware merge of the constituents' `SemanticInterface`s.
`UnifiedName` carries either a bare name (no collision) or a
qualified name (`"constituent.name"`). All accessors on `SemanticsView`
return `SemanticsName`s which may be qualified strings — consumers must
treat them as opaque identifiers.

### 6.1 Input — constituents' `SemanticInterface`s

For each `DataKindRef` in `ComposedSemanticInterface.constituents`, the
merge consults the constituent's `SemanticInterface` (from the Manifest's
per-kind index, I8). For `Unionset` and `Grainset`, constituents are the
top-level `ComplexDataKind` children (flattened per `12 §8`). For
`Joinset`, constituents are the author-declared member kinds. For
implicit `Relationship`-composition, constituents are the kinds owning
at least one selected `Semantics` plus any path-intermediate kinds
traversed by the BFS (§11.4).

### 6.2 Name-collision resolution — namespace-aware

When two or more constituents declare a `Semantics` of the same
`SemanticsName` and the same role, the merge rule depends on
`composition_kind`:

**`Unionset` / `Grainset`.** Collisions **unify**: both constituents
promote the shared name into the composed surface under its bare
`UnifiedName`. The `FieldOwnership` is `Shared(vec![A, B])` (§7.3.2).
Per `24` / `22` this is the `UNION ALL` / grain-sharding default.

**`Joinset` / `Relationship`.** Collisions **qualify**: each contributing
constituent promotes its name into the composed surface under a qualified
`UnifiedName` of the form `constituent.name`. The bare `name` does NOT
exist on the composed surface — any Request referencing the bare form
triggers `PlannerError::AmbiguousCompositionReference` (§14.3,
`PLAN_E_0505`) with the candidate qualifications in the Diagnostic.

**Example.** `orders` and `returns` each declare a `total: Measure`.
Under a `Joinset` composing both, the composed surface exposes
`orders.total` and `returns.total` but not a bare `total`.

**Role disagreement.** Two constituents declaring the same name with
**different roles** (e.g. one `Dimension`, one `Measure`) is always
an error, regardless of `composition_kind`:
`CompileError::CompositionRoleConflict` (§14.1). This check runs at
`compile` for explicit compositions; at plan for implicit ones.

### 6.3 Common-dimension promotion

When two or more constituents declare a `Dimension` of the same
`SemanticsName`, the same `DataType`, and semantically compatible
definition (same `expr` if any, same `type:` facet per `11 §6.1`), the
merge promotes it to a single composed-surface dimension under the
bare `UnifiedName` with `FieldOwnership::Shared`. This is the "all
datasets have a `date` dimension" case.

**Compatibility requires:**

- Same role (`Dimension`).
- Same `data_type` (exact match; no widening).
- Same `type:` facet (`temporal.grains`, `categorical.values`, etc.
  match structurally per `11 §6.1`).
- Same `expr:` (structural AST equality) or both lacking `expr`.
- Same `description` (if any; mismatches emit `COMP_W_0401` advisory
  but do not block).

**Incompatible dimensions** with the same name follow §6.2's
`composition_kind` rule — `Unionset` / `Grainset` unify under type
widening (per `13`), `Joinset` / `Relationship` qualify.

**Rationale.** Promotion produces the ergonomic, "author probably meant
this" surface. Qualification is available as a fallback when the
promotion check fails.

### 6.4 `Measure` / `Metric` surface

**Aggregation-semantics conflict — rejected.** Two constituents that
declare a `Measure` of the same `SemanticsName` with **different
aggregation functions** (e.g. `revenue` declared `agg: sum` on one
constituent, `agg: avg` on another) **cannot** be unified:
`CompileError::CompositionAggregationConflict` (§14.1) for explicit
compositions; `PlannerError::CompositionAggregationConflict` (§14.3,
`PLAN_E_0506`) for implicit. Authors resolve by renaming one side or
declaring the composition explicitly.

**Measures on one constituent, queried over the composed surface.**
Permitted. The Measure surfaces under its bare name (no collision) or
qualified name (collision). Planning consults `Cardinality` to decide
whether to pre-aggregate (fanout-safe rewrite per §3.3.2) or emit the
straightforward join with a `PLAN_W_0501 FanoutAdvisory` if the
`Additivity × TemporalShape` interaction (per `17`) is unsafe.

**Metrics.** Aggregation-function conflicts are rejected by the same
rules as Measures. Metrics that reference other Measures / Metrics via
`expr:` are resolved per `14b §6.4`'s compile-time reference-DAG
traversal; if the references span constituents, the implicit
composition's `PathSignature` is recorded on the ResolvedExprEntry for
the Metric and the planner consumes it at plan time.

**Filters.** Filters compose like Dimensions — promotion under
compatibility, qualification otherwise.

### 6.5 Composed-surface keys

**`Unionset`** — composed-surface keys are the intersection of the
constituents' `Key::Primary` / `Key::Unique` declarations sharing the
same `SemanticsName` and type (per `11 §8.3`). Keys that hold on all
constituents under union semantics are preserved; keys that hold on
some but not all are dropped.

**`Grainset`** — similar to `Unionset`; keys that hold at all declared
grains survive.

**`Joinset`** — the composed surface's primary key is inherited from
the anchor constituent (§13.2). `Key::Foreign` declarations become
internal join conditions (not user-surface keys).

**`Relationship` (implicit)** — composed-surface keys are **empty**.
Implicit compositions do not claim keys; authors who need an
addressable key on a composed surface declare a `Joinset`. (See open
`Q-COMP-018`.) The planner derives per-strategy internally-needed
keys (deduplication pins) outside the `keys` field.

## 7. `FieldProvenance`

```rust
#[non_exhaustive]
pub struct FieldProvenance {
    pub ownership: BTreeMap<UnifiedName, FieldOwnership>,
}

#[non_exhaustive]
pub enum FieldOwnership {
    Native(DataKindRef),              // this constituent natively provides the field
    Shared(Vec<DataKindRef>),         // multiple constituents agree on the field
    NullFill(Vec<DataKindRef>),       // listed constituents lack; NULL-fill on missing side
    Derived(PhysicalExpr),            // field exists only post-composition (rare)
}
```

Per-field ownership on a composed surface. Analogous to `15 §6`'s
`Coverage` but at the composition level and with different semantics
(ownership, not coverage-variant classification).

### 7.1 Purpose

The planner uses `FieldProvenance` to decide, for each requested field:

1. **Which constituent(s) contribute the data.** `Native` → one
   constituent. `Shared` → multiple, unifiable. `NullFill` → one
   provider, others contribute NULL. `Derived` → no constituent;
   synthesized post-join.
2. **Which Scan nodes need the field.** `Native(X)` → include in X's
   Scan column list; `Shared(V)` → include in all of V's Scans.
3. **When NULL-handling matters.** `NullFill` triggers `COALESCE`
   projection from the provider side.

### 7.2 `FieldOwnership` roster

The four variants cover the cases observed across the four
`CompositionKind`s. `#[non_exhaustive]` per I10; additional variants
may be introduced without semver breakage.

### 7.3 Per-variant semantics

#### 7.3.1 `Native(DataKindRef)`

The named constituent is the **sole** native provider. Other constituents
may not declare this field, or declare it with an incompatible shape
rejected by §6.3.

**Example (Joinset).** `orders.revenue` — only `orders` carries
`revenue`; `customers` does not. `FieldOwnership::Native("orders")`.

**Planner lowering.** The field's `PhysicalExpr` (from the constituent's
`ResolvedExprTable`, per `14b §2`) is retrieved; the Scan node for the
constituent includes the necessary physical columns.

#### 7.3.2 `Shared(Vec<DataKindRef>)`

Two or more constituents agree on the field (same role, same type, same
`expr:` — per §6.3's compatibility check). The ownership vec lists all
contributing constituents.

**Example (Unionset).** `date` declared on every constituent with the
same `temporal.grains: [day, week, month]` facet.
`FieldOwnership::Shared(vec!["adwords", "facebook", "klaviyo"])`.

**Planner lowering.** Per `composition_kind`:

- **`Unionset`** — each constituent's Scan contributes its own
  `PhysicalExpr` for the field; the `UNION ALL` stacks them.
- **`Grainset`** — each participating grain's Scan contributes; the
  grain-router picks the cheapest.
- **`Joinset` / `Relationship`** — the planner elects one constituent
  as the "canonical source" for the field (typically the
  shortest-path one from the Request anchor) and projects through it;
  other constituents' contributions are used for coverage analysis
  but not projected.

#### 7.3.3 `NullFill(Vec<DataKindRef>)`

Listed constituents **lack** the field; at least one constituent
provides it natively. This is the `Unionset` / `Grainset` partial-coverage
case (per `11 §*` / `15 §6.2`).

**Example (Unionset).** `unionset: paid_media` with three constituents
`adwords`, `facebook`, `klaviyo`. `country` is declared on the union
interface; `adwords` and `facebook` map it; `klaviyo` does not.
`FieldOwnership::NullFill(vec!["klaviyo"])`. The native providers
(`adwords`, `facebook`) are tracked separately in the
`CompositionCoverage` entry for `country`.

**Planner lowering.** The constituent(s) lacking the field emit `NULL`
(literal) in their Scan projection for that column. The `UNION ALL`
stacks a NULL-contributing branch alongside the mapped branches.

#### 7.3.4 `Derived(PhysicalExpr)`

The field exists **only on the composed surface** — no constituent
provides it, but a composition-level expression derives it from other
composed fields. Rare; typical cases are composition-level synthesized
keys, join-indicator columns, or composed metrics that reference
cross-constituent measures.

**Example.** A `Joinset` that wants a `source` dimension encoding which
constituent contributed each row of a `Unionset` constituent. The
`source` field does not exist on any constituent; a `Derived` expression
— e.g. `LIT('orders')` for rows from the `orders` branch — populates
it at composition time.

**Planner lowering.** The `PhysicalExpr` is attached to the composed
surface's post-join projection. No constituent Scan columns needed.

Whether `Derived` warrants a distinct variant (vs. folding into
`Native` with a synthetic `DataKindRef`) is tracked as `Q-COMP-015`;
Round 1 ratifies `Derived` distinct.

## 8. `CompositionCoverage`

```rust
#[non_exhaustive]
pub struct CompositionCoverage {
    pub entries: BTreeMap<(DataKindRef, UnifiedName), CoverageVariant>,
}
```

Reuses `15 §6`'s `CoverageVariant` enum (`Native` / `NullFill` /
`Derived`) but keyed by `(DataKindRef, UnifiedName)` rather than
`(SourceIndex, SemanticsName)`.

### 8.1 Relation to `15 §6`

`15` owns Binding-level coverage: for each `(source_index, semantics)`
pair within a single `Binding`, does the source natively provide the
Semantics, NULL-fill it, or derive it? `16` extends the concept to the
**composition level**: for each `(constituent_kind, unified_name)` pair
on a composed surface, does the constituent natively provide the unified
field, NULL-fill it, or derive it?

**`§6.4`'s scope boundary holds:** `15 §6.4` explicitly hands off
composition-level coverage to `16`. `16` **consumes** `15`'s
Binding-level coverage as input — the composition-level coverage for a
constituent is a **fold** of the constituent's per-source coverage
(§8.3).

### 8.2 Shape — keyed by `(DataKindRef, UnifiedName)`

The key tuple:

- `DataKindRef` — one constituent of the composition. Every constituent
  has an entry for every `UnifiedName` on the composed surface
  (missing = not-covered, not zero-entries — absence denotes the
  constituent does not participate in this field at all).
- `UnifiedName` — the composed-surface name (bare or qualified per §6.2).

Value: `CoverageVariant::{Native, NullFill, Derived}`.

- **`Native`** — the constituent provides the field natively. Derived
  from `FieldOwnership::Native(this_kind)` or
  `FieldOwnership::Shared([..., this_kind, ...])`.
- **`NullFill`** — the constituent does not provide the field; NULL is
  projected. Derived from `FieldOwnership::NullFill([..., this_kind, ...])`
  or from the constituent being listed in a `Shared` ownership where
  its per-Binding `Coverage` records `NullFill`.
- **`Derived`** — rare; the constituent synthesizes the field from other
  fields via an `expr:` unique to that constituent.

Per-constituent vs collapsed shape is tracked as `Q-COMP-010`; Round 1
ratifies per-constituent keyed-by-tuple.

### 8.3 Derivation — fold of per-Binding coverages

For each constituent and each `UnifiedName` on the composed surface, the
composition-level coverage folds the constituent's per-Binding coverage
entries (from `15 §6`):

```text
compose(kind_ref, unified_name) =
    let semantics = un_qualify(unified_name)               // strip `kind.` prefix if present
    let binding   = manifest.binding_for(kind_ref)         // I8: compile-time index
    match binding.coverage.entries.get((source_0, semantics)):
        Some(Native)   => Native
        Some(NullFill) => NullFill
        Some(Derived)  => Derived
        None           => NullFill    // not mapped by any source
```

For multi-source constituents (a `Unionset` / `Grainset` constituent
within a `Joinset`), the fold across sources follows the constituent's
own composition rules (per `22` / `24`) first, then the result is
re-folded into the outer composition. Authors of nested compositions
(e.g. `Joinset` over `Unionset`) can trace each field's provenance back
to the originating source via this cascade.

**Use by planner.** `CompositionCoverage` drives per-Scan column
inclusion and NULL-projection decisions. A Request selecting only
`Native`-covered fields can prune `NullFill`-only constituents from the
plan entirely (under `composition_kind ∈ {Joinset, Relationship}`);
under `Unionset` / `Grainset` all constituents participate regardless.

## 9. Explicit vs Implicit Composition

**Ratified (Q3):** composition falls into two families — **explicit**
(author-declared via a `ComplexDataKind` or a `Relationship` chain
named in a `Joinset`) and **implicit** (planner-synthesized via
`Relationship` graph traversal at plan time). The boundary is bounded
by the rules in §9.1.

### 9.1 Boundary rules

1. **Only over declared `Relationship`s.** The planner never synthesizes
   a join over an anonymous predicate. If a Request requires a join
   that no `Relationship` declares, the planner emits
   `PlannerError::NoCompositionPath` (§14.3, `PLAN_E_0501`).
2. **Unambiguous shortest-path only.** The planner picks the
   shortest-hop path connecting the owning `DataKind`s of all selected
   `Semantics`. Ties in path length are errors
   (`PLAN_E_0500 AmbiguousImplicitComposition`); authors disambiguate
   by declaring a `Joinset` pinning a specific path, or by removing
   one of the candidate `Relationship`s from the Model.
3. **Depth-limited to `MAX_IMPLICIT_COMPOSITION_DEPTH` hops.** Round 1
   ratifies `4` hops (see `open_questions Q-COMP-001`). Requests that
   would require a longer path emit `PlannerError::CompositionDepthExceeded`
   (§14.3, `PLAN_E_0502`). The limit protects against anonymously
   assembled "universal joins" that would never produce sensible
   analytic results; authors wanting deeper compositions declare a
   `Joinset`.
4. **No synthesis across `Directionality::Forward`.** A `Forward`
   relationship walked in reverse fails with
   `PlannerError::CrossCompositionForbidden` (§14.3, `PLAN_E_0503`).
5. **No chaining of `CompositionKind`s within one walk.** The planner
   does not walk `Relationship`s whose endpoints are composed kinds
   (`CompositionKind::Relationship`) produced by a prior pass within
   the same Request. If a Request would require such chaining, the
   planner errors with `PLAN_E_0504 CompositionChainingForbidden`
   (§14.3). Authors declare the full composition as a `Joinset` /
   `Unionset` / `Grainset`. (See open `Q-COMP-006`.) Explicit
   `Relationship`s between composed kinds (e.g. a `Joinset` and a
   `Simple`) remain **declarable** per `§2.1` — but they are walked
   only when the Request's resolution enters through the composed
   kind as a named anchor (author typed `from: "joinset_name"`).
6. **Implicit composition requires `Request.from = None`.** When
   `Request.from = Some(DataKindRef)`, the planner looks up the
   named kind directly. If the kind is `Complex`, its pre-materialized
   composed surface is used (per `§11.6`); no implicit synthesis runs.

### 9.2 Explicit composition

Explicit compositions are materialized in the Manifest (§10.1) and carry
a user-addressable name:

- **`Unionset`** (`ComplexDataKind`) — vertical composition (UNION ALL
  semantics; `24`). Constituents must be `SimpleDataKind`s or
  flattened nested complex kinds. The composed surface's name is the
  `Unionset`'s declared name.
- **`Grainset`** (`ComplexDataKind`) — grain-sharded composition (router
  picks cheapest grain; `22`). The composed surface's name is the
  `Grainset`'s declared name.
- **`Joinset`** (`ComplexDataKind`) — named subset of `Relationship`s
  with an author-declared anchor (§13; `23`). Constituents are the
  declared member `DataKind`s. The composed surface's name is the
  `Joinset`'s declared name.
- **Direct `Relationship` reference.** Authors cannot query "a
  `Relationship`" as a `from:` target — `Relationship`s are edges, not
  `DataKind`s. To query the two sides of a `Relationship` as a named
  surface, declare a two-constituent `Joinset` over that
  `Relationship`.

An explicit composition is queryable as `from: <name>` in a Request.
The planner resolves the name to a `ResolvedComplexDataKind`, reads
the pre-built `ComposedSemanticInterface`, and plans against it.

### 9.3 Implicit composition

Implicit compositions emerge only when `Request.from = None` and the
selected `Semantics` span multiple top-level `DataKind`s connected by a
chain of declared `Relationship`s. The planner's field-first algorithm
(§11) synthesizes a `ComposedSemanticInterface` with
`composition_kind: CompositionKind::Relationship`. The synthesized
surface is request-scoped — it is not cached, not persisted, and
not reusable by a subsequent Request.

### 9.4 Rationale

**Why bound implicit composition.** Unbounded implicit composition
would silently synthesize joins over arbitrary long chains — a behaviour
that admits surprising query shapes (e.g. "all `Semantics` in the Model
joined together") and creates undefined semantics (`ManyToMany`
chains producing quadratic-cardinality surfaces). Bounding forces the
author to name the composition they want — the act of declaring a
`Joinset` is both documentation and the disambiguation target.

**Why ambiguous paths error.** I4 (determinism). The alternative — a
heuristic tie-breaker (lexicographically-smallest `RelationshipId`,
fewest `ManyToMany`, etc.) — would make the Request's result depend on
Relationship-declaration order or on internal numbering. Authors would
struggle to predict outcomes; debugging would require reading the
planner's code. Error-on-tie keeps the author-facing semantics crisp.

**Why no chaining of `CompositionKind`s within one walk.** The
implicit-composition algorithm is a flat BFS over the
`RelationshipGraph`. Recursive composition (synthesize a surface, then
walk from it) would require redefining `UnifiedSemantics` to unify over
already-unified surfaces — a correctness hazard (which side of a
composed `Shared` field "owns" onward-composition?) without a clear
win. Authors who need that shape declare it explicitly.

## 10. Materialization Policy

**Ratified (Q2):**

- **Explicit compositions are materialized in the Manifest.** `Unionset`,
  `Grainset`, and `Joinset` compile to `ResolvedComplexDataKind` entries
  with pre-built `ComposedSemanticInterface`s.
- **Implicit `Relationship`-driven compositions are synthesized on
  demand at plan time.** The Manifest does **not** pre-materialize
  every possible N-kind composition; the combinatorial cost is
  untenable, and the per-Request synthesis is cheap (plan-time
  algorithm is O(|RelationshipGraph|) with small constants).

### 10.1 Explicit — materialized

At `compile`:

- Each declared `ComplexDataKind` (`Unionset` / `Grainset` / `Joinset`)
  produces a `ResolvedComplexDataKind` in the Manifest (per `33`).
- Each `ResolvedComplexDataKind` carries:
  - A `ComposedSemanticInterface` with `composition_kind` matching the
    declared complex-kind variant.
  - A `constituents` list reflecting the declared children (or declared
    member kinds for `Joinset`).
  - A fully-computed `UnifiedSemantics` (namespace-aware merge, §6).
  - A `FieldProvenance` (§7).
  - A `CompositionCoverage` (§8).
  - `traversed_paths` (for `Joinset`).
- The planner, when presented with `Request.from = Some(<complex-kind-name>)`,
  retrieves the `ResolvedComplexDataKind` and plans against the
  pre-built interface. No merge logic re-runs at plan time.

### 10.2 Implicit — synthesized on-demand

At `plan`, when `Request.from = None` and field-first resolution detects
multi-kind `Semantics`:

- The planner invokes §11's algorithm to synthesize a
  `ComposedSemanticInterface` with `composition_kind:
  CompositionKind::Relationship`.
- The synthesized interface is held on the planner's call stack for the
  duration of plan construction; it does not survive the planning call.
- No Manifest write occurs. Subsequent Requests re-synthesize.

### 10.3 Rationale

- **Materialization cost vs. naming.** An author who names a composition
  (`Joinset`) is making an editorial claim that the composition has
  analytic value — worth the compile-time cost of pre-building.
  Anonymous walks do not make that claim; the planner should not pay
  for compositions no one asked to keep.
- **Combinatorial blowup.** For N top-level `DataKind`s with M
  `Relationship`s, the set of possible implicit compositions is the
  set of connected subgraphs with 2..N vertices — super-polynomial.
  Pre-materializing is untenable.
- **Caching is already done.** The per-`Relationship` resolution cost
  — type inference of key pairs, `RelationshipId` assignment, per-edge
  metadata — is paid once at `compile` and stored in the Manifest
  (per `14b §4.2`). Plan-time synthesis walks the pre-resolved edges;
  the walk is cheap.
- **Per-Request synthesis is O(|graph|).** BFS over tens to a few
  hundred edges is microseconds; repeating it per Request is
  operationally fine.

## 11. Field-first Resolution Algorithm

The planner's entry point when `Request.from = None`. Per `10 §3.4`,
the `plan` stage consumes the Manifest (I8 — no I/O, no re-resolution)
and produces a `SemanticPlan`. Field-first resolution runs before plan
tree construction to decide *which surface* the plan will be built
against.

### 11.1 Inputs

- `manifest: &Manifest` — carrying:
  - Name indices: `SemanticsName → Vec<DataKindRef>` (all kinds that
    declare the name); `DataKindRef → SemanticInterface`;
    `RelationshipId → Relationship`; `RelationshipGraph` with adjacency
    list (per `14b §4.2`).
  - Per-kind `CompositionCoverage` (for composed kinds).
  - Pre-resolved expressions in `ResolvedExprTable` (per `14b §2`).
- `request: &Request` — with `request.from = None` and
  `request.select: Vec<SemanticsName>` of length ≥ 1.

### 11.2 Step — Map each selected name to its owning kinds

For each `name` in `request.select`:

```text
let owning = manifest.name_index.get(&name)
    .ok_or(PlannerError::UnknownSemantics { name })?;
```

`owning` is `Vec<DataKindRef>`. A name with zero entries is an error
(`PLAN_E_05xx` range; per `14b §7`). A name appearing on two or more
unrelated kinds (not via a `Shared` ownership on a materialized
composition, but as two separate Simple-kind declarations) is tracked
here as a candidate qualification target — if the Request later emits
a bare name under a composed surface, the planner consults this list
to produce the `PLAN_E_0505 AmbiguousCompositionReference` Diagnostic's
candidate-qualification suggestion.

Let `T = ⋃ owning` for all selected names — the set of **candidate
owning kinds** for the Request. Deduplicate across selected names.

### 11.3 Step — Single-kind fast path

If `|T| == 1`: the Request is satisfiable from a single kind. No
composition needed. The planner takes the single `DataKindRef` in `T`
and plans against its `SemanticInterface` directly. The `Request` is
treated as if `from: Some(T[0])` had been declared.

This fast path dominates well-authored Models where most Requests
target a single fact-like `DataKind` with co-located dimensions.

### 11.4 Step — Multi-target BFS over the `RelationshipGraph`

If `|T| >= 2`: find the minimum-hop **subgraph** connecting all members
of `T`. Per `14b §4.2`, the `RelationshipGraph` is an adjacency-list
representation indexed by `DataKindRef`; neighbours are iterated in
deterministic order (sorted by `(RelationshipId, reverse: false < reverse: true)`).

**Multi-target BFS.** For `|T| == 2` (call the members `t0, t1`), this
reduces to a single-source shortest-path BFS from `t0` until `t1` is
found. For `|T| >= 3`, the problem is a **Steiner tree** — find a
subgraph connecting all of `T` with minimum total edge count. Round 1
uses a brute-force enumeration of candidate cover trees up to
`MAX_IMPLICIT_COMPOSITION_DEPTH` edges; for the graph sizes expected
in v1 (10s–100s of `Relationship`s, `|T|` typically 2–4) this is fast.
Tracked as `Q-COMP-003` for future sophistication.

**Determinism.** Neighbours are iterated in sorted order (by
`(RelationshipId, direction_flag)`). Trees of the same edge count are
enumerated in lexicographically-smallest-edge-set order; the first
such tree found is selected. If two trees of the same edge count exist,
the planner emits `PlannerError::AmbiguousImplicitComposition`
(§14.3, `PLAN_E_0500`) rather than picking. Per I4 determinism; see
open `Q-COMP-002`.

**Depth bound.** Any walk attempting a hop count exceeding
`MAX_IMPLICIT_COMPOSITION_DEPTH` short-circuits with
`PlannerError::CompositionDepthExceeded` (§14.3, `PLAN_E_0502`).

**Directionality respected.** `Relationship`s with
`directionality: Forward` are walked only `from → to`. Attempts to
walk them in reverse fail with
`PlannerError::CrossCompositionForbidden` (§14.3, `PLAN_E_0503`).

**Disconnection.** If no subgraph within the depth bound connects all
of `T`, the planner emits `PlannerError::NoCompositionPath` (§14.3,
`PLAN_E_0501`) citing the disconnected kinds.

### 11.5 Step — Synthesize the `ComposedSemanticInterface`

Given the selected cover tree (call its `RelationshipId`s collected into
`RelationshipPath`s per leg, call the set of visited `DataKindRef`s
`constituents`):

1. Set `composition_kind = CompositionKind::Relationship`.
2. Set `traversed_paths = Vec<RelationshipPath>` per §5.2 shape (one
   `RelationshipPath` per BFS leg; flat for `|T| == 2`, tree-shaped
   for `|T| >= 3`).
3. Construct `UnifiedSemantics` per §6's merge rules, with the
   `CompositionKind::Relationship` collision rule (qualify on
   collision; promote on compatible shared dimensions / filters).
4. Construct `FieldProvenance` per §7: each selected `Semantics` maps
   to `Native(owning)` for single-owner names; `Shared(owners)` for
   multi-owner compatible names; names required by the traversal
   (join-keys on intermediate edges) surface as internal-only — they
   do not appear in `UnifiedSemantics.*` but are carried on
   `traversed_paths`.
5. Construct `CompositionCoverage` per §8.3: fold each constituent's
   per-Binding coverage into the composition-level entry. Intermediate
   constituents (walked only for connectivity, no selected field from
   them) still produce `CoverageVariant::Native` / `NullFill` /
   `Derived` entries for the join-key `Semantics` — this is what the
   planner needs to decide column-pruning of intermediate Scans.
6. Set `constituents = visited_data_kind_refs` (in BFS-visit order).
7. Return the synthesized `ComposedSemanticInterface`.

The planner then proceeds with plan tree construction against the
synthesized interface (per `34`).

### 11.6 Step — `Request.from = Some(DataKindRef)` path

When `Request.from` is specified, field-first resolution does NOT run.
Instead:

1. Look up the named `DataKindRef` in the Manifest.
2. If the kind is `Simple`, plan against its `SemanticInterface`.
3. If the kind is `Complex` (`Unionset` / `Grainset` / `Joinset`), plan
   against its pre-materialized `ComposedSemanticInterface` (§10.1).
4. **Selected-name membership check.** Every `name` in `request.select`
   must exist on the resolved (possibly composed) surface. A name not
   present triggers `PlannerError::SemanticsNotOnSurface`
   (§14.3, `PLAN_E_0507`). The check is per-name against `SemanticsView`
   on the resolved interface.

No implicit composition occurs. Authors using explicit `from:` have
opted into a fixed surface; the planner honours it.

### 11.7 Interaction with `14b`'s cross-kind path resolution

`14b §4`'s BFS runs at `compile` time to produce `PathSignature` entries
on the `ResolvedExprTable` — one per cross-kind reference inside any
declared `expr:`. `16`'s field-first BFS at `plan` time is structurally
the same algorithm over the same `RelationshipGraph`; the distinction
is timing and input:

- **`14b`'s BFS.** Input: one `SemanticExpr` with known `EntityRef`s.
  Output: the `PathSignature` that supports compiling the expression.
- **`16`'s BFS.** Input: a Request's `select` names and their owning
  kinds. Output: a synthesized `ComposedSemanticInterface`
  constituents-and-edges.

Both share: the neighbour-iteration order, the tie-break-by-error
policy, the depth bound. The `RelationshipGraph` is shared infrastructure
(built once at `compile`, read by both).

## 12. `Relationship` Graph Well-formedness

Checked at `validate` (declarations) and `compile` (type-dependent
validation). All violations abort the containing stage per I12
(fail-fast).

### 12.1 Duplicate `Relationship`s — `ValidateError::DuplicateRelationship`

Two `Relationship`s that share the same unordered `{from, to}` pair
**and** the same `Vec<KeyPair>` (up to permutation) are duplicates.
Under bidirectional semantics, `A → B` and `B → A` are the same edge;
the author must choose one direction to declare.

**Detection.** At `validate`, for each `Relationship`, canonicalize
the `{from, to}` into a sorted pair and the `KeyPair` list into a
sorted vec; collect into a set; a collision emits
`ValidateError::DuplicateRelationship { between: (DataKindRef, DataKindRef), relationship_ids: (RelationshipId, RelationshipId) }`.

**Intentional multi-edges.** Authors who want two joins between the
same pair (e.g. a "primary customer" edge and a "shipping customer"
edge) declare two `Relationship`s with **different** `keys` (different
`SemanticsName`s). The unordered `{from, to, keys}` triple must be
unique.

### 12.2 `KeyPair` type agreement — `CompileError::RelationshipKeyTypeMismatch`

At `compile` (after `14b`'s type inference), for each `KeyPair { left,
right }` in each `Relationship`:

1. Resolve `left`'s inferred `DataType` by consulting `14b`'s
   `ResolvedExprTable` for the `Relationship.from` binding (or each
   binding of a multi-source constituent).
2. Resolve `right`'s similarly.
3. Apply `13 §4`'s type-compatibility relation.
4. If incompatible, emit:

```text
CompileError::RelationshipKeyTypeMismatch {
    relationship_id,
    key_pair_index,
    left_type: DataType,
    right_type: DataType,
}
```

**Widening.** Numeric sides widen per `13 §5` (narrower side promotes
to wider side). No error if widening succeeds. String sides require
exact match — `String` vs `StringLarge` (if `13` distinguishes them) is
an error. Temporal sides require equal precision.

**Multi-source constituents.** If `Relationship.from` is a `Unionset` /
`Grainset`, the key's type must agree across all source bindings
(compatible per `13 §4`). Mismatch within one constituent's sources is
a `15 §6`-domain error (`CoverageInconsistency`); mismatch across
constituents of the same side of a `Relationship` is the same
`CompileError::RelationshipKeyTypeMismatch` with
`left_type` = the disagreeing-source type.

### 12.3 No self-references — `CompileError::RelationshipSelfReference`

A `Relationship` with `from == to` is rejected in v1:

```text
CompileError::RelationshipSelfReference { relationship_id, kind: DataKindRef }
```

Self-joins (e.g. `employees → managers` on the same `employees` kind)
are a legitimate modeling need, but v1 does not support them — the
planner's BFS and implicit-composition algorithms assume simple-graph
edges without self-loops. Deferred as `[TD-COMPOSITION-SELFJOIN]`.
Authors needing self-joins in v1 declare two distinct `DataKind`s
(typically with the same underlying `Binding`) and a `Relationship`
between them.

### 12.4 `Cardinality` / `JoinType` consistency

Soft check — emits advisories (`PLAN_W_0503 RelationshipCardinalityKeyMismatch`,
`PLAN_W_0501 FanoutAdvisory`) rather than hard errors. Rules:

- `Inner` + `OneToOne` + non-null keys on both sides → clean.
- `Inner` + `OneToMany` → emits `PLAN_W_0501` if any `Measure` from
  the `from` side is queried on the composed surface and its
  `Additivity` is `SemiAdditive` or `NonAdditive` (per `11 §7`);
  clean for `Additive`.
- `Inner` + `ManyToMany` → emits `PLAN_W_0502
  ManyToManyFanoutAdvisory` whenever the composition is walked.
- `Left` / `Right` + `OneToOne` → clean; the NULL-pad side adds no
  fanout.
- `Left` / `Right` + `OneToMany` / `ManyToOne` → same advisory rules
  as `Inner` apply for aggregate queries.
- `Full` + any → preserves all unmatched rows on both sides; fanout
  advisories fire if either side's unmatched rows have measures.

### 12.5 Graph connectivity — observational only

`validate` does NOT reject disconnected `RelationshipGraph`s. A Model
can legitimately have several disconnected subgraphs (multiple
business domains under one Manifest). Disconnection only matters at
plan time, when a Request's selected names span disconnected owners
(`PlannerError::NoCompositionPath`, §14.3).

## 13. `Joinset` as Explicit Subset

`Joinset` (per `12 §5`, detailed in `23`) is the **author-named** way
to declare an explicit composition over one or more `Relationship`s. It
narrows implicit composition into a specific anchored subset with
pinned cardinality and fanout assumptions.

### 13.1 Role in the composition hierarchy

- **Where `Relationship` is the edge**, `Joinset` is the **named walk**.
  A `Joinset` references one or more `Relationship`s and commits to a
  specific traversal order.
- **Where implicit `Relationship`-composition is request-scoped**,
  `Joinset` is **persistent**: it appears in the Manifest as a
  `ResolvedComplexDataKind`, is queryable as `from: <joinset-name>`,
  and reuses its `ComposedSemanticInterface` across Requests.
- **Where implicit composition is depth-bounded and
  shortest-path-only**, `Joinset` imposes **neither** constraint — an
  author can declare a 10-hop `Joinset` if it has analytic meaning, and
  can pick any valid path (not necessarily shortest) among alternatives.

### 13.2 Anchored root child

A `Joinset` declares one **root** constituent — the "driving" `DataKind`
that all joins hang off. The root's primary key (per `11 §8.3`) becomes
the `Joinset`'s composed-surface primary key (per §6.5). Walks proceed
from the root outward.

In v1 (per `12 §5.3`'s binary-`Joinset` ratification), a `Joinset`
has exactly one non-root constituent — the joined side. The root +
one-join structure mirrors the `fact + dim` star-schema pattern but
`semstrait` does not privilege one constituent structurally. N-ary
`Joinset`s are deferred as `[TD-JOINSET-NARY]` (v2).

### 13.3 Traversed relationships and `JoinType` override

A `Joinset` references one or more `Relationship`s as its traversal
path. Per-edge, a `Joinset` MAY override:

- `join_type` — the `Joinset`'s per-edge `join_type` overrides the
  `Relationship`'s declared `join_type`. Use-case: `ManyToOne`
  `Relationship` declared with `Inner`; a `Joinset` wanting
  NULL-padding for enrichment overrides to `Left`.
- `directionality` — only if the `Relationship`'s declared
  `directionality` permits the requested direction. A `Forward`
  `Relationship` cannot be walked in reverse even under `Joinset`
  override. (Same `PLAN_E_0503` semantics as implicit.)
- `cardinality` — **not overridable**. The `Cardinality` is a property
  of the data, not of the walk; a `Joinset` asserting a different
  cardinality than the `Relationship` declares is a misconfiguration.
  `CompileError::JoinsetCardinalityOverride` (§14.1) fires.

### 13.4 Pinned cardinality / fanout

A `Joinset`'s `ComposedSemanticInterface.cardinality_profile` (carried
on `ResolvedComplexDataKind`, per `33` and `23`) records the effective
cardinality of the composed surface: a fold of per-edge `Cardinality`
along the traversal path. For a binary `Joinset` with `OneToMany` on
its one edge, the effective cardinality is `OneToMany` (the `from`
side is the "anchor", the `to` side is the "fanout"). The planner
reuses the pinned profile at plan time for fanout-safe rewrite
decisions.

### 13.5 `Joinset` and implicit composition — no reuse (Round 1)

A reasonable optimization would have the planner detect "this implicit
composition covers exactly the same constituents as declared `Joinset`
X" and reuse `X`'s pre-materialized `ComposedSemanticInterface` instead
of synthesizing a new one. Round 1 **does not** do this, because:

- The `Joinset` may carry join-type overrides (§13.3) that the implicit
  composition did not request; silently picking up overrides would
  change semantics.
- The `Joinset` may impose a non-shortest traversal; the implicit
  algorithm promises shortest-path.
- Authors who want `Joinset X`'s surface write `from: "X"` explicitly.

Tracked as `[TD-COMPOSITION-JOINSET-REUSE]` (`Q-COMP-012`); revisit if
user feedback indicates the reuse-safe cases are common enough to
warrant detection logic.

**Explicit `Relationship`s that reference a `Joinset`.** `§2.1`
permits `Relationship`s whose `from` or `to` is a `Joinset`. The
`KeyPair.left` / `.right` references a namespaced `SemanticsName`
within the composed surface (e.g. `"order_details.customer_id"`). Such
`Relationship`s are declarable and walked only via explicit
`from: "joinset_name"` + subsequent explicit composition (another
`Joinset` or a Request whose selected `Semantics` pull the named
`Joinset` in as anchor); the implicit algorithm (§9.1 bullet 5) does
not chain them. See `Q-COMP-013`.

## 14. Error Model

New variants extend `CompileError`, `ValidateError`, and `PlannerError`
(defined in `10 §5`). Stable codes allocated per `30 §6`:

- **`COMP_E_0400-0499`** — compile-stage composition errors.
- **`PLAN_E_0500-0599`** — plan-stage composition errors.
- **`PLAN_W_0500-0599`** — plan-stage composition advisories (warnings).

All variants are `#[non_exhaustive]` per I10 on the parent enum.

### 14.1 `CompileError` additions

| Variant | Code | When |
|---|---|---|
| `RelationshipKeyTypeMismatch { relationship_id, key_pair_index, left_type, right_type }` | `COMP_E_0401` | `§12.2` — `KeyPair` sides have incompatible `DataType`s after `14b` inference. |
| `RelationshipSelfReference { relationship_id, kind }` | `COMP_E_0402` | `§12.3` — `Relationship.from == Relationship.to` (self-join, deferred). |
| `RelationshipKeyNotJoinable { relationship_id, key_pair_index, side, role }` | `COMP_E_0403` | `§2.3` — `KeyPair` references a `Measure` / `Metric` / `Filter` rather than `Key` / `Dimension`. |
| `CompositionRoleConflict { composition_name, name, roles }` | `COMP_E_0404` | `§6.2` — constituents declare same name with different roles (Dimension vs Measure). |
| `CompositionAggregationConflict { composition_name, name, aggregations }` | `COMP_E_0405` | `§6.4` — constituents declare same Measure name with different `agg:`. |
| `JoinsetCardinalityOverride { joinset_name, relationship_id, attempted, declared }` | `COMP_E_0406` | `§13.3` — `Joinset` tries to override per-edge `Cardinality`. |
| `JoinsetUnknownRelationship { joinset_name, relationship_name }` | `COMP_E_0407` | `Joinset` references a `Relationship` that does not exist. |
| `JoinsetUnreachableConstituent { joinset_name, constituent }` | `COMP_E_0408` | `Joinset` declares a constituent not connected to the root via declared `Relationship`s. |

### 14.2 `ValidateError` additions

| Variant | Code | When |
|---|---|---|
| `DuplicateRelationship { between, relationship_ids }` | `COMP_E_0410` | `§12.1` — two `Relationship`s declare the same unordered `{from, to, keys}`. |
| `RelationshipDataKindUnknown { relationship_id, side, kind }` | `COMP_E_0411` | `Relationship.from` / `.to` references an undeclared `DataKindRef`. |
| `RelationshipOnNestedKind { relationship_id, side, kind }` | `COMP_E_0412` | `§2.1` — `Relationship.from` / `.to` references a nested (non-Root-scope) kind. |
| `RelationshipEmptyKeys { relationship_id }` | `COMP_E_0413` | `§2.2` — `keys: Vec<KeyPair>` is empty. |

`ValidateError` uses the `COMP_E_04xx` range because these are
composition-specific checks even though they run at `validate`. The
`30 §6` allocation permits validate-stage codes to share range with
compile-stage codes when they're scoped to the same conceptual area.

### 14.3 `PlannerError` additions

| Variant | Code | When |
|---|---|---|
| `AmbiguousImplicitComposition { from_kinds, candidate_paths }` | `PLAN_E_0500` | `§11.4` / `§9.1` bullet 2 — two or more trees of equal edge count connect `T`. |
| `NoCompositionPath { from, to }` | `PLAN_E_0501` | `§11.4` — no subgraph within the depth bound connects all owning kinds. `from` / `to` are the disconnected kinds. |
| `CompositionDepthExceeded { from_kinds, max_depth }` | `PLAN_E_0502` | `§11.4` / `§9.1` bullet 3 — required walk exceeds `MAX_IMPLICIT_COMPOSITION_DEPTH`. |
| `CrossCompositionForbidden { relationship_id, attempted_direction }` | `PLAN_E_0503` | `§11.4` / `§2.4` — walk attempts reverse direction on a `Forward` `Relationship`. |
| `CompositionChainingForbidden { inner_composition_kind, outer_composition_kind }` | `PLAN_E_0504` | `§9.1` bullet 5 — implicit walk would chain into an already-composed surface. |
| `AmbiguousCompositionReference { name, candidates }` | `PLAN_E_0505` | `§6.2` — Request uses bare name on a composed surface with multiple qualifications. `candidates: Vec<UnifiedName>` carries the valid qualified forms. |
| `CompositionAggregationConflict { name, aggregations }` | `PLAN_E_0506` | `§6.4` — implicit composition attempts to unify `Measure` names with conflicting `agg:`. |
| `SemanticsNotOnSurface { name, surface }` | `PLAN_E_0507` | `§11.6` — Request's `from:` is set but selected name is not on the resolved surface. |
| `UnknownSemantics { name }` | `PLAN_E_0508` | `§11.2` — `Request.select` references a `SemanticsName` not in the Manifest. |

The `candidates` field on `PLAN_E_0505` carries `Vec<UnifiedName>` of
the valid qualified forms; diagnostic rendering includes one
`ContextLine` per candidate (per `30 §5.3`) with "use this form"
suggestions (per open `Q-COMP-014`, ratified yes).

### 14.4 Advisory (warning) additions

| Variant | Code | When |
|---|---|---|
| `FanoutAdvisory { relationship_id, cardinality, measure_name, additivity }` | `PLAN_W_0501` | `§3.3.2` — `OneToMany` or `ManyToOne` walked with a `SemiAdditive` / `NonAdditive` measure on the fanout side, and the planner cannot safely pre-aggregate (per `17` gating). |
| `ManyToManyFanoutAdvisory { relationship_id }` | `PLAN_W_0502` | `§3.3.4` — `ManyToMany` walked by composition. |
| `RelationshipCardinalityKeyMismatch { relationship_id, declared_cardinality, inferred_uniqueness }` | `PLAN_W_0503` | `§3.2` — `Cardinality` declared inconsistent with `Key::Primary` / `Key::Unique` on the key sides. |
| `CompositionSharedDimensionDescription { composition_name, name, descriptions }` | `COMP_W_0401` | `§6.3` — `Shared` promotion succeeded but constituents' `description` fields differ. |

Advisories do NOT abort the pipeline; they are collected as
`Diagnostic` entries alongside the produced plan / manifest. Consumers
(CLI, IDE) render them. Future `strict` mode (open `Q-COMP-005`) could
promote selected advisories to errors.

### 14.5 Code range summary

```text
COMP_E_04xx     validate + compile composition errors (§12, §13)
PLAN_E_05xx     plan-time composition errors (§11, §9.1)
PLAN_W_05xx     plan-time composition advisories (§3.3, §12.4)
COMP_W_04xx     compile-time composition advisories (§6.3)
```

`30 §6` ratifies the overall code-space; `16` allocates the
composition-specific ranges with headroom for future additions.

## 15. Interaction with Other Documents

`16`'s ratifications feed and consume several neighbours:

### 15.1 `14b` — path signatures

`14b §4.2`'s `RelationshipGraph` is the shared infrastructure both
documents consume: `14b` at compile (expression cross-kind resolution),
`16` at plan (field-first implicit composition). `14b §4.5`'s
`PathSignature` (`Vec<RelationshipId>`) is subset-consistent with
`16`'s `traversed_paths` on a composed surface — for every
`PathSignature` entry inside an expression on a composed Request, the
path is covered by the composition's `traversed_paths`.

`16` ratifies what a `Relationship` **is**; `14b`'s `PathSignature`
Vec<RelationshipId> is meaningful against that ratification. Changes
to `Relationship`'s shape in `16` propagate to `14b`'s path semantics.

### 15.2 `15` — coverage extension

`15 §6.4` explicitly hands off composition-level coverage to `16`;
`16 §8.3` consumes `15 §6`'s per-Binding `Coverage` as the fold input.
The two documents form a cascade: `15` → Binding-level coverage;
`16` → composition-level coverage. Authors of nested compositions
(e.g. `Joinset` over `Unionset`) trace field provenance through both.

`15 §2.2`'s `BindingId` appears in `16`'s planner algorithm as the
lookup key into per-kind bindings during implicit-composition synthesis.

### 15.3 `17` — temporal-shape-gated `AsOf` joins

`16 §4.3` defers `JoinType::AsOf` to `17`. `17`'s ratified
`TemporalShape` variants (`Timeseries`, `Events`, `Snapshot`, `SCD`)
gate the introduction: `AsOf` requires both constituents to declare
compatible temporal axes. Future `16` revisions adding `AsOf` will
forward-reference `17`'s shape-compatibility rules. Until then,
`JoinType::{Inner, Left, Right, Full}` exhaust the supported joins.

`17`'s `Additivity × TemporalShape` interaction also drives `16
§3.3.2`'s fanout-safe-rewrite triggering: the `Measure`'s `Additivity`
per `11 §7` plus the `TemporalShape` per `17` determines whether
pre-join aggregation preserves correctness.

### 15.4 `20-25` — per-`DataKind` strategies

- `20` (planner strategies overview) consumes `Cardinality` for
  fanout decisions and emits `PLAN_W_05xx` advisories.
- `21` (Simple / Dataset) is the leaf participant in any composition;
  `16`'s `FieldProvenance::Native` for a single-kind composition
  targets a `21`-owned `SemanticInterface`.
- `22` (Grainset) consumes `16`'s `CompositionCoverage` to select
  grain-shards for partial-coverage Requests.
- `23` (Joinset) details the full `Joinset` authoring and planning
  surface; `16 §13` ratifies the type-level contract.
- `24` (Unionset) details vertical composition; `16 §5.3`'s
  `CompositionKind::Unionset` flags the surface for `24`-specific
  planning.
- `25` (Metric surface) details how metrics wire through composition;
  `16 §6.4`'s measure / metric composition rules are the boundary.

### 15.5 `32` / `33` / `34` / `35` — surface / artefact / planner / IR

- `32` (YAML surface) ratifies the author-facing syntax for
  `relationships:`, `joinsets:`, `directionality:`, and defaults.
- `33` (Manifest) persists `ResolvedRelationship`,
  `ResolvedComplexDataKind`, and the `RelationshipGraph` adjacency
  index. `16`'s canonical types are the input; `33` commits to an
  on-disk shape.
- `34` (Planner) consumes `16`'s field-first algorithm as its entry
  point when `Request.from = None`. `34` ratifies the `plan`
  call-graph; `16` ratifies the specific algorithm step-1 runs.
- `35` (IR) carries `JoinType`, `Cardinality`, `RelationshipId` on
  `PlanNode::Join` per `16 §4.4`.

## 16. Ratified Decisions Index

Framework choices ratified in this document. Row order is chronological
within the document; the "Q" numbers are stable identifiers for
downstream citation.

| Q | Topic | Decision | Location |
|---|---|---|---|
| Q1 | Structural shape of `ComposedSemanticInterface` vs `SemanticInterface` (**resolves open item (i) from `00 §4.1`**) | Distinct type; shared `SemanticsView` trait for common accessors; not a variant or subtype of `SemanticInterface`. | §5.4, §5.5 |
| Q2 | Materialization policy for composed interfaces (**resolves open item (ii) from `00 §4.1`**) | Explicit (`Unionset` / `Grainset` / `Joinset`) → materialized in Manifest as `ResolvedComplexDataKind`. Implicit (`Relationship`-driven) → synthesized on-demand at plan time. | §10 |
| Q3 | Scope of implicit `Relationship`-driven composition (**resolves open item (iii) from `00 §4.1`**) | Bounded: only over declared `Relationship`s; unambiguous shortest-path only; depth-limited to `MAX_IMPLICIT_COMPOSITION_DEPTH` hops (Round-1 value `4`); no synthesis across `Forward` directionality in reverse; no chaining across already-composed surfaces. | §9.1 |
| Q4 | `Relationship` placement | Global top-level blocks in the `SemanticModel` (not inside any `DataKind`). Visible at `Root` scope per `11 §2`. | §2.1 |
| Q5 | `KeyPair` shape | Positional pairs — `Vec<KeyPair { left: SemanticsName, right: SemanticsName }>`. Both sides must resolve to `Key` or `Dimension` role; `Measure` / `Metric` / `Filter` rejected. | §2.3 |
| Q6 | Composite key ordering | Positional; `keys[i].left` pairs with `keys[i].right`. Multiple `KeyPair` entries represent one composite join condition under `AND`. | §2.3 |
| Q7 | `Directionality` variants | Two variants in v1: `Bidirectional` (default) and `Forward`. `#[non_exhaustive]` per I10. | §2.4 |
| Q8 | `Cardinality` variants | Four variants: `OneToOne`, `OneToMany`, `ManyToOne`, `ManyToMany`. `#[non_exhaustive]` per I10. Declared, not verified. | §3.1 |
| Q9 | `JoinType` variants (v1) | Four variants: `Inner`, `Left`, `Right`, `Full`. `Semi` / `Anti` / `AsOf` deferred. `#[non_exhaustive]` per I10. | §4.1, §4.3 |
| Q10 | `ComposedSemanticInterface` fields | `composition_kind`, `constituents`, `interface: UnifiedSemantics`, `provenance: FieldProvenance`, `coverage: CompositionCoverage`, `traversed_paths: Vec<RelationshipPath>`. | §5.1 |
| Q11 | `CompositionKind` variants | `Relationship` (implicit), `Unionset`, `Grainset`, `Joinset` (all explicit). `#[non_exhaustive]` per I10. | §5.3 |
| Q12 | `UnifiedSemantics` name-collision policy | `Unionset` / `Grainset` unify on compatible names (`FieldOwnership::Shared`). `Joinset` / `Relationship` qualify on collision (`constituent.name`). Bare name on collision under qualified form triggers `PLAN_E_0505`. | §6.2 |
| Q13 | `FieldOwnership` variants | `Native(DataKindRef)`, `Shared(Vec<DataKindRef>)`, `NullFill(Vec<DataKindRef>)`, `Derived(PhysicalExpr)`. `#[non_exhaustive]` per I10. | §7.2 |
| Q14 | `CompositionCoverage` shape | Keyed by `(DataKindRef, UnifiedName)` — per-constituent per-name entries. Reuses `15 §6`'s `CoverageVariant` enum (`Native` / `NullFill` / `Derived`). | §8.2 |
| Q15 | Field-first resolution algorithm | When `Request.from = None` and selected `Semantics` span ≥ 2 owning kinds, multi-target BFS over `RelationshipGraph` with deterministic neighbour order, shortest-hop wins, ties → `PLAN_E_0500`, depth bound enforced. | §11 |
| Q16 | Implicit composition produces `CompositionKind::Relationship` | Distinct from `Joinset`; request-scoped; not persisted; no reuse of explicit `Joinset`s even on constituent match (`[TD-COMPOSITION-JOINSET-REUSE]`). | §5.3, §13.5 |
| Q17 | Error-code allocation | `COMP_E_0400-0499` for compile / validate composition errors; `PLAN_E_0500-0599` for plan composition errors; `PLAN_W_0500-0599` for plan advisories; `COMP_W_0400-0499` for compile advisories. | §14.5 |

**Round-2 revisit candidates** (not in Q-numbered index; parked as
open questions):

- Depth-bound value (`Q-COMP-001`).
- Ambiguous-path heuristic (`Q-COMP-002`).
- Steiner-tree solver sophistication (`Q-COMP-003`).
- `CompositionKind::Relationship` vs `CompositionKind::Joinset` merge
  (`Q-COMP-004`).
- `strict` mode for `PLAN_W_0501` (`Q-COMP-005`).
- Relaxing cross-composition-kind chaining (`Q-COMP-006`).
- `Directionality` granularity (`Q-COMP-007`).
- Compile-time vs plan-time reverse-traversal detection (`Q-COMP-008`).
- Composite-key shape alternatives (`Q-COMP-009`).
- `CompositionCoverage` serialization shape (`Q-COMP-010`).
- `traversed_paths` tree-cover shape (`Q-COMP-011`).
- `[TD-COMPOSITION-JOINSET-REUSE]` (`Q-COMP-012`).
- `Relationship`s between composed kinds ergonomics (`Q-COMP-013`).
- `PLAN_E_0505` candidate suggestions (`Q-COMP-014`).
- `FieldOwnership::Derived` distinctness (`Q-COMP-015`).
- `ManyToMany` reject-by-default (`Q-COMP-016`).
- YAML-surface default for `JoinType` (`Q-COMP-017`).
- Derived keys on composed surfaces (`Q-COMP-018`).

Deferred-to-v2 tech debt:

- `[TD-COMPOSITION-SEMI-ANTI]` — `JoinType::Semi` / `Anti` variants (§4.3).
- `[TD-COMPOSITION-ASOF]` — `JoinType::AsOf` gated on `17` (§4.3).
- `[TD-COMPOSITION-SELFJOIN]` — self-referencing `Relationship`s (§12.3).
- `[TD-JOINSET-NARY]` — N-ary `Joinset`s (§13.2; owned by `23 §*`).
- `[TD-COMPOSITION-JOINSET-REUSE]` — implicit-composition reuse of
  declared `Joinset` (§13.5).
