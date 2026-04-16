---
doc: design/registry/functions_mapping
status: Living
purpose: Authoritative per-engine mapping of canonical functions and dedicated-variant expression nodes
authoritative-for:
  - canonical function catalog ↔ engine-native function names across DataFusion / Spark / DuckDB
  - rewrite-tier classification per (canonical, engine) pair (Name-only / Name-remap / Structural / Unsupported)
  - PlanBuilder-layer vs Dialect-layer rewrite assignment
  - adapter-extended function inventory (non-canonical, per-engine)
  - per-engine BinaryOp result-type reality (filling the gap `14a §5.2` leaves)
  - per-engine reconciliation-Cast edge cases for function results
  - TECH_DEBT entries for function-mapping shortfalls
depends-on:
  - foundations/14_expressions.md (shared `Expr` AST — `FunctionCall`, `BinaryOp`, dedicated variants)
  - foundations/14a_function_catalog.md (canonical catalog, `FunctionSpec`, `FnSignature`, rewrite tier vocabulary)
  - registry/README.md (registry policy, engine coverage, versioning posture)
  - registry/types_mapping.md (exemplar format; canonical DataType mappings consumed here)
  - apis/36_semstrait_adapter.md (adapter trait — PlanBuilder / Dialect layering)
---

# Functions Mapping Catalog

> **Scope.** Authoritative per-engine rendering of every canonical `FunctionSpec` ratified in `foundations/14a_function_catalog.md` §4, plus every dedicated-variant `Expr` node from `foundations/14_expressions.md` §3.2 that reaches the adapter. Per `14a §6.2`, `14a` owns canonical shape (engine-agnostic); this doc owns per-engine reality. It does NOT define new canonical entries — those live in `14a`. Adapter-extended entries (per `14a §7`) are catalogued here for cross-reference but registered in each adapter crate.

> **Status (2026-04-20):** Round-2 scaffold drafted against `14a` Q10 intersection-only population policy. Every row tracks (canonical name, signature, per-engine native form, rewrite tier, verification marker). Entries marked 🟡 are plausible from legacy `docs/FUNCTION_CATALOG.md` + engine-docs but have not been empirically verified against a live adapter test harness. Unresolved questions parked in `open_questions/functions_mapping_open_questions.md`.

---

## 1. Rewrite Tier Taxonomy

Every canonical function is classified, per engine, into one of four rewrite tiers. The tier governs where and how the adapter transforms the canonical `Expr` tree on its way to engine-native output.

| Tier | Description | Example |
|---|---|---|
| **Name-only** | Engine uses the same function name, same signature, same semantics. No rewrite — adapter emits the canonical name verbatim. | `upper` → `upper` (all three engines). |
| **Name-remap** | Engine uses a different function name, but same signature and same semantics. Adapter performs a string-level name substitution, no tree reshaping. | `position` → `strpos` (DataFusion, DuckDB). |
| **Structural** | Engine requires an expression-tree transformation — different arity, reordered args, substitution for a `BinaryOp`, wrap with another call, etc. | `date_add(d, i)` → `d + i` (DataFusion interval arithmetic). |
| **Unsupported** | Engine has no equivalent and no emulation path the adapter is willing to carry. Hard error at `adapt` time per `14a §6.3` via `AdaptError::UnsupportedFunction { name, engine, location }`. | (none in canonical set v1 — intersection-only population eliminates canonical-level unsupported by construction; appears only for adapter-extended entries consumed cross-engine.) |

The canonical catalog's Q10 intersection posture (per `14a §4.1`) guarantees that every canonical entry resolves to one of {Name-only, Name-remap, Structural} on each of the three first-class engines — `Unsupported` never arises at the canonical layer for DataFusion / Spark / DuckDB. Adapter-extended entries (§7) may legitimately resolve to `Unsupported` on engines that do not carry the adapter's registration.

## 2. The Two Rewrite Layers

Canonical `Expr` trees are transformed into engine-native output at two distinct adapter layers. Per `14a §6` / `apis/36_semstrait_adapter.md`, each function is handled at **exactly one** layer — the choice is driven by the node's IR shape.

| Layer | Runs | Input → Output | Handles |
|---|---|---|---|
| **PlanBuilder layer** (`EngineAdapter::rewrite_expr`) | During plan construction, before SQL emission | `Expr` tree → `Expr` tree | `FunctionCall` nodes (looked up via the per-adapter `FunctionRewriter` rewrite table), plus any dedicated-variant rewrites that convert to `FunctionCall` (e.g. DataFusion `RegexpExtract` → `array_element(regexp_match(...), idx+1)`). |
| **Dialect layer** (`SqlDialect` trait) | During SQL emission | `Expr` tree → SQL text | Dedicated `Expr` variants with dialect methods (`Like` / `ILike` / `RegexpMatch` / `RegexpExtract` / `DateTrunc` / `Cast`) rendered directly to engine-native SQL without first being reshaped into a `FunctionCall`. |

**The layering rule.** PlanBuilder-layer rewrites produce `Expr` — they never know the dialect. Dialect-layer rewrites produce SQL — they never reshape the `Expr` tree (only render). A PlanBuilder rewrite MAY convert a dedicated variant into a `FunctionCall`; after that conversion the node is indistinguishable from any other `FunctionCall` and is rendered by the generic scalar-call path. This happens for DataFusion's `RegexpExtract` only; all other dedicated variants render at the Dialect layer on all three engines.

**Why two layers.** PlanBuilder rewrites are engine-specific but dialect-independent — they exist because DataFusion's `regexp_match` returns `List<Utf8>` (a structural quirk) rather than because of SQL syntax. Dialect rewrites are pure syntax — `Like` emits `expr LIKE pattern` the same way in DF / DuckDB / ANSI, and only Spark's `ILike` needs a different SQL form (`LOWER(expr) LIKE LOWER(pattern)`). Keeping the layers separate lets the planner run its own optimizer passes over the post-PlanBuilder `Expr` tree before the dialect rendering commits to SQL text.

---

## 3. Aggregates

### 3.1 The closed five (`Aggregation` enum from `14 §3.2`)

`Sum` / `Avg` / `Count` / `Min` / `Max` are expressed in the AST via `Expr::Aggregate { aggregation, expr, distinct }` with a closed `Aggregation` enum — NOT via `FunctionCall`. Per `14a §4.7`, they are not registry entries; adapters render them via the Dialect layer's aggregate rendering path. Return types derive from the SQL:2016-style promotion table in `14 §5.4`.

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `sum` | `sum(expr)` | `sum(expr)` | Name-only | `sum(expr)` | Name-only | `sum(expr)` | Name-only | Universal. Closed variant. Return type per `14 §5.4`. |
| `avg` | `avg(expr)` | `avg(expr)` | Name-only | `avg(expr)` | Name-only | `avg(expr)` | Name-only | Universal. Closed variant. Return always `Double` for integer / `Double` input; widened `Decimal` for `Decimal` input. |
| `count` | `count(expr)` / `count(*)` | `count(expr)` / `count(*)` | Name-only | `count(expr)` / `count(*)` | Name-only | `count(expr)` / `count(*)` | Name-only | Universal. Returns `Long`. `count(*)` is represented in the AST as `Aggregate { aggregation: Count, expr: Literal(Integer(1)), distinct: false }` per `14 §3.2` notes, rendered as `count(*)` by the adapter's aggregate-render path. |
| `count_distinct` | `count(DISTINCT expr)` | `count(DISTINCT expr)` | Name-only | `count(DISTINCT expr)` | Name-only | `count(DISTINCT expr)` | Name-only | Expressed as `Aggregate { aggregation: Count, distinct: true }` per `14 §3.2`. Universal. |
| `min` | `min(expr)` | `min(expr)` | Name-only | `min(expr)` | Name-only | `min(expr)` | Name-only | Universal. Return type = operand type. |
| `max` | `max(expr)` | `max(expr)` | Name-only | `max(expr)` | Name-only | `max(expr)` | Name-only | Universal. Return type = operand type. |

Portability summary: **Universal** across all five aggregates.

`SUM DISTINCT` / `AVG DISTINCT` render per `14 §5.4` — `Aggregate { aggregation: Sum, distinct: true }` renders as `sum(DISTINCT expr)`. All three engines support these forms natively; portability **Universal**.

### 3.2 Non-closed aggregates (`14a §4.6` candidate list, pending Round-2 intersection verification)

*Every row 🟡 pending Round-2 empirical intersection scan per `14a §10.2`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `stddev` | `stddev(expr)` | `stddev(expr)` / `stddev_samp(expr)` 🟡 | Name-only | `stddev(expr)` / `stddev_samp(expr)` 🟡 | Name-only | `stddev(expr)` / `stddev_samp(expr)` 🟡 | Name-only | Canonical = sample stddev. Population-stddev variant `stddev_pop` is a separate canonical entry (same 🟡 intersection across all three). |
| `variance` | `variance(expr)` | `var_samp(expr)` 🟡 | Name-remap | `variance(expr)` / `var_samp(expr)` 🟡 | Name-only | `variance(expr)` / `var_samp(expr)` 🟡 | Name-only | DataFusion lacks `variance` alias historically; `var_samp` preferred. `TD-FUNCS-MAPPING-AGG-INTERSECTION`. |
| `median` | `median(expr)` | `median(expr)` 🟡 | Name-only | `median(expr)` | Name-only | `median(expr)` (Spark 3.4+) 🟡 | Name-only | Spark < 3.4 requires `percentile_approx(expr, 0.5)`; Spark 3.4+ adds native `median`. Floor-version constraint per Q-FUNCS-MAP-020. |
| `string_agg` | `string_agg(expr, sep)` | `string_agg(expr, sep)` 🟡 | Name-only | `string_agg(expr, sep)` | Name-only | `array_join(collect_list(expr), sep)` 🟡 | Structural | Spark has no `string_agg`; structural rewrite composes `collect_list` + `array_join`. **Demotion candidate** to adapter-extended if the structural rewrite is judged too coarse. |
| `percentile_cont` | `percentile_cont(expr, fraction)` | `approx_percentile_cont(expr, fraction)` 🟡 | Name-remap | `percentile_cont(fraction) WITHIN GROUP (ORDER BY expr)` 🟡 | Structural | `percentile_approx(expr, fraction)` 🟡 | Name-remap | **Likely demoted to adapter-extended.** Name / signature divergence too wide; DuckDB's `WITHIN GROUP` syntax is structural; engine semantics differ between exact (`percentile_cont`) and approximate (`approx_percentile`). `TD-FUNCS-MAPPING-PERCENTILE`. |
| `percentile_disc` | `percentile_disc(expr, fraction)` | 🟡 absent | Unsupported | `percentile_disc(fraction) WITHIN GROUP (ORDER BY expr)` 🟡 | Structural | 🟡 absent | Unsupported | **Likely demoted to adapter-extended.** Not intersection. |
| `approx_count_distinct` | `approx_count_distinct(expr)` | `approx_distinct(expr)` 🟡 | Name-remap | `approx_count_distinct(expr)` | Name-only | `approx_count_distinct(expr)` | Name-only | Intersection viable pending DF name-remap. Implementation semantics differ (HyperLogLog variants); acceptable under `14a §6.2`'s engine-delegation posture. |

Portability summary: **Partial** or **likely-demoted** for every non-closed aggregate. Final canonical membership depends on Round-2 intersection verification per `14a §10.2`.

---

## 4. Comparison Operators

All comparison operators are `BinaryOp` variants per `14 §3.2` (`BinaryOpKind::{Eq, NotEq, Lt, LtEq, Gt, GtEq}`) — rendered by the Dialect layer's `BinaryOp` emission path. Return type always `Boolean`.

| Canonical | `BinaryOpKind` | SQL | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|---|
| `equal` | `Eq` | `a = b` | `a = b` | Name-only | `a = b` | Name-only | `a = b` | Name-only | Universal. |
| `not_equal` | `NotEq` | `a <> b` / `a != b` | `a <> b` | Name-only | `a <> b` / `a != b` | Name-only | `a <> b` / `a != b` | Name-only | All three accept both spellings; canonical emission uses `<>` (ANSI). |
| `lt` | `Lt` | `a < b` | `a < b` | Name-only | `a < b` | Name-only | `a < b` | Name-only | Universal. |
| `lte` | `LtEq` | `a <= b` | `a <= b` | Name-only | `a <= b` | Name-only | `a <= b` | Name-only | Universal. |
| `gt` | `Gt` | `a > b` | `a > b` | Name-only | `a > b` | Name-only | `a > b` | Name-only | Universal. |
| `gte` | `GtEq` | `a >= b` | `a >= b` | Name-only | `a >= b` | Name-only | `a >= b` | Name-only | Universal. |

Portability summary: **Universal** across all six.

Per `14 §5.6`, semstrait does NOT validate operand-type compatibility for comparisons — each engine raises its own diagnostics at execution time if e.g. `Integer < String` is attempted. Cross-engine comparability rules (`Integer` vs `Double`, `String` vs `Date`) are engine-native; see §5 below for arithmetic promotion reference tables.

---

## 5. Arithmetic Operators — per-engine reality (fills the `14a §5.2` gap)

Arithmetic operators are `BinaryOp` variants (`BinaryOpKind::{Add, Subtract, Multiply, Divide, SafeDivide, Mod}`). Per `14a §5.2` / Q11, there is no canonical promotion lattice — semstrait delegates result-type derivation to the engine. This section documents the **per-engine observed behavior** so authors writing `data_type:` declarations at a Semantics boundary (and adapter implementers writing reconciliation Casts per `14 §6.4` rule 2) can predict each engine's natural result type.

### 5.1 Canonical → engine rendering

All arithmetic `BinaryOp`s render as native infix SQL operators across all three engines:

| Canonical | `BinaryOpKind` | SQL | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|---|
| `add` | `Add` | `a + b` | `a + b` | Name-only | `a + b` | Name-only | `a + b` | Name-only | Universal. |
| `subtract` | `Subtract` | `a - b` | `a - b` | Name-only | `a - b` | Name-only | `a - b` | Name-only | Universal. |
| `multiply` | `Multiply` | `a * b` | `a * b` | Name-only | `a * b` | Name-only | `a * b` | Name-only | Universal. |
| `divide` | `Divide` | `a / b` | `a / b` | Name-only | `a / b` | Name-only | `a / b` | Name-only | Universal in syntax; result-type differs per engine — see §5.2. |
| `safe_divide` | `SafeDivide` | `a / NULLIF(b, 0)` | `a / NULLIF(b, 0)` 🟡 | Structural | `a / NULLIF(b, 0)` 🟡 | Structural | `a / NULLIF(b, 0)` / `try_divide(a, b)` (Spark 3.3+) 🟡 | Structural | Canonical posture: emit `NULLIF` wrap universally. Spark 3.3+ adapter MAY switch to `try_divide`. See Q-FUNCS-MAP-017. |
| `mod` | `Mod` | `a % b` | `a % b` 🟡 | Name-only | `a % b` / `mod(a, b)` | Name-only | `a % b` / `mod(a, b)` / `pmod(a, b)` | Name-only | Canonical form is the `%` operator per `14a §4.3` / `14 §3.2`. `mod(a, b)` as a function call is adapter-extended. |

⚠️ `Divide` result-type divergence across engines — see §5.2.

### 5.2 Per-engine result types — integer arithmetic

⚠️ These tables record observed per-engine behavior; all rows 🟡 pending `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` (Q-FUNCS-MAP-012).

**Mixed-integer promotion** (`Add` / `Subtract` / `Multiply`):

| Left | Right | DataFusion | DuckDB | Spark | Notes |
|---|---|---|---|---|---|
| `Integer` | `Integer` | `Integer` | `Integer` | `Integer` | Convergent. Overflow is engine-per-engine; none raises a compile-time error. |
| `Integer` | `Long` | `Long` | `Long` | `Long` | Convergent widen-to-larger. |
| `Integer` | `Double` | `Double` | `Double` | `Double` | Convergent widen-to-float. |
| `Long` | `Long` | `Long` | `Long` | `Long` | Convergent. |
| `Long` | `Double` | `Double` | `Double` | `Double` | Convergent. |
| `Float` | `Double` | `Double` 🟡 | `Double` 🟡 | `Double` 🟡 | Convergent widen to double-precision. |

**Integer division** (`Divide`):

| Left | Right | DataFusion | DuckDB | Spark | Notes |
|---|---|---|---|---|---|
| `Integer` | `Integer` | `Integer` (truncating) 🟡 | `Double` (promotes) 🟡 | `Double` (promotes) 🟡 | ⚠️ **Divergent.** Authors declaring `data_type: Integer` on a quotient on DuckDB / Spark receive an implicit narrowing (`EXPR_W_CAST_NARROW`). See §16.1 and Q-FUNCS-MAP-016. |
| `Long` | `Long` | `Long` (truncating) 🟡 | `Double` 🟡 | `Double` 🟡 | ⚠️ **Divergent.** Same pattern. |
| `Integer` | `Double` | `Double` | `Double` | `Double` | Convergent. |
| `Double` | `Double` | `Double` | `Double` | `Double` | Convergent. |

### 5.3 Per-engine result types — `Decimal(p, s)` arithmetic

Decimal result-type rules are genuinely per-engine and are the widest divergence-surface in the three-engine trio. Rows documented from engine reference docs (DuckDB 1.1.x, Spark 3.5.x ANSI mode, DataFusion 40.x+) and are 🟡 until the `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` test harness lands.

**Addition / Subtraction** (`Decimal(p1, s1) + Decimal(p2, s2)`):

| Engine | Result precision | Result scale | Notes |
|---|---|---|---|
| DataFusion | `max(p1 - s1, p2 - s2) + max(s1, s2) + 1` | `max(s1, s2)` | 🟡 follows Arrow's decimal-arithmetic kernel rules; capped at precision 38 (Decimal128). |
| DuckDB | `max(p1 - s1, p2 - s2) + max(s1, s2) + 1` | `max(s1, s2)` | 🟡 follows SQL standard width-1-widening rule. |
| Spark | `max(p1 - s1, p2 - s2) + max(s1, s2) + 1` | `max(s1, s2)` | 🟡 documented in Spark `DecimalPrecision` analyzer. ANSI-mode overflow raises; non-ANSI wraps. |

Convergent at the precision/scale formula, divergent on overflow handling.

**Multiplication** (`Decimal(p1, s1) * Decimal(p2, s2)`):

| Engine | Result precision | Result scale | Notes |
|---|---|---|---|
| DataFusion | `p1 + p2 + 1` (capped 38) | `s1 + s2` | 🟡 Arrow kernel. |
| DuckDB | `p1 + p2 + 1` (capped 38) | `s1 + s2` | 🟡 SQL-standard form. |
| Spark | `p1 + p2 + 1` (capped 38) | `s1 + s2` | 🟡 Spark `DecimalPrecision`. |

Convergent.

**Division** (`Decimal(p1, s1) / Decimal(p2, s2)`):

| Engine | Result precision | Result scale | Notes |
|---|---|---|---|
| DataFusion | `p1 - s1 + s2 + max(6, s1 + p2 + 1)` (capped 38) 🟡 | `max(6, s1 + p2 + 1)` 🟡 | Follows Spark's formula per DataFusion analyzer docs. |
| DuckDB | `p1 + s2` (capped 38) 🟡 | `s1` 🟡 | ⚠️ **Divergent** — DuckDB does not inflate scale the way Spark does. |
| Spark | `p1 - s1 + s2 + max(6, s1 + p2 + 1)` (capped 38) 🟡 | `max(6, s1 + p2 + 1)` 🟡 | Canonical reference for DF's formula. |

⚠️ DuckDB vs Spark/DF Decimal-division divergence — a Semantics declaring `data_type: Decimal(p, s)` over a decimal division will need different reconciliation Casts depending on the target adapter. Tracked as `TD-FUNCS-MAPPING-DECIMAL-DIV`.

### 5.4 Interval arithmetic

| Expression | DataFusion | DuckDB | Spark | Notes |
|---|---|---|---|---|
| `Date + Interval` | native | native | native (requires SQL-typed interval literal) 🟡 | Per `registry/types_mapping.md §3.2`. Spark `CalendarInterval` rendering caveats apply. |
| `Timestamp + Interval` | native | native | native 🟡 | Same caveats. |
| `Interval + Interval` | native (within same subclass) 🟡 | native | native 🟡 | DF splits intervals into three Arrow subclasses — cross-subclass addition may require promotion to `IntervalMonthDayNano`. |
| `Date - Date` | `Interval` 🟡 | `Integer` (days) 🟡 | `Integer` (days) 🟡 | ⚠️ **Divergent return type.** See `TD-FUNCS-MAPPING-DATE-SUB-DATE`. |

Authors SHOULD declare `data_type:` explicitly on Semantics consuming interval arithmetic results.

---

## 6. Logical Operators

`BinaryOp` variants `And` / `Or` plus the unary / predicate variants `Not`, `IsNull`, `IsNotNull` from `14 §3.2`. Return type always `Boolean`.

| Canonical | IR Repr | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `and` | `BinaryOp::And` | `a AND b` | Name-only | `a AND b` | Name-only | `a AND b` | Name-only | Universal. |
| `or` | `BinaryOp::Or` | `a OR b` | Name-only | `a OR b` | Name-only | `a OR b` | Name-only | Universal. |
| `not` | `Expr::Not` | `NOT expr` | Name-only | `NOT expr` | Name-only | `NOT expr` | Name-only | Universal. |
| `is_null` | `Expr::IsNull` | `expr IS NULL` | Name-only | `expr IS NULL` | Name-only | `expr IS NULL` | Name-only | Universal. Rendered by Dialect-layer dedicated path. |
| `is_not_null` | `Expr::IsNotNull` | `expr IS NOT NULL` | Name-only | `expr IS NOT NULL` | Name-only | `expr IS NOT NULL` | Name-only | Universal. Rendered by Dialect-layer dedicated path. |

Portability summary: **Universal** across all five.

---

## 7. String Functions (`14a §4.2`)

*Round-2 intersection candidates from `14a §4.2` plus legacy `FUNCTION_CATALOG.md §7`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `upper` | `upper(str) -> String` | `upper(str)` | Name-only | `upper(str)` | Name-only | `upper(str)` | Name-only | Universal. |
| `lower` | `lower(str) -> String` | `lower(str)` | Name-only | `lower(str)` | Name-only | `lower(str)` | Name-only | Universal. |
| `length` | `length(str) -> Long` | `length(str)` / `char_length(str)` | Name-only | `length(str)` / `len(str)` | Name-only | `length(str)` / `char_length(str)` | Name-only | Character count (not bytes). `char_length` / `len` are adapter-side aliases the native parser accepts; canonical name is `length`. |
| `substring` | `substring(str, start, [length])` | `substr(str, start, [length])` / `substring(str FROM start [FOR length])` | Name-only | `substring(str, start, length)` / `substr(str, start, length)` | Name-only | `substring(str, pos, [length])` / `substr(str, pos, [length])` | Name-only | 1-indexed on all three. Spark allows negative `pos` (count from end); DuckDB / DF do not. Signature registered as two overloads (2-arg, 3-arg) per `14a §3.5`. |
| `trim` | `trim(str) -> String` | `trim(str)` | Name-only | `trim(str)` | Name-only | `trim(str)` | Name-only | Removes leading + trailing whitespace. Engine-native parsers accept extended forms (`TRIM(BOTH 'x' FROM str)`); canonical is the simple 1-arg form. |
| `ltrim` | `ltrim(str) -> String` | `ltrim(str)` | Name-only | `ltrim(str)` | Name-only | `ltrim(str)` | Name-only | Removes leading whitespace. |
| `rtrim` | `rtrim(str) -> String` | `rtrim(str)` | Name-only | `rtrim(str)` | Name-only | `rtrim(str)` | Name-only | Removes trailing whitespace. |
| `concat` | `concat(str, ...) -> String` (variadic) | `concat(...)` | Name-only | `concat(...)` | Name-only | `concat(...)` | Name-only | Variadic (N ≥ 1). NULL-argument handling differs subtly across engines; canonical posture: engine-delegated per `14 §5.6` / `14a §3.1`. |
| `replace` | `replace(str, from, to) -> String` | `replace(str, from, to)` | Name-only | `replace(str, from, to)` | Name-only | `replace(str, from, to)` | Name-only | Replaces ALL occurrences (not just first). Universal. |
| `lpad` | `lpad(str, len, [pad]) -> String` | `lpad(str, len, [pad])` | Name-only | `lpad(str, len, [pad])` | Name-only | `lpad(str, len, [pad])` | Name-only | Default `pad` = single space. 2-arg and 3-arg overloads. |
| `rpad` | `rpad(str, len, [pad]) -> String` | `rpad(str, len, [pad])` | Name-only | `rpad(str, len, [pad])` | Name-only | `rpad(str, len, [pad])` | Name-only | Same as `lpad`. |
| `reverse` | `reverse(str) -> String` | `reverse(str)` | Name-only | `reverse(str)` | Name-only | `reverse(str)` | Name-only | Universal. See Q-FUNCS-MAP-005 for `repeat` promotion candidacy (legacy also records `repeat` as universal name-only — likely a canonical Round-2 addition). |
| `split_part` | `split_part(str, delim, part_num) -> String` 🟡 | `split_part(str, delim, part_num)` | Name-only | `split_part(str, delim, part_num)` | Name-only | `split_part(str, delim, part_num)` (Spark 3.4+) 🟡 | Name-only | 1-indexed; returns empty string on out-of-range. Spark-version floor constraint per Q-FUNCS-MAP-020. |
| `position` | `position(substr, str) -> Long` 🟡 | `strpos(str, substr)` | Name-remap | `strpos(str, substr)` / `position(substr IN str)` | Name-remap | `locate(substr, str, [pos])` | Structural | ⚠️ Argument order and function name both diverge on Spark. Spark `locate` takes `(substr, str)` with optional start position; DF / DuckDB `strpos` takes `(str, substr)`. Canonical = `position(substr, str)` (matches SQL `POSITION(substr IN str)` natural reading). See Q-FUNCS-MAP-001. |

Portability summary: mostly **Universal** or **Partial** (name-remap). `position` is **Partial** with structural rewrite on Spark.

### 7.1 String functions demoted from legacy catalog

| Legacy ID | Name | Rationale | Disposition |
|---|---|---|---|
| S4 | `concat_ws` | Legacy canonical, universal name-only. Not in `14a §4.2` candidate list. | **Canonical promotion candidate** pending Round-2 `14a` update. 🟡 as adapter-extended in §8 until `14a` updates. `TD-FUNCS-MAPPING-CONCAT-WS-PROMOTE`. |
| S11 | `left` | Universal name-only on DF / DuckDB; Spark ≥ 3.2. Not in `14a §4.2`. | Adapter-extended (§8) pending `14a` update. Q-FUNCS-MAP-004. |
| S12 | `right` | Same story as `left`. | Adapter-extended (§8). Q-FUNCS-MAP-004. |
| S16 | `starts_with` | DF / DuckDB name-only; Spark name-remap `startswith` (no underscore). | **Canonical promotion candidate**; currently adapter-extended until `14a` updates. `TD-FUNCS-MAPPING-STARTS-WITH-PROMOTE`. |
| S17 | `ends_with` | Same as `starts_with`. | Same disposition. |
| S18 | `initcap` | DuckDB has no equivalent (legacy: "N/A"). Fails Q10 intersection. | **Demoted to adapter-extended** (DataFusion + Spark only). `TD-FUNCS-MAPPING-INITCAP`. Q-FUNCS-MAP-002. |
| S20 | `repeat` | Universal name-only across all three per legacy. Not in `14a §4.2`. | **Canonical promotion candidate**. `TD-FUNCS-MAPPING-REPEAT`. Q-FUNCS-MAP-005. |
| — | `contains` | Universal name-only across all three (legacy §12). Not in `14a §4.2`. | **Canonical promotion candidate**. `TD-FUNCS-MAPPING-CONTAINS`. |
| — | `translate` | Universal name-only (legacy §12). Not in `14a §4.2`. | **Canonical promotion candidate**. `TD-FUNCS-MAPPING-TRANSLATE`. |
| — | `md5` / `sha256` | Universal name-only (legacy §12). Cryptographic — semstrait does not canonicalize these v1. | Adapter-extended only. |

---

## 8. Math Functions (`14a §4.3`)

*Round-2 intersection candidates from `14a §4.3` plus legacy `FUNCTION_CATALOG.md §6`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `abs` | `abs(x) -> same` | `abs(x)` | Name-only | `abs(x)` | Name-only | `abs(x)` | Name-only | Universal. Return type = operand type (`SameAs(0)`). |
| `round` | `round(x, [digits]) -> same` | `round(x, [digits])` | Name-only | `round(x, [digits])` | Name-only | `round(x, [d])` | Name-only | Default `digits = 0`. 1-arg and 2-arg overloads. |
| `ceil` | `ceil(x) -> same` | `ceil(x)` / `ceiling(x)` | Name-only | `ceil(x)` / `ceiling(x)` | Name-only | `ceil(x)` / `ceiling(x)` | Name-only | `ceiling` is an adapter-side alias all three parsers accept. Canonical name = `ceil`. |
| `floor` | `floor(x) -> same` | `floor(x)` | Name-only | `floor(x)` | Name-only | `floor(x)` | Name-only | Universal. |
| `sqrt` | `sqrt(x) -> Double` | `sqrt(x)` | Name-only | `sqrt(x)` | Name-only | `sqrt(x)` | Name-only | Universal. Return `Double` regardless of operand. |
| `power` | `power(base, exp) -> Double` | `power(base, exp)` / `pow(base, exp)` | Name-only | `power(base, exp)` / `pow(base, exp)` | Name-only | `power(base, exp)` / `pow(base, exp)` | Name-only | `pow` is adapter-side alias. Canonical = `power`. |
| `exp` | `exp(x) -> Double` | `exp(x)` | Name-only | `exp(x)` | Name-only | `exp(x)` | Name-only | Universal. |
| `ln` | `ln(x) -> Double` | `ln(x)` | Name-only | `ln(x)` | Name-only | `ln(x)` | Name-only | Natural logarithm. Universal. |
| `log` | `log(base, x) -> Double` 🟡 | `log(base, x)` | Name-only | `log(x)` (1-arg = base-10) / — 🟡 | Partial | `log(base, x)` | Name-only | ⚠️ **Semantic divergence.** Spark / DF: `log(base, x)` (2-arg). DuckDB: `log(x)` (1-arg, base-10). See `TD-FUNCS-MAPPING-LOG-ARITY`. Canonical = 2-arg per majority rule. |
| `log10` | `log10(x) -> Double` | `log10(x)` | Name-only | `log10(x)` / `log(x)` | Name-only | `log10(x)` | Name-only | Base-10 logarithm. Universal. |
| `sign` | `sign(x) -> same` 🟡 | `signum(x)` | Name-remap | `sign(x)` | Name-only | `signum(x)` / `sign(x)` | Name-only | DataFusion only exposes `signum`; canonical = `sign` (majority); DF adapter name-remaps. |

Portability summary: mostly **Universal** or **Partial** (minor name-remap / alias). `log` is **Partial** with a semantic divergence (arity) — see Q-FUNCS-MAP about arity unification at Round-2.

### 8.1 Math functions explicitly excluded from canonical

| Name | Reason |
|---|---|
| `mod(a, b)` | Canonical form is `BinaryOpKind::Mod` (`%`) per `14 §3.2` / `14a §4.3`. The function-call form is an adapter-extended convenience on engines that prefer it (DuckDB, Spark). Legacy entry R11 superseded. |
| `trunc` / `truncate` | Legacy §12 notes universal name-only. Not in `14a §4.3` candidate list. **Canonical promotion candidate** — `TD-FUNCS-MAPPING-TRUNC-PROMOTE`. |

---

## 9. Temporal Functions (`14a §4.4`)

*Round-2 intersection candidates from `14a §4.4` plus legacy `FUNCTION_CATALOG.md §9`. `DateTrunc` is a dedicated `Expr` variant (not a registry entry) — documented in §11.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `date_part` | `date_part(part, expr) -> Long` | `date_part('year', expr)` / `extract(YEAR FROM expr)` | Name-only | `date_part('year', expr)` / `extract(YEAR FROM expr)` / `year(expr)` | Name-only | `date_part('YEAR', expr)` / `extract(YEAR FROM expr)` / `year(expr)` | Name-only | `extract` / `year(expr)` / etc. are adapter-side syntactic sugars all three parsers accept. Canonical name = `date_part`. Q-FUNCS-MAP-007. |
| `year` | `year(expr) -> Long` 🟡 | `year(expr)` / `date_part('year', expr)` | Name-only | `year(expr)` | Name-only | `year(expr)` | Name-only | Convenience alias for `date_part('year', expr)`. Candidate in `14a §4.4`. All three engines accept as function call. |
| `month` | `month(expr) -> Long` 🟡 | `month(expr)` | Name-only | `month(expr)` | Name-only | `month(expr)` | Name-only | Same convenience pattern as `year`. |
| `day` | `day(expr) -> Long` 🟡 | `day(expr)` | Name-only | `day(expr)` | Name-only | `day(expr)` / `dayofmonth(expr)` | Name-only | Universal. Spark also accepts `dayofmonth`. |
| `hour` | `hour(expr) -> Long` 🟡 | `hour(expr)` | Name-only | `hour(expr)` | Name-only | `hour(expr)` | Name-only | Universal. |
| `minute` | `minute(expr) -> Long` 🟡 | `minute(expr)` | Name-only | `minute(expr)` | Name-only | `minute(expr)` | Name-only | Universal. |
| `second` | `second(expr) -> Long` 🟡 | `second(expr)` | Name-only | `second(expr)` | Name-only | `second(expr)` | Name-only | Universal. Returns integer seconds — fractional-second extraction goes through `date_part('millisecond', ...)` or similar. |
| `current_date` | `current_date() -> Date` | `current_date()` | Name-only | `current_date` / `current_date()` / `today()` | Name-only | `current_date()` / `curdate()` | Name-only | Canonical emission always uses paren form. Q-FUNCS-MAP-010. |
| `current_timestamp` | `current_timestamp() -> Timestamp` | `current_timestamp()` / `now()` | Name-only | `current_timestamp` / `current_timestamp()` / `now()` | Name-only | `current_timestamp()` / `now()` | Name-only | Canonical emission always uses paren form. Determinism per query (sourced from `SessionContext` per `apis/34 §*`). |
| `date_add` | `date_add(date, interval) -> Date` | `date + interval` 🟡 | Structural | `date_add(date, interval)` / `dateadd(date, interval)` | Name-only | `date + interval` 🟡 | Structural | ⚠️ DataFusion + Spark have no interval-form `date_add`; Spark's `date_add(date, num_days)` is integer-days only. Both adapters structurally rewrite to `date + interval`. Q-FUNCS-MAP-008. `TD-FUNCS-MAPPING-DATE-ADD-SPARK`. |
| `date_sub` | `date_sub(date, interval) -> Date` | `date - interval` 🟡 | Structural | `date_sub(date, interval)` 🟡 | Name-only | `date - interval` 🟡 | Structural | Mirrors `date_add`. DF / Spark structural rewrite. |
| `date_diff` | `date_diff(d1, d2) -> Long` (days) 🟡 | `CAST(d2 - d1 AS BIGINT)` 🟡 | Structural | `date_diff('day', d1, d2)` 🟡 | Structural | `datediff(d2, d1)` 🟡 | Name-remap | ⚠️ Three-way signature divergence. Canonical = 2-arg integer-days form (matches Spark). DF structural rewrite; DuckDB structural rewrite adding the `'day'` unit arg; Spark name-remap (underscore stripped). Q-FUNCS-MAP-009. `TD-FUNCS-MAPPING-DATE-DIFF-ARITY`. |
| `extract` | `extract(part, expr) -> Long` 🟡 | `extract(YEAR FROM expr)` | Name-only | `extract(YEAR FROM expr)` | Name-only | `extract(YEAR FROM expr)` | Name-only | ⚠️ Canonical `extract` vs `date_part` overlap. Both are listed; `14a §4.4` treats them as aliases. See Q-FUNCS-MAP-007. |
| `to_date` | `to_date(str, [format]) -> Date` | `to_date(str, [fmt])` | Name-only | `CAST(str AS DATE)` (1-arg) 🟡 / `strptime(str, fmt)` (2-arg, reversed args) 🟡 | Structural | `to_date(str, [fmt])` | Name-only | ⚠️ DuckDB has no native `to_date()`. 1-arg: structural rewrite to `CAST`. 2-arg: **demoted to adapter-extended** (DF + Spark only) due to DuckDB's `strptime` arg reversal. Q-FUNCS-MAP-011. `TD-FUNCS-MAPPING-TO-DATE-FORMAT`. |
| `to_timestamp` | `to_timestamp(str, [format]) -> Timestamp` | `to_timestamp(str, [fmt])` | Name-only | `CAST(str AS TIMESTAMP)` (1-arg) 🟡 / `strptime(str, fmt)::TIMESTAMP` (2-arg) 🟡 | Structural | `to_timestamp(str, [fmt])` | Name-only | Same pattern as `to_date`. Format-string dialects differ across engines (strftime vs Java SimpleDateFormat) — see `TD-FUNCS-MAPPING-DATETIME-FORMATS`. |

Portability summary: a mix of **Universal** (date_part, year..second, current_date, current_timestamp, extract), **Partial** (date_add, date_sub, date_diff with structural rewrites on DF / DuckDB / Spark), and **demoted / split** (2-arg `to_date`, 2-arg `to_timestamp`).

---

## 10. Logical / Conditional Helpers (`14a §4.5`)

*Round-2 intersection candidates from `14a §4.5`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `greatest` | `greatest(x, ...) -> same` (variadic) 🟡 | `greatest(x, ...)` | Name-only | `greatest(x, ...)` | Name-only | `greatest(x, ...)` | Name-only | ⚠️ NULL semantics divergence: DF / Spark propagate NULL; DuckDB ignores NULL. See Q-FUNCS-MAP-014. `TD-FUNCS-MAPPING-GREATEST-LEAST-NULL`. |
| `least` | `least(x, ...) -> same` (variadic) 🟡 | `least(x, ...)` | Name-only | `least(x, ...)` | Name-only | `least(x, ...)` | Name-only | Same NULL-semantics caveat as `greatest`. |
| `if` | `if(cond, then, else) -> unified(then, else)` | *(no native `if` function; use `CASE`)* | Structural | *(no native `if` function; use `CASE`)* | Structural | `if(cond, then, else)` | Name-only | **Demoted to adapter-extended** (Spark-only). Canonical authors express this as `Expr::Case { when: [{condition, result}], else_expr: Some(else) }`. Q-FUNCS-MAP-015. |
| `ifnull` | `ifnull(x, y)` | *(alias: `coalesce(x, y)`)* 🟡 | Name-remap | `ifnull(x, y)` / `coalesce(x, y)` | Name-only | `ifnull(x, y)` / `nvl(x, y)` / `coalesce(x, y)` | Name-only | **Demoted to adapter-extended.** Canonical authors use `Expr::Coalesce(args=[x, y])` (dedicated variant per `14 §3.2`). Q-FUNCS-MAP-015. |
| `nvl` | `nvl(x, y)` | *(no native `nvl`)* | Unsupported | `nvl(x, y)` / `coalesce(x, y)` | Name-only | `nvl(x, y)` / `coalesce(x, y)` | Name-only | **Demoted to adapter-extended** (DuckDB + Spark). Canonical authors use `Expr::Coalesce`. Q-FUNCS-MAP-015. |

Portability summary: `greatest` / `least` are **Partial** (universal names, divergent NULL semantics). `if` / `ifnull` / `nvl` demoted to adapter-extended per Q-FUNCS-MAP-015.

---

## 11. Dedicated-Variant Functions (`14 §3.2`)

Functions that have a dedicated `Expr` variant rather than flowing through `FunctionCall` — these are NOT in the `FunctionRegistry` (per `14 §3.2` notes, `14a §4.2–§4.5` exclusions). Adapters render them via the Dialect layer's dedicated methods (`SqlDialect::like`, `ilike`, `regexp_match`, `regexp_extract`, `date_trunc`, `type_name`, etc.) with one exception (DataFusion `RegexpExtract`) that is reshaped to a `FunctionCall` at the PlanBuilder layer.

| Canonical | IR variant | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `case` | `Expr::Case { when, else_expr }` | `CASE WHEN cond THEN val [ELSE val] END` | Name-only | `CASE WHEN cond THEN val [ELSE val] END` | Name-only | `CASE WHEN cond THEN val [ELSE val] END` | Name-only | Universal SQL. Dialect-layer rendering. |
| `coalesce` | `Expr::Coalesce(args)` | `coalesce(a, b, ...)` | Name-only | `coalesce(a, b, ...)` | Name-only | `coalesce(a, b, ...)` | Name-only | Universal. |
| `nullif` | `Expr::NullIf { left, right }` | `nullif(a, b)` | Name-only | `nullif(a, b)` | Name-only | `nullif(a, b)` | Name-only | Universal. |
| `is_null` | `Expr::IsNull(expr)` | `expr IS NULL` | Name-only | `expr IS NULL` | Name-only | `expr IS NULL` | Name-only | Universal. (Also documented in §6 logical.) |
| `is_not_null` | `Expr::IsNotNull(expr)` | `expr IS NOT NULL` | Name-only | `expr IS NOT NULL` | Name-only | `expr IS NOT NULL` | Name-only | Universal. |
| `in_list` | `Expr::InList { expr, list, negated }` | `expr IN (a, b, ...)` / `expr NOT IN (...)` | Name-only | `expr IN (...)` / `expr NOT IN (...)` | Name-only | `expr IN (...)` / `expr NOT IN (...)` | Name-only | Universal. `negated` flag flips `IN` ↔ `NOT IN`. |
| `between` | `Expr::Between { expr, low, high, negated }` | `expr BETWEEN low AND high` / `expr NOT BETWEEN ...` | Name-only | `expr BETWEEN ... AND ...` | Name-only | `expr BETWEEN ... AND ...` | Name-only | Universal. |
| `like` | `Expr::Like { expr, pattern, negated }` | `expr LIKE pattern` / `expr NOT LIKE pattern` | Name-only | `expr LIKE pattern` | Name-only | `expr LIKE pattern` | Name-only | Universal. |
| `ilike` | `Expr::ILike { expr, pattern, negated }` | `expr ILIKE pattern` | Name-only | `expr ILIKE pattern` | Name-only | `LOWER(expr) LIKE LOWER(pattern)` | Structural | ⚠️ Spark has no native ILIKE; Dialect-layer emits the `LOWER(...)` form. ANSI fallback uses the same form. |
| `regexp_match` | `Expr::RegexpMatch { expr, pattern, negated }` → Boolean | `regexp_like(expr, pattern)` | Name-remap | `regexp_matches(expr, pattern)` | Name-remap | `expr RLIKE pattern` (full-match) / `expr RLIKE CONCAT('.*', pattern, '.*')` (partial) | Structural | ⚠️ Three different spellings. Regex dialect is canonical (RE2-compatible subset per `14 §3.2`) — per-engine regex-flavor variance is out of scope. |
| `regexp_extract` | `Expr::RegexpExtract { expr, pattern, group }` → String | `array_element(regexp_match(expr, pattern), group + 1)` 🟡 | Structural (PlanBuilder) | `regexp_extract(expr, pattern, group)` | Name-only | `regexp_extract(expr, pattern, group)` | Name-only | ⚠️ DataFusion has no scalar `regexp_extract`; `regexp_match` returns `List<Utf8>`. PlanBuilder-layer rewrites to `array_element(regexp_match(...), group + 1)` (1-indexed). Canonical `group` convention: `0` = entire match, `1+` = capture groups (matches DuckDB / Spark). |
| `date_trunc` | `Expr::DateTrunc { expr, grain }` → same | `date_trunc('grain', expr)` | Name-only | `date_trunc('grain', expr)` | Name-only | `date_trunc('grain', expr)` | Name-only | Universal. `grain` is a `Grain` enum from `13` — the adapter lowercases the variant's name for `'grain'`. |
| `cast` | `Expr::Cast { expr, target }` | `CAST(expr AS <type>)` | Name-only | `CAST(expr AS <type>)` / `expr::<type>` | Name-only | `CAST(expr AS <type>)` | Name-only | Universal. Target-type spelling per `registry/types_mapping.md §1`. |
| `negate` | `Expr::Negate(expr)` | `-expr` | Name-only | `-expr` | Name-only | `-expr` | Name-only | Universal. Operand type classes (numeric / interval) engine-validated. |
| `not` | `Expr::Not(expr)` | `NOT expr` | Name-only | `NOT expr` | Name-only | `NOT expr` | Name-only | Universal. (Also in §6.) |

Portability summary: **Universal** for most. `ilike` is **Partial** (Spark structural). `regexp_match` is **Partial** (all three differ). `regexp_extract` is **Partial** (DataFusion structural via PlanBuilder).

### 11.1 `regexp_replace` — `FunctionCall`, not a dedicated variant

`regexp_replace` does NOT have a dedicated `Expr` variant (per `14 §3.2`); it is authored as a `FunctionCall` and lives in the canonical catalog (legacy §8 P5).

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `regexp_replace` | `regexp_replace(str, pattern, replacement) -> String` | `regexp_replace(str, regexp, replacement [, flags])` | Name-only | `regexp_replace(str, pattern, replacement [, options])` | Name-only | `regexp_replace(str, regexp, rep)` | Name-only | Canonical = 3-arg form. 4-arg form differs semantically (DF/DuckDB = flags; Spark = start position) — 4-arg is demoted to adapter-extended only. Q-FUNCS-MAP-006. |

---

## 12. Adapter-Extended Functions (`14a §7`)

Engine-specific functions NOT in the canonical catalog, registered via `RegistryExtension` impls per `14a §7.1`. Listed here for cross-reference; the authoritative inventory lives in each adapter crate. Every entry 🟡 pending `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` (Q-FUNCS-MAP-018).

### 12.1 DataFusion-extended

| Name | Signature | Purpose | Source |
|---|---|---|---|
| `array_element` | `array_element(array, idx) -> element_type` | 1-indexed array-element access. Used internally by the `RegexpExtract` structural rewrite (§11). | DataFusion built-in. |
| `regexp_match` | `regexp_match(str, pattern) -> List<Utf8>` | Returns array of all capture groups. Not canonical because the `List` return type is engine-specific; canonical `regexp_match` returns `Boolean`. | DataFusion built-in. |
| `array_to_string` | `array_to_string(array, sep) -> String` | Array join. | DataFusion built-in. |
| `sha2` / `sha224` / `sha384` / `sha512` | `sha_(str) -> String` | Cryptographic hashes. Not canonical v1. | DataFusion built-in. |
| `nanvl` | `nanvl(x, y) -> same` 🟡 | NaN-safe `ifnull` for floats. | DataFusion built-in. |

### 12.2 DuckDB-extended

| Name | Signature | Purpose | Source |
|---|---|---|---|
| `list_extract` | `list_extract(list, idx) -> element_type` | 1-indexed list access. DuckDB idiom for array element lookup. | DuckDB built-in. |
| `regexp_extract_all` | `regexp_extract_all(str, pattern) -> List<String>` | All matches, not just first. | DuckDB built-in. |
| `list_append` | `list_append(list, elem) -> list` | Immutable list append. | DuckDB built-in. |
| `strptime` | `strptime(str, fmt) -> Timestamp` | Custom-format timestamp parse. See Q-FUNCS-MAP-011. | DuckDB built-in. |
| `epoch_ms` | `epoch_ms(ts) -> Long` 🟡 | Timestamp → epoch milliseconds. | DuckDB built-in. |

### 12.3 Spark-extended

| Name | Signature | Purpose | Source |
|---|---|---|---|
| `collect_set` | `collect_set(expr) -> Array<T>` | Aggregate to distinct array. `FunctionCategory::Aggregate`. | Spark built-in. |
| `collect_list` | `collect_list(expr) -> Array<T>` | Aggregate to array (preserving duplicates). `FunctionCategory::Aggregate`. Used by the `string_agg` structural rewrite (§3.2). | Spark built-in. |
| `array_join` | `array_join(array, sep, [null_replacement]) -> String` | Array join. | Spark built-in. |
| `percentile_approx` | `percentile_approx(expr, frac) -> same` | Approximate percentile. | Spark built-in. |
| `try_divide` | `try_divide(a, b) -> Double` (Spark 3.3+) 🟡 | NULL-on-zero-divisor safe division. See Q-FUNCS-MAP-017. | Spark built-in. |
| `pmod` | `pmod(a, b) -> same` | Positive modulo. | Spark built-in. |
| `startswith` / `endswith` | Spark native names for `starts_with` / `ends_with`. | — | Spark built-in; adapter name-remaps to canonical if promoted (§7.1). |

### 12.4 Cross-engine adapter-extended (legacy demotions)

These were canonical in legacy `FUNCTION_CATALOG.md` but failed `14a` Q10 intersection; they remain usable under per-adapter registration but are NOT canonical.

| Name | Rationale for demotion | Per-adapter disposition | TD |
|---|---|---|---|
| `initcap` | No DuckDB native. | DataFusion + Spark adapter-extended. Author using DuckDB must emulate or omit. | `TD-FUNCS-MAPPING-INITCAP` |
| `percentile_cont` / `percentile_disc` | DuckDB uses `WITHIN GROUP` syntax; Spark lacks exact variants. | All three adapter-extended with per-engine signatures. | `TD-FUNCS-MAPPING-PERCENTILE` |
| 2-arg `to_date` / `to_timestamp` | DuckDB's `strptime` reverses arg order. | DataFusion + Spark adapter-extended as 2-arg; DuckDB adapter-extended as `strptime` (separate canonical name). | `TD-FUNCS-MAPPING-TO-DATE-FORMAT` |
| `if(cond, then, else)` | Not universal (Spark-only native). | Spark adapter-extended; canonical authors use `Expr::Case`. | `TD-FUNCS-MAPPING-IF-IFNULL-NVL` |
| `ifnull` / `nvl` | Overlap with `coalesce`; DF lacks `nvl`. | Per-adapter aliases; canonical authors use `Expr::Coalesce`. | Same TD. |

---

## 13. Per-Adapter Rewrite Summary

Cross-references every rewrite asserted in §§3–11 into a per-adapter layering digest. Useful as a checklist during adapter implementation and as a lookup-by-engine when investigating divergent emission. `PB` = PlanBuilder layer (`EngineAdapter::rewrite_expr`); `DL` = Dialect layer (`SqlDialect` trait method).

### 13.1 DataFusion

**PlanBuilder-layer rewrites** (canonical `Expr` → engine-oriented `Expr`):

| Source | Target | Tier | §ref |
|---|---|---|---|
| `FunctionCall("position", [s, t])` | `FunctionCall("strpos", [t, s])` | Name-remap + arg-reorder | §7 |
| `FunctionCall("sign", [x])` | `FunctionCall("signum", [x])` | Name-remap | §8 |
| `FunctionCall("variance", [x])` | `FunctionCall("var_samp", [x])` 🟡 | Name-remap | §3.2 |
| `FunctionCall("approx_count_distinct", [x])` | `FunctionCall("approx_distinct", [x])` 🟡 | Name-remap | §3.2 |
| `FunctionCall("date_add", [d, i])` | `BinaryOp(Add, d, i)` | Structural | §9 |
| `FunctionCall("date_sub", [d, i])` | `BinaryOp(Subtract, d, i)` | Structural | §9 |
| `FunctionCall("date_diff", [d1, d2])` | `Cast(BinaryOp(Subtract, d2, d1), Long)` 🟡 | Structural | §9 |
| `FunctionCall("to_date", [s])` | `Cast(s, Date)` 🟡 | Structural (DuckDB only; DF is Name-only) | — |
| `Expr::RegexpExtract { expr, pattern, group }` | `FunctionCall("array_element", [FunctionCall("regexp_match", [expr, pattern]), group + 1])` | Structural | §11 |
| `FunctionCall("percentile_cont", [x, p])` | `FunctionCall("approx_percentile_cont", [x, p])` 🟡 | Name-remap | §3.2 |

**Dialect-layer rendering** (dedicated variants → SQL):

| Variant | SQL | Method |
|---|---|---|
| `Expr::Like` | `expr LIKE pattern` | default `binary_op` path |
| `Expr::ILike` | `expr ILIKE pattern` | `ilike` |
| `Expr::RegexpMatch` | `regexp_like(expr, pattern)` (partial) / `regexp_like(expr, CONCAT('^', pattern, '$'))` (full) | `regexp_match` |
| `Expr::RegexpExtract` | *(rewritten to FunctionCall at PlanBuilder; rendered via generic call path)* | — |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc` |
| `Expr::Cast` | `CAST(expr AS <type>)` | `type_name` (per `types_mapping §1`) |
| `Aggregate { distinct: true }` | `fn(DISTINCT expr)` | aggregate-render path |

### 13.2 DuckDB

**PlanBuilder-layer rewrites:**

| Source | Target | Tier | §ref |
|---|---|---|---|
| `FunctionCall("position", [s, t])` | `FunctionCall("strpos", [t, s])` | Name-remap + arg-reorder | §7 |
| `FunctionCall("to_date", [s])` | `Cast(s, Date)` 🟡 | Structural | §9 |
| `FunctionCall("to_timestamp", [s])` | `Cast(s, Timestamp)` 🟡 | Structural | §9 |
| `FunctionCall("date_diff", [d1, d2])` | `FunctionCall("date_diff", [Literal("day"), d1, d2])` 🟡 | Structural (3-arg) | §9 |
| `FunctionCall("log", [base, x])` | *(left as-is if DuckDB supports 2-arg form; else arity mismatch)* 🟡 | — | §8 |
| `FunctionCall("percentile_cont", [x, p])` | `<WITHIN GROUP structural>` 🟡 | Structural | §3.2 |

**Dialect-layer rendering:**

| Variant | SQL | Method |
|---|---|---|
| `Expr::Like` | `expr LIKE pattern` | default |
| `Expr::ILike` | `expr ILIKE pattern` | `ilike` |
| `Expr::RegexpMatch` | `regexp_matches(expr, pattern)` (partial) / `regexp_matches(expr, CONCAT('^', pattern, '$'))` (full) | `regexp_match` |
| `Expr::RegexpExtract` | `regexp_extract(expr, pattern, group)` | `regexp_extract` |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc` |
| `Expr::Cast` | `CAST(expr AS <type>)` | `type_name` |

### 13.3 Spark

**PlanBuilder-layer rewrites:**

| Source | Target | Tier | §ref |
|---|---|---|---|
| `FunctionCall("position", [s, t])` | `FunctionCall("locate", [s, t])` 🟡 | Name-remap (arg order matches canonical) | §7 |
| `FunctionCall("starts_with", [s, p])` | `FunctionCall("startswith", [s, p])` 🟡 | Name-remap (underscore strip) | §7.1 |
| `FunctionCall("ends_with", [s, p])` | `FunctionCall("endswith", [s, p])` 🟡 | Name-remap | §7.1 |
| `FunctionCall("sign", [x])` | `FunctionCall("signum", [x])` 🟡 | Name-remap | §8 |
| `FunctionCall("date_add", [d, i])` | `BinaryOp(Add, d, i)` | Structural | §9 |
| `FunctionCall("date_sub", [d, i])` | `BinaryOp(Subtract, d, i)` | Structural | §9 |
| `FunctionCall("date_diff", [d1, d2])` | `FunctionCall("datediff", [d2, d1])` 🟡 | Name-remap + arg-reorder | §9 |
| `FunctionCall("string_agg", [e, s])` | `FunctionCall("array_join", [FunctionCall("collect_list", [e]), s])` 🟡 | Structural | §3.2 |
| `FunctionCall("median", [x])` (Spark < 3.4) | `FunctionCall("percentile_approx", [x, 0.5])` 🟡 | Structural | §3.2 |
| `BinaryOp(SafeDivide, a, b)` (Spark 3.3+) | `FunctionCall("try_divide", [a, b])` 🟡 | Structural (optional optimization) | §5.1 |

**Dialect-layer rendering:**

| Variant | SQL | Method |
|---|---|---|
| `Expr::Like` | `expr LIKE pattern` | default |
| `Expr::ILike` | `LOWER(expr) LIKE LOWER(pattern)` | `ilike` (no native ILIKE) |
| `Expr::RegexpMatch` | `expr RLIKE pattern` (full-match) / `expr RLIKE CONCAT('.*', pattern, '.*')` (partial) | `regexp_match` |
| `Expr::RegexpExtract` | `regexp_extract(expr, pattern, group)` | `regexp_extract` |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc` |
| `Expr::Cast` | `CAST(expr AS <type>)` | `type_name` |

### 13.4 ANSI (fallback dialect)

For adapters that emit ANSI SQL without a specific engine target (used by the catalog-inspection-only paths or as a baseline-comparison form). Every rewrite is Name-only at the Dialect layer:

| Variant | SQL |
|---|---|
| `Expr::ILike` | `LOWER(expr) LIKE LOWER(pattern)` |
| `Expr::RegexpMatch` | `REGEXP_LIKE(expr, pattern)` |
| `Expr::RegexpExtract` | `REGEXP_EXTRACT(expr, pattern, group)` |
| `Expr::DateTrunc` | `DATE_TRUNC('grain', expr)` |
| `Expr::Cast` | `CAST(expr AS <type>)` |

ANSI does not perform PlanBuilder-layer rewrites — it emits the canonical `Expr` tree verbatim where SQL standard supports it.

---

## 14. Per-Engine Coverage Gaps

Canonical-catalog entries from `14a §4` that FAILED the Q10 intersection test and were demoted to adapter-extended. Each gap has a `TD-FUNCS-MAPPING-*` tag tracking the demotion and any potential future promotion if the engine landscape shifts. Cross-references §13's per-adapter rewrite summaries.

| TD ID | Canonical candidate | Failing engine(s) | Demotion reason | Resolution path |
|---|---|---|---|---|
| `TD-FUNCS-MAPPING-INITCAP` | `initcap` | DuckDB | No native equivalent. | DuckDB adds native `initcap`, or adapter accepts structural emulation (e.g. `regexp_replace` pattern). |
| `TD-FUNCS-MAPPING-PERCENTILE` | `percentile_cont` / `percentile_disc` | Spark (lacks exact), DuckDB (`WITHIN GROUP` syntax) | Signature divergence. | Spark adds exact-percentile aggregate, or canonical catalog accepts a structural rewrite for DuckDB. |
| `TD-FUNCS-MAPPING-TO-DATE-FORMAT` | 2-arg `to_date(str, fmt)` / `to_timestamp(str, fmt)` | DuckDB (uses `strptime`, reversed args) | Arg-order divergence on DuckDB. | DuckDB adds `to_date` wrapper, or canonical accepts arg-swap structural rewrite. |
| `TD-FUNCS-MAPPING-IF-IFNULL-NVL` | `if` / `ifnull` / `nvl` | All three except Spark (`if`), DataFusion (`nvl`). | Overlap with `coalesce` / `case` dedicated variants. | Consensus to keep these as adapter-extended — dedicated variants subsume the use-cases. |
| `TD-FUNCS-MAPPING-DATE-ADD-SPARK` | `date_add(date, interval)` | Spark (only integer-days form native) | Arg-type divergence. | Already structurally rewritten via `date + interval` at Spark adapter's PlanBuilder. Non-blocking. |
| `TD-FUNCS-MAPPING-DATE-DIFF-ARITY` | `date_diff(d1, d2)` | All three diverge in arity / name / unit | Signature divergence. | Canonical = 2-arg; per-engine structural + name-remap rewrites in place. |
| `TD-FUNCS-MAPPING-LOG-ARITY` | `log(base, x)` | DuckDB (1-arg base-10 only) | Arity / semantic divergence. | DuckDB adds 2-arg form, or canonical splits into `log` (1-arg base-10 only) + `logb` (2-arg). |
| `TD-FUNCS-MAPPING-SAFEDIVIDE-SPARK` | `SafeDivide` Spark rendering | — | Optimization opportunity, not a gap. | Spark 3.3+ adapter may emit `try_divide`. |
| `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` | BinaryOp promotion tables §5.2–§5.3 | All three | Rows drafted from docs, not empirically verified. | Test harness against live adapter instances. |
| `TD-FUNCS-MAPPING-DECIMAL-DIV` | `Decimal / Decimal` result type | DuckDB (divergent from DF / Spark) | Result-type divergence. | Reconciliation Cast at Semantics boundary. |
| `TD-FUNCS-MAPPING-DATE-SUB-DATE` | `Date - Date` result type | DataFusion (returns `Interval`) vs DuckDB / Spark (return `Integer` days) | Result-type divergence. | Document as expected; author declares `data_type:` appropriately. |
| `TD-FUNCS-MAPPING-AGG-INTERSECTION` | Non-closed aggregates §3.2 | All three (pending verification) | Intersection not yet verified. | `14a` Round-2 intersection scan. |
| `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` | §12 seed lists | All three | Inventories not yet authoritative. | Per-adapter crate README ratifies full list. |
| `TD-FUNCS-MAPPING-DATETIME-FORMATS` | `to_date` / `to_timestamp` format strings | All three differ (strftime / Java SimpleDateFormat) | Format-string dialect divergence. | Either canonical format-string grammar OR engine-delegated per-call. |
| `TD-FUNCS-MAPPING-CONCAT-WS-PROMOTE` | `concat_ws` | — | Not in `14a §4.2`; universal per legacy. | Add to `14a §4.2` in Round-2. |
| `TD-FUNCS-MAPPING-STARTS-WITH-PROMOTE` | `starts_with` / `ends_with` | Spark (name-remap, no underscore) | Not in `14a §4.2`; minor name-remap only. | Add to `14a §4.2` in Round-2. |
| `TD-FUNCS-MAPPING-REPEAT` | `repeat` | — | Not in `14a §4.2`; universal per legacy. | Add to `14a §4.2` in Round-2. |
| `TD-FUNCS-MAPPING-CONTAINS` | `contains` | — | Not in `14a §4.2`; universal per legacy §12. | Add to `14a §4.2` in Round-2. |
| `TD-FUNCS-MAPPING-TRANSLATE` | `translate` | — | Not in `14a §4.2`; universal per legacy §12. | Add to `14a §4.2` in Round-2. |
| `TD-FUNCS-MAPPING-TRUNC-PROMOTE` | `trunc` / `truncate` | — | Not in `14a §4.3`; universal per legacy §12. | Add to `14a §4.3` in Round-2. |
| `TD-FUNCS-MAPPING-LEFT-RIGHT` | `left` / `right` | Spark (requires ≥ 3.2) | Version-floor constraint. | Confirm Spark floor; add to `14a §4.2`. |
| `TD-FUNCS-MAPPING-REGEXP-REPLACE-4ARG` | 4-arg `regexp_replace` | All three (DF / DuckDB = flags; Spark = position) | Semantic divergence. | Keep canonical as 3-arg; 4-arg adapter-extended with distinct per-engine specs. |

---

## 15. Versioning

Following `registry/types_mapping.md §4` and `README.md` policy, each mapping row SHOULD cite the adapter crate + verified engine version. Current default pins (tentative; may shift per Q-FUNCS-MAP-020):

| Engine | Target version(s) | Notes |
|---|---|---|
| DataFusion | 40.x+ 🟡 | Major releases every ~6-8 weeks. `contains` landed in 40.x; pre-40 adapter floors rejected. |
| DuckDB | 1.1.x 🟡 | Matches `types_mapping.md §1` pin. |
| Spark | 3.5.x (SQL dialect) 🟡 | Floor version for features like native `median`, `split_part`, `try_divide` — see per-row `(Spark 3.Z+)` notes. Spark 3.4 is the minimum for `TimestampNTZType` (per `types_mapping.md §3.2`), which transitively floors the adapter at 3.4. |

Rows documenting features behind a specific engine version carry `(Engine X.Y+)` inline. Unverified rows are marked 🟡; as adapter implementation lands and verifies each row against a live engine, the 🟡 marker is removed and the exact verified version replaces the range.

Breaking changes in an engine (rename, semantic change between major versions) are documented as additional rows or dated annotations — not destructive edits — per `README.md §versioning-and-churn`.

---

## 16. Cast Semantics of Function Results

Edge cases where the per-engine function-result type diverges despite identical canonical signatures. These flow into the Semantics-boundary reconciliation Cast rule (`14 §6.4` rule 2) and compose with `registry/types_mapping.md §2`'s canonical cast matrix.

### 16.1 Integer-division result

Per §5.2: canonical `a / b` where `a, b: Integer` produces:

| Engine | Result type | Reconciliation to declared `Integer` | Reconciliation to declared `Double` |
|---|---|---|---|
| DataFusion | `Integer` (truncating) 🟡 | None (identity). | Widening `CAST(... AS DOUBLE)` per `types_mapping §2.3`. |
| DuckDB | `Double` (promoting) 🟡 | Narrowing `CAST(... AS INTEGER)` — emits `EXPR_W_CAST_NARROW`; value-lossy. | None (identity). |
| Spark | `Double` (promoting) 🟡 | Narrowing `CAST(... AS BIGINT)` — emits `EXPR_W_CAST_NARROW`. | None. |

⚠️ Authors targeting multiple engines with a declared integer quotient should write an explicit cast in `expr:` rather than relying on boundary reconciliation — the cross-engine behavior difference is surprising. `TD-FUNCS-MAPPING-INT-DIV-RESULT`.

### 16.2 Decimal-division result

Per §5.3: DuckDB's decimal-division result-type formula (`p1 + s2` / `s1`) is narrower than DataFusion / Spark (Spark-style `p1 - s1 + s2 + max(6, s1 + p2 + 1)` / `max(6, s1 + p2 + 1)`). A Semantics declaring `data_type: Decimal(18, 4)` over a decimal division may need different reconciliation Casts per adapter. `TD-FUNCS-MAPPING-DECIMAL-DIV`.

### 16.3 `Date - Date` difference

Per §5.4: returns `Interval` on DataFusion, `Integer` (days) on DuckDB and Spark. A Semantics declaring `data_type: Integer` over `d2 - d1` reconciles differently:

| Engine | Natural result | Reconciliation to `Integer` days |
|---|---|---|
| DataFusion | `Interval` 🟡 | Structural — extract days via `date_part('day', ...)` or similar. Cannot be a simple `Cast`. |
| DuckDB | `Integer` 🟡 | None (identity). |
| Spark | `Integer` 🟡 | None (identity). |

DataFusion's divergence is best handled by canonicalizing `date_diff(d1, d2)` (§9) rather than writing `d2 - d1` directly. `TD-FUNCS-MAPPING-DATE-SUB-DATE`.

### 16.4 `count(*)` / `count(expr)` return width

| Engine | Return type |
|---|---|
| DataFusion | `Int64` / `Long` |
| DuckDB | `BIGINT` / `Long` |
| Spark | `LongType` / `Long` |

Convergent. Canonical `14 §5.4` specifies `Long`. No reconciliation needed.

### 16.5 `sum(Integer)` return width

| Engine | Return type |
|---|---|
| DataFusion | `Int64` / `Long` |
| DuckDB | `HUGEINT` (128-bit) 🟡 — divergent at extreme values | |
| Spark | `LongType` / `Long` |

⚠️ DuckDB promotes `sum` over 32-bit integers to 128-bit to protect against overflow; DF / Spark promote to 64-bit (SQL:2016 canonical, per `14 §5.4`). Large aggregations over DuckDB MAY produce values outside `Long` range — the adapter emits a warning-level Diagnostic (`EXPR_W_SUM_OVERFLOW_RISK` 🟡) when the physical source's row count × max-absolute-value estimate exceeds `i64::MAX`. `TD-FUNCS-MAPPING-SUM-HUGEINT`.

---

## 17. Interaction with Other Documents

- **`foundations/14_expressions.md`** — this registry is the downstream consumer of `14 §3.2`'s `Expr` variant catalog. Every dedicated variant and every `FunctionCall` name is mapped here.
- **`foundations/14a_function_catalog.md`** — canonical upstream. `14a §4` defines which functions are canonical; this doc maps them. `14a §5.2` defers per-engine BinaryOp reality here (fulfilled in §5).
- **`registry/README.md`** — shared policy (engine coverage, versioning, Living status).
- **`registry/types_mapping.md`** — canonical type mappings consumed by §11 (`Cast` variant target rendering) and §16 (reconciliation Casts).
- **`apis/36_semstrait_adapter.md`** — the `EngineAdapter` trait and its `Dialect` / `PlanBuilder` layering. This doc specifies WHAT each adapter rewrites; `36` specifies HOW the layering composes.
- **Adapter crates** (future `semstrait-adapter-datafusion`, `semstrait-adapter-duckdb`, `semstrait-adapter-spark`) — own the authoritative `FunctionRewriter` table and `RegistryExtension` implementations. §12 mirrors each adapter's contribution; the adapter's own README is authoritative.
- **`open_questions/functions_mapping_open_questions.md`** — parked unresolved questions surfaced by Round-2 drafting.

---

## 18. TECH_DEBT Index

Consolidated list of all `TD-FUNCS-MAPPING-*` entries emitted by this doc. Each maps back to its originating §.

| TD ID | § | Current posture |
|---|---|---|
| `TD-FUNCS-MAPPING-INITCAP` | 14 | Demoted to adapter-extended (DF + Spark only). |
| `TD-FUNCS-MAPPING-PERCENTILE` | 14 | Likely demoted pending Round-2 verification. |
| `TD-FUNCS-MAPPING-TO-DATE-FORMAT` | 14 | 2-arg forms demoted. |
| `TD-FUNCS-MAPPING-IF-IFNULL-NVL` | 14 | Demoted; canonical authors use `Case` / `Coalesce`. |
| `TD-FUNCS-MAPPING-DATE-ADD-SPARK` | 14 | Structural rewrite in place. |
| `TD-FUNCS-MAPPING-DATE-DIFF-ARITY` | 14 | Structural + name-remap in place. |
| `TD-FUNCS-MAPPING-LOG-ARITY` | 14 | Open — requires `14a` Round-2 clarification. |
| `TD-FUNCS-MAPPING-SAFEDIVIDE-SPARK` | 14 | Optimization; non-blocking. |
| `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` | 14 | Blocked on test harness. |
| `TD-FUNCS-MAPPING-DECIMAL-DIV` | 14 | Reconciliation Cast at Semantics boundary. |
| `TD-FUNCS-MAPPING-DATE-SUB-DATE` | 14 | Documented as expected divergence. |
| `TD-FUNCS-MAPPING-AGG-INTERSECTION` | 14 | Blocked on `14a` Round-2. |
| `TD-FUNCS-MAPPING-ADAPTER-INVENTORY` | 14 | Blocked on per-adapter crate readmes. |
| `TD-FUNCS-MAPPING-DATETIME-FORMATS` | 14 | Open design item. |
| `TD-FUNCS-MAPPING-CONCAT-WS-PROMOTE` | 14 | Proposed `14a` Round-2 addition. |
| `TD-FUNCS-MAPPING-STARTS-WITH-PROMOTE` | 14 | Proposed `14a` Round-2 addition. |
| `TD-FUNCS-MAPPING-REPEAT` | 14 | Proposed `14a` Round-2 addition. |
| `TD-FUNCS-MAPPING-CONTAINS` | 14 | Proposed `14a` Round-2 addition. |
| `TD-FUNCS-MAPPING-TRANSLATE` | 14 | Proposed `14a` Round-2 addition. |
| `TD-FUNCS-MAPPING-TRUNC-PROMOTE` | 14 | Proposed `14a` Round-2 addition. |
| `TD-FUNCS-MAPPING-LEFT-RIGHT` | 14 | Proposed `14a` Round-2 addition, version-floor-constrained. |
| `TD-FUNCS-MAPPING-REGEXP-REPLACE-4ARG` | 14 | 4-arg form adapter-extended per engine. |
| `TD-FUNCS-MAPPING-INT-DIV-RESULT` | 16.1 | Divergent per engine; author-side cast recommended. |
| `TD-FUNCS-MAPPING-SUM-HUGEINT` | 16.5 | Warning Diagnostic on DuckDB when range risk exceeds `i64::MAX`. |
