# semstrait-core

Foundation crate for the semstrait workspace. Zero internal workspace dependencies. No I/O. No network. No engine dependencies.

Defines the shared type vocabulary consumed by every other crate in the workspace: logical data types, expression trees, column schemas, temporal grains, constraint types, and data formats.

---

## DataType — Logical Type System

8 ANSI SQL logical types. Engine adapters map these to physical types (e.g., `Integer` -> Arrow Int64, `String` -> Utf8). Physical type mapping is the adapter's responsibility, not the semantic layer's.

| Variant | SQL Equivalent | Default | Notes |
|---------|---------------|---------|-------|
| `Integer` | INTEGER/BIGINT | — | Covers all integer widths (`i8`..`i64`, `tinyint`..`bigint`) |
| `Number` | DOUBLE PRECISION | — | Covers `float`, `f32`, `f64`, `double` |
| `Decimal { precision, scale }` | DECIMAL(p,s) | `(18,2)` | `precision` 1–38, `scale` 0–precision |
| `String` | VARCHAR | — | Covers `text`, `varchar`, `utf8`, `large_utf8` |
| `Boolean` | BOOLEAN | — | `bool`, `boolean` |
| `Date` | DATE | — | Calendar date without time. Covers `date32`, `date64` |
| `Timestamp { precision }` | TIMESTAMP(p) | `(0)` = seconds | `3`=ms, `6`=us. Aliases: `timestamp_ms`, `timestamp_us` |
| `Binary` | BLOB | — | Raw bytes |

**Parsing** (`FromStr`): accepts canonical names and aliases. YAML models use these strings in `data_type:` fields. Case-insensitive.

**Type predicates**: `is_numeric()`, `is_integer()`, `is_float()`, `is_temporal()`.

**Serde**: serializes to/from the canonical string representation (`"decimal(10,2)"`, `"timestamp(6)"`).

---

## Expr — Expression Tree

Unified AST used across the entire pipeline: YAML parsing -> manifest compilation -> planning -> IR -> SQL emission -> Substrait serialization. Raw SQL is never stored — only typed `Expr` trees.

### Variants (22)

**Leaf nodes:**

| Variant | Purpose | Convenience constructor |
|---------|---------|------------------------|
| `Column(ColumnRef)` | Physical or semantic column reference, optional qualifier | `Expr::column("name")`, `Expr::qualified_column("t", "name")` |
| `Literal(Literal)` | Int, float, string, boolean, or null | `Expr::int(42)`, `Expr::float(3.14)`, `Expr::string("x")`, `Expr::boolean(true)`, `Expr::null()` |
| `EntityRef(EntityRef)` | Semantic reference (measure/metric name). Resolved to `Column` during planning | — |

**Aggregation:**

| Variant | Aggregation enum | Constructor |
|---------|-----------------|-------------|
| `Aggregate(AggregateExpr)` | `Sum`, `Avg`, `Count`, `CountDistinct`, `Min`, `Max` | `Expr::sum(expr)`, `Expr::avg(expr)`, `Expr::count(expr)`, `Expr::count_distinct(expr)` |

**Binary operators** (`BinaryOp` enum → `BinaryExpr`):

| Category | Operators | Constructors |
|----------|-----------|-------------|
| Arithmetic | `Add`, `Subtract`, `Multiply`, `Divide`, `SafeDivide` | `Expr::add()`, `Expr::subtract()`, `Expr::multiply()`, `Expr::divide()`, `Expr::safe_divide()` |
| Comparison | `Eq`, `NotEq`, `Lt`, `LtEq`, `Gt`, `GtEq` | `Expr::eq()`, `Expr::not_eq()`, `Expr::lt()`, etc. |
| Logical | `And`, `Or` | `Expr::and()`, `Expr::or()`, `Expr::and_many(vec)` |

`SafeDivide` emits `CASE WHEN divisor = 0 THEN NULL ELSE ... END` — used for metric ratio expressions.

**Unary:** `Negate`, `Not`, `IsNull`, `IsNotNull`

**Predicates:**

| Variant | SQL | Notes |
|---------|-----|-------|
| `InList` / `Not InList` | `expr IN (values)` | `negated: bool` field |
| `Between` | `expr BETWEEN low AND high` | — |
| `Like` | `expr LIKE pattern` | — |
| `ILike` | `expr ILIKE pattern` | Case-insensitive. Engine support varies |
| `RegexpMatch` | `regexp_match(expr, pattern)` | `full_match` flag: true = Spark (full string), false = DF/DuckDB (substring) |
| `RegexpExtract` | `regexp_extract(expr, pattern, group_idx)` | 0-based `group_idx` |

**Compound:**

| Variant | SQL | Constructor |
|---------|-----|-------------|
| `Case` | `CASE WHEN ... THEN ... ELSE ... END` | `Expr::case(when_then, else_expr)` |
| `Coalesce` | `COALESCE(expr1, expr2, ...)` | — |
| `NullIf` | `NULLIF(expr, null_expr)` | `Expr::null_if(expr, null_expr)` |
| `DateTrunc` | `DATE_TRUNC(grain, expr)` | — |
| `Cast` | `CAST(expr AS type)` | `Expr::cast(expr, DataType::Integer)` |
| `Guard` | `CASE WHEN cond THEN expr ELSE NULL END` | Sugar for measure/metric filters. Expanded during planning |
| `FunctionCall` | `func(args...)` | Escape hatch for non-standard functions |

### Tree Utilities

| Method | Purpose |
|--------|---------|
| `walk(&mut f)` | Depth-first visitor. Calls `f` on every node |
| `transform(&f)` | Recursive rewrite. `f` returns `Ok(new_expr)` or `Ok(self)` to keep. Bottom-up |
| `column_refs()` | Extract all `Column` names as `HashSet<String>` |

### Aggregation Enum

`Aggregation` has 6 variants: `Sum`, `Avg`, `Count`, `CountDistinct`, `Min`, `Max`. Each exposes `sql_name()` for emission (`"SUM"`, `"AVG"`, etc.). `CountDistinct` emits `COUNT(DISTINCT ...)`.

---

## Schema — Column Schema for Plan Nodes

Every `PlanNode` carries an output `Schema`. Field references use `schema.ordinal("name")` — never positional indices.

```rust
Schema::from_fields(vec![
    ("id".to_string(), DataType::Integer, false),
    ("name".to_string(), DataType::String, true),
])
```

**Operations:**

| Method | Purpose |
|--------|---------|
| `ordinal("name")` | Column name → ordinal position. Returns `SchemaError` if missing |
| `project(&["a", "b"])` | Keep only named columns, re-assign ordinals |
| `join(left, right)` | Concatenate schemas. Right ordinals shifted by `left.len()` |
| `emit_mapping(target)` | Map self's ordinals → target's ordinals by column name |
| `get_column("name")` | Lookup by name |
| `get_column_by_ordinal(n)` | Lookup by ordinal |
| `contains("name")` | Existence check |

---

## Grain — Temporal Granularity

7 levels: `Minute`, `Hour`, `Day`, `Week`, `Month`, `Quarter`, `Year`.

`coarseness()` returns 0 (Minute) through 6 (Year). Used by the grainset planner for dataset routing — prefer coarser-grain datasets when the query grain allows.

Used in `DateTrunc` expressions and in `TemporalMapping` (column_mapping grain overrides).

---

## Constraints — Pre-Resolution Validity Gates

Runtime types for the `constraints:` YAML field. Evaluated at planner step 0, before any dataset routing.

```yaml
measures:
  - name: revenue
    constraints:
      dimensions:
        one_of: [date]         # at least one must be in the query
        none_of: [user_id]     # none can be in the query
        all: [region]          # all must be in the query
      aggregations:
        allowed: [sum, avg]
        prohibited: [count_distinct]
```

Three types: `MeasureConstraints` (top-level), `DimensionConstraints` (`one_of`/`none_of`/`all`), `AggregationConstraints` (`allowed`/`prohibited`).

---

## Other Types

| Type | Module | Purpose |
|------|--------|---------|
| `DataFormat` | `format.rs` | Physical data format enum: `Iceberg`, `Parquet`, `Csv` |
| `GlobPattern` | `types.rs` | Glob matching for catalog table names (`*`, `?`). Used during source resolution |
| `CoreError` | `error.rs` | General core errors |
| `SchemaError` | `error.rs` | `ColumnNotFound(String)` — returned by `Schema::ordinal()` |

---

## Module Map

```
semstrait-core/src/
  lib.rs              Public API surface and re-exports
  data_type.rs        DataType enum, FromStr with 30+ aliases, Serde, type predicates
  expr.rs             Expr AST (22 variants), convenience constructors, walk/transform/column_refs
  schema.rs           Schema + SchemaColumn — ordinal-based, join/project/emit_mapping
  grain.rs            Grain enum (7 levels), coarseness ordering
  constraints.rs      MeasureConstraints, DimensionConstraints, AggregationConstraints
  types.rs            GlobPattern with glob_match()
  format.rs           DataFormat enum (Iceberg, Parquet, Csv)
  error.rs            CoreError, SchemaError
```

---

## Design Principles

- **Zero dependencies on other workspace crates** — everything above depends on core, never the reverse
- **No I/O** — no file system, network, or database access
- **Pure data types** — types are `Clone`, `Debug`, `Serialize`/`Deserialize` where needed
- **DSL only** — raw SQL is never stored; all expressions are typed `Expr` trees
- **Logical types only** — physical type mapping belongs to engine adapters
