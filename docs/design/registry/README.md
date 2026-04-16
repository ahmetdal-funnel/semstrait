---
doc: design/registry/README
status: Living
purpose: Index of registry catalogs — authoritative per-engine mappings of canonical semstrait primitives
---

# Registry

The `docs/design/registry/` folder holds **catalog documents**: authoritative per-engine mappings of canonical semstrait primitives (data types, functions, temporal shapes, join types, ...) to their engine-native counterparts in DataFusion, Spark, DuckDB, and future targets.

## Why a separate folder

Design documents under `foundations/`, `data-kinds/`, and `apis/` define **canonical specifications** — what `DataType`, `CanonicalFn`, `Grain`, etc. *are* and how they compose. Those documents stabilize once ratified; they change rarely.

Engine mappings are **living catalogs**. They grow every time we add an engine, extend a function, discover an engine-specific edge case, or resolve a cast-semantics detail. Keeping them in the same documents as the canonical specs would force unrelated churn on foundational docs every time an adapter is extended.

The registry solves this with a clean separation:

- **`foundations/13_types_and_grain.md`** — ratifies the canonical `DataType` set, YAML grammar, and shape-unification rules. Stable.
- **`registry/types_mapping.md`** — documents how each canonical `DataType` maps to each target engine. Extensible; updated whenever an engine is added or mapping detail is refined.

## Current contents

| Document | Scope | Status |
|---|---|---|
| [`types_mapping.md`](types_mapping.md) | Canonical `DataType` ↔ DataFusion / Spark / DuckDB native types, cast semantics, per-engine gaps | Stub pending ratification of adapter surface (`34` / `36` / `37`) |
| [`functions_mapping.md`](functions_mapping.md) | Canonical function catalog ↔ engine function names, rewrite tiers, arity/signature differences, aliases, rewrite rules | Draft — tracks `foundations/14a_function_catalog.md` |
| [`temporal_shape_mapping.md`](temporal_shape_mapping.md) | Per-engine mapping of `TemporalShape` variants (Timeseries, Events, Snapshot, SCD Type 0–6) and the `AsOf` rewrite-tier matrix | Draft — tracks `foundations/17_temporal_shape.md` |
| [`join_types_mapping.md`](join_types_mapping.md) | Canonical `JoinType` ↔ engine-native join syntax, `AsOf` support matrix, cardinality-informed emission | Draft — tracks `foundations/16_composition.md` and `apis/35_semstrait_ir.md` |

## Engine coverage policy

Every registry catalog must cover the three **first-class compute targets**:

- **DataFusion** — primary implementation target; first adapter to ratify.
- **Spark** — SQL dialect and typed DataFrame target; drives lowest-common-denominator decisions (e.g. no native `Time`).
- **DuckDB** — reference warehouse-SQL dialect; drives SQL-idiom decisions.

Additional engines (future: Substrait-native, ClickHouse, BigQuery, Snowflake, Trino) are added as columns to the existing tables when their adapters land. Adding an engine column MUST NOT change the canonical set — if an engine lacks native support for a canonical variant, the catalog documents the adapter's emulation strategy and files a TECH_DEBT entry (see `TD-ADAPTER-SPARK-TIME` in `foundations/13` as a worked example).

## Versioning and churn

Registry documents are **Living**: they track adapter implementation reality. Each entry SHOULD cite the adapter crate and version where it was verified (e.g. "DuckDB 1.1.x"). Breaking changes in an engine (a rename, a semantics change between engine major versions) are documented as additional rows or dated annotations, not destructive edits.

Canonical specs under `foundations/` may reference the registry but never depend on its specific contents. A canonical variant is defined by the semantic it carries, not by any single engine's mapping.
