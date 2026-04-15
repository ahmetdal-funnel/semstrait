# semstrait-model

YAML model parsing and reference resolution for semstrait semantic models.

This crate handles deserialization of YAML semantic model files into typed Rust structs and resolves `ref:` entries to their inline definitions. It depends only on `semstrait-core` and provides the foundational types used by `semstrait-manifest` for compilation.

---

## Usage

```rust
use semstrait_model::{parse, resolve_refs};

let yaml = std::fs::read_to_string("model.yaml")?;
let model = parse(&yaml)?;
let resolved = resolve_refs(model)?;
```

---

## Type Hierarchy

### Root Model

| Type | Description |
|------|-------------|
| `SemanticModel` | Root: name, description, namespace, data_kinds, relationships, reusable definitions |
| `SemanticInterface` | Shared interface: dimensions, measures, metrics, filters, keys |
| `DataKind` | Unified entity enum: `Dataset`, `Grainset`, `Unionset`, `Joinset` |
| `DataKindEntry` | Dataset reference within a kind: `Inline(DataKindBinding)` or `Ref(DataKindRef)` |
| `Relationship` | Top-level join definition (from, to, type, columns, cardinality) |
| `DataKindRelationship` | Kind-internal join definition (same structure, scoped to kind) |

### DataKind Variants

| Type | Description |
|------|-------------|
| `DatasetKind` | Standalone dataset: interface + `DatasetExtras` |
| `GrainsetKind` | Grain-partitioned: interface + child datasets + `DataKindExtras` |
| `UnionsetKind` | UNION: interface + mode + child datasets + `DataKindExtras` |
| `JoinsetKind` | JOIN: interface + associativity + child datasets + relationships + `DataKindExtras` |
| `UnionMode` | `All` (default) or `Unique` |
| `JoinAssociativity` | `Left` (default), `Right`, `Full` |

```yaml
# Grainset: route to cheapest covering dataset
grainsets:
  - name: orders
    ...

# Unionset: UNION ALL with NULL-fill
unionsets:
  - name: events
    mode: unique  # or "all" (default)
    ...

# Joinset: BFS join chain from anchor
joinsets:
  - name: customers
    associativity: left
    ...
```

### Dimensions

| Type | Description |
|------|-------------|
| `DimensionEntry` | `Ref(name)` or `Inline(Dimension)` |
| `Dimension` | name, data_type, dim_type (default: Categorical), optional expr (computed) |
| `DimensionType` | `Temporal`, `Categorical` (default), `Binary`, `Geo`, `Bucketed`, `Metadata` |
| `TemporalDimension` | `grains: Vec<TemporalGrain>` (auto-derived if empty) |
| `CategoricalDimension` | optional `enum` constraint |
| `MetadataDimension` | `path: Option<PathExtraction>`, `partition: Option<PartitionExtraction>` |
| `TemporalGrain` | `Minute`, `Hour`, `Day`, `Week`, `Month`, `Quarter`, `Year` |

### Measures & Metrics

| Type | Description |
|------|-------------|
| `MeasureEntry` / `MetricEntry` | `Ref(name)` or `Inline(...)` |
| `Measure` | name, data_type, optional `agg`, optional `expr`, additivity, constraints, filters |
| `Metric` | name, data_type, optional `agg`, required `expr`, additivity, constraints, filters |
| `AggregationType` | `Sum`, `Avg`, `Count`, `CountDistinct`, `Min`, `Max` |
| `AdditivityType` | `Full`, `Semi(SemiAdditivity)`, `Non` |
| `MeasureConstraints` | `dimensions` (one_of, none_of, all) + `aggregations` (allowed, prohibited) |
| `MeasureFilter` | Named filter expression (applied via conditional aggregation) |

**Note:** `MeasureConstraints` applies to both measures and metrics despite the name.

### Data Types

Standard SQL logical types — engine-agnostic. The adapter layer maps these to engine-specific physical types.

```yaml
# Supported data_type values in YAML:
string, text, varchar           # SQL VARCHAR
integer, int, i32               # SQL INTEGER
long, bigint, int64, i64        # SQL BIGINT
float, float32, f32             # SQL REAL
double, float64, f64            # SQL DOUBLE PRECISION
bool, boolean                   # SQL BOOLEAN
date                            # SQL DATE
timestamp, datetime             # SQL TIMESTAMP
decimal                         # SQL DECIMAL(18,2) default
decimal(p, s)                   # SQL DECIMAL(p, s) explicit
i8, i16                         # small integer variants
```

### Column Mapping

| Type | Description |
|------|-------------|
| `ColumnMapping` | `Auto` (identity), `Inherited` (from kind), `Explicit(HashMap)` |
| `ColumnMappingValue` | `Simple(col)`, `WithGrain { column, grain }`, `Literal(value)`, `Anchored(map)` |

```yaml
# Explicit: semantic name -> physical column
column_mapping:
  order_date: created_at
  revenue: amount

# With temporal grain override
column_mapping:
  order_date:
    column: month_start
    grain: month

# Literal constant injection
column_mapping:
  source:
    literal: "search"

# Anchored sub-name mapping (for composed expressions)
column_mapping:
  total_cost:
    order_sum: physical_order_amount
    delivery_cost: physical_delivery_fee

# Auto: physical names = semantic names (identity)
column_mapping: auto
```

### Extras (Three-Level Inheritance)

Extras configure physical binding — temporal config, storage, catalog, and column mapping. They exist at three scopes with field-by-field override resolution: **dataset.extras > kind.dataset.extras > kind.extras**.

| Type | Scope | Key Difference |
|------|-------|----------------|
| `DataKindExtras` | Kind-level defaults | `column_mapping` is `Option` (optional default) |
| `DataKindBindingExtras` | Per-dataset in kind | `column_mapping` defaults to `Inherited` |
| `DatasetExtras` | Standalone dataset | No column_mapping (standalone datasets own their schema) |

### Temporal Config

| Type | Description |
|------|-------------|
| `TemporalConfig` | `grain` (optional), `dimension` (optional), `temporal_type` (required) |
| `TemporalHistorization` | `Timeseries`, `Events`, `Snapshot`, `Scd` (4 variants) |
| `TimeseriesConfig` | `occurred_at` — periodic data, semi-additive |
| `EventsConfig` | `occurred_at` — independent occurrences, fully additive |
| `SnapshotConfig` | `snapshotted_at` — point-in-time snapshots |
| `ScdConfig` | `ScdType` — Type1 through Type6 (slowly changing dimensions) |

```yaml
extras:
  temporal:
    grain: day                    # data-level cadence (enables grain auto-propagation)
    dimension: order_date         # links to semantic dimension name
    type:
      events:
        occurred_at: event_timestamp

  # Timeseries (semi-additive, window function dedup)
  temporal:
    type:
      timeseries:
        occurred_at: created_at

  # SCD Type 2
  temporal:
    type:
      scd:
        type_2:
          valid_from: effective_date
          valid_to: end_date
```

### Storage Config

| Type | Description |
|------|-------------|
| `StorageConfig` | `format`, `paths`, `tables`, `partition_def` |
| `DataFormat` | `Iceberg`, `Parquet`, `Csv` (from semstrait-core) |

`paths` and `tables` are mutually exclusive. Both support glob/wildcard patterns expanded at compile time.

```yaml
extras:
  storage:
    format: parquet
    paths:
      - "s3://bucket/orders/*.parquet"
      - "s3://bucket/orders_archive/*.parquet"

  # Or table-based (catalog resolves metadata)
  storage:
    tables:
      - "analytics.orders"
      - "analytics.orders_*"   # wildcard expanded via CatalogProvider
```

### Catalog Reference

| Type | Description |
|------|-------------|
| `CatalogRef` | Named reference to a catalog from `catalogs.yaml` |

```yaml
# Shorthand
extras:
  catalog: polaris_prod

# With namespace override
extras:
  catalog:
    alias: polaris_prod
    namespace: analytics
```

### Keys & AI Context

| Type | Description |
|------|-------------|
| `Keys` | `primary`, `unique`, `foreign` key definitions |
| `AiContext` | `synonyms`, `query_patterns`, `value_examples`, `semantic_tags` |

---

## Catalogs Module

Parses `catalogs.yaml` — external catalog configuration with authentication.

| Type | Description |
|------|-------------|
| `CatalogsConfig` | Named map of catalog entries |
| `CatalogEntry` | URI, warehouse, namespace, auth method |
| `CatalogAuthMethod` | `OAuth2`, `Bearer`, `AwsSecrets` |

```rust
use semstrait_model::catalogs::parse_catalogs;

let config = parse_catalogs("catalogs.yaml")?;
```

---

## Computed Dimensions & Expression Syntax

Dimensions (and measures/metrics) support an optional `expr:` field for computed values. The expression references **semantic names** — physical binding is resolved at compile time via `column_mapping`.

### Inline String Expressions

Simple arithmetic, function calls, and CASE expressions as a single string:

```yaml
dimensions:
  - name: market
    data_type: string
    expr: "UPPER(region)"

  - name: market_tier
    data_type: string
    expr: "CASE WHEN region IN ('US', 'EU') THEN 'Tier 1' ELSE 'Tier 2' END"

measures:
  - name: cpc
    agg: avg
    expr: "cost / clicks"
```

### Declarative YAML Expression Blocks

Structured YAML that maps 1:1 to `Expr` variants. Used for complex expressions (nested CASE, regex, function compositions):

```yaml
dimensions:
  - name: market
    data_type: string
    expr:
      case:
        when:
          - condition:
              in_list:
                expr: dataset_name
                list: ["adwords", "facebook"]
            then:
              upper: region
          - condition:
              like:
                expr: campaign
                pattern: "UK_%"
            then: "GB"
        else: ""
```

### Supported Declarative Expression Keys

| YAML Key | Expr Variant | Example |
|----------|-------------|---------|
| `case` | `Case` | `case: { when: [{condition: ..., then: ...}], else: ... }` |
| `in_list` | `InList` | `in_list: { expr: region, list: ["US", "EU"] }` |
| `like` | `Like` | `like: { expr: name, pattern: "%test%" }` |
| `ilike` | `ILike` | `ilike: { expr: name, pattern: "%test%" }` |
| `regexp_match` | `RegexpMatch` | `regexp_match: { expr: campaign, pattern: "^[A-Z]{2}_" }` |
| `regexp_extract` | `RegexpExtract` | `regexp_extract: { expr: campaign, pattern: "^([A-Z]{2})_", group: 1 }` |
| `between` | `Between` | `between: { expr: amount, low: 0, high: 100 }` |
| `is_null` | `IsNull` | `is_null: region` |
| `is_not_null` | `IsNotNull` | `is_not_null: region` |
| `coalesce` | `Coalesce` | `coalesce: [region, "Unknown"]` |
| `nullif` | `NullIf` | `nullif: { expr: value, null_expr: 0 }` |
| `upper`, `lower`, etc. | `FunctionCall` | `upper: region` (shorthand for single-arg functions) |

### Registered Functions (28 ANSI SQL)

Validated at compile time with arity checking:

- **String:** UPPER, LOWER, TRIM, LTRIM, RTRIM, LENGTH, CONCAT, REPLACE, SUBSTRING, LEFT, RIGHT, LPAD, RPAD
- **Math:** ABS, CEIL, FLOOR, ROUND, POWER, SQRT, MOD
- **Date:** CURRENT_DATE, CURRENT_TIMESTAMP, DATE_ADD, DATEDIFF, EXTRACT
- **Conditional:** GREATEST, LEAST, CAST

Unknown functions pass validation with a warning (extensibility for engine-specific functions).

### Note on Declarative Expressions

Declarative YAML expression blocks work at all scopes — top-level `datasets:`, `grainsets:`, `unionsets:`, and `joinsets:`. The original serde_yaml 0.9 limitation (DL-049) was resolved via a custom `Deserialize` impl for `ExprSource` (DL-061).

---

## Schema

JSON Schema definitions for model validation are in the `schema/` directory:

- `semantic-model.schema.yaml` -- YAML-based JSON Schema for model files
- `reference.yaml` -- Reference documentation for model structure

---

## Dependencies

- `semstrait-core` -- `DataType`, `Expr`, `DataFormat`, `GlobPattern`
- `serde`, `serde_yaml` -- YAML deserialization
