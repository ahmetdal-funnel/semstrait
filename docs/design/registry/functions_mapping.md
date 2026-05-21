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

> **Status (2026-04-20):** Round-2 scaffold drafted against `14a` Q10 intersection-only population policy. Every row tracks (canonical name, signature, per-engine native form, rewrite tier, verification marker). Entries marked 🟡 are plausible from legacy `docs/FUNCTION_CATALOG.md` + engine-docs but have not been empirically verified against a live adapter test harness. Unresolved questions parked in `questions/open/functions_mapping_questions.md`.

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

### 3.2 Non-closed aggregates (`14a §4.6`)

*Ratified Round-2 (2026-05-21). Canonical FunctionSpecs at `14a §4.6`. All entries `FunctionCategory::Aggregate` + `Additivity::NonAdditive`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `stddev` | `(Numeric) -> Double` | `stddev` (alias `stddev_samp`) | Name-only | `stddev` (alias `stddev_samp`) | Name-only | `stddev` (aliases `stddev_samp`, `std`) | Name-only | Bare name = sample across all three (since Spark 1.6). |
| `stddev_pop` | `(Numeric) -> Double` | `stddev_pop` | Name-only | `stddev_pop` | Name-only | `stddev_pop` | Name-only | Population variant. Universal. |
| `variance` | `(Numeric) -> Double` | `var_samp` (PlanBuilder name-remap; DF bare name is `var`) | Name-remap | `variance` (alias `var_samp`) | Name-only | `variance` (alias `var_samp`; since 1.6) | Name-only | Bare name = sample. DF historically uses `var` / `var_samp`; PlanBuilder rewrites to `var_samp` for clarity. |
| `var_pop` | `(Numeric) -> Double` | `var_pop` | Name-only | `var_pop` | Name-only | `var_pop` | Name-only | Population variant. Universal. |
| `median` | `(Numeric) -> Double`, `(Decimal(p,s)) -> Decimal(p,s)` | `median` (exact) | Name-only | `median` | Name-only | `median` (Spark 3.4+) | Name-only | Spark floor at 3.4+ — consistent with `types_mapping.md §3.2`'s 3.4 floor for `TimestampNTZType`. |
| `string_agg` | `(String, String) -> String` | `string_agg(expr, sep)` | Name-only | `string_agg(expr, sep)` (aliases `group_concat`, `listagg`) | Name-only | `string_agg(expr, sep)` (Spark 3.3+) | Name-only | Spark floor at 3.3+. ORDER BY / DISTINCT clause modifiers are adapter-extended. |
| `percentile_cont` | `(Float, Numeric) -> Double` | `percentile_cont(p) WITHIN GROUP (ORDER BY col)` | Name-only (dialect-layer) | `percentile_cont(p) WITHIN GROUP (ORDER BY col)` | Name-only (dialect-layer) | `percentile_cont(p) WITHIN GROUP (ORDER BY col)` (Spark 3.1+) | Name-only (dialect-layer) | Author-facing surface is FunctionCall `percentile_cont(fraction, col)`. Dialect layer renders SQL-standard `WITHIN GROUP (ORDER BY col)` form (same shape as `count(DISTINCT)`'s DISTINCT rendering — pure SQL syntax, not a structural Expr rewrite). |
| `approx_count_distinct` | `(Any) -> Long` | `approx_distinct` (PlanBuilder name-remap) | Name-remap | `approx_count_distinct` | Name-only | `approx_count_distinct` (since 1.6) | Name-only | DF emits `approx_distinct`. Implementation backends differ (HyperLogLog vs HyperLogLog++) — engine-delegated per `14a §6.2`. |

Portability summary: 8 ratified canonical entries. **Universal** (Name-only) on `stddev`, `stddev_pop`, `var_pop`, `median`, `string_agg`, `percentile_cont`, and `approx_count_distinct`-on-DuckDB/Spark. Two name-remaps on DataFusion (`variance` → `var_samp`; `approx_count_distinct` → `approx_distinct`). Spark version floors: `median` 3.4+, `string_agg` 3.3+, `percentile_cont` 3.1+ — within the existing 3.4+ floor mandated by `types_mapping.md §3.2`.

`percentile_disc` is NOT canonical — DataFusion lacks it entirely. Adapter-extended on DuckDB + Spark only (§12.4).

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

Per `14 §5.4`, semstrait does NOT validate operand-type compatibility for comparisons — each engine raises its own diagnostics at execution time if e.g. `Integer < String` is attempted. Cross-engine comparability rules (`Integer` vs `Double`, `String` vs `Date`) are engine-native; see §5 below for arithmetic promotion reference tables.

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

*Ratified Round-2 (2026-05-21). Canonical FunctionSpecs at `14a §4.2`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `upper` | `(String) -> String` | `upper` | Name-only | `upper` | Name-only | `upper` | Name-only | Universal. Unicode-aware. |
| `lower` | `(String) -> String` | `lower` | Name-only | `lower` | Name-only | `lower` | Name-only | Universal. Unicode-aware. |
| `length` | `(String) -> Integer`, `(Array<T>) -> Integer` | String → `length` (or `character_length`); Array → `array_length` | Name-remap (Array) | `length` (polymorphic over String + LIST) | Name-only | String → `length`; Array → `size` | Name-remap (Array) | Character count for String; element count for Array. DF returns `Int32` for `Utf8`/`Utf8View`, `Int64` for `LargeUtf8` — adapter normalizes. |
| `substring` | `(String, Integer) -> String`, `(String, Integer, Integer) -> String` | `substr` / `substring` | Name-only | `substring` / `substr` | Name-only | `substring` / `substr` | Name-only | 1-indexed. Positive `pos` only — negative `pos` is non-portable (each engine differs) and rejected at compile. |
| `trim` | `(String) -> String`, `(String, String) -> String` | `btrim` (alias `trim`) | Name-remap | `trim` | Name-only | `trim` | Name-only | Default = ASCII space `0x20`. 2-arg form: strips any character in the second arg from both ends. |
| `ltrim` | `(String) -> String`, `(String, String) -> String` | `ltrim` | Name-only | `ltrim` | Name-only | `ltrim` (Spark 3.x: `(str, set)` order) | Name-only | Default = ASCII space. Set semantics in 2-arg form. |
| `rtrim` | `(String) -> String`, `(String, String) -> String` | `rtrim` | Name-only | `rtrim` | Name-only | `rtrim` (Spark 3.x: `(str, set)` order) | Name-only | Default = ASCII space. Set semantics in 2-arg form. |
| `concat` | variadic `(String, String...) -> String` | `a || b || c || ...` (PlanBuilder rewrite) | Structural | `a || b || c || ...` (PlanBuilder rewrite) | Structural | `concat(...)` | Name-only | NULL-propagating canonical. DF / DuckDB native `concat` is NULL-skip — rewritten to `||`-chain so semantics match Spark's NULL-propagating `concat`. |
| `replace` | `(String, String, String) -> String` | `replace` | Name-only | `replace` | Name-only | `replace` | Name-only | Replaces all occurrences. Literal substring (not regex). |
| `lpad` | `(String, Integer) -> String`, `(String, Integer, String) -> String` | `lpad` (both arities) | Name-only | 2-arg → `lpad(s, n, ' ')` rewrite; 3-arg native | Structural (2-arg) / Name-only (3-arg) | 2-arg → `lpad(s, n, ' ')` rewrite; 3-arg native | Structural (2-arg) / Name-only (3-arg) | DF native for both arities; Spark / DuckDB require 3-arg natively, so 2-arg form is rewritten by injecting `' '` at the PlanBuilder layer. Truncates if input already exceeds target length. |
| `rpad` | `(String, Integer) -> String`, `(String, Integer, String) -> String` | `rpad` (both arities) | Name-only | 2-arg → `rpad(s, n, ' ')` rewrite; 3-arg native | Structural (2-arg) / Name-only (3-arg) | 2-arg → `rpad(s, n, ' ')` rewrite; 3-arg native | Structural (2-arg) / Name-only (3-arg) | Mirror of `lpad`. |
| `reverse` | `(String) -> String`, `(Array<T>) -> Array<T>` | String → `reverse`; Array → `array_reverse` | Name-remap (Array) | `reverse` (polymorphic over String + LIST) | Name-only | `reverse` (polymorphic over String + ARRAY) | Name-only | Reverses by code points (String) / element order (Array). |
| `split_part` | `split_part(str, delim, part_num) -> String` 🟡 | `split_part` | Name-only | `split_part` | Name-only | `split_part` (Spark 3.4+) 🟡 | Name-only | 1-indexed; returns empty string on out-of-range. Spark-version floor constraint per Q-FUNCS-MAP-020. Pre-Round-2 row; not in `14a §4.2`. |
| `position` | `position(substr, str) -> Long` 🟡 | `strpos(str, substr)` | Name-remap | `strpos(str, substr)` / `position(substr IN str)` | Name-remap | `locate(substr, str, [pos])` | Structural | ⚠️ Argument order and function name both diverge on Spark. Pre-Round-2 row; not in `14a §4.2`. See Q-FUNCS-MAP-001. |

Portability summary: 12 ratified canonical entries across the three engines. Two structural rewrites (DF / DuckDB `concat` → `||`-chain; Spark / DuckDB `lpad`/`rpad` 2-arg → 3-arg). All others Name-only or single Name-remap.

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

*Ratified Round-2 (2026-05-21). Canonical FunctionSpecs at `14a §4.3`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `abs` | `(Numeric) -> same` | `abs` | Name-only | `abs` | Name-only | `abs` | Name-only | Type-preserving across all engines. Signed-integer min overflow is engine-visible. |
| `round` | `(Float\|Decimal, [Integer]) -> same` | `round` | Name-only | `round` | Name-only | `round` | Name-only | Half-away-from-zero across all engines. Integer args rejected at compile (no native integer overload on DF). |
| `ceil` | `(Float) -> Float`, `(Decimal(p,s)) -> Decimal(p,0)` | `ceil` | Name-only | `ceil` | Name-only | `cast(ceil(x) as Double)` for Float; `ceil` for Decimal | Structural (Float) | Spark's `ceil(Float) -> BIGINT` requires cast wrap to keep canonical Float-in/Float-out shape. `ceiling` alias accepted by all engine parsers. |
| `floor` | `(Float) -> Float`, `(Decimal(p,s)) -> Decimal(p,0)` | `floor` | Name-only | `floor` | Name-only | `cast(floor(x) as Double)` for Float; `floor` for Decimal | Structural (Float) | Same shape as `ceil` mirrored. |
| `sqrt` | `(Float) -> Float` | `sqrt` | Name-only | `sqrt` | Name-only | `sqrt` | Name-only | Negative input is engine-visible (DuckDB errors, DF/Spark NaN). |
| `power` | `(Float, Float) -> Float` | `power` | Name-only | `power` | Name-only | `power` | Name-only | `pow` is adapter-side alias accepted by all parsers. |
| `exp` | `(Float) -> Float` | `exp` | Name-only | `exp` | Name-only | `exp` | Name-only | Universal. |
| `ln` | `(Float) -> Float` | `ln` | Name-only | `ln` | Name-only | `ln` | Name-only | Natural logarithm. Non-positive input engine-visible. |
| `log` | `(Float, Float) -> Float` (2-arg only) | `log` | Name-only | `log` | Name-only | `log` | Name-only | 2-arg form `log(base, value)` agreed across all three engines. **1-arg `log(x)` is NOT canonical** — DF/DuckDB = log10, Spark = ln; semantic divergence; authors use `ln(x)` or `log10(x)` explicitly. |
| `log10` | `(Float) -> Float` | `log10` | Name-only | `log10` | Name-only | `log10` | Name-only | Base-10 logarithm. |
| `sign` | `(Numeric) -> Integer` | `cast(signum(x) as Integer)` | Structural | `sign` (returns TINYINT, widens to Integer) | Name-only | `sign` | Name-only | DF only exposes `signum` (returns Float), so wrap with cast to canonical Integer. |

Portability summary: 11 ratified canonical entries. Three structural rewrites (DF `sign`→`signum`+cast; Spark `ceil(Float)`/`floor(Float)` cast wrap). Domain-error behavior on `sqrt`/`ln`/`log` for out-of-range inputs is engine-visible quirk, not gated.

### 8.1 Math functions explicitly excluded from canonical

| Name | Reason |
|---|---|
| `mod(a, b)` | Canonical form is `BinaryOpKind::Mod` (`%`) per `14 §3.2` / `14a §4.3`. The function-call form is an adapter-extended convenience on engines that prefer it (DuckDB, Spark). Legacy entry R11 superseded. |
| `trunc` / `truncate` | Legacy §12 notes universal name-only. Not in `14a §4.3` candidate list. **Canonical promotion candidate** — `TD-FUNCS-MAPPING-TRUNC-PROMOTE`. |

---

## 9. Temporal Functions (`14a §4.4`)

*Ratified Round-2 (2026-05-21). Canonical FunctionSpecs at `14a §4.4`. `DateTrunc` is a dedicated `Expr` variant (not a registry entry) — documented in §11. `EXTRACT(part FROM x)` is parser sugar that lowers to `date_part('part', x)` — `extract` is NOT a registry entry.*

Author-facing surface for every row below is `FunctionCall` (`14a §4.4`). Per-engine emission MAY rewrite to `BinaryOp` infix at the PlanBuilder layer (e.g. DF / Spark `date_add(d, i)` → `d + i`); that is a structural-rewrite-without-demotion, matching the `regexp_extract` precedent.

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `date_part` | `(String, Date\|Timestamp) -> Long` | `date_part(part, expr)` | Name-only | `date_part(part, expr)` | Name-only | `date_part(part, expr)` | Name-only | First arg is a part literal (`'year'`, `'month'`, `'day'`, `'hour'`, `'minute'`, `'second'`, `'millisecond'`, …). All three engines accept. SQL sugar `EXTRACT(part FROM x)` is parsed to canonical `date_part('part', x)`. |
| `year` | `(Date\|Timestamp) -> Long` | `date_part('year', expr)` (PlanBuilder rewrite) | Structural | `year(expr)` | Name-only | `year(expr)` | Name-only | DF lacks the 1-arg `year(x)` function; PlanBuilder rewrites to `date_part('year', x)`. |
| `month` | `(Date\|Timestamp) -> Long` | `date_part('month', expr)` (PlanBuilder rewrite) | Structural | `month(expr)` | Name-only | `month(expr)` | Name-only | Same DF rewrite pattern as `year`. |
| `day` | `(Date\|Timestamp) -> Long` | `date_part('day', expr)` (PlanBuilder rewrite) | Structural | `day(expr)` | Name-only | `day(expr)` / `dayofmonth(expr)` | Name-only | Same DF rewrite pattern. |
| `hour` | `(Timestamp) -> Long` | `date_part('hour', expr)` (PlanBuilder rewrite) | Structural | `hour(expr)` | Name-only | `hour(expr)` | Name-only | Same DF rewrite pattern. |
| `minute` | `(Timestamp) -> Long` | `date_part('minute', expr)` (PlanBuilder rewrite) | Structural | `minute(expr)` | Name-only | `minute(expr)` | Name-only | Same DF rewrite pattern. |
| `second` | `(Timestamp) -> Long` | `date_part('second', expr)` (PlanBuilder rewrite) | Structural | `second(expr)` | Name-only | `second(expr)` | Name-only | Same DF rewrite pattern. Integer seconds; sub-second extraction via `date_part('millisecond', …)`. |
| `current_date` | `() -> Date` | `current_date()` | Name-only | `current_date()` | Name-only | `current_date()` | Name-only | Canonical emission always uses paren form. Per-query determinism (sourced from session). |
| `current_timestamp` | `() -> Timestamp` | `current_timestamp()` | Name-only | `current_timestamp()` | Name-only | `current_timestamp()` | Name-only | Same paren-form rule. |
| `date_add` | `(Date\|Timestamp, Interval) -> same` | `BinaryOp(Add, d, i)` (PlanBuilder rewrite) | Structural | `date_add(d, i)` | Name-only | `BinaryOp(Add, d, i)` (PlanBuilder rewrite) | Structural | DF + Spark have no Interval-arg `date_add` function — `date_add(d, n)` on Spark is integer-days only. Both PlanBuilders rewrite the canonical FunctionCall to `d + i`. Spark's integer-days form is adapter-extended (`Spark.date_add(d: Date, n: Integer)`). |
| `date_sub` | `(Date\|Timestamp, Interval) -> same` | `BinaryOp(Subtract, d, i)` (PlanBuilder rewrite) | Structural | `date_sub(d, i)` | Name-only | `BinaryOp(Subtract, d, i)` (PlanBuilder rewrite) | Structural | Mirrors `date_add`. |
| `date_diff` | `(String, Date\|Timestamp, Date\|Timestamp) -> Long` | `(end - start) extracted via date_part(part, …)` (PlanBuilder structural) | Structural | `date_diff(part, start, end)` | Name-only | `'day'` part → `datediff(end, start)`; other parts → `date_part(part, end) - date_part(part, start)` (PlanBuilder structural) | Structural | Canonical = 3-arg `(part, start, end)`. Returns signed difference in `part` units (positive when `end > start`). DuckDB native; DF + Spark structural. The 2-arg integer-days form (Spark-style `datediff(end, start)`) is adapter-extended. |
| `to_date` | `(String) -> Date` | `to_date(str)` | Name-only | `Cast(str, Date)` (PlanBuilder rewrite) | Structural | `to_date(str)` | Name-only | ISO-8601 input only. DuckDB has no native `to_date()`; PlanBuilder casts string to Date. Format-string overload `(String, String)` is adapter-extended. |
| `to_timestamp` | `(String) -> Timestamp` | `to_timestamp(str)` | Name-only | `Cast(str, Timestamp)` (PlanBuilder rewrite) | Structural | `to_timestamp(str)` | Name-only | Same pattern as `to_date`. Format-string overload adapter-extended. |

Portability summary: 14 ratified canonical entries. Five DF structural rewrites for unary component shortcuts (`year`/`month`/`day`/`hour`/`minute`/`second` → `date_part`). Two structural rewrites for interval arithmetic on DF + Spark (`date_add`/`date_sub` → BinaryOp). Three-engine structural divergence on `date_diff` (DuckDB native; DF + Spark structural). DuckDB structural for `to_date`/`to_timestamp` (Cast).

### 9.1 Temporal forms explicitly excluded from canonical

| Name | Reason |
|---|---|
| `extract(part FROM x)` | Parser sugar — lowers to `date_part('part', x)` at parse time. Not a separate registry entry. |
| 2-arg `to_date(str, fmt)` / `to_timestamp(str, fmt)` | Format-string dialects diverge across engines (DF Chrono strftime, Spark Java DateTimeFormatter, DuckDB strptime syntax) and are mutually incompatible. Adapter-extended on each engine with engine-native format syntax. `TD-FUNCS-MAPPING-TO-DATE-FORMAT` / `TD-FUNCS-MAPPING-DATETIME-FORMATS`. |
| 2-arg `date_diff(start, end)` (integer-days) | Convenience form; canonical 3-arg `date_diff('day', start, end)` covers it. Adapter-extended on engines that prefer the shorter signature (Spark `datediff`). `TD-FUNCS-MAPPING-DATE-DIFF-2ARG`. |
| `date_add(d, n)` (Spark integer-days form) | Spark-specific integer-days variant — adapter-extended only. Canonical uses Interval second arg. |

---

## 10. Logical / Conditional Helpers (`14a §4.5`)

*Ratified Round-2 (2026-05-21). Canonical FunctionSpecs at `14a §4.5`.*

| Canonical | Signature | DataFusion | Tier | DuckDB | Tier | Spark | Tier | Notes |
|---|---|---|---|---|---|---|---|---|
| `greatest` | variadic `(T, T...) -> T` | `greatest(x, ...)` | Name-only | `greatest(x, ...)` | Name-only | `greatest(x, ...)` | Name-only | NULL-skip across all three: returns greatest non-NULL value; NULL only when every arg is NULL. Earlier `TD-FUNCS-MAPPING-GREATEST-LEAST-NULL` (DF/Spark "propagate" vs DuckDB "ignore") was a Round-1 misclassification — closed. |
| `least` | variadic `(T, T...) -> T` | `least(x, ...)` | Name-only | `least(x, ...)` | Name-only | `least(x, ...)` | Name-only | Mirror of `greatest`. Same NULL-skip semantics across all three. |

Portability summary: **Universal** Name-only across all three engines.

### 10.1 Forms explicitly excluded from canonical

| Form | Reason |
|---|---|
| `if(cond, then, else)` | `Expr::Case` (dedicated variant per `14 §3.3`) covers the use-case directly. Authors write `Case { when: [{cond, then}], else_expr: Some(else) }`. Spark / DuckDB native `if(...)` and DF's missing-`if` are all served by `Case` rendering. Not registered to keep the surface minimal. |
| `ifnull(a, b)` / `nvl(a, b)` | `Expr::Coalesce` (dedicated variant) covers both. Authors write `Coalesce([a, b])`. All three adapters render `Coalesce` natively (universal); the DuckDB-lacks-`nvl` and DF-`nvl-as-alias-of-ifnull` quirks disappear at the Coalesce layer. |

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
| `percentile_disc` | `percentile_disc(p) WITHIN GROUP (ORDER BY col) -> same` | Discrete percentile (returns actual data point). DataFusion lacks an equivalent — adapter-extended only. | DuckDB built-in. |

### 12.3 Spark-extended

| Name | Signature | Purpose | Source |
|---|---|---|---|
| `collect_set` | `collect_set(expr) -> Array<T>` | Aggregate to distinct array. `FunctionCategory::Aggregate`. | Spark built-in. |
| `collect_list` | `collect_list(expr) -> Array<T>` | Aggregate to array (preserving duplicates). `FunctionCategory::Aggregate`. | Spark built-in. |
| `array_join` | `array_join(array, sep, [null_replacement]) -> String` | Array join. | Spark built-in. |
| `percentile_approx` | `percentile_approx(expr, frac) -> same` | Approximate percentile. | Spark built-in. |
| `percentile_disc` | `percentile_disc(p) WITHIN GROUP (ORDER BY col) -> same` (Spark 3.1+) | Discrete percentile. DataFusion lacks an equivalent — adapter-extended only. | Spark built-in. |
| `try_divide` | `try_divide(a, b) -> Double` (Spark 3.3+) 🟡 | NULL-on-zero-divisor safe division. See Q-FUNCS-MAP-017. | Spark built-in. |
| `pmod` | `pmod(a, b) -> same` | Positive modulo. | Spark built-in. |
| `startswith` / `endswith` | Spark native names for `starts_with` / `ends_with`. | — | Spark built-in; adapter name-remaps to canonical if promoted (§7.1). |

### 12.4 Cross-engine adapter-extended (legacy demotions)

These were canonical in legacy `FUNCTION_CATALOG.md` but failed `14a` Q10 intersection; they remain usable under per-adapter registration but are NOT canonical.

| Name | Rationale for demotion | Per-adapter disposition | TD |
|---|---|---|---|
| `initcap` | No DuckDB native. | DataFusion + Spark adapter-extended. Author using DuckDB must emulate or omit. | `TD-FUNCS-MAPPING-INITCAP` |
| `percentile_disc` | DataFusion has no equivalent. | DuckDB + Spark adapter-extended (Spark 3.1+). DF authors use `percentile_cont` or `approx_percentile_cont`. | `TD-FUNCS-MAPPING-PERCENTILE` |
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
| `FunctionCall("trim", [s, ...])` | `FunctionCall("btrim", [s, ...])` | Name-remap | §7 |
| `FunctionCall("length", [arr])` (Array arg) | `FunctionCall("array_length", [arr])` | Name-remap | §7 |
| `FunctionCall("reverse", [arr])` (Array arg) | `FunctionCall("array_reverse", [arr])` | Name-remap | §7 |
| `FunctionCall("concat", [a, b, c, ...])` | `BinaryOp(Concat, ... a || b || c ...)` | Structural | §7 |
| `FunctionCall("position", [s, t])` | `FunctionCall("strpos", [t, s])` | Name-remap + arg-reorder | §7 |
| `FunctionCall("sign", [x])` | `Cast(FunctionCall("signum", [x]), Integer)` | Structural (name-remap + cast to canonical Integer) | §8 |
| `FunctionCall("variance", [x])` | `FunctionCall("var_samp", [x])` | Name-remap | §3.2 |
| `FunctionCall("approx_count_distinct", [x])` | `FunctionCall("approx_distinct", [x])` | Name-remap | §3.2 |
| `FunctionCall("year", [x])` | `FunctionCall("date_part", [Literal("year"), x])` | Structural | §9 |
| `FunctionCall("month", [x])` | `FunctionCall("date_part", [Literal("month"), x])` | Structural | §9 |
| `FunctionCall("day", [x])` | `FunctionCall("date_part", [Literal("day"), x])` | Structural | §9 |
| `FunctionCall("hour", [x])` | `FunctionCall("date_part", [Literal("hour"), x])` | Structural | §9 |
| `FunctionCall("minute", [x])` | `FunctionCall("date_part", [Literal("minute"), x])` | Structural | §9 |
| `FunctionCall("second", [x])` | `FunctionCall("date_part", [Literal("second"), x])` | Structural | §9 |
| `FunctionCall("date_add", [d, i])` | `BinaryOp(Add, d, i)` | Structural | §9 |
| `FunctionCall("date_sub", [d, i])` | `BinaryOp(Subtract, d, i)` | Structural | §9 |
| `FunctionCall("date_diff", [Literal(part), start, end])` | `Cast(FunctionCall("date_part", [Literal(part), BinaryOp(Subtract, end, start)]), Long)` (`'day'` part) / part-wise extraction for non-day parts | Structural | §9 |
| `Expr::RegexpExtract { expr, pattern, group }` | `FunctionCall("array_element", [FunctionCall("regexp_match", [expr, pattern]), group + 1])` | Structural | §11 |

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
| `FunctionCall("percentile_cont", [p, col])` | `percentile_cont(p) WITHIN GROUP (ORDER BY col)` | aggregate-render path (`WITHIN GROUP` form) |

### 13.2 DuckDB

**PlanBuilder-layer rewrites:**

| Source | Target | Tier | §ref |
|---|---|---|---|
| `FunctionCall("concat", [a, b, c, ...])` | `BinaryOp(Concat, ... a || b || c ...)` | Structural | §7 |
| `FunctionCall("lpad", [s, n])` (2-arg form) | `FunctionCall("lpad", [s, n, Literal(" ")])` | Structural | §7 |
| `FunctionCall("rpad", [s, n])` (2-arg form) | `FunctionCall("rpad", [s, n, Literal(" ")])` | Structural | §7 |
| `FunctionCall("position", [s, t])` | `FunctionCall("strpos", [t, s])` | Name-remap + arg-reorder | §7 |
| `FunctionCall("to_date", [s])` | `Cast(s, Date)` | Structural | §9 |
| `FunctionCall("to_timestamp", [s])` | `Cast(s, Timestamp)` | Structural | §9 |
| `FunctionCall("log", [base, x])` | *(left as-is if DuckDB supports 2-arg form; else arity mismatch)* 🟡 | — | §8 |

**Dialect-layer rendering:**

| Variant | SQL | Method |
|---|---|---|
| `Expr::Like` | `expr LIKE pattern` | default |
| `Expr::ILike` | `expr ILIKE pattern` | `ilike` |
| `Expr::RegexpMatch` | `regexp_matches(expr, pattern)` (partial) / `regexp_matches(expr, CONCAT('^', pattern, '$'))` (full) | `regexp_match` |
| `Expr::RegexpExtract` | `regexp_extract(expr, pattern, group)` | `regexp_extract` |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc` |
| `Expr::Cast` | `CAST(expr AS <type>)` | `type_name` |
| `FunctionCall("percentile_cont", [p, col])` | `percentile_cont(p) WITHIN GROUP (ORDER BY col)` | aggregate-render path |

### 13.3 Spark

**PlanBuilder-layer rewrites:**

| Source | Target | Tier | §ref |
|---|---|---|---|
| `FunctionCall("length", [arr])` (Array arg) | `FunctionCall("size", [arr])` | Name-remap | §7 |
| `FunctionCall("lpad", [s, n])` (2-arg form) | `FunctionCall("lpad", [s, n, Literal(" ")])` | Structural | §7 |
| `FunctionCall("rpad", [s, n])` (2-arg form) | `FunctionCall("rpad", [s, n, Literal(" ")])` | Structural | §7 |
| `FunctionCall("position", [s, t])` | `FunctionCall("locate", [s, t])` 🟡 | Name-remap (arg order matches canonical) | §7 |
| `FunctionCall("starts_with", [s, p])` | `FunctionCall("startswith", [s, p])` 🟡 | Name-remap (underscore strip) | §7.1 |
| `FunctionCall("ends_with", [s, p])` | `FunctionCall("endswith", [s, p])` 🟡 | Name-remap | §7.1 |
| `FunctionCall("ceil", [x])` (Float arg) | `Cast(FunctionCall("ceil", [x]), Double)` | Structural (cast wrap — Spark's `ceil(Float)` returns BIGINT) | §8 |
| `FunctionCall("floor", [x])` (Float arg) | `Cast(FunctionCall("floor", [x]), Double)` | Structural (cast wrap) | §8 |
| `FunctionCall("date_add", [d, i])` | `BinaryOp(Add, d, i)` | Structural | §9 |
| `FunctionCall("date_sub", [d, i])` | `BinaryOp(Subtract, d, i)` | Structural | §9 |
| `FunctionCall("date_diff", [Literal("day"), start, end])` | `FunctionCall("datediff", [end, start])` | Structural (name-remap + arg-reorder, day part only) | §9 |
| `FunctionCall("date_diff", [Literal(part), start, end])` (non-day part) | `BinaryOp(Subtract, FunctionCall("date_part", [Literal(part), end]), FunctionCall("date_part", [Literal(part), start]))` | Structural (per-part extraction) | §9 |
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
| `FunctionCall("percentile_cont", [p, col])` | `percentile_cont(p) WITHIN GROUP (ORDER BY col)` | aggregate-render path (Spark 3.1+) |

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
| `TD-FUNCS-MAPPING-PERCENTILE` | `percentile_cont` / `percentile_disc` | — / DataFusion (`percentile_disc` absent) | Resolved 2026-05-21. | **CLOSED for `percentile_cont`** — Spark 3.1+ has native exact form; all three engines converge on `WITHIN GROUP` SQL-standard syntax (dialect-layer rendering, not a structural rewrite). `percentile_disc` demoted to adapter-extended (DuckDB + Spark only) per §12.4. |
| `TD-FUNCS-MAPPING-TO-DATE-FORMAT` | 2-arg `to_date(str, fmt)` / `to_timestamp(str, fmt)` | All three (mutually incompatible format-string dialects) | Format-string syntax divergence (DF Chrono strftime / Spark Java DateTimeFormatter / DuckDB strptime). | Adapter-extended on each engine with engine-native format syntax. Canonical 1-arg ISO-only form is ratified. |
| `TD-FUNCS-MAPPING-IF-IFNULL-NVL` | `if` / `ifnull` / `nvl` | — | Overlap with `Case` / `Coalesce` dedicated variants. | **CLOSED 2026-05-21.** Not registered; authors write `Expr::Case` / `Expr::Coalesce` directly. Dedicated variants render natively on all three engines. |
| `TD-FUNCS-MAPPING-DATE-ADD-SPARK` | `date_add(date, interval)` | Spark (only integer-days form native) | Arg-type divergence. | Already structurally rewritten via `date + interval` at Spark adapter's PlanBuilder. Non-blocking. Spark integer-days form `date_add(d: Date, n: Integer)` is adapter-extended. |
| `TD-FUNCS-MAPPING-DATE-DIFF-2ARG` | 2-arg `date_diff(start, end)` integer-days form | — | Convenience overload; canonical 3-arg `date_diff('day', start, end)` covers the use-case. | Adapter-extended on engines that prefer the shorter signature (Spark `datediff`, DuckDB legacy form). |
| `TD-FUNCS-MAPPING-LOG-ARITY` | `log(base, x)` | DuckDB (1-arg base-10 only) | Arity / semantic divergence. | DuckDB adds 2-arg form, or canonical splits into `log` (1-arg base-10 only) + `logb` (2-arg). |
| `TD-FUNCS-MAPPING-SAFEDIVIDE-SPARK` | `SafeDivide` Spark rendering | — | Optimization opportunity, not a gap. | Spark 3.3+ adapter may emit `try_divide`. |
| `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` | BinaryOp promotion tables §5.2–§5.3 | All three | Rows drafted from docs, not empirically verified. | Test harness against live adapter instances. |
| `TD-FUNCS-MAPPING-DECIMAL-DIV` | `Decimal / Decimal` result type | DuckDB (divergent from DF / Spark) | Result-type divergence. | Reconciliation Cast at Semantics boundary. |
| `TD-FUNCS-MAPPING-DATE-SUB-DATE` | `Date - Date` result type | DataFusion (returns `Interval`) vs DuckDB / Spark (return `Integer` days) | Result-type divergence. | Document as expected; author declares `data_type:` appropriately. |
| `TD-FUNCS-MAPPING-AGG-INTERSECTION` | Non-closed aggregates §3.2 | — | Resolved 2026-05-21. | **CLOSED** — Round-2 intersection ratified 8 canonical entries (`stddev`, `stddev_pop`, `variance`, `var_pop`, `median`, `string_agg`, `percentile_cont`, `approx_count_distinct`); `percentile_disc` adapter-extended only. |
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
- **`questions/open/functions_mapping_questions.md`** — parked unresolved questions surfaced by Round-2 drafting.

---

## 18. TECH_DEBT Index

Consolidated list of all `TD-FUNCS-MAPPING-*` entries emitted by this doc. Each maps back to its originating §.

| TD ID | § | Current posture |
|---|---|---|
| `TD-FUNCS-MAPPING-INITCAP` | 14 | Demoted to adapter-extended (DF + Spark only). |
| `TD-FUNCS-MAPPING-PERCENTILE` | 14 | `percentile_cont` ratified canonical (dialect-layer `WITHIN GROUP`); `percentile_disc` adapter-extended. |
| `TD-FUNCS-MAPPING-TO-DATE-FORMAT` | 14 | 2-arg forms demoted. |
| `TD-FUNCS-MAPPING-IF-IFNULL-NVL` | 14 | **CLOSED 2026-05-21** — `if` / `ifnull` / `nvl` not registered; authors use `Case` / `Coalesce`. |
| `TD-FUNCS-MAPPING-DATE-ADD-SPARK` | 14 | Structural rewrite in place; integer-days form adapter-extended. |
| `TD-FUNCS-MAPPING-DATE-DIFF-2ARG` | 14 | Canonical = 3-arg; 2-arg form adapter-extended. |
| `TD-FUNCS-MAPPING-LOG-ARITY` | 14 | Open — requires `14a` Round-2 clarification. |
| `TD-FUNCS-MAPPING-SAFEDIVIDE-SPARK` | 14 | Optimization; non-blocking. |
| `TD-FUNCS-MAPPING-BINOP-EMPIRICAL` | 14 | Blocked on test harness. |
| `TD-FUNCS-MAPPING-DECIMAL-DIV` | 14 | Reconciliation Cast at Semantics boundary. |
| `TD-FUNCS-MAPPING-DATE-SUB-DATE` | 14 | Documented as expected divergence. |
| `TD-FUNCS-MAPPING-AGG-INTERSECTION` | 14 | **CLOSED 2026-05-21** — 8 canonical entries ratified at `14a §4.6`. |
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
