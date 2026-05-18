---
prereqs: [00, 11, 13, 14, 14a, 18, 19]
authoritative-for:
  - compile-time `Binding` process (one per `Dataset` leaf, `binding_id`, `sources`, compile-resolved `semantic_mapping`, `coverage`) and its identity / uniqueness rules
  - `BindingId` as a `u32` newtype, its allocation discipline, and `(DataKindId, BindingId)` global-uniqueness rule
  - the `PhysicalSource` sum type (`File`, `Table`, `Snapshot`) and the `Schema`, `PartitionColumn`, `CatalogRef` shapes it carries
  - the `FileFormat` enumeration (`Parquet`, `Csv`, `Json`, `Orc`, `Avro`) and the per-format schema-resolution strategy
  - glob-expansion algorithm, ordering determinism, and error model (no-match / catalog-unreachable / partial-schema)
  - `SemanticMapping` completeness rule at `compile` — every Semantics covered exactly once; extras rejected (struct shape owned by `18 §10`)
  - `Coverage` at Binding level (`Native`, `NullFill`, `Derived`, `Metadata`); scope boundary with `16`'s composition-level Coverage
  - the SemanticManifest-layer `ResolvedColumnMapping` shape (pre-split flat maps per value category) and its planner-speed contract — name retained at the SemanticManifest surface per `33 §5.3`
  - `MetadataDimension` extraction semantics — v1 supports **path-token extraction only** (`path.token: N`, 0-indexed scheme-stripped, raw segment); partition extraction described in `13 §4.7` is deferred to v2. Author writes the recipe on the Dimension type (`type: { metadata: { path: { token: N } } }`); compile synthesizes `SemanticMapping.entries[name] = SemanticMappingValue::Metadata(MetadataDimensionRecipe)` (4th variant, owned by `18 §10`); per-source `LiteralValue`s are eagerly resolved at compile via the layer-3 `path_token` mechanic (§8) plus a `Cast` to the declared `data_type`, and stored on each `ResolvedPhysicalSource.metadata_values` (§7.6)
  - the three-stratum expression model that lives across `14`/`19`/`15`: `SemanticExpr` (logical, semantic-name-keyed, supports constant folding / partial evaluation), `PhysicalExpr` (lowered SQL-equivalent over canonical types/functions), and the **compile-time-mechanic stratum** (non-expression layer, `15`-owned, e.g. `path_token`); a metadata extraction is layer-3 — not a registry function (`14a`) and not a `PhysicalExpr` variant
  - schema-reconciliation rules (widening silent; narrowing warns; incompatible errors; cross-source type agreement)
  - the compile-time resolution flow for bindings (the sub-steps inside `compile` that produce a `ResolvedBinding`)
  - error-code allocation `COMP_E_0300–0399` (schema/binding) and `COMP_E_0200–0299` (catalog/source resolution for binding-owned variants)
refined-by:
  - 16 (extends `Coverage` to the `ComposedSemanticInterface` level — per-constituent provenance of each field on a composed surface)
  - 20 (DataKind lifecycle — when a `ResolvedBinding` is constructed during `compile`, how it attaches to its `ResolvedDataKind`)
  - 21 (Simple / Dataset — per-variant consumer rules for a single Binding)
  - 22 (Grainset — Bindings live on Simple children; grain selection reads per-child Coverage)
  - 23 (Unionset — per-branch Bindings and per-source NullFill coverage)
  - 24 (Joinset — Bindings on Simple members; join-path construction is `16`'s job, not `15`'s)
  - 25 (Applicability matrix — per-DataKind-variant Binding-consumption cells)
  - 33 (`semstrait-manifest` — persistence of `ResolvedBinding` / `ResolvedColumnMapping` on the `SemanticManifest`)
  - 37 (`semstrait-catalog` — `CatalogRef`, `CatalogProvider`, `FileSystem` traits consumed during compile-time resolution)
---

# 15. Mapping and Binding

> **Struct ownership (2026-04-27 consolidation).** The v1 authoring-layer `SemanticMappingValue` enum roster — **4 variants** `{Column(String), Literal(LiteralValue), Expr(PhysicalExpr), Metadata(MetadataDimensionRecipe)}` — the `LiteralValue` enum, the `MetadataDimensionRecipe` / `MetadataExtraction` shapes, and the `semantic_mapping:` YAML block shape are ratified in `[18_entities.md §10](./18_entities.md#10-semanticmapping-value-shape)`. This doc owns the *Binding process* on top — compile-time resolution, per-source `Coverage`, `PhysicalSource` / `Schema` / `FileFormat` taxonomy, glob expansion, schema reconciliation, the SemanticManifest-layer `ResolvedColumnMapping` materialization, and the layer-3 compile-time mechanics (`path_token`, §8) that produce the per-source resolved metadata literals.
>
> **Rename cascade (body).** `ColumnMapping` → `SemanticMapping` (container), `ColumnMappingValue` → `SemanticMappingValue` (enum), `Computed` → `Expr` (variant). The v1 roster at the authoring surface is **4-variant**: `Metadata` is restored as a distinct variant alongside `Column` / `Literal` / `Expr` (reversing the earlier `[TD-MAP-METADATA-FOLD]` decision — see the resolution note below). The `ResolvedColumnMapping` SemanticManifest-layer name is retained per `33 §5.3` — a future symmetry rename to `ResolvedSemanticMapping` is tracked but is not part of this consolidation.
>
> `**auto` default.** An absent `semantic_mapping:` block on a `Dataset`'s `extras` is equivalent to `semantic_mapping: auto` per `18 §10.3` — every Semantic 1:1 to a physical column of the same name; explicit entries narrow the default.
>
> **Three-stratum expression model.** `15` is the authoritative anchor for the layered model that spans the expression docs:
>
> 1. `**SemanticExpr` (layer 1, owned by `14`; resolved by `19 §3`)** — logical expression tree, operates on Semantic names (`@semantics_ref`); supports logical constructs (`CASE WHEN`, etc.) and is the input to **constant folding / partial evaluation** when one or more leaves resolve to compile-time-known values (e.g. metadata literals, declared `Literal`-mapped Semantics).
> 2. `**PhysicalExpr` (layer 2, owned by `14`)** — lowered SQL-equivalent expression tree over canonical types and functions (`14a`); expression nodes only — no compile-time mechanics, no logical-only constructs that did not survive lowering.
> 3. **Compile-time mechanics (layer 3, owned by `15`)** — non-expression logic that runs purely during `compile` to produce values for the SemanticManifest. v1 host: `path_token` (§8). Layer-3 outputs feed back into layer-1 / layer-2 as resolved literals (`SemanticMappingValue::Metadata` recipe → eagerly-resolved per-source `LiteralValue` on `ResolvedPhysicalSource.metadata_values`); they are **never** registered as `14a` functions and **never** appear as `PhysicalExpr` variants.
>
> The fold from layer 1 to layer 2 is where partial evaluation lands: a `SemanticExpr` whose subtree depends only on layer-3 literals (e.g. `CASE WHEN @dataset_name = 'asd' THEN @col1 ELSE @col2 END` where `@dataset_name` is a metadata-bound Semantic) is rewritten at the lowering boundary into a flat `Column` projection per source, so plan-time scans never need to evaluate the conditional. The fold language is owned by `[19 §4.1](19_expression_flow.md)`; `15` ratifies the layer-3 inputs that fuel it.
>
> `**[TD-MAP-METADATA-FOLD]` — RESOLVED (2026-04-27).** The fold of `Metadata` into `Expr` is reversed; `Metadata` is restored as a distinct 4th variant. The author surface is the Dimension's `type: { metadata: { path: { token: N } } }` block (per `13 §4.7` / `18 §4`), and the binding-resolution pass synthesizes `SemanticMappingValue::Metadata(MetadataDimensionRecipe)` before the completeness check. Per-source resolved `LiteralValue`s land on `ResolvedPhysicalSource.metadata_values` (§7.6), not in the recipe. v1 scope is **path-token extraction only**; partition-level extraction (`partition.level: N`) is deferred to v2 — the Dimension-side body grammar in `13 §4.7` retains the partition shape for forward-compatibility, but compile-time recipe synthesis and runtime extraction are wired only for the path arm.

## 1. Purpose and Scope

### 1.1 What `15` ratifies

`15` is the foundations document that closes the loop between the **Semantics-facing** surface of a `SimpleDataKind` (named and shaped by `11`, typed by `13`, expressed by `14` / `14a`, resolved by `19`) and the **physical-facing** surface underneath it: the files, tables, or snapshots that actually hold the data, the schemas those targets expose, and the recipe (direct column / literal / computed / metadata-extraction) that produces each Semantics value from whatever rows the physical target returns.

Everything that sits **below the `SemanticInterface` boundary for a single `SimpleDataKind`** is authoritative here:

- The `Binding` — the single-instance join between Semantics and Physical, owned by exactly one `SimpleDataKind`.
- The `PhysicalSource` roster — the 1-or-more resolved targets the Binding points at (a file path, a catalog table, an Iceberg snapshot).
- The `SemanticMapping` — the Semantics-name-keyed recipe table.
- The per-source `Coverage` — which Semantics each source actually provides.
- The SemanticManifest-layer counterpart `ResolvedColumnMapping` — the flattened, pre-indexed form the planner consumes at query time.
- The compile-time resolution flow that moves a Model-level binding declaration into a `ResolvedBinding` living on a `ResolvedDataKind`.
- The `MetadataDimension` extraction mechanics introduced structurally in `13 §4.7`. v1 scope: **path-token extraction only** — `15 §8.1` pins down the layer-3 `path_token` compile mechanic, the `Cast`-to-declared-`data_type` policy, and the error conditions. Partition-level extraction (`partition.level: N`) is deferred to v2 (§8.0 v1-scope banner); `15` retains the original section structure as a forward-compatibility hint, with the v2 surface clearly marked.
- The **layer-3 compile-time-mechanic stratum** for non-expression compile work — the third stratum of the layered expression model (banner above). `path_token` is the v1 inhabitant; future inhabitants land here as the recipe surface grows.

### 1.2 What `15` does NOT ratify

- `**ComplexDataKind` composition.** `Unionset`, `Grainset`, and `Joinset` do not carry their own `Binding`s; they aggregate the `Binding`s of their constituent `SimpleDataKind`s. The composition mechanics, `ComposedSemanticInterface` shape, and per-composition `Coverage` (which constituent provides each field on the unified surface) all live in `foundations/16_composition.md`. `15` is explicit about the boundary in §6.4.
- **DataKind lifecycle.** How `ResolvedBinding`s attach to `ResolvedDataKind`s during `compile`, and the post-compile guarantees about their ordering inside the `SemanticManifest`, are ratified in `foundations/20_taxonomy.md` and the crate contract in `apis/33_semstrait_manifest.md`. `15 §10` enumerates the steps inside `compile` that produce a `ResolvedBinding`; it does not ratify their position in the `compile` driver.
- **Expression resolution.** `SemanticMappingValue::Expr(PhysicalExpr)` stores a compiled `PhysicalExpr`. The substitution algorithm that produces that `PhysicalExpr` from a `SemanticExpr` (via the `FunctionRegistry` in `14a` and the cross-DataKind walk) is owned by `[19 §3](19_expression_flow.md)`. `15 §5.3` describes only the **storage site** and the **wrapper-invariant contract** the stored `PhysicalExpr` must satisfy.
- **Catalog-provider shape.** The `CatalogProvider` trait surface, the `FileSystem` trait surface, their async posture, and their error enums are ratified in `apis/37_semstrait_catalog.md`. `15 §3.2` uses `CatalogRef` as an **opaque handle** into that surface; consumers of `15` never reach into the catalog crate directly.
- **Per-engine dialect specifics.** Nothing in `15` branches on engine identity (I3). A `PhysicalSource` carries a logical `DataType`-bearing `Schema`; conversion to an engine-specific type is adapter territory (`36`, I2).

### 1.3 Design posture

`15`'s posture is **compile-once, store-flat, plan-fast**:

- **Compile-once.** Every glob is expanded, every catalog table is fetched, every physical schema is reconciled against every declared `DataType`, every `SemanticExpr` on the semantic side is compiled into a `PhysicalExpr` over columns/literals, every per-source coverage bit is decided — all before the `SemanticManifest` is sealed. I5 demands it, I8 requires it to make plan-time O(1), and I4 demands it reproducibly.
- **Store-flat.** The Model-layer `SemanticMapping` is an enum-valued map keyed by `SemanticsName`. The SemanticManifest-layer `ResolvedColumnMapping` splits it into four parallel flat maps — one per variant — so the planner's per-Semantics lookup is a single HashMap probe with no enum match in the hot path (I6).
- **Plan-fast.** The planner never re-resolves anything in `15`'s scope. It reads `ResolvedBinding`, picks sources based on Coverage (Unionset §6.1; Grainset `17`), and emits `PlanNode`s. No catalog call, no filesystem call, no expression compilation.

### 1.4 Reference implementations — where `15` sits in the peer-group landscape

The brief is: pick a name for every concept we can't avoid having, and resist importing vocabulary that would re-open ratified decisions. Peers:

- **dbt metricflow.** `data_source.sql_table` / `data_source.sql_query` + `identifiers` + `measures` + `dimensions` — a single model-side block that couples a physical table to a set of Semantics. `15`'s `Binding` is the direct analog. metricflow has no analog for `MetadataDimension` (it does not pattern-match on S3 paths); it relies on the upstream warehouse to have the data already in shape. `15` keeps `MetadataDimension` because semstrait compiles over lake-native paths with partition-encoded metadata (`year=.../month=...`).
- **Cube.js.** `cube.sql_table` / `cube.sql` + `dimensions` + `measures` + `segments` + `pre_aggregations`. The `cube.sql` escape hatch is raw SQL; I1 forbids that here, so `SemanticMappingValue::Expr` carries a typed `PhysicalExpr` instead. Cube's `partition_granularity` is a roll-up concept ratified here by `Grain` + per-source `Coverage`, not by a Binding-level knob.
- **LookML.** `view.sql_table_name` + `view.derived_table` + per-dimension `sql:`. LookML's `${TABLE}.column` and `${other_view.field}` patterns are what `SemanticExpr` bare identifiers replace (resolved per `[14 §6.5](14_expressions.md)` + `[19 §3](19_expression_flow.md)`). LookML's `sql_trigger` / PDT machinery is outside `15`'s scope entirely.
- **Iceberg catalog.** The `Snapshot` variant of `PhysicalSource` is directly inspired by Iceberg's snapshot model — `metadata.current-snapshot-id` pins reproducibility (I4) against a moving warehouse state. Iceberg's partition-transform vocabulary (`year`, `month`, `day`, `hour`, `bucket[N]`, `truncate[N]`) informs `PartitionColumn` but is **not** replicated on it — partition-transform awareness is a planner concern (pruning), not a Binding concern.

The peers supply structural precedent and error-case nudges; nothing in the peer set overrides the semstrait vocabulary ratified in `00 §4`.

### 1.5 Guardrails — how `15` upholds `00 §9` invariants


| Invariant                                       | Where `15` keeps it                                                                                                                                                                                                                                                    |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **I1** — no raw SQL in canonical layer          | `SemanticMappingValue::Expr` carries `PhysicalExpr` (an `Expr` tree), never a string. The YAML surface for computed entries parses through `14 §6` into `SemanticExpr`, then compiles to `PhysicalExpr` per `19 §3`.                                                     |
| **I2** — physical types via adapters only       | `PhysicalSource.schema.columns[_].data_type` is logical `DataType` (per `13 §2`). The `14a` registry's promotion lattice is what decides widen/narrow; no Arrow/Spark/DuckDB type leaks into the SemanticManifest.                                                     |
| **I3** — no engine branching in canonical layer | `PhysicalSource` is engine-agnostic. No variant, no field, no error code in `15` names an engine. Adapters read `PhysicalSource` at `adapt` time and decide how to register it with their engine (`36`).                                                               |
| **I4** — SemanticManifest is deterministic      | Glob expansion sorts by lexical order of resolved absolute identifier (§3.5). Catalog-fetch results are sorted by fully-qualified name before being folded into the SemanticManifest. Ties are impossible by construction.                                             |
| **I5** — resolution is compile-time             | All Binding work (glob expansion, catalog fetch, schema resolution, SemanticMapping well-formedness, PhysicalExpr compilation, Coverage derivation) happens in `compile`. Plan-time reads only `ResolvedColumnMapping` and a pre-sorted `ResolvedPhysicalSource` list. |
| **I8** — SemanticManifest is planner-complete   | The SemanticManifest stores `ResolvedBinding`, `ResolvedColumnMapping`, and the per-source `ResolvedPhysicalSource` list. No planner step re-fetches a schema, re-expands a glob, or re-compiles a Computed expression.                                                |
| **I10** — non-exhaustive public sum types       | `PhysicalSource`, `FileFormat`, `SemanticMappingValue`, `CoverageVariant`, `CompileError` (all `15`-owned variants) carry `#[non_exhaustive]`. Adding a new file format, a new coverage variant, a new binding-error kind is MINOR per `30 §2`.                        |


I6 / I11 apply transitively — `15` describes the compile-time surface, which is the only place async / I/O is allowed per I11; the SemanticManifest forms that `15` produces is then consumed synchronously by the planner.

## 2. The `Binding`

### 2.1 Structure

A `Binding` is the single join point between a `SimpleDataKind`'s `SemanticInterface` and its physical backing. Every `SimpleDataKind` owns **exactly one** `Binding`; this is an invariant at both the Model layer (the YAML parser rejects multiple binding blocks on one kind per `11 §5.3`) and the SemanticManifest layer (the compile-time materialization produces a single `ResolvedBinding` per `ResolvedDataKind::Simple`). `ComplexDataKind`s carry **no** `Binding` of their own — they aggregate the `Binding`s of their constituent Simple children, through the `ComposedSemanticInterface` machinery in `16`.

Model-layer shape:

```rust
#[non_exhaustive]
pub struct Binding {
    pub binding_id: BindingId,
    pub sources: Vec<PhysicalSource>,
    pub semantic_mapping: SemanticMapping,
    pub coverage: Option<Coverage>,
}
```

Every field is populated by `compile`; the Model-layer YAML surface in `semstrait-model` uses a parallel `BindingSpec` type whose fields are unresolved (glob patterns, declarative `SourceRef`s, and `ExprSource` values inside `SemanticMappingSpec`). The resolution flow in §10 consumes a `BindingSpec` and produces a `Binding` (and, by the time it lands in the SemanticManifest, a `ResolvedBinding` — §7).

The `#[non_exhaustive]` tag is present to allow a future `post_binding_hook: Option<PhysicalExpr>` or similar Semantics-adjacent extension to be added as a MINOR per `30 §4`.

### 2.2 `BindingId`

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct BindingId(pub u32);
```

`BindingId` is the compile-time assigned identifier for a Binding. The canonical definition lives in `19 §3.2`; `15` ratifies its allocation discipline:

- **Allocation site.** Bindings are assigned their ID during the `compile` stage's binding-resolution pass (§10 step 1). The `compile` driver owns a monotonically-increasing `u32` counter, handed out in iteration order over the deterministic `ResolvedDataKind` roster. First ID is `0`; there is no `NULL` / `UNDEFINED` value.
- **Uniqueness scope.** `BindingId` is **unique within a SemanticManifest**, not within a `DataKind`. The compile stage's counter spans the entire SemanticManifest; every Binding across every `SimpleDataKind` gets a distinct `BindingId`.
- `**(DataKindId, BindingId)` uniqueness.** Because a `SimpleDataKind` owns exactly one `Binding`, `(DataKindId, BindingId)` is vacuously unique: given a `DataKindId`, the mapping to `BindingId` is a function (exactly one value). SemanticManifest indices may key on either half.
- **Stability across `compile` invocations.** `BindingId` is **stable within a single `compile` run and NOT stable across runs.** Recompiling a Model (even a byte-identical Model) produces the same IDs if and only if the `compile` driver's iteration order over `ResolvedDataKind`s is deterministic (I4). Adding a `SimpleDataKind` anywhere earlier in the iteration order shifts every later Binding's ID — which is intended. Cross-SemanticManifest Binding comparison by ID is not supported; the planner keys on `(DataKindId, BindingId)` and never on `BindingId` alone against another SemanticManifest.
- **Serialization.** The `u32` surfaces directly in the persisted SemanticManifest form (`33`). Serialized `SemanticManifest`s that round-trip through `Repository::store` and `Repository::load` preserve the IDs unchanged; round-tripping is NOT a re-`compile` and ID stability is preserved by structural equality, not by reconstruction.

**Proposed (Round 1):** the counter resets to `0` per SemanticManifest (per-compile scope). A cross-SemanticManifest namespace (e.g. embedding the SemanticManifest content hash into the ID) is not adopted; it would break the `u32` shape and have no concrete use case. See `questions/open/15_questions.md` Q-MAP-001.

### 2.3 Cross-reference: `19 §3.2`'s `ResolvedExprKey`

`19 §3.2`'s `ResolvedExprTable` is keyed on `(SemanticsName, BindingId)` precisely because `15` ratifies `BindingId` as the per-Binding identity. `15 §7.2` describes the SemanticManifest's storage split: the `ResolvedExprTable` stores the physical expression bodies; the `ResolvedColumnMapping.computed: HashMap<SemanticsName, PhysicalExpr>` is a **per-Binding denormalization** that copies the `PhysicalExpr` into the Binding's own hashmap for O(1) access without going through the global table. Whether the SemanticManifest stores the `PhysicalExpr` once (in the table) with the `ResolvedColumnMapping.computed` value being a pointer/index, or twice (once in each structure), is an implementation choice ratified in `33`. From the contract surface of `15`, both structures are populated and both are plan-readable.

### 2.4 Cross-reference: Complex composition

Per `00 §4.1` / `11 §5` / `16 §2`, a `ComplexDataKind` does not own a `Binding`. The composition rule is:

- **Unionset** — the union composition carries *N* `Binding`s, one per Simple (or nested) branch. The per-branch `Coverage` is what tells the planner "branch *b* has `NullFill` for Semantics *s*" so it can emit `SELECT NULL AS s` in the branch's `Project`. `15` ratifies the per-Binding `Coverage` shape (§6); `16` ratifies how a Unionset reads it at composition time.
- **Grainset** — each level resolves to a child DataKind with its own Binding set. Grain-selection is a planner decision that reads per-child Binding `Coverage` (specifically, which Semantics resolve to `Native` at which grain). `15` does not ratify the grain axis itself (that is `13 §5` + `17`); it only ratifies that Coverage captures the per-source bit the planner reads.
- **Joinset** — each join member is a Simple (or nested Complex) kind with its own Binding(s). The join-path construction over declared `Relationship`s is owned by `16`. `15`'s `Binding` has no Joinset-specific field.

## 3. The `PhysicalSource`

### 3.1 Sum-type shape

A `PhysicalSource` is a compile-time-resolved physical target. The variants span the practical v1 axis:

```rust
#[non_exhaustive]
pub enum PhysicalSource {
    File {
        path: String,
        format: FileFormat,
        schema: Schema,
        partitions: Vec<PartitionColumn>,
    },
    Table {
        catalog_ref: CatalogRef,
        schema: Schema,
        partitions: Vec<PartitionColumn>,
    },
    Snapshot {
        catalog_ref: CatalogRef,
        snapshot_id: SnapshotId,
        schema: Schema,
        partitions: Vec<PartitionColumn>,
    },
}
```

Every variant carries a resolved `schema: Schema` and a resolved `partitions: Vec<PartitionColumn>`. The tree of open `#[non_exhaustive]` types propagates: adding a variant (e.g. `PhysicalSource::Stream { ... }` for a future Kafka integration) is MINOR, and adding a field to any existing variant (e.g. `File { ... , read_options: ReadOptions }`) is MINOR provided the variant is `#[non_exhaustive]` — which it is at the variant level per Rust's rules when the enum itself is `#[non_exhaustive]`.

### 3.2 `Schema` shape

The schema is the compile-time snapshot of the physical columns exposed by the source:

```rust
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

pub struct SchemaColumn {
    pub name: ColumnName,
    pub data_type: DataType,
    pub nullable: bool,
}

pub struct ColumnName(pub String);
```

Ordering in `Schema.columns` is the source's native order (Parquet field order from the footer, Iceberg table schema field order, CSV header order, etc.). The compile stage preserves this ordering for determinism (I4) but the planner does not semantically depend on it; column lookup is by `ColumnName` through `ResolvedColumnMapping`.

`DataType` here is **logical** (I2) — the `13 §2` canonical set. Conversion from a physical-source type system (Arrow `DataType`, Iceberg type, CSV-inferred scalar) to the canonical `DataType` happens inside each `FileFormat`'s schema-resolution strategy (§4) or inside the relevant `CatalogProvider` implementation (`37`). By the time `Schema` exists, every type is logical.

`**nullable` source of truth.** Logical nullability is read from the physical source's metadata when available (Parquet: the `FieldType.required`/`optional` markers; Iceberg: `required: bool` per schema field; CSV/JSON: always `true` unless a declared schema overrides). It is not inferred from the data.

### 3.3 `CatalogRef`

```rust
pub struct CatalogRef {
    pub catalog_id: CatalogId,
    pub fqn: Fqn,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CatalogId(pub u32);

pub struct Fqn(pub String);
```

`CatalogRef` is an **opaque handle** back into the catalog registry owned by `semstrait-catalog` (`37`). The compile-time `CatalogProvider` interaction (fetch schema, fetch snapshot-id, list tables under glob) uses `catalog_id` to route to the right provider and `fqn` to address the specific table. The planner never dereferences a `CatalogRef`; it appears on `PhysicalSource::Table` / `Snapshot` so adapters (`36`) can carry the catalog identity into their engine-registration call.

`Fqn` is the dot-separated fully-qualified name in the catalog's own namespace convention (e.g. `"ns.sales.transactions"`). The `fqn`'s canonical spelling is what the catalog reports; semstrait does not normalize case or whitespace.

### 3.4 `PartitionColumn`

```rust
pub struct PartitionColumn {
    pub name: ColumnName,
    pub position: usize,
    pub data_type: DataType,
    pub nullable: bool,
}
```

`PartitionColumn.position` is **1-indexed** (matches `MetadataDimension.partition.level` per `13 §4.7` and §8 below). `position == 1` is the coarsest-declared partition column; `position == N` is the innermost. For File sources with Hive-style path-encoded partitioning (`year=*/month=*/day=*/*.parquet`), `PartitionColumn` is populated during glob expansion (§3.5) from the path components. For Table / Snapshot sources backed by Iceberg, it is populated from the partition spec that Iceberg REST returns (per `37` / legacy `CATALOG_RESOLUTION.md §4`).

**Grain inference is NOT done in `15`.** The Iceberg partition-transform vocabulary (`year`, `month`, `day`, `hour`) may carry a grain hint; consuming that hint is the planner's job (`22` Grainset, `17` TemporalShape). `15`'s `PartitionColumn` records only the resolved name/type/position; the transform, if any, is captured upstream in `37`'s `TableMetadataResponse` and attached to the `CatalogRef` / Binding via the compile-stage step that walks the catalog response. The exact placement of that transform record is a `37`-owned question.

### 3.5 Source resolution

The Model's YAML surface places a Binding's sources under `extras.storage.paths:` and `extras.storage.tables:` (per `32 §4`). Each entry resolves at compile to one or more `PhysicalSource`s; the Binding's `sources: Vec<PhysicalSource>` is the concatenation of every entry's resolution in author order across both fields. Per `21 §3.2 / §4.5`, multiple `PhysicalSource`s in one Binding compose under `Union ALL` with optional per-source pre-aggregation as a planner optimization.

A `PhysicalSource` is an **engine-level LogicalRelation** — one Substrait `ReadRel`, one DataFusion `TableScan`, one Spark `LogicalRelation`, one SQL `FROM` reference. Each resolved `PhysicalSource` carries a concrete identifier (path / FQN); engine-internal mechanics (Hive partition discovery, multi-file consolidation, schema merge for path sources; partition-spec consultation for catalog tables) live below the `PhysicalSource` boundary and are the adapter / engine's responsibility per `35 §4.2.1`.

#### 3.5.1 `paths:` resolution — `PhysicalSource::File`

- **Concrete path** (`"s3://bucket/orders/2024-01/customers.parquet"`, `"s3://bucket/orders/"`) → produces **one** `PhysicalSource::File` with the path stored verbatim. The engine treats the source as a single LogicalRelation and handles file consolidation, schema merge, and Hive-partition discovery internally.
- **Wildcard path** (`"s3://bucket/*/orders/"`, `"s3://lake/year=*/sales/"`) → compile enumerates each resolved variation via `FileSystem::expand_glob` → produces **one `PhysicalSource::File` per resolved variation**. Each resulting source has a concrete (wildcard-free) path string; downstream metadata extraction (`§8`'s `path_token`) operates on that concrete path per source.

#### 3.5.2 `tables:` resolution — `PhysicalSource::Table` / `Snapshot`

- **Concrete table FQN** (`"iceberg.sales.transactions"`) → produces **one** `PhysicalSource::Table`, or `Snapshot` if the spec carries `at: { snapshot_id: ... }`.
- **Table-name glob** (`"iceberg.sales.*_transactions"`) → compile-expanded via `CatalogProvider::list_tables` → produces **one `PhysicalSource::Table` per resolved FQN**.

#### 3.5.4 `partition_def` carriage — manifest-side, runtime-dormant in v1

`extras.storage.partition_def:` (per `32 §4`) is the canonical catalog-less partition declaration for file sources, in v1 form `Range { column }` / `List { column }`. The compile pass parses, schema-validates, and carries it verbatim onto each `PhysicalSource::File` it produces from a `paths:` entry. **No v1 plan-time logic consumes it** — adapters defer partition pruning to engine-side discovery from filter predicates per `35 §4.2.1`. The declaration is forward-compat for v2+ consumers (per-partition extraction per `Q-MAP-009`; partition-aware grain inference per `17`; planner pruning hints). This is a closure clause of `Q-MAP-002`.

#### 3.5.6 Expansion algorithm

The deterministic algorithm (I4) is:

1. **Classify each source spec** by which YAML key carried it (`paths:` → File; `tables:` → Table). `Snapshot` is produced only when a `tables:` entry carries an `at:` subkey.
2. **If the entry contains a glob metacharacter** (`*`, `?`, `[`), call the respective provider to enumerate:
  - `paths:` entries → `FileSystem::expand_glob(pattern) → Vec<String>` (ordered).
  - `tables:` entries → `CatalogProvider::list_tables(namespace, pattern) → Vec<Fqn>` (ordered).
3. **Sort the returned list lexicographically** by the full resolved identifier (absolute file path for `File`, `Fqn` for `Table`). This is the `15`-mandated determinism fence: provider ordering is not trusted to be stable across calls; `compile` sorts it explicitly.
4. **For each resolved identifier, produce one `PhysicalSource`:**
  - File → fetch format (inferred from extension when `extras.storage.format` is absent, else taken verbatim; §4) → fetch schema per §4's per-format strategy → extract `PartitionColumn`s from any Hive-style `key=value` segments in the resolved path → carry `partition_def` from the storage block (§3.5.4) → emit `File { ... }`.
  - Table → `CatalogProvider::load_table_metadata(fqn)` → emit `Table { ... }` (or `Snapshot { ... }` if an `at:` was present).
5. **Check the resolved list is non-empty per entry.** Empty → `CompileError::NoSourcesMatched { binding_id, pattern }` (§11).

Ordering is fully specified: step 3's lexical sort makes the `sources: Vec<PhysicalSource>` field of the Binding a deterministic function of `(pattern, catalog/filesystem snapshot)`. Re-running `compile` against the same pattern and the same underlying set yields the same order, byte-identical SemanticManifest (I4).

#### 3.5.7 Error model

- `CompileError::NoSourcesMatched { binding_id, pattern }` — the pattern produced zero matches. Fail-fast per `10 §3.3` / `30 §7`. Proposed code: `COMP_E_0301`.
- `CompileError::GlobExpansionFailed { binding_id, pattern, cause }` — the filesystem or catalog raised an I/O error during expansion. Surface the upstream error as `cause` (an `IntoDiagnostic`-compatible trait object). Proposed code: `COMP_E_0302`. Lives in the `COMP_E_0200-0299` sub-range (catalog/source resolution per `30 §6.2`) since the failure is at source-resolution time, not schema-assembly time — §11 maps these carefully.
- `CompileError::CatalogUnavailable { catalog_id, cause }` — the catalog whose `CatalogId` was named in the binding spec is not registered, not reachable, or returned an unexpected error outside the per-table fetch path. Proposed code: `COMP_E_0203`.

#### 3.5.8 Cross-source schema agreement within one Binding

When source resolution produces multiple `PhysicalSource`s (multiple author entries across `paths:` / `tables:`, or a wildcard expansion under one entry), their schemas MUST agree on every column that a Semantics references (full cross-source type-agreement rule is in §9.3). Soft agreement — a column exists in some sources but not all — is a `Coverage` question (per-source `Native` vs `NullFill`; §6). Hard agreement — a column exists in all sources but with different logical `DataType`s — is a compile error (`CompileError::CrossSourceTypeDisagreement`; §11).

This is one of the `15`-specific pitfalls that peers handle inconsistently: metricflow essentially forbids the pattern (one `data_source` is one table); Cube.js leaves it to the author via `rollup_join`; Iceberg handles schema evolution at the catalog level. `15`'s answer: the Binding is the atomic unit, so every source in it either provides the column (Native) or is explicitly missing it (NullFill); mixed types at the same name across sources is a Model bug.

### 3.6 Ordering and stability within the Binding

`Binding.sources` preserves the §3.5 step-3 lexical order. The planner reads sources by index when Unionset-style per-source branching is in play (`23`); the ordering is stable across any number of re-reads of the same SemanticManifest. The per-source `Coverage` is keyed on this index.

## 4. `FileFormat`

### 4.1 Enumeration

```rust
#[non_exhaustive]
pub enum FileFormat {
    Parquet,
    Csv(CsvOptions),
    Json(JsonOptions),
    Orc,
    Avro,
}
```

The v1 set covers the mainstream columnar + text formats. Adding a variant (e.g. `Delta`, `Iceberg` as a file-level format, `Arrow`) is MINOR per `30 §2`.

**Why `Parquet` and `Orc` have no `*Options` companion in v1.** They are self-describing: the schema is in the footer, and there are no per-read behavioral knobs that semstrait needs to expose on the Binding surface. Future read-option knobs (projection pushdown strategy, bloom-filter handling, compression selection) are adapter concerns (`36`) and do not propagate into `FileFormat`. If one ever needs to — for a per-Binding override — the variant can grow `Parquet(ParquetOptions)` as a MINOR.

**Why `Avro` has no `*Options` companion in v1.** Avro schemas are carried with the file (object-container format); there's nothing to declare at the Binding surface. If a schema registry integration lands later, the variant grows `Avro(AvroOptions)` as a MINOR.

### 4.2 `CsvOptions`

```rust
pub struct CsvOptions {
    pub has_header: bool,
    pub declared_schema: Option<Schema>,
    pub delimiter: u8,
    pub quote: u8,
    pub null_sentinel: Option<String>,
}
```

- `has_header` — when `true`, the first row is treated as column names (and the schema is read from it); when `false`, columns are named `_col0`, `_col1`, ... (positional). See §4.4 for the interaction with `declared_schema`.
- `declared_schema: Option<Schema>` — the author can supply the full schema inline in the Model. When present, it takes precedence over inferred or header-derived schemas (§4.4).
- `delimiter: u8` — default `b','`. Other common values: `b'\t'` for TSV, `b';'` for European CSV.
- `quote: u8` — default `b'"'`. Set to `b'\''` to enable single-quote strings.
- `null_sentinel: Option<String>` — a string value treated as NULL when reading. Default: empty string is NULL. `None` disables sentinel handling; an empty string is a legit empty-string value.

These are the minimum knobs observed in the peer set (`dbt` via `external_sources`, `duckdb`'s `read_csv_auto`, DataFusion's `CsvReadOptions`) that **cannot be ignored** without breaking the author's ability to describe the file. Every other CSV knob (date format, decimal separator, encoding) is a v2+ addition gated on concrete need.

### 4.3 `JsonOptions`

```rust
pub struct JsonOptions {
    pub shape: JsonShape,
    pub declared_schema: Option<Schema>,
    pub sample_rows: usize,
}

#[non_exhaustive]
pub enum JsonShape {
    Ndjson,
    JsonArray,
}
```

- `shape` — `Ndjson` is the streaming form (one JSON object per line, the common lake format); `JsonArray` is a single top-level array (the common REST-response form).
- `declared_schema` — same semantics as `CsvOptions`.
- `sample_rows` — how many records to scan when inferring a schema in the absence of `declared_schema`. Default: `100` (matches DataFusion and metricflow defaults).

### 4.4 Per-format schema-resolution strategy

The compile-time strategy per format:


| `FileFormat` | Primary source                   | Fallback                                                                                                                                                                     | Override | Result                                                                                                                                                                                                                                              |
| ------------ | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Parquet`    | File footer metadata             | —                                                                                                                                                                            | —        | Schema from footer; physical Arrow type → canonical `DataType` via a fixed mapping table owned by `semstrait-catalog`'s filesystem reader.                                                                                                          |
| `Csv(opts)`  | `opts.declared_schema` if `Some` | if `opts.has_header`, read header row and treat every column as `String` unless a declared type is overridden per column; else positional `_colN` with every column `String` | n/a      | Every CSV column is `String` unless `declared_schema` overrides. Downstream `Cast`s (§9.1) do the conversion.                                                                                                                                       |
| `Json(opts)` | `opts.declared_schema` if `Some` | sample `opts.sample_rows` records, infer per-field scalar types by widest-observed promotion, treat mixed types as `String`, record nullability as "any null observed"       | n/a      | Inferred schemas carry a `W` diagnostic — `COMP_W_0301 SchemaInferenceUsed` — advising authors to declare the schema explicitly for reproducibility (I4 is best-effort when inference is used; a different sample may infer different nullability). |
| `Orc`        | File footer metadata             | —                                                                                                                                                                            | —        | Same shape as Parquet.                                                                                                                                                                                                                              |
| `Avro`       | Object-container schema          | —                                                                                                                                                                            | —        | Direct schema from the container; logical type conversion done in `semstrait-catalog` per the same canonical mapping table as Parquet.                                                                                                              |


Inferred schemas (CSV without declared schema; JSON without declared schema) DEGRADE the I4 determinism guarantee because the "inferred" result depends on the actual bytes of the first N records. The design admission is explicit: **Binding output is deterministic w.r.t. a given catalog snapshot + filesystem snapshot**; if the bytes at the source change between runs, the schema can change. This is captured as `COMP_W_0301 SchemaInferenceUsed` and advised against for production Models.

**Proposed (Round 1):** JSON inference does not recurse into nested objects — only top-level scalar fields are typed; nested-object fields fall through as `String`. Array typing is not supported (arrays become `String`). Complex types (arrays, structs) are out of scope per `00 §10`. Authors needing nested JSON model the unnest explicitly in upstream jobs. See `questions/closed/15_questions.md` Q-MAP-004 (closed).

### 4.5 Format inference from path

When a Model's binding spec is a file glob without an explicit `format:`, the format is inferred from the file-extension suffix of each resolved absolute path:


| Extension (lower-cased)      | Inferred `FileFormat`                                                                                                                                                                                                    |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `.parquet`, `.pq`            | `Parquet`                                                                                                                                                                                                                |
| `.csv`, `.tsv`               | `Csv(CsvOptions::default())` (`tsv` forces `delimiter = b'\t'`)                                                                                                                                                          |
| `.json`, `.jsonl`, `.ndjson` | `Json(JsonOptions { shape: Ndjson, ... default })`; `.json` alone uses `shape: JsonArray` only if the top-level is a single `[`, otherwise falls through to `Ndjson` — this tie-breaker is the heuristic ratified for v1 |
| `.orc`                       | `Orc`                                                                                                                                                                                                                    |
| `.avro`                      | `Avro`                                                                                                                                                                                                                   |
| anything else                | `CompileError::UnrecognizedFileFormat { path }` (COMP_E_0303)                                                                                                                                                            |


Mixed-format globs are not supported: every path resolved from one glob must infer to the same format, or `CompileError::MixedFormatsInGlob { pattern }` fires (COMP_E_0304). The decision is ratified against the peer-group norm — metricflow and Cube both require format homogeneity per source.

## 5. `SemanticMapping`

### 5.1 Structure

The `SemanticMappingValue` enum roster, its `LiteralValue` payload, the `MetadataDimensionRecipe` / `MetadataExtraction` shapes, and the `auto` default are ratified in `[18 §10](./18_entities.md#10-semanticmapping-value-shape)` — v1 roster `{Column(String), Literal(LiteralValue), Expr(PhysicalExpr), Metadata(MetadataDimensionRecipe)}` (4 variants). `15` owns the container `SemanticMapping` (the per-Binding Semantics-name-keyed recipe table) and the compile-time semantics of each variant:

```rust
pub struct SemanticMapping {
    pub entries: BTreeMap<SemanticsName, SemanticMappingValue>,
}
// SemanticMappingValue: owned by `18 §10`.
```

`BTreeMap` keying is deliberate: it gives the Model-layer shape a deterministic iteration order (alphabetical on `SemanticsName`) which feeds straight into the `ResolvedExprTable` ordering ratified by `19 §3.4`. The SemanticManifest-layer `ResolvedColumnMapping` (§7) splits entries into per-category flat maps for O(1) lookup — the ordering fence is paid at SemanticManifest-construction time, not at plan time.

### 5.2 `SemanticMappingValue::Column` — compile semantics

The most common case: the Semantics value is a direct physical column reference. The variant carries a bare `String` (the physical column name) per `18 §10`:

- The string MUST resolve to a column in **every** `PhysicalSource` in the Binding whose `Coverage` for this Semantics is `Native` (§6). Sources with `NullFill` do not require the column.
- No per-source divergence in spelling: the column name is a single value; cross-source name-mapping is not supported at the Binding layer (it is a `Coverage` / NullFill question).
- The compile stage wraps a `Column(name)` whose physical `DataType` does not exactly match the Semantics's declared `data_type` with a `Cast` — the wrapped form is re-homed into `ResolvedColumnMapping.computed` per §9.1 rather than `ResolvedColumnMapping.columns`.

### 5.3 `SemanticMappingValue::Literal` — compile semantics

A typed constant per `18 §10.2`. The `LiteralValue` enum carries the type tag implicitly via its discriminant (`Null`, `Bool`, `Int`, `Float`, `Decimal`, `String`, `Date`, `Timestamp`).

**Literal validation at compile.** The `LiteralValue`'s discriminant is validated against the Semantics's declared `DataType` for representability:

- `LiteralValue::Int(i)` against a Semantics declared as `Byte` → validate `i ∈ [-128, 127]`; else `CompileError::LiteralOverflow { name, value, data_type }` (COMP_E_0305).
- `LiteralValue::Null` against a non-nullable declared Semantics → `CompileError::NullLiteralForNonNullableSemantics { name }` (COMP_E_0306).
- `LiteralValue::String(s)` against a Semantics declared as `Date`/`Timestamp` → requires RFC 3339 parse success; else `CompileError::LiteralParseFailed { name, value, target_type }` (COMP_E_0307).

### 5.4 `SemanticMappingValue::Expr` — compile semantics

An author-written expression, compiled to `PhysicalExpr` by `19 §3.3`:

- The wrapped `PhysicalExpr` honors the wrapper invariants from `14 §3.3`: no `EntityRef`, no `Aggregate`, `Column` allowed. Type inference (`14 §5`) has already run; `expr.inferred_type()` is populated.
- `expr.referenced_columns()` — every column name it reads must exist in every `PhysicalSource` in the Binding whose `Coverage` for this Semantics is `Derived` (§6). Sources with `NullFill` do not require the columns.

The YAML-to-`PhysicalExpr` compilation pathway for `Expr` entries:

```
YAML ExprSource  (14 §4)
   → SemanticExpr  (authors may write @other_semantics; 19 §3.3 substitutes them)
   → PhysicalExpr  (columns resolved against this Binding's PhysicalSource schemas)
```

The compile stage invokes `SemanticExpr::resolve` per `[19 §3.1](19_expression_flow.md)` per `Expr` entry. The binding context supplies (a) the cross-source reconciled schema over which `Column` identifiers resolve, (b) the `FunctionRegistry` (via `14a`), (c) the substitution map for typed-leaf identifiers into same-Binding `Expr` / `Column` entries. The output is a `PhysicalExpr` with `inferred_type` set; §9.1 then compares `inferred_type` against the declared Semantics `DataType` and emits a `Cast` at the Semantics boundary if needed.

**Cross-reference to `19 §3.5` cycle detection.** `Expr`-entries within a single `SemanticMapping` can refer to other Semantics via typed semantic leaves. The `19 §3.5` Tarjan-SCC pass runs over the Binding's `Expr` entries and detects same-Binding cycles (`e1 → e2 → e1`); the failure is `CompileErrorKind::CyclicReference` per `[19 §8.1](19_expression_flow.md)`.

### 5.5 `SemanticMappingValue::Metadata` — compile semantics

The 4th variant. v1 scope is **path-token extraction only**; partition-level extraction described in `13 §4.7` is deferred to v2 (see §8.0 v1-scope banner and §1's resolved-`[TD-MAP-METADATA-FOLD]` note).

**Authoring surface — Dimension type, not `semantic_mapping:`.** The author writes the recipe on the Dimension itself, in its `type:` block (per `13 §4.7` / `18 §4.2`):

```yaml
dimensions:
  - name: source_year
    data_type: string
    type:
      metadata:
        source:
          path:
            token: 1
```

There is **no author-side `semantic_mapping:` entry** for a metadata Dimension. The binding-resolution pass (`§10.4` step 4.0) detects each Dimension whose `type` is `Metadata(...)`, reads the `MetadataDimensionBody.source` shape (`18 §4`), and synthesizes the corresponding `SemanticMapping.entries[name] = SemanticMappingValue::Metadata(MetadataDimensionRecipe { extraction, data_type })` entry **before the completeness check (§5.6) runs**. A `semantic_mapping:` block that *does* contain an explicit entry for a metadata Dimension is rejected as a `SpuriousBindingEntry`-class error (or, equivalently, an authoring-layer parse error if `32` chooses to catch it earlier).

**Compile-time mechanic, not an expression.** Metadata extraction is layer 3 of the three-stratum model (banner, §1). It is **not** a `PhysicalExpr` variant, **not** a function in the `14a` registry, and **not** subject to `19 §3`'s substitution / cycle-detection passes. The recipe (`MetadataDimensionRecipe`) is a compile-output struct that pairs the extraction kind with the declared `data_type:`; its evaluation runs inside `compile` to produce the per-source `LiteralValue`s that feed `ResolvedPhysicalSource.metadata_values` (§7.6).

**Per-source resolution flow** (one pass per `(source_index, Metadata-bound semantics)` pair, executed during the Coverage-derivation pass §10.5):

1. Read the `MetadataDimensionRecipe.extraction` kind. v1: `Path { token }`.
2. Run the layer-3 mechanic for the source variant:
  - `PhysicalSource::File { path, .. }` → `path_token(path, token)` (§8.1) → returns a `String` (the raw segment) on success, or an error on out-of-range / empty (§8.1.2).
  - `PhysicalSource::Table { .. }` / `PhysicalSource::Snapshot { .. }` → not applicable for path extraction; coverage records `Metadata` (per §6) but no value is produced and the source is rejected at compile per the v1 fail-fast policy (`COMP_E_0312 MetadataTokenOnNonFileSource`).
3. Cast the extracted `String` to the declared `data_type:` per `MetadataDimensionRecipe.data_type`. Cast failure is a compile error (`COMP_E_0321 MetadataCastFailed`).
4. Wrap the cast result in the appropriate `LiteralValue` discriminant; insert into the source's `metadata_values: HashMap<SemanticsName, LiteralValue>` (§7.6).

**Cross-source value divergence is allowed.** The recipe is global to the Binding, but the **values may differ across sources** — that is the whole point of metadata extraction. The SemanticManifest stores one resolved `LiteralValue` per source (per metadata Dim); the planner reads the value for the source it is currently processing.

**Constant folding feed.** Layer-1 (`SemanticExpr`) consumers that reference a metadata-bound Semantic are eligible for partial evaluation at the `SemanticExpr → PhysicalExpr` lowering boundary: the metadata `LiteralValue` is known per source at compile time, so a `CASE WHEN @dataset_name = 'asd' THEN @col1 ELSE @col2 END` can be flattened to a per-source `Column(col1)` / `Column(col2)` projection. The partial-evaluation language is owned by `[19 §4.1](19_expression_flow.md)`; `15`'s contract is to make the per-source resolved literals available on `ResolvedPhysicalSource.metadata_values`.

Detailed runtime mechanics for `path_token`, exhaustively described in §8.1.

### 5.6 Completeness: coverage of the `SemanticInterface`

Per `11 §6`, a `SimpleDataKind`'s `SemanticInterface` is the complete named surface (Dimensions, Measures, Metrics, Filters, Keys). `15` ratifies the rule:

**Every Semantics name in the `SemanticInterface` MUST appear exactly once as a key in `SemanticMapping.entries`.**

- A name in the interface but missing from `entries` → `CompileError::MissingBindingEntry { semantics, binding_id }` (COMP_E_0308). Fail-fast.
- A name in `entries` but not in the interface → `CompileError::SpuriousBindingEntry { name, binding_id }` (COMP_E_0309). Fail-fast.
- A name duplicated in `entries` — the YAML parser rejects duplicate keys at a lower layer (`32`) via standard YAML duplicate-key handling; at the `15` level, `BTreeMap` is an unambiguous map and duplication is structurally impossible.

**Edge case: Semantics with a `Constraint` that derives its value (e.g. `Measure(Count, Key)` per `11 §8.4`).** Per `11 §8.4`, a count-like Measure declared with `Constraint::DerivesFrom(Key)` does not require a physical column; it counts the Key's rows. `SemanticMapping` still includes an entry for that Semantics — proposal: `SemanticMappingValue::Expr { expr: PhysicalExpr(Count(Column(<key_column>))) }` where the key column is the `SemanticMapping`'s entry for the referenced Key. The `compile` stage synthesizes the `Computed` entry from the `Constraint::DerivesFrom` spec — authors do not need to re-declare it. **Proposed (Round 1):** this is a compile-stage synthesis, not a YAML-surface convenience; the Model's authored `SemanticMapping` can omit the key-derived Measure entry, and the compile stage fills it in before the completeness check runs. See `questions/open/15_questions.md` Q-MAP-003.

**Edge case: `ComputedDimension` (per `14 §1.2`).** These ALWAYS map to `SemanticMappingValue::Expr`; they never have a `Column`-valued entry. The YAML parse enforces this at `32`.

**Edge case: Name case.** `SemanticsName` preserves the author's case (`14 §4.3`); the parser does no case folding. Mapping-key mismatches due to case errors (`"customer_id"` declared, `"CustomerID"` mapped) → `SpuriousBindingEntry` + `MissingBindingEntry` pair.

### 5.7 Shape constraints

- `SemanticMappingValue::Column` is the **common path**; it should account for the majority of entries in any real Model. Complex variants exist for the edge cases.
- `SemanticMappingValue::Expr` is for author-declared computed Semantics (`14 §1.2`) AND for the synthesized Measures from §5.6 and for the `Cast`-wrapped Column cases from §9.1. The cardinality of Computed entries should be bounded by the sum of authored-computed-Semantics + cast-wrapped Semantics in the `SemanticInterface`.
- `SemanticMappingValue::Metadata` is **always compile-synthesized** (§5.5) from a Dimension whose `type:` block carries a `metadata:` recipe (`13 §4.7` / `18 §4`); it has no author-facing `semantic_mapping:` YAML and is never authored directly. Cardinality is bounded by the number of metadata-typed Dimensions in the `SemanticInterface`. v1 scope: path-token only.
- `SemanticMappingValue::Literal` is a rarely-used fallback — it captures NullFill-style "this source does not have this column, use a constant" patterns, but the more common NullFill pattern uses `Coverage::NullFill` (§6) which does not need a Literal entry (the planner emits a `NULL` cast itself).

## 6. `Coverage` at Binding Level

### 6.1 Structure

When a Binding's `sources: Vec<PhysicalSource>` has cardinality > 1, not every source necessarily provides every Semantics. `Coverage` is the per-source × per-Semantics truth table that tells the planner "what to do for source *i*, Semantics *s*":

```rust
pub struct Coverage {
    pub entries: HashMap<CoverageKey, CoverageVariant>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CoverageKey {
    pub source_index: usize,
    pub semantics: SemanticsName,
}

#[non_exhaustive]
pub enum CoverageVariant {
    Native,
    NullFill,
    Derived,
    Metadata,
}
```

- `source_index` indexes into `Binding.sources` (0-based).
- `semantics` is the `SemanticsName` in the Binding's `SemanticMapping`.
- **Default when absent:** `Native`. The `HashMap` stores only the non-default entries (`NullFill`, `Derived`, `Metadata`). A missing entry means "this source provides this Semantics via the direct-column mechanism of the `SemanticMappingValue`". This is the common case and the storage is sparse.

Variants:

- `**Native`** — the source has a physical column (for `Column` / `Expr` variants whose referenced columns are all present) or the Semantics is a `Literal` (uniformly applicable across sources).
- `**NullFill**` — the source does NOT have the physical column(s) that `SemanticMappingValue::Column` / `::Expr` requires. At plan time, the planner emits a `Project[Cast(Null, declared_type) AS semantics]` at the branch covering this source (Unionset pattern; `23`). For non-Unionset consumers, `NullFill` on any source is a `CompileError::UnusableNullFillInNonUnionContext { binding_id, source_index, semantics }` (COMP_E_0310) — the v1 behavior: NullFill is meaningful only when the binding is consumed by a Unionset or Grainset constituent that tolerates it; in a bare Dataset / Joinset consumer, a NullFill source is a Model bug.
- `**Derived**` — the source has the *upstream* columns that an `Expr` expression needs, but not necessarily the direct Semantics column. Used for `Expr` entries whose expression's `referenced_columns` are all present on this source. (For simple `Column`-valued Semantics, `Derived` is indistinguishable from `Native` and is not used — the planner short-circuits to `Native`.)
- `**Metadata*`* — the Semantics's value comes from the source's metadata, eagerly resolved at compile via the layer-3 mechanic (§8). The actual `LiteralValue` lives on `ResolvedPhysicalSource.metadata_values[semantics]` (§7.6); the planner reads from that map rather than a column scan or expression evaluation. Used exclusively for `SemanticMappingValue::Metadata`-bound Semantics. Distinct from `Native` because the read path is not a column scan, distinct from `Derived` because no expression evaluation is involved, and distinct from `NullFill` because a value is present (compile fails fast if the metadata cannot be resolved — see §8.1.2 / §11). Used by the planner to know "skip column scan, read constant from per-source metadata map" — see `34 §5` for pushdown reasoning.

### 6.2 Computation at compile time

`Coverage` is populated during §10 step 5 of the resolution flow. The algorithm per `(source_index, semantics)`:

1. Look up `SemanticMapping.entries[semantics]` → `SemanticMappingValue`.
2. Dispatch on variant:
  - `Column(name)` — check `Binding.sources[source_index].schema.columns` for `name`. Present → `Native`. Absent → `NullFill`.
  - `Literal(_)` — always `Native`. (Literals do not depend on source schemas.)
  - `Expr(expr)` (author-written compute) — check every name in `expr.referenced_columns()` against the source's schema. All present → `Derived` (since the Semantics is computed, not directly present). Any missing → `NullFill`.
  - `Metadata(recipe)` — dispatch on `recipe.extraction`:
    - `Path { token }` — applicable iff `Binding.sources[source_index]` is `PhysicalSource::File`. For `Table` / `Snapshot` sources, fail-fast at compile with `COMP_E_0312 MetadataTokenOnNonFileSource` (path-token has no defined value for non-file sources in v1; this is a Model bug, not a per-source NullFill). On `File`: run the layer-3 `path_token` mechanic (§8.1) — out-of-range fails with `COMP_E_0311`, empty segment fails with `COMP_E_0320`, cast-to-`recipe.data_type` failure fails with `COMP_E_0321`. On success: emit `Metadata` and write the resolved `LiteralValue` into `ResolvedPhysicalSource.metadata_values[semantics]` (§7.6).
3. Persist the resulting `CoverageVariant` in `Coverage.entries` iff it is not `Native` (to keep storage sparse).

### 6.3 Derived-cross-source rule

When a Semantics maps to `Computed`, its `Coverage` on a specific source depends on whether the expression's column references are present on that source. The `Derived` variant encodes: "the upstream columns are here, the planner computes the Semantics on this source branch." The `NullFill` variant encodes: "the upstream columns are not here, the planner emits a NULL-filled constant of the declared Semantics type on this source branch."

**Proposed (Round 1):** `Derived` is a distinct variant from `Native` (rather than collapsing into `Native`) because consumers that care about provenance — notably the `16` composition layer building a `ComposedSemanticInterface` coverage map — need to distinguish "this is physically present as a column" (`Native` on a `Column`-valued Semantics) from "this is computed from upstream columns that happen to be present" (`Derived`). The distinction matters for pushdown reasoning in `34 §5`: Native reads are always pushdownable, Derived reads require pushing the computation. See `questions/closed/15_questions.md` Q-MAP-005 (closed).

### 6.4 Scope boundary with `16`

`15` owns `Coverage` at the **Binding** level — one Coverage per Binding, keyed on `(source_index, semantics)`. `16` owns `Coverage` at the **ComposedSemanticInterface** level — one Coverage per composition, keyed on `(constituent_datakind, composed_field)`.

The two are orthogonal. A Unionset of three Simple branches has:

- One `Coverage` per branch Binding (from `15`).
- One composition-level `Coverage` on the Unionset itself (from `16`), which records which constituent provides which field of the unified surface.

`15` does not speak about composition-level coverage; `16` uses `15`'s Binding coverage as an input when it builds the composition coverage (for each composed field, look up which constituent Binding has `Native` / `NullFill` for the underlying Semantics; this feeds the planner's per-branch NULL-fill emission in `23`).

## 7. SemanticManifest-Layer Counterpart: `ResolvedColumnMapping`

### 7.1 Motivation

The Model-layer `SemanticMapping` is a single `BTreeMap<SemanticsName, SemanticMappingValue>`. At plan time, the planner's hot-loop lookup pattern is "given a Semantics name, jump straight to the physical recipe." Matching the sum-type at every lookup is wasteful when the per-variant shape is known; a flat per-variant HashMap is faster and avoids the planner carrying an enum match in its hottest inner loop.

Per I8 and the `Resolved*` prefix convention (`00 §4.1`), the SemanticManifest stores `ResolvedColumnMapping`, a denormalized / pre-indexed form:

### 7.2 Structure

```rust
pub struct ResolvedColumnMapping {
    pub columns: HashMap<SemanticsName, ColumnName>,
    pub literals: HashMap<SemanticsName, ResolvedLiteral>,
    pub computed: HashMap<SemanticsName, PhysicalExpr>,
    pub metadata: HashMap<SemanticsName, MetadataDimensionRecipe>,
    pub source_coverage: HashMap<CoverageKey, CoverageVariant>,
}

pub struct ResolvedLiteral {
    pub value: LiteralValue,
    pub data_type: DataType,
}
```

The four top-level HashMaps are **disjoint**: a given `SemanticsName` appears in exactly one of them. The completeness rule (§5.6) is preserved: the union of their key sets equals the `SemanticInterface`'s name set.

`source_coverage` is the SemanticManifest-layer form of the §6 `Coverage` — same key/value shape, just promoted from an `Option<Coverage>` to a bare field (the `None` case is represented as an empty map; the per-source default is still `Native`).

`metadata: HashMap<SemanticsName, MetadataDimensionRecipe>` is the **per-Binding recipe table** for metadata-bound Semantics (`MetadataDimensionRecipe` shape per `18 §10.4`). The recipe is **global to the Binding** (extraction kind + declared `data_type:`); per-source resolved `LiteralValue`s live separately on each `ResolvedPhysicalSource.metadata_values` (§7.6) — the planner reads the recipe to know the kind, and the per-source map for the actual value of the source it is processing.

### 7.3 Construction

The compile stage produces `ResolvedColumnMapping` from the Model-layer `SemanticMapping` (the v1 4-variant roster per `18 §10`) via a single pass — each variant routes to its dedicated flat map:

```
for (semantics, value) in model.semantic_mapping.entries:
    match value:
        Column(name)        → resolved.columns.insert(semantics, name)
        Literal(lit)        → resolved.literals.insert(semantics, ResolvedLiteral::from(lit))
        Expr(expr)          → resolved.computed.insert(semantics, expr)
        Metadata(recipe)    → resolved.metadata.insert(semantics, recipe)
```

`expr` is the already-compiled `PhysicalExpr` (with `inferred_type` populated per §9.2) — no further expression work happens here. `recipe` is the already-synthesized `MetadataDimensionRecipe` from §5.5 / §10.4.0 (compile pass, before the completeness check). The per-source `LiteralValue`s for metadata Semantics are populated into each `ResolvedPhysicalSource.metadata_values` during the Coverage-derivation pass (§10.5), not here — `ResolvedColumnMapping.metadata` only carries the recipe.

**Cast-wrapped Column entries (`§9.1`)** flow into `resolved.computed` (not `resolved.columns`) because the cast wraps the `Column` reference into a `PhysicalExpr` — the model-layer `SemanticMapping` carries the rewritten `Expr` entry by the time §7.3's pass runs (§10.4 step 4 performs the rewrite before §10.6's materialization).

### 7.4 Planner access pattern

Plan-time per-Semantics lookup is per-(Binding, source) — the planner already knows which source it is processing (Unionset branch, Grainset child, etc.):

```rust
fn resolve_semantics<'a>(
    rb: &'a ResolvedBinding,
    src_idx: usize,
    s: &SemanticsName,
) -> SemanticsRecipe<'a> {
    if let Some(col)    = rb.column_mapping.columns.get(s)  { return SemanticsRecipe::Column(col); }
    if let Some(lit)    = rb.column_mapping.literals.get(s) { return SemanticsRecipe::Literal(lit); }
    if let Some(exp)    = rb.column_mapping.computed.get(s) { return SemanticsRecipe::Computed(exp); }
    if let Some(recipe) = rb.column_mapping.metadata.get(s) {
        // Per-source resolved value lives on the source itself.
        let value = &rb.sources[src_idx].metadata_values[s];
        return SemanticsRecipe::Metadata { recipe, value };
    }
    // Invariant: one of the above must hit, by compile-time completeness (§5.6).
    unreachable_by_invariant!("completeness guaranteed at compile")
}
```

Each branch is an O(1) HashMap probe. The metadata branch performs a second O(1) probe into the source's `metadata_values` map. The sum-type match on the Model-layer enum is never paid at plan time.

### 7.5 Relation to `19`'s `ResolvedExprTable`

`19 §3.2`'s `ResolvedExprTable` is a **SemanticManifest-global** map from `(SemanticsName, BindingId)` to `PhysicalExpr`. `ResolvedColumnMapping.computed` is a **per-Binding denormalization** of that table, filtered to the Binding's own entries. Both exist:

- The global table supports cross-Binding planner work (e.g. `19 §3.4.5` Relationship-path composition via `PathSignature`, where an expression is shared across a Joinset's members).
- The per-Binding HashMap supports single-Binding planner work (per-`Scan` expression lookup) without the extra `BindingId` in the key.

Whether the two share storage (the `ResolvedColumnMapping.computed` values are pointers into the global table) or are duplicated is a `33`-owned implementation choice. From `15`'s contract surface, both are populated and both are plan-readable; `33` will ratify the storage strategy. **Proposed (Round 1):** duplicate storage by default; the memory overhead is a small constant per binding-semantics pair, and the planner is free of aliasing concerns. See `questions/open/15_questions.md` Q-MAP-006.

### 7.6 `ResolvedBinding` envelope

The Binding's SemanticManifest-layer counterpart is:

```rust
pub struct ResolvedBinding {
    pub binding_id: BindingId,
    pub sources: Vec<ResolvedPhysicalSource>,
    pub column_mapping: ResolvedColumnMapping,
}

#[non_exhaustive]
pub struct ResolvedPhysicalSource {
    /// The resolved physical target. Same variants as `PhysicalSource` (§3.1).
    pub source: PhysicalSource,

    /// Per-source resolved metadata literals — one entry per metadata-bound
    /// Semantic in the Binding's interface. Populated during Coverage
    /// derivation (`§10.5`) by running the layer-3 mechanic (`§8`) plus
    /// `Cast` to the recipe's declared `data_type` (§5.5). Empty when the
    /// Binding has no metadata-typed Semantics. Keys are exactly the keys
    /// of `ResolvedColumnMapping.metadata`.
    pub metadata_values: HashMap<SemanticsName, LiteralValue>,
}
```

`ResolvedPhysicalSource` wraps the SemanticManifest-layer `PhysicalSource` (§3.1) and adds the per-source `metadata_values` map. The map is sized by the number of metadata-bound Semantics in the Binding's interface — `0` for Bindings that declare no metadata Dimensions; `N` for Bindings with `N` such Dimensions. **Values may differ across sources** (e.g. for a glob spanning `year=2024/...` and `year=2025/...` paths, the metadata-bound `source_year` Semantic resolves to different `LiteralValue`s on each source). The recipe in `ResolvedColumnMapping.metadata[name]` is constant across the Binding; only the resolved value varies.

Future manifest-layer optimizations (e.g. pre-computed partition pruning indices) evolve as MINOR per `30 §4`; the `#[non_exhaustive]` on `ResolvedPhysicalSource` allows additive growth.

## 8. `MetadataDimension` Semantics

> **§8.0 v1-scope banner — path-only.** The author-side body in `13 §4.7` carries both `path` and `partition` shapes for forward-compatibility:
>
> ```rust
> // 13 §4.7 (recap, author surface)
> pub struct MetadataDimension {
>     pub path: Option<PathExtraction>,
>     pub partition: Option<PartitionExtraction>,
> }
> ```
>
> v1's runtime / compile-mechanic surface is **path-only**. The compile-stage recipe synthesis (§5.5 / §10.4 step 4.0) only handles `MetadataExtraction::Path` (`18 §10.4`); a Dimension with `partition: Some(_)` is rejected at compile with `COMP_E_0322 MetadataPartitionDeferredV2` until v2 ratifies the partition arm. The §8.2 / §8.3 / §8.4 sections below are retained as **v2 design parking** — they describe the eventual partition-extraction mechanic, but no compile path implements them in v1.

This section is the **layer-3 compile-time mechanic** (banner, §1) for path-token extraction. Recap of the relationship across layers and docs:

- `13 §4.7` / `18 §4` — the author surface (`MetadataDimensionBody`, `MetadataSource`).
- `18 §10.4` — the compile-output struct shapes (`MetadataDimensionRecipe`, `MetadataExtraction`).
- `15 §5.5` — the compile-stage synthesis flow that converts the author body into a `SemanticMappingValue::Metadata(...)` entry.
- `15 §8` (this section) — the actual layer-3 mechanic that runs at compile to produce per-source `LiteralValue`s for `ResolvedPhysicalSource.metadata_values` (§7.6).

The mechanic is **not a `PhysicalExpr` variant** (it has no SQL-equivalent; it operates on metadata not row data) and **not a `14a` registry function** (it has no per-row evaluation; it runs once at compile per `(source, semantics)` pair). It lives in `15`'s implementation as a free function `path_token(path: &str, token: u32) -> Result<String, CompileError>` invoked during the Coverage-derivation pass (§10.5).

### 8.1 `path_token` — the v1 path extraction mechanic

Applicable to `PhysicalSource::File` variants only. Tokenization rule:

1. Take the source's resolved path (e.g. `"s3://bucket/year=2024/month=01/day=15/data.parquet"`).
2. Strip any scheme prefix (`"s3://"`, `"gs://"`, `"file://"`).
3. Split the remainder on `/` into non-empty segments. Leading `/` produces no empty-string token; consecutive `/` collapse to a single delimiter.
4. The 0-indexed segment array is the input to `path_token`.

For the example path, segments are:

```
0: "bucket"
1: "year=2024"
2: "month=01"
3: "day=15"
4: "data.parquet"
```

`path_token(path, 1)` → `"year=2024"` (the full segment, not the value after `=` — see §8.1.1).

**Local-path case.** `"/mnt/data/year=2024/month=01/file.parquet"` → `0: "mnt"`, `1: "data"`, `2: "year=2024"`, etc.

**Windows-style paths are NOT supported in v1** (explicit non-goal; the Model surface is lake-native).

#### 8.1.1 Token extraction result type — raw segment

`path_token` returns the raw segment as a `String` — the whole token, NOT the `=`-suffix value. For `"year=2024"`, the result is `"year=2024"`.

**Rationale.** Splitting on `=` would be an implicit second-parse step that fails silently on paths like `"year-2024"` or `"year"` (no `=` at all). Returning the whole segment keeps the layer-3 contract narrow; authors who need the value-only form compose it via a `SemanticMappingValue::Expr` Dimension that calls `substring_after(@source_segment, '=')` from the `14a` function catalog, where `@source_segment` is itself a metadata-bound Dimension carrying the raw segment. (Constant folding at the `SemanticExpr → PhysicalExpr` lowering boundary collapses this composition into a per-source `Literal` in practice; see §1's three-stratum note.)

#### 8.1.2 `Cast` to declared `data_type:` — fail-fast at compile

The recipe (`MetadataDimensionRecipe.data_type`) carries the Dimension's declared type. After `path_token` returns the raw `String`, the compile stage applies a `Cast` to the declared type:

- `data_type: String` → identity (no cast emission).
- `data_type: Integer` / `Long` / `Decimal` → parse the string as the numeric type. Parse failure → `COMP_E_0321 MetadataCastFailed`.
- `data_type: Date` / `Timestamp` → RFC 3339 parse. Parse failure → `COMP_E_0321`.
- `data_type: Bool` → `"true"` / `"false"` (case-insensitive); else `COMP_E_0321`.

Cast failures are **fail-fast at compile**, not deferred to plan or run time. The metadata literal is known at compile and so is its target type; failure to cast indicates the author wrote an incompatible declared type for the Dimension or the source path does not match the expected shape — both are Model bugs that the author must fix.

#### 8.1.3 Error conditions


| Code                                       | Trigger                                                                                                                                                                                                                                                         | Detail                                                                                                                                           |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `COMP_E_0311 MetadataTokenOutOfRange`      | `token >= segments.len()` for a given source's path.                                                                                                                                                                                                            | `{ binding_id, source_index, token_index, path }`. Fail-fast at compile (§10.5).                                                                 |
| `COMP_E_0312 MetadataTokenOnNonFileSource` | A metadata-bound Semantic with `Path { token }` extraction is in a Binding whose source at `source_index` is `Table` or `Snapshot`.                                                                                                                             | `{ binding_id, source_index, semantics }`. Path-token extraction has no defined value for non-file sources in v1 — fail-fast at compile (§10.5). |
| `COMP_E_0320 MetadataTokenEmpty`           | The segment at the requested index is empty (e.g. trailing `/` with no following content; should not occur after the §8.1 step-3 normalization, but the check is defensive against author misuse like `token: N` where `N` aligns with a degenerate component). | `{ binding_id, source_index, token_index, path }`. Fail-fast at compile.                                                                         |
| `COMP_E_0321 MetadataCastFailed`           | The raw `String` segment cannot be cast to the recipe's declared `data_type:`.                                                                                                                                                                                  | `{ binding_id, source_index, semantics, raw_segment, declared_type }`. Fail-fast at compile (§8.1.2).                                            |


All four checks run per-source during §10.5 (Coverage derivation); failures triggered immediately at compile, never at plan or run time.

### 8.2 `partition.level: N` extraction — DEFERRED TO v2

> **v2-deferred per §8.0.** The compile pass rejects `MetadataDimensionBody.source.partition: Some(_)` with `COMP_E_0322`. The text below is **design parking** — the eventual partition-extraction mechanic, retained for forward-compat reference. It does not run in v1.

Applicable to all `PhysicalSource` variants that declare `partitions: Vec<PartitionColumn>`.

Rule: 1-indexed. `level: 1` is the first partition column (`PartitionColumn.position == 1`); `level: N` is the *N*-th partition column.

For `PhysicalSource::File` with Hive-style path partitioning (`year=2024/month=01/day=15/data.parquet`), the compile-time source-resolution algorithm in §3.5.6 step 4 builds the `PartitionColumn` list from the path components. The partitions are ordered outer-to-inner in the path; `position: 1` = `year`, `position: 2` = `month`, etc.

For `PhysicalSource::Table` and `Snapshot` backed by Iceberg (or equivalent), the partition column list is the table's declared partition spec (the `default-spec-id` at the catalog level). Partition-transform identities (`identity`, `year`, `month`, `bucket[N]`) are carried on the catalog side (`37`) and are not surfaced on the `PartitionColumn` struct itself in `15`'s v1.

#### 8.2.1 Partition extraction result type — v2 design parking

The extracted value is the partition column's value for a given row, typed as the declared `PartitionColumn.data_type`:

- Hive-style path: typically `String` unless a `data_type` override is declared.
- Iceberg / Unity table: the declared partition-column `DataType` (which may be `Integer`, `String`, `Date`, etc.).

The exact result-type contract for Hive-style partitions (raw segment vs value-after-`=`, declared override grammar, type-inference fallback) is a v2 ratification item — see `questions/deferred/15_questions.md` Q-MAP-009 (deferred).

#### 8.2.2 Error conditions — v2 design parking

- `CompileError::MetadataPartitionUnavailable { binding_id, source_index, semantics, level }` — the source has no partition spec or fewer partition columns than `level`. Proposed code: `COMP_E_0313` (deferred until v2 ratifies the partition arm).
- `CompileError::InconsistentPartitioning { binding_id, sources: Vec<usize>, semantics }` — multi-source Binding where sources disagree on partition structure for referenced levels. Proposed code: `COMP_E_0314` (deferred until v2).

### 8.3 Cross-variant exclusion — v2 design parking

When the partition arm lands in v2, the YAML parser (`32`) will enforce `path.is_some() XOR partition.is_some()` at the `MetadataDimensionBody` author surface. v1 only has the `path` arm in scope, so the `partition: Some(_)` arm is rejected at compile (`COMP_E_0322`) regardless of the `path` arm's state.

### 8.4 `Metadata` Coverage uniformity — v1

Per §6.2, a metadata-bound Semantic on every applicable source emits `Coverage::Metadata` and writes its resolved `LiteralValue` into `ResolvedPhysicalSource.metadata_values[semantics]`. **There is no `NullFill` interaction in v1**: applicability is a fail-fast Model-correctness check (`COMP_E_0312` for `Path { token }` on a non-File source), not a per-source soft-fallback. A Binding that mixes file-glob and table sources with a path-token-bound Dimension fails at compile, period — it is not legal even inside a Unionset consumer.

The lenient "fill with NULL on non-applicable sources" pattern (the previous `8.4` behavior) was rejected because: (a) metadata extraction is a Model-correctness contract, not a per-row data-quality concern; (b) silently NULLing a metadata Semantic disrupts the constant-folding feed (§5.5) that is a core motivation for the recipe; and (c) when authors need cross-source heterogeneity in this dimension, the right tool is a separate Binding per source class composed by a Unionset, not implicit per-source NULL fallback.

## 9. Schema Reconciliation at Compile Time

### 9.1 Widening / narrowing / incompatible casts

When a Semantics declares `data_type: Integer` in its `SemanticInterface` and the physical `PhysicalSource.schema.columns[_].data_type` is `Long`, the compile stage reconciles the two per `14 §6.4`'s cast policy:


| Declared × Physical (per `14 §6.4` subset relevant here)             | Action                                                                                                                                                                                                                        | Diagnostic                                                                                                                                                                                 |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Same logical type                                                    | Pass-through.                                                                                                                                                                                                                 | —                                                                                                                                                                                          |
| Widening numeric (e.g. declared `Long`, physical `Integer`)          | Emit `PhysicalExpr(Cast(Column, declared_type))` on the fly at the Semantics boundary; the `SemanticMappingValue::Column` entry is rewritten to `SemanticMappingValue::Expr { expr: Cast(Column, declared) }` during compile. | `COMP_I_0301 ImplicitWideningCast` (info-level).                                                                                                                                           |
| Narrowing numeric (e.g. declared `Integer`, physical `Long`)         | Emit the same `Cast` wrapping as above.                                                                                                                                                                                       | `COMP_W_0302 ImplicitNarrowingCast` (warning-level, advises the author to double-check — narrowing a real `i64` that overflows `i32` is an engine-level runtime error, not a compile one). |
| Precision widening (Decimal → wider Decimal)                         | Emit `Cast`.                                                                                                                                                                                                                  | `COMP_I_0301`.                                                                                                                                                                             |
| Precision narrowing (Decimal → narrower Decimal)                     | Emit `Cast`.                                                                                                                                                                                                                  | `COMP_W_0302`.                                                                                                                                                                             |
| Float / Decimal cross-cast                                           | Emit `Cast`.                                                                                                                                                                                                                  | `COMP_W_0303 FloatDecimalCrossCast` — these casts can lose precision; always warn.                                                                                                         |
| `String` ↔ non-`String` (e.g. declared `Integer`, physical `String`) | No cast emitted; `CompileError::IncompatiblePhysicalType { semantics, declared, physical }` (COMP_E_0315).                                                                                                                    | Fail-fast.                                                                                                                                                                                 |
| `Date` / `Timestamp` ↔ non-temporal                                  | `CompileError::IncompatiblePhysicalType` (COMP_E_0315).                                                                                                                                                                       | Fail-fast.                                                                                                                                                                                 |
| `Binary` ↔ any non-`Binary`                                          | `CompileError::IncompatiblePhysicalType`.                                                                                                                                                                                     | Fail-fast.                                                                                                                                                                                 |


The full cast matrix lives in `14 §6.4`; §9.1 is the physical-to-semantic reconciliation slice of that matrix. `15` does not re-ratify the matrix; it sites the hook where the matrix is consulted.

**Where `Cast` lives.** After reconciliation, a `SemanticMappingValue::Column { name: "amount" }` over a physical `Long` with a declared Semantics `Integer` is rewritten to `SemanticMappingValue::Expr { expr: Cast(Column("amount"), DataType::Integer) }`. The SemanticManifest-layer `ResolvedColumnMapping.computed` stores the `Cast`-wrapped `PhysicalExpr`; no separate "cast-needed" flag exists. This keeps the planner's code path uniform (every non-literal/non-metadata Semantics is either a direct Column read or an expression evaluation).

### 9.2 `Computed`-entry type inference reconciliation

For `SemanticMappingValue::Expr { expr }`, the `expr.inferred_type()` (from `14 §5`) is compared to the declared Semantics `DataType`:


| Inferred × Declared | Action                                                                                                   |
| ------------------- | -------------------------------------------------------------------------------------------------------- |
| Same                | Pass-through.                                                                                            |
| Widening            | Wrap: `expr = Cast(expr, declared)`. Info-level diagnostic `COMP_I_0304 ImplicitWideningCastOnComputed`. |
| Narrowing           | Wrap: `expr = Cast(expr, declared)`. Warning-level `COMP_W_0305 ImplicitNarrowingCastOnComputed`.        |
| Incompatible        | `CompileError::ComputedTypeMismatch { semantics, inferred, declared }` (COMP_E_0316).                    |


The wrapping lives on the SemanticManifest-layer `PhysicalExpr` in `ResolvedColumnMapping.computed`.

### 9.3 Cross-source type agreement

A Binding's `sources` may comprise multiple `PhysicalSource`s, each with its own `schema`. A Semantics referencing a physical column `c` through `SemanticMappingValue::Column { name: c }` requires:

- **Every source where Coverage is `Native`** must have `c` in its schema.
- The `DataType` of `c` MUST be identical across all such sources.

If a source has `c` but with a different `DataType` than another source's `c`, the Binding is rejected:

- `CompileError::CrossSourceTypeDisagreement { binding_id, column, types }` (COMP_E_0317) — `types: Vec<(usize, DataType)>` enumerates the divergent cases per-source.

This is strict intentionally: a Binding that accepts `c: Integer` in source 0 and `c: Long` in source 1 would force every plan-time scan to decide which cast to apply per source, pushing the reconciliation into the hot path. Compile-time rejection keeps the post-SemanticManifest contract "one Semantics, one type per Binding."

For `Computed` entries with a cross-source `referenced_columns` set: the same strict rule applies — every column referenced by the expression must have the same logical `DataType` across every `Derived`-covered source.

### 9.4 Nullability reconciliation

The Semantics-level nullability (from `11 §6` / `14 §5.2`) is compared against the physical source's per-column `nullable: bool`:

- Declared Non-nullable + any source reporting nullable → `COMP_W_0306 NullableSourceForNonNullableSemantics` (warning, not error — the source may still contain no nulls in practice; the runtime engine will enforce). Advises the author to either relax the declared type or add a `filter: NOT NULL` in a wrapping Measure/Dimension.
- Declared Nullable + physical non-nullable → silent; this is always safe.

**Proposed (Round 1):** the nullability mismatch is a warning, not an error. Upgrading to an error is a v2 conversation; some authors have legitimate workflows where the source-reported nullability is conservative (Parquet marking `optional` for a column that is in practice always populated). See `questions/open/15_questions.md` Q-MAP-010.

### 9.5 Reconciliation site in `compile`

The reconciliation happens in §10 step 4 (SemanticMapping completeness + reconciliation), after the sources and their schemas are in place but before the Coverage pass (§10 step 5). Steps 2–3 build the cross-source reconciled schema view; step 4 consults it per-Semantics.

## 10. Compile-Time Resolution Flow

The binding-resolution flow lives inside the `compile` stage, between `10 §3.3`'s stage-level sub-steps. Per `10`'s breakdown, `compile` orchestrates catalog metadata fetch, source resolution, `ExprSource` compilation, and SemanticManifest-index construction; `15`'s flow is the sub-sequence specific to each Binding.

For each `SimpleDataKind` in the validated `SemanticModel`:

### 10.1 Step 1 — `BindingId` allocation

The `compile` driver assigns the next `BindingId` to the Binding. The counter increments. This step is a single arithmetic operation; it carries no cost.

### 10.2 Step 2 — Source resolution (glob expansion + catalog fetch)

Expand the Model's source spec per §3.5. Outcomes:

- Produce a `Vec<PhysicalSource>` with `sources.len() >= 1` (else `COMP_E_0301 NoSourcesMatched`).
- Every source has `schema` and `partitions` populated.
- Every catalog call is made (fail-fast on `CatalogUnavailable` / `SchemaFetchFailed`).
- `Snapshot`-variant sources pin their `snapshot_id` from the catalog's current-snapshot response.

This is the **I/O-heavy step**: the filesystem is hit per `FileSystem::expand_glob` call, the catalog per `CatalogProvider::load_table_metadata` call. Per I11, these are the only `await` points in `compile`; their async shape is ratified in `37`.

### 10.3 Step 3 — Schema resolution per source

Per source, resolve the schema through §4's per-format strategy:

- Parquet / ORC / Avro: read from file footer / container.
- CSV / JSON: use declared schema if present; else infer from header/sample; emit `COMP_W_0301 SchemaInferenceUsed` for inferred cases.
- Table / Snapshot: read from catalog metadata.

Populate `Schema.columns` (logical `DataType`s, nullability).

### 10.4 Step 4 — `SemanticMapping` well-formedness and reconciliation

1. **Metadata recipe synthesis (§5.5)** — runs FIRST, before the completeness check. For each Dimension whose `type:` is `Metadata(MetadataDimensionBody)` (per `13 §4.7` / `18 §4`):
  - **v2-deferred guard.** If `body.source.partition: Some(_)` → emit `COMP_E_0322 MetadataPartitionDeferredV2`. Path-only is in scope for v1.
  - Construct `SemanticMappingValue::Metadata(MetadataDimensionRecipe { extraction: Path { token }, data_type })` from the Dimension body. `data_type` comes from the Dimension's declared `data_type:` field.
  - Insert into `SemanticMapping.entries[name]`. If the author already authored an entry under that name (a misuse) → `COMP_E_0309 SpuriousBindingEntry` (the metadata recipe wins; the explicit author entry is rejected).
  - The synthesized entries are now part of the SemanticMapping seen by the completeness check below.
2. **Completeness check (§5.6):**
  - Missing Semantics → `COMP_E_0308 MissingBindingEntry`.
  - Spurious entry → `COMP_E_0309 SpuriousBindingEntry`.
3. **Per-entry variant dispatch:**
  - `Column`: validate the column exists on at least one source; if on NO source, `COMP_E_0310`-lite (actually `ColumnMissingOnAllSources { semantics, column }`, COMP_E_0318). Reconcile declared vs physical `DataType` per §9.1; wrap in `Cast` if widening/narrowing. Record cross-source type agreement per §9.3.
  - `Literal`: validate representability (§5.3 error list).
  - `Computed`: invoke `SemanticExpr::resolve` per `[19 §3.1](19_expression_flow.md)`. Use the cross-source reconciled schema as the column-lookup context. Check `expr.referenced_columns` across sources per §9.3. Reconcile `expr.inferred_type` vs declared Semantics `DataType` per §9.2.
  - `Metadata`: per-source applicability check only — actual value resolution happens in step 5 (Coverage derivation). For `Path { token }` extraction: every source in the Binding must be `PhysicalSource::File` (else `COMP_E_0312 MetadataTokenOnNonFileSource`); the Binding must have ≥1 source. The deeper checks (out-of-range token, empty segment, cast failure) run during the per-source pass in step 5.
4. **Synthesize compile-derived entries** for Constraints that require a `SemanticMappingValue::Expr` (§5.6 edge case — e.g. `Measure(Count, DerivesFrom(Key))`). These entries are added to the Model-layer `SemanticMapping` struct in-place during the reconciliation pass, then flow through step 4.3's Computed branch.

### 10.5 Step 5 — `Coverage` derivation

Per §6.2. For each `(source_index, semantics)` pair, compute the `CoverageVariant` and store in `Coverage.entries` iff not `Native`.

For metadata-bound Semantics (§5.5), this pass also performs the **per-source value resolution** that feeds `ResolvedPhysicalSource.metadata_values`:

1. Run the layer-3 mechanic (§8.1) — `path_token(source.path, recipe.extraction.token)`.
2. Cast the resulting `String` to `recipe.data_type` (§8.1.2).
3. Insert the resulting `LiteralValue` into the source's `metadata_values: HashMap<SemanticsName, LiteralValue>` map.
4. Emit `Coverage::Metadata` for the `(source_index, semantics)` pair.

Any failure (`COMP_E_0311`, `COMP_E_0312`, `COMP_E_0320`, `COMP_E_0321`) halts the Binding's resolution per §10.8 fail-fast posture. There is no per-source soft-fallback for metadata extraction in v1 (§8.4).

If any entry is `NullFill` and the Binding's owning `DataKind` is not a Simple constituent of a Unionset (or other NullFill-tolerant consumer), emit `COMP_E_0310 UnusableNullFillInNonUnionContext`. The consumer-tolerance check requires traversing up the Model's parent-DataKind reference; this traversal is the `compile` driver's job, not `15`'s — `15`'s Coverage-derivation pass just computes the per-source variant and flags NullFill entries to the driver.

### 10.6 Step 6 — `ResolvedBinding` / `ResolvedColumnMapping` materialization

Per §7. Build the flat HashMaps:

- `columns: HashMap<SemanticsName, ColumnName>` — populated from surviving `Column` entries.
- `literals: HashMap<SemanticsName, ResolvedLiteral>` — from `Literal`.
- `computed: HashMap<SemanticsName, PhysicalExpr>` — from `Expr` (native) + synthesized Cast-wrapped `Column` entries (§9.1) + synthesized derived-Measure entries (§5.6).
- `metadata: HashMap<SemanticsName, MetadataDimensionRecipe>` — from `Metadata` (the per-Binding recipe; per-source values are on each `ResolvedPhysicalSource.metadata_values`, populated in step 5).
- `source_coverage: HashMap<CoverageKey, CoverageVariant>` — from the Coverage-derivation pass.

Wrap each `PhysicalSource` into a `ResolvedPhysicalSource { source, metadata_values }` (§7.6) using the `metadata_values` maps populated in step 5. Attach to a `ResolvedBinding { binding_id, sources: Vec<ResolvedPhysicalSource>, column_mapping: ResolvedColumnMapping }`.

### 10.7 Step 7 — SemanticManifest-index contribution

The Binding's `ResolvedBinding` is handed off to the SemanticManifest-index-construction stage. The `ResolvedExprTable` (per `19 §3.4`) absorbs every Computed entry as `(SemanticsName, BindingId) → PhysicalExpr`. The per-DataKind Binding index is populated (`DataKindId → Vec<BindingId>` — vector length 1 for `SimpleDataKind`). These are `33`-owned SemanticManifest structures; `15`'s flow just feeds them.

### 10.8 Error-handling posture

The entire §10 flow is **fail-fast** (per `30 §7` / `10 §3.3`). A compile error in step 2 halts the whole Binding's resolution; subsequent steps are not attempted. Warnings accumulate throughout and are attached to the eventual `CompileError` or to the success `SemanticManifest`.

Some steps CAN collect multiple errors before returning (step 4's completeness check reports every missing / spurious entry in one pass before failing). Others are strictly fail-fast (step 2's glob expansion returns the first failure). The convention: structural well-formedness checks accumulate; I/O-bound and dependency-chain checks fail-fast.

### 10.9 Placement within `10 §3.3`

`10 §3.3` enumerates `compile`'s sub-steps as: **catalog fetch → glob expand → schema resolve → name resolve → ExprSource compile → index build**. `15 §10`'s per-Binding flow is a refinement: it enumerates the sub-structure of the "ExprSource compile → index build" cluster specifically for Binding work. The catalog-fetch and glob-expand sub-steps are shared across Bindings (a single catalog snapshot is taken for the whole compile invocation); `15 §10 step 2` calls into those shared resources per-Binding.

## 11. Error Model

### 11.1 Binding-owned `CompileError` variants

All compile-time error variants introduced or re-surfaced by `15`, with proposed stable codes per `30 §6.2`. The `COMP_E_0200-0299` sub-range (catalog/source resolution) hosts source-level errors; the `COMP_E_0300-0399` sub-range (schema/binding) hosts SemanticMapping / reconciliation errors.


| Code          | Variant                                                                                  | Sub-range | Trigger                                                                                                |
| ------------- | ---------------------------------------------------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------ |
| `COMP_E_0203` | `CatalogUnavailable { catalog_id, cause }`                                               | 0200–0299 | catalog whose `CatalogId` was named on the binding is unreachable / not registered                     |
| `COMP_E_0301` | `NoSourcesMatched { binding_id, pattern }`                                               | 0300–0399 | glob / table-glob produced zero matches                                                                |
| `COMP_E_0302` | `GlobExpansionFailed { binding_id, pattern, cause }`                                     | 0300–0399 | filesystem / catalog raised during glob enumeration                                                    |
| `COMP_E_0303` | `UnrecognizedFileFormat { path }`                                                        | 0300–0399 | file extension does not map to a known `FileFormat`                                                    |
| `COMP_E_0304` | `MixedFormatsInGlob { pattern }`                                                         | 0300–0399 | glob resolved to files with different inferred formats                                                 |
| `COMP_E_0305` | `LiteralOverflow { name, value, data_type }`                                             | 0300–0399 | `SemanticMappingValue::Literal` value does not fit the declared type                                   |
| `COMP_E_0306` | `NullLiteralForNonNullableSemantics { name }`                                            | 0300–0399 | `LiteralValue::Null` declared on a non-nullable Semantics                                              |
| `COMP_E_0307` | `LiteralParseFailed { name, value, target_type }`                                        | 0300–0399 | string literal failed to parse into target temporal type                                               |
| `COMP_E_0308` | `MissingBindingEntry { semantics, binding_id }`                                          | 0300–0399 | Semantics in interface, absent from `SemanticMapping`                                                  |
| `COMP_E_0309` | `SpuriousBindingEntry { name, binding_id }`                                              | 0300–0399 | `SemanticMapping` key not in Semantics                                                                 |
| `COMP_E_0310` | `UnusableNullFillInNonUnionContext { binding_id, source_index, semantics }`              | 0300–0399 | NullFill derived for a Binding whose owning DataKind is not tolerance-consumed                         |
| `COMP_E_0311` | `MetadataTokenOutOfRange { binding_id, source_index, token_index, path }`                | 0300–0399 | v1 path-token: token index ≥ segment count after scheme-strip (§8.1.3)                                 |
| `COMP_E_0312` | `MetadataTokenOnNonFileSource { binding_id, source_index, semantics }`                   | 0300–0399 | v1 path-token: metadata-bound Semantic in a Binding whose source is `Table` or `Snapshot` (§8.1.3)     |
| `COMP_E_0313` | `MetadataPartitionUnavailable { binding_id, source_index, semantics, level }`            | 0300–0399 | **DEFERRED to v2** (partition extraction non-goal in v1; §8.0 / §8.2.2). Reserved code; no v1 emitter. |
| `COMP_E_0314` | `InconsistentPartitioning { binding_id, sources, semantics }`                            | 0300–0399 | **DEFERRED to v2** (§8.0 / §8.2.2). Reserved code; no v1 emitter.                                      |
| `COMP_E_0315` | `IncompatiblePhysicalType { semantics, declared, physical }`                             | 0300–0399 | declared and physical `DataType` not cast-compatible per `14 §6.4`                                     |
| `COMP_E_0316` | `ComputedTypeMismatch { semantics, inferred, declared }`                                 | 0300–0399 | Computed expression's `inferred_type` not cast-compatible with declared Semantics type                 |
| `COMP_E_0317` | `CrossSourceTypeDisagreement { binding_id, column, types }`                              | 0300–0399 | a column has different logical types in different sources of one Binding                               |
| `COMP_E_0318` | `ColumnMissingOnAllSources { binding_id, semantics, column }`                            | 0300–0399 | `Column`-valued Semantics's physical column is absent from every source                                |
| `COMP_E_0319` | `SchemaFetchFailed { source, cause }`                                                    | 0300–0399 | format-specific schema-resolution step failed (Parquet footer unreadable, JSON sample I/O error, etc.) |
| `COMP_E_0320` | `MetadataTokenEmpty { binding_id, source_index, token_index, path }`                     | 0300–0399 | v1 path-token: extracted segment is empty (defensive against degenerate path components; §8.1.3)       |
| `COMP_E_0321` | `MetadataCastFailed { binding_id, source_index, semantics, raw_segment, declared_type }` | 0300–0399 | v1 path-token: raw segment cannot be cast to the recipe's declared `data_type` (§8.1.2)                |
| `COMP_E_0322` | `MetadataPartitionDeferredV2 { binding_id, semantics }`                                  | 0300–0399 | Dimension body declares `partition: Some(_)`; partition extraction is deferred to v2 per §8.0          |


### 11.2 Binding-adjacent warnings


| Code          | Variant                                                             | Trigger                                                       |
| ------------- | ------------------------------------------------------------------- | ------------------------------------------------------------- |
| `COMP_I_0301` | `ImplicitWideningCast { semantics, from, to }`                      | §9.1 widening-cast wrapping                                   |
| `COMP_W_0301` | `SchemaInferenceUsed { source }`                                    | CSV-without-declared / JSON-without-declared schema inference |
| `COMP_W_0302` | `ImplicitNarrowingCast { semantics, from, to }`                     | §9.1 narrowing-cast wrapping                                  |
| `COMP_W_0303` | `FloatDecimalCrossCast { semantics, from, to }`                     | §9.1 float/decimal cross cast                                 |
| `COMP_I_0304` | `ImplicitWideningCastOnComputed { semantics, from, to }`            | §9.2 widening on Computed                                     |
| `COMP_W_0305` | `ImplicitNarrowingCastOnComputed { semantics, from, to }`           | §9.2 narrowing on Computed                                    |
| `COMP_W_0306` | `NullableSourceForNonNullableSemantics { semantics, source_index }` | §9.4 nullability mismatch                                     |


### 11.3 Re-surfaced errors from `14` / `14a` / `19`

Errors raised by `14` / `14a` / `19` during Computed-entry compilation pass through `15`'s resolution flow unmodified; `15` does not re-codify them. Examples:

- `EXPR_E_0201 UnknownReference` (from `19 §3.3` substitution)
- `EXPR_E_0206 ComputedCycle` (from `19 §3.5`'s SCC pass)
- `EXPR_E_0401 TypeInferenceFailed` (from `14 §7`)
- `EXPR_E_0301 FunctionArityMismatch` (from `14a §8`)

These are reported by the owning doc's code ranges; `15` ensures its error-reporting context includes the `binding_id` where relevant (adapters of the `Diagnostic` location field fill this in).

### 11.4 Error location discipline

Every `15`-owned `CompileError` SHOULD carry a `Diagnostic.location: Option<Location>` pointing into the Model YAML source:

- Binding-shaped errors (`NoSourcesMatched`, `GlobExpansionFailed`, `CatalogUnavailable`) → point at the YAML `binding:` block or its `sources:` sub-key.
- SemanticMapping-shaped errors (`MissingBindingEntry`, `SpuriousBindingEntry`, `IncompatiblePhysicalType`) → point at the specific `semantic_mapping[<name>]:` entry.
- Reconciliation-shaped errors (`CrossSourceTypeDisagreement`, `CrossSourceTypeDisagreement`) → point at the Binding; the `types: Vec<(usize, DataType)>` field enumerates per-source divergence.

The precise `Location` / `ByteSpan` shape is ratified in `30 §5` and `32 §?` (SourceId variant). `15` stipulates only the semantic target.

### 11.5 Code-range governance

`15`'s allocation adds twenty-one `COMP_E_`* codes (`COMP_E_0203, 0301-0322`) and seven advisory codes (three `COMP_I_*`, four `COMP_W_*`). All are within the `COMP_E_0300-0399` / `COMP_W_0300-0399` reservation for "schema / binding (per `15`)" in `30 §6.2`. Three of the codes (`COMP_E_0313`, `COMP_E_0314`, `COMP_E_0322`) are partition-extraction-related — `0322` is emitted in v1 as the v2-deferral guard for `partition: Some(_)`; `0313` and `0314` are reserved (no v1 emitter) and will activate when the partition arm lands. Adding further `15`-owned codes is MINOR per `30 §6.3`; the sub-range has remaining space for roughly 47 additional codes before coming close to the `COMP_E_0400-0499` neighbor (relationships / index build).

The `CatalogUnavailable (COMP_E_0203)` code sits in the `COMP_E_0200-0299` catalog/source-resolution range per `30 §6.2`; `15` owns the schema/binding range but is a **consumer** of catalog-resolution errors, so it re-surfaces them at their owning range rather than re-numbering.

## 12. Interaction with Other Documents

### 12.1 `19 §3.2` — `ResolvedExprTable` keying

`19 §3.2` keys its global `ResolvedExprTable` on `(SemanticsName, BindingId)`. `15 §2.2` ratifies the `BindingId`'s shape, allocation, and uniqueness; the two docs are tightly coupled and should be read together. Every ratified property of `BindingId` in §2.2 is honored by `19`'s table construction; conversely, `19`'s requirement that its table's keys are stable-within-a-SemanticManifest is exactly the allocation discipline `15` ratifies.

### 12.2 `16` — Coverage at the `ComposedSemanticInterface` level

`16` extends `Coverage` from the Binding-level X-axis (per-source) to the composition-level axis (per-constituent-DataKind). Concretely:

- `15`'s `Coverage` answers "for Binding *B* with sources `[s0, s1, s2]`, which Semantics does `s1` cover natively vs NullFill?"
- `16`'s `Coverage` answers "for Unionset *U* with constituents `[D0, D1, D2]`, which fields of the composed interface does `D1` cover natively vs NullFill?"

`16` consumes `15`'s Coverage as input: for a composition-level "D1 covers field *f*" decision, `16` looks up the constituent's Binding's coverage of the underlying Semantics. The decision logic is `16`'s; the input data is `15`'s.

### 12.3 `20` — DataKind lifecycle integration

`20` ratifies how a `ResolvedDataKind` is constructed and where its `ResolvedBinding` attaches. `15 §10`'s flow produces a `ResolvedBinding`; `20`'s `compile` driver splices it into the `ResolvedDataKind::Simple` variant's `bindings: Vec<ResolvedBinding>` field (which, per `15 §2.1`, has length exactly 1).

### 12.4 `33` — SemanticManifest persistence of `ResolvedColumnMapping`

`33`'s crate contract ratifies the SemanticManifest struct shape, including the persisted form of `ResolvedBinding` / `ResolvedColumnMapping`. Serialization format (whether `serde_json`, a custom binary, or both) is `33`'s choice; `15` only stipulates that the structural shape in §7.2 is preserved round-trip (per I4).

The v1 design pencils in `serde` derivations on all `15`-ratified types (with the `serde` feature in `semstrait-core` per `31 §10`). The `PhysicalExpr` inside `ResolvedColumnMapping.computed` serializes through `14`'s `Expr` serialization (`31` / `14 §9`).

### 12.5 `37` — `CatalogProvider` integration

`15 §3`'s `CatalogRef` is an opaque handle into `37`'s `CatalogProvider` registry. `15 §3.5.6`'s step 2 makes the provider calls; the async posture and error-enum shape are ratified in `37`. `15`'s `CompileError::CatalogUnavailable` wraps `37`'s `CatalogError` via `IntoDiagnostic` (per `30 §5`).

### 12.6 `21`–`25` — Per-DataKind strategies consume Bindings

- `21 Dataset` — a Dataset (Simple) has exactly one Binding; its strategy is "scan the Binding's sources, read the `ResolvedColumnMapping`, emit a `Scan → Project` sub-plan."
- `22 Grainset` — levels resolve to child DataKinds; each level reads its child's Bindings. Grain-selection reads per-child Binding Coverage.
- `23 Unionset` — branches have Bindings; the per-branch NULL-fill is emitted based on Binding Coverage.
- `24 Joinset` — members have Bindings; the join path is composed via `16`'s Relationships; per-member scan plans read from member Bindings.
- `25 Applicability matrix` — the per-variant table explicitly notes which Binding properties are consumed by which strategy.

### 12.7 `10 §3.3` — Placement in `compile`

`15 §10` is the per-Binding sub-sequence of `10 §3.3`'s "source resolution → schema resolution → name resolution → expression compile → index build" pipeline. `15 §10.9` locates the sub-sequence precisely.

## 14. Non-Goals

Explicit non-goals of `15`. Authors searching for these topics should look elsewhere:

- **Query-time execution** — engines execute; semstrait emits; `15` compiles the input to `adapt`, nothing more.
- **Schema drift against the SemanticManifest** — drift detection is a **query-time** concern, covered by `CatalogProvider::check_schema_drift` per I11 and ratified in `37` / `38`. `15` freezes a schema at `compile` time; any post-compile drift is detected by a separate entry point.
- **Partition pruning** — the `Coverage` captured in `15` records per-source applicability; partition-predicate pushdown is a planner optimization covered in `34 §5`.
- **Per-engine register hooks** — `DataFusionConnector::register_manifest_sources` and peers are adapter-layer concerns (`36`). `15` produces a `ResolvedBinding`; the adapter reads it and decides how to register it.
- **Catalog-provider-specific snapshot semantics** — Iceberg REST snapshot pinning is described in `37`; `15` consumes the resulting `SnapshotId` as an opaque value.
- **Statistics-driven optimization** — `15` never surfaces row counts, histograms, or cardinality estimates. These are future planner concerns.
- **Write paths** — every `PhysicalSource` variant is read-oriented. Write-side semantics (materialized-view refresh, pre-aggregation persistence) are outside semstrait's mandate per `00 §10`.
- **Multi-format heterogeneous sources in one Binding** — a Binding's glob resolves to one format; mixed Parquet + CSV in one glob is rejected by `COMP_E_0304`. A Unionset of two Bindings, each with its own format, is the supported pattern.
- **Column renaming across sources** — a `SemanticMappingValue::Column { name }` resolves to a single physical name across every source. Per-source name mapping is a `Coverage` / NullFill question; semstrait's v1 answer is "rename upstream, in your ingestion job."

## 15. Summary of Vocabulary Anchors

For quick lookup when other docs reference `15`:


| Term                      | Shape (§ref)                                                                                    |
| ------------------------- | ----------------------------------------------------------------------------------------------- |
| `Binding`                 | struct §2.1                                                                                     |
| `BindingId`               | `struct(pub u32)` §2.2                                                                          |
| `PhysicalSource`          | `enum` File/Table/Snapshot §3.1                                                                 |
| `Schema`                  | struct with `Vec<SchemaColumn>` §3.2                                                            |
| `SchemaColumn`            | struct `{ name, data_type, nullable }` §3.2                                                     |
| `CatalogRef`              | struct `{ catalog_id, fqn }` §3.3                                                               |
| `PartitionColumn`         | struct `{ name, position (1-indexed), data_type, nullable }` §3.4                               |
| `FileFormat`              | `enum` Parquet/Csv/Json/Orc/Avro §4.1                                                           |
| `CsvOptions`              | struct §4.2                                                                                     |
| `JsonOptions`             | struct §4.3                                                                                     |
| `SemanticMapping`         | struct holding `BTreeMap<SemanticsName, SemanticMappingValue>` §5.1                             |
| `SemanticMappingValue`    | `enum` Column/Literal/Expr/Metadata §5.1 (struct-owned by `18 §10`)                             |
| `MetadataDimensionRecipe` | struct `{ extraction, data_type }` (`18 §10.4`)                                                 |
| `MetadataExtraction`      | `enum` Path { token: u32 } (v1; v2 adds Partition) (`18 §10.4`)                                 |
| `Coverage`                | struct holding `HashMap<CoverageKey, CoverageVariant>` §6.1                                     |
| `CoverageVariant`         | `enum` Native/NullFill/Derived/Metadata §6.1                                                    |
| `ResolvedBinding`         | struct §7.6                                                                                     |
| `ResolvedPhysicalSource`  | struct `{ source: PhysicalSource, metadata_values: HashMap<SemanticsName, LiteralValue> }` §7.6 |
| `ResolvedColumnMapping`   | struct with four flat HashMaps + coverage §7.2                                                  |
| `ResolvedLiteral`         | struct `{ value, data_type }` §7.2                                                              |
| `path_token`              | layer-3 compile-time mechanic (free fn): `(path: &str, token: u32) -> Result<String, _>` §8.1   |


Everything in this table is `pub`-visible in `semstrait-manifest` (post-resolve) or `semstrait-model` (pre-resolve), per `30 §4` / `33`'s final roster.

---

**End of document.** Open reconciliation items live in `docs/design/questions/open/15_questions.md`; ratified items in `docs/design/questions/closed/15_questions.md`; post-v1 items in `docs/design/questions/deferred/15_questions.md`.
