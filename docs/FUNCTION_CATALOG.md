# Function Catalog — Canonical IR to Engine Mapping

**Version:** 2.0 | **Status:** Reference specification for expression rewriting design

---

## 1. Overview

Semstrait defines a **canonical function set** in the IR layer. Each canonical function has a documented mapping to engine-specific equivalents. Engines that differ from the canonical form require expression rewriting in the adapter layer, injected via `PlanBuilder::rewrite_expr()` during plan construction.

### Canonical Selection Rule

The canonical function name and signature is chosen as the **most popular form across DataFusion, DuckDB, and Spark**. When all three engines agree, that is the canonical form. When they diverge, the 2-of-3 majority wins. Ties are broken by ANSI SQL precedence.

### Rewriting Tiers

| Tier | Description | Example |
|------|-------------|---------|
| **Name-only** | Engine uses the same function name and semantics | `upper` -> `upper` (all engines) |
| **Name-remap** | Engine uses a different name, same semantics and signature | `position` -> `strpos` (DataFusion) |
| **Structural** | Engine needs expression tree transformation | `date_add(d, i)` -> `d + i` (DataFusion interval arithmetic) |
| **Unsupported** | Engine has no equivalent — produces error | (none currently) |

### Architecture

```
semstrait-ir/src/rewrite.rs:
  - CanonicalFn enum (38 variants — identifies each canonical FunctionCall)
  - FunctionTarget enum (SameName | Rename | Rewrite | Unsupported)
  - FunctionRewriter struct (HashMap<CanonicalFn, FunctionTarget>)

semstrait-ir/src/plan_builder.rs:
  - PlanBuilder::rewrite_expr(&self, expr: Expr) -> Expr (default: identity)

semstrait-adapter/src/engines/datafusion/plan_builder.rs:
  - DataFusionPlanBuilder (impl PlanBuilder with rewrite_expr override)

semstrait-adapter/src/sql/dialect.rs:
  - SqlDialect trait (regexp_match, regexp_extract, ilike, date_trunc, etc.)
  - Per-engine dialect structs (DataFusionDialect, DuckDbDialect, SparkDialect)

semstrait-adapter/src/sql/expr_renderer.rs:
  - ExprSqlRenderer — renders Expr tree to SQL via dialect
```

### Two Rewrite Layers

1. **PlanBuilder layer** (`rewrite_expr`): Transforms `Expr` tree during plan construction. Handles `FunctionCall` nodes via `FunctionRewriter` HashMap, and dedicated `Expr` variants (`RegexpExtract`, etc.) via pattern matching. Runs before SQL emission.

2. **Dialect layer** (`SqlDialect` trait): Generates engine-specific SQL syntax during emission. Handles dedicated `Expr` variants that have dialect methods (`regexp_match`, `regexp_extract`, `ilike`, `date_trunc`). Runs during SQL rendering.

**Rule:** Each function is handled at exactly one layer. `FunctionCall` nodes go through PlanBuilder rewrite. Dedicated `Expr` variants go through Dialect rendering. The only exception is when a PlanBuilder rewrite converts a dedicated variant into a `FunctionCall` (e.g., DataFusion `RegexpExtract` -> `FunctionCall("regexp_match")`).

---

## 2. Aggregate Functions

All aggregate functions are universal across all three engines. No rewriting needed.

| ID | Canonical | Signature | DataFusion | DuckDB | Spark | Returns |
|----|-----------|-----------|------------|--------|-------|---------|
| A1 | `sum` | `sum(expr)` | `sum` | `sum` | `sum` | SameAsInput |
| A2 | `avg` | `avg(expr)` | `avg` | `avg` | `avg` | Number |
| A3 | `count` | `count([expr])` | `count` | `count` | `count` | Integer |
| A4 | `count_distinct` | `count(DISTINCT expr)` | `count(DISTINCT)` | `count(DISTINCT)` | `count(DISTINCT)` | Integer |
| A5 | `min` | `min(expr)` | `min` | `min` | `min` | SameAsInput |
| A6 | `max` | `max(expr)` | `max` | `max` | `max` | SameAsInput |

**IR representation:** `Expr::Aggregate(AggregateExpr)` — dedicated variant, NOT `FunctionCall`. Not handled by `FunctionRewriter`.

---

## 3. Comparison Operators

Binary operators in the IR (`Expr::BinaryOp`). Universal across all engines.

| ID | Canonical | SQL | IR Repr |
|----|-----------|-----|---------|
| C1 | `equal` | `=` | `BinaryOp::Eq` |
| C2 | `not_equal` | `<>` / `!=` | `BinaryOp::NotEq` |
| C3 | `lt` | `<` | `BinaryOp::Lt` |
| C4 | `lte` | `<=` | `BinaryOp::LtEq` |
| C5 | `gt` | `>` | `BinaryOp::Gt` |
| C6 | `gte` | `>=` | `BinaryOp::GtEq` |

---

## 4. Boolean / Logic

| ID | Canonical | SQL | IR Repr |
|----|-----------|-----|---------|
| L1 | `and` | `AND` | `BinaryOp::And` |
| L2 | `or` | `OR` | `BinaryOp::Or` |
| L3 | `not` | `NOT expr` | `Expr::Not` |
| L4 | `is_null` | `expr IS NULL` | `Expr::IsNull` |
| L5 | `is_not_null` | `expr IS NOT NULL` | `Expr::IsNotNull` |

---

## 5. Conditional / Null-Handling

| ID | Canonical | Signature | IR Repr | Returns |
|----|-----------|-----------|---------|---------|
| N1 | `coalesce` | `coalesce(expr1, expr2, ...)` | `Expr::Coalesce` | SameAsInput |
| N2 | `nullif` | `nullif(expr1, expr2)` | `Expr::NullIf` | SameAsInput |
| N3 | `greatest` | `greatest(expr1, expr2, ...)` | `FunctionCall` | SameAsInput |
| N4 | `least` | `least(expr1, expr2, ...)` | `FunctionCall` | SameAsInput |
| N5 | `case/when` | `CASE WHEN cond THEN val [ELSE val] END` | `Expr::Case` | varies |
| N6 | `in` | `expr IN (val1, val2, ...)` | `Expr::InList` | Boolean |
| N7 | `between` | `expr BETWEEN low AND high` | `Expr::Between` | Boolean |

All three engines support these identically. No rewriting needed. `N1` and `N2` are dedicated `Expr` variants (rendered by `ExprSqlRenderer`). `N3` and `N4` are `FunctionCall` nodes (name-only).

---

## 6. Arithmetic

| ID | Canonical | Signature | IR Repr | Returns |
|----|-----------|-----------|---------|---------|
| R1 | `add` | `a + b` | `BinaryOp::Add` | SameAsInput |
| R2 | `subtract` | `a - b` | `BinaryOp::Subtract` | SameAsInput |
| R3 | `multiply` | `a * b` | `BinaryOp::Multiply` | SameAsInput |
| R4 | `divide` | `a / b` | `BinaryOp::Divide` | Number |
| R5 | `abs` | `abs(expr)` | `FunctionCall` | SameAsInput |
| R6 | `ceil` | `ceil(expr)` | `FunctionCall` | SameAsInput |
| R7 | `floor` | `floor(expr)` | `FunctionCall` | SameAsInput |
| R8 | `round` | `round(expr [, decimal_places])` | `FunctionCall` | SameAsInput |
| R9 | `power` | `power(base, exponent)` | `FunctionCall` | Number |
| R10 | `sqrt` | `sqrt(expr)` | `FunctionCall` | Number |
| R11 | `mod` | `mod(a, b)` | `FunctionCall` | SameAsInput |

### R5-R10: Name-only across all engines

All three engines use identical names and semantics. DuckDB also accepts `pow` as alias for `power`, and `ceiling` as alias for `ceil`. Spark also accepts `pow` and `ceiling`. No rewriting needed.

### R8 `round` — Signature Detail

| Engine | Signature | Behavior |
|--------|-----------|----------|
| DataFusion | `round(expr [, decimal_places])` | `decimal_places` defaults to 0 |
| DuckDB | `round(expr, precision)` | `precision` required in some contexts |
| Spark | `round(expr [, d])` | `d` defaults to 0 |

### R11 `mod` — Rewrite Detail

| Engine | Rewrite | Detail |
|--------|---------|--------|
| DataFusion | **Structural** | `mod(a, b)` -> `a - floor(a / b) * b`. DF has `%` operator in SQL but no `mod()` function in Substrait. |
| DuckDB | Name-only | `mod(a, b)` natively supported. Also `a % b`. |
| Spark | Name-only | `mod(a, b)` natively supported. Also `pmod(a, b)` for positive modulo. |

---

## 7. String Functions

### Universal (Name-only on all engines)

| ID | Canonical | Signature | Behavior | Returns |
|----|-----------|-----------|----------|---------|
| S1 | `upper` | `upper(str)` | Converts string to uppercase | String |
| S2 | `lower` | `lower(str)` | Converts string to lowercase | String |
| S3 | `concat` | `concat(str1, str2, ...)` | Concatenates variadic strings. NULLs treated as empty strings. | String |
| S5 | `length` | `length(str)` | Returns character count (not bytes) | Integer |
| S6 | `trim` | `trim(str)` | Removes leading and trailing whitespace | String |
| S7 | `ltrim` | `ltrim(str)` | Removes leading whitespace | String |
| S8 | `rtrim` | `rtrim(str)` | Removes trailing whitespace | String |
| S9 | `replace` | `replace(str, from, to)` | Replaces all occurrences of `from` with `to` | String |
| S11 | `left` | `left(str, n)` | Returns leftmost `n` characters | String |
| S12 | `right` | `right(str, n)` | Returns rightmost `n` characters | String |
| S13 | `lpad` | `lpad(str, len [, pad])` | Left-pads to `len` with `pad` (default space) | String |
| S14 | `rpad` | `rpad(str, len [, pad])` | Right-pads to `len` with `pad` (default space) | String |
| S19 | `reverse` | `reverse(str)` | Reverses the string | String |
| S20 | `repeat` | `repeat(str, n)` | Repeats string `n` times | String |

**Aliases recognized by `CanonicalFn::from_name`:** `ucase` -> `upper`, `lcase` -> `lower`, `len` -> `length`, `char_length` -> `length`.

### Functions with Engine Variations

| ID | Canonical | Signature | DataFusion | DuckDB | Spark | Tier |
|----|-----------|-----------|------------|--------|-------|------|
| S4 | `concat_ws` | `concat_ws(sep, str1, str2, ...)` | `concat_ws` | `concat_ws` | `concat_ws` | Name-only |
| S10 | `substring` | `substring(str, start [, len])` | `substr` / `substring` | `substring` / `substr` | `substr` / `substring` | Name-only |
| S15 | `split_part` | `split_part(str, delim, part)` | `split_part` | `split_part` | `split_part` | Name-only |
| S16 | `starts_with` | `starts_with(str, prefix)` | `starts_with` | `starts_with` / `prefix` | `startswith` (no underscore) | Name-remap (Spark) |
| S17 | `ends_with` | `ends_with(str, suffix)` | `ends_with` | `ends_with` / `suffix` | `endswith` (no underscore) | Name-remap (Spark) |
| S18 | `initcap` | `initcap(str)` | `initcap` | N/A (no equivalent) | `initcap` | Name-only (DuckDB unsupported) |
| S21 | `position` | `position(substr, str)` | `strpos(str, substr)` | `strpos(str, substr)` / `position` | `locate(substr, str [, pos])` | Name-remap (DF) |

### S4 `concat_ws` — Behavior Detail

Concatenates strings with a separator, skipping NULLs.

| Engine | Signature | NULL handling |
|--------|-----------|---------------|
| DataFusion | `concat_ws(sep, str1, str2, ...)` | Skips NULLs; returns NULL only if separator is NULL |
| DuckDB | `concat_ws(sep, str1, str2, ...)` | Skips NULLs |
| Spark | `concat_ws(sep, str1, str2, ...)` | Skips NULLs |

### S10 `substring` — Behavior Detail

Extracts substring starting at `start` position (1-indexed) for `len` characters.

| Engine | Signature | Notes |
|--------|-----------|-------|
| DataFusion | `substr(str, start [, len])` or `substring(str FROM start [FOR len])` | Both forms supported; 1-indexed |
| DuckDB | `substring(str, start, len)` or `substr(str, start, len)` | 1-indexed |
| Spark | `substring(str, pos [, len])` or `substr(str, pos [, len])` | 1-indexed; `pos` can be negative (count from end) |

### S15 `split_part` — Behavior Detail

Splits string by delimiter and returns the Nth part (1-indexed).

| Engine | Signature | Notes |
|--------|-----------|-------|
| DataFusion | `split_part(str, delimiter, part_num)` | Part 1 = first segment. Returns empty string for out-of-range. |
| DuckDB | `split_part(str, delimiter, part_num)` | Same semantics |
| Spark | `split_part(str, delimiter, part_num)` | Same semantics (Spark 3.4+) |

### S16 `starts_with` / S17 `ends_with` — Rewrite Detail

| Engine | `starts_with` | `ends_with` |
|--------|---------------|-------------|
| DataFusion | `starts_with(str, prefix)` | `ends_with(str, suffix)` |
| DuckDB | `starts_with(str, prefix)` / `prefix(str, prefix)` | `ends_with(str, suffix)` / `suffix(str, suffix)` |
| Spark | `startswith(str, prefix)` (no underscore) | `endswith(str, suffix)` (no underscore) |

Spark adapter: name-remap `starts_with` -> `startswith`, `ends_with` -> `endswith`.

### S21 `position` — Rewrite Detail

Returns the 1-based index of the first occurrence of `substr` in `str`, or 0 if not found.

| Engine | Function | Signature | Canonical arg order |
|--------|----------|-----------|---------------------|
| DataFusion | `strpos` | `strpos(string, substring)` | Same (string, substring) |
| DuckDB | `strpos` / `position` | `strpos(string, substring)` or `position(substr IN string)` | Same |
| Spark | `locate` | `locate(substring, string [, pos])` | **Reversed** (substring, string) |

**Current rewrite:** DataFusion — name-remap `position` -> `strpos`.
**Spark future:** structural rewrite to swap argument order when using `locate`.

---

## 8. Pattern Matching

| ID | Canonical | Signature | IR Repr | Returns |
|----|-----------|-----------|---------|---------|
| P1 | `like` | `expr LIKE pattern` | `Expr::Like` | Boolean |
| P2 | `ilike` | `expr ILIKE pattern` (case-insensitive) | `Expr::ILike` | Boolean |
| P3 | `regexp_like` | `regexp_like(str, pattern [, flags])` | `Expr::RegexpMatch` | Boolean |
| P4 | `regexp_extract` | `regexp_extract(str, pattern [, group_idx])` | `Expr::RegexpExtract` | String |
| P5 | `regexp_replace` | `regexp_replace(str, pattern, replacement [, flags])` | `FunctionCall` | String |

### P1 `like` — Dialect Rendering

All engines support `LIKE` natively. Rendered by `ExprSqlRenderer` -> `SqlDialect` (no special method, emitted as `expr LIKE pattern`).

### P2 `ilike` — Dialect Rendering

Case-insensitive LIKE. Rendered via `SqlDialect::ilike()`.

| Engine | SQL Output | Notes |
|--------|------------|-------|
| DataFusion | `expr ILIKE pattern` | Native support |
| DuckDB | `expr ILIKE pattern` | Native support |
| Spark | `LOWER(expr) LIKE LOWER(pattern)` | No native ILIKE; lowercases both sides |
| ANSI | `LOWER(expr) LIKE LOWER(pattern)` | Fallback |

### P3 `regexp_like` (Boolean Regex Match) — DETAILED

**Canonical behavior:** Returns TRUE if the string matches the regular expression pattern (partial/substring match by default).

| Engine | Function | Signature | Return Type | Match Semantics |
|--------|----------|-----------|-------------|-----------------|
| **DataFusion** | `regexp_like` | `regexp_like(str, regexp [, flags])` | `Boolean` | Partial match (anchored with `^...$` for full) |
| **DuckDB** | `regexp_matches` | `regexp_matches(str, pattern [, options])` | `Boolean` | Partial match |
| **Spark** | `regexp_like` | `regexp_like(str, pattern)` | `Boolean` | Partial match. Aliases: `rlike`, `regexp` (without flags param) |

**IR variant:** `Expr::RegexpMatch` with `full_match: bool` field. When `full_match = true`, dialect wraps pattern with `^` and `$` anchors.

**Dialect rendering (`SqlDialect::regexp_match`):**

| Engine | `full_match = false` | `full_match = true` |
|--------|----------------------|---------------------|
| DataFusion | `regexp_like(expr, pattern)` | `regexp_like(expr, CONCAT('^', pattern, '$'))` |
| DuckDB | `regexp_matches(expr, pattern)` | `regexp_matches(expr, CONCAT('^', pattern, '$'))` |
| Spark | `expr RLIKE CONCAT('.*', pattern, '.*')` | `expr RLIKE pattern` (Spark RLIKE is full-match by default) |
| ANSI | `REGEXP_LIKE(expr, pattern)` | `REGEXP_LIKE(expr, CONCAT('^', pattern, '$'))` |

DataFusion dialect correctly emits `regexp_like(expr, pattern)` which returns `Boolean` directly.

### P4 `regexp_extract` (Capture Group Extraction) — DETAILED

**Canonical behavior:** Extracts a specific capture group from the first regex match. Returns a scalar string.

| Engine | Function | Signature | Return Type | Default group_idx |
|--------|----------|-----------|-------------|-------------------|
| **DataFusion** | `regexp_match` | `regexp_match(str, regexp [, flags])` | `List<Utf8>` (array of ALL capture groups) | N/A (returns array) |
| **DuckDB** | `regexp_extract` | `regexp_extract(str, pattern [, group_idx [, options]])` | `VARCHAR` (scalar) | 0 = entire match |
| **Spark** | `regexp_extract` | `regexp_extract(str, regexp [, idx])` | `STRING` (scalar) | 1 = first capture group |

**Critical engine differences:**

1. **DataFusion** has no `regexp_extract`. Uses `regexp_match` which returns an array of all capture groups. To get a scalar string, the result is the array itself (name-remap only — confirmed by user).

2. **DuckDB** default `group_idx=0` means **entire match**; `1` = first capture group.

3. **Spark** default `idx=1` means **first capture group**; `0` = entire match. This is the opposite default from DuckDB.

**Canonical group_idx convention:** Follows DuckDB/Spark semantics — `0` = entire match, `1+` = capture groups. The IR `RegexpExtractExpr.group_idx` field uses this convention.

**Rewriting:**

| Engine | Rewrite Layer | Detail |
|--------|---------------|--------|
| DataFusion | PlanBuilder | `Expr::RegexpExtract` -> `FunctionCall("array_element", [FunctionCall("regexp_match", [expr, pattern]), group_idx + 1])`. DataFusion's `regexp_match` returns `List<Utf8>`; `array_element` extracts a single group as scalar string (1-based index). |
| DuckDB | Dialect | `SqlDialect::regexp_extract(expr, pattern, group_idx)` emits `regexp_extract(expr, pattern, group_idx)` |
| Spark | Dialect | `SqlDialect::regexp_extract(expr, pattern, group_idx)` emits `regexp_extract(expr, pattern, group_idx)` |
| ANSI | Dialect | `REGEXP_EXTRACT(expr, pattern, group_idx)` |

**DataFusion rewrite rationale:** DataFusion's `regexp_match` returns `List<Utf8>` (array of all capture groups), not a scalar string. To extract a single group, the result is wrapped with `array_element(regexp_match(expr, pattern), group_idx + 1)` where `array_element` is 1-based (canonical `group_idx` 0 = entire match maps to index 1).

### P5 `regexp_replace` — Detail

Replaces occurrences of a regex pattern with a replacement string.

| Engine | Function | Signature | Notes |
|--------|----------|-----------|-------|
| DataFusion | `regexp_replace` | `regexp_replace(str, regexp, replacement [, flags])` | `flags`: `g` for global, `i` for case-insensitive |
| DuckDB | `regexp_replace` | `regexp_replace(str, pattern, replacement [, options])` | `options`: `g` for global |
| Spark | `regexp_replace` | `regexp_replace(str, regexp, rep [, position])` | 4th arg is start position (1-indexed), NOT flags |

**Canonical:** `regexp_replace(str, pattern, replacement [, flags])`. Name-only for all engines. Spark's 4th parameter has different semantics (position vs flags) — for V1, we only use 3-arg form.

---

## 9. Date/Time Functions

| ID | Canonical | Signature | IR Repr | Returns |
|----|-----------|-----------|---------|---------|
| D1 | `date_trunc` | `date_trunc(grain, expr)` | `Expr::DateTrunc` | Date/Timestamp |
| D2 | `date_part` | `date_part(part, expr)` | `FunctionCall` | Integer |
| D3 | `current_date` | `current_date()` | `FunctionCall` | Date |
| D4 | `current_timestamp` | `current_timestamp()` | `FunctionCall` | Timestamp |
| D5 | `date_add` | `date_add(date, interval)` | `FunctionCall` | Date |
| D6 | `date_diff` | `date_diff(date1, date2)` | `FunctionCall` | Integer |
| D7 | `to_date` | `to_date(expr [, format])` | `FunctionCall` | Date |
| D8 | `to_timestamp` | `to_timestamp(expr [, format])` | `FunctionCall` | Timestamp |

### D1 `date_trunc` — Dialect Rendering

Truncates a date/timestamp to the specified granularity. Rendered via `SqlDialect::date_trunc()`.

| Engine | Syntax | Example |
|--------|--------|---------|
| DataFusion | `date_trunc('month', expr)` | lowercase function name |
| DuckDB | `date_trunc('month', expr)` | lowercase function name |
| Spark | `date_trunc('month', expr)` | lowercase function name |
| ANSI | `DATE_TRUNC('month', expr)` | uppercase function name |

### D2 `date_part` — Detail

Extracts a date component (year, month, day, hour, etc.) as an integer.

| Engine | Function | Signature | Aliases |
|--------|----------|-----------|---------|
| DataFusion | `date_part` | `date_part('year', expr)` | `extract(YEAR FROM expr)` |
| DuckDB | `date_part` | `date_part('year', expr)` | `extract(YEAR FROM expr)`, `year(expr)` |
| Spark | `date_part` | `date_part('YEAR', expr)` | `extract(YEAR FROM expr)`, `year(expr)` |

Name-only across all engines.

### D3 `current_date` / D4 `current_timestamp` — Detail

| Engine | `current_date` | `current_timestamp` |
|--------|-----------------|---------------------|
| DataFusion | `current_date()` | `now()` / `current_timestamp()` |
| DuckDB | `current_date` (no parens) / `today()` | `current_timestamp` (no parens) / `now()` |
| Spark | `current_date()` / `curdate()` | `current_timestamp()` / `now()` |

Rendered via `SqlDialect::current_timestamp()`. `current_date` as `FunctionCall` emits the engine's preferred form.

### D5 `date_add` — Rewrite Detail

Adds an interval to a date.

| Engine | Rewrite | Detail |
|--------|---------|--------|
| DataFusion | **Structural** | `date_add(d, i)` -> `d + i` (interval arithmetic). DF has no `date_add()` function. |
| DuckDB | Name-only | `date_add(date, interval)` natively supported. Also `dateadd`. |
| Spark | Name-only | `date_add(start_date, num_days)` or `dateadd(start_date, num_days)`. Note: Spark's `date_add` takes integer days, not intervals. |

### D6 `date_diff` — Rewrite Detail

Returns the difference between two dates.

| Engine | Rewrite | Detail |
|--------|---------|--------|
| DataFusion | **Structural** | `date_diff(d1, d2)` -> `d2 - d1` (date subtraction). DF has no `datediff()`. |
| DuckDB | Name-only | `date_diff('day', d1, d2)` or `datediff('day', d1, d2)`. Note: DuckDB uses 3-arg form with unit. |
| Spark | Name-only | `datediff(end, start)` — returns integer days. 2-arg form. |

### D7 `to_date` / D8 `to_timestamp` — Detail

Converts strings to date/timestamp types.

| Engine | `to_date` | `to_timestamp` | Notes |
|--------|-----------|----------------|-------|
| DataFusion | `to_date(str [, fmt])` | `to_timestamp(str [, fmt])` | Format: strftime-style |
| DuckDB | `CAST(str AS DATE)` or `str::DATE` | `CAST(str AS TIMESTAMP)` or `str::TIMESTAMP` | No native `to_date()` function; `strptime` for custom format |
| Spark | `to_date(str [, fmt])` | `to_timestamp(str [, fmt])` | Format: Java SimpleDateFormat-style |

DuckDB adapter: structural rewrite `to_date(str)` -> `CAST(str AS DATE)` (or keep as `FunctionCall` and let SQL emitter handle).

---

## 10. Type Functions

| ID | Canonical | IR Repr | Notes |
|----|-----------|---------|-------|
| T1 | `cast` | `Expr::Cast` | Dedicated variant. Dialect renders via `SqlDialect::type_name()` for engine-specific type names. |

**Cast type mapping:**

| Canonical Type | DataFusion | DuckDB | Spark |
|----------------|------------|--------|-------|
| `Integer` | `BIGINT` | `BIGINT` | `BIGINT` |
| `Number` | `DOUBLE` | `DOUBLE` | `DOUBLE` |
| `String` | `VARCHAR` | `VARCHAR` | `STRING` |
| `Boolean` | `BOOLEAN` | `BOOLEAN` | `BOOLEAN` |
| `Date` | `DATE` | `DATE` | `DATE` |
| `Timestamp(p)` | `TIMESTAMP(p)` | `TIMESTAMP(p)` | `TIMESTAMP(p)` |
| `Decimal(p,s)` | `DECIMAL(p,s)` | `DECIMAL(p,s)` | `DECIMAL(p,s)` |

---

## 11. Expression Rewriting Design

### PlanBuilder Integration

Expression rewriting is injected via `PlanBuilder::rewrite_expr()`, called inside the default `build_filter()`, `build_project()`, `build_aggregate()`, and `build_join()` implementations — before expressions are placed into PlanNode.

```
Planner resolves expression
    -> calls plan_builder.build_filter(schema, input, predicate)
        -> build_filter calls self.rewrite_expr(predicate)
            -> rewrite_expr walks Expr tree bottom-up via Expr::transform()
            -> FunctionCall nodes: looked up in HashMap<CanonicalFn, FunctionTarget>
            -> Dedicated Expr variants: pattern-matched for engine-specific transforms
        -> rewritten expression placed into FilterNode
        -> finalize_node() called
```

### DataFusion Rewrite Summary

**PlanBuilder layer** (`DataFusionPlanBuilder::rewrite_expr`):

| Source | Target | Tier | Detail |
|--------|--------|------|--------|
| `FunctionCall("position")` | `FunctionCall("strpos")` | Name-remap | Via FunctionRewriter HashMap |
| `FunctionCall("mod")` | `BinaryOp(a - floor(a/b) * b)` | Structural | Via FunctionRewriter |
| `FunctionCall("date_add")` | `BinaryOp(d + interval)` | Structural | Via FunctionRewriter |
| `FunctionCall("date_diff")` | `BinaryOp(d2 - d1)` | Structural | Via FunctionRewriter |
| `Expr::RegexpExtract` | `FunctionCall("array_element", [FunctionCall("regexp_match", [expr, pattern]), group_idx + 1])` | Structural | Via pattern match; `array_element` extracts scalar from `regexp_match` array result |

**Dialect layer** (`DataFusionDialect`):

| Expr Variant | SQL Output | Method |
|--------------|------------|--------|
| `Expr::RegexpMatch` | `regexp_like(expr, pattern)` | `regexp_match()` |
| `Expr::ILike` | `expr ILIKE pattern` | `ilike()` |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc()` |
| `Expr::Cast` | `CAST(expr AS BIGINT)` etc. | `type_name()` |
| `Expr::RegexpExtract` | N/A (rewritten to FunctionCall by PlanBuilder) | — |

### DuckDB Rewrite Summary

**PlanBuilder layer:** No rewrites (DuckDB has no PlanBuilder yet — uses default identity).

**Dialect layer** (`DuckDbDialect`):

| Expr Variant | SQL Output | Method |
|--------------|------------|--------|
| `Expr::RegexpMatch` | `regexp_matches(expr, pattern)` | `regexp_match()` |
| `Expr::RegexpExtract` | `regexp_extract(expr, pattern, group_idx)` | `regexp_extract()` |
| `Expr::ILike` | `expr ILIKE pattern` | `ilike()` |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc()` |
| `Expr::Cast` | `CAST(expr AS BIGINT)` etc. | `type_name()` |

### Spark Rewrite Summary

**PlanBuilder layer:** No rewrites (Spark has no PlanBuilder yet — uses default identity).

**Dialect layer** (`SparkDialect`):

| Expr Variant | SQL Output | Method |
|--------------|------------|--------|
| `Expr::RegexpMatch` | `expr RLIKE pattern` (full), `expr RLIKE CONCAT('.*', pattern, '.*')` (partial) | `regexp_match()` |
| `Expr::RegexpExtract` | `regexp_extract(expr, pattern, group_idx)` | `regexp_extract()` |
| `Expr::ILike` | `LOWER(expr) LIKE LOWER(pattern)` | `ilike()` |
| `Expr::DateTrunc` | `date_trunc('grain', expr)` | `date_trunc()` |
| `Expr::Cast` | `CAST(expr AS BIGINT)` etc. | `type_name()` |

---

## 12. Coverage Audit

### IR Representation Coverage

Every canonical function maps to exactly one IR representation:

| Representation | Functions | Count |
|----------------|-----------|-------|
| `Expr::BinaryOp` | Comparison (C1-C6), Boolean (L1-L2), Arithmetic (R1-R4) | 12 |
| `Expr::Not` / `IsNull` / `IsNotNull` | Boolean (L3-L5) | 3 |
| `Expr::Aggregate` | Aggregates (A1-A6) | 6 |
| `Expr::Case` / `Coalesce` / `NullIf` / `InList` / `Between` | Conditional (N1-N7) | 7 |
| `Expr::Like` / `ILike` | Pattern (P1-P2) | 2 |
| `Expr::RegexpMatch` | Pattern (P3) | 1 |
| `Expr::RegexpExtract` | Pattern (P4) | 1 |
| `Expr::DateTrunc` | Date (D1) | 1 |
| `Expr::Cast` | Type (T1) | 1 |
| `Expr::FunctionCall` | All `CanonicalFn` enum variants (38) | 38 |

### YAML Parser Coverage (ExprBlock)

Functions available as declarative YAML tags (via `expr_block_serde!` macro):

| Status | Functions | Notes |
|--------|-----------|-------|
| Covered | `upper`, `lower`, `concat`, `concat_ws`, `length`, `trim`, `ltrim`, `rtrim`, `replace`, `substring`, `left`, `right`, `lpad`, `rpad`, `reverse`, `repeat`, `initcap`, `starts_with`, `ends_with`, `position`, `split_part`, `abs`, `ceil`, `floor`, `round`, `power`, `sqrt`, `mod`, `date_part`, `current_date`, `current_timestamp`, `date_add`, `date_diff`, `to_date`, `to_timestamp`, `greatest`, `least`, `like`, `ilike`, `regexp_match`, `regexp_extract`, `regexp_replace`, `coalesce`, `nullif`, `case`, `in_list`, `between`, `date_trunc`, `cast`, `negate`, `guard` | 51 tags |

All canonical functions have declarative YAML block support. Functions are also available via inline DSL expressions (e.g., `expr: "split_part(name, '_', 1)"`).

### Functions Present in All 3 Engines but NOT in CanonicalFn

These functions exist in DataFusion, DuckDB, and Spark but are not yet in our canonical set:

| Function | DataFusion | DuckDB | Spark | Priority |
|----------|------------|--------|-------|----------|
| `contains` | `contains(str, substr)` | `contains(str, substr)` | `contains(str, substr)` | Medium |
| `log` | `log(base, x)` / `log10(x)` / `log2(x)` | `log(x)` / `log2(x)` / `log10(x)` | `log(base, x)` / `log10(x)` / `log2(x)` | Low |
| `ln` | `ln(x)` | `ln(x)` | `ln(x)` | Low |
| `exp` | `exp(x)` | `exp(x)` | `exp(x)` | Low |
| `sign` | `signum(x)` | `sign(x)` | `signum(x)` / `sign(x)` | Low |
| `trunc` / `truncate` | `trunc(x [, d])` | `trunc(x [, d])` | `trunc(x, d)` | Low |
| `translate` | `translate(str, from, to)` | `translate(str, from, to)` | `translate(str, from, to)` | Low |
| `md5` | `md5(str)` | `md5(str)` | `md5(str)` | Low |
| `sha256` | `sha256(str)` | `sha256(str)` | `sha2(str, 256)` | Low |

These can be added to `CanonicalFn` as needed. They pass through as name-only `FunctionCall` nodes today via the inline DSL.

---

## 13. Known Bugs

No known bugs at this time.

### Fixed (historical)

- **BUG-1 (fixed):** DataFusion dialect emitted `regexp_match(...) IS NOT NULL` instead of `regexp_like(...)` for boolean regex matching. Fixed to use native `regexp_like` which returns `Boolean` directly.
- **BUG-2 (fixed):** DataFusion `regexp_extract` PlanBuilder rewrite passed `group_idx` as 3rd arg to `regexp_match` (interpreted as flags). Fixed to wrap with `array_element(regexp_match(expr, pattern), group_idx + 1)` for correct scalar extraction.

---

## 14. Complex Type Functions (OUT OF SCOPE — V1.x)

Listed for future reference. Not implemented in V1.

### Array Functions

| ID | Canonical | DataFusion | DuckDB | Spark |
|----|-----------|------------|--------|-------|
| X1 | `array_element` | `array_element(array, idx)` | `list_extract(list, idx)` / `array[idx]` | `element_at(array, idx)` |
| X2 | `array_length` | `array_length(array)` | `len(list)` / `array_length(list)` | `size(array)` / `array_size(array)` |
| X3 | `array_contains` | `array_has(array, elem)` | `list_contains(list, elem)` | `array_contains(array, elem)` |
| X4 | `array_agg` | `array_agg(expr)` | `list(expr)` / `array_agg(expr)` | `collect_list(expr)` / `array_agg(expr)` |
| X5 | `array_concat` | `array_concat(a1, a2)` | `list_concat(l1, l2)` | `concat(a1, a2)` |
| X6 | `array_sort` | `array_sort(array)` | `list_sort(list)` | `array_sort(array)` |

### Map Functions

| ID | Canonical | DataFusion | DuckDB | Spark |
|----|-----------|------------|--------|-------|
| X7 | `map_keys` | `map_keys(map)` | `map_keys(map)` | `map_keys(map)` |
| X8 | `map_values` | `map_values(map)` | `map_values(map)` | `map_values(map)` |
| X9 | `map_extract` | `map_extract(map, key)` | `element_at(map, key)` | `element_at(map, key)` |

### Struct Functions

| ID | Canonical | DataFusion | DuckDB | Spark |
|----|-----------|------------|--------|-------|
| X10 | `named_struct` | `named_struct(k1,v1,...)` | `struct_pack(k1:=v1,...)` | `named_struct(k1,v1,...)` |
| X11 | `get_field` | `get_field(struct, name)` | `struct.field` accessor | `struct.field` accessor |

---

## 15. Window Functions (EXPERIMENTAL — V1.x)

Control flow and usage constraints not yet defined. Listed for tracking.

| Function | DataFusion | DuckDB | Spark | Notes |
|----------|------------|--------|-------|-------|
| `row_number()` | `row_number()` | `row_number()` | `row_number()` | Rendered via `SqlDialect::window_row_number()` |
| `rank()` | `rank()` | `rank()` | `rank()` | |
| `dense_rank()` | `dense_rank()` | `dense_rank()` | `dense_rank()` | |
| `lag(expr, n)` | `lag(expr, n)` | `lag(expr, n)` | `lag(expr, n)` | |
| `lead(expr, n)` | `lead(expr, n)` | `lead(expr, n)` | `lead(expr, n)` | |
| `first_value(expr)` | `first_value(expr)` | `first_value(expr)` | `first_value(expr)` / `first(expr)` | |
| `last_value(expr)` | `last_value(expr)` | `last_value(expr)` | `last_value(expr)` / `last(expr)` | |
| `nth_value(expr, n)` | `nth_value(expr, n)` | `nth_value(expr, n)` | `nth_value(expr, n)` | |

---

## 16. Substrait Anchor Mapping

Function anchors are plan-local integer IDs used in Substrait `ScalarFunction.function_reference`. The adapter provides a `FunctionAnchorMap` that maps anchors to engine-specific function names for `SimpleExtensionDeclaration` entries.

Anchor values are arbitrary — engines resolve by name, not by anchor number.

### Current Anchor Allocation

| Range | Category | Anchors |
|-------|----------|---------|
| 1-6 | Aggregates | SUM=1, AVG=2, COUNT=3, COUNT_DISTINCT=4, MIN=5, MAX=6 |
| 100-105 | Comparison | EQUAL=100, NOT_EQUAL=101, LT=102, LTE=103, GT=104, GTE=105 |
| 200-213 | Boolean/Conditional | AND=200, OR=201, IS_NULL=202, ... REGEXP_EXTRACT=213 |
| 300-303 | Arithmetic | ADD=300, SUBTRACT=301, MULTIPLY=302, DIVIDE=303 |
| 400-420 | String | UPPER=400, LOWER=401, CONCAT=402, ... POSITION=420 |
| 500-506 | Date/Time | DATE_PART=500, CURRENT_DATE=501, ... TO_TIMESTAMP=506 |
| 600-606 | Math | ABS=600, CEIL=601, FLOOR=602, ROUND=603, ... MOD=606 |
| 700-702 | Conditional/Pattern | GREATEST=700, LEAST=701, REGEXP_REPLACE=702 |

New anchor ranges are allocated as functions are added. No spec requirement for specific values.
