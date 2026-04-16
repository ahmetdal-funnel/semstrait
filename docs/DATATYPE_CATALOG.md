# Data Type Catalog — Canonical to Engine Mapping

**Version:** 1.0 | **Status:** Reference specification for type mapping design

---

## 1. Overview

Semstrait defines 8 **canonical logical types** in `semstrait-core::DataType`. These represent
business-level data categories, not physical storage formats. Engine adapters map these
to engine-specific SQL type names via `SqlDialect::type_name()`, used in `CAST` expressions
and plan artifact generation.

### Architecture

```
semstrait-core/src/data_type.rs:
  - DataType enum (8 variants: Integer, Number, Decimal, String, Boolean, Date, Timestamp, Binary)
  - FromStr: accepts YAML aliases (i32, float, varchar, bigint, etc.)
  - Display: canonical lowercase names (integer, number, string, etc.)

semstrait-adapter/src/sql/dialect.rs:
  - SqlDialect::type_name(&self, dt: &DataType) -> String
  - Per-engine overrides in DataFusionDialect, DuckDbDialect, SparkDialect

semstrait-ir/src/substrait/type_mapping.rs:
  - datatype_to_substrait(): canonical DataType -> Substrait proto::Type
  - substrait_to_datatype(): Substrait proto::Type -> canonical DataType

semstrait-adapter/src/sql/expr_renderer.rs:
  - CAST rendering: CAST({expr} AS {dialect.type_name(dt)})

semstrait-adapter/src/sql/polyglot/expr_builder.rs:
  - CAST rendering: inner.cast(&data_type.to_string()) — uses canonical Display
```

### Type Mapping Layers

1. **Model -> Core**: `map_data_type()` in `steps.rs` maps YAML `data_type` (I64, F64, etc.)
   to canonical `DataType` (Integer, Number, etc.). Many-to-one: I8/I16/I32/I64 all become Integer.

2. **Core -> Substrait**: `type_mapping.rs` maps canonical DataType to Substrait proto types.
   Fixed 1:1 mapping. Integer -> I64, Number -> Fp64, etc.

3. **Core -> SQL**: `SqlDialect::type_name()` maps canonical DataType to engine-specific SQL
   keywords. This is where engine differences matter.

---

## 2. Canonical Types

| Canonical | Semantic Meaning | Substrait Proto | ANSI SQL |
|-----------|-----------------|-----------------|----------|
| `Integer` | Whole numbers | `I64` (nullable) | `INTEGER` |
| `Number` | Floating-point | `Fp64` (nullable) | `DOUBLE PRECISION` |
| `Decimal{p,s}` | Fixed-precision | `Decimal{p,s}` (nullable) | `DECIMAL(p,s)` |
| `String` | Text values | `String` (nullable) | `VARCHAR` |
| `Boolean` | True/false | `Bool` (nullable) | `BOOLEAN` |
| `Date` | Calendar date | `Date` (nullable) | `DATE` |
| `Timestamp{p}` | Date+time | `PrecisionTimestamp{p}` (nullable) | `TIMESTAMP(p)` |
| `Binary` | Raw bytes | `Binary` (nullable) | `VARBINARY` |

### V1 Scope

Complex/composite types (Array, Map, Struct, Union) are **out of scope** for V1.
The 8 canonical types cover all common analytics column types.

---

## 3. Per-Type Engine Mapping

### 3.1 Integer

Canonical `Integer` represents 64-bit signed integers in all engines.

| Engine | SQL Keyword | Notes |
|--------|------------|-------|
| ANSI | `INTEGER` | Standard, but ambiguous (often 32-bit in practice) |
| DataFusion | `BIGINT` | Arrow Int64. DF also accepts `INTEGER` but it maps to Int32 |
| DuckDB | `BIGINT` | 8-byte signed. `INTEGER` = 4-byte in DuckDB |
| Spark | `BIGINT` | 8-byte signed. `INT`/`INTEGER` = 4-byte in Spark |

**Canonical choice:** `BIGINT` for all engines (3/3 agree for 64-bit semantics).
ANSI fallback uses `INTEGER` as it is the ANSI standard keyword, though semantics
differ (some engines interpret as 32-bit).

### 3.2 Number

Canonical `Number` represents IEEE 754 double-precision floating point.

| Engine | SQL Keyword | Notes |
|--------|------------|-------|
| ANSI | `DOUBLE PRECISION` | Standard keyword |
| DataFusion | `DOUBLE` | Arrow Float64. `DOUBLE PRECISION` also accepted |
| DuckDB | `DOUBLE` | Alias: `FLOAT8` |
| Spark | `DOUBLE` | No aliases in cast context |

**Canonical choice:** `DOUBLE` for engines (3/3 agree). ANSI uses `DOUBLE PRECISION`.

### 3.3 Decimal

Canonical `Decimal{precision, scale}` represents fixed-precision decimal.

| Engine | SQL Keyword | Default (bare `DECIMAL`) | Max Precision |
|--------|------------|--------------------------|---------------|
| ANSI | `DECIMAL(p,s)` | Implementation-defined | Implementation-defined |
| DataFusion | `DECIMAL(p,s)` | `DECIMAL(38,10)` | 38 |
| DuckDB | `DECIMAL(p,s)` | `DECIMAL(18,3)` | 38 |
| Spark | `DECIMAL(p,s)` | `DECIMAL(10,0)` | 38 |

**Canonical choice:** `DECIMAL(p,s)` — all engines agree on syntax.
Semstrait always emits with explicit `(p,s)` to avoid default divergence.
Note: Spark also accepts `DEC`, `NUMERIC` as aliases. DuckDB accepts `NUMERIC`.

### 3.4 String

Canonical `String` represents variable-length text.

| Engine | SQL Keyword | Notes |
|--------|------------|-------|
| ANSI | `VARCHAR` | Standard |
| DataFusion | `VARCHAR` | Arrow Utf8. Also accepts `TEXT`, `STRING` |
| DuckDB | `VARCHAR` | Primary name. `STRING`, `TEXT`, `CHAR`, `BPCHAR` are aliases (all identical behavior) |
| Spark | `STRING` | Native type. `VARCHAR(n)` is a schema constraint, not a cast target |

**Canonical choice:** `VARCHAR` for ANSI/DataFusion/DuckDB (3/4 including ANSI).
Spark **must** use `STRING` — `CAST(x AS VARCHAR)` works but `STRING` is idiomatic
and `VARCHAR` without length specifier may produce warnings in some Spark versions.

### 3.5 Boolean

Canonical `Boolean` represents true/false values.

| Engine | SQL Keyword | Notes |
|--------|------------|-------|
| ANSI | `BOOLEAN` | Standard |
| DataFusion | `BOOLEAN` | Arrow Boolean |
| DuckDB | `BOOLEAN` | Aliases: `BOOL`, `LOGICAL` |
| Spark | `BOOLEAN` | No aliases in cast |

**Canonical choice:** `BOOLEAN` — all engines agree (4/4). No override needed.

### 3.6 Date

Canonical `Date` represents calendar dates without time component.

| Engine | SQL Keyword | Notes |
|--------|------------|-------|
| ANSI | `DATE` | Standard |
| DataFusion | `DATE` | Arrow Date32 |
| DuckDB | `DATE` | 4-byte (days since epoch) |
| Spark | `DATE` | Calendar date |

**Canonical choice:** `DATE` — all engines agree (4/4). No override needed.

### 3.7 Timestamp

Canonical `Timestamp{precision}` represents date+time with fractional seconds.
Precision: 0=seconds, 3=milliseconds, 6=microseconds, 9=nanoseconds.

| Engine | SQL Keyword | Precision Syntax | Default Precision | Timezone |
|--------|------------|------------------|-------------------|----------|
| ANSI | `TIMESTAMP(p)` | Parametric | Implementation-defined | Without tz |
| DataFusion | `TIMESTAMP` | No precision param in CAST | Microseconds (6) | Without tz |
| DuckDB | `TIMESTAMP` | Suffixed: `TIMESTAMP_S`, `TIMESTAMP_MS`, `TIMESTAMP_NS` | Microseconds (6) | Without tz |
| Spark | `TIMESTAMP` | No precision param | Microseconds | `TIMESTAMP_LTZ` by default |

**Engine-specific mapping:**

| Canonical | DataFusion | DuckDB | Spark |
|-----------|-----------|--------|-------|
| `Timestamp{0}` | `TIMESTAMP(0)` | `TIMESTAMP_S` | `TIMESTAMP` |
| `Timestamp{3}` | `TIMESTAMP(3)` | `TIMESTAMP_MS` | `TIMESTAMP` |
| `Timestamp{6}` | `TIMESTAMP(6)` | `TIMESTAMP` | `TIMESTAMP` |
| `Timestamp{9}` | `TIMESTAMP` | `TIMESTAMP_NS` | `TIMESTAMP` |

**Notes:**
- DataFusion: Supports `TIMESTAMP(p)` syntax where p must be one of {0, 3, 6, 9}.
  Bare `TIMESTAMP` defaults to nanoseconds (p=9). The precision parameter maps to
  Arrow TimeUnit: 0=Second, 3=Millisecond, 6=Microsecond, 9=Nanosecond.
- DuckDB: Uses precision-specific type names. `TIMESTAMP` = microseconds.
  Other precisions require explicit suffix (`TIMESTAMP_S`, `TIMESTAMP_MS`, `TIMESTAMP_NS`).
- Spark: Always `TIMESTAMP` — no precision parameter in SQL. Internal precision
  is microseconds. `TIMESTAMP_NTZ` available for timezone-naive semantics.

### 3.8 Binary

Canonical `Binary` represents raw byte sequences.

| Engine | SQL Keyword | Notes |
|--------|------------|-------|
| ANSI | `VARBINARY` | Standard |
| DataFusion | `BYTEA` | Only `BYTEA` is supported. `BINARY` and `VARBINARY` are unsupported |
| DuckDB | `BLOB` | Primary name. `BINARY`, `VARBINARY`, `BYTEA` are aliases |
| Spark | `BINARY` | Only `BINARY`. `VARBINARY` not valid |

**Canonical choice:** Engine-specific override required for all engines.
- DataFusion: `BYTEA` (only supported binary type keyword)
- DuckDB: `BLOB` (idiomatic, though `VARBINARY` alias works)
- Spark: `BINARY`
- ANSI: `VARBINARY` (SQL standard)

---

## 4. Summary: type_name() Override Matrix

This table shows which canonical types need engine-specific `type_name()` overrides
vs falling through to the ANSI default.

| Canonical | ANSI Default | DataFusion | DuckDB | Spark |
|-----------|-------------|------------|--------|-------|
| `Integer` | `INTEGER` | `BIGINT` | `BIGINT` | `BIGINT` |
| `Number` | `DOUBLE PRECISION` | `DOUBLE` | `DOUBLE` | `DOUBLE` |
| `Decimal` | `DECIMAL(p,s)` | same | same | same |
| `String` | `VARCHAR` | same | same | **`STRING`** |
| `Boolean` | `BOOLEAN` | same | same | same |
| `Date` | `DATE` | same | same | same |
| `Timestamp` | `TIMESTAMP(p)` | **`TIMESTAMP(p)`** | **precision-suffixed** | **`TIMESTAMP`** |
| `Binary` | `VARBINARY` | **`BYTEA`** | **`BLOB`** | **`BINARY`** |

Legend: **bold** = requires explicit override; "same" = ANSI default is correct.

### Change Summary vs Current Code

| Engine | Currently Overrides | Missing Overrides |
|--------|-------------------|-------------------|
| DataFusion | Integer, Number | **Timestamp** (DF supports `TIMESTAMP(p)` but only for p in {0,3,6,9}; bare ANSI format works for these), **Binary** (`BYTEA` — current `VARBINARY` fallback is unsupported) |
| DuckDB | Integer, Number | **Timestamp** (precision suffixes `TIMESTAMP_S`/`_MS`/`_NS`), **Binary** (`BLOB`) |
| Spark | Integer, Number | **String** (`STRING`), **Timestamp** (drop precision param), **Binary** (`BINARY`) |

---

## 5. Substrait Type Mapping

The `type_mapping.rs` mapping is canonical and engine-independent. It maps between
`semstrait-core::DataType` and Substrait protobuf types:

| Canonical | Substrait Direction: -> | Substrait Direction: <- |
|-----------|------------------------|------------------------|
| Integer | `I64` | `I8/I16/I32/I64` -> Integer |
| Number | `Fp64` | `Fp32/Fp64` -> Number |
| Decimal | `Decimal{p,s}` | `Decimal{p,s}` -> Decimal |
| String | `String` | `String` -> String |
| Boolean | `Bool` | `Bool` -> Boolean |
| Date | `Date` | `Date` -> Date |
| Timestamp | `PrecisionTimestamp{p}` | `PrecisionTimestamp{p}` -> Timestamp{p}, `Timestamp` -> Timestamp{6} |
| Binary | `Binary` | `Binary` -> Binary |

All types are emitted as `Nullable`. The reverse mapping collapses width variants
(I8/I16/I32/I64 all become Integer) since the semantic layer uses logical types
without width distinction.

---

## 6. Known Issues

### RESOLVED

1. **BUG: Spark `type_name` for String** — was falling through to ANSI `VARCHAR`,
   should be `STRING`. Spark's native string type is `STRING`.

2. **BUG: All engines `type_name` for Binary** — was falling through to ANSI `VARBINARY`.
   DataFusion needs `BYTEA`, DuckDB needs `BLOB`, Spark needs `BINARY`.

3. **BUG: Timestamp precision for DuckDB/Spark/DataFusion** — was falling through
   to ANSI `TIMESTAMP(p)`. DuckDB uses precision-specific suffixed types.
   Spark/DataFusion use bare `TIMESTAMP` without precision parameter.

### DEFERRED (V1.x)

4. **Timezone-aware timestamps** — `TIMESTAMP WITH TIME ZONE` / `TIMESTAMPTZ`
   not modeled in canonical DataType. Would require a new variant or flag.

5. **Interval types** — Not in scope for V1. Used internally by `date_add`/`date_diff`
   rewrites but not exposed as a canonical type.

6. **Complex types** — Array, Map, Struct, Union not modeled. Required for V1.x
   when `regexp_match` return type (`List<Utf8>`) needs proper typing.
