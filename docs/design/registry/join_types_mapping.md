---
doc: design/registry/join_types_mapping
status: Living
purpose: Authoritative per-engine mapping of canonical `JoinType` variants and the `AsOf` rewrite-tier matrix
prereqs: [16]
authoritative-for:
  - canonical `JoinType` variants (`Inner`, `Left`, `Right`, `Full`) ↔ engine-native SQL / relational join syntax across DataFusion / DuckDB / Spark (v1) and Snowflake / BigQuery / Substrait (planned)
  - `JoinType::AsOf(AsOfAnchor)` rewrite-tier classification per (anchor, engine) pair (First-class / Structural / Unsupported) — companion summary to `temporal_shape_mapping.md §4`, indexed here by `JoinType`
  - `USING` vs `ON` emission preference and reserved-variant (`Semi`, `Anti`, `Lateral`) roster
  - cardinality-informed adapter hints (`DISTINCT` wraps, `collect_list`-style aggregation rewrites) when `Relationship.cardinality ∈ {OneToMany, ManyToMany}`
  - per-engine outer-join gaps (DuckDB historical `FULL OUTER`, dialect-reference SQLite) with adapter workaround
  - cross-engine idiom library (windowed-ASOF emulation; lateral-join emulation; correlated-subquery fallbacks)
  - `TD-JOIN-*` entries tracking per-engine shortfalls, reserved-variant planning, and deferred sophistication
depends-on:
  - foundations/16_composition.md (`JoinType` enum, `Cardinality`, `PlanNode::Join` carriage, `Directionality` reversal of `Left` ↔ `Right`)
  - foundations/17_temporal_shape.md (`JoinType::AsOf` vocabulary ratification, `AsOfAnchor` families, per-shape-pair legality matrix) — transitive prereq via `16 §4.3`
  - registry/README.md (registry policy, engine coverage, versioning posture)
  - registry/types_mapping.md (sibling exemplar; canonical `DataType` mappings consumed by join key-pair type agreement)
  - registry/functions_mapping.md (sibling exemplar; `ROW_NUMBER` / `QUALIFY` / comparison operators consumed by AsOf structural rewrites)
  - registry/temporal_shape_mapping.md (sibling; authoritative for per-(anchor, engine) AsOf emission — `§4.2` is the canonical rewrite-tier table; this doc is its `JoinType`-indexed companion)
  - apis/35_semstrait_ir.md (`PlanNode::Join.join_type: JoinType` carriage; `AsOf(AsOfAnchor)` payload round-tripping per `17 §5.4`, DEFERRED)
  - apis/36_semstrait_adapter.md (adapter trait; PlanBuilder / Dialect layering that consumes this registry)
---

# Join Types Mapping Catalog

> **Scope.** Authoritative per-engine rendering of every canonical `JoinType` variant ratified in `foundations/16_composition.md §4` — `Inner`, `Left`, `Right`, `Full` — plus the `JoinType::AsOf(AsOfAnchor)` extension ratified in `foundations/17_temporal_shape.md §5.1` (implementation-DEFERRED per `17 §10`). This document is a **Living catalog**: entries gain detail, annotations, or additional engine columns as adapters land. It does NOT define new canonical variants, reserved-variant semantics, or anchor families — those live in `16` and `17`. Per `00 §6.6`, canonical specs never depend on this catalog's contents.

> **Status (drafted 2026-04-20):** Round-1 scaffold drafted against `16` Round-1 ratification and `17` Round-1 ratification. Non-temporal variants (`Inner` / `Left` / `Right` / `Full`) are straightforward and drafted complete; `AsOf` emission cross-links to `temporal_shape_mapping.md §3.5` / `§4` which owns the authoritative per-(anchor, engine) table. This catalog is its `JoinType`-indexed companion: a reader who starts with "how does each engine render each `JoinType` variant" lands here; a reader who starts with "how does each engine express each `TemporalShape` or `AsOfAnchor`" lands in `temporal_shape_mapping.md`. Rows marked 🟡 are plausible from engine documentation (DuckDB 1.1.x, DataFusion 40.x+, Spark 3.5.x, Snowflake docs, BigQuery docs, Substrait 0.48+, Apache Calcite reference) but have not been empirically verified against a live adapter test harness. Unresolved items parked in [`questions/open/join_types_mapping_questions.md`](../questions/open/join_types_mapping_questions.md).

---

## 1. Purpose and Scope

### 1.1 What this catalog ratifies

Per `16 §1.2`'s forward-refs to per-engine emission and `00 §6.6`'s Living-catalog policy, this registry is the authoritative home for:

- **Per-engine `JoinType` emission.** How each target engine spells each canonical variant — the SQL keyword (`INNER JOIN`, `LEFT OUTER JOIN`, etc.), the ellipsis rules (`OUTER` optional vs required), and the emit-side preferences (`USING` vs `ON`, `NATURAL JOIN` avoidance).
- **`AsOf` rewrite-tier index, keyed by `JoinType`.** A summary cross-referencing `temporal_shape_mapping.md §4.2`'s authoritative per-(anchor, engine) classification. Useful when a reader's entry-point is `JoinType::AsOf` rather than a specific `TemporalShape`.
- **Reserved-variant roster.** Semi / Anti / Lateral / cross / natural — what semstrait does NOT canonicalize in v1, why, and how an author expresses the intent via ratified alternatives.
- **Cardinality-informed adapter hints.** When `Relationship.cardinality` declares `OneToMany` or `ManyToMany`, which engines admit `DISTINCT`-wrap / `collect_list` / array-aggregation hints the adapter MAY emit (outside of the fanout-safe rewrite owned by `16 §3.3.2`).
- **Gap catalog.** Per engine, the canonical `16` / `17` semantics the engine does NOT natively support, plus the workaround strategy.
- **Cross-engine idioms.** Patterns shared across adapters (windowed-ASOF emulation; lateral-join emulation via correlated subqueries) — written once, referenced per row.

### 1.2 What this catalog does NOT ratify

- **Canonical `JoinType` semantics.** `16 §4.2` is authoritative; this registry documents translation, not redefinition. A conflict between engine-native behavior and `16 §4.2` resolves in favor of `16` — the adapter emits the form that preserves canonical semantics.
- **`AsOf` per-shape legality.** `17 §5.2`'s shape-pair matrix is authoritative; this doc references it and documents emission for the legal pairs.
- **`AsOfAnchor` per-engine emission detail.** That lives in `temporal_shape_mapping.md §3.5` / `§4.2` / `§6.3`. `§3.5` of this doc carries a JoinType-first summary and cross-references the sibling catalog for the structural rewrites.
- **Per-adapter `RegistryExtension` of non-canonical joins.** Adapter crates may register engine-specific join variants (e.g. Spark's `LEFT ANTI`, Snowflake's `MINUS` as an anti-join form) as adapter-extended; inventory lives in each adapter's README per the `TD-JOIN-ADAPTER-INVENTORY` precedent (parallel to `functions_mapping.md §12`).
- **Planner choice between `Inner`, `Left`, `Right`, `Full`** for a given `Relationship` — `16 §4.2` plus author declaration decide; this registry renders whatever the planner hands the adapter.
- **Fanout-safe rewrite algorithm.** Owned by `16 §3.3.2` / `20 §…`; this registry documents the adapter-side hinting (DISTINCT etc.) that composes with the planner's rewrite, not the rewrite itself.

### 1.3 Engine-shortfall posture

Same three-tier response as `temporal_shape_mapping.md §1.3` (in decreasing order of preference):

1. **Structural rewrite** into engine-supported constructs preserving canonical semantics. Example: pre-1.1 DuckDB `FULL OUTER JOIN` emulated via `LEFT JOIN ... UNION ALL ... RIGHT JOIN ... WHERE left IS NULL`.
2. **Warning-plus-degradation** when canonical semantics are preserved but performance characteristics differ. Example: `AsOf` rewrite via `ROW_NUMBER()` on engines without native `ASOF JOIN`.
3. **`ADAPT_E_0302 UnsupportedFeature`** when no correct workaround exists. Reserved for future anchor families on engines that cannot express them; does not arise for `Inner` / `Left` / `Right` / `Full` on any first-class engine.

---

## 2. Engine Roster

Engine coverage mirrors `temporal_shape_mapping.md §2` for cross-catalog consistency. The three first-class compute targets ship in v1; additional engines land as columns / rows when their adapters ratify. Apache Calcite is listed as a **reference** for completeness of the join taxonomy — semstrait does not target Calcite as an execution backend, but Calcite's enumeration of `JoinRelType` variants is the widest industry superset and drives reserved-variant triage in `§3.6`.

| Engine | Version target | Adapter crate | Status | Notes |
|---|---|---|---|---|
| **DataFusion** | 40.x+ 🟡 | `semstrait-adapter-datafusion` (planned) | Round-1 target | Primary reference. All of `Inner / Left / Right / Full` native via `LogicalPlan::Join`; `AsOf` Structural-tier (see `temporal_shape_mapping.md §4.2`). |
| **DuckDB** | 1.1.x 🟡 | `semstrait-adapter-duckdb` (planned) | Round-1 target | All four non-temporal variants native. `FULL OUTER JOIN` landed in 0.3.x (well below floor). Native `ASOF JOIN` since 0.9. |
| **Spark** | 3.5.x 🟡 | `semstrait-adapter-spark` (planned) | Round-1 target | All four non-temporal native. `AsOf` Structural via `QUALIFY` + `ROW_NUMBER` (Spark 3.4+). Dialect preference: `LEFT` over `RIGHT` — see `§3.3.2`. |
| **Snowflake** | 2024.x 🟡 | `semstrait-adapter-snowflake` (planned) | Planned | All four native. Native `ASOF JOIN` since 2023 with `MATCH_CONDITION` syntax. |
| **BigQuery** | 2024.x 🟡 | `semstrait-adapter-bigquery` (planned) | Planned | All four native. No native `ASOF`; `AsOf` Structural via `QUALIFY`. |
| **Substrait** | 0.48+ 🟡 | `semstrait-adapter-substrait` (planned) | Planned | Plan-IR emission. `JoinRel.JoinType` enum covers `UNSPECIFIED / INNER / OUTER / LEFT / RIGHT / SEMI / ANTI / SINGLE / MARK`. `AsOf` emitted via `extension_uri`. |
| **Apache Calcite** | — | — | Reference only | `JoinRelType::INNER | LEFT | RIGHT | FULL | SEMI | ANTI` — cited as the widest mainstream taxonomy; used here to triage reserved-variant decisions in `§3.6`. |

Additional engines (ClickHouse, Trino, Iceberg-SQL-REST, Postgres as a reference dialect) add as columns to `§3` / `§4` / `§7` tables when their adapters land. Adding a column MUST NOT change canonical variants — if an engine lacks native support, the catalog documents the adapter's emulation strategy and files a `TD-JOIN-*` entry.

---

## 3. Per-`JoinType` Mapping

One subsection per canonical variant. Each subsection carries: (a) the canonical definition summary from `16 §4.2`, (b) a per-engine SQL-form table, (c) emission notes covering `USING` vs `ON`, `OUTER` keyword optionality, and any per-engine idioms.

### 3.1 `Inner`

**Canonical summary** (`16 §4.2`): produces rows with a match on **both** sides of the `KeyPair`. Non-matching rows on either side are dropped. Combined with `Cardinality::OneToOne` and non-NULL join columns, the canonical "joined table" semantics.

| Canonical | DataFusion | DuckDB | Spark | Snowflake | BigQuery | Substrait | Tier |
|---|---|---|---|---|---|---|---|
| `Inner` | `INNER JOIN` / `JOIN` | `INNER JOIN` / `JOIN` | `INNER JOIN` / `JOIN` | `INNER JOIN` / `JOIN` | `INNER JOIN` / `JOIN` | `JoinRel { join_type: INNER }` | **First-class** |

**Portability summary: Universal.** Every engine accepts the bare `JOIN` keyword as an alias for `INNER JOIN`.

**Emission notes.**

- **Canonical adapter emission uses `INNER JOIN`** (explicit keyword) for readability even though every first-class engine accepts the bare `JOIN` form. See Q-JOIN-MAP-001 for the explicit-vs-implicit debate; Round-1 posture is explicit.
- **`ON` vs `USING`.** All four SQL engines accept both. When every `KeyPair` in the `Relationship` has `left == right` (same `SemanticsName` on both sides), a `USING` emission is syntactically available and more compact. Round-1 posture: **always emit `ON` form** — the `USING` form masks key-column duplicates in the output schema (Postgres / Spark drop one copy; DuckDB / BigQuery collapse the pair into one column), which complicates downstream projection mapping. See Q-JOIN-MAP-002.
- **`NATURAL JOIN` is never emitted.** Implicit-column matching by name is hostile to semstrait's explicit-key-pairing model (`16 §2.3`'s `KeyPair` requires explicit `SemanticsName` pairing). `NATURAL JOIN` is not in this catalog at all.

### 3.2 `Left`

**Canonical summary** (`16 §4.2`): preserves all rows from `Relationship.from`; NULL-pads the `to` side for unmatched `from` rows. Combined with `ManyToOne`, the canonical "enrich facts with dim, keep unmatched facts" pattern.

| Canonical | DataFusion | DuckDB | Spark | Snowflake | BigQuery | Substrait | Tier |
|---|---|---|---|---|---|---|---|
| `Left` | `LEFT OUTER JOIN` / `LEFT JOIN` | `LEFT OUTER JOIN` / `LEFT JOIN` | `LEFT OUTER JOIN` / `LEFT JOIN` | `LEFT OUTER JOIN` / `LEFT JOIN` | `LEFT OUTER JOIN` / `LEFT JOIN` | `JoinRel { join_type: LEFT }` | **First-class** |

**Portability summary: Universal.** The `OUTER` keyword is optional on every SQL engine; the bare `LEFT JOIN` is universally accepted.

**Emission notes.**

- **Canonical adapter emission uses `LEFT JOIN`** (bare form). All five SQL engines treat this identically to `LEFT OUTER JOIN`. Explicit `OUTER` is legal but verbose; Round-1 preference is bare.
- **`Bidirectional` reversal under `16 §2.4.3`.** When the planner walks a `Left`-declared `Relationship` in reverse under `Directionality::Bidirectional`, the effective emission becomes `Right` (see `§3.3`). The adapter flips the keyword at emission time; the canonical-layer `JoinType` value is unchanged.

### 3.3 `Right`

**Canonical summary** (`16 §4.2`): preserves all rows from `Relationship.to`; NULL-pads the `from` side for unmatched `to` rows. Less common in analytic workloads but symmetric with `Left`.

| Canonical | DataFusion | DuckDB | Spark | Snowflake | BigQuery | Substrait | Tier |
|---|---|---|---|---|---|---|---|
| `Right` | `RIGHT OUTER JOIN` / `RIGHT JOIN` | `RIGHT OUTER JOIN` / `RIGHT JOIN` | `RIGHT OUTER JOIN` / `RIGHT JOIN` | `RIGHT OUTER JOIN` / `RIGHT JOIN` | `RIGHT OUTER JOIN` / `RIGHT JOIN` | `JoinRel { join_type: RIGHT }` | **First-class** |

**Portability summary: Universal.** Every SQL engine supports `RIGHT [OUTER] JOIN` natively.

#### 3.3.1 Dialect-level preference for `LEFT` over `RIGHT`

Several engines recommend rewriting `RIGHT` to `LEFT` at the SQL layer for optimizer-plan consistency:

- **Spark SQL style guide** prefers `LEFT` over `RIGHT` on the grounds that `LEFT JOIN` reads left-to-right aligned with the FROM clause narrative. Spark's Catalyst optimizer normalizes `RIGHT` to `LEFT` internally (by swapping operands) — emitting `RIGHT` directly is semantically identical but inconsistent with the canonical rendering.
- **BigQuery** documentation notes `RIGHT JOIN` is "supported for ANSI compatibility" with a mild nudge toward `LEFT`.
- **DataFusion** accepts `RIGHT` natively; no documented preference.
- **DuckDB** accepts both; no documented preference.
- **Snowflake** accepts both; no documented preference.

**Round-1 posture**: semstrait emits `Right` verbatim as `RIGHT JOIN`. The adapter does NOT automatically rewrite to `LEFT` with swapped operands because:

1. The `Relationship.from` / `.to` orientation is canonical (`16 §2.2`); preserving it in emission preserves the author's intent across debugging / plan-introspection tooling.
2. Engine optimizers normalize internally; the emission form does not change execution cost on any first-class engine 🟡.
3. Authors who want `LEFT`-style narrative swap `from` and `to` at the `Relationship` level (`16 §2.4`).

Tracked as `TD-JOIN-RIGHT-REWRITE` (see Q-JOIN-MAP-003 for the opposite stance — automatic rewrite).

#### 3.3.2 Reversal-under-`Bidirectional`

Per `16 §2.4.3`, a `Left`-declared `Relationship` walked in reverse under `Directionality::Bidirectional` emits as `RIGHT JOIN` (the `to` side becomes the "left" in the FROM clause; the original `from` side becomes the "right" and is preserved). Symmetrically, a `Right`-declared `Relationship` walked in reverse emits as `LEFT JOIN`. `Inner` and `Full` are inversion-symmetric and require no keyword flip. The adapter's per-direction emission logic handles this flip; the canonical-layer `JoinType` value is unchanged.

### 3.4 `Full`

**Canonical summary** (`16 §4.2`): preserves rows from both sides; NULL-pads whichever side lacks a match. Useful for union-like semantics that the shape of a `Unionset` would not express cleanly.

| Canonical | DataFusion | DuckDB | Spark | Snowflake | BigQuery | Substrait | Tier |
|---|---|---|---|---|---|---|---|
| `Full` | `FULL OUTER JOIN` / `FULL JOIN` | `FULL OUTER JOIN` / `FULL JOIN` (DuckDB ≥ 0.3.x) | `FULL OUTER JOIN` / `FULL JOIN` | `FULL OUTER JOIN` / `FULL JOIN` | `FULL OUTER JOIN` / `FULL JOIN` | `JoinRel { join_type: OUTER }` | **First-class** |

**Portability summary: Universal at current engine floors.** All five SQL engines at Round-1 pinned versions support `FULL [OUTER] JOIN` natively.

**Emission notes.**

- **Canonical adapter emission uses `FULL OUTER JOIN`** (explicit `OUTER`) because the bare `FULL JOIN` form is less recognizable in dialects that rarely see the full-outer pattern. The `OUTER` keyword is a readability aid; both forms are accepted.
- **Substrait.** Note that Substrait's `JoinRel.JoinType::OUTER` variant is the full-outer form (not a generic "outer" umbrella). `LEFT` and `RIGHT` are distinct variants; `OUTER` specifically means full-outer.

#### 3.4.1 Historical gaps — dialect-reference only

Engines outside the first-class Round-1 roster historically lacked `FULL OUTER JOIN`. Documented here for dialect-compatibility reference (not a Round-1 concern; semstrait does not emit to these engines in v1):

- **SQLite** — added `FULL OUTER JOIN` in 3.39 (2022-06-25). Pre-3.39 requires the `LEFT ... UNION ALL ... WHERE IS NULL` emulation.
- **MySQL** — no native `FULL OUTER JOIN` (through MySQL 8.x). The `LEFT ... UNION ... RIGHT ... WHERE IS NULL` emulation is the canonical workaround.
- **DuckDB pre-0.3.x** — missing; the adapter floor `1.1.x` is well above this.

When the adapter roster expands to include SQLite / MySQL / similar gap-bearing engines, the per-engine column gains an "emulation" row and `TD-JOIN-FULL-HISTORICAL-DIALECTS` tracks the required adapter scaffolding. The emulation pattern:

```sql
SELECT ... FROM left_table l LEFT JOIN right_table r ON <keys>
UNION ALL
SELECT ... FROM left_table l RIGHT JOIN right_table r ON <keys>
WHERE l.<any_key> IS NULL
```

This is a documented cross-engine idiom; see `§6.3`.

### 3.5 `AsOf` — companion summary

**Canonical summary** (`17 §5.1`): temporal-proximity join matching the most recent `to`-side row whose anchor satisfies the anchor condition relative to the `from`-side probe timestamp. Two anchor families in canonical v1: `AsOfAnchor::ScdWindow` (half-open `[valid_from, valid_to)` match) and `AsOfAnchor::SnapshotLatestAtOrBefore` (latest-`<=`-probe match). Vocabulary-ratified in `17 §5`, implementation-**DEFERRED** per `17 §10`.

**Authority boundary.** `temporal_shape_mapping.md §3.5` and `§4.2` are the **authoritative** per-(anchor, engine) emission tables. This section is a `JoinType`-indexed companion summary for readers who approach the question as "how does each engine render `JoinType::AsOf`" rather than "how does each engine render `TemporalShape × AsOfAnchor`". The two views of the same matrix must stay in sync; when they diverge, `temporal_shape_mapping` wins (per Q-JOIN-MAP-004 Option A).

#### 3.5.1 Rewrite-tier quick reference

🟡 pending `TD-JOIN-ASOF-EMPIRICAL` — parallel to `temporal_shape_mapping.md §4.2`'s `TD-TEMPORAL-ASOF-EMPIRICAL`.

| Anchor | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|
| `AsOfAnchor::ScdWindow` | Structural | First-class (half-open fallback preferred) 🟡 | Structural (`QUALIFY`-based, Spark 3.4+) 🟡 | First-class (half-open fallback preferred) 🟡 | Structural (`QUALIFY`-based) 🟡 |
| `AsOfAnchor::SnapshotLatestAtOrBefore` | Structural | First-class (`ASOF JOIN`) 🟡 | Structural (`QUALIFY`-based, Spark 3.4+) 🟡 | First-class (`ASOF JOIN ... MATCH_CONDITION`) 🟡 | Structural (`QUALIFY`-based) 🟡 |

See `temporal_shape_mapping.md §4.2` for the authoritative tier table with full notes and per-engine rationale, `§3.5.1` for the trailing-closure rationale on native-`ASOF` engines, `§6.3` for the canonical half-open-predicate emission form, and `§6.4` for the `QUALIFY`-vs-subquery-wrap structural-rewrite pattern.

#### 3.5.2 Round-1 emission status

Per `17 §5.1` and `17 §10`, the planner does not emit `JoinType::AsOf(anchor)` in Round 1 and the `32` YAML surface keeps the `join_type:` enum closed at `Inner | Left | Right | Full`. This registry captures the **emission target** adapters will converge on once planner support lands. The cross-referenced `temporal_shape_mapping.md §3.5` / `§4.2` / `§6` rows are equally Round-1 draft; they carry the 🟡 marker pending adapter test-harness verification.

### 3.6 Reserved future variants — DEFERRED

Per `16 §4.3`, three variant families are **deferred from v1**. They appear in Apache Calcite's `JoinRelType` (the widest taxonomy baseline) and in various adapter layers; semstrait keeps them out of canonical until ratified demand surfaces. This subsection documents the reserved roster and the Round-1 alternative authors should use.

#### 3.6.1 `Semi` — DEFERRED

| Calcite | Substrait | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|---|
| `SEMI` | `SEMI` | `LeftSemi`, `RightSemi` (via `LogicalPlan::Join`) | `SEMI JOIN` (DuckDB ≥ 0.7) 🟡 | `LEFT SEMI JOIN` | `SEMI JOIN` / `EXISTS` subquery | `EXISTS` subquery only (no native `SEMI JOIN` keyword) |

**Canonical semantics.** Returns rows from the `from` side where **at least one** matching row exists on the `to` side, **without** duplicating `from` rows for multi-match `to` rows (unlike `Inner` with a fanout). Correctness-preserving optimization of `Inner` + `DISTINCT` on the `from`-side columns.

**Round-1 alternative** (per `16 §4.3`). Authors expressing semi-join intent write a `Filter` constraint referencing an `EXISTS`-style semantics, typically via a computed `Dimension` with a boolean expression. Equivalent result shape; different authoring surface.

**Disposition.** Tracked as `TD-COMPOSITION-SEMI-ANTI` in `16 §4.3`. A future v2 may introduce `JoinType::Semi` as a canonical variant with cross-engine structural rewrite (`EXISTS` subquery on engines lacking the keyword).

#### 3.6.2 `Anti` — DEFERRED

| Calcite | Substrait | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|---|
| `ANTI` | `ANTI` | `LeftAnti`, `RightAnti` | `ANTI JOIN` (DuckDB ≥ 0.7) 🟡 | `LEFT ANTI JOIN` | `ANTI JOIN` / `NOT EXISTS` | `NOT EXISTS` subquery only |

**Canonical semantics.** Returns rows from the `from` side where **no** matching row exists on the `to` side. Correctness-preserving optimization of `Left` + `WHERE right IS NULL`.

**Round-1 alternative.** `Filter` constraint with a `NOT EXISTS`-style expression, or an author-emitted `Left` join plus a filter on `to`-side NULL.

**Disposition.** Same `TD-COMPOSITION-SEMI-ANTI`.

#### 3.6.3 `Lateral` — DEFERRED (adapter-extended candidate)

| Calcite | Substrait | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|---|
| *(not in `JoinRelType`; expressed via `Correlate`)* | *(not a `JoinRel` variant; expressed via subquery correlation)* | *(via `LogicalPlan::SubqueryAlias` + correlation)* 🟡 | `LATERAL` subquery | `LATERAL VIEW` (array / map unnesting) / `LATERAL` subquery (Spark 3.4+) 🟡 | `LATERAL` subquery | `LATERAL` subquery (aka `CROSS JOIN UNNEST`) |

**Canonical semantics.** Row-wise correlated evaluation of a `to`-side subquery, producing zero-or-more rows per `from` row. Primary use-cases: array / struct unnesting, per-row derived subqueries, top-N-per-group pushdown.

**Round-1 stance.** Not a canonical `JoinType` variant; row-wise correlation does not fit `16 §4.2`'s pure-`JoinType` model (which assumes symmetric static operands). Authors needing lateral semantics in v1 either:

1. Emit unnesting explicitly through a `FunctionCall` with array / struct return type (per `14a §7` adapter-extended function registration), or
2. Fold the correlated computation into a `Dimension` / `Measure` `expr:` per `14` / `14a`.

**Disposition.** Tracked as `TD-JOIN-LATERAL-CANONICAL`. Potential adapter-extended registration per engine (Q-JOIN-MAP-005 Option A); no canonical promotion path in v1.

#### 3.6.4 Other variants in Calcite / Substrait — reference only

Calcite's `JoinRelType` is exhausted by `{INNER, LEFT, RIGHT, FULL, SEMI, ANTI}`. Substrait adds `SINGLE` (exactly-one-match) and `MARK` (boolean-indicator join for `EXISTS` rewriting). Both are planner-internal optimizations in their host systems — not author-facing join variants. semstrait does not canonicalize them; a future adapter MAY emit them as rewrites of `Inner` / `Left` + dedup / mark patterns. Tracked as `TD-JOIN-SUBSTRAIT-INTERNAL`.

---

## 4. `AsOf` Rewrite-Tier Summary Table

Per `§3.5`'s authority boundary, this section is a **cross-referenced summary** of `temporal_shape_mapping.md §4.2`. The full per-(anchor, engine) table with notes lives there; this view is indexed by `JoinType::AsOf` for readers starting from the `JoinType` end.

### 4.1 Tier taxonomy (identical to `temporal_shape_mapping.md §4.1`)

| Tier | Description | Adapter action |
|---|---|---|
| **First-class** | Engine exposes native `ASOF JOIN` (or equivalent) expressing the canonical semantics with zero rewrite (or a single trailing filter for `ScdWindow` per `temporal_shape_mapping.md §3.5.1`). | Emit native syntax; optionally append trailing filter. |
| **Structural** | Engine requires window-function + predicate rewrite. Correctness preserved; performance may differ. | PlanBuilder-layer rewrite per `temporal_shape_mapping.md §6.3` / `§6.4`. |
| **Unsupported** | No correct emission path. Hard error: `ADAPT_E_0302 UnsupportedFeature { canonical: "JoinType::AsOf(...)", engine }`. | Fail at `adapt` time. |

Per `16 §4.3` + `17 §5.2`'s legality posture, canonical v1 guarantees **`Unsupported` does not arise** for `ScdWindow` / `SnapshotLatestAtOrBefore` on any first-class engine — every engine admits either First-class or Structural emission.

### 4.2 Per-(anchor, engine) summary

| Anchor | DataFusion | DuckDB | Spark 3.5+ | Snowflake | BigQuery | Adapter responsibility |
|---|---|---|---|---|---|---|
| `AsOfAnchor::ScdWindow` | Structural | First-class (half-open preferred) 🟡 | Structural (`QUALIFY` + `ROW_NUMBER`) 🟡 | First-class (half-open preferred) 🟡 | Structural (`QUALIFY` + `ROW_NUMBER`) 🟡 | See `temporal_shape_mapping.md §6.3` for half-open predicate; `§3.5.1` for trailing-closure rationale. |
| `AsOfAnchor::SnapshotLatestAtOrBefore` | Structural | First-class (`ASOF JOIN ... ON probe >= anchor`) 🟡 | Structural (`QUALIFY` + `ROW_NUMBER`) 🟡 | First-class (`ASOF JOIN ... MATCH_CONDITION`) 🟡 | Structural (`QUALIFY` + `ROW_NUMBER`) 🟡 | See `temporal_shape_mapping.md §6.4` for `QUALIFY` vs subquery-wrap; `§6.2` for `ROW_NUMBER` variant. |

**One-line per engine (cross-ref to `temporal_shape_mapping.md §4.2`):**

- **DataFusion** — Structural for both anchors; no native `ASOF`. `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE`.
- **DuckDB** — First-class for both. `ScdWindow` uses half-open predicate in practice per `temporal_shape_mapping.md` Q-TEMPORAL-MAP-004 Option C.
- **Spark 3.5+** — Structural for both via `QUALIFY ROW_NUMBER() = 1`. Pre-3.4 requires subquery wrap. `TD-TEMPORAL-ASOF-SPARK-NATIVE`.
- **Snowflake** — First-class for both via `ASOF JOIN ... MATCH_CONDITION`.
- **BigQuery** — Structural for both; `QUALIFY` supported natively.

### 4.3 Cross-catalog synchronization

The `§4.2` table duplicates rows present in `temporal_shape_mapping.md §4.2`. Per Q-JOIN-MAP-004 Option A, divergence between the two is a **documentation bug** — `temporal_shape_mapping.md §4.2` is authoritative. A future refactor MAY eliminate the duplication by factoring the tier table into a shared fragment; Round-1 posture tolerates duplication as a navigation aid (readers arriving from either `JoinType` or `TemporalShape` find the tier in their entry-point doc).

---

## 5. Cardinality-informed Emission

`Relationship.cardinality` (ratified in `16 §3`) carries analytic intent about the multiplicity of the join. When `cardinality ∈ {OneToMany, ManyToMany}`, the join may produce row-duplication that the author did not intend; `16 §3.3.2` / `16 §3.3.4` ratify the **planner-level** fanout-safe rewrite (pre-join aggregation). This section documents **adapter-level** emission hints that compose with the planner's rewrite — specifically, idioms for `DISTINCT` wrapping, array aggregation, and deduplication pins.

### 5.1 When the adapter MAY emit `DISTINCT`

The adapter MAY wrap the join output in a `SELECT DISTINCT ...` when:

1. `Relationship.cardinality == ManyToMany` AND
2. The planner's fanout-safe rewrite did NOT fire (i.e. `PLAN_W_0502 ManyToManyFanoutAdvisory` was emitted — `16 §14.4`) AND
3. The composed Request projects only fields that are functionally dependent on the `Relationship.from` primary key.

**Per-engine support.** All five SQL engines natively support `SELECT DISTINCT`; Substrait emits via `AggregateRel` with empty measure list. This is Universal at Round-1 pins.

**Round-1 posture.** Per Q-JOIN-MAP-006 Option A, the adapter does **NOT** automatically emit `DISTINCT` — it is authoring-surface-affecting (changes row count) and risks masking genuine fanout bugs. Authors who want deduplication declare a `Joinset` with explicit deduplication semantics, or submit a Request whose projections trigger `SELECT DISTINCT` through explicit aggregation. `TD-JOIN-CARDINALITY-DISTINCT-AUTO`.

### 5.2 When the adapter MAY emit `collect_list` / array aggregation

For `Cardinality::OneToMany` where the Request wants per-`from`-row aggregation of the `to`-side, some engines admit a more compact `array_agg` / `collect_list` form than the planner-level fanout-safe rewrite:

| Engine | Array-aggregation function | Round-1 adapter posture |
|---|---|---|
| DataFusion | `array_agg(col)` 🟡 | Not emitted automatically. |
| DuckDB | `list(col)` / `array_agg(col)` | Not emitted automatically. |
| Spark | `collect_list(col)` / `collect_set(col)` | Not emitted automatically. |
| Snowflake | `array_agg(col)` / `array_agg(DISTINCT col)` | Not emitted automatically. |
| BigQuery | `ARRAY_AGG(col)` | Not emitted automatically. |

**Round-1 posture.** Same as `§5.1` — these are Semantic-changing projections (produce array-valued output). Only emitted when the author's `Measure` / `Metric` explicitly requests array-valued aggregation via a canonical function in `14a` (Round-2 candidate: `TD-FUNCS-MAPPING-ARRAY-AGG` — parallels to the array-valued-function landscape). The `JoinType` mapping does not drive this emission; it is documented here for cross-reference only.

### 5.3 Cardinality and `JoinType` composition table

Per `16 §12.4`'s soft-check list, certain `(Cardinality, JoinType)` pairs emit advisories. The adapter's emission is unchanged by the advisory — the `JoinType` SQL keyword remains identical regardless of `Cardinality` — but the adapter MAY surface per-engine optimizer hints when available:

| `Cardinality` × `JoinType` | Planner advisory | Adapter-level emission hint |
|---|---|---|
| `OneToOne` + any | None (clean per `16 §3.3.1`) | — |
| `OneToMany` + `Inner` / `Left` | `PLAN_W_0501 FanoutAdvisory` if measure on `from` side is `SemiAdditive` / `NonAdditive` | Snowflake `/*+ BROADCAST(to_side) */`-style hint 🟡 when `to` side is known-small dimension; else none. |
| `ManyToOne` + `Inner` / `Left` | `PLAN_W_0501` if measure on `to` side at mismatched Additivity | Same broadcast hint opportunity for `from` side when dimension-like. |
| `ManyToMany` + any | `PLAN_W_0502 ManyToManyFanoutAdvisory` | Adapter MAY append `/*+ HINT */` per Q-JOIN-MAP-006; Round-1 posture: no automatic hint emission. |

**Engine-specific hint syntax** — reference only, NOT emitted in Round 1:

- **Spark**: `/*+ BROADCAST(t) */`, `/*+ SHUFFLE_HASH(t) */`, `/*+ MERGE(t) */`.
- **Snowflake**: query hints via session parameters; no inline hint syntax.
- **BigQuery**: no inline optimizer hints (optimizer is autonomous).
- **DuckDB**: no inline optimizer hints (optimizer is autonomous).
- **DataFusion**: no inline optimizer hints at the SQL layer (analyzer-pass machinery exists at the `LogicalPlan` layer but is not SQL-surface-exposed) 🟡.

Tracked as `TD-JOIN-CARDINALITY-HINTS`.

---

## 6. Cross-Engine Idioms

Patterns shared across multiple adapters. Written once here; `§3` / `§4` / `§7` rows reference these sections rather than inline the emission.

### 6.1 Windowed-`ASOF` emulation

The canonical Structural-tier emission for `AsOf` on engines without native `ASOF JOIN`. See `temporal_shape_mapping.md §6.2` / `§6.4` for the authoritative forms; summarized here for cross-reference.

**`QUALIFY`-variant** (DuckDB / Spark 3.4+ / Snowflake / BigQuery; preferred when available):

```sql
SELECT e.*, c.*
FROM events e LEFT JOIN customers_scd c
  ON e.customer_id = c.customer_id
 AND c.valid_from <= e.occurred_at
QUALIFY ROW_NUMBER() OVER (
  PARTITION BY e.event_id ORDER BY c.valid_from DESC
) = 1
```

**Subquery-wrap variant** (DataFusion / Spark < 3.4 / ANSI fallback):

```sql
SELECT *
FROM (
  SELECT e.*, c.*,
    ROW_NUMBER() OVER (
      PARTITION BY e.event_id ORDER BY c.valid_from DESC
    ) AS rn
  FROM events e LEFT JOIN customers_scd c
    ON e.customer_id = c.customer_id
   AND c.valid_from <= e.occurred_at
) t
WHERE rn = 1
```

Both forms produce identical result sets; `QUALIFY` is shorter and avoids the explicit subquery scope. Adapter selection per `temporal_shape_mapping.md §6.4`.

### 6.2 Lateral-join emulation via correlated subquery

When lateral semantics are required on an engine with no native `LATERAL` (or when the adapter prefers not to emit `LATERAL` per `§3.6.3`'s reserved-variant posture), the universal emulation is a correlated subquery:

```sql
SELECT e.*, (
  SELECT c.segment
  FROM customers c
  WHERE c.id = e.customer_id
  ORDER BY c.valid_from DESC
  LIMIT 1
) AS segment
FROM events e
```

**Per-engine notes.**

- DataFusion / DuckDB / Spark / Snowflake / BigQuery all support correlated scalar subqueries with `LIMIT 1`.
- The correlation variable (`e.customer_id` inside the subquery) ties the subquery to each outer row — semantically equivalent to a `LATERAL` top-1 per group.
- Round-1 posture: the adapter does NOT emit this form automatically — authors declaring a `Relationship` with an `AsOfAnchor::SnapshotLatestAtOrBefore` get the Structural rewrite path of `§6.1` instead, which is uniform across engines.

### 6.3 `FULL OUTER` emulation via `LEFT UNION RIGHT`

For dialects without native `FULL OUTER JOIN` (historical SQLite, MySQL, older DuckDB) — reference only; not emitted at Round-1 pins:

```sql
SELECT l.*, r.* FROM t1 l LEFT JOIN t2 r ON l.k = r.k
UNION ALL
SELECT l.*, r.* FROM t1 l RIGHT JOIN t2 r ON l.k = r.k
WHERE l.k IS NULL
```

Correctness relies on: the `LEFT` branch contributing all `t1` rows with matching-or-NULL `t2` fields; the `RIGHT` branch contributing the `t2`-only rows with NULL `t1` fields. The `WHERE l.k IS NULL` predicate eliminates duplicates (rows that matched on both sides are captured by the `LEFT` branch). `UNION ALL` (not `UNION`) is correct here because the two branches are guaranteed disjoint.

### 6.4 `DISTINCT` vs `GROUP BY` post-join

When adapters need deduplication after a `ManyToMany` join (per `§5.1` opt-in), two emission forms are available:

```sql
SELECT DISTINCT <cols> FROM <join> WHERE ...
```

vs.

```sql
SELECT <cols> FROM <join> WHERE ... GROUP BY <cols>
```

Semantically identical when `<cols>` have no NULL values on any grouping column (since `GROUP BY` and `DISTINCT` have different NULL semantics on certain engines 🟡). All first-class engines accept both; Round-1 posture: `DISTINCT` form for readability.

---

## 7. Gap Catalog

Per-engine inventory of canonical `16` / `17` semantics the engine does NOT natively support for the `JoinType` axis, plus adapter workaround strategy. Parallels `temporal_shape_mapping.md §5`.

### 7.1 DataFusion gap inventory

| Canonical feature | DataFusion status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(*)` | No native `ASOF JOIN` | Structural rewrite per `temporal_shape_mapping.md §6.4` | `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` (shared) |
| `JoinType::Semi` / `JoinType::Anti` | Natively supported at `LogicalPlan::Join` (`LeftSemi`, `LeftAnti`) but not canonical in v1 | Author uses `EXISTS` / `NOT EXISTS` filter; canonical v2 may promote. | `TD-COMPOSITION-SEMI-ANTI` (shared) |
| `LATERAL` subquery | Supported at `LogicalPlan::SubqueryAlias` 🟡; no SQL-surface keyword | N/A in Round 1; author emits computed Dimension. | `TD-JOIN-LATERAL-CANONICAL` |

**Round-1 posture**: DataFusion is First-class for `Inner / Left / Right / Full`; Structural for both `AsOf` anchor families.

### 7.2 DuckDB gap inventory

| Canonical feature | DuckDB status | Adapter workaround | TD |
|---|---|---|---|
| Historical `FULL OUTER JOIN` gap | Resolved in 0.3.x (well below 1.1.x floor) | — | — |
| `JoinType::Semi` / `JoinType::Anti` | Native `SEMI JOIN` / `ANTI JOIN` keywords 🟡 (DuckDB ≥ 0.7) | Not canonical in v1; if promoted, DuckDB row becomes First-class. | `TD-COMPOSITION-SEMI-ANTI` (shared) |
| `AsOfAnchor::ScdWindow` trailing closure | Native `ASOF JOIN` lacks upper-bound enforcement | Half-open predicate form per `temporal_shape_mapping.md §3.5.1` / Q-TEMPORAL-MAP-004 Option C | `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE` (shared) |

**Round-1 posture**: DuckDB is First-class for all four non-temporal variants AND for both `AsOf` anchor families.

### 7.3 Spark gap inventory

| Canonical feature | Spark status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(*)` | No native | `QUALIFY ROW_NUMBER() = 1` rewrite (3.4+) 🟡; subquery wrap pre-3.4 | `TD-TEMPORAL-ASOF-SPARK-NATIVE` (shared) |
| `RIGHT JOIN` dialect preference | Native but Catalyst normalizes to `LEFT` with operand swap | Emit `RIGHT JOIN` verbatim per `§3.3.1`; optimizer normalizes internally | `TD-JOIN-RIGHT-REWRITE` |
| `JoinType::Semi` / `JoinType::Anti` | Native `LEFT SEMI JOIN` / `LEFT ANTI JOIN` | Not canonical in v1 | `TD-COMPOSITION-SEMI-ANTI` (shared) |
| `LATERAL` (subquery form) | Spark 3.4+ 🟡 | Per-version adapter selection once canonical lateral lands | `TD-JOIN-LATERAL-CANONICAL` |

### 7.4 Snowflake gap inventory (planned adapter)

| Canonical feature | Snowflake status | Adapter workaround | TD |
|---|---|---|---|
| `AsOfAnchor::ScdWindow` | Native `ASOF JOIN ... MATCH_CONDITION` lacks upper bound | Half-open predicate per Q-TEMPORAL-MAP-004 Option C | `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE` (shared) |
| `RIGHT JOIN` | Native; no documented normalization preference | Emit verbatim | — |
| `SEMI` / `ANTI` keywords | Native | Not canonical in v1 | `TD-COMPOSITION-SEMI-ANTI` (shared) |

### 7.5 BigQuery gap inventory (planned adapter)

| Canonical feature | BigQuery status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(*)` | No native | `QUALIFY ROW_NUMBER() = 1` rewrite (BigQuery supports `QUALIFY` natively) | `TD-TEMPORAL-ASOF-SPARK-NATIVE` (structurally shared) |
| `SEMI JOIN` keyword | Absent (use `EXISTS` subquery) | Not canonical in v1; if promoted, Structural via `EXISTS` | `TD-COMPOSITION-SEMI-ANTI` (shared) |
| `ANTI JOIN` keyword | Absent (use `NOT EXISTS`) | Same posture | Same |

### 7.6 Substrait gap inventory (planned adapter)

| Canonical feature | Substrait status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(anchor)` | No native `JoinRel.JoinType` variant | Emit via `extension_uri` with semstrait-specific URI; downstream consumers must understand the extension. | `TD-JOIN-SUBSTRAIT-ASOF-EXT` |
| Reserved variants (`SINGLE`, `MARK`) | Native `JoinRel.JoinType` variants | Not canonical; adapter MAY emit as rewrite of `Inner` + dedup / mark patterns in future. | `TD-JOIN-SUBSTRAIT-INTERNAL` |

---

## 8. Round-1 Open Items

Unresolved questions parked in [`questions/open/join_types_mapping_questions.md`](../questions/open/join_types_mapping_questions.md). Summary:

| Q | Title | Round-1 position | Blocking? |
|---|---|---|---|
| Q-JOIN-MAP-001 | Explicit `INNER` keyword vs bare `JOIN` | Option A — explicit `INNER JOIN` | No |
| Q-JOIN-MAP-002 | `ON` vs `USING` emission form | Option A — always `ON` | No |
| Q-JOIN-MAP-003 | `RIGHT JOIN` auto-rewrite to `LEFT` with swapped operands | Option A — preserve author orientation, emit `RIGHT` verbatim | No |
| Q-JOIN-MAP-004 | `AsOf` tier-table authority (this doc vs `temporal_shape_mapping.md`) | Option A — `temporal_shape_mapping.md §4.2` authoritative; this doc mirrors | No |
| Q-JOIN-MAP-005 | `LATERAL` reserved-variant disposition | Option A — adapter-extended per engine; no canonical promotion | No |
| Q-JOIN-MAP-006 | Cardinality-informed `DISTINCT` / hint auto-emission | Option A — no automatic emission; opt-in via Request shape | No |
| Q-JOIN-MAP-007 | `FULL OUTER` emulation for historical dialects (SQLite, MySQL) | Deferred — not in Round-1 adapter roster | No |

None blocks ratification of this registry; all are coordination items with adapter implementation (`35` / `36`), `16 §4.3` deferrals, or `17 §10` deferrals shared with `temporal_shape_mapping.md`.

---

## 9. Versioning

Following `registry/README.md §versioning-and-churn` and sibling-catalog precedent (`functions_mapping.md §15`, `types_mapping.md §4`, `temporal_shape_mapping.md §8`), each mapping row SHOULD cite the engine version it was verified against. Round-1 tentative pins align with `temporal_shape_mapping.md §8`:

| Engine | Target version(s) | Notes |
|---|---|---|
| DataFusion | 40.x+ 🟡 | Matches sibling catalogs. All four non-temporal variants stable since early DataFusion releases. |
| DuckDB | 1.1.x 🟡 | `FULL OUTER JOIN` since 0.3.x (well below floor); `ASOF JOIN` since 0.9. |
| Spark | 3.5.x 🟡 | `QUALIFY` requires 3.4+; `LATERAL` subquery (if ever canonicalized) requires 3.4+. |
| Snowflake | 2024.x 🟡 | `ASOF JOIN` GA since 2023; `MATCH_CONDITION` syntax stable. |
| BigQuery | 2024.x 🟡 | `QUALIFY` long-available; no version-floor concerns for the four non-temporal variants. |
| Substrait | 0.48+ 🟡 | `JoinRel.JoinType` stable; `extension_uri` mechanism stable. |

Rows citing features behind a specific engine version carry `(Engine X.Y+)` inline. Unverified rows are marked 🟡; as adapter implementation lands and verifies, the 🟡 marker is removed and the exact verified version replaces the range. Breaking changes between engine majors are documented as additional rows or dated annotations — not destructive edits — per `registry/README.md §versioning-and-churn`.

---

## 10. TECH_DEBT Index

Consolidated list of all `TD-JOIN-*` entries surfaced in this catalog plus shared `TD-COMPOSITION-*` / `TD-TEMPORAL-*` entries that this catalog references. Entries retire as adapter implementation, `16` / `17` ratification, or engine-landscape shift resolves them.

| TD ID | § | Canonical concern | Engine(s) | Current posture |
|---|---|---|---|---|
| `TD-JOIN-RIGHT-REWRITE` | `§3.3.1`, `§7.3` | Automatic `RIGHT` → `LEFT`-with-swap rewrite | Spark (Catalyst preference) | Open — Round-1 posture: preserve orientation. Revisit if empirical perf divergence surfaces. |
| `TD-JOIN-FULL-HISTORICAL-DIALECTS` | `§3.4.1`, `§6.3` | `FULL OUTER` emulation scaffolding | SQLite pre-3.39, MySQL, others | Deferred — not in Round-1 roster. |
| `TD-JOIN-LATERAL-CANONICAL` | `§3.6.3`, `§7.1`, `§7.3` | Canonical `Lateral` join variant | All | Open — adapter-extended candidate per Q-JOIN-MAP-005. |
| `TD-JOIN-SUBSTRAIT-INTERNAL` | `§3.6.4`, `§7.6` | Substrait-only `SINGLE` / `MARK` join variants | Substrait | Open — potential adapter-side rewrite target for `Inner` + dedup. |
| `TD-JOIN-SUBSTRAIT-ASOF-EXT` | `§7.6` | Substrait `AsOf` emission via `extension_uri` | Substrait | Open — blocked on planner `AsOf` support per `17 §10`. |
| `TD-JOIN-ASOF-EMPIRICAL` | `§3.5.1`, `§4.2` | Rewrite-tier rows drafted from docs, not empirically verified | All | Shared with `TD-TEMPORAL-ASOF-EMPIRICAL` — blocked on test harness. |
| `TD-JOIN-CARDINALITY-DISTINCT-AUTO` | `§5.1` | Auto-emit `SELECT DISTINCT` on `ManyToMany` | All | Open — Round-1: no auto-emission; author-driven only. |
| `TD-JOIN-CARDINALITY-HINTS` | `§5.3` | Engine-specific optimizer hints (Spark `BROADCAST`, etc.) | Spark (primary); others lack inline syntax | Open — Round-1 posture: no hint emission. |
| `TD-COMPOSITION-SEMI-ANTI` | `§3.6.1`, `§3.6.2`, `§7.*` | `JoinType::Semi` / `JoinType::Anti` canonical promotion | All | Shared with `16 §4.3`. Deferred to v2. |
| `TD-COMPOSITION-ASOF` | `§3.5` | `JoinType::AsOf` canonical variant | All | Shared with `16 §4.3`. Vocabulary-ratified in `17 §5`; planner implementation DEFERRED per `17 §10`. |
| `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` | `§4.2`, `§7.1` | Native `ASOF JOIN` on DataFusion | DataFusion | Shared with `temporal_shape_mapping.md §7`. |
| `TD-TEMPORAL-ASOF-SPARK-NATIVE` | `§4.2`, `§7.3` | Native `ASOF JOIN` on Spark | Spark | Same. |
| `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE` | `§3.5`, `§7.2`, `§7.4` | Half-open predicate preferred on First-class-tier engines | DuckDB, Snowflake | Same. |

---

## 11. Interaction with Other Documents

- **`foundations/16_composition.md`** — canonical upstream. `16 §4` defines the `JoinType` enum (`Inner`, `Left`, `Right`, `Full`); `16 §4.3` defers `Semi` / `Anti` / `AsOf`; `16 §4.4` ratifies `PlanNode::Join` carriage. `16` never depends on this registry's specifics (`00 §6.6`).
- **`foundations/17_temporal_shape.md`** — authoritative for the `JoinType::AsOf(AsOfAnchor)` extension: vocabulary (`17 §5.1`), per-shape-pair legality (`17 §5.2`), cardinality implications (`17 §5.3`), reverse-direction forbiddance (`17 §5.3`), and implementation-DEFERRED status (`17 §10`).
- **`registry/README.md`** — shared policy (engine coverage, versioning, Living status).
- **`registry/types_mapping.md`** — sibling. `KeyPair` type-agreement (per `16 §12.2`) consumes this catalog's type mappings at `compile`; this doc assumes `types_mapping.md §2`'s widening rules as background.
- **`registry/functions_mapping.md`** — sibling. `ROW_NUMBER()`, comparison operators, and `NULLIF`-style constructs consumed by `AsOf` structural rewrites (`§3.5`, `§6.1`). `QUALIFY` is an SQL-clause-level construct (not a function); its per-engine version-floor story is owned by this doc's `§2` and by `temporal_shape_mapping.md §5.3`.
- **`registry/temporal_shape_mapping.md`** — sibling and authoritative co-owner of the `AsOf` rewrite-tier table (`§4.2` there; `§4.2` here is a summary). When the two views diverge, `temporal_shape_mapping.md` wins per Q-JOIN-MAP-004 Option A.
- **`apis/35_semstrait_ir.md`** — `PlanNode::Join.join_type: JoinType` carriage per `16 §4.4`; `AsOf(AsOfAnchor)` payload round-tripping per `17 §5.4` (DEFERRED).
- **`apis/36_semstrait_adapter.md`** — the `EngineAdapter` trait + PlanBuilder / Dialect layering that consumes this registry. Per-adapter crates ratify their own emission paths; `§3` / `§4` / `§7` rows seed each adapter's implementation.
- **Adapter crates** (future `semstrait-adapter-datafusion`, `-duckdb`, `-spark`, `-snowflake`, `-bigquery`, `-substrait`) — own authoritative per-engine emission tables and adapter-extended joins (reserved-variant registrations, optimizer hints).
- **`questions/open/join_types_mapping_questions.md`** — parked unresolved questions surfaced during Round-1 drafting.
