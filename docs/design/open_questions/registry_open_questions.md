---
doc: design/open_questions/registry_open_questions
status: Living — consolidated index
purpose: Single-file index of every open question surfaced against any `registry/*_mapping.md` catalog. Pointers only; authoritative bodies live in the per-catalog sidecars.
depends-on:
  - open_questions/functions_mapping_open_questions.md
  - open_questions/join_types_mapping_open_questions.md
  - open_questions/temporal_shape_mapping_open_questions.md
  - registry/README.md
  - registry/functions_mapping.md
  - registry/join_types_mapping.md
  - registry/temporal_shape_mapping.md
---

# Registry — Open Questions Index

> Navigation-only aggregate. This file lists every open question surfaced against a `registry/*_mapping.md` catalog so that a reviewer, adapter implementer, or AI agent can see the full backlog at a glance without opening three sidecars. Each row **points** to the authoritative entry in the per-catalog sidecar file; bodies are not duplicated here. See `../00_overview.md §6.2` for the doc-map rules this index follows.

---

## How to use this index

- **To see what's open** — scan the tables below; every Q-ID links to the full entry.
- **To read the full context** — click through to the per-sidecar file. The sidecar is authoritative for arguments, options, refs, and the Round-1 default / current position.
- **To close an item** — edit the per-sidecar entry (add a `**Resolution.**` block, flip the status line), then flip the **Status** column below to `CLOSED` with a one-line rationale + the closing decision's doc location. Do not move the body; `STATUS.md` + the changelog track when the closure shipped.
- **To add a new item** — append the question to the per-catalog sidecar, then add one row to the matching table below. New rows preserve the `Q-{CAT}-MAP-{NNN}` numbering scheme the sidecars already use.

## Doc map (quick)

| Sidecar | Scope | Count | Longest TD tag |
|---|---|---|---|
| [`functions_mapping_open_questions.md`](functions_mapping_open_questions.md) | Canonical function ↔ engine-function mapping | 20 | `TD-FUNCS-MAPPING-*` |
| [`join_types_mapping_open_questions.md`](join_types_mapping_open_questions.md) | Canonical `JoinType` ↔ engine-native join syntax | 7 | `TD-JOIN-*` |
| [`temporal_shape_mapping_open_questions.md`](temporal_shape_mapping_open_questions.md) | `TemporalShape` variants ↔ engine temporal idioms | 8 | `TD-TEMPORAL-*` |
| **Total** | — | **35** | — |

All three sidecars remain canonical for the detail of their respective questions. This index is **purely navigational**; it carries no independent authority.

---

## `functions_mapping_open_questions.md` — 20 open items

| ID | Topic | Round-1 position (one-liner) | Blocking? |
|---|---|---|---|
| [Q-FUNCS-MAP-001](functions_mapping_open_questions.md#q-funcs-map-001--position-vs-strpos-vs-locate-which-is-canonical) | `position` vs `strpos` vs `locate` canonical name | `position` canonical, 🟡 Spark arg-swap structural rewrite; deferred to Round-3 adapter verification | No |
| [Q-FUNCS-MAP-002](functions_mapping_open_questions.md#q-funcs-map-002--initcap-on-duckdb) | `initcap` on DuckDB | Demoted to adapter-extended; `TD-FUNCS-MAPPING-INITCAP` | No |
| [Q-FUNCS-MAP-003](functions_mapping_open_questions.md#q-funcs-map-003--concat_ws-spark-null-handling-parity) | `concat_ws` NULL handling parity | 🟡 pending DuckDB 1.1.x empirical test | No |
| [Q-FUNCS-MAP-004](functions_mapping_open_questions.md#q-funcs-map-004--left--right-intersection) | `left` / `right` canonical promotion | Adapter-extended only; `TD-FUNCS-MAPPING-LEFT-RIGHT` tracks promotion candidate | No |
| [Q-FUNCS-MAP-005](functions_mapping_open_questions.md#q-funcs-map-005--repeat-is-universal--promote-to-canonical) | `repeat` canonical promotion | 🟡 "canonical pending `14a` Round-2"; `TD-FUNCS-MAPPING-REPEAT` | No |
| [Q-FUNCS-MAP-006](functions_mapping_open_questions.md#q-funcs-map-006--regexp_replace-4-arg-variant) | `regexp_replace` 4-arg variant | 3-arg canonical; `TD-FUNCS-MAPPING-REGEXP-REPLACE-4ARG` | No |
| [Q-FUNCS-MAP-007](functions_mapping_open_questions.md#q-funcs-map-007--date_part-vs-extract-which-is-canonical) | `date_part` vs `extract` canonical | `date_part` canonical; `extract` adapter-emission alias; 🟡 until `14a` Round-2 | No |
| [Q-FUNCS-MAP-008](functions_mapping_open_questions.md#q-funcs-map-008--date_add-semantics-across-engines) | `date_add` semantics | Canonical + Spark `Structural` rewrite to `d + i`; `TD-FUNCS-MAPPING-DATE-ADD-SPARK` | No |
| [Q-FUNCS-MAP-009](functions_mapping_open_questions.md#q-funcs-map-009--date_diff-arity--unit-arg) | `date_diff` arity / unit arg | 2-arg canonical `date_diff(d1, d2)` returning integer days | No |
| [Q-FUNCS-MAP-010](functions_mapping_open_questions.md#q-funcs-map-010--current_date--current_timestamp-parenless-forms) | `current_date` paren vs parenless | Always paren form; low-priority style question | No |
| [Q-FUNCS-MAP-011](functions_mapping_open_questions.md#q-funcs-map-011--to_date-on-duckdb) | `to_date` on DuckDB | Partial; documented with per-adapter emit rules | No |
| [Q-FUNCS-MAP-012](functions_mapping_open_questions.md#q-funcs-map-012--binaryop-per-engine-promotion-tables-need-empirical-verification) | BinaryOp per-engine promotion verification | All rows 🟡; `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` harness pending | No |
| [Q-FUNCS-MAP-013](functions_mapping_open_questions.md#q-funcs-map-013--non-closed-aggregate-intersection) | Non-closed aggregate intersection | `TD-FUNCS-MAPPING-AGG-INTERSECTION` — every row 🟡 | No |
| [Q-FUNCS-MAP-014](functions_mapping_open_questions.md#q-funcs-map-014--greatest--least-null-propagation) | `greatest` / `least` null propagation | SQL-standard (NULL propagates); DuckDB row 🟡; `TD-FUNCS-MAPPING-GREATEST-LEAST-NULL` | No |
| [Q-FUNCS-MAP-015](functions_mapping_open_questions.md#q-funcs-map-015--ifnull--nvl--if-intersection) | `ifnull` / `nvl` / `if` canonical | All demoted to adapter-extended; authors use `coalesce` / `Case` | No |
| [Q-FUNCS-MAP-016](functions_mapping_open_questions.md#q-funcs-map-016--binaryop-integer-division-result-type) | BinaryOp integer-division result type | 🟡; `TD-FUNCS-MAPPING-INT-DIV-RESULT` | No |
| [Q-FUNCS-MAP-017](functions_mapping_open_questions.md#q-funcs-map-017--safedivide-rendering) | `SafeDivide` rendering form | Universal `a / NULLIF(b, 0)`; Spark 3.3+ MAY opt into `try_divide`; 🟡 | No |
| [Q-FUNCS-MAP-018](functions_mapping_open_questions.md#q-funcs-map-018--adapter-extended-function-inventory) | Adapter-extended function inventory | Seed list only 🟡; authoritative lists live in adapter crates; `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` | No |
| [Q-FUNCS-MAP-019](functions_mapping_open_questions.md#q-funcs-map-019--adapter-crate-identity--version-scheme) | Adapter crate identity / version scheme | Engine-version only, per `types_mapping.md` precedent | No |
| [Q-FUNCS-MAP-020](functions_mapping_open_questions.md#q-funcs-map-020--datafusion-version-pin) | DataFusion version pin | "DataFusion 40.x+" tentative; final pin awaits `apis/36_semstrait_adapter.md` | No |

## `join_types_mapping_open_questions.md` — 7 open items

| ID | Topic | Round-1 position (one-liner) | Blocking? |
|---|---|---|---|
| [Q-JOIN-MAP-001](join_types_mapping_open_questions.md#q-join-map-001--explicit-inner-keyword-vs-bare-join) | Explicit `INNER` keyword vs bare `JOIN` | Option A — always emit explicit `INNER JOIN` | No |
| [Q-JOIN-MAP-002](join_types_mapping_open_questions.md#q-join-map-002--on-vs-using-emission-form) | `ON` vs `USING` emission form | Option A — always emit `ON` | No |
| [Q-JOIN-MAP-003](join_types_mapping_open_questions.md#q-join-map-003--right-join-auto-rewrite-to-left-with-swapped-operands) | `RIGHT JOIN` auto-rewrite to `LEFT` | Option A — preserve author orientation | No |
| [Q-JOIN-MAP-004](join_types_mapping_open_questions.md#q-join-map-004--asof-rewrite-tier-table-authority) | `AsOf` rewrite-tier-table authority | Option A — `temporal_shape_mapping.md §4.2` authoritative; this registry's `§4.2` is navigation-aid companion | No |
| [Q-JOIN-MAP-005](join_types_mapping_open_questions.md#q-join-map-005--lateral-reserved-variant-disposition) | `LATERAL` reserved-variant disposition | Option A — adapter-extended per engine; no canonical promotion | No |
| [Q-JOIN-MAP-006](join_types_mapping_open_questions.md#q-join-map-006--cardinality-informed-distinct--hint-auto-emission) | Cardinality-informed `DISTINCT` / hint auto-emission | Option A — no automatic emission; `TD-JOIN-CARDINALITY-HINTS` tracks future opt-in | No |
| [Q-JOIN-MAP-007](join_types_mapping_open_questions.md#q-join-map-007--full-outer-emulation-for-historical-dialects-sqlite-mysql) | `FULL OUTER` emulation for historical dialects | Option B — pattern documented in `§6.3`; no active engine row consumes it | No |

## `temporal_shape_mapping_open_questions.md` — 8 open items

| ID | Topic | Round-1 position (one-liner) | Blocking? |
|---|---|---|---|
| [Q-TEMPORAL-MAP-001](temporal_shape_mapping_open_questions.md#q-temporal-map-001--datafusion-native-asof-join-adoption-timeline) | DataFusion native `ASOF JOIN` adoption timeline | Option A — Structural-tier DataFusion; `TD-TEMPORAL-ASOF-DATAFUSION-NATIVE` | No |
| [Q-TEMPORAL-MAP-002](temporal_shape_mapping_open_questions.md#q-temporal-map-002--duckdb-iceberg--delta-extension-gating-policy) | DuckDB Iceberg / Delta extension gating | Option A — feature-gated emission; `TD-TEMPORAL-SNAPSHOT-DUCKDB-ICEBERG` | No |
| [Q-TEMPORAL-MAP-003](temporal_shape_mapping_open_questions.md#q-temporal-map-003--spark-qualify-version-floor-pin) | Spark `QUALIFY` version-floor pin | Option A — Spark 3.5.x floor | No |
| [Q-TEMPORAL-MAP-004](temporal_shape_mapping_open_questions.md#q-temporal-map-004--asof-join-valid_to-closure-adapter-side-filter-vs-plan-level-rewrite) | `ASOF JOIN` `valid_to` closure strategy | Option A — adapter-side filter | No |
| [Q-TEMPORAL-MAP-005](temporal_shape_mapping_open_questions.md#q-temporal-map-005--snapshot-cadence-rollup-reducer-policies) | Snapshot cadence-rollup reducer policies | Policy catalog in §… ; `TD-TEMPORAL-CADENCE-REDUCERS` | No |
| [Q-TEMPORAL-MAP-006](temporal_shape_mapping_open_questions.md#q-temporal-map-006--engine-version-pins) | Engine version pins (cross-catalog consistency) | Align with `functions_mapping.md §15` pins | No |
| [Q-TEMPORAL-MAP-007](temporal_shape_mapping_open_questions.md#q-temporal-map-007--sentinel-aware-valid_to-emission-coordination-with-17-10-d13) | Sentinel-aware `valid_to` emission coordination with `17 §10 D13` | `17 §10 D13` authoritative for sentinel semantics; registry emits conformingly | No |
| [Q-TEMPORAL-MAP-008](temporal_shape_mapping_open_questions.md#q-temporal-map-008--adapter-extended-time-travel--bi-temporal-idioms-inventory) | Adapter-extended time-travel / bi-temporal idioms inventory | Seed list only 🟡; authoritative lists live in adapter crates | No |

---

## Status rollup (2026-04-17)

- **Total open items across registry sidecars:** 35 (20 + 7 + 8).
- **Blocking for Round-1 ratification:** 0. Every item has a documented Round-1 default or current position that the corresponding `registry/*_mapping.md` catalog uses; closures land either with adapter-implementation review (majority), adapter-harness empirical verification (`TD-FUNCS-MAPPING-BINOP-EMPIRICAL`, `TD-FUNCS-MAPPING-AGG-INTERSECTION`), or cross-catalog version-pin alignment (`Q-TEMPORAL-MAP-006` + `Q-FUNCS-MAP-020`).
- **Tech-debt tag families:** `TD-FUNCS-MAPPING-*`, `TD-JOIN-*`, `TD-TEMPORAL-*`. Each sidecar carries the full TD-tag-to-Q-ID cross-reference table in its final section.
- **Cross-catalog coordination items:** `Q-FUNCS-MAP-020` + `Q-TEMPORAL-MAP-006` (engine version pins); `Q-JOIN-MAP-004` + `Q-TEMPORAL-MAP-001`/`004` (`AsOf` authority split between `join_types_mapping.md` and `temporal_shape_mapping.md`).

---

## See also

- `./registry/README.md` — registry-catalog index.
- `../00_overview.md §6.2` — full doc-map and `open_questions/` precedence.
- `../STATUS.md` — project-level phase map; registry ratification state is tracked per-catalog there.

> **Maintenance note.** If the sidecar count changes (a new catalog is added, an existing one is split / merged), update both the doc-map table at the top of this file and `registry/README.md §Current contents` in the same commit.
