# semstrait-core

Foundation crate for the semstrait workspace. Zero internal workspace dependencies.

No I/O. No network. No engine dependencies. Pure type definitions and shared primitives.

---

## Key Types

- **`DataType`** — ANSI SQL logical type system (Integer, Number, String, Boolean, Date, Timestamp, Decimal, Binary). Adapter layer maps to engine-specific physical types.
- **`Schema`**, **`SchemaColumn`** — ordinal-based column schema for plan nodes
- **`DataFormat`** — data format enum (Iceberg, Parquet, Csv)
- **`Grain`** — temporal granularity levels (Minute, Hour, Day, Week, Month, Quarter, Year)
- **`Expr`** — unified expression tree (aggregations, arithmetic, CASE, DATE_TRUNC, ILIKE, REGEXP_MATCH, REGEXP_EXTRACT, FunctionCall, etc.)
- **`GlobPattern`** — glob pattern matching for catalog table names (`*`, `?`)
- **Constraint types** — `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints`

---

## Module Map

```
semstrait-core/src/
├── lib.rs                  Public API surface and re-exports
├── data_type.rs            DataType enum (8 ANSI SQL logical types)
├── schema.rs               Schema, SchemaColumn
├── grain.rs                Grain enum (Minute, Hour, Day, Week, Month, Quarter, Year)
├── expr.rs                 Unified Expr AST (Aggregate, BinaryOp, Case, DateTrunc, etc.)
├── constraints.rs          Measure/dimension/aggregation constraint types
├── types.rs                GlobPattern with glob_match()
├── format.rs               DataFormat enum (Iceberg, Parquet, Csv)
└── error.rs                CoreError, SchemaError
```

---

## Design Principles

- **Zero dependencies on other workspace crates** — everything above depends on core, never the reverse
- **No I/O** — no file system, network, or database access
- **Pure data types** — types are `Clone`, `Debug`, `Serialize`/`Deserialize` where needed
- **DSL only** — raw SQL is never stored; all expressions are typed `Expr` trees
