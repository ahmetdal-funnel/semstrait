---
doc: design/open_questions/functions_mapping_open_questions
status: Living
purpose: Parked unresolved questions discovered while drafting `registry/functions_mapping.md`
depends-on:
  - foundations/14a_function_catalog.md
  - registry/functions_mapping.md
---

# Open Questions — `registry/functions_mapping.md`

> Unresolved items surfaced during Round-2 drafting of the functions-mapping catalog. Each entry links to a location in the mapping doc where the question currently resolves to 🟡 / `TD-FUNCS-MAPPING-*` placeholder. Questions migrate out of this file as they are answered by adapter implementation, empirical verification, or a `14a`-level ratification.

---

## Q-FUNCS-MAP-001 — `position` vs `strpos` vs `locate`: which is canonical?

**Context.** `14a §4.2` lists `position` as a candidate with the note "verify DF name — may need rename to `strpos`". Legacy `FUNCTION_CATALOG.md §7 S21` picks `position` as canonical and uses a DataFusion name-remap (`strpos`) plus a Spark structural rewrite (argument order is reversed — `locate(substr, str)`).

**Question.** Should the canonical name remain `position` (intersection via structural rewrite on Spark), or should the function be demoted to adapter-extended per Q10 (since Spark's divergence is arg-order-structural, not merely name-remap)?

**Status.** Currently flagged `position` as canonical with 🟡 on Spark row; treating the arg swap as a `Structural` rewrite. Decision deferred to Round-3 adapter verification.

---

## Q-FUNCS-MAP-002 — `initcap` on DuckDB

**Context.** Legacy catalog §7 S18 records `initcap` as "N/A (no equivalent)" on DuckDB. DataFusion and Spark both have it natively.

**Question.** Per Q10 intersection-only canonicalization, should `initcap` be in the canonical catalog? Current reading: demote to adapter-extended (DataFusion + Spark), not canonical. A DuckDB implementation would require a structural rewrite via `upper(substr) || lower(rest-of-words)` — non-trivial, and arguably better authored explicitly.

**Status.** Currently demoted to adapter-extended (§6 of mapping doc). Tracked as `TD-FUNCS-MAPPING-INITCAP`.

---

## Q-FUNCS-MAP-003 — `concat_ws` Spark NULL handling parity

**Context.** Legacy §7 S4 claims all three engines skip NULLs in `concat_ws`. Spark's actual behavior differs: NULL arguments are silently skipped, but the separator being NULL returns NULL. DataFusion matches Spark. DuckDB behavior less certain.

**Question.** Is DuckDB's `concat_ws` NULL-skip semantics truly identical to Spark / DataFusion, or does it return NULL on any NULL argument?

**Status.** Row marked 🟡 pending empirical test against DuckDB 1.1.x.

---

## Q-FUNCS-MAP-004 — `left` / `right` intersection

**Context.** Legacy §7 S11 / S12 lists `left(str, n)` and `right(str, n)` as universal. `14a §4.2` does NOT list them among candidates. DataFusion and DuckDB both support them natively; Spark has `left` / `right` since 3.2.

**Question.** Should `left` / `right` be canonical (added to `14a §4.2` in Round-2 intersection scan) or remain adapter-extended? Alternatively, kept out of canonical and expressed via `substring`?

**Status.** Not in canonical v1 catalog per `14a §4.2` candidate list. Legacy entries cited in the per-engine adapter-extended section as DataFusion / DuckDB / Spark ≥3.2 entries. `TD-FUNCS-MAPPING-LEFT-RIGHT` tracks a potential promotion.

---

## Q-FUNCS-MAP-005 — `repeat` is universal — promote to canonical?

**Context.** Legacy §7 S20 records `repeat(str, n)` as name-only across all three engines. Not in `14a §4.2`'s candidate list.

**Question.** Include in canonical catalog? All three engines name-only — trivial intersection candidate.

**Status.** Proposed canonical promotion in `TD-FUNCS-MAPPING-REPEAT`. Currently documented as "canonical pending `14a` Round-2 update" 🟡.

---

## Q-FUNCS-MAP-006 — `regexp_replace` 4-arg variant

**Context.** Legacy §8 P5 notes Spark's 4th arg is `position` (1-indexed start), while DataFusion / DuckDB treat the 4th arg as `flags` / `options`. Current posture: canonical restricted to 3-arg form.

**Question.** Should the 4-arg form be expressible at all? Current: no — 3-arg canonical only. Adapter-extended entries could register a 4-arg `regexp_replace` per-engine, but semantics diverge such that a single canonical 4-arg is not possible.

**Status.** Documented as 3-arg canonical. `TD-FUNCS-MAPPING-REGEXP-REPLACE-4ARG` tracks the divergence.

---

## Q-FUNCS-MAP-007 — `date_part` vs `extract`: which is canonical?

**Context.** `14a §4.4` lists both `extract` and (implicitly via D2 legacy) `date_part` as candidates. `extract(YEAR FROM expr)` is ANSI SQL; `date_part('year', expr)` is a function-form preferred by DataFusion / DuckDB / Spark. They are semantically equivalent in the first-class trio.

**Question.** Canonical name is `date_part` (function form, matches all three engines name-only) or `extract` (ANSI SQL form)? Legacy picks `date_part`. Worth confirming at Round-2 intersection scan.

**Status.** Currently canonical `date_part` with `extract` listed as an adapter-emission form (all three accept it). 🟡 until `14a` Round-2 ratifies.

---

## Q-FUNCS-MAP-008 — `date_add` semantics across engines

**Context.** Legacy §9 D5 documents three different `date_add` signatures:
- DataFusion: structural rewrite `d + i` (no `date_add` function)
- DuckDB: `date_add(date, interval)` (interval form)
- Spark: `date_add(start_date, num_days)` (integer-days form — NOT an interval)

Spark's `date_add(d, i)` with an interval raises an error; authors must use `d + i` or `date_add(d, days)`.

**Question.** Is the canonical `date_add(date, interval)` signature even expressible on Spark? The Spark integer-days form is semantically narrower. Two options:
1. Demote `date_add` to adapter-extended, canonical authors use `date + interval` (`BinaryOp::Add`) instead.
2. Keep canonical `date_add(date, interval)`; Spark adapter rewrites to `d + i` structurally (matching DataFusion's approach).

**Status.** Currently option 2: `date_add` is canonical with Spark `Structural` rewrite to `d + i`. Marked 🟡 on Spark row. `TD-FUNCS-MAPPING-DATE-ADD-SPARK` tracks.

---

## Q-FUNCS-MAP-009 — `date_diff` arity / unit arg

**Context.** Legacy §9 D6 records three divergent signatures:
- DataFusion: structural rewrite `d2 - d1` (no `date_diff` function)
- DuckDB: `date_diff('day', d1, d2)` (3-arg with unit)
- Spark: `datediff(end, start)` (2-arg, days only, note the name has no underscore)

**Question.** What IS the canonical signature? Legacy's 2-arg form `date_diff(d1, d2)` matches neither DuckDB's 3-arg form nor Spark's `datediff` name.

**Status.** Currently: `date_diff(d1, d2)` 2-arg canonical form, returning integer days.
- DataFusion: structural `d2 - d1` (cast to integer-days).
- DuckDB: structural `date_diff('day', d1, d2)`.
- Spark: name-remap to `datediff`.

All three rows marked 🟡. `TD-FUNCS-MAPPING-DATE-DIFF-ARITY`.

---

## Q-FUNCS-MAP-010 — `current_date` / `current_timestamp` parenless forms

**Context.** DuckDB accepts `current_date` and `current_timestamp` as identifiers (no parens) AND as functions. DataFusion / Spark require the paren form. Legacy emits `current_date()` / `current_timestamp()` uniformly.

**Question.** Any compatibility benefit to emitting the parenless form on DuckDB? Current: always use `current_date()` — universal across all three.

**Status.** Documented as "always paren form"; non-blocking. Parked as low-priority style question.

---

## Q-FUNCS-MAP-011 — `to_date` on DuckDB

**Context.** Legacy §9 D7 records DuckDB as not having a native `to_date()` function; the idiom is `CAST(str AS DATE)` or `str::DATE` or `strptime(str, format)` for formatted parsing.

**Question.** Does canonical `to_date(str)` (no format) rewrite structurally to `CAST(str AS DATE)` on DuckDB? The 2-arg form `to_date(str, format)` has no clean DuckDB mapping — must call `strptime`, which takes args in the opposite order.

**Status.** Currently:
- 1-arg `to_date(str)` — canonical, structural rewrite to `CAST(str AS DATE)` on DuckDB.
- 2-arg `to_date(str, format)` — demoted to adapter-extended (DataFusion / Spark only) since DuckDB's `strptime(format, str)` has reversed arg order.

`TD-FUNCS-MAPPING-TO-DATE-FORMAT` tracks the 2-arg form.

---

## Q-FUNCS-MAP-012 — BinaryOp per-engine promotion tables need empirical verification

**Context.** Per `14a §5.2` Q11, no canonical BinaryOp lattice. Per user's mapping-doc scope, §5 of this doc MUST publish per-engine reality tables.

**Question.** The tables currently drafted for `Integer + Long`, `Integer + Double`, `Decimal(p,s) + Decimal(p',s')` etc. are based on documentation, not on a test harness running against live DataFusion / Spark / DuckDB instances. Which rows survive empirical verification?

**Status.** All rows marked 🟡 pending a `tests/empirical/binop_promotion.rs` harness. `TD-FUNCS-MAPPING-BINOP-EMPIRICAL`.

---

## Q-FUNCS-MAP-013 — Non-closed aggregate intersection

**Context.** `14a §4.6` names `stddev`, `variance`, `median`, `string_agg`, `percentile_cont`, `percentile_disc`, `approx_count_distinct` as candidates pending intersection verification.

**Question.** Which of these survive Round-2 intersection? Legacy catalog carries no entries for this category — it predates `14a`'s non-closed-aggregate concept.

**Status.** Section §4 of the mapping doc lists these as `TD-FUNCS-MAPPING-AGG-INTERSECTION` with a plausibility column based on engine docs. Every row 🟡 pending verification.

---

## Q-FUNCS-MAP-014 — `greatest` / `least` null propagation

**Context.** `14a §4.5` candidate `greatest` / `least`. The engines differ in NULL handling:
- DataFusion: returns NULL if any arg is NULL (SQL standard)
- DuckDB: ignores NULL args (like `coalesce`-ish)
- Spark: returns NULL if any arg is NULL (SQL standard)

**Question.** Canonical semantics? If canonical follows "SQL standard — NULL propagates", then DuckDB needs a structural rewrite to emulate via `CASE WHEN any-arg IS NULL THEN NULL ELSE greatest(...) END`. If canonical follows DuckDB's ignore-NULL semantics, DF / Spark need a structural rewrite.

**Status.** Currently canonical = SQL-standard (NULL propagates); DuckDB row marked 🟡 pending verification, with `TD-FUNCS-MAPPING-GREATEST-LEAST-NULL` as potential adapter rewrite work.

---

## Q-FUNCS-MAP-015 — `ifnull` / `nvl` / `if` intersection

**Context.** `14a §4.5` lists `if` / `ifnull` / `nvl` as candidates.
- `if(cond, then, else)` — Spark has it as a native function; DataFusion accepts it as an alias for `CASE`; DuckDB: no `if()` function (use `CASE`).
- `ifnull(a, b)` ≡ `coalesce(a, b)` — all three accept it, but DuckDB / Spark recommend `coalesce`.
- `nvl(a, b)` ≡ `coalesce(a, b)` — Spark + DuckDB native; DataFusion: not present.

**Question.** Are any of these worth keeping canonical, or does `coalesce` + `case` fully replace them?

**Status.** Currently demoted: `if` / `ifnull` / `nvl` are all adapter-extended convenience aliases; canonical authors should use `coalesce(x, y)` (for `ifnull` / `nvl`) or `Case { when: [...], else: ... }` (for `if`). `TD-FUNCS-MAPPING-IF-IFNULL-NVL`.

---

## Q-FUNCS-MAP-016 — BinaryOp integer-division result type

**Context.** `Integer / Integer` produces:
- DataFusion: `Integer` (truncating division in Arrow) 🟡
- DuckDB: `Double` (automatic promotion to float) 🟡
- Spark: `Double` (automatic promotion) 🟡

**Question.** Is this genuinely the per-engine behavior across v1.x / 3.5.x? A Semantics declaring `data_type: Integer` over `a / b` where `a, b: Integer` will produce different reconciliation Casts per engine.

**Status.** Documented in §9 of the mapping doc (cast semantics edge cases). All three rows 🟡. `TD-FUNCS-MAPPING-INT-DIV-RESULT`.

---

## Q-FUNCS-MAP-017 — `SafeDivide` rendering

**Context.** `14 §5.6` defines `BinaryOpKind::SafeDivide` as "Divide that returns NULL on zero divisor". Per-engine native support:
- DataFusion: no native `safe_divide`; rewrite as `CASE WHEN b = 0 THEN NULL ELSE a / b END` or `a / NULLIF(b, 0)`.
- DuckDB: no native `safe_divide`; same rewrite as DF.
- Spark: has `try_divide(a, b)` since 3.3; older versions need the CASE / NULLIF emulation.

**Question.** Which rewrite form is the adapter's default emission? `a / NULLIF(b, 0)` is shorter and uses standard SQL; `try_divide` is Spark-specific but cleaner.

**Status.** Currently: universal `a / NULLIF(b, 0)` emission across adapters (simplest, portable). Spark 3.3+ adapter MAY opt into `try_divide`. Marked 🟡. `TD-FUNCS-MAPPING-SAFEDIVIDE-SPARK`.

---

## Q-FUNCS-MAP-018 — Adapter-extended function inventory

**Context.** User's task §6 requests DataFusion / DuckDB / Spark adapter-extended function inventories. Legacy doc doesn't enumerate these systematically — only calls out a handful (`array_element`, `list_extract`, `collect_set`, `array_join`).

**Question.** What's the full inventory of each engine's registered adapter-extended functions? This list should be maintained per-adapter-crate, not in the canonical registry doc.

**Status.** §6 of mapping doc carries **seed** lists from legacy + engine reputation; marked as non-exhaustive and 🟡. The authoritative lists will live in each adapter crate's `README.md` and be mirrored here. `TD-FUNCS-MAPPING-ADAPTER-INVENTORY`.

---

## Q-FUNCS-MAP-019 — Adapter crate identity / version scheme

**Context.** `types_mapping.md §1` precedent is to cite engine versions (e.g. "DuckDB 1.1.x") rather than adapter-crate versions. `functions_mapping.md` should follow the same convention, but each row's "verified against" line is not yet standardized.

**Question.** Cite engine version only, or also adapter crate version?

**Status.** Following `types_mapping.md` precedent — engine version only. Adapter-crate identity encoded implicitly via the per-engine column. Non-blocking.

---

## Q-FUNCS-MAP-020 — DataFusion version pin

**Context.** DataFusion releases frequently (major version bumps every ~6-8 weeks). Naming functions to a specific version pins the mapping to that engine version. Example: DataFusion 40.x added `contains`; DataFusion 35.x did not.

**Question.** What's the floor version for semstrait's DataFusion adapter? Legacy doc implies a recent version (45.x in user's task framing); no version is actually pinned in legacy text.

**Status.** Tentatively "DataFusion 40.x+"; all rows citing DF 45.x are 🟡. Final pin awaits `apis/36_semstrait_adapter.md` ratification of the adapter's MSRV-equivalent engine floor.
