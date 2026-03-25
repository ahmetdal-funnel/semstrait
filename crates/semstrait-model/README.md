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

## Key Types

| Type | Description |
|------|-------------|
| `SemanticModel` | Root model: name, description, namespace, kinds, datasets |
| `Kind` | Semantic entity with dimensions, measures, metrics, constraints, datasets |
| `KindTypeSpec` | `Grainset`, `Unionset { union_mode }`, `Joinset { associativity }` |
| `Dimension` | Named dimension with `DimensionType` (temporal/categorical/metadata/binary/geo/bucketed) |
| `Measure` | Aggregation expression with optional declarative `agg:`, constraints, additivity |
| `Metric` | Derived expression computed from measures/other metrics |
| `Dataset` | Physical data source binding with column mapping and storage |
| `ColumnMapping` | `Auto` (identity) or `Explicit(HashMap)` per dataset |
| `Relationship` | Join definition between datasets (from, to, type, columns, cardinality) |

### Kind Types

```yaml
# Grainset: route to cheapest covering dataset
type:
  grainset:

# Unionset: UNION ALL with NULL-fill
type:
  unionset:
    union_mode: all  # or "distinct"

# Joinset: BFS join chain from anchor
type:
  joinset:
    associativity: left
```

### Column Mapping

```yaml
# Explicit mapping: semantic name -> physical column
column_mapping:
  order_date: created_at
  revenue: amount

# Temporal grain override
column_mapping:
  order_date:
    column: month_start
    grain: month

# Auto mapping: physical names = semantic names (identity)
column_mapping: auto
```

---

## Schema

JSON Schema definitions for model validation are in the `schema/` directory:

- `semantic-model.schema.yaml` -- YAML-based JSON Schema for model files
- `reference.yaml` -- Reference documentation for model structure
- `semantic-model.legacy.json` -- Legacy JSON Schema format

---

## Dependencies

- `semstrait-core` -- `DataType`, `Expr`, `GlobPattern`
- `serde`, `serde_yaml` -- YAML deserialization
