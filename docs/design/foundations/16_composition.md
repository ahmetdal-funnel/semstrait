---
prereqs: [00, 10, 11, 12, 13, 14, 14a, 14b, 15, 18]
authoritative-for:
  - `Relationship` **composition semantics** — placement (global top-level), scope visibility, traversal rules, per-variant fanout analysis (struct shape owned by `18 §2`)
  - `ComposedSemanticInterface` — the unified queryable surface presented to the planner
  - `CompositionKind` — discriminator for the three flavours of composed surface (`Unionset` / `Grainset` / `Joinset`)
  - `Origin` — `Explicit` (author-declared) vs `Implicit` (compile-enumerated) axis on every composition
  - `ImplicitId` — content-stable canonical-form hash for implicit compositions
  - `UnifiedSemantics` — namespace-aware merge of constituent `SemanticInterface`s
  - `FieldProvenance` / `FieldOwnership` — per-field ownership on a composed surface
  - `CompositionCoverage` — extends `15 §6`'s `Coverage` to the composition level
  - `RelationshipPath` — the composition-level chain of `RelationshipId` traversals
  - explicit vs implicit composition — the `Origin` axis, the authoring contract, the implicit-explicit-clash rejection rule
  - materialization policy — compile-time eager enumeration of implicit compositions; cap and canonical-ID scheme
  - field-first resolution — the planner's lookup algorithm over the pre-built composition index
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
> `Origin` axis distinguishing `Origin::Explicit` (author-declared) from
> `Origin::Implicit` (compile-enumerated) compositions, and the
> field-first resolution algorithm the planner runs as a pure lookup
> over compile-enumerated compositions.
>
> **Three open items from `00 §4.1` land here (in the `ComposedSemanticInterface`
> row).** `16` closes them:
>
> - **(i)** Structural shape of `ComposedSemanticInterface` vs bare
>   `SemanticInterface` — **ratified:** distinct type, with a shared
>   `SemanticsView` trait for the accessors both expose (`§5.4`, `§16 Q1`).
> - **(ii)** Whether composed interfaces are materialized in the SemanticManifest
>   or synthesized by the planner on demand — **ratified (revised
>   2026-04-29):** all compositions — explicit (`Unionset` / `Grainset` /
>   `Joinset` declared in YAML) **and** implicit (`Joinset` /
>   `Unionset` enumerated by the planner from declared `Relationship`s
>   or coverage overlap) — are **materialized at compile time** in the
>   SemanticManifest. Implicit compositions are bounded by depth +
>   enumeration cap; their identity is a content-stable
>   `ImplicitId(BLAKE3-256)` of the canonical form. Plan-time is a pure
>   lookup (`§10`, `§16 Q2`).
> - **(iii)** Scope of implicit composition vs required explicit
>   declaration — **ratified (revised 2026-04-29):** implicit composition is
>   bounded to chains of **declared** `Relationship`s, walks
>   **transparently** through composed surfaces, is **depth-limited** to
>   `MAX_IMPLICIT_COMPOSITION_DEPTH` hops, and is **count-capped** at
>   `MAX_IMPLICIT_ENUMERATION_COUNT` per Model. Path ambiguity (multiple
>   shortest paths) errors at plan time; coverage ambiguity (multiple
>   independent kinds covering the same Semantics) synthesizes an
>   implicit `Unionset`. An explicit composition whose canonical form
>   matches an enumerable implicit composition is rejected at compile
>   (`§9.1`, `§10.6`, `§16 Q3`).
>
> **Status (Round 2 ratified 2026-04-29).** Unified Joinset model,
> compile-time eager materialization, intent-advisory drop, and
> implicit-explicit-clash rejection all closed. Round-1 ratifications
> for explicit-only composition shape (Q1, Q4–Q14) preserved.

## 1. Purpose and Scope

`semstrait` composes data along two axes. Vertically, an author groups
equivalent-shape `DataKind`s into a `Unionset` (append) or a `Grainset`
(grain-sharded). Horizontally, an author declares `Relationship`s between
`DataKind`s with complementary `Semantics`, and either (a) names the
traversal as a `Joinset` or (b) leaves the planner to walk the graph
implicitly when a `Request`'s selected `Semantics` span multiple kinds.

`16` is the authoritative specification for the horizontal axis's **core
type machinery** (`Relationship`, `Cardinality`, `JoinType`,
`Directionality`, `ComposedSemanticInterface`, `Origin`, `ImplicitId`,
`UnifiedSemantics`, `FieldProvenance`, `CompositionCoverage`), for the
**boundary** between `Origin::Explicit` and `Origin::Implicit`
compositions, the **eager-materialization policy** that enumerates
implicit compositions at compile, and the **field-first resolution
algorithm** the planner runs as a pure lookup at plan time.
Per-`DataKind` materialization strategies (`Unionset`, `Grainset`,
`Joinset` bodies), the YAML authoring surface, and the
SemanticManifest / Planner IR that carry the ratified shapes are
refined in the `refined-by` docs.

### 1.1 What `16` ratifies (index)

`16` ratifies: the `Relationship` struct + `KeyPair` + `Directionality`
(§2); `Cardinality` (§3); `JoinType` + `PlanNode::Join` carriage (§4);
`ComposedSemanticInterface` + `CompositionKind` (3 variants) + `Origin`
axis + `ImplicitId` + `SemanticsView` trait (§5, **resolves (i)**);
`UnifiedSemantics` merge logic (§6); `FieldProvenance` +
`FieldOwnership` (§7); `CompositionCoverage` extending `15 §6` (§8); the
explicit-vs-implicit composition boundary, including transparent
unfolding through composed surfaces (§9, **resolves (iii)**); the
materialization policy — compile-time eager enumeration with cap +
canonical-ID + clash-reject — (§10, **resolves (ii)**); the field-first
resolution algorithm as pure lookup (§11); `Relationship` graph
well-formedness preconditions (§12); `Joinset`'s explicit and implicit
forms under the unified model (§13); and new `CompileError` /
`ValidateError` / `PlannerError` variants with stable codes in the
`COMP_E_04xx` / `PLAN_E_05xx` / `PLAN_W_05xx` ranges (§14).

### 1.2 What `16` does NOT ratify

- **Per-`DataKind` planning strategies** — Scan / Join / Aggregate /
  Project lowering lives in `20`–`25` and `34`; `16` ratifies the
  type-of-surface the strategies plan against.
- **YAML authoring syntax** — `relationships:`, `joinsets:`,
  `directionality:` block shapes and defaults live in `32`.
- **`SemanticManifest` serialization** — on-disk shape of
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

Five stances govern:

1. **Name, not column.** `Relationship.keys` pair `SemanticsName`s; per
   I1 physical resolution is `15`'s responsibility (§2.3).
2. **Declare the edges, let the compiler enumerate the walks.** Authors
   declare pairwise `Relationship`s; compile enumerates every implicit
   `Joinset` (and implicit `Unionset` for coverage overlap) within the
   depth + count bounds. Authors who want to override defaults — pin a
   non-shortest path, change `JoinType` per leg, restrict via filters —
   declare an explicit `Joinset` (§9, §10, §13).
3. **Materialize everything.** Both explicit and implicit compositions
   are materialized at compile time. Plan-time is a pure lookup over
   the SemanticManifest's pre-built composition index. The
   eager-enumeration cap (`MAX_IMPLICIT_ENUMERATION_COUNT = 2000`)
   protects against pathological models (§10.4).
4. **One canonical form per composition.** An explicit `Joinset` whose
   canonical form (sorted `(RelationshipId, direction)` tuples) matches
   an enumerable implicit `Joinset` is rejected at compile
   (`COMP_E_0414`). Authors differentiate via per-leg overrides,
   filters, or `keys`; otherwise the planner uses the equivalent
   implicit form (§10.6).
5. **Fail fast, disambiguate up.** Path ambiguity (multiple shortest
   paths between same constituents) errors at plan time; authors
   disambiguate by declaring a differentiated explicit `Joinset` (I4
   determinism; §9.1, §14.3).

### 1.4 Guardrails upheld

- **I1 (canonical layer).** `Relationship.keys` are `SemanticsName`s;
  physical resolution is `15`'s responsibility.
- **I4 (determinism).** Ambiguous implicit-composition paths error
  (`PLAN_E_0500`); no heuristic tie-break.
- **I5 (compile-time resolution).** Name indices and the
  `RelationshipGraph` are pre-built at `compile`; the planner's walk
  is lookup, not resolution.
- **I8 (SemanticManifest is planner-complete).** The planner reads indices and
  graph from the SemanticManifest; no catalog fetch, no re-parse.
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
[`questions/closed/16_questions.md`](../questions/closed/16_questions.md)
Q-COMP-013 — closed); its `KeyPair.left` or `.right` may reference a
namespaced name within the composed surface (e.g.
`"order_details.customer_id"`).

### 2.2 Structure

Fields:

- `id: RelationshipId` — assigned at `compile` in declared-iteration order,
  `u32` shape, SemanticManifest-wide unique (`14b §4.2` owns the assignment). The
  ID is internal to one SemanticManifest; not stable across recompiles (see
  `14b_questions OQ-7`).
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
`RelationshipId` — they are the same edge, walked in two directions.
The compile-time enumeration in `§10.4` normalizes direction at walk
time: given a `current_node` and an unvisited neighbour `target_node`,
the step is flagged `reverse: true` when `current_node ==
Relationship.to && target_node == Relationship.from`, and
`reverse: false` otherwise. The `PathSignature` (`14b §4.5`) records
the `RelationshipId` alone; the direction is reconstructed at plan
time by matching `current_node` against the stored `from` / `to`.

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
  emit the straightforward join (potentially yielding duplicated
  contributions the author is responsible for handling in their
  Request — fanout is the natural consequence of declaring a `OneToMany`
  relationship and asking aggregation across it).
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
  that attribute. The author owns this trade-off when declaring the
  relationship.
- **Join-type compatibility:** any. `Inner` drops unmatched `from` rows;
  `Left` preserves unmatched `from` rows with NULL `to` fields;
  `Right` / `Full` are less common but permitted.
- **Planning implication:** the planner may schedule aggregation after
  the join without correctness risk for measures on `from`; measures
  on `to` require pre-join aggregation on `to` if the strategy supports
  it, otherwise the straightforward join.

#### 3.3.4 `ManyToMany`

Multiple rows on each side match multiple rows on the other.

- **Fanout:** bilateral. Aggregations are at risk on both sides.
- **Canonical modeling.** `ManyToMany` without an intermediate junction
  `DataKind` is usually an anti-pattern in analytics. Authors are
  nudged toward declaring two `ManyToOne` `Relationship`s through a
  junction `DataKind`. v1 permits direct `ManyToMany` declaration
  (`Q-COMP-016`); the structural-mismatch advisory `PLAN_W_0503`
  (§14.4) still fires when the cardinality contradicts declared
  uniqueness on the key sides.
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
    pub origin: Origin,                          // §5.6 — Explicit vs Implicit
    pub constituents: Vec<DataKindRef>,
    pub interface: UnifiedSemantics,             // §6 — namespace-aware merge
    pub provenance: FieldProvenance,             // §7 — per-field ownership
    pub coverage: CompositionCoverage,           // §8 — extends 15 §6
    pub traversed_paths: Vec<RelationshipPath>,  // §5.2
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

- `composition_kind: CompositionKind` — the kind discriminator (§5.3).
  Three variants: `Joinset` / `Unionset` / `Grainset`.
- `origin: Origin` — the provenance axis (§5.6). `Origin::Explicit` for
  author-declared compositions; `Origin::Implicit { id: ImplicitId }`
  for compile-enumerated compositions. `Grainset` is always `Explicit`.
- `constituents: Vec<DataKindRef>` — the top-level `DataKind`s
  participating. Exactly the kinds that contribute at least one field
  or one edge to the composition. Order is significant — author-declared
  for `Origin::Explicit`; canonical (sorted by `DataKindName`) for
  `Origin::Implicit`.
- `interface: UnifiedSemantics` — the merged semantic surface (§6).
- `provenance: FieldProvenance` — per-unified-name ownership (§7).
- `coverage: CompositionCoverage` — per-constituent per-name coverage
  (§8).
- `traversed_paths: Vec<RelationshipPath>` — the `Relationship`
  traversal that produced the composition (§5.2). Empty for `Unionset`
  and `Grainset`; non-empty for `Joinset` (regardless of `Origin`).

### 5.2 `traversed_paths`

The `RelationshipPath` struct is owned by [`14b §4.5`](./14b_expression_resolution.md#45-pathsignature) — a `#[derive(Ord, PartialOrd, Eq, PartialEq)]` newtype over `Vec<RelationshipId>`. `16` consumes that shape; it does not redefine it.

For `CompositionKind::Joinset` (regardless of `Origin`), this records
the `RelationshipId` chain that produced the composition. Shape is
`Vec<RelationshipPath>`, not a single `RelationshipPath`, because the
implicit-Joinset enumeration (§10.4) may yield a **tree cover** over
3+ constituents (Steiner tree) — one `RelationshipPath` per "leg" of
the tree. Explicit Joinsets with multi-leg traversals (deferred per
`[TD-JOINSET-NARY]`) follow the same shape.

For `CompositionKind::Unionset` and `CompositionKind::Grainset`, this
field is empty (vertical compositions do not traverse `Relationship`s).

`traversed_paths` is consistent with `14b §4.5`'s `PathSignature`: for a
cross-kind reference inside a composed Request, the per-expression
`PathSignature` is a subset of the composition's `traversed_paths`.

### 5.3 `CompositionKind`

```rust
#[non_exhaustive]
pub enum CompositionKind {
    Joinset,    // horizontal — Relationship-mediated traversal
    Unionset,   // vertical — UNION ALL append
    Grainset,   // grain-sharded — coarsest-to-finest router
}
```

Three variants. The fourth Round-1 variant — `CompositionKind::Relationship`
for "implicit Relationship-driven composition" — was retired
(2026-04-29) when the unified Joinset model collapsed implicit and
explicit composition into a single kind discriminator + an `Origin`
axis (§5.6). Implicit Relationship-mediated compositions are now
`CompositionKind::Joinset` with `Origin::Implicit`.

**Per-variant origin matrix** (per §5.6):

| Variant | `Origin::Explicit` | `Origin::Implicit` |
|---|---|---|
| `Joinset` | author-declared `joinsets:` block (§13, `12 §5`) | compile-enumerated from declared `Relationship`s (§10.4, §11) |
| `Unionset` | author-declared `unionsets:` block (`12 §3`) | compile-enumerated for coverage overlap (§10.5) |
| `Grainset` | author-declared `grainsets:` block (`12 §4`) | n/a — Grainset is always explicit (no implicit grain inference in v1) |

All three variants are materialized in the `SemanticManifest` per §10.
The author addresses explicit compositions by their declared name
(`from: <name>`); implicit compositions are addressed by their
`ImplicitId` canonical hash (§5.7), surfaced under a synthetic name
the planner assigns at compile (`§10.4.4`).

`#[non_exhaustive]` per I10. Future MINOR additions (e.g.
`Snapshotset`, `Windowset`) are admissible without semver breakage.

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

### 5.6 `Origin` axis

```rust
#[non_exhaustive]
pub enum Origin {
    Explicit,
    Implicit { id: ImplicitId },
}
```

**Why an axis, not separate types.** The unified-Joinset model
(2026-04-29) collapsed the previous `CompositionKind::Relationship`
(implicit, planner-synthesized) into `CompositionKind::Joinset` with
`Origin::Implicit`. Both forms share the same struct shape, the same
`UnifiedSemantics`, the same `FieldProvenance`, and the same plan-time
contract — they differ only in **where the composition's identity
comes from**.

- **`Explicit`** — author-declared `joinsets:` / `unionsets:` /
  `grainsets:` block. The composition's name (`DataKindName`) is the
  author's text; the planner addresses it directly. Author MAY
  declare overrides (per-leg `JoinType`, `keys`, filters) that
  differentiate it from the equivalent implicit composition.
- **`Implicit { id: ImplicitId }`** — compile-enumerated from declared
  `Relationship`s (Joinset) or coverage overlap (Unionset). Compile
  assigns a synthetic `DataKindName` derived from the `ImplicitId`
  (§5.7) and indexes it under both that name and the canonical hash.
  No author overrides — the composition uses defaults from the
  underlying `Relationship` declarations.

`Grainset` is always `Origin::Explicit` in v1; there is no implicit
grain inference. (Future v2 might introduce
`Origin::Implicit` for catalog-discovered grain hierarchies, tracked
as `[TD-GRAINSET-IMPLICIT]`.)

**Equivalence under `Origin`.** Two compositions with the same
`composition_kind`, the same `constituents` set (as an unordered
set), and the same canonical form (sorted `(RelationshipId,
direction)` for Joinset; sorted `Vec<DataKindRef>` for Unionset) are
**equivalent** by canonical form. The implicit-explicit clash check
(§10.6) detects when an `Origin::Explicit` composition has the same
canonical form as an enumerable `Origin::Implicit` and rejects it
(`COMP_E_0414`).

**Plan-time semantics are origin-agnostic.** Strategies, the
field-first resolver, and adapter rendering all operate on the
struct shape; they do not branch on `origin`. The axis matters at
compile (enumeration, clash detection) and in author diagnostics
(error messages cite the `name` for explicit, the canonical
`ImplicitId` for implicit).

`#[non_exhaustive]` per I10.

### 5.7 `ImplicitId` — canonical-form hash

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ImplicitId(pub [u8; 32]);
```

A 32-byte content-stable hash of the composition's **canonical form**
— the sorted, normalized, byte-stable encoding of the structural
identity. Hash function is BLAKE3-256 (the same primitive `13 §5`
uses for `SourceHash`); collision-resistance and speed both more than
adequate at v1 scale. The exact byte-encoding is `pub(crate)` (compile
internals); public surface is the 32-byte tag and round-trip equality.

**Canonical form per `composition_kind`:**

- **`Joinset`.** Sorted `Vec<(RelationshipId, Direction)>` — the
  set of `Relationship` traversals, each tagged with its direction
  (forward / reverse). `RelationshipId` is the SemanticManifest-unique
  ID assigned at compile (per `14b §4.2`, stable within one
  SemanticManifest, not across recompiles per `14b OQ-7`). Sort key:
  `(RelationshipId.0, Direction::Forward < Direction::Reverse)`.
- **`Unionset`.** Sorted `Vec<DataKindRef>` — the set of constituent
  top-level kinds covered by the implicit Unionset, each represented
  by its `DataKindName`. Sort key: `DataKindName` lex order.
- **`Grainset`.** Not applicable — Grainset is always `Origin::Explicit`
  in v1.

**Stability properties.**

- **Within one SemanticManifest.** `ImplicitId` is fully stable —
  identical canonical forms always hash to identical bytes.
- **Across recompiles.** `ImplicitId` is **not** stable across
  recompiles, because `RelationshipId` is not stable (per `14b OQ-7`).
  A model that adds a new `Relationship` will renumber existing
  `RelationshipId`s, which changes every `ImplicitId` derived from
  them. This is acceptable — `ImplicitId` is a SemanticManifest-internal
  identity, never persisted outside the artifact.
- **Across runs of the same SemanticManifest.** Stable, because
  `SemanticManifest` is byte-deterministic per `33 §4`'s
  determinism contract.

**Synthetic name derivation.** Compile assigns each implicit
composition a `DataKindName` of the form
`__implicit_{joinset|unionset}_{first-8-hex-chars-of-ImplicitId}`.
The double-underscore prefix and `__implicit_` namespace are
reserved per `11 §3.2` (no author may declare a `DataKindName`
matching this pattern; `ValidateError::ReservedName` fires per
`11 §14.x`). The synthetic name lets the planner address implicit
compositions through the same `name_index` it uses for explicit
ones; collisions on the 8-hex prefix are resolved by extending the
suffix to full 64 hex chars (extremely rare at v1 scale).

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
merge consults the constituent's `SemanticInterface` (from the SemanticManifest's
per-kind index, I8). For `Unionset` and `Grainset` (any `Origin`),
constituents are the top-level `ComplexDataKind` children (flattened
per `12 §8`). For `Joinset` (any `Origin`), constituents are the
author-declared member kinds (`Origin::Explicit`) or the kinds visited
by the canonical-form enumeration (`Origin::Implicit`) plus any
path-intermediate kinds the traversal touches (§10.4).

### 6.2 Name-collision resolution — namespace-aware

When two or more constituents declare a `Semantics` of the same
`SemanticsName` and the same role, the merge rule depends on
`composition_kind`:

**`Unionset` / `Grainset`.** Collisions **unify**: both constituents
promote the shared name into the composed surface under its bare
`UnifiedName`. The `FieldOwnership` is `Shared(vec![A, B])` (§7.3.2).
Per `24` / `22` this is the `UNION ALL` / grain-sharding default.

**`Joinset`.** Collisions **qualify** (regardless of `Origin`): each
contributing constituent promotes its name into the composed surface
under a qualified `UnifiedName` of the form `constituent.name`. The
bare `name` does NOT exist on the composed surface — any Request
referencing the bare form triggers
`PlannerError::AmbiguousCompositionReference` (§14.3, `PLAN_E_0505`)
with the candidate qualifications in the Diagnostic.

**Example.** `orders` and `returns` each declare a `total: Measure`.
Under a `Joinset` composing both — explicit or implicit — the composed
surface exposes `orders.total` and `returns.total` but not a bare
`total`.

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
widening (per `13`), `Joinset` qualifies.

**Rationale.** Promotion produces the ergonomic, "author probably meant
this" surface. Qualification is available as a fallback when the
promotion check fails.

### 6.4 `Measure` / `Metric` surface

**Aggregation-semantics conflict — rejected at compile.** Two
constituents that declare a `Measure` of the same `SemanticsName` with
**different aggregation functions** (e.g. `revenue` declared
`agg: sum` on one constituent, `agg: avg` on another) **cannot** be
unified: `CompileError::CompositionAggregationConflict` (§14.1) fires
during composition materialization, regardless of `Origin` (the
implicit-composition enumerator at compile sees the same constituent
shapes as the explicit path). Authors resolve by renaming one side or
narrowing the implicit Joinset enumeration via filters declared on an
explicit `Joinset` (which differentiates it from the equivalent
implicit form per §10.6).

**Measures on one constituent, queried over the composed surface.**
Permitted. The Measure surfaces under its bare name (no collision) or
qualified name (collision). Planning consults `Cardinality` to decide
whether to pre-aggregate (fanout-safe rewrite per §3.3.2) or emit the
straightforward join when the `Additivity × TemporalShape` interaction
(per `17`) is unsafe — fanout falls to the author's declared shape.

**Metrics.** Aggregation-function conflicts are rejected by the same
rules as Measures. Metrics that reference other Measures / Metrics via
`expr:` are resolved per `14b §6.4`'s compile-time reference-DAG
traversal; if the references span constituents, the implicit
composition's `PathSignature` is recorded on the ResolvedExprEntry for
the Metric and the planner consumes it at plan time.

**Filters.** Filters compose like Dimensions — promotion under
compatibility, qualification otherwise.

### 6.5 Composed-surface keys — declare-or-derive

Every composed surface carries `keys` populated post-compile. The
population rule depends on `composition_kind` and `origin`:

**`Unionset`** — composed-surface keys are the intersection of the
constituents' `Key::Primary` / `Key::Unique` declarations sharing the
same `SemanticsName` and type (per `11 §8.3`). Keys that hold on all
constituents under union semantics are preserved; keys that hold on
some but not all are dropped. Same rule for `Origin::Explicit` and
`Origin::Implicit`.

**`Grainset`** — similar to `Unionset`; keys that hold at all declared
grains survive. Always `Origin::Explicit` in v1.

**`Joinset` (`Origin::Explicit`)** — the author MAY declare `keys` on
the `joinsets:` block. If declared, those keys win. Otherwise, the
composed-surface primary key is **derived** from the anchor
constituent's `Key::Primary` (§13.2). `Key::Foreign` declarations
become internal join conditions (not user-surface keys).

**`Joinset` (`Origin::Implicit`)** — keys are **derived** from the
anchor constituent (the first `DataKindRef` in the canonical
`constituents` order, which corresponds to the canonical-form starting
node). Same rule as the no-keys-declared explicit case; the implicit
form has no author-declaration override.

**Why declare-or-derive (vs Round-1 "implicit = empty").** The unified
Joinset model treats explicit and implicit as the same shape with
different origin. Empty keys on implicit Joinsets would force the
planner into a fallback path for "key-required" plan-time decisions
(deduplication, GROUP BY pins, certain optimizer rewrites). Deriving
keys eliminates the fallback; the derivation is the same logic the
explicit-no-keys-declared path runs. (`Q-COMP-018` closed 2026-04-29.)

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
- **`Joinset`** — the planner elects one constituent as the "canonical
  source" for the field (typically the shortest-path one from the
  Joinset's anchor) and projects through it; other constituents'
  contributions are used for coverage analysis but not projected.
  Same rule for `Origin::Explicit` and `Origin::Implicit`.

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
`Derived` / `Metadata`) but keyed by `(DataKindRef, UnifiedName)` rather than
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

Value: `CoverageVariant::{Native, NullFill, Derived, Metadata}`.

- **`Native`** — the constituent provides the field natively. Derived
  from `FieldOwnership::Native(this_kind)` or
  `FieldOwnership::Shared([..., this_kind, ...])`.
- **`NullFill`** — the constituent does not provide the field; NULL is
  projected. Derived from `FieldOwnership::NullFill([..., this_kind, ...])`
  or from the constituent being listed in a `Shared` ownership where
  its per-Binding `Coverage` records `NullFill`.
- **`Derived`** — rare; the constituent synthesizes the field from other
  fields via an `expr:` unique to that constituent.
- **`Metadata`** — the constituent provides the field as a metadata
  literal (path-token, etc.) eagerly resolved at compile per `15 §5.5`
  / `15 §8`. The composed surface inherits this provenance from the
  constituent's per-Binding `Coverage::Metadata`. Plan-time consumers
  treat a `Metadata` cell as a per-source constant; partial evaluation
  at the `SemanticExpr → PhysicalExpr` lowering boundary may collapse
  composed expressions whose only non-`Native` inputs are `Metadata`
  cells (`15 §1`'s three-stratum note).

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
        Some(Metadata) => Metadata
        None           => NullFill    // not mapped by any source
```

The `Metadata` case folds identically to `Native` / `Derived` (the constituent provides the field; the difference is the read path). `15 §8.4` ratifies that metadata-bound Semantics are fail-fast at compile if any source in the constituent's Binding cannot resolve them — so `compose` never sees a "metadata applicable on some sources, NullFill on others" mix within a single constituent.

For multi-source constituents (a `Unionset` / `Grainset` constituent
within a `Joinset`), the fold across sources follows the constituent's
own composition rules (per `22` / `24`) first, then the result is
re-folded into the outer composition. Authors of nested compositions
(e.g. `Joinset` over `Unionset`) can trace each field's provenance back
to the originating source via this cascade.

**Use by planner.** `CompositionCoverage` drives per-Scan column
inclusion and NULL-projection decisions. A Request selecting only
`Native`-covered fields can prune `NullFill`-only constituents from the
plan entirely under `composition_kind == Joinset` (regardless of
`Origin`); under `Unionset` / `Grainset` all constituents participate
regardless.

## 9. Explicit vs Implicit Composition

**Ratified (Q3, revised 2026-04-29).** Composition has a single
structural model (`composition_kind` × `Origin`, §5) and two
provenance flavours:

- **`Origin::Explicit`** — author-declared `joinsets:` /
  `unionsets:` / `grainsets:` blocks in the `SemanticModel`. The
  author owns the name and any per-leg overrides / filters / keys.
- **`Origin::Implicit`** — compile-enumerated from declared
  `Relationship`s (Joinset) or coverage overlap (Unionset). Compile
  produces the same `ResolvedJoinset` / `ResolvedUnionset` shape;
  the synthetic `DataKindName` and `ImplicitId` are assigned per
  §5.7. No author overrides — defaults from the underlying
  `Relationship` declarations.

Both flavours land in the SemanticManifest (§10) and are addressable
through the same `name_index`. Plan-time field-first resolution (§11)
is a pure lookup over both forms.

### 9.1 Boundary rules

1. **Implicit Joinsets only over declared `Relationship`s.** Compile
   never enumerates a join over an anonymous predicate. If a Request
   requires a join that no `Relationship` declares — and no implicit
   Unionset covers the field set — the planner emits
   `PlannerError::NoCompositionPath` (§14.3, `PLAN_E_0501`).
2. **Path ambiguity errors at plan time.** When multiple implicit
   Joinsets cover the same Request constituent set with equal
   shortest-path cost (e.g. `customer → billing_address → city` vs
   `customer → shipping_address → city`), compile enumerates **both**
   as distinct implicit Joinsets (different `ImplicitId`s); the
   planner detects the ambiguity at lookup time and emits
   `PLAN_E_0500 AmbiguousImplicitComposition` with both candidate
   `DataKindName`s in the diagnostic. Authors disambiguate by
   declaring a differentiated explicit `Joinset` (per-leg overrides,
   filters, or `keys` make the canonical form distinct from any
   enumerable implicit form, side-stepping the §10.6 clash).
3. **Coverage ambiguity → implicit `Unionset`.** When N independent
   top-level kinds (no `Relationship` between them) cover the same
   Semantics, compile enumerates an implicit `Unionset` over those
   kinds (§10.5). The planner addresses it through the standard
   field-first lookup; no error.
4. **Depth-limited to `MAX_IMPLICIT_COMPOSITION_DEPTH` hops.** Q-COMP-001
   closed 2026-04-28 ratifies `4` hops for v1. Compile does not
   enumerate implicit Joinsets requiring a longer path. Requests that
   would need a longer path fall through to
   `PlannerError::CompositionDepthExceeded` (§14.3, `PLAN_E_0502`).
   Authors wanting deeper compositions declare an explicit `Joinset`.
5. **Hard cap `MAX_IMPLICIT_ENUMERATION_COUNT = 2000`.** Compile
   enumerates at most 2000 implicit Joinsets + Unionsets per Model
   (§10.4). Cap-exceeded → `CompileError::ImplicitEnumerationExploded`
   (§14.1, `COMP_E_0409`). Authors with pathological models tighten
   the implicit graph (declare explicit Joinsets for common subsets,
   remove redundant Relationships, or restructure with `Forward`
   directionality on edges that should not produce implicit walks).
6. **No synthesis across `Directionality::Forward`.** A `Forward`
   relationship is walked only `from → to` during enumeration. An
   implicit walk requiring reverse direction is dropped from the
   enumeration; explicit `Joinset` declarations attempting reverse
   walk fail with `PlannerError::CrossCompositionForbidden` (§14.3,
   `PLAN_E_0503`).
7. **Transparent unfolding through composed surfaces.** Compile's
   implicit-Joinset enumeration walks the unfolded graph — a
   `Unionset` or `Joinset` constituent is treated as the union of its
   own constituents during enumeration. The previous Round-1
   prohibition on chaining (`PLAN_E_0504
   CompositionChainingForbidden`) is **retired** (2026-04-29). The
   implicit-explicit clash check (§10.6) prevents the unified
   compositions from accidentally degenerating into duplicates.
8. **Implicit-explicit clash → `COMP_E_0414`.** An author-declared
   explicit `Joinset` whose canonical form (sorted
   `(RelationshipId, direction)` tuples) matches an enumerable
   implicit Joinset is **rejected at compile** with
   `CompileError::ExplicitImplicitCompositionClash` (§14.1,
   `COMP_E_0414`, §10.6). Authors differentiate the explicit form
   via per-leg `JoinType` overrides, filters, or `keys`; otherwise
   the planner uses the equivalent implicit Joinset (the explicit
   declaration is redundant).
9. **`Request.from = Some(DataKindRef)` skips field-first.** The
   named kind — Simple, explicit Complex, or implicit Complex
   (addressed by synthetic name) — is looked up directly. No
   re-resolution.

### 9.2 Implicit-Joinset enumeration sketch

At compile, after `RelationshipGraph` construction:

1. **Seed** with every pair `(A, B)` such that `A` and `B` are
   top-level kinds connected by at least one `Relationship`.
2. **Expand** each seed by walking outward up to
   `MAX_IMPLICIT_COMPOSITION_DEPTH` hops, respecting `Directionality`.
   Every reachable subset of size 2..(`depth + 1`) becomes a
   candidate.
3. **Canonicalize** each candidate by sorting its
   `(RelationshipId, direction)` tuples; duplicates collapse.
4. **Hash** the canonical form into `ImplicitId` (§5.7).
5. **Materialize** as `ResolvedJoinset { origin: Implicit { id }, … }`
   per `33`.
6. **Cap-check** at each addition: cumulative count >
   `MAX_IMPLICIT_ENUMERATION_COUNT` → `COMP_E_0409`.

The full algorithm is in §10.4. The pre-clash check (§10.6) runs
before materialization to reject explicit Joinsets with matching
canonical forms.

### 9.3 Implicit-Unionset enumeration sketch

At compile, after per-kind `Coverage` derivation:

1. **Build the inverse-coverage map**: for every `SemanticsName`,
   the set of top-level kinds that cover it natively or via
   `Derived` / `Metadata`.
2. **Identify coverage groups**: every set of 2+ kinds that all
   cover the same `SemanticsName` and are NOT connected by any
   `Relationship` form a candidate implicit Unionset.
3. **Canonicalize** by sorting `Vec<DataKindRef>`; duplicates
   collapse.
4. **Hash** the canonical form into `ImplicitId` (§5.7).
5. **Materialize** as `ResolvedUnionset { origin: Implicit { id }, … }`.
6. **Cap-check** as in §9.2 step 6.

Full algorithm in §10.5.

### 9.4 Rationale — eager materialization

**Why eager (vs Round-1 on-demand).** Round-1 had implicit
compositions synthesized at plan time per Request. The unified Joinset
model (2026-04-29) ratified eager materialization: compile enumerates
every implicit Joinset / Unionset within bounds and stores them in the
SemanticManifest. Three reasons:

- **Repeat-Request amortization.** A model serving N Requests with
  similar shape pays plan-time synthesis N times under the on-demand
  model; eager pays the cost once at compile and N lookups. At v1
  scale (10s–100s of Relationships, depth ≤ 4, cap 2000), compile
  cost is sub-second; per-lookup cost is microseconds.
- **Manifest-as-single-source-of-truth (I8).** The planner reads
  exclusively from the SemanticManifest. Synthesis at plan time
  conflicts with this — implicit compositions would exist
  ephemerally, never persisted, never inspectable by audit tooling.
  Eager materialization makes the SemanticManifest planner-complete
  for compositions too.
- **Clash detection at compile.** The implicit-explicit clash rule
  (§10.6) requires both forms exist in the same compile pass.
  On-demand synthesis would defer clash detection to plan time
  (worse author UX and harder to diagnose).

**Why bound + cap implicit enumeration.** Unbounded enumeration
admits `O(2^|relationships|)` candidates on dense graphs — a
combinatorial explosion. The depth bound (`4`) covers Kimball-style
analytic patterns; the count cap (`2000`) protects against
pathologically connected models. Above the cap, compile errors
(`COMP_E_0409`) and the author tightens the model.

**Why path-ambiguity errors at plan time.** I4 (determinism). When
two implicit Joinsets of equal shortest cost cover the same Request
constituent set, the planner cannot pick without author intent.
Heuristic tie-breakers (lex-smallest `RelationshipId`, fewest
`ManyToMany`) silently change semantics on Relationship reordering.
Error-on-tie keeps the author-facing contract crisp; the explicit
`Joinset` is the disambiguation surface.

**Why coverage-ambiguity becomes a Unionset, not an error.** A model
that declares two top-level kinds both covering `revenue` (with no
`Relationship` between them) is making the editorial claim "these
are alternative sources of the same semantics." `UNION ALL` with
optional pre-aggregation is the correct compositional answer. The
planner builds the implicit Unionset and the Request resolves
naturally — no author intervention required for the common case.
Where the author wants to suppress this, they remove one of the
constituents from the model or rename to disambiguate semantics.

**Why transparent unfolding through composed surfaces.** Round-1
prohibited chaining (`PLAN_E_0504`). The unified Joinset model
removes the prohibition: compile sees the unfolded graph (composed
surfaces decomposed into their constituents during enumeration), so
"walking through" a `Unionset` to reach a `Simple` kind beyond it is
mechanically the same as walking the underlying `Relationship`s
directly. The clash check (§10.6) prevents accidental duplication.

## 10. Materialization Policy

**Ratified (Q2, revised 2026-04-29):** all compositions —
`Origin::Explicit` and `Origin::Implicit`, `Joinset` and `Unionset`
and `Grainset` — are materialized in the SemanticManifest at compile
time. Plan-time field-first resolution (§11) is a pure lookup over the
pre-built composition index. Implicit enumeration is bounded by depth
(`MAX_IMPLICIT_COMPOSITION_DEPTH = 4`) and a hard count cap
(`MAX_IMPLICIT_ENUMERATION_COUNT = 2000`).

### 10.1 Explicit — materialized

At `compile`, for each declared `ComplexDataKind` (`Unionset` /
`Grainset` / `Joinset`):

- Produce a `ResolvedComplexDataKind` in the SemanticManifest (per `33`)
  with `origin: Origin::Explicit`.
- Compute `ComposedSemanticInterface` carrying:
  - A `composition_kind` matching the declared complex-kind variant.
  - The author-declared `constituents` list (or declared member kinds
    for `Joinset`).
  - A fully-computed `UnifiedSemantics` (namespace-aware merge, §6).
  - A `FieldProvenance` (§7).
  - A `CompositionCoverage` (§8).
  - `traversed_paths` (for `Joinset`).
  - `keys` per §6.5 (declared or derived).

The planner, when presented with `Request.from = Some(<complex-kind-name>)`,
retrieves the `ResolvedComplexDataKind` by name and plans against the
pre-built interface. No merge logic re-runs at plan time.

### 10.2 Implicit — also materialized at compile

At `compile`, after explicit compositions are materialized:

- Run the implicit-Joinset enumeration (§10.4) and implicit-Unionset
  enumeration (§10.5). Each enumerated composition is a
  `ResolvedComplexDataKind` with `origin: Origin::Implicit { id }`.
- Run the implicit-explicit clash check (§10.6) before materialization
  closes; clashes fail compile with `COMP_E_0414`.
- Index every implicit composition under its synthetic `DataKindName`
  (`__implicit_{joinset|unionset}_{first-8-hex-of-id}`, §5.7) and
  under its `ImplicitId`. Both indices are addressable on the
  SemanticManifest.
- The planner addresses implicit compositions via the same lookup
  path as explicit ones.

### 10.3 Rationale

(See §9.4 for the detailed why-eager argument.) In summary: eager
materialization amortizes synthesis cost across Requests, preserves
SemanticManifest-as-source-of-truth (I8), enables compile-time clash
detection (§10.6), and is bounded by the depth + cap so combinatorial
explosion is ruled out.

### 10.4 Implicit-`Joinset` enumeration

Algorithm at `compile`, run after `RelationshipGraph` construction
(`14b §4.2`) and after explicit compositions are materialized (§10.1):

```text
enumerate_implicit_joinsets(graph, explicit_canonical_forms) -> Vec<ResolvedJoinset>
    candidates: Set<CanonicalForm> = {}
    for each pair (A, B) of top-level kinds connected by ≥ 1 Relationship in graph:
        bfs_from(A, B, max_depth = MAX_IMPLICIT_COMPOSITION_DEPTH)
            for each path discovered (single edge or multi-hop):
                canonicalize(path) -> CanonicalForm
                candidates.insert(CanonicalForm)
    for each |T| in 3..(MAX_IMPLICIT_COMPOSITION_DEPTH + 1):
        steiner_enumerate(graph, |T|, max_depth = MAX_IMPLICIT_COMPOSITION_DEPTH)
            for each cover tree discovered:
                canonicalize(tree) -> CanonicalForm
                candidates.insert(CanonicalForm)
    if |candidates| > MAX_IMPLICIT_ENUMERATION_COUNT:
        emit COMP_E_0409 ImplicitEnumerationExploded { count, cap }
    return candidates
        .filter(|cf| cf NOT in explicit_canonical_forms)  // §10.6 clash check happens here
        .map(|cf| materialize(cf))
```

**Bounds:**

```rust
pub const MAX_IMPLICIT_COMPOSITION_DEPTH:   usize = 4;
pub const MAX_IMPLICIT_ENUMERATION_COUNT:   usize = 2000;
```

- `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` — Q-COMP-001 closed 2026-04-28.
  Covers Kimball star / snowflake / galaxy-via-bridge plus a 4-hop
  margin for cross-fact-via-shared-dim cases.
- `MAX_IMPLICIT_ENUMERATION_COUNT = 2000` — Q-COMP-005 closed
  2026-04-29 (revised; was previously the strict-mode advisory
  question, now repurposed for the enumeration cap). Starting v1
  value; revisitable post-v1 with telemetry. Cap-exceeded is a
  compile error, not a silent truncation — silent truncation would
  produce a SemanticManifest where some implicit compositions are
  enumerable and others are not, depending on enumeration order, a
  determinism violation.

**Canonicalization (Joinset):** sort the path / tree's
`(RelationshipId, Direction)` tuples by `(RelationshipId.0,
direction)` where `Direction::Forward < Direction::Reverse`. The
sorted vector is the byte-encoded canonical form input to BLAKE3-256.

**Determinism:** neighbor iteration in `bfs_from` and
`steiner_enumerate` is sorted by `(RelationshipId.0,
direction_flag)`; canonical-form sort is total; `BTreeMap` insertion
order is canonical-form-sorted. The full enumeration is reproducible
byte-for-byte given the same `RelationshipGraph`.

### 10.5 Implicit-`Unionset` enumeration

Algorithm at `compile`, run after per-kind `Coverage` derivation
(`15 §6`) and after the implicit-Joinset enumeration (§10.4):

```text
enumerate_implicit_unionsets(coverage_index, relationship_graph, explicit_canonical_forms)
    -> Vec<ResolvedUnionset>
    inverse_coverage: BTreeMap<SemanticsName, Set<DataKindRef>> = {}
    for each (kind, name) in coverage_index where coverage(kind, name) ∈ {Native, Derived, Metadata}:
        inverse_coverage[name].insert(kind)
    candidates: Set<CanonicalForm> = {}
    for each (semantics, kinds) in inverse_coverage where |kinds| ≥ 2:
        for each subset s ⊆ kinds where |s| ≥ 2:
            if no Relationship exists in relationship_graph between any pair in s:
                canonicalize(s) -> CanonicalForm
                candidates.insert(CanonicalForm)
    if |candidates| + |implicit_joinsets| > MAX_IMPLICIT_ENUMERATION_COUNT:
        emit COMP_E_0409 ImplicitEnumerationExploded { count, cap }
    return candidates
        .filter(|cf| cf NOT in explicit_canonical_forms)  // §10.6 clash check
        .map(|cf| materialize(cf))
```

**Why "no Relationship between any pair in `s`."** If a `Relationship`
exists, the unified Joinset model handles it via implicit-Joinset
enumeration (§10.4). The Unionset path is reserved for genuine
coverage overlap — where the kinds are alternative sources of the
same semantics, not joinable subsets.

**Canonicalization (Unionset):** sort `Vec<DataKindRef>` by
`DataKindName` lex order. The sorted vector is the byte-encoded
canonical form input to BLAKE3-256.

**Cap is shared with §10.4.** The enumeration cap counts implicit
Joinsets + Unionsets together; the budget is per Model.

### 10.6 Implicit-explicit reconciliation — REJECT clashes

A canonical form `cf` appearing in both the **explicit composition
set** (declared `joinsets:` / `unionsets:` materialized in §10.1) and
the **implicit candidate set** (§10.4 / §10.5) is a **clash**. Compile
rejects with:

```text
CompileError::ExplicitImplicitCompositionClash {
    explicit_name:    DataKindName,
    canonical_kind:   CompositionKind,
    candidate_differentiators: Vec<&'static str>,
}
```

The Diagnostic message:

> Explicit `<kind>` `<name>` has the same canonical form as an
> enumerable implicit composition. Either remove the explicit
> declaration (the planner will use the equivalent implicit
> composition automatically) or differentiate by:
> - declaring per-leg `JoinType` overrides
> - adding `filter:` predicates
> - declaring `keys` that diverge from the anchor's primary key
> Implicit compositions enumerated against this canonical form are
> indexed under synthetic name `<__implicit_…>` (`ImplicitId`:
> `<hex>`).

**Why reject (not collapse, not coexist).** The user ratified
"reject" as the v1 behavior:

- **Collapse** (Round-1 default proposal A) silently substitutes the
  explicit name for the implicit one. Risk: an author looking at the
  SemanticManifest sees only the explicit form, with no signal that
  the planner would have produced the same shape implicitly. The
  explicit declaration looks load-bearing when in fact it isn't.
- **Coexist** (proposal B) keeps both with distinct names. Risk: the
  field-first resolver finds two candidates with identical canonical
  form and must pick — exactly the ambiguity the rejection avoids.
- **Reject** (chosen) makes the redundancy a compile error. Authors
  who *want* an explicit form must add at least one differentiator;
  those without differentiators learn they can drop the declaration
  and rely on enumeration. This educates author intent and keeps the
  SemanticManifest's composition index minimal.

**`candidate_differentiators` payload.** Up to three suggested
differentiations the author can add to make the canonical form
distinct (e.g. `["join_type override on relationship_id=42",
"filter: orders.status == 'open'", "keys: [orders.id, customers.id]"]`).
The compiler computes these from the `Relationship` set's available
overrides and surfaces them in the diagnostic per the
`ContextLine`-style suggestion pattern (`30 §5.3`).

**Edge case — explicit Joinset's canonical form requires a
hop count exceeding `MAX_IMPLICIT_COMPOSITION_DEPTH`.** No clash
fires because the implicit enumeration would never produce that
form. The explicit declaration stands on its own; it's the only path
to the composition.

**Edge case — explicit Joinset declares non-shortest path.** No
clash; non-shortest paths are not enumerated as implicit (§10.4
enumerates shortest only per `Q-COMP-002`). The explicit declaration
is meaningful (it pins a specific non-default path).

## 11. Field-first Resolution Algorithm

The planner's entry point when `Request.from = None`. Per `10 §3.4`,
the `plan` stage consumes the SemanticManifest (I8 — no I/O, no
re-resolution) and produces a `SemanticPlan`. Field-first resolution
runs before plan tree construction to decide *which composition
surface* the plan will be built against.

Under the unified Joinset model (2026-04-29), every viable composition
is pre-materialized at compile (§10). The runtime algorithm reduces to
a **lookup over the SemanticManifest's composition index** — no BFS,
no synthesis, no ambiguity resolution at plan time except the
documented path-ambiguity error (§11.4).

### 11.1 Inputs

- `manifest: &SemanticManifest` — carrying:
  - `name_index: BTreeMap<SemanticsName, Vec<DataKindRef>>` — all
    kinds (Simple + Complex, including implicit Joinsets and
    Unionsets indexed under their synthetic names) that declare or
    expose the name. Per `33 §5.x`.
  - `composition_index: BTreeMap<DataKindName, ResolvedComplexDataKind>` —
    every composition (explicit + implicit) materialized in §10. Per
    `33 §7.2`.
  - `composition_by_canonical: BTreeMap<ImplicitId, DataKindName>` —
    reverse index from `ImplicitId` to the synthetic name. Per
    `33 §7.2`.
  - `composition_by_constituent_set: BTreeMap<BTreeSet<DataKindRef>, Vec<DataKindName>>` —
    for each unordered constituent set, the list of compositions
    covering exactly that set (used by step 11.4 to detect path
    ambiguity). Per `33 §7.2`.
- `request: &Request` — with `request.from = None` and
  `request.select: Vec<SemanticsName>` of length ≥ 1.

### 11.2 Step — Map each selected name to its owning kinds

For each `name` in `request.select`:

```text
let owning = manifest.name_index.get(&name)
    .ok_or(PlannerError::UnknownSemantics { name })?;
```

`owning` is `Vec<DataKindRef>`. The list may include implicit
compositions (under their synthetic names) when the implicit
composition's `UnifiedSemantics` exposes the name. A name with zero
entries is an error (`PLAN_E_0508 UnknownSemantics`).

Let `T = ⋃ owning` for all selected names — the set of **candidate
owning kinds** for the Request, restricted to top-level kinds (we
filter out the synthetic implicit-composition names at this step;
they re-enter at §11.4 via the constituent-set lookup).

### 11.3 Step — Single-kind fast path

If `|T| == 1`: the Request is satisfiable from a single top-level
kind. The planner takes that `DataKindRef` and plans against its
`SemanticInterface` (Simple) or its pre-materialized
`ComposedSemanticInterface` (explicit Complex). The `Request` is
treated as if `from: Some(T[0])` had been declared.

This fast path dominates well-authored Models where most Requests
target a single fact-like `DataKind` with co-located dimensions.

### 11.4 Step — Composition lookup (multi-kind path)

If `|T| >= 2`: look up the implicit / explicit composition that
covers `T`.

```text
let candidates = manifest.composition_by_constituent_set
    .get(&BTreeSet::from(T))
    .cloned()
    .unwrap_or_default();
```

**Outcome cases:**

- **`|candidates| == 1`** — exactly one composition (explicit or
  implicit) covers `T`. The planner uses it as the resolved target.
- **`|candidates| == 0`** — no composition covers exactly `T`. Two
  sub-cases:
  - **Connectable but not enumerated** — the kinds are connected by
    `Relationship`s but no implicit Joinset was enumerated for this
    exact `T` (e.g. depth bound at compile dropped it). Error:
    `PlannerError::NoCompositionPath` (§14.3, `PLAN_E_0501`) — author
    declares an explicit Joinset to escape the depth bound.
  - **Disconnected coverage** — no implicit Unionset was enumerated
    either (e.g. coverage groups did not include `T` exactly because
    one of the kinds is a partial-coverage outlier). Same error:
    `PLAN_E_0501` cites the kinds with no shared composition path.
- **`|candidates| >= 2`** — multiple compositions of equal canonical
  cost cover `T` (path ambiguity, e.g. billing-vs-shipping address).
  Error: `PlannerError::AmbiguousImplicitComposition` (§14.3,
  `PLAN_E_0500`) cites every candidate's `DataKindName` and, for
  implicit candidates, the `ImplicitId` and the
  `Relationship`-traversal differences. Authors disambiguate by
  declaring an explicit `Joinset` with at least one
  differentiator (per §10.6's `candidate_differentiators` pattern),
  which makes the explicit form's canonical distinct from the
  ambiguous implicit ones — the implicit Joinset against which the
  explicit shadowed form was disambiguated remains in the index for
  Requests that *don't* select fields requiring the differentiator.

(Path ambiguity at plan time is the v1 surface for `Q-COMP-002`'s
"error-on-tie" decision; the implicit enumeration at compile produces
both candidates as distinct, deferring the ambiguity to plan-time
field-first lookup.)

**Sub-step — depth-bound exception path.** If a Request's `T` would
require a hop count greater than `MAX_IMPLICIT_COMPOSITION_DEPTH`
(reached only via a path no implicit Joinset was enumerated for),
the planner detects this from the absence of the constituent set in
`composition_by_constituent_set` and the *presence* of the
constituents in the `RelationshipGraph` connected component
distance > bound. Error: `PlannerError::CompositionDepthExceeded`
(§14.3, `PLAN_E_0502`). Authors declare an explicit Joinset to escape.

**Sub-step — directionality violation.** A `Forward`-only
`Relationship` traversed in reverse is dropped at compile from
implicit enumeration. A Request whose constituent set requires that
reverse traversal will surface as `PLAN_E_0501 NoCompositionPath` (no
implicit composition was enumerated) — the underlying diagnostic
hint cites the `Forward`-only `Relationship` and references
`PLAN_E_0503 CrossCompositionForbidden` for the explicit-`Joinset`
escape path.

### 11.5 Step — Final selection and surface handoff

After lookup yields exactly one composition (Explicit or Implicit),
the planner:

1. Retrieves the `ResolvedComplexDataKind` from
   `manifest.composition_index`.
2. Performs a selected-name membership check: every `name` in
   `request.select` must exist on the resolved composed surface
   (`SemanticsView`). Missing → `PLAN_E_0507 SemanticsNotOnSurface`.
3. Hands the surface (and `traversed_paths` for Joinsets) to the
   strategy dispatcher (`34 §7`).

No `ComposedSemanticInterface` synthesis occurs at plan — the resolved
surface is directly the one materialized at compile.

### 11.6 Step — `Request.from = Some(DataKindRef)` path

When `Request.from` is specified, field-first resolution does NOT run.
Instead:

1. Look up the named `DataKindRef` in the SemanticManifest. The lookup
   may resolve to a Simple kind, an explicit Complex kind, or — if
   the author types a synthetic name — an implicit composition.
2. If the kind is `Simple`, plan against its `SemanticInterface`.
3. If the kind is `Complex`, plan against its pre-materialized
   `ComposedSemanticInterface` (§10.1).
4. **Selected-name membership check.** Every `name` in `request.select`
   must exist on the resolved (possibly composed) surface. A name not
   present triggers `PlannerError::SemanticsNotOnSurface`
   (§14.3, `PLAN_E_0507`). The check is per-name against `SemanticsView`
   on the resolved interface.

Authors using explicit `from:` have opted into a fixed surface; the
planner honours it. Synthetic implicit-composition names are
addressable but unstable across recompiles (per §5.7) — we recommend
authors who want a stable name declare an explicit `Joinset` with at
least one differentiator.

### 11.7 Interaction with `14b`'s cross-kind path resolution

`14b §4`'s BFS runs at `compile` time to produce `PathSignature`
entries on the `ResolvedExprTable` — one per cross-kind reference
inside any declared `expr:`. The implicit-Joinset enumeration in
§10.4 is structurally similar (same `RelationshipGraph`, same
neighbor-iteration order, same depth bound, same canonicalization),
but it explores **all** paths up to the depth bound — not just the
ones referenced by an expression.

Both share: the neighbour-iteration order, the tie-break-by-error
policy, the depth bound, and the `RelationshipGraph` infrastructure
(built once at `compile`).

Plan-time field-first resolution (§11) is a pure lookup over the
pre-built indices — no BFS, no synthesis, no graph traversal. The
heavy work is moved to compile per §10.

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
compile-time implicit-composition enumeration (§10.4) and the
plan-time field-first lookup (§11) both assume simple-graph edges
without self-loops. Deferred as `[TD-COMPOSITION-SELFJOIN]`. Authors
needing self-joins in v1 declare two distinct `DataKind`s (typically
with the same underlying `Binding`) and a `Relationship` between them.

### 12.4 `Cardinality` / `JoinType` consistency

Soft check — emits structural advisory `PLAN_W_0503
RelationshipCardinalityKeyMismatch` (§14.4) when declared
`Cardinality` contradicts `Key::Primary` / `Key::Unique` declarations
on the key sides. v1 has no intent-level fanout advisories — fanout is
the natural consequence of the relationship's declared cardinality and
the author owns it.

### 12.5 Graph connectivity — observational only

`validate` does NOT reject disconnected `RelationshipGraph`s. A Model
can legitimately have several disconnected subgraphs (multiple
business domains under one SemanticManifest). Disconnection only matters at
plan time, when a Request's selected names span disconnected owners
(`PlannerError::NoCompositionPath`, §14.3).

## 13. `Joinset` — Explicit and Implicit

`Joinset` (per `12 §5`, detailed in `23`) is the
`Relationship`-mediated horizontal composition kind. The unified model
(2026-04-29) gives every `Joinset` an `Origin` axis (§5.6):

- **`Origin::Explicit`** — author-declared `joinsets:` block. The
  author owns the name, anchor, traversal order, and any per-leg
  `JoinType` overrides / filters / `keys`.
- **`Origin::Implicit`** — compile-enumerated from declared
  `Relationship`s up to the depth bound (§10.4). Synthetic name +
  `ImplicitId` per §5.7. Defaults from the underlying `Relationship`
  declarations.

Both forms produce the same `ResolvedJoinset` shape (per `33 §4.5`)
and are addressable via the same field-first lookup (§11).

### 13.1 Role in the composition hierarchy

- **Where `Relationship` is the edge**, `Joinset` is the **named or
  enumerated walk**. Explicit Joinsets carry an author-declared name;
  implicit ones carry a synthetic name derived from the canonical-form
  hash.
- **Both forms are persistent.** Both appear in the SemanticManifest
  as `ResolvedJoinset` entries and survive across plan calls. Plan
  cost is a lookup, not synthesis.
- **Differentiation on canonical form (§10.6).** An explicit Joinset
  whose canonical form matches an enumerable implicit Joinset is
  **rejected at compile** (`COMP_E_0414`). Authors who declare an
  explicit `Joinset` are committing to at least one differentiator —
  per-leg `JoinType` override, `filter:`, declared `keys`, or a
  non-shortest path — that makes the canonical form distinct.
- **Depth bound applies only to implicit.** Explicit Joinsets can
  declare arbitrarily deep traversals (no `MAX_IMPLICIT_COMPOSITION_DEPTH`
  cap on author-declared paths) and may pick non-shortest paths
  among alternatives.

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
  NULL-padding for enrichment overrides to `Left`. (Forward-ref:
  `AsOf` override is admitted as a post-v1 additive MINOR after the
  implicit-synthesis milestone — Q-TEMPORAL-003 closed Option B
  2026-04-28; see `17 §5.5`.)
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

### 13.5 Explicit-implicit reconciliation — clash rejection

Under the unified Joinset model, an explicit `Joinset` whose canonical
form (sorted `(RelationshipId, Direction)` tuples per §5.7) matches
an enumerable implicit `Joinset` is **rejected at compile** with
`CompileError::ExplicitImplicitCompositionClash` (§14.1, `COMP_E_0414`).
Per §10.6, the author-facing message lists candidate
differentiators and recommends either dropping the explicit
declaration (the planner will use the equivalent implicit form) or
adding at least one differentiator.

**Differentiator menu** (any one suffices to make the canonical form
distinct):

- **Per-leg `JoinType` override.** The explicit Joinset declares a
  `join_type` on at least one traversed `Relationship` that differs
  from the `Relationship`'s declared `join_type` (per §13.3). The
  canonical form's `JoinType` byte differs.
- **`filter:` predicate.** The explicit Joinset declares a
  `filter:` constraint per `12 §5`. The canonical form includes the
  filter's serialized `PhysicalExpr` byte hash.
- **Declared `keys`.** The explicit Joinset overrides §6.5's derived
  keys with author-declared ones. The canonical form includes the
  declared `keys` byte hash.
- **Non-shortest path.** The explicit Joinset traverses a path with
  hop count strictly greater than the implicit Joinset's
  shortest-path enumeration would produce for the same constituent
  set. Implicit enumeration generates only shortest paths, so a
  longer-path explicit Joinset has no implicit twin.

This replaces Round-1's "no reuse" rule (formerly tracked as
`[TD-COMPOSITION-JOINSET-REUSE]`, retired 2026-04-29 — `Q-COMP-012`
closed).

**Explicit `Relationship`s that reference a `Joinset`.** `§2.1`
permits `Relationship`s whose `from` or `to` is a `Joinset`. The
`KeyPair.left` / `.right` references a namespaced `SemanticsName`
within the composed surface (e.g. `"order_details.customer_id"`). Such
`Relationship`s are declarable and the implicit-Joinset enumeration
(§10.4) walks transparently through the composed kind by treating it
as the union of its constituents during canonical-form construction —
the prior Round-1 prohibition on chaining is retired (§9.1 bullet 7).
See `Q-COMP-013` (closed 2026-04-29).

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
| `ImplicitEnumerationExploded { count, cap }` | `COMP_E_0409` | `§10.4` / `§10.5` — implicit Joinset + Unionset enumeration exceeded `MAX_IMPLICIT_ENUMERATION_COUNT`. Author tightens the implicit graph (declare explicit Joinsets for common subsets, restructure Relationships, or add `Forward` directionality). |
| `ExplicitImplicitCompositionClash { explicit_name, canonical_kind, candidate_differentiators }` | `COMP_E_0414` | `§10.6` / `§13.5` — explicit `Joinset` / `Unionset` canonical form matches an enumerable implicit composition. Author either drops the explicit declaration or adds a differentiator (per-leg `JoinType` override, `filter:`, declared `keys`, or non-shortest path). |

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
| `AmbiguousImplicitComposition { constituent_set, candidates }` | `PLAN_E_0500` | `§11.4` / `§9.1` bullet 2 — two or more compositions of equal canonical cost cover the same Request constituent set (path ambiguity, e.g. billing-vs-shipping). `candidates: Vec<DataKindName>` cites every conflicting composition (explicit or implicit, with `ImplicitId` for the latter). |
| `NoCompositionPath { from, to }` | `PLAN_E_0501` | `§11.4` — no composition (explicit or implicit) covers the Request constituent set. Includes hints when a `Forward`-directionality `Relationship` was the cause. |
| `CompositionDepthExceeded { from_kinds, max_depth }` | `PLAN_E_0502` | `§11.4` / `§9.1` bullet 4 — required walk exceeds `MAX_IMPLICIT_COMPOSITION_DEPTH`; no implicit Joinset enumerated for the constituent set. Author declares an explicit Joinset to escape. |
| `CrossCompositionForbidden { relationship_id, attempted_direction }` | `PLAN_E_0503` | `§11.4` / `§2.4` — explicit `Joinset` declares reverse traversal on a `Forward` `Relationship`. |
| `AmbiguousCompositionReference { name, candidates }` | `PLAN_E_0505` | `§6.2` — Request uses bare name on a composed surface with multiple qualifications. `candidates: Vec<UnifiedName>` carries the valid qualified forms. |
| `CompositionAggregationConflict { name, aggregations }` | `PLAN_E_0506` | `§6.4` — left in place for backward-compat; in v1 this fires only when a Request's selected fields cross composition boundaries in a way the eager materialization could not anticipate (rare). Most aggregation conflicts surface at compile per `COMP_E_0405`. |
| `SemanticsNotOnSurface { name, surface }` | `PLAN_E_0507` | `§11.6` — Request's `from:` is set but selected name is not on the resolved surface. |
| `UnknownSemantics { name }` | `PLAN_E_0508` | `§11.2` — `Request.select` references a `SemanticsName` not in the SemanticManifest. |

`PLAN_E_0504 CompositionChainingForbidden` is **retired (2026-04-29)**:
the unified Joinset model walks transparently through composed
surfaces (`§9.1` bullet 7); the chaining prohibition no longer
applies. The code is reserved for forward-compat (no MINOR
re-allocation).

The `candidates` field on `PLAN_E_0505` carries `Vec<UnifiedName>` of
the valid qualified forms; diagnostic rendering includes one
`ContextLine` per candidate (per `30 §5.3`) with "use this form"
suggestions (per open `Q-COMP-014`, ratified yes).

### 14.4 Advisory (warning) additions

| Variant | Code | When |
|---|---|---|
| `RelationshipCardinalityKeyMismatch { relationship_id, declared_cardinality, inferred_uniqueness }` | `PLAN_W_0503` | `§3.2` — `Cardinality` declared inconsistent with `Key::Primary` / `Key::Unique` on the key sides. **Structural** advisory — the model itself is internally inconsistent. Kept in v1. |
| `CompositionSharedDimensionDescription { composition_name, name, descriptions }` | `COMP_W_0401` | `§6.3` — `Shared` promotion succeeded but constituents' `description` fields differ. **Structural** advisory — kept in v1. |

**Retired advisories (2026-04-29).** Two intent-level fanout
advisories are removed in v1:

- `PLAN_W_0501 FanoutAdvisory` — flagged `OneToMany` / `ManyToOne`
  walks with `SemiAdditive` / `NonAdditive` measures on the fanout
  side. Removed because fanout is the natural consequence of the
  author's declared `Cardinality` and `Additivity`; v1 trusts those
  declarations rather than second-guessing intent. Authors who need
  fanout-detection in their workflow can author a separate audit
  query.
- `PLAN_W_0502 ManyToManyFanoutAdvisory` — fired on every
  `ManyToMany` walk. Removed for the same reason; junction-table
  modeling is a documentation recommendation, not an enforcement
  surface in v1.

Both codes (`0501`, `0502`) are reserved (no MINOR re-allocation) so a
future v2 with telemetry can re-introduce them under `strict` mode if
warranted.

Advisories that remain do NOT abort the pipeline; they are collected
as `Diagnostic` entries alongside the produced plan / manifest.
Consumers (CLI, IDE) render them.

### 14.5 Code range summary

```text
COMP_E_04xx     validate + compile composition errors (§12, §13, §10.4–§10.6)
                in v1: 0401–0409, 0410–0413, 0414
                reserved (forward-compat): 0415–0499
PLAN_E_05xx     plan-time composition errors (§11)
                in v1: 0500–0503, 0505–0508
                retired:  0504 (CompositionChainingForbidden)
                reserved (forward-compat): 0509–0599
PLAN_W_05xx     plan-time composition advisories (§3.2, §12.4)
                in v1: 0503
                retired in v1 (reserved for future): 0501, 0502
                reserved (forward-compat): 0504–0599
COMP_W_04xx     compile-time composition advisories (§6.3)
                in v1: 0401
                reserved (forward-compat): 0402–0499
```

Per `30 §5.1`, there is no central error-code allocation table;
identification is by typed-kind variant. `16` allocates the
composition-specific code ranges with headroom for future additions.

## 15. Interaction with Other Documents

`16`'s ratifications feed and consume several neighbours:

### 15.1 `14b` — path signatures

`14b §4.2`'s `RelationshipGraph` is the shared infrastructure both
documents consume at compile: `14b` for expression cross-kind
resolution, `16 §10.4 / §10.5` for implicit-composition enumeration.
`14b §4.5`'s `PathSignature` (`Vec<RelationshipId>`) is
subset-consistent with `16`'s `traversed_paths` on a composed surface
— for every `PathSignature` entry inside an expression on a composed
Request, the path is covered by the composition's `traversed_paths`.

Plan-time, `14b` consumers and `16`'s field-first resolver (§11)
both read pre-built indices from the SemanticManifest — no graph
traversal at plan.

`16` ratifies what a `Relationship` **is**; `14b`'s `PathSignature`
`Vec<RelationshipId>` is meaningful against that ratification. Changes
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
- `33` (SemanticManifest) persists `ResolvedRelationship`,
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
| Q2 | Materialization policy for composed interfaces (**resolves open item (ii) from `00 §4.1`**) — *revised 2026-04-29* | All compositions (explicit + implicit, all `composition_kind`s) materialized at compile in the `SemanticManifest`. Implicit enumeration bounded by `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` and capped at `MAX_IMPLICIT_ENUMERATION_COUNT = 2000`. Plan-time is pure lookup. | §10 |
| Q3 | Scope of implicit composition (**resolves open item (iii) from `00 §4.1`**) — *revised 2026-04-29* | Only over declared `Relationship`s (Joinset) or coverage overlap (Unionset). Path ambiguity → `PLAN_E_0500` at plan time. Coverage ambiguity → implicit `Unionset`. Depth-limited (4 hops). Hard cap (2000). Transparent unfolding through composed surfaces. Implicit-explicit clash → `COMP_E_0414`. | §9.1, §10.6 |
| Q4 | `Relationship` placement | Global top-level blocks in the `SemanticModel` (not inside any `DataKind`). Visible at `Root` scope per `11 §2`. | §2.1 |
| Q5 | `KeyPair` shape | Positional pairs — `Vec<KeyPair { left: SemanticsName, right: SemanticsName }>`. Both sides must resolve to `Key` or `Dimension` role; `Measure` / `Metric` / `Filter` rejected. | §2.3 |
| Q6 | Composite key ordering | Positional; `keys[i].left` pairs with `keys[i].right`. Multiple `KeyPair` entries represent one composite join condition under `AND`. | §2.3 |
| Q7 | `Directionality` variants | Two variants in v1: `Bidirectional` (default) and `Forward`. `#[non_exhaustive]` per I10. | §2.4 |
| Q8 | `Cardinality` variants | Four variants: `OneToOne`, `OneToMany`, `ManyToOne`, `ManyToMany`. `#[non_exhaustive]` per I10. Declared, not verified. | §3.1 |
| Q9 | `JoinType` variants (v1) | Four variants: `Inner`, `Left`, `Right`, `Full`. `Semi` / `Anti` / `AsOf` deferred. `#[non_exhaustive]` per I10. | §4.1, §4.3 |
| Q10 | `ComposedSemanticInterface` fields — *revised 2026-04-29* | `composition_kind`, `origin: Origin`, `constituents`, `interface: UnifiedSemantics`, `provenance: FieldProvenance`, `coverage: CompositionCoverage`, `traversed_paths: Vec<RelationshipPath>`. New `origin` field carries `Explicit` vs `Implicit { id }`. | §5.1, §5.6 |
| Q11 | `CompositionKind` variants — *revised 2026-04-29* | Three variants: `Joinset`, `Unionset`, `Grainset`. The Round-1 fourth variant `Relationship` retired; implicit Relationship-mediated compositions are now `Joinset` with `Origin::Implicit`. `#[non_exhaustive]` per I10. | §5.3 |
| Q12 | `UnifiedSemantics` name-collision policy — *revised 2026-04-29* | `Unionset` / `Grainset` unify on compatible names (`FieldOwnership::Shared`). `Joinset` (regardless of `Origin`) qualifies on collision (`constituent.name`). Bare name on collision under qualified form triggers `PLAN_E_0505`. | §6.2 |
| Q13 | `FieldOwnership` variants | `Native(DataKindRef)`, `Shared(Vec<DataKindRef>)`, `NullFill(Vec<DataKindRef>)`, `Derived(PhysicalExpr)`. `#[non_exhaustive]` per I10. | §7.2 |
| Q14 | `CompositionCoverage` shape | Keyed by `(DataKindRef, UnifiedName)` — per-constituent per-name entries. Reuses `15 §6`'s `CoverageVariant` enum (`Native` / `NullFill` / `Derived` / `Metadata`). | §8.2 |
| Q15 | Field-first resolution algorithm — *revised 2026-04-29* | Pure lookup over the SemanticManifest's pre-built `composition_index` and `composition_by_constituent_set`. Single-kind fast path, then constituent-set lookup; path ambiguity (multiple compositions cover same set) → `PLAN_E_0500`. No synthesis at plan. | §11 |
| Q16 | Implicit composition under unified Joinset model — *revised 2026-04-29* | Implicit compositions are first-class: `Origin::Implicit { id: ImplicitId }` on `Joinset` / `Unionset`. Compile enumerates all viable implicit compositions within depth + cap; clash with explicit canonical form → `COMP_E_0414`. Synthetic name `__implicit_…` per §5.7. | §5.6, §5.7, §10.6, §13.5 |
| Q17 | Error-code allocation | `COMP_E_04xx` for compile / validate composition errors; `PLAN_E_05xx` for plan composition errors; `PLAN_W_05xx` for plan advisories; `COMP_W_04xx` for compile advisories. v1 retires `PLAN_W_0501` / `0502` (intent advisories), `PLAN_E_0504` (chaining prohibition). New: `COMP_E_0409` (cap), `COMP_E_0414` (clash). | §14.5 |
| Q18 | Composed-surface keys policy — *new 2026-04-29* | Always populated post-compile. `Unionset` / `Grainset` derive from intersection of constituent keys. `Joinset` (any `Origin`) declares-or-derives from anchor's primary key. Closes Q-COMP-018. | §6.5 |
| Q19 | Implicit-explicit reconciliation — *new 2026-04-29* | REJECT clashes at compile (`COMP_E_0414`). Author must drop the explicit declaration or add a differentiator (per-leg `JoinType` override, `filter:`, declared `keys`, or non-shortest path). | §10.6, §13.5 |

**Round-2 revisit candidates** (not in Q-numbered index; parked as
open questions):

- `Directionality` granularity (`Q-COMP-007`).
- Compile-time vs plan-time reverse-traversal detection (`Q-COMP-008`).
- Composite-key shape alternatives (`Q-COMP-009`).
- `CompositionCoverage` serialization shape (`Q-COMP-010`).
- `PLAN_E_0505` candidate suggestions (`Q-COMP-014`).
- `FieldOwnership::Derived` distinctness (`Q-COMP-015`).
- `ManyToMany` reject-by-default (`Q-COMP-016`).
- YAML-surface default for `JoinType` (`Q-COMP-017`).

**Closed in Round 2 (2026-04-29):**

- `Q-COMP-004` — implicit composition produces `Joinset`-style surface
  (under unified model with `Origin::Implicit`, not the retired
  `CompositionKind::Relationship`).
- `Q-COMP-005` — repurposed: intent advisories `PLAN_W_0501` /
  `0502` retired; new use of the slot ratifies the
  `MAX_IMPLICIT_ENUMERATION_COUNT = 2000` cap.
- `Q-COMP-006` — chaining prohibition retired; transparent unfolding.
- `Q-COMP-011` — `Vec<RelationshipPath>` tree-cover ratified.
- `Q-COMP-012` — `[TD-COMPOSITION-JOINSET-REUSE]` retired; explicit
  Joinsets must differ in canonical form from any enumerable implicit
  Joinset (`COMP_E_0414`).
- `Q-COMP-013` — `Relationship`s between composed kinds permitted;
  unified-model implicit enumeration walks transparently through
  composed kinds.
- `Q-COMP-018` — composed-surface keys are declare-or-derived (always
  populated post-compile).

Deferred-to-v2 tech debt:

- `[TD-COMPOSITION-SEMI-ANTI]` — `JoinType::Semi` / `Anti` variants (§4.3).
- `[TD-COMPOSITION-ASOF]` — `JoinType::AsOf` gated on `17` (§4.3).
- `[TD-COMPOSITION-SELFJOIN]` — self-referencing `Relationship`s (§12.3).
- `[TD-JOINSET-NARY]` — N-ary `Joinset`s (§13.2; owned by `23 §*`).
- `[TD-GRAINSET-IMPLICIT]` — `Origin::Implicit` for `Grainset` (catalog-
  discovered grain hierarchies; §5.6).
- `[TD-COMPOSITION-STEINER-SOLVER]` — polynomial-time exact Steiner
  solver for the implicit-composition multi-target enumeration, gated
  on profiling evidence that brute-force enumeration becomes a hot
  path on pathological Models (§10.4; Q-COMP-003 closed 2026-04-28).
- `[TD-COMPOSITION-FANOUT-ADVISORY]` — re-introduce `PLAN_W_0501` /
  `0502` under a `strict` planner mode if telemetry shows authors
  consistently mis-using fanout in v1.
