---
doc: design/open_questions/join_types_mapping
status: Round-1 open items
scope: Unresolved questions surfaced while drafting `registry/join_types_mapping.md` (Round 1)
---

# Open Questions — `registry/join_types_mapping.md`

Each entry is self-contained and includes the context, options considered, and the Round-1 position adopted in `registry/join_types_mapping.md`. Entries resolve once empirical adapter-harness data or adapter-implementation review retires them. None of these questions blocks Round-1 ratification of the registry — all are coordination items with canonical deferrals (`16 §4.3` / `17 §10`) or adapter implementation (`35` / `36` / adapter crates).

> **Index pointer.** For a one-file view of every open question across all registry sidecars (functions / join-types / temporal-shape), see [`registry_open_questions.md`](registry_open_questions.md). That index is pure navigation — the full question bodies stay here.

---

## Q-JOIN-MAP-001 — Explicit `INNER` keyword vs bare `JOIN`

**Context.** All five first-class SQL engines (DataFusion, DuckDB, Spark, Snowflake, BigQuery) accept the bare `JOIN` keyword as equivalent to `INNER JOIN`. Emitting one form or the other is a pure readability / consistency decision; there is no semantic divergence. `functions_mapping.md` does not carry a directly analogous choice; `temporal_shape_mapping.md` does not opine.

**Options.**

- **Option A — Always emit explicit `INNER JOIN`.**
    - Pros: Unambiguous in rendered SQL; consistent with the explicit `LEFT OUTER JOIN` / `RIGHT OUTER JOIN` / `FULL OUTER JOIN` forms (though `OUTER` is itself optional — see Q-JOIN-MAP-008 if surfaced); easier to grep / read.
    - Cons: Two extra keywords per join emission; some SQL-style guides (notably large Spark codebases) prefer the bare form.
- **Option B — Always emit bare `JOIN`.**
    - Pros: More compact; matches Spark community style.
    - Cons: Ambiguous to readers used to explicit forms; inconsistent with how outer joins render.
- **Option C — Per-engine preference.**
    - Pros: Match each engine's community style.
    - Cons: Cross-engine debugging (where an author compares emitted SQL side-by-side) becomes harder; complicates adapter test-baseline maintenance.

**Round-1 position.** **Option A** — always emit explicit `INNER JOIN`. Readability and cross-engine diff consistency outweigh the four-character terseness win. Revisit if an adapter implementer surfaces a concrete downside (e.g. a version-specific parser that disambiguates `JOIN` differently).

**Disposition.** Captured as the canonical emission form in `registry/join_types_mapping.md §3.1`. Open for revisiting; does not block ratification.

---

## Q-JOIN-MAP-002 — `ON` vs `USING` emission form

**Context.** When every `KeyPair` in a `Relationship` has the same `SemanticsName` on both sides (`left_key == right_key`), SQL engines admit a compact `USING` clause in addition to the explicit `ON` clause:

```sql
FROM orders o INNER JOIN customers c USING (customer_id)
-- vs --
FROM orders o INNER JOIN customers c ON o.customer_id = c.customer_id
```

The two forms are not fully semantically equivalent:

- `USING` **collapses** the shared column — the joined output contains one `customer_id` column (unqualified) rather than two (`o.customer_id` and `c.customer_id`). This changes downstream projection resolution.
- `ON` preserves both sides' columns with their table qualifiers intact.

Some engines (Postgres, Spark) drop one copy; others (DuckDB, BigQuery) collapse to one unqualified column. Snowflake accepts `USING` but applies it consistently per ANSI semantics.

**Options.**

- **Option A — Always emit `ON`.**
    - Pros: Uniform output schema across engines; no key-column collapsing; straightforward downstream projection mapping.
    - Cons: Slightly more verbose when all keys match by name.
- **Option B — Emit `USING` when all `KeyPair`s satisfy `left == right`.**
    - Pros: More compact; matches the "natural semantic join" intent authors express when they use the same `SemanticsName` on both sides of a `Relationship`.
    - Cons: Output schema varies cross-engine; downstream projection logic must handle the collapsed-column case per engine; regression risk on schema-sensitive adapters.
- **Option C — Per-engine choice.**
    - Pros: Optimize each engine's idiom.
    - Cons: Even more schema divergence; debugging pain.

**Round-1 position.** **Option A** — always emit `ON`. The uniform output schema outweighs the compactness win. Authors who want the `USING`-collapse semantic should model it at the canonical layer (e.g. project away one copy of the key via a `Dimension` expression), not at the adapter layer.

**Disposition.** `registry/join_types_mapping.md §3.1` adopts Option A. Open — revisit if adapter implementers surface a concrete cost (e.g. a plan-optimization regression) to the `ON` form.

---

## Q-JOIN-MAP-003 — `RIGHT JOIN` auto-rewrite to `LEFT` with swapped operands

**Context.** Spark SQL style guides prefer `LEFT JOIN` over `RIGHT JOIN` on the grounds that `LEFT` reads left-to-right aligned with the `FROM` clause narrative. Catalyst (Spark's optimizer) internally normalizes `RIGHT` to `LEFT` by swapping operands. BigQuery documentation carries a mild nudge the same direction. Other engines (DataFusion, DuckDB, Snowflake) treat the two forms as equivalent with no documented preference.

Rewriting `RIGHT` to `LEFT` at the adapter layer would swap the FROM-clause operand order. For example:

```sql
-- Canonical (as-authored): Relationship { from: events, to: customers, join_type: Right }
-- Preserved emission
FROM events e RIGHT JOIN customers c ON e.customer_id = c.id
-- Rewritten-to-LEFT emission
FROM customers c LEFT JOIN events e ON e.customer_id = c.id
```

The two forms produce identical result rows but different operand order in rendered SQL.

**Options.**

- **Option A — Preserve author orientation; emit `RIGHT JOIN` verbatim.**
    - Pros: Rendered SQL mirrors `Relationship.from` / `.to` orientation, which is a canonical author concern (`16 §2.2`); debugging / plan-introspection tooling shows author intent directly; engine optimizers normalize internally so runtime cost is unchanged.
    - Cons: Reads against local style conventions on Spark; some reviewers may find mixed `LEFT` / `RIGHT` in emitted SQL confusing.
- **Option B — Auto-rewrite to `LEFT` with swapped operands.**
    - Pros: Uniform `LEFT`-dominant emission; matches community style on Spark / BigQuery.
    - Cons: Rendered SQL no longer mirrors canonical `Relationship.from` / `.to` orientation; harder to trace a rendered join back to its author-level `Relationship`; no runtime benefit (optimizers normalize anyway).
- **Option C — Per-engine policy** (rewrite on Spark / BigQuery; preserve on others).
    - Pros: Match each engine's community preference.
    - Cons: Divergent behavior across adapters; breaks the "canonical-layer invariance" principle.

**Round-1 position.** **Option A** — preserve author orientation. Rendered SQL traceability to canonical `Relationship` orientation outweighs local style preferences. Engine optimizers handle runtime normalization internally.

**Disposition.** `registry/join_types_mapping.md §3.3.1` adopts Option A and tracks `TD-JOIN-RIGHT-REWRITE`. Open — revisit if adapter implementers find a concrete perf divergence, or if a `SqlStyle` adapter option surfaces to let downstream consumers choose.

---

## Q-JOIN-MAP-004 — `AsOf` rewrite-tier-table authority

**Context.** `registry/temporal_shape_mapping.md §4.2` carries a per-(`AsOfAnchor`, engine) rewrite-tier table classifying each pair as First-class / Structural / Unsupported. `registry/join_types_mapping.md §4` also benefits from such a table — readers approaching the question from the `JoinType` end rather than the `TemporalShape` end need the same classification. Duplicating the table in both places creates a synchronization risk; factoring to a shared fragment creates navigation complexity.

**Options.**

- **Option A — `temporal_shape_mapping.md §4.2` is authoritative; `join_types_mapping.md §4` mirrors with explicit cross-reference.**
    - Pros: Clear single-source-of-truth; divergence between the two becomes a documentation bug with a clear winner; minimal added infrastructure.
    - Cons: Manual duplication risk — a row edited in one doc must be mirrored in the other; Round-1 mitigations rely on reviewer discipline.
- **Option B — `join_types_mapping.md §4` is authoritative; `temporal_shape_mapping.md §4.2` mirrors.**
    - Pros / Cons: symmetric reversal; weaker because `TemporalShape` is the canonical axis that defines `AsOfAnchor` legality per `17 §5.2`, so that doc is the more natural home.
- **Option C — Shared fragment (e.g. `registry/_fragments/asof_tier_table.md` included from both).**
    - Pros: Genuine single source.
    - Cons: Adds infrastructure semstrait docs do not otherwise use; fragment-inclusion mechanism not defined for the Markdown corpus; harder to read in a plain-text tree.
- **Option D — Eliminate the duplication by moving all `AsOf` detail to one doc and linking from the other.**
    - Pros: Smaller corpus.
    - Cons: Readers entering from the `JoinType` doc lose immediate access to the tier view; navigation cost.

**Round-1 position.** **Option A** — `temporal_shape_mapping.md §4.2` is authoritative; this registry's `§4.2` is a navigation-aid companion with explicit "authoritative cross-reference" language. Reviewer discipline mitigates the duplication risk. Option C is the right long-term answer if the duplication burden grows; not worth the scaffolding investment in Round 1.

**Disposition.** `registry/join_types_mapping.md §3.5`, `§4.3` adopt Option A. Open — revisit if the `AsOf` tier matrix grows complex enough to warrant fragment extraction.

---

## Q-JOIN-MAP-005 — `LATERAL` reserved-variant disposition

**Context.** `LATERAL` subquery is supported by Spark 3.4+, DuckDB, Snowflake, BigQuery (as `CROSS JOIN UNNEST` for array unnesting), and partially by DataFusion (at the `LogicalPlan` layer, not SQL surface 🟡). Apache Calcite does NOT expose `LATERAL` as a `JoinRelType` — it models the semantics via `Correlate`. Substrait similarly uses subquery correlation, not a `JoinRel` variant.

Canonical `JoinType` in `16 §4` models symmetric static-operand joins — `LATERAL`'s row-wise correlated evaluation does not fit that model cleanly. Authors needing lateral semantics in v1 can express them via:

1. Canonical function calls returning array / struct / set types (per `14a §7` adapter-extended function registration) for unnesting.
2. A `Dimension` / `Measure` expression that folds the correlated computation into scalar.
3. Direct author-side SQL emission via an `Adapter`-specific escape hatch (e.g. raw-SQL `Dataset` backing).

**Options.**

- **Option A — No canonical `JoinType::Lateral` variant; adapter-extended per engine.**
    - Pros: Keeps canonical `JoinType` enum minimal and clean; matches Calcite's `JoinRelType` posture; authors use canonical function calls for unnesting; adapter authors can expose engine-specific `LATERAL` emission via `RegistryExtension`.
    - Cons: Authors hitting complex row-wise correlated patterns have no first-class canonical path.
- **Option B — Canonicalize `JoinType::Lateral` with a structural rewrite matrix.**
    - Pros: First-class author surface for lateral semantics.
    - Cons: Forces every adapter to implement `LATERAL` emulation (via correlated subqueries) even when the underlying use-case is naturally a `FunctionCall`; pollutes the `JoinType` enum with an asymmetric variant; complicates `PlanNode::Join`'s symmetric-operand model.
- **Option C — Canonicalize `JoinType::Lateral` narrowly scoped to top-N-per-group.**
    - Pros: Covers the common case; clean semantics (deterministic `N`).
    - Cons: Narrow scope; doesn't cover array unnesting; inconsistent with the general `LATERAL` shape.

**Round-1 position.** **Option A** — adapter-extended per engine; no canonical promotion. Top-N-per-group lateral semantics are expressible via `AsOfAnchor::SnapshotLatestAtOrBefore` with `N = 1` (`17 §5.1`); array unnesting is expressible via canonical functions in `14a`. Non-Round-1 patterns surface as adapter-extended joins, tracked per-adapter (parallel to `functions_mapping.md §12`'s `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` precedent).

**Disposition.** `registry/join_types_mapping.md §3.6.3` adopts Option A and tracks `TD-JOIN-LATERAL-CANONICAL`. Revisit in v2 if author-facing demand surfaces for a canonical lateral variant beyond the top-N-per-group case.

---

## Q-JOIN-MAP-006 — Cardinality-informed `DISTINCT` / hint auto-emission

**Context.** `Relationship.cardinality` (per `16 §3`) declares authoring intent. When `cardinality ∈ {OneToMany, ManyToMany}`, the join may produce row-duplication the author did not intend. `16 §3.3.2` ratifies the **planner-level** fanout-safe rewrite (pre-join aggregation). An orthogonal adapter-layer question: should the adapter emit hints (`DISTINCT`, optimizer hints like Spark's `/*+ BROADCAST */`) automatically based on `Cardinality`?

Concrete examples:

- `ManyToMany` + projection-only-on-`from`-side: adapter could emit `SELECT DISTINCT ...` to dedupe.
- `OneToMany` + known-small `to` side: Spark adapter could emit `/*+ BROADCAST(to_side) */`.

Both change observable behavior (row count for `DISTINCT`; explain-plan / runtime cost for hints).

**Options.**

- **Option A — No automatic emission.**
    - Pros: Predictable — rendered SQL mirrors canonical `JoinType` + plan-level rewrite only; no hidden behavior; planner-level fanout-safe rewrite (`16 §3.3.2`) owns correctness; adapter layer owns only faithful emission.
    - Cons: Leaves per-engine optimization on the table; `ManyToMany` joins where the author wants dedup must author an explicit `SELECT DISTINCT`-style pattern.
- **Option B — Auto-emit `DISTINCT` on `ManyToMany`; no optimizer hints.**
    - Pros: Captures the common dedup intent without hint-specific complexity.
    - Cons: Changes row count; risks masking genuine fanout bugs; non-uniform across engines (`DISTINCT` semantics on NULL differs per dialect 🟡).
- **Option C — Auto-emit both `DISTINCT` and engine-specific optimizer hints where available.**
    - Pros: Maximum optimization.
    - Cons: Unpredictable rendered SQL; hints are engine-specific and require per-engine config; Spark-only broadcast hints create cross-engine asymmetry; hints can hurt perf when cost-estimate assumptions are wrong.
- **Option D — Opt-in via per-`Relationship` or per-Request adapter hint.**
    - Pros: Author control.
    - Cons: Adapter-surface expansion; not a pure registry concern — bleeds into `32` / `36` adapter options.

**Round-1 position.** **Option A** — no automatic emission. Predictable emission outweighs the opportunistic optimization. Planner-level fanout-safe rewrite (`16 §3.3.2`) handles correctness; adapter-layer hint emission is a future opt-in feature (`TD-JOIN-CARDINALITY-HINTS`).

**Disposition.** `registry/join_types_mapping.md §5.1`, `§5.3` adopt Option A and track `TD-JOIN-CARDINALITY-DISTINCT-AUTO` + `TD-JOIN-CARDINALITY-HINTS`. Revisit when an adapter-option surface lands in `36`.

---

## Q-JOIN-MAP-007 — `FULL OUTER` emulation for historical dialects (SQLite, MySQL)

**Context.** `FULL OUTER JOIN` is absent natively from SQLite < 3.39 and from MySQL 8.x. semstrait does not include these engines in the Round-1 adapter roster (see `§2`). The documented emulation pattern (`LEFT ... UNION ALL ... RIGHT ... WHERE IS NULL` — `§6.3`) is universal.

**Options.**

- **Option A — Not in Round 1; defer until adapter includes a gap-bearing dialect.**
    - Pros: Scope discipline; no scaffolding cost for engines we do not target.
    - Cons: Documentation lacks the ready-to-use pattern when an adapter for these engines eventually lands.
- **Option B — Document the emulation form now** (as `§6.3` does) **but with no engine row; pre-wire the pattern.**
    - Pros: Minor docs investment pays dividends when the adapter lands.
    - Cons: Slight documentation churn if the pattern needs revision before any adapter uses it.

**Round-1 position.** **Option B** — pattern documented in `§6.3` as a cross-engine idiom for future use; no active engine row in `§3.4` consumes it.

**Disposition.** `registry/join_types_mapping.md §6.3` carries the pattern; `TD-JOIN-FULL-HISTORICAL-DIALECTS` tracks the scaffolding for when the adapter roster expands.

---

## Cross-references to other open-question files

- [`temporal_shape_mapping_open_questions.md`](temporal_shape_mapping_open_questions.md) — `AsOf` rewrite-tier questions are authoritative there. In particular:
    - `Q-TEMPORAL-MAP-004` (ScdWindow trailing-closure on native-`ASOF` engines) — resolved Option C in `temporal_shape_mapping.md`; this doc's `§3.5.1` inherits.
    - `Q-TEMPORAL-MAP-001` (DataFusion native `ASOF JOIN`) — shares `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE`.
    - `Q-TEMPORAL-MAP-003` (Spark `QUALIFY` versioning) — shares `TD-TEMPORAL-ASOF-SPARK-NATIVE`.
- [`functions_mapping_open_questions.md`](functions_mapping_open_questions.md) — `ROW_NUMBER` consumed by `AsOf` structural rewrites is ratified in `14a`; emission-level divergence (e.g. frame clauses) would surface there.
- [`16_open_questions.md`](16_open_questions.md) — `16 §4.3` defers `Semi` / `Anti` / `AsOf` canonical promotion. `TD-COMPOSITION-SEMI-ANTI` / `TD-COMPOSITION-ASOF` are shared.
- [`17_open_questions.md`](17_open_questions.md) — `17 §10` defers `JoinType::AsOf(AsOfAnchor)` planner implementation. `TD-COMPOSITION-ASOF` and the `AsOf` emission detail shared with `temporal_shape_mapping.md` are ratified there.
- [`36_open_questions.md`](36_open_questions.md) — adapter-surface hint emission (Q-JOIN-MAP-006 Option D) becomes an adapter-API concern when pursued.
