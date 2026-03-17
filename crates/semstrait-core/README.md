# semstrait-core

Foundation crate for the semstrait workspace. Zero internal workspace dependencies.

No I/O. No network. No engine dependencies. Pure type definitions and shared primitives.

---

## Key Types

- **`DataType`** — Arrow-aligned data type system (Int64, Float64, String, Date, Timestamp, Decimal, etc.)
- **`Schema`**, **`SchemaColumn`** — ordinal-based column schema for plan nodes
- **`ConsumerProfile`** — capability flags shared by planner and connectors (e.g., `SemiAdditiveStrategy`)
- **`Grain`** — temporal granularity levels (Day, Month, Quarter, Year)
- **`DslExpr`** — DSL expression tree (aggregations, arithmetic, CASE, DATE_TRUNC, etc.)
- **`GlobPattern`** — glob pattern matching for catalog table names (`*`, `?`)
- **Constraint types** — `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints`

---

## Module Map

```
semstrait-core/src/
├── lib.rs                  Public API surface and re-exports
├── data_type.rs            DataType enum, StructField
├── schema.rs               Schema, SchemaColumn
├── consumer_profile.rs     ConsumerProfile, SemiAdditiveStrategy
├── grain.rs                Grain enum (Day, Month, Quarter, Year)
├── dsl_expr.rs             DslExpr AST (Agg, Binary, Case, DateTrunc, etc.)
├── constraints.rs          Measure/dimension/aggregation constraint types
├── types.rs                GlobPattern with glob_match()
└── error.rs                CoreError, SchemaError
```

---

## Design Principles

- **Zero dependencies on other workspace crates** — everything above depends on core, never the reverse
- **No I/O** — no file system, network, or database access
- **Pure data types** — types are `Clone`, `Debug`, `Serialize`/`Deserialize` where needed
- **DSL only** — raw SQL is never stored; all expressions are typed `DslExpr` trees
