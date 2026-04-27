---
doc: design/registry/temporal_shape_mapping
status: Living
purpose: Authoritative per-engine mapping of canonical `TemporalShape` variants and `JoinType::AsOf` anchors
prereqs: [17]
authoritative-for:
  - canonical `TemporalShape` variants ↔ engine-native temporal idioms across DataFusion / DuckDB / Spark (v1) and Snowflake / BigQuery / Substrait (planned)
  - per-engine SCD Type0–Type6 emission patterns (append-only, overwrite, valid-window, prior-value column, history table, hybrid)
  - snapshot-selection emission patterns (native time-travel syntax, WHERE-max pattern, version-AS-OF clauses)
  - `JoinType::AsOf` rewrite-tier classification per (anchor, engine) pair (First-class / Structural-rewrite / Unsupported)
  - per-engine gap catalog against canonical `17` semantics, with workaround strategies the adapter uses
  - cross-engine idiom library (WHERE-max snapshot selection, windowed as-of via ROW_NUMBER/QUALIFY, half-open valid-window predicate)
  - `TD-TEMPORAL-*` entries tracking per-engine shortfalls, sentinel conventions, and canonical-vs-native divergences
depends-on:
  - foundations/17_temporal_shape.md (canonical `TemporalShape`, `ScdSubtype`, `JoinType::AsOf`, `AsOfAnchor`; this registry maps those canonical variants)
  - foundations/16_composition.md (`JoinType` enum; `AsOf` is the composition-layer extension ratified in `17 §5`)
  - registry/README.md (registry policy, engine coverage, versioning posture)
  - registry/types_mapping.md (sibling exemplar; canonical `DataType` mappings for `Timestamp` / `Date` consumed by temporal predicates)
  - registry/functions_mapping.md (sibling exemplar; `date_trunc`, `date_part`, `row_number`, and comparison operators consumed by temporal-shape emission)
  - apis/34_semstrait_planner.md (planner's `Request.temporal` consumption; DEFERRED per `17 §10 D4`)
  - apis/35_semstrait_ir.md (`PlanNode::Join` carriage of `JoinType::AsOf(anchor)`; DEFERRED per `17 §10 D1`)
  - apis/36_semstrait_adapter.md (adapter trait; PlanBuilder / Dialect layering that consumes this registry)
---

# Temporal Shape Mapping Catalog

> **Scope.** Authoritative per-engine rendering of every canonical `TemporalShape` variant ratified in `foundations/17_temporal_shape.md §2` (`Timeseries`, `Events`, `Snapshot`, `Scd { subtype: Type0..Type6 }`) and every `JoinType::AsOf(AsOfAnchor)` variant ratified in `17 §5.1`. This document is a **Living catalog**: entries gain detail, annotations, or additional engine columns as adapters land. It does NOT define new canonical shapes, subtypes, or anchor families — those live in `17`. Per `00 §6.6`, canonical specs in `17` never depend on the specific contents of this catalog.

> **Status (as of drafting, 2026-04-20):** Round-1 scaffold drafted against `17` Round-1 ratification. Planner-side `Request.temporal` consumption and `AsOf` emission are DEFERRED per `17 §10`; this registry captures the **emission target** each adapter will converge on once the planner lands. Rows marked 🟡 are plausible based on engine documentation (DuckDB 1.1.x, DataFusion 40.x+, Spark 3.5.x, Snowflake docs, BigQuery docs, Iceberg / Delta time-travel specs) but have not been empirically verified against a live adapter test harness. Unresolved items parked in [`questions/open/temporal_shape_mapping_questions.md`](../questions/open/temporal_shape_mapping_questions.md).

---

## 1. Purpose and Scope

### 1.1 What this catalog ratifies

Per `17 §1.2`'s forward-refs and `00 §6.6`'s Living-catalog policy, this registry is the authoritative home for:

- **Per-engine temporal-shape emission.** How each target engine naturally expresses each canonical `TemporalShape` variant — function calls, syntactic constructs, or workaround predicates when the engine lacks a native idiom.
- **`AsOf` rewrite-tier classification.** Per-(anchor, engine) pair: does the engine admit native `ASOF JOIN` (First-class tier), require structural rewrite to window-functions + predicates (Structural tier), or lack any emission path (Unsupported tier)?
- **Gap catalog.** Per engine, the canonical `17`-ratified semantics the engine does NOT natively support, plus the workaround strategy the adapter uses.
- **Cross-engine idioms.** Patterns shared across multiple adapters (WHERE-max snapshot selection; windowed ROW_NUMBER as-of; half-open valid-window predicate) — written once, referenced from each adapter's row.

### 1.2 What this catalog does NOT ratify

- **Canonical semantics.** Every conflict between an engine-native form and the canonical `17` specification resolves in favor of the canonical form; this registry documents the translation, not a redefinition. Per `00 §6.6`: "canonical specs under `foundations/` may reference the registry but never depend on its specific contents."
- **Planner algorithm for `Request.temporal`.** The multi-shape resolution algorithm (`17 §6.5` DEFERRED; `17 §10 D4, D5`) lands in `34 §…`. This registry documents the per-engine emission the planner will produce; it does not describe how the planner chooses between shapes.
- **Adapter-extended temporal idioms.** Engines expose temporal features outside the canonical `17` roster — Snowflake's `BEFORE (STATEMENT => ...)`, BigQuery's `MATCH_RECOGNIZE`, Iceberg branch-aware reads, DuckDB's `time_bucket()`. These are adapter-extended per Q-TEMPORAL-MAP-008 (Option B adopted: defer to adapter-crate READMEs). `§3.2.1` calls out the `MATCH_RECOGNIZE` exclusion explicitly; the rest live in each adapter's own documentation.
- **Sentinel-value conventions for `valid_to`.** `17 §10 D13` reserves future work to ratify author-declared sentinels (`'9999-12-31'`, etc.); until then this catalog emits `NULL`-aware predicates and documents the sentinel forms adapters encounter (§3.4.3).

### 1.3 Engine-shortfall posture

When an engine lacks native support for a canonical variant, this catalog documents three response strategies in decreasing order of preference:

1. **Structural rewrite** into a combination of engine-supported constructs that preserves canonical semantics. Example: DataFusion `AsOf(ScdWindow)` → `Inner` join + `FilterNode(valid_from <= probe AND (valid_to > probe OR valid_to IS NULL))` (§6.3).
2. **Warning-plus-degradation** when canonical semantics cannot be fully preserved but a reasonable approximation exists. Example: Spark < 3.5 `ASOF` rewrite via subquery-wrapped `ROW_NUMBER()` with a warning about performance (§6.4). Degradation must preserve correctness; only performance characteristics differ.
3. **`ADAPT_E_0302 UnsupportedFeature`** when the gap is fundamental and no correct workaround exists. Documented per engine in §5 with the canonical variant that triggers it.

---

## 2. Engine Roster

Engine coverage follows `registry/README.md §engine-coverage-policy`. The three first-class compute targets ship in v1; additional engines land as columns / rows when their adapters ratify.

| Engine | Version target | Adapter crate | Implementation status | Notes |
|---|---|---|---|---|
| **DataFusion** | 40.x+ 🟡 | `semstrait-adapter-datafusion` (planned) | Round-1 target | Primary reference. No native `ASOF JOIN`; structural rewrites only. No Iceberg / Delta time-travel in vanilla DF. |
| **DuckDB** | 1.1.x 🟡 | `semstrait-adapter-duckdb` (planned) | Round-1 target | Native `ASOF JOIN` since 0.9; Iceberg / Delta extensions opt-in (Q-TEMPORAL-MAP-002). SQL-idiom reference. |
| **Spark** | 3.5.x 🟡 | `semstrait-adapter-spark` (planned) | Round-1 target | No native `ASOF JOIN`; structural rewrites via `QUALIFY` + `ROW_NUMBER` (Spark 3.4+). Delta time-travel via `VERSION AS OF` / `TIMESTAMP AS OF`. |
| **Snowflake** | 2024.x 🟡 | `semstrait-adapter-snowflake` (planned) | Planned | Native `ASOF JOIN` since 2023. Time-travel via `BEFORE (STATEMENT => ...)` / `AT (TIMESTAMP => ...)`. |
| **BigQuery** | 2024.x 🟡 | `semstrait-adapter-bigquery` (planned) | Planned | No native `ASOF JOIN`; structural rewrites. `FOR SYSTEM_TIME AS OF` for snapshot / time-travel. `MATCH_RECOGNIZE` (adapter-extended, §3.2.1). |
| **Substrait** | 0.48+ 🟡 | `semstrait-adapter-substrait` (planned) | Planned | Plan-IR emission. `JoinRel` carries canonical `JoinType`; `AsOf` emitted via Substrait extensions (`extension_uri`). |

Additional engines (ClickHouse, Trino, Iceberg-SQL-REST) add as columns to §3 / §4 / §5 tables when their adapters land. Adding a column MUST NOT change canonical variants — if an engine lacks native support, the catalog documents the adapter's emulation strategy and files a `TD-TEMPORAL-*` entry.

---

## 3. Shape-by-Shape Mapping

One subsection per canonical `TemporalShape` variant. Each subsection carries: (a) the canonical definition summary from `17`, (b) a per-engine mapping table, (c) emission notes for workaround patterns.

### 3.1 `Timeseries`

**Canonical summary** (`17 §2.1`): dense regularly-spaced observations at a known cadence `grain`. Rolls up to any `G' >= grain` via `DateTrunc` bucketing; sub-grain requests are `PLAN_E_1710` (`17 §9.3`).

| Canonical emission | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|
| Rollup bucket | `date_trunc('grain', occurred_at)` | `date_trunc('grain', occurred_at)` / `time_bucket(INTERVAL 'N unit', occurred_at)` 🟡 | `date_trunc('grain', occurred_at)` / `window(occurred_at, 'N unit')` 🟡 | `date_trunc('grain', occurred_at)` | `timestamp_trunc(occurred_at, GRAIN)` |
| Gap-filling (dense → dense) | `generate_series(...)` + `LEFT JOIN` 🟡 | `range(...)` / `generate_series(...)` + `LEFT JOIN` | `sequence(start, end, interval)` + `explode` + `LEFT JOIN` 🟡 | `generator(TIMESTAMPDIFF(...))` 🟡 | `GENERATE_TIMESTAMP_ARRAY(...)` + `UNNEST` 🟡 |
| Per-grain aggregation | `GROUP BY date_trunc(...)` | `GROUP BY date_trunc(...)` / `GROUP BY time_bucket(...)` | `GROUP BY date_trunc(...)` / `window()` | `GROUP BY date_trunc(...)` | `GROUP BY timestamp_trunc(...)` |

**Portability summary: Universal** — every engine emits via `date_trunc` + `GROUP BY`. The registry's `date_trunc` entry in `functions_mapping.md §11` covers the per-engine function-mapping detail; this row just notes that `Timeseries` shape itself adds nothing exotic. DuckDB's `time_bucket` and Spark's `window()` are adapter-extended alternatives preferred for irregular or offset buckets (e.g. "weeks starting Sunday"); they live outside canonical `17` per `17 §1.2`.

**Gap-filling as adapter concern.** `17 §2.1` characterizes `Timeseries` as "dense"; adapters reading a sparse source that declares `Timeseries` shape MAY emit gap-filling scaffold (per engine row) to satisfy the density invariant. `PLAN_W_17xx ShapeAwareRequestOnUnclassifiedDataKind` does not apply — the source is classified; the planner's gap-fill pass is DEFERRED per `17 §10`.

### 3.2 `Events`

**Canonical summary** (`17 §2.1`): sparse discrete occurrences at `occurred_at_dim`. No density assumption; any `grain ∈ occurred_at_dim.grains` is a legal bucket.

| Canonical emission | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|
| Bucket-then-aggregate | `GROUP BY date_trunc('grain', occurred_at)` | `GROUP BY date_trunc('grain', occurred_at)` | `GROUP BY date_trunc('grain', occurred_at)` | `GROUP BY date_trunc('grain', occurred_at)` | `GROUP BY timestamp_trunc(occurred_at, GRAIN)` |
| Sub-day grain (`hour`, `minute`) | `date_trunc('hour', occurred_at)` | same | same | same | `timestamp_trunc(occurred_at, HOUR)` |
| Filtering by window | `occurred_at >= :start AND occurred_at < :end` | same | same | same | same |

**Portability summary: Universal.** `Events` is the simplest shape — every engine supports `GROUP BY` on a bucket expression. No rewrite-tier concerns; no TD entries originate from this row.

#### 3.2.1 `MATCH_RECOGNIZE` and pattern-matching idioms — explicitly out of scope

Several engines (BigQuery, Snowflake, Trino) expose `MATCH_RECOGNIZE` for sequential-pattern event detection. This is **out of canonical scope** per `17 §1.2`'s forward-ref stance — canonical `Events` expresses "rows exist where something happened" at the aggregation level, not row-sequence pattern matching. `MATCH_RECOGNIZE` is adapter-extended on engines that support it and lives in each adapter crate's README, per Q-TEMPORAL-MAP-008 Option B. `TD-TEMPORAL-MATCH-RECOGNIZE` tracks a possible future canonical `EventPattern` variant if author demand materializes.

### 3.3 `Snapshot`

**Canonical summary** (`17 §2.1, §4.1`): periodic full-state capture at `snapshotted_at_dim`. Optional declared `cadence: Grain`. As-of semantics: "latest snapshot at or before `Request.temporal.as_of`" (`17 §6.2`). Sub-cadence requests are `PLAN_E_1711` (`17 §9.3`).

#### 3.3.1 Per-engine mapping

| # | Row | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|---|
| 1 | Vanilla SQL latest-at-or-before | WHERE-max pattern (§6.1) | WHERE-max pattern | WHERE-max pattern | WHERE-max pattern | WHERE-max pattern |
| 2 | Native table-level time-travel | *(none)* | *(none in vanilla)* | `VERSION AS OF` / `TIMESTAMP AS OF` (Delta; Spark 3.3+) 🟡 | `AT (TIMESTAMP => ...)` / `BEFORE (STATEMENT => ...)` 🟡 | `FOR SYSTEM_TIME AS OF :ts` |
| 3 | Iceberg `VERSION AS OF` (extension) 🟡 | via Iceberg DataFusion binding 🟡 | `iceberg_scan('path', version => N)` (iceberg extension, Q-TEMPORAL-MAP-002) 🟡 | `VERSION AS OF N` (Iceberg connector) | `AT (BRANCH => 'main', VERSION => N)` 🟡 | — |
| 4 | Iceberg `TIMESTAMP AS OF` 🟡 | — 🟡 | `iceberg_scan('path', timestamp => :ts)` (iceberg extension) 🟡 | `TIMESTAMP AS OF :ts` | `AT (TIMESTAMP => :ts)` | — |
| 5 | Delta `VERSION AS OF` 🟡 | — 🟡 | `delta_scan('path', version => N)` (delta extension) 🟡 | `VERSION AS OF N` | — | — |
| 6 | Row-level `snapshotted_at` filter | `WHERE snapshotted_at = (SELECT MAX(snapshotted_at) FROM ... WHERE snapshotted_at <= :as_of)` | same | same | same | same |
| 7 | `QUALIFY`-based latest | — | `QUALIFY ROW_NUMBER() OVER (PARTITION BY entity ORDER BY snapshotted_at DESC) = 1 WHERE snapshotted_at <= :as_of` 🟡 | same (Spark 3.4+) 🟡 | same | same 🟡 |
| 8 | Subquery-wrapped ROW_NUMBER | `SELECT ... FROM (SELECT *, ROW_NUMBER() OVER (...) AS rn FROM ... WHERE snapshotted_at <= :as_of) WHERE rn = 1` | same | same (pre-3.4) | same | same |
| 9 | Snowflake transactional time-travel | — | — | — | `BEFORE (STATEMENT => '<query_id>')` 🟡 | — |
| 10 | Snowflake clone-at-time | — | — | — | `AT (OFFSET => -60*60)` 🟡 | — |

**Portability summary: Partial.** Rows 1 and 6 are universal (WHERE-max fallback). Rows 2–5 depend on source-format integration (Iceberg / Delta / native). Rows 7–8 are window-function rewrites universal on any engine with `ROW_NUMBER`. Rows 9–10 are Snowflake-specific time-travel idioms outside canonical `17`; documented for reference, adapter-extended.

#### 3.3.2 WHERE-max pattern — the canonical fallback

When no native time-travel is available (DataFusion, vanilla DuckDB / Spark / BigQuery against non-time-travel tables), the adapter emits:

```sql
SELECT ...
FROM snapshots s
WHERE s.snapshotted_at = (
  SELECT MAX(s2.snapshotted_at)
  FROM snapshots s2
  WHERE s2.snapshotted_at <= :as_of
)
```

This form is canonical-equivalent for all snapshot sources. The alternative `QUALIFY ROW_NUMBER() = 1` form (row 7) is preferred on engines that support `QUALIFY` because it avoids the self-join, but both are correct. See §6.1 for the idiom pattern and §6.4 for the QUALIFY rewrite.

#### 3.3.3 Cadence-rollup reducer policies

`17 §4.1` specifies snapshot cadence-rollup requires a reducer (first / last / average / configurable), with last-in-period as the Round-1 default. `17 §10 D8` defers the full reducer-policy catalog to `25 §…`. Per Q-TEMPORAL-MAP-005 Option A, this registry carries only last-in-period emission in Round 1; other reducers wait for `25 §…`.

**Last-in-period** (the Round-1 default). Select, per period, the snapshot with the maximum `snapshotted_at` within that period:

| Engine | Emission |
|---|---|
| DataFusion | `QUALIFY`-absent: subquery-wrapped `ROW_NUMBER() OVER (PARTITION BY date_trunc('week', snapshotted_at) ORDER BY snapshotted_at DESC) = 1` 🟡 |
| DuckDB | `QUALIFY ROW_NUMBER() OVER (PARTITION BY date_trunc('week', snapshotted_at) ORDER BY snapshotted_at DESC) = 1` 🟡 |
| Spark (3.4+) | `QUALIFY ROW_NUMBER() OVER (...)` 🟡 |
| Snowflake | `QUALIFY ROW_NUMBER() OVER (...)` |
| BigQuery | same via subquery wrap (no `QUALIFY`) 🟡 |

Tracked as `TD-TEMPORAL-SNAPSHOT-CADENCE-ROLLUP` in §7.

### 3.4 `Scd` (Slowly-Changing Dimension) Type0–Type6

**Canonical summary** (`17 §2.2`): seven Kimball subtypes ratified at the vocabulary level. Round-1 planner targets Type1 / Type2; Type0 / Type3 recognized for advisory purposes; Type4 / Type5 / Type6 vocabulary-ratified, planner work DEFERRED (`17 §10 D7, D9, D10`).

#### 3.4.1 Per-subtype emission

| SCD Subtype | Canonical semantics (`17 §2.2`) | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|---|
| `Type0` (retain original) | No update after insert. Append-only. | Regular table; ingest-layer enforces immutability (Q-TEMPORAL-006 Option A: out of scope for semstrait). | same | same | same (or use `TRANSIENT` / write-once policies) 🟡 | same |
| `Type1` (overwrite) | One row per entity; updates overwrite. No history. | `MERGE ... WHEN MATCHED THEN UPDATE WHEN NOT MATCHED THEN INSERT` (DF DML 🟡) | `INSERT OR REPLACE` / `MERGE INTO` | `MERGE INTO` (Delta 🟡) / `UPSERT` | `MERGE INTO` | `MERGE INTO` |
| `Type2` (full history) | `valid_from` / `valid_to` per row; `current_flag` optional. | `Inner`/`Left` + half-open predicate (§6.3) | native `ASOF JOIN` (§3.5) + trailing closure (§3.5.1), or half-open predicate | `QUALIFY ROW_NUMBER()` rewrite, or half-open predicate | native `ASOF JOIN` + trailing closure, or half-open predicate | structural rewrite (no native `ASOF`), or half-open predicate |
| `Type3` (prior-value column) | Current + one prior value. No versioning. | Regular column read; no special emission. | same | same | same | same |
| `Type4` (history table) | Current table + separate history table (per `history_data_kind_ref`). | Plan-level join to history-kind's resolved binding (DEFERRED; `17 §10 D7`) | same | same | same | same |
| `Type5` (Type 4 + mini-dim) | Type 4 + outrigger mini-dim (`mini_dim_ref`). | DEFERRED (`17 §10 D9`) | same | same | same | same |
| `Type6` (hybrid) | Type 1 + Type 2 (+ optional Type 3). Carries both current and historical columns on each row. | half-open predicate for as-of; direct column read for current (`current_value_dim`). Detection of current-vs-history disagreement DEFERRED (`17 §10 D10`). | same | same | same | same |

**Portability summary per subtype:**

- **Type0**: Universal. No query-emission differences; semstrait does not enforce append-only at query-plan time (Q-TEMPORAL-006).
- **Type1**: Universal at query time. Ingest / MERGE forms differ but are outside semstrait's query-emission scope.
- **Type2**: **Partial**. Native `ASOF JOIN` available on DuckDB / Snowflake; Structural rewrite on DataFusion / Spark / BigQuery. See §4 rewrite-tier table.
- **Type3**: Universal. The prior-value column is read as a regular column.
- **Type4 / Type5 / Type6**: DEFERRED per `17 §10 D7, D9, D10`. Structural shape ratified; emission draft in this registry.

#### 3.4.2 Valid-window JOIN pattern (Type 2 / 5 / 6)

The canonical emission for `Events ↔ Scd::Type2` (and its sibling subtypes) with `AsOfAnchor::ScdWindow` is:

```sql
SELECT e.*, c.*
FROM events e
LEFT JOIN customers_scd c
  ON e.customer_id = c.customer_id
 AND c.valid_from <= e.occurred_at
 AND (c.valid_to > e.occurred_at OR c.valid_to IS NULL)
```

The `IS NULL` disjunct handles open-ended current rows (the canonical open-ended convention per `17 §2.2`). For sentinel-bearing sources (e.g. `valid_to = '9999-12-31'` instead of `NULL`), §3.4.3 documents the observed conventions; Round-1 emission uses the `NULL`-aware form only, per Q-TEMPORAL-MAP-007 Option A.

This form is the **universal canonical** for `ScdWindow` anchors: it works on every engine, and per Q-TEMPORAL-MAP-004 Option C it is preferred even on engines with native `ASOF JOIN` (since `ASOF` alone does not enforce the upper bound). See §6.3.

#### 3.4.3 Sentinel-vs-NULL conventions

Observed conventions for `valid_to` on open-ended rows in the wild; documentary only in Round 1:

| Convention | Adopted by | Round-1 emission posture |
|---|---|---|
| `valid_to IS NULL` | canonical open-ended convention per `17 §2.2` | Emitted by adapters; `valid_to > probe OR valid_to IS NULL` predicate. |
| `valid_to = '9999-12-31'` | Kimball classical; many Oracle / SQL Server shops | Not emitted; author may file an adapter-side `SentinelConfig` once `17 §10 D13` lands. Until then, `PLAN_W_1731 ScdCurrentRowHeuristic` fires on sentinel-bearing sources. |
| `valid_to = '2999-12-31'` / `'3000-01-01'` / `'9999-12-31 23:59:59'` | shop-specific | Same posture. |
| `valid_to > '2099-01-01'` as a heuristic "sentinel-ish" range | rare; not recommended | Unsupported; `PLAN_W_1731` heuristic path. |

Tracked as `TD-TEMPORAL-SENTINEL-RATIFICATION` in §7; resolution gated on `17 §10 D13` ratifying author-declared sentinels.

### 3.5 `JoinType::AsOf` variant emission

**Canonical summary** (`17 §5.1`): temporal-proximity join matching the most recent `to`-side row whose anchor satisfies the anchor condition relative to the `from`-side probe timestamp. Two anchor families in canonical v1: `AsOfAnchor::ScdWindow { probe_dim, to_valid_from_dim, to_valid_to_dim }` and `AsOfAnchor::SnapshotLatestAtOrBefore { probe_dim, to_snapshotted_at_dim }`.

**Implementation-DEFERRED status** (`17 §10 D1, D2`). The planner does not yet emit `JoinType::AsOf` in Round 1. This registry describes the **emission target** adapters will converge on once planner support lands.

| Engine | Native `ASOF JOIN`? | `ScdWindow` emission | `SnapshotLatestAtOrBefore` emission |
|---|---|---|---|
| DataFusion | No 🟡 | Structural: `Inner`/`Left` + `FilterNode(valid_from <= probe AND (valid_to > probe OR valid_to IS NULL))` (§6.3) | Structural: subquery-wrapped `ROW_NUMBER()` latest-at-or-before pattern (§6.4) |
| DuckDB | Yes — `ASOF JOIN` since 0.9 | Half-open predicate (§6.3) preferred per Q-TEMPORAL-MAP-004 Option C; `ASOF JOIN ... ON probe >= valid_from` + trailing `WHERE (valid_to > probe OR valid_to IS NULL)` (§3.5.1) as alternative 🟡 | Native `ASOF JOIN ... ON probe >= snapshotted_at` 🟡 |
| Spark | No | Structural: `QUALIFY ROW_NUMBER() OVER (PARTITION BY entity ORDER BY anchor DESC) = 1 WHERE anchor <= probe` (§6.4; Spark 3.4+). Pre-3.4 uses subquery wrap. 🟡 | Same window-function rewrite. |
| Snowflake | Yes — `ASOF JOIN` since 2023 🟡 | Same posture as DuckDB (Q-TEMPORAL-MAP-004 Option C). 🟡 | Native `ASOF JOIN ... MATCH_CONDITION (probe >= snapshotted_at)` 🟡 |
| BigQuery | No | Structural `QUALIFY`-based rewrite (BigQuery supports `QUALIFY`) 🟡 | Same. |

#### 3.5.1 Trailing-closure rationale for `ScdWindow` on native-`ASOF` engines

Native `ASOF JOIN` in DuckDB and Snowflake accepts a **single inequality match condition** (e.g. `probe >= valid_from`), producing the most recent matching row. It does NOT enforce the `valid_to > probe` upper bound required by canonical `AsOfAnchor::ScdWindow` (`17 §5.1`). Two emission strategies:

1. **Native `ASOF JOIN` + trailing filter**: `ASOF JOIN ... ON probe >= valid_from` + `WHERE matched_row.valid_to > probe OR matched_row.valid_to IS NULL`. Captures engine-optimizer `ASOF` fast paths, but adds a filter.
2. **Half-open predicate on `Inner`/`Left` join**: `ON c.valid_from <= e.probe AND (c.valid_to > e.probe OR c.valid_to IS NULL)`. Single emission form across every engine.

Per Q-TEMPORAL-MAP-004 Option C, the adapter selects per anchor family: `ScdWindow` prefers the half-open predicate (§6.3); `SnapshotLatestAtOrBefore` prefers native `ASOF` where available (since the anchor has no upper bound to enforce). Tracked as `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE`.

---

## 4. `AsOf` Rewrite-Tier Table

**The most important table in this doc.** For every (anchor family, engine) pair, classifies the emission into one of three tiers; drives the adapter's PlanBuilder-layer dispatch.

### 4.1 Tier taxonomy

| Tier | Description | Adapter action |
|---|---|---|
| **First-class** | Engine exposes a syntactic construct that expresses the canonical semantics with zero rewrite (or a single trailing filter for `ScdWindow` per §3.5.1). | Emit native syntax; optionally append trailing filter. |
| **Structural** | Engine requires an expression-tree + predicate rewrite to preserve canonical semantics. Correctness is preserved; performance may differ from native. | PlanBuilder-layer rewrite to window-function + predicate form (§6.3 / §6.4). |
| **Unsupported** | No correct emission path exists. Hard error: `ADAPT_E_0302 UnsupportedFeature { canonical: "JoinType::AsOf(...)", engine }`. | Fail at `adapt`-time per `17 §9.3` / adapter contract. |

Per `14a §6.3` / `17`-equivalent posture, canonical v1 guarantees `Unsupported` does not arise for `ScdWindow` / `SnapshotLatestAtOrBefore` on any first-class engine — every engine admits either First-class or Structural emission. `Unsupported` is reserved for future anchor families (e.g. hypothetical `BiTemporalWindow`) on engines that cannot express them.

### 4.2 Per-(anchor, engine) classification

🟡 pending `TD-TEMPORAL-ASOF-EMPIRICAL` — empirical verification against live adapter test harness.

| Anchor | DataFusion | DuckDB | Spark | Snowflake | BigQuery |
|---|---|---|---|---|---|
| `AsOfAnchor::ScdWindow` | **Structural** (§6.3) | **First-class** with half-open fallback (§3.5.1, Q-TEMPORAL-MAP-004 Option C) 🟡 | **Structural** (`QUALIFY`-based, §6.4; Spark 3.4+) 🟡 | **First-class** with half-open fallback 🟡 | **Structural** (`QUALIFY`-based) 🟡 |
| `AsOfAnchor::SnapshotLatestAtOrBefore` | **Structural** (§6.4) | **First-class** (`ASOF JOIN ... ON probe >= anchor`) 🟡 | **Structural** (`QUALIFY`-based, Spark 3.4+) 🟡 | **First-class** (`ASOF JOIN ... MATCH_CONDITION (probe >= anchor)`) 🟡 | **Structural** (`QUALIFY`-based) 🟡 |

**One-line per engine:**

- **DataFusion**: Structural for both anchors (no native `ASOF`); emission via `Inner`/`Left` + half-open filter (`ScdWindow`) or subquery-wrapped `ROW_NUMBER` (`SnapshotLatestAtOrBefore`). Tracked as `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` per Q-TEMPORAL-MAP-001 (Option A: remain Structural indefinitely).
- **DuckDB**: First-class for both; `ScdWindow` picks half-open predicate in practice per Q-TEMPORAL-MAP-004 Option C to enforce upper bound.
- **Spark**: Structural for both; emission via `QUALIFY ROW_NUMBER() = 1` on Spark 3.4+; subquery wrap pre-3.4. Spark 3.5.x floor per Q-TEMPORAL-MAP-003 Option A.
- **Snowflake**: First-class for both (native `ASOF JOIN ... MATCH_CONDITION`).
- **BigQuery**: Structural for both; emission via `QUALIFY`-based window rewrite (BigQuery supports `QUALIFY` natively).

---

## 5. Gap Catalog

Per-engine inventory of canonical `17` semantics the engine does NOT natively support, and the workaround strategy the adapter uses (or a TODO for future implementation).

### 5.1 DataFusion gap inventory

| Canonical feature | DataFusion status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(ScdWindow)` | No native `ASOF JOIN` | Structural rewrite to `Inner`/`Left` + half-open `FilterNode` (§6.3). | `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` |
| `JoinType::AsOf(SnapshotLatestAtOrBefore)` | No native | Subquery-wrapped `ROW_NUMBER()` latest-at-or-before (§6.4). | same |
| Snapshot time-travel at source level | No vanilla `VERSION AS OF` syntax | Delegate to catalog / Iceberg binding (if present); else WHERE-max (§6.1). | `TD-TEMPORAL-SNAPSHOT-DATAFUSION-NATIVE` |
| `QUALIFY` clause | Not supported 🟡 | Subquery wrap (§6.4). | — |
| `FOR SYSTEM_TIME AS OF` | Not supported | WHERE-max fallback per snapshot source. | same |

**Round-1 posture**: DataFusion is Structural-tier for every temporal shape. `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` records the door-open option if DataFusion lands native `AsOf` (Q-TEMPORAL-MAP-001 Option A: no migration plan assumed).

### 5.2 DuckDB gap inventory

| Canonical feature | DuckDB status | Adapter workaround | TD |
|---|---|---|---|
| Iceberg `VERSION AS OF` | Via `iceberg` extension 🟡 | Feature-gated adapter capability `DuckDbAdapterConfig::iceberg_extension` per Q-TEMPORAL-MAP-002 Option A; WHERE-max fallback when off. | `TD-TEMPORAL-SNAPSHOT-DUCKDB-ICEBERG` |
| Delta `VERSION AS OF` | Via `delta` extension 🟡 | Same posture, `delta_extension` flag. | same |
| `SCD Type2 valid_to` sentinel | Convention-dependent (sentinel vs NULL) | Round-1 `NULL`-aware emission only per Q-TEMPORAL-MAP-007 Option A. | `TD-TEMPORAL-SENTINEL-RATIFICATION` |

**Round-1 posture**: DuckDB is First-class for `ASOF JOIN` (both anchor families); the gaps are at the **source-format** layer (Iceberg / Delta extensions), not the SQL layer.

### 5.3 Spark gap inventory

| Canonical feature | Spark status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(*)` | No native `ASOF JOIN` | `QUALIFY ROW_NUMBER() = 1` rewrite (Spark 3.4+; §6.4). Pre-3.4 uses subquery wrap. | `TD-TEMPORAL-ASOF-SPARK-NATIVE` |
| `QUALIFY` clause | Spark 3.4+ 🟡 | Subquery-wrap fallback for Spark < 3.4. Round-1 floor is 3.5.x per Q-TEMPORAL-MAP-003. | `TD-TEMPORAL-SPARK-QUALIFY-VERSION` |
| Iceberg `VERSION AS OF` | Iceberg connector; Spark SQL syntax native | Emit directly when source is Iceberg. | — |
| Delta `VERSION AS OF` / `TIMESTAMP AS OF` | Delta connector; Spark SQL syntax native (3.3+) 🟡 | Emit directly when source is Delta. | — |
| `Timestamp` tz-naive emission | `TimestampNTZType` requires Spark 3.4+ | Per `types_mapping.md §3.2`, Spark 3.4 is the floor. This registry transitively requires the same. | `TD-ADAPTER-SPARK-TIME` (shared with `types_mapping`) |

### 5.4 Snowflake gap inventory (planned adapter)

| Canonical feature | Snowflake status | Adapter workaround | TD |
|---|---|---|---|
| `AsOfAnchor::ScdWindow` | Native `ASOF JOIN` 🟡 | Half-open predicate per Q-TEMPORAL-MAP-004 Option C. | `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE` |
| Table-level time-travel | Native `AT`/`BEFORE` 🟡 | Emit directly; falls back to WHERE-max for non-time-travel tables. | — |
| `MATCH_RECOGNIZE` | Native | Adapter-extended per §3.2.1. | `TD-TEMPORAL-MATCH-RECOGNIZE` |

### 5.5 BigQuery gap inventory (planned adapter)

| Canonical feature | BigQuery status | Adapter workaround | TD |
|---|---|---|---|
| `JoinType::AsOf(*)` | No native | `QUALIFY`-based rewrite (BigQuery supports `QUALIFY`). | — |
| Table-level time-travel | `FOR SYSTEM_TIME AS OF` (7-day default retention) | Emit directly; honor retention via catalog-layer validation. | `TD-TEMPORAL-BIGQUERY-RETENTION` 🟡 |
| `MATCH_RECOGNIZE` | Native | Adapter-extended per §3.2.1. | `TD-TEMPORAL-MATCH-RECOGNIZE` |

---

## 6. Cross-Engine Idioms

Patterns shared across multiple adapters. Written once here; §3 / §4 / §5 rows reference these sections rather than inline the emission.

### 6.1 WHERE-max snapshot selection

The canonical fallback for "latest snapshot at or before `:as_of`" on engines without native time-travel. Works universally (DataFusion / DuckDB / Spark / Snowflake / BigQuery; any engine with correlated subqueries).

```sql
SELECT ...
FROM snapshots s
WHERE s.snapshotted_at = (
  SELECT MAX(s2.snapshotted_at)
  FROM snapshots s2
  WHERE s2.snapshotted_at <= :as_of
)
```

**Correctness.** Returns the full snapshot at the maximum `snapshotted_at` that is `<=` the probe. If no snapshot exists `<= :as_of`, returns zero rows — which matches `PLAN_E_1730 SnapshotAsOfNoCoveringSnapshot` behavior at the plan layer (`17 §9.3`).

**Performance.** Self-join-like subquery; engines typically hoist `MAX(snapshotted_at)` to a single scan. §6.2's windowed variant may outperform on wide tables.

### 6.2 Windowed latest-at-or-before (ROW_NUMBER variant)

```sql
SELECT *
FROM (
  SELECT *,
    ROW_NUMBER() OVER (ORDER BY snapshotted_at DESC) AS rn
  FROM snapshots
  WHERE snapshotted_at <= :as_of
) t
WHERE rn = 1
```

**Portability.** Universal across any engine with window functions. DuckDB / Spark 3.4+ / Snowflake / BigQuery replace the outer `WHERE rn = 1` with `QUALIFY ROW_NUMBER() OVER (...) = 1` (see §6.4).

**Per-entity version.** For SCD-style as-of joins, add `PARTITION BY entity_id`:

```sql
SELECT *
FROM (
  SELECT *,
    ROW_NUMBER() OVER (PARTITION BY entity_id ORDER BY valid_from DESC) AS rn
  FROM scd_table
  WHERE valid_from <= :probe
) t
WHERE rn = 1
  AND (valid_to > :probe OR valid_to IS NULL)
```

### 6.3 Half-open valid-window predicate (the canonical `ScdWindow` form)

The canonical emission for `AsOfAnchor::ScdWindow`. Per Q-TEMPORAL-MAP-004 Option C, preferred on every engine — including native-`ASOF` engines — because it enforces the upper bound in a single join clause:

```sql
SELECT e.*, c.*
FROM events e
LEFT JOIN customers_scd c
  ON e.customer_id = c.customer_id
 AND c.valid_from <= e.occurred_at
 AND (c.valid_to > e.occurred_at OR c.valid_to IS NULL)
```

**Cardinality guarantee.** In a well-formed Type 2 SCD (no overlapping windows per entity), this join produces ≤ 1 match per `(e.customer_id, e.occurred_at)` pair — matching `17 §5.3`'s `ManyToOne` cardinality.

**Alternative**: native `ASOF JOIN` (DuckDB / Snowflake). See §3.5.1 for why the half-open form is preferred.

### 6.4 `QUALIFY` vs subquery-wrap

`QUALIFY` is the post-window-function filter clause analogous to `HAVING` for `GROUP BY`. Supported on DuckDB, Spark 3.4+, Snowflake, BigQuery, Trino; NOT supported on DataFusion or vanilla ANSI SQL. The adapter picks per engine:

| Engine | Form |
|---|---|
| DuckDB / Spark 3.4+ / Snowflake / BigQuery | `SELECT ... FROM t QUALIFY ROW_NUMBER() OVER (...) = 1` |
| DataFusion / Spark < 3.4 / ANSI | Subquery wrap: `SELECT ... FROM (SELECT ..., ROW_NUMBER() OVER (...) AS rn FROM t) WHERE rn = 1` |

Semantically identical; `QUALIFY` is shorter and avoids the explicit subquery.

### 6.5 Default-current emission

For a history-preserving SCD kind (`Type2` / `Type5` / `Type6`) queried without `Request.temporal.as_of`, the default-current lookup per `17 §6.3`:

1. If `current_flag_dim` is declared: `WHERE is_current = TRUE`.
2. Else if the source uses `NULL`-tailed open-ended rows: `WHERE valid_to IS NULL`.
3. Else fall back to the `PLAN_W_1731 ScdCurrentRowHeuristic` path — `QUALIFY ROW_NUMBER() OVER (PARTITION BY entity_id ORDER BY valid_from DESC) = 1` (or subquery-wrap).

Per Q-TEMPORAL-MAP-007 Option A, Round-1 emission supports only cases 1 and 2; case 3 falls through to the heuristic advisory. Author-declared sentinel support (case 2 with `valid_to = '9999-12-31'` instead of `NULL`) is DEFERRED per `17 §10 D13`.

---

## 7. TECH_DEBT Index

Consolidated list of all `TD-TEMPORAL-*` entries surfaced in this catalog. Each maps back to the originating §; entries retire as adapter implementation, `17` ratification, or engine-landscape shift resolves them.

| TD ID | § | Canonical concern | Engine(s) | Current posture |
|---|---|---|---|---|
| `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` | §5.1 | Native `ASOF JOIN` support | DataFusion | Open — remain Structural indefinitely (Q-TEMPORAL-MAP-001 Option A). Revisit if DataFusion lands native `AsOf`. |
| `TD-TEMPORAL-ASOF-SPARK-NATIVE` | §5.3 | Native `ASOF JOIN` support | Spark | Open — `QUALIFY`-based rewrite is the long-term plan. |
| `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE` | §3.5.1 | Trailing-filter vs half-open predicate for `ScdWindow` on native-`ASOF` engines | DuckDB, Snowflake | Resolved per Q-TEMPORAL-MAP-004 Option C: half-open predicate preferred. |
| `TD-TEMPORAL-ASOF-EMPIRICAL` | §4.2 | Rewrite-tier rows drafted from docs, not empirically verified | All | Blocked on test harness (parallel to `TD-FUNCS-MAPPING-BINOP-EMPIRICAL`). |
| `TD-TEMPORAL-SNAPSHOT-DATAFUSION-NATIVE` | §5.1 | Source-level `VERSION AS OF` syntax | DataFusion | Open — delegate to catalog / Iceberg bindings. |
| `TD-TEMPORAL-SNAPSHOT-DUCKDB-ICEBERG` | §5.2 | Iceberg / Delta extension gating | DuckDB | Open — feature-gated per Q-TEMPORAL-MAP-002 Option A. |
| `TD-TEMPORAL-SNAPSHOT-CADENCE-ROLLUP` | §3.3.3 | Cadence-rollup reducer catalog | All | Open — last-in-period Round-1; full set waits for `25 §…` per `17 §10 D8`. |
| `TD-TEMPORAL-SPARK-QUALIFY-VERSION` | §5.3 | Spark version-floor pin for `QUALIFY` | Spark | Resolved per Q-TEMPORAL-MAP-003 Option A: Spark 3.5.x floor. |
| `TD-TEMPORAL-SENTINEL-RATIFICATION` | §3.4.3 | Author-declared `valid_to` sentinel support | All | Open — `NULL`-aware only Round 1; gated on `17 §10 D13`. |
| `TD-TEMPORAL-MATCH-RECOGNIZE` | §3.2.1 | Sequential-pattern event detection | Snowflake, BigQuery, Trino | Open — adapter-extended per Q-TEMPORAL-MAP-008 Option B. |
| `TD-TEMPORAL-BIGQUERY-RETENTION` | §5.5 | BigQuery 7-day default time-travel retention | BigQuery | Open — catalog-layer validation. |

---

## 8. Versioning

Following `registry/types_mapping.md §4` and `registry/functions_mapping.md §15` precedent, each mapping row SHOULD cite the engine version it was verified against. Round-1 tentative pins per Q-TEMPORAL-MAP-006 Option A (per-catalog drift):

| Engine | Target version(s) | Notes |
|---|---|---|
| DataFusion | 40.x+ 🟡 | Matches `functions_mapping.md §15`. No native `ASOF`; Structural-tier indefinitely. |
| DuckDB | 1.1.x 🟡 | Matches `functions_mapping.md §15` / `types_mapping.md §1`. `ASOF JOIN` since 0.9 (well below floor). Iceberg / Delta extensions Q-TEMPORAL-MAP-002. |
| Spark | 3.5.x 🟡 | Matches `functions_mapping.md §15`. `QUALIFY` required (Spark 3.4+); `TimestampNTZType` required (3.4+); Round-1 pins at 3.5.x. `TD-TEMPORAL-SPARK-QUALIFY-VERSION`. |
| Snowflake | 2024.x 🟡 | `ASOF JOIN` GA since 2023; `MATCH_CONDITION` syntax stable. Pin firms up at adapter ratification. |
| BigQuery | 2024.x 🟡 | `FOR SYSTEM_TIME AS OF` long-available; 7-day default retention (`TD-TEMPORAL-BIGQUERY-RETENTION`). |
| Substrait | 0.48+ 🟡 | Plan-IR target; `AsOf` emitted via extension URI. |

Rows citing features behind a specific engine version carry `(Engine X.Y+)` inline. Unverified rows are marked 🟡; as adapter implementation lands and verifies each row against a live engine, the 🟡 marker is removed and the exact verified version replaces the range. Breaking changes between engine major versions are documented as additional rows or dated annotations — not destructive edits — per `registry/README.md §versioning-and-churn`.

---

## 9. Round-1 Open Items

Unresolved questions parked in [`questions/open/temporal_shape_mapping_questions.md`](../questions/open/temporal_shape_mapping_questions.md). Summary:

| Q | Title | Round-1 position | Blocking? |
|---|---|---|---|
| Q-TEMPORAL-MAP-001 | DataFusion native `ASOF JOIN` adoption | Option A — Structural indefinitely | No |
| Q-TEMPORAL-MAP-002 | DuckDB Iceberg / Delta extension gating | Option A — feature-gated | No |
| Q-TEMPORAL-MAP-003 | Spark `QUALIFY` version-floor pin | Option A — Spark 3.5.x floor | No |
| Q-TEMPORAL-MAP-004 | `ASOF JOIN` `valid_to` closure strategy | Option C — anchor-family-conditional | No |
| Q-TEMPORAL-MAP-005 | Snapshot cadence-rollup reducer policies | Option A — last-in-period only | No |
| Q-TEMPORAL-MAP-006 | Engine version pins | Option A — per-catalog drift | No |
| Q-TEMPORAL-MAP-007 | Sentinel-aware `valid_to` emission | Option A — `NULL`-aware only | No |
| Q-TEMPORAL-MAP-008 | Adapter-extended temporal idioms inventory | Option B — defer to adapter READMEs | No |

None blocks ratification of this registry; all are coordination items with adapter implementation (`34` / `36`), `17 §10` deferrals, or `25 §…` ratifications.

---

## 10. Interaction with Other Documents

- **`foundations/17_temporal_shape.md`** — canonical upstream. `17 §2` defines the shape taxonomy this registry maps; `17 §5.1` defines the `AsOfAnchor` enum `§3.5` / `§4` emit. `17` never depends on this registry's specifics (`00 §6.6`).
- **`foundations/16_composition.md`** — canonical `JoinType` enum extended by `17 §5.1` with `AsOf(anchor)`; this registry maps the extension.
- **`registry/README.md`** — shared policy (engine coverage, versioning, Living status).
- **`registry/types_mapping.md`** — sibling catalog consumed by temporal predicates (`Timestamp` / `Date` / `Interval` emission for `valid_from` / `valid_to` / `snapshotted_at`). Spark's `TimestampNTZType` floor (3.4+) transitively constrains this catalog.
- **`registry/functions_mapping.md`** — sibling catalog consumed by `date_trunc` / `date_part` / `row_number` / comparison operators for temporal emission. Spark 3.5.x floor aligns with this catalog per Q-TEMPORAL-MAP-003.
- **`apis/34_semstrait_planner.md`** — planner's shape-aware strategy dispatch; `Request.temporal` consumption; `AsOf` emission. DEFERRED per `17 §10 D1, D4`. This registry describes the emission target.
- **`apis/35_semstrait_ir.md`** — `PlanNode::Join` carriage of `JoinType::AsOf(anchor)`. DEFERRED per `17 §10 D1`.
- **`apis/36_semstrait_adapter.md`** — the `EngineAdapter` trait + PlanBuilder / Dialect layering that consumes this registry. Per-adapter crates ratify their own `FunctionRewriter` and emission paths; §3 / §4 / §5 rows seed each adapter's implementation.
- **Adapter crates** (future `semstrait-adapter-datafusion`, `-duckdb`, `-spark`, `-snowflake`, `-bigquery`, `-substrait`) — own authoritative per-engine emission tables, feature flags (`DuckDbAdapterConfig::iceberg_extension`, etc.), and adapter-extended temporal idioms (§3.2.1, §5.4, §5.5 rows).
- **`questions/open/temporal_shape_mapping_questions.md`** — parked unresolved questions surfaced by Round-1 drafting.
