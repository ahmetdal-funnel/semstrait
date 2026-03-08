# CONTEXT.md

Context for AI coding assistants (Cursor, Claude Code, etc.) working on semstrait.

## Project Overview

semstrait is a semantic layer for Rust analytics applications. It compiles semantic models to Substrait compute plans, providing both schema context for UIs/LLMs and engine-portable query execution.

## Purpose

semstrait serves two primary use cases:

1. **Semantic context for applications** - Frontend/UX (web or conversational) can consume the semantic model to:
   - Display available dimensions, measures, metrics with labels
   - Generate query builders, filters, visualizations
   - Provide context for LLM-powered interfaces

2. **Engine-agnostic compute plans** - Compile semantic queries to Substrait, which can execute on:
   - DataFusion
   - DuckDB
   - Velox
   - Any Substrait-compatible engine

The semantic model is the single source of truth: define once, use for both UX generation and query execution.

## Architecture

**Noun modules** (data structures):
- `semantic_model/` - Semantic IR: Schema, SemanticModel, Dimension, Measure, Metric (format-agnostic)
- `query/` - Query request types (what the user wants to compute)
- `plan/` - Relational algebra: PlanNode, Expr, Column (close to Substrait)

**Verb modules** (transformations):
- `parser/` - Input format → `semantic_model::Schema` (YAML built-in, extensible)
- `selector/` - Selects optimal dataset from grain sets based on query requirements
- `resolver/` - Validates query against schema, resolves attribute references
- `planner/` - Semantic query → relational algebra plan
- `emitter/` - Plan → Substrait protobuf

## Root container (YAML)

Each semantic model has a **root container**: either a single **grain set** or a **union set** of members. The union set is a **recursive tree**:

- **Single grain set**: `grain_set: { name: orders, dimensions: [...], measures: [...], datasets: [...] }`
- **Union set (flat or nested)**: `union_set` is an array where each item is either:
  - **Leaf**: `grain_set: { name: adwords, ... }` — a single grain set (same as before).
  - **Group**: `name`, optional `label`, `description`, `dimensions`, `measures`, and **`union_set`** (array of the same item type). A group defines shared dimensions/measures for its children; they are merged along the path (child overrides parent by name).

Example nested structure: one group "Paid channels" with shared dimensions, and two child groups "Facebook" and "Adwords", each with their own dimensions/measures and child grain sets. Effective dimensions and measures at each leaf are computed by merging from root to leaf (child wins).

**Effective grain sets**: `SemanticModel::grain_sets()` returns a **flat** `Vec<GrainSet>` with dimensions and measures already merged from ancestor groups. The selector, planner, and resolver use this list; they do not need to know about the tree.

**Union instead of partition**: Scenarios that would previously use a single grain set with multiple partitioned datasets (e.g. one row per account) are modeled with **nested union_set**: one grain set per slice (e.g. `facebookads_111`, `facebookads_222`). The conformed-dimension path builds one branch per grain set and unions them. Use `_dataset.path` (container path from root to grain set, e.g. `facebookads.facebookads_111` or `adwords`) to identify the slice in the result.

A **grain set** is a set of datasets at the same logical grain with aggregate awareness; the selector picks the best dataset for the query. Code and errors may still refer to "dataset group" for the same concept.

## Key Design Decisions

- **`semantic_model/` IS the semantic IR** - all input parsers produce these types
- **`plan/` is relational algebra** - not semantic concepts, close to Substrait
- **Type-safe enums** - `DataType`, `Aggregation` validate at parse time, not runtime
- **Enums shared across layers** - e.g., `semantic_model::Aggregation` used directly in `plan::AggregateExpr`
- **Serde with aliases** - YAML strings like "count_distinct" deserialize to enum variants
- **Virtual dimensions** - Dimensions with `virtual: true` have no physical table and emit constant literal values (e.g., `_dataset` for metadata)
- **Conformed dimensions** - Declared at the highest level of the semantic model; use **two-part paths** (`dimension.attribute`), UNION across containers
- **Path-qualified dimensions** - Scoped to a container; use **path.dimension.attribute** where path is the container path from root. Path may be a **leaf** grain set (one result) or a **group** (UNION of all leaves under that path). E.g. `facebookads.campaign.name` (group) or `facebookads.facebookads_account_a.campaign.name` (leaf)
- **Typed NULLs in UNION** - When combining dimensions from different grain sets, NULLs carry the correct type for schema compatibility
- **Source types are declarative** - `Source` enum supports `Parquet` (file path) and `Iceberg` (table identifier). Catalog/connection resolution is the service layer's responsibility, not the semantic model's

## Expressions

Measure and metric expressions are structured YAML that deserialize directly to Rust enums:

```yaml
expr:
  multiply: [quantity, priceeach]
```

→ `ExprNode::Multiply(vec![...])` in `semantic_model/measure.rs`

Supported: `add`, `subtract`, `multiply`, `divide`, `case` (CASE WHEN), `column`, `literal`

To add a new expression type: add variant to `ExprNode` enum, serde handles deserialization automatically.

## Code Style

- Validation at parse time via serde, not runtime checks
- Tests in same file: `#[cfg(test)] mod tests { ... }`
- Custom error types with `Display` impl
- Prefer pattern matching over if-let chains

## Common Tasks

| Task | Location |
|------|----------|
| Add data type | `model/types.rs` - add to `DataType` enum, update `FromStr` |
| Add aggregation | `model/types.rs` - add to `Aggregation` enum, update `FromStr` |
| Add input format | Create parser crate that produces `model::Schema` |
| Add plan node | `plan/node.rs` + update `planner/` and `emitter/` |
| Add source type | `semantic_model/datasetgroup.rs` - add variant to `Source` enum, update accessors on `Dataset` and `Dimension`, update `semstrait-js` `Source` type |
| Add `_dataset` attribute | 1) Add to model's `_dataset` dimension attributes, 2) Add to dataset's `_dataset` list |
| Add dataset property | 1) Add to `dataset.properties`, 2) Declare in `_dataset` dimension, 3) Add to dataset's `_dataset` list |
| Add conformed dimension | Define at model-level `dimensions` (queryable with 2-part path) |
| Add container-scoped dimension | Define in grain_set or union group (queryable with path.dimension.attribute) |

## Source Types

The `Source` enum (`semantic_model/datasetgroup.rs`) describes where physical data lives:

| Type | YAML | Fields | Notes |
|------|------|--------|-------|
| Parquet | `type: parquet` | `path` | Local or remote file path, supports template variables |
| Iceberg | `type: iceberg` | `table` | Table identifier (e.g., `warehouse.orderfact`). Catalog resolution is the service layer's responsibility |

The planner and emitter never inspect `Source` -- they use `dataset.dataset` and `dimension.table` for Substrait `NamedTable` references. `Source` is consumed by the service layer to register tables in the query engine.

## Dimension Path Types

| Defined At | Path Format | Example | Behavior |
|------------|-------------|---------|----------|
| Model-level (conformed) | Two-part | `dates.year` | Conformed; UNION across all containers |
| Container-scoped | Path + two-part | `adwords.campaign.name` (leaf) or `facebookads.campaign.name` (group) | Path may be a leaf grain set or a union group; group path UNIONs all leaves under it |
| Virtual (model-level) | Two-part | `_dataset.path` | Literal values across all grain sets |

## Testing

```bash
cargo test           # Run all tests
cargo test types     # Run tests matching "types"
```
