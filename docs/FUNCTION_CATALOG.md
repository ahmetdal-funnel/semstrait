# Function Catalog — Canonical IR to Engine Mapping

**Version:** 1.0 | **Status:** Reference specification for expression rewriting design

---

## 1. Overview

Semstrait defines a **canonical function set** in the IR layer. Each canonical function has a documented mapping to engine-specific equivalents. Engines that differ from the canonical form require expression rewriting in the adapter layer, injected via `PlanBuilder::rewrite_expr()` during plan construction.

### Rewriting Tiers

| Tier | Description | Example |
|------|-------------|---------|
| **Name-only** | Engine uses the same function name and semantics | `upper` -> `upper` (all engines) |
| **Name-remap** | Engine uses a different name, same semantics and signature | `substring` -> `substr` (DataFusion) |
| **Structural** | Engine needs expression tree transformation | `regexp_extract(x,p,i)` -> `array_element(regexp_match(x,p), i+1)` (DataFusion) |
| **Unsupported** | Engine has no equivalent — produces error | `initcap` on DuckDB |

### Architecture

- `CanonicalFn` enum in `semstrait-ir` — identifies each canonical function
- `FunctionRewriter` in `semstrait-ir` — data-driven map from `CanonicalFn` to `FunctionTarget`
- `PlanBuilder::rewrite_expr()` — called during node construction, delegates to `FunctionRewriter` for `FunctionCall` nodes and pattern-matches dedicated `Expr` variants
- Engine adapters construct their `FunctionRewriter` with engine-specific mappings

---

## 2. Aggregate Functions

All aggregate functions are universal across all three engines. No rewriting needed.

| ID | Canonical | DataFusion | DuckDB | Spark | Tier | Arity | Returns |
|----|-----------|------------|--------|-------|------|-------|---------|
| A1 | `sum` | `sum` | `sum` | `sum` | Name-only | 1 | SameAsInput |
| A2 | `avg` | `avg` | `avg` | `avg` | Name-only | 1 | Number |
| A3 | `count` | `count` | `count` | `count` | Name-only | 0-1 | Integer |
| A4 | `count_distinct` | `count` + DISTINCT flag | `count` + DISTINCT | `count` + DISTINCT | Name-only + flag | 1 | Integer |
| A5 | `min` | `min` | `min` | `min` | Name-only | 1 | SameAsInput |
| A6 | `max` | `max` | `max` | `max` | Name-only | 1 | SameAsInput |

**IR representation:** `Expr::Aggregate(AggregateExpr)` — dedicated variant, not `FunctionCall`.

---

## 3. Comparison Operators

Comparison operators are binary operators in the IR (`Expr::BinaryOp`), mapped to Substrait `ScalarFunction` with name-based resolution. Universal across all engines.

| ID | Canonical | DataFusion Substrait name | DuckDB SQL | Spark SQL | Tier |
|----|-----------|---------------------------|------------|-----------|------|
| C1 | `equal` | `equal` | `=` | `=` | Name-only |
| C2 | `not_equal` | `not_equal` | `<>` / `!=` | `<>` / `!=` | Name-only |
| C3 | `lt` | `lt` | `<` | `<` | Name-only |
| C4 | `lte` | `lte` | `<=` | `<=` | Name-only |
| C5 | `gt` | `gt` | `>` | `>` | Name-only |
| C6 | `gte` | `gte` | `>=` | `>=` | Name-only |

**IR representation:** `Expr::BinaryOp(BinaryExpr { op: BinaryOp::Eq, .. })` — dedicated variant.

---

## 4. Boolean / Logic

| ID | Canonical | DataFusion | DuckDB | Spark | Tier | IR Repr |
|----|-----------|------------|--------|-------|------|---------|
| L1 | `and` | `and` | `AND` | `and` | Name-only | `BinaryOp::And` |
| L2 | `or` | `or` | `OR` | `or` | Name-only | `BinaryOp::Or` |
| L3 | `not` | `not` | `NOT` | `not` | Name-only | `Expr::Not` |
| L4 | `is_null` | `is_null` | `IS NULL` | `isnull` | Name-only | `Expr::IsNull` |
| L5 | `is_not_null` | `is_not_null` | `IS NOT NULL` | `isnotnull` | Name-only | `Expr::IsNotNull` |

---

## 5. Conditional / Null-Handling

| ID | Canonical | DataFusion | DuckDB | Spark | Tier | IR Repr | Arity | Returns |
|----|-----------|------------|--------|-------|------|---------|-------|---------|
| N1 | `coalesce` | `coalesce` | `coalesce` | `coalesce` | Name-only | `Expr::Coalesce` | 1+ (variadic) | SameAsInput |
| N2 | `nullif` | `nullif` | `nullif` | `nullif` | Name-only | `Expr::NullIf` | 2 | SameAsInput |
| N3 | `greatest` | `greatest` | `greatest` | `greatest` | Name-only | `FunctionCall` | 1+ (variadic) | SameAsInput |
| N4 | `least` | `least` | `least` | `least` | Name-only | `FunctionCall` | 1+ (variadic) | SameAsInput |
| N5 | `case/when` | native IfThen | `CASE WHEN` | `case/when` | Native Substrait | `Expr::Case` | N/A | varies |
| N6 | `in` | native SingularOrList | `IN (...)` | `in` | Native Substrait | `Expr::InList` | 2+ | Boolean |
| N7 | `between` | `between` | `BETWEEN x AND y` | `between` | Name-only | `Expr::Between` | 3 | Boolean |

---

## 6. Arithmetic

| ID | Canonical | DataFusion | DuckDB | Spark | Tier | IR Repr | Arity | Returns |
|----|-----------|------------|--------|-------|------|---------|-------|---------|
| R1 | `add` | `add` | `+` / `add` | `+` | Name-only | `BinaryOp::Add` | 2 | SameAsInput |
| R2 | `subtract` | `subtract` | `-` / `subtract` | `-` | Name-only | `BinaryOp::Subtract` | 2 | SameAsInput |
| R3 | `multiply` | `multiply` | `*` / `multiply` | `*` | Name-only | `BinaryOp::Multiply` | 2 | SameAsInput |
| R4 | `divide` | `divide` | `/` / `divide` | `/` | Name-only | `BinaryOp::Divide` | 2 | Number |
| R5 | `abs` | `abs` | `abs` | `abs` | Name-only | `FunctionCall` | 1 | SameAsInput |
| R6 | `ceil` | `ceil` | `ceil` / `ceiling` | `ceil` / `ceiling` | Name-only | `FunctionCall` | 1 | SameAsInput |
| R7 | `floor` | `floor` | `floor` | `floor` | Name-only | `FunctionCall` | 1 | SameAsInput |
| R8 | `round` | `round` | `round` | `round` | Name-only | `FunctionCall` | 1-2 | SameAsInput |
| R9 | `power` | `power` | `pow` / `power` | `pow` / `power` | Name-only | `FunctionCall` | 2 | Number |
| R10 | `sqrt` | `sqrt` | `sqrt` | `sqrt` | Name-only | `FunctionCall` | 1 | Number |
| R11 | `mod` | `%` operator | `%` / `mod` | `mod` / `pmod` | Name-remap (DF) | `FunctionCall` | 2 | SameAsInput |

**R11 DataFusion note:** DataFusion supports modulo via the `%` operator in SQL. In Substrait, map to a scalar function or emit as binary op. Verify DataFusion Substrait consumer behavior.

---

## 7. String Functions

| ID | Canonical | DataFusion | DuckDB | Spark | Tier (DF) | Arity | Returns |
|----|-----------|------------|--------|-------|-----------|-------|---------|
| S1 | `upper` | `upper` | `upper` / `ucase` | `upper` / `ucase` | Name-only | 1 | String |
| S2 | `lower` | `lower` | `lower` / `lcase` | `lower` / `lcase` | Name-only | 1 | String |
| S3 | `concat` | `concat` | `concat` | `concat` | Name-only | 1+ (variadic) | String |
| S4 | `concat_ws` | `concat_ws` | `concat_ws` | `concat_ws` | Name-only | 2+ (variadic) | String |
| S5 | `length` | `length` / `char_length` | `length` / `len` | `length` / `char_length` | Name-only | 1 | Integer |
| S6 | `trim` | `trim` | `trim` | `trim` | Name-only | 1 | String |
| S7 | `ltrim` | `ltrim` | `ltrim` | `ltrim` | Name-only | 1 | String |
| S8 | `rtrim` | `rtrim` | `rtrim` | `rtrim` | Name-only | 1 | String |
| S9 | `replace` | `replace` | `replace` | `replace` | Name-only | 3 | String |
| S10 | `substring` | `substr` / `substring` | `substring` / `substr` | `substr` / `substring` | Name-only | 2-3 | String |
| S11 | `left` | `left` | `left` | `left` | Name-only | 2 | String |
| S12 | `right` | `right` | `right` | `right` | Name-only | 2 | String |
| S13 | `lpad` | `lpad` | `lpad` | `lpad` | Name-only | 2-3 | String |
| S14 | `rpad` | `rpad` | `rpad` | `rpad` | Name-only | 2-3 | String |
| S15 | `split_part` | `split_part` | `split_part` | `split_part` | Name-only | 3 | String |
| S16 | `starts_with` | `starts_with` | `starts_with` / `prefix` | `startswith` | Name-remap (Spark) | 2 | Boolean |
| S17 | `ends_with` | `ends_with` | `ends_with` / `suffix` | `endswith` | Name-remap (Spark) | 2 | Boolean |
| S18 | `initcap` | `initcap` | N/A | `initcap` | Unsupported (DuckDB) | 1 | String |
| S19 | `reverse` | `reverse` | `reverse` | `reverse` | Name-only | 1 | String |
| S20 | `repeat` | `repeat` | `repeat` | `repeat` | Name-only | 2 | String |
| S21 | `position` | `strpos` | `position` / `strpos` | `locate` / `position` | Name-remap | 2 | Integer |

**S10 note:** All three engines support both `substr` and `substring`. DataFusion docs list both. No rewriting needed.

**S21 note:**
- DataFusion: `strpos(string, substring)` returns 1-based position. Also has `position(substr IN str)` SQL syntax.
- DuckDB: `position(substring IN string)` or `strpos(string, substring)`. Same semantics.
- Spark: `locate(substring, string[, pos])` — **argument order reversed** and has optional start position. Also supports `position(substr IN str)`.
- For Spark adapter: structural rewrite to swap argument order if using `locate`.

---

## 8. Pattern Matching

| ID | Canonical | DataFusion | DuckDB | Spark | Tier (DF) | IR Repr | Arity | Returns |
|----|-----------|------------|--------|-------|-----------|---------|-------|---------|
| P1 | `like` | `like` | `LIKE` | `like` | Name-only | `Expr::Like` | 2 | Boolean |
| P2 | `ilike` | `ilike` | `ILIKE` | `ilike` | Name-only | `Expr::ILike` | 2 | Boolean |
| P3 | `regexp_like` | `regexp_like` | `regexp_matches` | `regexp_like` / `rlike` | Name-remap (DuckDB) | `Expr::RegexpMatch` | 2-3 | Boolean |
| P4 | `regexp_extract` | see below | `regexp_extract` | `regexp_extract` | Structural (DF) | `Expr::RegexpExtract` | 2-3 | String |
| P5 | `regexp_replace` | `regexp_replace` | `regexp_replace` | `regexp_replace` | Name-only | `FunctionCall` | 3-4 | String |

### P3 — `regexp_like` (boolean regex match) — DETAILED

Returns TRUE if the string matches the pattern anywhere (partial match).

| Engine | Function | Signature | Return | Source |
|--------|----------|-----------|--------|--------|
| **DataFusion** | `regexp_like` | `regexp_like(str, regexp[, flags])` | Boolean | [DF docs: regexp_like](https://datafusion.apache.org/user-guide/sql/scalar_functions.html) |
| **DuckDB** | `regexp_matches` | `regexp_matches(string, pattern[, options])` | Boolean | [DuckDB docs: regexp](https://duckdb.org/docs/lts/sql/functions/regular_expressions.html) |
| **Spark** | `regexp_like` | `regexp_like(str, pattern)` | Boolean | [Spark docs](https://spark.apache.org/docs/latest/api/sql/index.html) — aliases: `rlike`, `regexp` |

**Current IR variant:** `Expr::RegexpMatch` — rename to `Expr::RegexpLike` (canonical alignment).

**Rewriting rules:**
- DataFusion: Name-only (`regexp_like` — native support)
- DuckDB: Name-remap → `regexp_matches`
- Spark: Name-only (`regexp_like` — native support)

### P4 — `regexp_extract` (capture group extraction) — DETAILED

Extracts a specific capture group from the first regex match. Returns a scalar string.

| Engine | Function | Signature | Return | Source |
|--------|----------|-----------|--------|--------|
| **DataFusion** | `regexp_match` | `regexp_match(str, regexp[, flags])` | **List\<String\>** (array of ALL capture groups) | [DF docs: regexp_match](https://datafusion.apache.org/user-guide/sql/scalar_functions.html) |
| **DuckDB** | `regexp_extract` | `regexp_extract(string, pattern[, group=0][, options])` | **VARCHAR** (scalar, single group) | [DuckDB docs: regexp](https://duckdb.org/docs/lts/sql/functions/regular_expressions.html) |
| **Spark** | `regexp_extract` | `regexp_extract(str, pattern[, groupIndex=0])` | **STRING** (scalar, single group) | [Spark docs](https://spark.apache.org/docs/latest/api/sql/index.html) |

**Critical difference:** DataFusion's `regexp_match` returns an **array** of all capture groups, not a scalar. Our canonical `regexp_extract` returns a single string for one group.

**Rewriting rules:**
- DataFusion: **Structural** → `array_element(regexp_match(expr, pattern), group_idx + 1)`. DataFusion's `regexp_match` returns 1-indexed List\<Utf8\>; `array_element` extracts a single element.
- DuckDB: Name-only (`regexp_extract` — native, matching semantics)
- Spark: Name-only (`regexp_extract` — native, matching semantics)

**Group index differences:**
- Canonical (IR): 0 = first capture group (following DuckDB default)
- DuckDB: default group=0 means entire match; group=1 = first capture group
- Spark: default groupIndex=0 means entire match; groupIndex=1 = first capture group
- DataFusion: `regexp_match` returns array starting at index 1 (first capture group)

**TODO:** Align group index semantics. DuckDB and Spark both use 0=entire match, 1+=capture groups. Our canonical should follow this convention.

---

## 9. Date/Time Functions

| ID | Canonical | DataFusion | DuckDB | Spark | Tier (DF) | IR Repr | Arity | Returns |
|----|-----------|------------|--------|-------|-----------|---------|-------|---------|
| D1 | `date_trunc` | `date_trunc` | `date_trunc` | `date_trunc` | Name-only | `Expr::DateTrunc` | 2 | Date/Timestamp |
| D2 | `date_part` | `date_part` / `extract` | `date_part` / `extract` | `date_part` / `extract` | Name-only | `FunctionCall` | 2 | Integer |
| D3 | `current_date` | `current_date()` | `current_date` / `today()` | `current_date()` / `curdate()` | Name-only | `FunctionCall` | 0 | Date |
| D4 | `current_timestamp` | `now()` / `current_timestamp()` | `now()` / `current_timestamp` | `now()` / `current_timestamp()` | Name-only | `FunctionCall` | 0 | Timestamp |
| D5 | `date_add` | interval arithmetic | `date_add` | `date_add` / `dateadd` | Structural (DF) | `FunctionCall` | 2 | Date |
| D6 | `date_diff` | interval arithmetic | `date_diff` / `datediff` | `datediff` / `date_diff` | Structural (DF) | `FunctionCall` | 2-3 | Integer |
| D7 | `to_date` | `to_date` | `CAST(x AS DATE)` | `to_date` | Name-remap (DuckDB) | `FunctionCall` | 1-2 | Date |
| D8 | `to_timestamp` | `to_timestamp` | `CAST(x AS TIMESTAMP)` | `to_timestamp` | Name-remap (DuckDB) | `FunctionCall` | 1-2 | Timestamp |

**D5 DataFusion note:** DataFusion does not have `date_add()`. Date arithmetic uses interval expressions: `date_column + INTERVAL '1' DAY`. Structural rewrite builds `BinaryOp::Add` with interval literal.

**D6 DataFusion note:** DataFusion does not have `datediff()`. Use date subtraction and extract: `(date2 - date1)`. Structural rewrite varies by unit (days, months, years).

---

## 10. Type Functions

| ID | Canonical | DataFusion | DuckDB | Spark | Tier | IR Repr |
|----|-----------|------------|--------|-------|------|---------|
| T1 | `cast` | native Cast | `CAST(x AS type)` | `cast` | Native Substrait | `Expr::Cast` → `RexType::Cast` |

---

## 11. Complex Type Functions (OUT OF SCOPE — V1.x)

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

## 12. Expression Rewriting Design

### PlanBuilder Integration

Expression rewriting is injected via `PlanBuilder::rewrite_expr()`, called inside the default `build_filter()`, `build_project()`, `build_aggregate()`, and `build_join()` implementations — before expressions are placed into PlanNode.

```
Planner resolves expression
    -> calls plan_builder.build_filter(schema, input, predicate)
        -> build_filter calls self.rewrite_expr(predicate)
            -> rewrite_expr walks Expr tree bottom-up via Expr::transform()
            -> FunctionCall nodes looked up in HashMap<CanonicalFn, FunctionTarget>
            -> Dedicated Expr variants (RegexpMatch, RegexpExtract) pattern-matched
        -> rewritten expression placed into FilterNode
        -> finalize_node() called
```

### Key Types

```
semstrait-ir/src/rewrite.rs:
  - CanonicalFn enum (function identity for data-driven mapping)
  - FunctionTarget enum (SameName | Rename | Rewrite | Unsupported)
  - FunctionRewriter struct (HashMap<CanonicalFn, FunctionTarget>)

semstrait-ir/src/plan_builder.rs:
  - PlanBuilder::rewrite_expr(&self, expr: Expr) -> Expr (default: identity)

semstrait-adapter/src/engines/datafusion/:
  - DataFusionPlanBuilder (impl PlanBuilder with rewrite_expr override)
```

### DataFusion Rewrite Summary

| Function | Rewrite | Detail |
|----------|---------|--------|
| `regexp_like` | Name-only | DataFusion has native `regexp_like` |
| `regexp_extract` | Structural | `array_element(regexp_match(expr, pattern), group_idx + 1)` |
| `date_add` | Structural | `expr + INTERVAL 'N' unit` |
| `date_diff` | Structural | Date subtraction + extract |
| `mod` | Name-remap | `%` operator |
| `position` | Name-remap | `strpos` |
| All others | Name-only | Pass through unchanged |

---

## 13. Substrait Anchor Mapping

Function anchors are plan-local integer IDs used in Substrait `ScalarFunction.function_reference`. The adapter provides a `FunctionAnchorMap` (renamed from `FunctionRegistry`) that maps anchors to engine-specific function names for `SimpleExtensionDeclaration` entries.

Anchor values are arbitrary — engines resolve by name, not by anchor number.

### Current Anchor Allocation

| Range | Category | Anchors |
|-------|----------|---------|
| 1-6 | Aggregates | SUM=1, AVG=2, COUNT=3, COUNT_DISTINCT=4, MIN=5, MAX=6 |
| 100-105 | Comparison | EQUAL=100, NOT_EQUAL=101, LT=102, LTE=103, GT=104, GTE=105 |
| 200-213 | Boolean/Conditional | AND=200, OR=201, IS_NULL=202, ... REGEXP_EXTRACT=213 |
| 300-303 | Arithmetic | ADD=300, SUBTRACT=301, MULTIPLY=302, DIVIDE=303 |
| 400-420 | String (NEW) | UPPER=400, LOWER=401, CONCAT=402, ... |
| 500-508 | Date/Time (NEW) | DATE_PART=500, CURRENT_DATE=501, ... |
| 600-610 | Math (NEW) | ABS=600, CEIL=601, FLOOR=602, ROUND=603, ... |

New anchor ranges are allocated as functions are added. No spec requirement for specific values.
