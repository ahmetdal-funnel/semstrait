---
doc: design/questions/open/temporal_shape_mapping_questions
status: Living
purpose: Parked unresolved questions discovered while drafting `registry/temporal_shape_mapping.md`
depends-on:
  - foundations/17_temporal_shape.md
  - registry/temporal_shape_mapping.md
  - registry/functions_mapping.md
  - registry/types_mapping.md
  - apis/34_semstrait_planner.md
  - apis/35_semstrait_ir.md
  - apis/36_semstrait_adapter.md
---

# Open Questions — `registry/temporal_shape_mapping.md`

> Unresolved items surfaced during Round-1 drafting of the temporal-shape-mapping catalog. Each entry restates the question, lists its ratified references, enumerates options, records the Round-1 default, and marks blocking status. Questions migrate out of this file as adapter implementation lands or engine landscape shifts.

> **Index pointer.** For a one-file view of every open question across all registry sidecars (functions / join-types / temporal-shape), see [`registry_questions.md`](registry_questions.md). That index is pure navigation — the full question bodies stay here.

---

## Q-TEMPORAL-MAP-001 — DataFusion native `ASOF JOIN` adoption timeline

**Context.** DataFusion's `LogicalPlan::Join` carries `JoinType` variants `Inner | Left | Right | Full | LeftSemi | RightSemi | LeftAnti | RightAnti | LeftMark` as of drafting (40.x+ 🟡). No `AsOf` variant exists. The DataFusion community has surfaced `ASOF JOIN` proposals periodically; no ratified landing date.

**Question.** Should this registry assume DataFusion remains Structural-tier indefinitely, or record a migration plan for when the community lands a native `AsOf` variant?

**Refs.**
- `temporal_shape_mapping.md §4.2` row 1 — current DataFusion tier.
- `temporal_shape_mapping.md §5.1` — DataFusion gap inventory.
- `temporal_shape_mapping.md §7` `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE`.
- `17 §5.1` — canonical `JoinType::AsOf(AsOfAnchor)`.

**Options.**
- **A. Remain Structural indefinitely.** Current Round-1 posture. Adapter does PlanBuilder-layer rewrite to `Inner/Left` + half-open `FilterNode` for `ScdWindow`; `Left` + `ROW_NUMBER` + subquery filter for `SnapshotLatestAtOrBefore`. No migration plan.
- **B. Record a First-class-tier row conditioned on a DataFusion feature flag.** When DataFusion lands `AsOf` natively, the adapter gains a feature gate (`datafusion-asof-native`) and promotes to First-class tier. §4.2 gains a second DataFusion row.
- **C. Defer to adapter crate.** Let the DataFusion adapter's own README maintain the DF-version-specific emission table; registry carries only the Structural row as the Round-1 assumption.

**Arguments for A (adopted).**
- Stable Round-1 posture. No speculative engine-roadmap commitment in the registry.
- Structural form is well-defined and adequate for every `AsOfAnchor` family in the canonical roster.

**Arguments for B.**
- Registry is the natural home for cross-engine tier comparisons; per-version conditions already appear (Spark 3.4+ `QUALIFY`).

**Arguments for C.**
- Registry mirrors; adapter crate ratifies. Consistent with the `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` pattern.

**Current position in `temporal_shape_mapping.md`.** Option A — Structural-tier DataFusion with `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` tracking. Revisit if DataFusion lands `AsOf` natively; the migration is a new row plus tier-column flip, no semantic change to consumers.

**Blocking?** No.

---

## Q-TEMPORAL-MAP-002 — DuckDB Iceberg / Delta extension-gating policy

**Context.** DuckDB's `iceberg` and `delta` extensions provide time-travel syntax (`iceberg_scan('path', version => N)`, etc.) 🟡. Vanilla DuckDB has no snapshot concept. The adapter can either assume-extensions-present (emit the extension form and let the DuckDB session fail if the extension is not loaded) or feature-gate behind an adapter capability flag.

**Question.** What is the registry's posture on extension gating?

**Refs.**
- `temporal_shape_mapping.md §3.3.1` DuckDB rows 3–5.
- `temporal_shape_mapping.md §5.2` DuckDB gap inventory.
- `17 §6.2` — Snapshot `as_of` semantics.

**Options.**
- **A. Feature-gate behind adapter capability.** Adapter crate exposes `DuckDbAdapterConfig { iceberg_extension: bool, delta_extension: bool }`; registry references the feature flags. Safe default: off. When both are off, every Snapshot source falls back to the `WHERE`-max structural form.
- **B. Assume extensions present when source is Iceberg / Delta.** When `ResolvedSource` carries an Iceberg or Delta catalog kind, emit the extension form unconditionally; downstream session failure if extension missing is the user's problem.
- **C. Defer to adapter.** Registry documents the extension idioms but does not prescribe the gating policy.

**Arguments for A (adopted).**
- Matches `types_mapping.md §3.2`'s version-floor-conditional posture for Spark.
- Explicit capability flag gives authors a clear signal; reliable emission.

**Arguments for B.**
- Reduces adapter configuration surface.

**Arguments for C.**
- Consistent with `functions_mapping.md §15`'s posture on adapter-crate-owned details.

**Current position in `temporal_shape_mapping.md`.** Option A — feature-gated emission noted in §3.3.1 rows 3–5; `TD-TEMPORAL-SNAPSHOT-DUCKDB-ICEBERG` tracks the detail.

**Blocking?** No — lands with the DuckDB adapter's capability surface in `36 §…`.

---

## Q-TEMPORAL-MAP-003 — Spark `QUALIFY` version-floor pin

**Context.** Spark added `QUALIFY` in version 3.4 🟡. Structural `AsOf` rewrite on Spark < 3.4 requires subquery wrapping; Spark 3.4+ can use `QUALIFY ROW_NUMBER() OVER (...) = 1`. `types_mapping.md §3.2` floors Spark at 3.4 for `TimestampNTZType`; `functions_mapping.md §15` pins Spark at 3.5.x.

**Question.** Does this registry adopt Spark 3.5.x as the version floor (matching `functions_mapping.md`), or allow 3.4+ (matching `types_mapping.md` transitively)?

**Refs.**
- `temporal_shape_mapping.md §8` Spark row.
- `temporal_shape_mapping.md §5.3` `TD-TEMPORAL-SPARK-QUALIFY-VERSION`.
- `types_mapping.md §3.2` — Spark TimestampNTZType requires 3.4+.
- `functions_mapping.md §15` — Spark 3.5.x pin.

**Options.**
- **A. Align with `functions_mapping.md` — Spark 3.5.x floor.** `QUALIFY` is always available; structural rewrites emit the compact `QUALIFY`-based form. Pre-3.5 Spark is not supported; `ADAPT_E_*` at adapter-init time.
- **B. Align with transitive `types_mapping.md` — Spark 3.4+ floor.** Both `TimestampNTZType` and `QUALIFY` are available. Canonical Timestamp is the tightest constraint.
- **C. Branch per-version.** Spark 3.4+ uses `QUALIFY`; Spark 3.3 requires `Timestamp` emulation fallback and subquery-wrapped `ROW_NUMBER`. Maximum compatibility; maximum adapter complexity.

**Arguments for A (adopted).**
- Single floor across registry catalogs; simplifies cross-catalog reasoning.
- Spark 3.5 is the current stable branch; pre-3.5 will EOL during semstrait Round-1 adopter window.

**Arguments for B.**
- Widens the supported Spark range at no material cost.

**Arguments for C.**
- Best compatibility story for long-tail Spark users.

**Current position in `temporal_shape_mapping.md`.** Option A — Spark 3.5.x floor in §8; Round-1 emission always uses `QUALIFY` where a window-function rewrite is needed. Re-home to Option B if adapter-side empirical verification finds 3.4 users with `QUALIFY`-compatible emission.

**Blocking?** No.

---

## Q-TEMPORAL-MAP-004 — `ASOF JOIN` `valid_to` closure: adapter-side filter vs plan-level rewrite

**Context.** Native `ASOF JOIN` in DuckDB / Snowflake 🟡 accepts a single inequality (`probe >= valid_from`). This does NOT enforce the `valid_to > probe` upper bound required by canonical `AsOfAnchor::ScdWindow` (`17 §5.1`). Per §3.5.1, the adapter emits a trailing `WHERE matched_row.valid_to > probe OR matched_row.valid_to IS NULL`. Alternatively, the planner can rewrite `ScdWindow` anchors to a plain `Inner/Left` + half-open `FilterNode` on ALL engines (including First-class-tier ones), dropping the `ASOF` emission entirely.

**Question.** Should First-class `ASOF JOIN` engines emit native syntax + trailing closure (current Round-1 posture), or skip native `ASOF` entirely and always use the half-open-predicate form (§6.3)?

**Refs.**
- `temporal_shape_mapping.md §3.5.1` — trailing-closure rationale.
- `temporal_shape_mapping.md §6.3` — alternative canonical form.
- `temporal_shape_mapping.md §7` `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE`.
- `17 §5.1` — `AsOfAnchor::ScdWindow` semantics.

**Options.**
- **A. Native `ASOF` + trailing closure.** First-class engines use native syntax; adapter appends `valid_to` filter. Captures engine optimizer benefits (ASOF-aware reads).
- **B. Universal half-open predicate.** ALL engines — even First-class — emit `ON c.valid_from <= e.probe AND (c.valid_to > e.probe OR c.valid_to IS NULL)` verbatim. Simpler; single-form canonical emission. First-class engines lose `ASOF`-specific optimizations but gain predicate clarity.
- **C. Anchor-family-conditional.** `ScdWindow` anchor always takes Option B (half-open); `SnapshotLatestAtOrBefore` takes Option A (native `ASOF` since the anchor has no upper bound).

**Arguments for A.**
- Engine optimizers may have `ASOF`-specific fast paths (DuckDB's `ASOF` implementation has been tuned).
- Keeps emission close to the "first-class" description in §4.

**Arguments for B.**
- Simplest adapter surface: one emission form for every engine for `ScdWindow`.
- Eliminates `TD-TEMPORAL-ASOF-SCD-WINDOW-CLOSURE` entirely — the trailing-closure gap vanishes.
- SCD Type-2 well-formedness already guarantees ≤1 match per entity-probe; `ASOF`'s syntactic benefit is minimal.

**Arguments for C (adopted).**
- Matches the anchor-specific semantics: `ScdWindow` has a closed upper bound (half-open), so half-open predicate is the natural form; `SnapshotLatestAtOrBefore` has no upper bound, so `ASOF` is the natural form.
- §6.3 already notes this: "the adapter picks the plainer form" for `ScdWindow` on First-class engines.

**Current position in `temporal_shape_mapping.md`.** Option C adopted implicitly in §6.3. §4.2 records `ScdWindow` tier as First-class on DuckDB / Snowflake 🟡 for reference, but §6.3 and §3.5.1 document that the adapter may legitimately pick the half-open form even on First-class engines. Emission-strategy selection is an adapter optimization detail.

**Blocking?** No.

---

## Q-TEMPORAL-MAP-005 — Snapshot cadence-rollup reducer policies

**Context.** `17 §4.1` specifies that rolling up daily snapshots to a weekly view requires a reducer (first / last / average / configurable), with last-in-period as the Round-1 default. `17 §10 D8` defers the full reducer-policy catalog to `25 §…`. This registry (§3.3.3) drafts per-engine idioms for last-in-period; other reducers (first-in-period, average, max) are not yet specified.

**Question.** Which reducers does this registry enumerate as ratified Round-1 forms, which wait for `25 §…`?

**Refs.**
- `temporal_shape_mapping.md §3.3.3` — last-in-period drafts.
- `temporal_shape_mapping.md §7` `TD-TEMPORAL-SNAPSHOT-CADENCE-ROLLUP`.
- `17 §4.1` — cadence-rollup policy.
- `17 §10 D8` — DEFERRED.

**Options.**
- **A. Last-in-period only.** This registry carries only last-in-period emission per §3.3.3. Other reducers await `25 §…`.
- **B. Enumerate full reducer set (first / last / average / max / min).** This registry drafts per-engine idioms for the full set; `25 §…` ratifies policy defaults and author-authoring surface.
- **C. Full set + author-configurable.** Option B + a Semantics-level `cadence_reducer:` field on `Snapshot` declarations.

**Arguments for A (adopted).**
- Registry scope stays narrow. `25 §…` is the canonical home for the reducer catalog.
- Round-1 default (last-in-period) has a well-defined emission; other reducers can be added as rows when `25 §…` ratifies them.

**Arguments for B.**
- All reducer emissions are well-known SQL idioms; no information is gained by waiting.

**Arguments for C.**
- Complete; but cross-doc scope: `25 §…` owns the authoring surface.

**Current position in `temporal_shape_mapping.md`.** Option A — §3.3.3 covers last-in-period; full enumeration waits for `25 §…` per `17 §10 D8`.

**Blocking?** No.

---

## Q-TEMPORAL-MAP-006 — Engine version pins

**Context.** `temporal_shape_mapping.md §8` carries tentative pins: DataFusion 40.x+ 🟡, DuckDB 1.1.x 🟡, Spark 3.5.x 🟡. `functions_mapping.md §15` and `types_mapping.md §4` carry sibling pins. The pins shift as adapter crates land and verify against live engines.

**Question.** Should registry pins be re-ratified cross-catalog at once (single coordination pass), or allowed to drift per-catalog?

**Refs.**
- `temporal_shape_mapping.md §8`.
- `functions_mapping.md §15`.
- `types_mapping.md §4`.
- `README.md §versioning-and-churn`.

**Options.**
- **A. Per-catalog drift.** Each registry catalog carries its own pins; cross-catalog coordination is documentary (not enforced).
- **B. Cross-catalog unified pins.** A single `registry/engine_versions.md` (hypothetical future doc) defines the pins; all catalogs reference it.
- **C. Per-adapter-crate pin in the adapter's own README; registries mirror.** The adapter crate's README is authoritative; registries cite.

**Arguments for A (adopted).**
- Lowest coordination overhead. Matches current practice.

**Arguments for B.**
- Ensures cross-catalog consistency automatically.

**Arguments for C.**
- Matches `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` precedent (adapter README authoritative for adapter-specific inventory).

**Current position in `temporal_shape_mapping.md`.** Option A — per-catalog drift. Pins in §8 are tentative 🟡; adapter ratification per `36 §…` (forward-ref) will settle.

**Blocking?** No.

---

## Q-TEMPORAL-MAP-007 — Sentinel-aware `valid_to` emission coordination with `17 §10 D13`

**Context.** `17 §2.3` notes SCD Type-2 open-ended rows conventionally carry `NULL` or a sentinel (e.g. `'9999-12-31'`). `17 §6.3` default-current heuristic picks the row with `current_flag_dim = TRUE` when declared, else `valid_to IS NULL`, else `MAX(valid_from)` per entity (the `PLAN_W_1731 ScdCurrentRowHeuristic` path). `17 §10 D13` tracks a future `valid_to_sentinel: '9999-12-31'` field on the SCD shape payload.

**Question.** When the sentinel-aware authoring field lands, this registry will need a row per engine documenting the `WHERE valid_to = :sentinel` emission form. What's the Round-1 posture until then?

**Refs.**
- `temporal_shape_mapping.md §3.4.3` — sentinel-vs-NULL table.
- `temporal_shape_mapping.md §6.5` — default-current emission.
- `temporal_shape_mapping.md §7` `TD-TEMPORAL-SENTINEL-RATIFICATION`.
- `17 §6.3` — default-current heuristic.
- `17 §10 D13` — DEFERRED.

**Options.**
- **A. `NULL`-aware emission only.** Current Round-1 posture. All adapters emit `valid_to IS NULL OR valid_to > :as_of`. Sentinel-convention authors get `PLAN_W_1731` heuristic path (every query pays the `MAX(valid_from)` cost).
- **B. Adapter-config-based sentinel.** Adapter exposes `SentinelConfig { valid_to_sentinel: Option<String> }`; registry renders the configured sentinel in the predicate. Author-side workaround until `17 §10 D13` lands.
- **C. Wait for `17 §10 D13`.** No Round-1 sentinel support; authors use `NULL` or accept the heuristic.

**Arguments for A (adopted).**
- Minimal adapter surface; no forward-ref to unratified `17` extensions.
- Heuristic path is always available.

**Arguments for B.**
- Useful workaround for shops with existing `'9999-12-31'` conventions; frictionful migration to `NULL`.

**Arguments for C.**
- Matches A functionally; explicit about the wait.

**Current position in `temporal_shape_mapping.md`.** Option A — §3.4.3 / §6.5 document `NULL`-aware emission only; `TD-TEMPORAL-SENTINEL-RATIFICATION` tracks coordination with `17 §10 D13`.

**Blocking?** No.

---

## Q-TEMPORAL-MAP-008 — Adapter-extended time-travel / bi-temporal idioms inventory

**Context.** Several engines expose time-travel or temporal idioms that are NOT in canonical `17` (`MATCH_RECOGNIZE` on BigQuery / Snowflake; Snowflake's `BEFORE (STATEMENT => '...')` transactional time-travel; BigQuery's system-time vs data-time split; DuckDB's `time_bucket()` with arbitrary offsets). These are adapter-extended on a per-engine basis.

**Question.** Should this registry maintain a seed inventory of adapter-extended temporal idioms (paralleling `functions_mapping.md §12`), or leave them entirely to per-adapter crate READMEs?

**Refs.**
- `temporal_shape_mapping.md §3.2.1` — `MATCH_RECOGNIZE` exclusion.
- `temporal_shape_mapping.md §3.3.1` rows 9–10 — Snowflake `BEFORE (STATEMENT => ...)`.
- `temporal_shape_mapping.md §7` `TD-TEMPORAL-MATCH-RECOGNIZE`.
- `functions_mapping.md §12` — adapter-extended inventory precedent.

**Options.**
- **A. Seed inventory in registry.** Add a `§12` analogue to this catalog enumerating each engine's adapter-extended temporal idioms; authoritative list lives in each adapter crate's README.
- **B. Defer entirely.** No adapter-extended section; per-adapter READMEs own. Registry references them.
- **C. Fold into the Gap Catalog.** Non-canonical engine features become "inverse gaps" (engines have capability, canonical does not cover). Listed in §5 rows with a "not-a-gap-but-noteworthy" marker.

**Arguments for A.**
- Matches `functions_mapping.md §12` precedent; readers have one place to find all engine-specific temporal features.

**Arguments for B (adopted).**
- Registry scope stays tight around canonical-shape mappings.
- Adapter-extended temporal features are less numerous than function extensions; seed value is lower.

**Arguments for C.**
- Reuses existing structure.

**Current position in `temporal_shape_mapping.md`.** Option B — `§3.2.1` calls out `MATCH_RECOGNIZE` as explicitly out of scope; no dedicated adapter-extended section. Revisit if adapter-extended temporal inventory grows past a handful of entries.

**Blocking?** No — adapter-crate READMEs own the authoritative list.

---

## Summary — Round-1 position

| Q | Title | Round-1 position | Blocking? |
|---|---|---|---|
| Q-TEMPORAL-MAP-001 | DataFusion native `ASOF JOIN` adoption | Option A — Structural indefinitely; revisit if DF lands native `AsOf` | No |
| Q-TEMPORAL-MAP-002 | DuckDB Iceberg / Delta extension gating | Option A — feature-gated adapter capability | No |
| Q-TEMPORAL-MAP-003 | Spark `QUALIFY` version-floor pin | Option A — Spark 3.5.x floor (aligned with `functions_mapping.md`) | No |
| Q-TEMPORAL-MAP-004 | `ASOF JOIN` `valid_to` closure strategy | Option C — anchor-family-conditional; `ScdWindow` always half-open, `SnapshotLatestAtOrBefore` native `ASOF` | No |
| Q-TEMPORAL-MAP-005 | Snapshot cadence-rollup reducer policies | Option A — last-in-period only; full set waits for `25 §…` | No |
| Q-TEMPORAL-MAP-006 | Engine version pins | Option A — per-catalog drift | No |
| Q-TEMPORAL-MAP-007 | Sentinel-aware `valid_to` emission | Option A — `NULL`-aware only; wait for `17 §10 D13` | No |
| Q-TEMPORAL-MAP-008 | Adapter-extended temporal idioms inventory | Option B — defer to adapter-crate READMEs | No |

None of the open questions blocks ratification of `registry/temporal_shape_mapping.md`. All are coordination items with adapter implementation, `17 §10` deferrals, or `25 §…` ratifications.
