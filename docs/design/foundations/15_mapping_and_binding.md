---
prereqs: [00, 11, 13, 14, 14a, 14b]
authoritative-for:
  - the `Binding` struct shape (one per `SimpleDataKind`, `binding_id`, `sources`, `column_mapping`, `coverage`) and its identity / uniqueness rules
  - `BindingId` as a `u32` newtype, its allocation discipline, and `(DataKindId, BindingId)` global-uniqueness rule
  - the `PhysicalSource` sum type (`File`, `Table`, `Snapshot`) and the `Schema`, `PartitionColumn`, `CatalogRef` shapes it carries
  - the `FileFormat` enumeration (`Parquet`, `Csv`, `Json`, `Orc`, `Avro`) and the per-format schema-resolution strategy
  - glob-expansion algorithm, ordering determinism, and error model (no-match / catalog-unreachable / partial-schema)
  - the `ColumnMapping` structure and its value variants (`Column`, `Literal`, `Computed`, `Metadata`)
  - `ColumnMapping` completeness rule (every Semantics covered exactly once; extras rejected)
  - `Coverage` at Binding level (`Native`, `NullFill`, `Derived`); scope boundary with `16`'s composition-level Coverage
  - the Manifest-layer `ResolvedColumnMapping` shape (pre-split flat HashMaps per value variant) and its planner-speed contract
  - `MetadataDimension` extraction semantics (`path.token: N` 0-indexed; `partition.level: N` 1-indexed) and their error conditions
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
  - 33 (`semstrait-manifest` — persistence of `ResolvedBinding` / `ResolvedColumnMapping` on the `Manifest`)
  - 37 (`semstrait-catalog` — `CatalogRef`, `CatalogProvider`, `FileSystem` traits consumed during compile-time resolution)
---

# 15. Mapping and Binding

> **Status:** ratified. `15` fixes the shape and compile-time resolution of the link between a `SimpleDataKind`'s `SemanticInterface` and its physical backing — the `Binding`, the `ColumnMapping`, the `PhysicalSource` list, and the `Coverage` recorded at the Binding level. All concrete structures, error variants, and flow steps described below are authoritative for `3x` API docs and for the per-DataKind-variant strategy docs `20`–`25`. Open reconciliation items are parked in `docs/design/open_questions/15_open_questions.md`.

## 1. Purpose and Scope

### 1.1 What `15` ratifies

`15` is the foundations document that closes the loop between the **Semantics-facing** surface of a `SimpleDataKind` (named and shaped by `11`, typed by `13`, expressed by `14` / `14a` / `14b`) and the **physical-facing** surface underneath it: the files, tables, or snapshots that actually hold the data, the schemas those targets expose, and the recipe (direct column / literal / computed / metadata-extraction) that produces each Semantics value from whatever rows the physical target returns.

Everything that sits **below the `SemanticInterface` boundary for a single `SimpleDataKind`** is authoritative here:

- The `Binding` — the single-instance join between Semantics and Physical, owned by exactly one `SimpleDataKind`.
- The `PhysicalSource` roster — the 1-or-more resolved targets the Binding points at (a file path, a catalog table, an Iceberg snapshot).
- The `ColumnMapping` — the Semantics-name-keyed recipe table.
- The per-source `Coverage` — which Semantics each source actually provides.
- The Manifest-layer counterpart `ResolvedColumnMapping` — the flattened, pre-indexed form the planner consumes at query time.
- The compile-time resolution flow that moves a Model-level binding declaration into a `ResolvedBinding` living on a `ResolvedDataKind`.
- The `MetadataDimension` extraction mechanics (`path.token`, `partition.level`) introduced structurally in `13 §4.7`; `15` pins down the runtime recipe and the error conditions.

### 1.2 What `15` does NOT ratify

- **`ComplexDataKind` composition.** `Unionset`, `Grainset`, and `Joinset` do not carry their own `Binding`s; they aggregate the `Binding`s of their constituent `SimpleDataKind`s. The composition mechanics, `ComposedSemanticInterface` shape, and per-composition `Coverage` (which constituent provides each field on the unified surface) all live in `foundations/16_composition.md`. `15` is explicit about the boundary in §6.4.
- **DataKind lifecycle.** How `ResolvedBinding`s attach to `ResolvedDataKind`s during `compile`, and the post-compile guarantees about their ordering inside the `Manifest`, are ratified in `foundations/20_taxonomy.md` and the crate contract in `apis/33_semstrait_manifest.md`. `15 §10` enumerates the steps inside `compile` that produce a `ResolvedBinding`; it does not ratify their position in the `compile` driver.
- **Expression resolution.** `ColumnMappingValue::Computed { expr: PhysicalExpr }` stores a compiled `PhysicalExpr`. The substitution algorithm that produces that `PhysicalExpr` from a `SemanticExpr` (via the `FunctionRegistry` in `14a` and the cross-DataKind walk in `14b`) is owned by `14b`. `15 §5.3` describes only the **storage site** and the **wrapper-invariant contract** the stored `PhysicalExpr` must satisfy.
- **Catalog-provider shape.** The `CatalogProvider` trait surface, the `FileSystem` trait surface, their async posture, and their error enums are ratified in `apis/37_semstrait_catalog.md`. `15 §3.2` uses `CatalogRef` as an **opaque handle** into that surface; consumers of `15` never reach into the catalog crate directly.
- **Per-engine dialect specifics.** Nothing in `15` branches on engine identity (I3). A `PhysicalSource` carries a logical `DataType`-bearing `Schema`; conversion to an engine-specific type is adapter territory (`36`, I2).

### 1.3 Design posture

`15`'s posture is **compile-once, store-flat, plan-fast**:

- **Compile-once.** Every glob is expanded, every catalog table is fetched, every physical schema is reconciled against every declared `DataType`, every `SemanticExpr` on the semantic side is compiled into a `PhysicalExpr` over columns/literals, every per-source coverage bit is decided — all before the `Manifest` is sealed. I5 demands it, I8 requires it to make plan-time O(1), and I4 demands it reproducibly.
- **Store-flat.** The Model-layer `ColumnMapping` is an enum-valued map keyed by `SemanticsName`. The Manifest-layer `ResolvedColumnMapping` splits it into four parallel flat maps — one per variant — so the planner's per-Semantics lookup is a single HashMap probe with no enum match in the hot path (I6).
- **Plan-fast.** The planner never re-resolves anything in `15`'s scope. It reads `ResolvedBinding`, picks sources based on Coverage (Unionset §6.1; Grainset `17`), and emits `PlanNode`s. No catalog call, no filesystem call, no expression compilation.

### 1.4 Reference implementations — where `15` sits in the peer-group landscape

The brief is: pick a name for every concept we can't avoid having, and resist importing vocabulary that would re-open ratified decisions. Peers:

- **dbt metricflow.** `data_source.sql_table` / `data_source.sql_query` + `identifiers` + `measures` + `dimensions` — a single model-side block that couples a physical table to a set of Semantics. `15`'s `Binding` is the direct analog. metricflow has no analog for `MetadataDimension` (it does not pattern-match on S3 paths); it relies on the upstream warehouse to have the data already in shape. `15` keeps `MetadataDimension` because semstrait compiles over lake-native paths with partition-encoded metadata (`year=.../month=...`).
- **Cube.js.** `cube.sql_table` / `cube.sql` + `dimensions` + `measures` + `segments` + `pre_aggregations`. The `cube.sql` escape hatch is raw SQL; I1 forbids that here, so `ColumnMappingValue::Computed` carries a typed `PhysicalExpr` instead. Cube's `partition_granularity` is a roll-up concept ratified here by `Grain` + per-source `Coverage`, not by a Binding-level knob.
- **LookML.** `view.sql_table_name` + `view.derived_table` + per-dimension `sql:`. LookML's `${TABLE}.column` and `${other_view.field}` patterns are what `SemanticExpr` bare identifiers replace (resolved per `14 §4.3` / `14b`). LookML's `sql_trigger` / PDT machinery is outside `15`'s scope entirely.
- **Iceberg catalog.** The `Snapshot` variant of `PhysicalSource` is directly inspired by Iceberg's snapshot model — `metadata.current-snapshot-id` pins reproducibility (I4) against a moving warehouse state. Iceberg's partition-transform vocabulary (`year`, `month`, `day`, `hour`, `bucket[N]`, `truncate[N]`) informs `PartitionColumn` but is **not** replicated on it — partition-transform awareness is a planner concern (pruning), not a Binding concern.

The peers supply structural precedent and error-case nudges; nothing in the peer set overrides the semstrait vocabulary ratified in `00 §4`.

### 1.5 Guardrails — how `15` upholds `00 §9` invariants

| Invariant | Where `15` keeps it |
|---|---|
| **I1** — no raw SQL in canonical layer | `ColumnMappingValue::Computed` carries `PhysicalExpr` (an `Expr` tree), never a string. The YAML surface for computed entries parses through `14 §4` into `SemanticExpr`, then compiles to `PhysicalExpr` per `14b`. |
| **I2** — physical types via adapters only | `PhysicalSource.schema.columns[_].data_type` is logical `DataType` (per `13 §2`). The `14a` registry's promotion lattice is what decides widen/narrow; no Arrow/Spark/DuckDB type leaks into the Manifest. |
| **I3** — no engine branching in canonical layer | `PhysicalSource` is engine-agnostic. No variant, no field, no error code in `15` names an engine. Adapters read `PhysicalSource` at `adapt` time and decide how to register it with their engine (`36`). |
| **I4** — Manifest is deterministic | Glob expansion sorts by lexical order of resolved absolute identifier (§3.5). Catalog-fetch results are sorted by fully-qualified name before being folded into the Manifest. Ties are impossible by construction. |
| **I5** — resolution is compile-time | All Binding work (glob expansion, catalog fetch, schema resolution, ColumnMapping well-formedness, PhysicalExpr compilation, Coverage derivation) happens in `compile`. Plan-time reads only `ResolvedColumnMapping` and a pre-sorted `ResolvedPhysicalSource` list. |
| **I8** — Manifest is planner-complete | The Manifest stores `ResolvedBinding`, `ResolvedColumnMapping`, and the per-source `ResolvedPhysicalSource` list. No planner step re-fetches a schema, re-expands a glob, or re-compiles a Computed expression. |
| **I10** — non-exhaustive public sum types | `PhysicalSource`, `FileFormat`, `ColumnMappingValue`, `CoverageVariant`, `CompileError` (all `15`-owned variants) carry `#[non_exhaustive]`. Adding a new file format, a new coverage variant, a new binding-error kind is MINOR per `30 §2`. |

I6 / I11 apply transitively — `15` describes the compile-time surface, which is the only place async / I/O is allowed per I11; the Manifest forms that `15` produces is then consumed synchronously by the planner.

## 2. The `Binding`

### 2.1 Structure

A `Binding` is the single join point between a `SimpleDataKind`'s `SemanticInterface` and its physical backing. Every `SimpleDataKind` owns **exactly one** `Binding`; this is an invariant at both the Model layer (the YAML parser rejects multiple binding blocks on one kind per `11 §5.3`) and the Manifest layer (the compile-time materialization produces a single `ResolvedBinding` per `ResolvedDataKind::Simple`). `ComplexDataKind`s carry **no** `Binding` of their own — they aggregate the `Binding`s of their constituent Simple children, through the `ComposedSemanticInterface` machinery in `16`.

Model-layer shape:

```rust
#[non_exhaustive]
pub struct Binding {
    pub binding_id: BindingId,
    pub sources: Vec<PhysicalSource>,
    pub column_mapping: ColumnMapping,
    pub coverage: Option<Coverage>,
}
```

Every field is populated by `compile`; the Model-layer YAML surface in `semstrait-model` uses a parallel `BindingSpec` type whose fields are unresolved (glob patterns, declarative `SourceRef`s, and `ExprSource` values inside `ColumnMappingSpec`). The resolution flow in §10 consumes a `BindingSpec` and produces a `Binding` (and, by the time it lands in the Manifest, a `ResolvedBinding` — §7).

The `#[non_exhaustive]` tag is present to allow a future `post_binding_hook: Option<PhysicalExpr>` or similar Semantics-adjacent extension to be added as a MINOR per `30 §4`.

### 2.2 `BindingId`

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct BindingId(pub u32);
```

`BindingId` is the compile-time assigned identifier for a Binding. The canonical definition lives in `14b §2`; `15` ratifies its allocation discipline:

- **Allocation site.** Bindings are assigned their ID during the `compile` stage's binding-resolution pass (§10 step 1). The `compile` driver owns a monotonically-increasing `u32` counter, handed out in iteration order over the deterministic `ResolvedDataKind` roster. First ID is `0`; there is no `NULL` / `UNDEFINED` value.
- **Uniqueness scope.** `BindingId` is **unique within a Manifest**, not within a `DataKind`. The compile stage's counter spans the entire Manifest; every Binding across every `SimpleDataKind` gets a distinct `BindingId`.
- **`(DataKindId, BindingId)` uniqueness.** Because a `SimpleDataKind` owns exactly one `Binding`, `(DataKindId, BindingId)` is vacuously unique: given a `DataKindId`, the mapping to `BindingId` is a function (exactly one value). Manifest indices may key on either half.
- **Stability across `compile` invocations.** `BindingId` is **stable within a single `compile` run and NOT stable across runs.** Recompiling a Model (even a byte-identical Model) produces the same IDs if and only if the `compile` driver's iteration order over `ResolvedDataKind`s is deterministic (I4). Adding a `SimpleDataKind` anywhere earlier in the iteration order shifts every later Binding's ID — which is intended. Cross-Manifest Binding comparison by ID is not supported; the planner keys on `(DataKindId, BindingId)` and never on `BindingId` alone against another Manifest.
- **Serialization.** The `u32` surfaces directly in the persisted Manifest form (`33`). Serialized `Manifest`s that round-trip through `Repository::store` and `Repository::load` preserve the IDs unchanged; round-tripping is NOT a re-`compile` and ID stability is preserved by structural equality, not by reconstruction.

**Proposed (Round 1):** the counter resets to `0` per Manifest (per-compile scope). A cross-Manifest namespace (e.g. embedding the Manifest content hash into the ID) is not adopted; it would break the `u32` shape and have no concrete use case. See `open_questions/15_open_questions.md` Q-MAP-001.

### 2.3 Cross-reference: `14b`'s `ResolvedExprKey`

`14b §2`'s `ResolvedExprTable` is keyed on `(SemanticsName, BindingId)` precisely because `15` ratifies `BindingId` as the per-Binding identity. `15 §7.2` describes the Manifest's storage split: the `ResolvedExprTable` stores the physical expression bodies; the `ResolvedColumnMapping.computed: HashMap<SemanticsName, PhysicalExpr>` is a **per-Binding denormalization** that copies the `PhysicalExpr` into the Binding's own hashmap for O(1) access without going through the global table. Whether the Manifest stores the `PhysicalExpr` once (in the table) with the `ResolvedColumnMapping.computed` value being a pointer/index, or twice (once in each structure), is an implementation choice ratified in `33`. From the contract surface of `15`, both structures are populated and both are plan-readable.

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

**`nullable` source of truth.** Logical nullability is read from the physical source's metadata when available (Parquet: the `FieldType.required`/`optional` markers; Iceberg: `required: bool` per schema field; CSV/JSON: always `true` unless a declared schema overrides). It is not inferred from the data.

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

### 3.5 Glob expansion

The Model's YAML-surface binding can express its source as:

- a concrete file path (`"s3://bucket/data/2024-01/customers.parquet"`) → produces one `PhysicalSource::File`.
- a glob pattern (`"s3://bucket/data/year=*/month=*/*.parquet"`) → produces 1..N `PhysicalSource::File`s.
- a concrete table FQN (`"iceberg.sales.transactions"`) → produces one `PhysicalSource::Table` (or `Snapshot` if `at: { snapshot_id: ... }` is present).
- a table-name glob (`"iceberg.sales.*_transactions"`) → produces 1..N `PhysicalSource::Table`s.

#### 3.5.1 Expansion algorithm

The deterministic algorithm (I4) is:

1. **Classify the source spec** into `File` / `Table` based on the YAML surface key (Model parser, not `15`). `Snapshot` is produced only when the Table spec carries an `at:` subkey.
2. **If it contains a glob metacharacter** (`*`, `?`, `[`), call the respective provider to enumerate:
   - File sources → `FileSystem::expand_glob(pattern) → Vec<String>` (ordered).
   - Table sources → `CatalogProvider::list_tables(namespace, pattern) → Vec<Fqn>` (ordered).
3. **Sort the returned list lexicographically** by the full resolved identifier (absolute file path for `File`, `Fqn` for `Table`). This is the `15`-mandated determinism fence: provider ordering is not trusted to be stable across calls; `compile` sorts it explicitly.
4. **For each resolved identifier, produce one `PhysicalSource`:**
   - File → fetch format (inferred from extension or declared; §4) → fetch schema per §4's per-format strategy → extract `PartitionColumn`s from the Hive-style path components of the matched identifier → emit `File { ... }`.
   - Table → `CatalogProvider::load_table_metadata(fqn)` → emit `Table { ... }` (or `Snapshot { ... }` if an `at:` was present).
5. **Check the resolved list is non-empty.** Empty → `CompileError::NoSourcesMatched { binding_id, pattern }` (§11).

Ordering is fully specified: step 3's lexical sort makes the `sources: Vec<PhysicalSource>` field of the Binding a deterministic function of `(pattern, catalog/filesystem snapshot)`. Re-running `compile` against the same pattern and the same underlying set yields the same order, byte-identical Manifest (I4).

#### 3.5.2 Error model

- `CompileError::NoSourcesMatched { binding_id, pattern }` — the pattern produced zero matches. Fail-fast per `10 §3.3` / `30 §7`. Proposed code: `COMP_E_0301`.
- `CompileError::GlobExpansionFailed { binding_id, pattern, cause }` — the filesystem or catalog raised an I/O error during expansion. Surface the upstream error as `cause` (an `IntoDiagnostic`-compatible trait object). Proposed code: `COMP_E_0302`. Lives in the `COMP_E_0200-0299` sub-range (catalog/source resolution per `30 §6.2`) since the failure is at source-resolution time, not schema-assembly time — §11 maps these carefully.
- `CompileError::CatalogUnavailable { catalog_id, cause }` — the catalog whose `CatalogId` was named in the binding spec is not registered, not reachable, or returned an unexpected error outside the per-table fetch path. Proposed code: `COMP_E_0203`.

#### 3.5.3 Cross-source schema agreement within one Binding

When glob expansion produces multiple `PhysicalSource`s, their schemas MUST agree on every column that a Semantics references (full cross-source type-agreement rule is in §9.3). Soft agreement — a column exists in some sources but not all — is a `Coverage` question (per-source `Native` vs `NullFill`; §6). Hard agreement — a column exists in all sources but with different logical `DataType`s — is a compile error (`CompileError::CrossSourceTypeDisagreement`; §11).

This is one of the `15`-specific pitfalls that peers handle inconsistently: metricflow essentially forbids the pattern (one `data_source` is one table); Cube.js leaves it to the author via `rollup_join`; Iceberg handles schema evolution at the catalog level. `15`'s answer: the Binding is the atomic unit, so every source in it either provides the column (Native) or is explicitly missing it (NullFill); mixed types at the same name across sources is a Model bug.

### 3.6 Ordering and stability within the Binding

`Binding.sources` preserves the §3.5 step-3 lexical order. The planner reads sources by index when Unionset-style per-source branching is in play (`23`); the ordering is stable across any number of re-reads of the same Manifest. The per-source `Coverage` is keyed on this index.

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

| `FileFormat` | Primary source | Fallback | Override | Result |
|---|---|---|---|---|
| `Parquet` | File footer metadata | — | — | Schema from footer; physical Arrow type → canonical `DataType` via a fixed mapping table owned by `semstrait-catalog`'s filesystem reader. |
| `Csv(opts)` | `opts.declared_schema` if `Some` | if `opts.has_header`, read header row and treat every column as `String` unless a declared type is overridden per column; else positional `_colN` with every column `String` | n/a | Every CSV column is `String` unless `declared_schema` overrides. Downstream `Cast`s (§9.1) do the conversion. |
| `Json(opts)` | `opts.declared_schema` if `Some` | sample `opts.sample_rows` records, infer per-field scalar types by widest-observed promotion, treat mixed types as `String`, record nullability as "any null observed" | n/a | Inferred schemas carry a `W` diagnostic — `COMP_W_0301 SchemaInferenceUsed` — advising authors to declare the schema explicitly for reproducibility (I4 is best-effort when inference is used; a different sample may infer different nullability). |
| `Orc` | File footer metadata | — | — | Same shape as Parquet. |
| `Avro` | Object-container schema | — | — | Direct schema from the container; logical type conversion done in `semstrait-catalog` per the same canonical mapping table as Parquet. |

Inferred schemas (CSV without declared schema; JSON without declared schema) DEGRADE the I4 determinism guarantee because the "inferred" result depends on the actual bytes of the first N records. The design admission is explicit: **Binding output is deterministic w.r.t. a given catalog snapshot + filesystem snapshot**; if the bytes at the source change between runs, the schema can change. This is captured as `COMP_W_0301 SchemaInferenceUsed` and advised against for production Models.

**Proposed (Round 1):** JSON inference does not recurse into nested objects — only top-level scalar fields are typed; nested-object fields fall through as `String`. Array typing is not supported (arrays become `String`). Complex types (arrays, structs) are out of scope per `00 §10`. Authors needing nested JSON model the unnest explicitly in upstream jobs. See `open_questions/15_open_questions.md` Q-MAP-004.

### 4.5 Format inference from path

When a Model's binding spec is a file glob without an explicit `format:`, the format is inferred from the file-extension suffix of each resolved absolute path:

| Extension (lower-cased) | Inferred `FileFormat` |
|---|---|
| `.parquet`, `.pq` | `Parquet` |
| `.csv`, `.tsv` | `Csv(CsvOptions::default())` (`tsv` forces `delimiter = b'\t'`) |
| `.json`, `.jsonl`, `.ndjson` | `Json(JsonOptions { shape: Ndjson, ... default })`; `.json` alone uses `shape: JsonArray` only if the top-level is a single `[`, otherwise falls through to `Ndjson` — this tie-breaker is the heuristic ratified for v1 |
| `.orc` | `Orc` |
| `.avro` | `Avro` |
| anything else | `CompileError::UnrecognizedFileFormat { path }` (COMP_E_0303) |

Mixed-format globs are not supported: every path resolved from one glob must infer to the same format, or `CompileError::MixedFormatsInGlob { pattern }` fires (COMP_E_0304). The decision is ratified against the peer-group norm — metricflow and Cube both require format homogeneity per source.

## 5. `ColumnMapping`

### 5.1 Structure

`ColumnMapping` is the per-Binding Semantics-name-keyed recipe table:

```rust
pub struct ColumnMapping {
    pub entries: BTreeMap<SemanticsName, ColumnMappingValue>,
}

#[non_exhaustive]
pub enum ColumnMappingValue {
    Column {
        name: ColumnName,
    },
    Literal {
        value: LiteralValue,
        data_type: DataType,
    },
    Computed {
        expr: PhysicalExpr,
    },
    Metadata(MetadataDimension),
}
```

`BTreeMap` keying is deliberate: it gives the Model-layer shape a deterministic iteration order (alphabetical on `SemanticsName`) which feeds straight into the `ResolvedExprTable` ordering ratified by `14b §4`. The Manifest-layer `ResolvedColumnMapping` (§7) uses `HashMap` for O(1) lookup — the ordering fence is paid at Manifest-construction time, not at plan time.

### 5.2 `ColumnMappingValue::Column`

The most common case: the Semantics value is a direct column reference:

- `name: ColumnName` — must resolve to a column in **every** `PhysicalSource` in the Binding whose `Coverage` for this Semantics is `Native` (§6). Sources with `NullFill` do not require the column.
- No per-source divergence in spelling: `ColumnName` is a single value; cross-source name-mapping is not supported at the Binding layer (it is a `Coverage` / NullFill question).

### 5.3 `ColumnMappingValue::Literal`

A typed constant:

- `value: LiteralValue` — the canonical literal enum (`14 §3.2`): `Null`, `Bool(bool)`, `Integer(i64)`, `Decimal(d128)`, `Float(f64)`, `String(String)`, `Binary(Vec<u8>)`, `Date(NaiveDate)`, `Timestamp(DateTime<Utc>)`, etc. per `13 §2`.
- `data_type: DataType` — the logical type the literal is cast to at emit time. Required (not inferred) so that mixed-sources mappings (e.g. some sources carry the column Natively as `Integer`, other sources use this Literal fallback for NullFill pushdown) present a single canonical type to the rest of the pipeline.

**Literal validation.** At compile time, the `data_type` is validated against the `LiteralValue` for representability:
- `LiteralValue::Integer(i)` with `data_type = Byte` → validate `i ∈ [-128, 127]`; else `CompileError::LiteralOverflow { name, value, data_type }` (COMP_E_0305).
- `LiteralValue::Null` with any non-nullable declared Semantics → `CompileError::NullLiteralForNonNullableSemantics { name }` (COMP_E_0306).
- `LiteralValue::String(s)` with `data_type = Date`/`Timestamp` → requires RFC 3339 parse success; else `CompileError::LiteralParseFailed { name, value, target_type }` (COMP_E_0307).

### 5.4 `ColumnMappingValue::Computed`

An author-written expression, already compiled to `PhysicalExpr` by `14b §3`:

- `expr: PhysicalExpr` — honors `PhysicalExpr`'s wrapper invariants from `14 §3.3`: no `EntityRef`, no `Aggregate`, `Column` allowed. Type inference (`14 §5`) has already run; `expr.inferred_type()` is populated.
- `expr.referenced_columns()` — every column name it reads must exist in every `PhysicalSource` in the Binding whose `Coverage` for this Semantics is `Derived` (§6). Sources with `NullFill` do not require the columns.

The YAML-to-`PhysicalExpr` compilation pathway for Computed entries is:

```
YAML ExprSource  (14 §4)
   → SemanticExpr  (authors may write @other_semantics; 14b §3 substitutes them)
   → PhysicalExpr  (columns resolved against this Binding's PhysicalSource schemas)
```

The compile stage invokes `14b::resolve_to_physical(semantic_expr, binding_context)` per Computed entry. `binding_context` supplies (a) the cross-source reconciled schema over which `Column` identifiers resolve, (b) the `FunctionRegistry` (via `14a`), (c) the substitution map for `@entity_ref` identifiers into same-Binding Computed / Column entries. The output is a `PhysicalExpr` with `inferred_type` set; §9.1 then compares `inferred_type` against the declared Semantics `DataType` and emits a `Cast` at the Semantics boundary if needed.

**Cross-reference to `14b §5` cycle detection.** `Computed`-entries within a single `ColumnMapping` can refer to other Semantics via `EntityRef`. The `14b §5` Tarjan-SCC pass runs over the Binding's Computed entries and detects same-Binding cycles (`e1 → e2 → e1`). `CompileError::ComputedCycle { binding_id, cycle }` (owned by `14b §8.3`, re-surfaced via `15 §11`) is the failure.

### 5.5 `ColumnMappingValue::Metadata`

A physical-metadata extraction:

- `Metadata(MetadataDimension)` — the full `MetadataDimension` per `13 §4.7`, carrying optional `path: PathExtraction { token: usize }` and `partition: PartitionExtraction { level: usize }`. At most one of the two is `Some`; both `None` is a YAML validation error caught by `11 §6.1`.
- Result type of the extraction: always `DataType::String` from path tokens; for partitions, the declared `PartitionColumn.data_type` (the catalog's declared partition type, which Iceberg reports explicitly and which Hive-style paths report as `String` by convention with a logical type hint in the catalog).

Detailed mechanics, exhaustively described in §8.

### 5.6 Completeness: coverage of the `SemanticInterface`

Per `11 §6`, a `SimpleDataKind`'s `SemanticInterface` is the complete named surface (Dimensions, Measures, Metrics, Filters, Keys). `15` ratifies the rule:

**Every Semantics name in the `SemanticInterface` MUST appear exactly once as a key in `ColumnMapping.entries`.**

- A name in the interface but missing from `entries` → `CompileError::MissingBindingEntry { semantics, binding_id }` (COMP_E_0308). Fail-fast.
- A name in `entries` but not in the interface → `CompileError::SpuriousBindingEntry { name, binding_id }` (COMP_E_0309). Fail-fast.
- A name duplicated in `entries` — the YAML parser rejects duplicate keys at a lower layer (`32`) via standard YAML duplicate-key handling; at the `15` level, `BTreeMap` is an unambiguous map and duplication is structurally impossible.

**Edge case: Semantics with a `Constraint` that derives its value (e.g. `Measure(Count, Key)` per `11 §8.4`).** Per `11 §8.4`, a count-like Measure declared with `Constraint::DerivesFrom(Key)` does not require a physical column; it counts the Key's rows. `ColumnMapping` still includes an entry for that Semantics — proposal: `ColumnMappingValue::Computed { expr: PhysicalExpr(Count(Column(<key_column>))) }` where the key column is the `ColumnMapping`'s entry for the referenced Key. The `compile` stage synthesizes the `Computed` entry from the `Constraint::DerivesFrom` spec — authors do not need to re-declare it. **Proposed (Round 1):** this is a compile-stage synthesis, not a YAML-surface convenience; the Model's authored `ColumnMapping` can omit the key-derived Measure entry, and the compile stage fills it in before the completeness check runs. See `open_questions/15_open_questions.md` Q-MAP-003.

**Edge case: `ComputedDimension` (per `14 §1.2`).** These ALWAYS map to `ColumnMappingValue::Computed`; they never have a `Column`-valued entry. The YAML parse enforces this at `32`.

**Edge case: Name case.** `SemanticsName` preserves the author's case (`14 §4.3`); the parser does no case folding. Mapping-key mismatches due to case errors (`"customer_id"` declared, `"CustomerID"` mapped) → `SpuriousBindingEntry` + `MissingBindingEntry` pair.

### 5.7 Shape constraints

- `ColumnMappingValue::Column` is the **common path**; it should account for the majority of entries in any real Model. Complex variants exist for the edge cases.
- `ColumnMappingValue::Computed` is for author-declared computed Semantics (`14 §1.2`) AND for the synthesized Measures from §5.6 and for the `Cast`-wrapped Column cases from §9.1. The cardinality of Computed entries should be bounded by the sum of authored-computed-Semantics + cast-wrapped Semantics in the `SemanticInterface`.
- `ColumnMappingValue::Metadata` is for explicit Metadata-typed Semantics authored with the `metadata:` block (`13 §4.7`). Never synthesized.
- `ColumnMappingValue::Literal` is a rarely-used fallback — it captures NullFill-style "this source does not have this column, use a constant" patterns, but the more common NullFill pattern uses `Coverage::NullFill` (§6) which does not need a Literal entry (the planner emits a `NULL` cast itself).

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
}
```

- `source_index` indexes into `Binding.sources` (0-based).
- `semantics` is the `SemanticsName` in the Binding's `ColumnMapping`.
- **Default when absent:** `Native`. The `HashMap` stores only the non-default entries (`NullFill` and `Derived`) and cross-source `Native` with a `Computed` mapping. A missing entry means "this source provides this Semantics via the direct-column mechanism of the `ColumnMappingValue`". This is the common case and the storage is sparse.

Variants:

- **`Native`** — the source has a physical column (for `Column` / `Computed` variants with all referenced columns present) or the Semantics is a `Literal` / `Metadata` extraction that is uniformly applicable (all sources have the same partition structure for `Metadata::Partition`; all paths match the same token shape for `Metadata::Path`).
- **`NullFill`** — the source does NOT have the physical column(s) that `ColumnMappingValue::Column` / `::Computed` requires. At plan time, the planner emits a `Project[Cast(Null, declared_type) AS semantics]` at the branch covering this source (Unionset pattern; `23`). For non-Unionset consumers, `NullFill` on any source is a `CompileError::UnusableNullFillInNonUnionContext { binding_id, source_index, semantics }` (COMP_E_0310) — the v1 behavior: NullFill is meaningful only when the binding is consumed by a Unionset or Grainset constituent that tolerates it; in a bare Dataset / Joinset consumer, a NullFill source is a Model bug.
- **`Derived`** — the source has the *upstream* columns that a `Computed` expression needs, but not necessarily the direct Semantics column. Used for Computed entries whose expression's `referenced_columns` are all present on this source. (For simple `Column`-valued Semantics, `Derived` is indistinguishable from `Native` and is not used — the planner short-circuits to `Native`.)

### 6.2 Computation at compile time

`Coverage` is populated during §10 step 5 of the resolution flow. The algorithm per `(source_index, semantics)`:

1. Look up `ColumnMapping.entries[semantics]` → `ColumnMappingValue`.
2. Dispatch on variant:
   - `Column { name }` — check `Binding.sources[source_index].schema.columns` for `name`. Present → `Native`. Absent → `NullFill`.
   - `Literal { ... }` — always `Native`. (Literals do not depend on source schemas.)
   - `Computed { expr }` — check every name in `expr.referenced_columns()` against the source's schema. All present → `Derived` (since the Semantics is computed, not directly present). Any missing → `NullFill`.
   - `Metadata(m)` — check applicability of the extraction:
     - `m.path.is_some()`: `PhysicalSource::File` only; `Table` / `Snapshot` → `NullFill` (the token is meaningless for table sources).
     - `m.partition.is_some()`: `PhysicalSource::{File, Table, Snapshot}` with `partitions.len() >= m.partition.as_ref().unwrap().level`; else `NullFill`.
     - Both `None`: structurally rejected at YAML-parse time; unreachable here.
3. Persist the resulting `CoverageVariant` in `Coverage.entries` iff it is not `Native` (to keep storage sparse).

### 6.3 Derived-cross-source rule

When a Semantics maps to `Computed`, its `Coverage` on a specific source depends on whether the expression's column references are present on that source. The `Derived` variant encodes: "the upstream columns are here, the planner computes the Semantics on this source branch." The `NullFill` variant encodes: "the upstream columns are not here, the planner emits a NULL-filled constant of the declared Semantics type on this source branch."

**Proposed (Round 1):** `Derived` is a distinct variant from `Native` (rather than collapsing into `Native`) because consumers that care about provenance — notably the `16` composition layer building a `ComposedSemanticInterface` coverage map — need to distinguish "this is physically present as a column" (`Native` on a `Column`-valued Semantics) from "this is computed from upstream columns that happen to be present" (`Derived`). The distinction matters for pushdown reasoning in `34 §5`: Native reads are always pushdownable, Derived reads require pushing the computation. See `open_questions/15_open_questions.md` Q-MAP-005.

### 6.4 Scope boundary with `16`

`15` owns `Coverage` at the **Binding** level — one Coverage per Binding, keyed on `(source_index, semantics)`. `16` owns `Coverage` at the **ComposedSemanticInterface** level — one Coverage per composition, keyed on `(constituent_datakind, composed_field)`.

The two are orthogonal. A Unionset of three Simple branches has:

- One `Coverage` per branch Binding (from `15`).
- One composition-level `Coverage` on the Unionset itself (from `16`), which records which constituent provides which field of the unified surface.

`15` does not speak about composition-level coverage; `16` uses `15`'s Binding coverage as an input when it builds the composition coverage (for each composed field, look up which constituent Binding has `Native` / `NullFill` for the underlying Semantics; this feeds the planner's per-branch NULL-fill emission in `23`).

### 6.5 Worked example

Consider a `SimpleDataKind` with Semantics `{customer_id, total, channel}` bound to a glob expanding to three files:

```
s3://b/year=2024/month=01/data.parquet   schema: {customer_id, total}
s3://b/year=2024/month=02/data.parquet   schema: {customer_id, total, channel}
s3://b/year=2024/month=03/data.parquet   schema: {customer_id, total, channel}
```

`ColumnMapping`:
```
customer_id → Column { name: "customer_id" }
total       → Column { name: "total" }
channel     → Column { name: "channel" }
```

Coverage:
```
(0, channel) → NullFill    # file 0 does not have `channel`; other entries default to Native
(1, *)       → Native      # default, not stored
(2, *)       → Native      # default, not stored
```

Without a Unionset consumer, this Binding fails compile (§6.1 variant `NullFill` constraint in non-Unionset contexts → `COMP_E_0310`). With a Unionset consumer, `23` reads the Coverage and emits, per branch:

```
Branch 0 (file 0):  Project[customer_id, total, CAST(NULL AS <channel_type>) AS channel]
Branch 1 (file 1):  Project[customer_id, total, channel]
Branch 2 (file 2):  Project[customer_id, total, channel]
```

## 7. Manifest-Layer Counterpart: `ResolvedColumnMapping`

### 7.1 Motivation

The Model-layer `ColumnMapping` is a single `BTreeMap<SemanticsName, ColumnMappingValue>`. At plan time, the planner's hot-loop lookup pattern is "given a Semantics name, jump straight to the physical recipe." Matching the sum-type at every lookup is wasteful when the per-variant shape is known; a flat per-variant HashMap is faster and avoids the planner carrying an enum match in its hottest inner loop.

Per I8 and the `Resolved*` prefix convention (`00 §4.1`), the Manifest stores `ResolvedColumnMapping`, a denormalized / pre-indexed form:

### 7.2 Structure

```rust
pub struct ResolvedColumnMapping {
    pub columns: HashMap<SemanticsName, ColumnName>,
    pub literals: HashMap<SemanticsName, ResolvedLiteral>,
    pub computed: HashMap<SemanticsName, PhysicalExpr>,
    pub metadata: HashMap<SemanticsName, MetadataDimension>,
    pub source_coverage: HashMap<CoverageKey, CoverageVariant>,
}

pub struct ResolvedLiteral {
    pub value: LiteralValue,
    pub data_type: DataType,
}
```

The four top-level HashMaps are **disjoint**: a given `SemanticsName` appears in exactly one of them. The completeness rule (§5.6) is preserved: the union of their key sets equals the `SemanticInterface`'s name set.

`source_coverage` is the Manifest-layer form of the §6 `Coverage` — same key/value shape, just promoted from an `Option<Coverage>` to a bare field (the `None` case is represented as an empty map; the per-source default is still `Native`).

### 7.3 Construction

The compile stage produces `ResolvedColumnMapping` from the Model-layer `ColumnMapping` via a single pass:

```
for (semantics, value) in model.column_mapping.entries:
    match value:
        Column { name }             → resolved.columns.insert(semantics, name)
        Literal { value, data_type} → resolved.literals.insert(semantics, ResolvedLiteral { value, data_type })
        Computed { expr }           → resolved.computed.insert(semantics, expr)
        Metadata(m)                 → resolved.metadata.insert(semantics, m)
```

`expr` is the already-compiled `PhysicalExpr` (with `inferred_type` populated per §9.1) so no further expression work happens here.

### 7.4 Planner access pattern

Plan-time per-Semantics lookup:

```rust
fn resolve_semantics<'a>(rb: &'a ResolvedBinding, s: &SemanticsName) -> SemanticsRecipe<'a> {
    if let Some(col) = rb.column_mapping.columns.get(s)    { return SemanticsRecipe::Column(col); }
    if let Some(lit) = rb.column_mapping.literals.get(s)   { return SemanticsRecipe::Literal(lit); }
    if let Some(exp) = rb.column_mapping.computed.get(s)   { return SemanticsRecipe::Computed(exp); }
    if let Some(md)  = rb.column_mapping.metadata.get(s)   { return SemanticsRecipe::Metadata(md); }
    // Invariant: one of the above must hit, by compile-time completeness (§5.6).
    unreachable_by_invariant!("completeness guaranteed at compile")
}
```

Each branch is an O(1) HashMap probe. The sum-type match on the Model-layer enum is never paid at plan time.

### 7.5 Relation to `14b`'s `ResolvedExprTable`

`14b §4`'s `ResolvedExprTable` is a **Manifest-global** map from `(SemanticsName, BindingId)` to `PhysicalExpr`. `ResolvedColumnMapping.computed` is a **per-Binding denormalization** of that table, filtered to the Binding's own entries. Both exist:

- The global table supports cross-Binding planner work (e.g. `14b §6.2` Relationship-path composition, where an expression is shared across a Joinset's members).
- The per-Binding HashMap supports single-Binding planner work (per-`Scan` expression lookup) without the extra `BindingId` in the key.

Whether the two share storage (the `ResolvedColumnMapping.computed` values are pointers into the global table) or are duplicated is a `33`-owned implementation choice. From `15`'s contract surface, both are populated and both are plan-readable; `33` will ratify the storage strategy. **Proposed (Round 1):** duplicate storage by default; the memory overhead is a small constant per binding-semantics pair, and the planner is free of aliasing concerns. See `open_questions/15_open_questions.md` Q-MAP-006.

### 7.6 `ResolvedBinding` envelope

The Binding's Manifest-layer counterpart is:

```rust
pub struct ResolvedBinding {
    pub binding_id: BindingId,
    pub sources: Vec<ResolvedPhysicalSource>,
    pub column_mapping: ResolvedColumnMapping,
}
```

`ResolvedPhysicalSource` is the Manifest-layer `PhysicalSource` — structurally identical in v1 (no denormalization needed; the Model-layer shape is already flat enough). If a future manifest-layer optimization demands a richer form (e.g. pre-computed partition pruning indices), it evolves as a MINOR per `30 §4`.

## 8. `MetadataDimension` Semantics

Recall `13 §4.7`:

```rust
pub struct MetadataDimension {
    pub path: Option<PathExtraction>,
    pub partition: Option<PartitionExtraction>,
}

pub struct PathExtraction {
    pub token: usize,
}

pub struct PartitionExtraction {
    pub level: usize,
}
```

At most one of `path` / `partition` is `Some` (parser-enforced, `32`).

### 8.1 `path.token: N` extraction

Applicable to `PhysicalSource::File` variants only. The path tokenization rule:

1. Take the source's `path` (e.g. `"s3://bucket/year=2024/month=01/day=15/data.parquet"`).
2. Split on `/` into non-empty segments.
3. Strip any scheme prefix (`"s3://"`, `"gs://"`, `"file://"`): these are treated as a single "scheme" token with index `0` only if the author-facing Model uses 0-indexed absolute counting, OR skipped entirely if the Model uses 0-indexed post-scheme counting.

**Proposed (Round 1): 0-indexed post-scheme.** Segments after the scheme are counted from `0`. For `"s3://bucket/year=2024/month=01/day=15/data.parquet"`, segments are:

```
0: "bucket"
1: "year=2024"
2: "month=01"
3: "day=15"
4: "data.parquet"
```

So `path: { token: 1 }` → `"year=2024"` (the full token, not the value — see §8.1.1 below). See `open_questions/15_open_questions.md` Q-MAP-007.

**Local-path case.** `"/mnt/data/year=2024/month=01/file.parquet"`: `0: "mnt"`, `1: "data"`, `2: "year=2024"`, etc. Leading `/` produces no empty-string token; the first non-empty segment is `0`. Windows-style paths are NOT supported in v1 (explicit non-goal; the Model surface is lake-native).

#### 8.1.1 Token extraction result type

The extracted token is a raw `String` — the whole segment, NOT the `=`-suffix value. For `"year=2024"`, the extraction yields `"year=2024"`.

**Rationale.** Splitting on `=` to yield `"2024"` would be a second, implicit parse step that can fail silently (what about paths like `"year-2024"` or `"year"`?). Returning the whole segment keeps `15`'s contract narrow; authors needing the value-only form can wrap the Metadata extraction in a `Computed` expression that calls `substring_after(metadata_segment, '=')`. The function catalog (`14a`) provides `substring_after` for this pattern.

**Proposed (Round 1):** raw segment as the default. A sibling `path.value_of_kv_token: N` extraction (that extracts the value after `=`) is **not** ratified in v1; authors compose it explicitly. See `open_questions/15_open_questions.md` Q-MAP-008.

#### 8.1.2 Error conditions

- `CompileError::MetadataTokenOutOfRange { binding_id, source_index, token_index, path }` — the source's path has fewer segments than `token_index + 1`. Proposed code: `COMP_E_0311`.
- `CompileError::MetadataTokenOnNonFileSource { binding_id, source_index, semantics }` — a Semantics with `path.token` is mapped on a Binding whose source at that index is `Table` or `Snapshot`. Proposed code: `COMP_E_0312`.

The check runs per-source during §10 step 5 (`Coverage` derivation); out-of-range triggers immediately, not at plan time.

### 8.2 `partition.level: N` extraction

Applicable to all `PhysicalSource` variants that declare `partitions: Vec<PartitionColumn>`.

Rule: 1-indexed. `level: 1` is the first partition column (`PartitionColumn.position == 1`); `level: N` is the *N*-th partition column.

For `PhysicalSource::File` with Hive-style path partitioning (`year=2024/month=01/day=15/data.parquet`), the compile-time glob expansion in §3.5.1 step 4 builds the `PartitionColumn` list from the path components. The partitions are ordered outer-to-inner in the path; `position: 1` = `year`, `position: 2` = `month`, etc.

For `PhysicalSource::Table` and `Snapshot` backed by Iceberg (or equivalent), the partition column list is the table's declared partition spec (the `default-spec-id` at the catalog level). Partition-transform identities (`identity`, `year`, `month`, `bucket[N]`) are carried on the catalog side (`37`) and are not surfaced on the `PartitionColumn` struct itself in `15`'s v1.

#### 8.2.1 Partition extraction result type

The extracted value is the partition column's value for a given row, typed as the declared `PartitionColumn.data_type`:

- Hive-style path: always `String` unless the author declares a `data_type` override in the Model (future extension; not in v1).
- Iceberg / Unity table: the declared partition-column `DataType` (which may be `Integer`, `String`, `Date`, etc.).

**Proposed (Round 1):** for Hive-style path partitioning, the v1 extraction is raw-value `String` (the part after the `=`, not the whole segment — note the asymmetry with §8.1.1, rationalized because path-style partitioning is a value-carrying convention, whereas free-form path tokens are not). See `open_questions/15_open_questions.md` Q-MAP-009.

#### 8.2.2 Error conditions

- `CompileError::MetadataPartitionUnavailable { binding_id, source_index, semantics, level }` — the source has no partition spec (`partitions.is_empty()`) or fewer partition columns than `level`. Proposed code: `COMP_E_0313`.
- `CompileError::InconsistentPartitioning { binding_id, sources: Vec<usize>, semantics }` — the Binding has multiple sources and a Semantics references `partition.level`, but the sources have different partition structures (different column names at the same level, or different level counts). Because the Semantics extraction MUST produce a consistent logical value across the Binding, the sources must agree on partitioning for levels the Semantics references. Proposed code: `COMP_E_0314`.

The `InconsistentPartitioning` rule is strict: all sources in the Binding MUST agree on (a) the number of partition levels (at least up to the max referenced `level`), (b) the name and declared type of the partition column at each referenced level. Sources that merely have *more* partition levels than referenced are fine — only referenced levels are checked. Two sources partitioned by `(year, month)` and `(year, month, day)` are compatible IF no Semantics references `partition.level: 3`; incompatible IF one does.

The more lenient "per-source partition structure" discipline is a Cube.js pattern; metricflow's normalization through `data_source` enforces the strict form; semstrait follows metricflow — strict agreement is forced at compile so plan-time composition works without any per-source branching on partition shape.

### 8.3 Cross-variant exclusion

The YAML parser (`32`) enforces `path.is_some() XOR partition.is_some()`. Both-Some is a `ParseError`; both-None is a `ParseError`. `15` does not re-validate.

### 8.4 `NullFill` + Metadata interaction

Per §6.2, a Metadata Semantics on a non-applicable source (e.g. `path.token: 2` on a `Table` source) → `Coverage::NullFill`. The planner emits a `NULL` cast at that branch (Unionset consumer) or fails at compile time (non-Unionset consumer — `COMP_E_0310`). Combining a file-glob source and a table source in one Binding with a `path.token` Semantics is therefore legal only inside a Unionset-consumed Binding; in a bare Simple/Dataset consumption path, it is a Model bug.

## 9. Schema Reconciliation at Compile Time

### 9.1 Widening / narrowing / incompatible casts

When a Semantics declares `data_type: Integer` in its `SemanticInterface` and the physical `PhysicalSource.schema.columns[_].data_type` is `Long`, the compile stage reconciles the two per `14 §6.4`'s cast policy:

| Declared × Physical (per `14 §6.4` subset relevant here) | Action | Diagnostic |
|---|---|---|
| Same logical type | Pass-through. | — |
| Widening numeric (e.g. declared `Long`, physical `Integer`) | Emit `PhysicalExpr(Cast(Column, declared_type))` on the fly at the Semantics boundary; the `ColumnMappingValue::Column` entry is rewritten to `ColumnMappingValue::Computed { expr: Cast(Column, declared) }` during compile. | `COMP_I_0301 ImplicitWideningCast` (info-level). |
| Narrowing numeric (e.g. declared `Integer`, physical `Long`) | Emit the same `Cast` wrapping as above. | `COMP_W_0302 ImplicitNarrowingCast` (warning-level, advises the author to double-check — narrowing a real `i64` that overflows `i32` is an engine-level runtime error, not a compile one). |
| Precision widening (Decimal → wider Decimal) | Emit `Cast`. | `COMP_I_0301`. |
| Precision narrowing (Decimal → narrower Decimal) | Emit `Cast`. | `COMP_W_0302`. |
| Float / Decimal cross-cast | Emit `Cast`. | `COMP_W_0303 FloatDecimalCrossCast` — these casts can lose precision; always warn. |
| `String` ↔ non-`String` (e.g. declared `Integer`, physical `String`) | No cast emitted; `CompileError::IncompatiblePhysicalType { semantics, declared, physical }` (COMP_E_0315). | Fail-fast. |
| `Date` / `Timestamp` ↔ non-temporal | `CompileError::IncompatiblePhysicalType` (COMP_E_0315). | Fail-fast. |
| `Binary` ↔ any non-`Binary` | `CompileError::IncompatiblePhysicalType`. | Fail-fast. |

The full cast matrix lives in `14 §6.4`; §9.1 is the physical-to-semantic reconciliation slice of that matrix. `15` does not re-ratify the matrix; it sites the hook where the matrix is consulted.

**Where `Cast` lives.** After reconciliation, a `ColumnMappingValue::Column { name: "amount" }` over a physical `Long` with a declared Semantics `Integer` is rewritten to `ColumnMappingValue::Computed { expr: Cast(Column("amount"), DataType::Integer) }`. The Manifest-layer `ResolvedColumnMapping.computed` stores the `Cast`-wrapped `PhysicalExpr`; no separate "cast-needed" flag exists. This keeps the planner's code path uniform (every non-literal/non-metadata Semantics is either a direct Column read or an expression evaluation).

### 9.2 `Computed`-entry type inference reconciliation

For `ColumnMappingValue::Computed { expr }`, the `expr.inferred_type()` (from `14 §5`) is compared to the declared Semantics `DataType`:

| Inferred × Declared | Action |
|---|---|
| Same | Pass-through. |
| Widening | Wrap: `expr = Cast(expr, declared)`. Info-level diagnostic `COMP_I_0304 ImplicitWideningCastOnComputed`. |
| Narrowing | Wrap: `expr = Cast(expr, declared)`. Warning-level `COMP_W_0305 ImplicitNarrowingCastOnComputed`. |
| Incompatible | `CompileError::ComputedTypeMismatch { semantics, inferred, declared }` (COMP_E_0316). |

The wrapping lives on the Manifest-layer `PhysicalExpr` in `ResolvedColumnMapping.computed`.

### 9.3 Cross-source type agreement

A Binding's `sources` may comprise multiple `PhysicalSource`s, each with its own `schema`. A Semantics referencing a physical column `c` through `ColumnMappingValue::Column { name: c }` requires:

- **Every source where Coverage is `Native`** must have `c` in its schema.
- The `DataType` of `c` MUST be identical across all such sources.

If a source has `c` but with a different `DataType` than another source's `c`, the Binding is rejected:

- `CompileError::CrossSourceTypeDisagreement { binding_id, column, types }` (COMP_E_0317) — `types: Vec<(usize, DataType)>` enumerates the divergent cases per-source.

This is strict intentionally: a Binding that accepts `c: Integer` in source 0 and `c: Long` in source 1 would force every plan-time scan to decide which cast to apply per source, pushing the reconciliation into the hot path. Compile-time rejection keeps the post-Manifest contract "one Semantics, one type per Binding."

For `Computed` entries with a cross-source `referenced_columns` set: the same strict rule applies — every column referenced by the expression must have the same logical `DataType` across every `Derived`-covered source.

### 9.4 Nullability reconciliation

The Semantics-level nullability (from `11 §6` / `14 §5.2`) is compared against the physical source's per-column `nullable: bool`:

- Declared Non-nullable + any source reporting nullable → `COMP_W_0306 NullableSourceForNonNullableSemantics` (warning, not error — the source may still contain no nulls in practice; the runtime engine will enforce). Advises the author to either relax the declared type or add a `filter: NOT NULL` in a wrapping Measure/Dimension.
- Declared Nullable + physical non-nullable → silent; this is always safe.

**Proposed (Round 1):** the nullability mismatch is a warning, not an error. Upgrading to an error is a v2 conversation; some authors have legitimate workflows where the source-reported nullability is conservative (Parquet marking `optional` for a column that is in practice always populated). See `open_questions/15_open_questions.md` Q-MAP-010.

### 9.5 Reconciliation site in `compile`

The reconciliation happens in §10 step 4 (ColumnMapping completeness + reconciliation), after the sources and their schemas are in place but before the Coverage pass (§10 step 5). Steps 2–3 build the cross-source reconciled schema view; step 4 consults it per-Semantics.

## 10. Compile-Time Resolution Flow

The binding-resolution flow lives inside the `compile` stage, between `10 §3.3`'s stage-level sub-steps. Per `10`'s breakdown, `compile` orchestrates catalog metadata fetch, source resolution, `ExprSource` compilation, and Manifest-index construction; `15`'s flow is the sub-sequence specific to each Binding.

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

### 10.4 Step 4 — `ColumnMapping` well-formedness and reconciliation

1. **Completeness check (§5.6):**
   - Missing Semantics → `COMP_E_0308 MissingBindingEntry`.
   - Spurious entry → `COMP_E_0309 SpuriousBindingEntry`.
2. **Per-entry variant dispatch:**
   - `Column`: validate the column exists on at least one source; if on NO source, `COMP_E_0310`-lite (actually `ColumnMissingOnAllSources { semantics, column }`, COMP_E_0318). Reconcile declared vs physical `DataType` per §9.1; wrap in `Cast` if widening/narrowing. Record cross-source type agreement per §9.3.
   - `Literal`: validate representability (§5.3 error list).
   - `Computed`: invoke `14b::resolve_to_physical`. Use the cross-source reconciled schema as the column-lookup context. Check `expr.referenced_columns` across sources per §9.3. Reconcile `expr.inferred_type` vs declared Semantics `DataType` per §9.2.
   - `Metadata`: validate applicability per §8's error conditions across sources. `COMP_E_0311`–`COMP_E_0314`.
3. **Synthesize compile-derived entries** for Constraints that require a `ColumnMappingValue::Computed` (§5.6 edge case — e.g. `Measure(Count, DerivesFrom(Key))`). These entries are added to the Model-layer `ColumnMapping` struct in-place during the reconciliation pass, then flow through step 4.2's Computed branch.

### 10.5 Step 5 — `Coverage` derivation

Per §6.2. For each `(source_index, semantics)` pair, compute the `CoverageVariant` and store in `Coverage.entries` iff not `Native`.

If any entry is `NullFill` and the Binding's owning `DataKind` is not a Simple constituent of a Unionset (or other NullFill-tolerant consumer), emit `COMP_E_0310 UnusableNullFillInNonUnionContext`. The consumer-tolerance check requires traversing up the Model's parent-DataKind reference; this traversal is the `compile` driver's job, not `15`'s — `15`'s Coverage-derivation pass just computes the per-source variant and flags NullFill entries to the driver.

### 10.6 Step 6 — `ResolvedBinding` / `ResolvedColumnMapping` materialization

Per §7. Build the flat HashMaps:

- `columns: HashMap<SemanticsName, ColumnName>` — populated from surviving `Column` entries.
- `literals: HashMap<SemanticsName, ResolvedLiteral>` — from `Literal`.
- `computed: HashMap<SemanticsName, PhysicalExpr>` — from `Computed` (native) + synthesized Cast-wrapped `Column` entries (§9.1) + synthesized derived-Measure entries (§5.6).
- `metadata: HashMap<SemanticsName, MetadataDimension>` — from `Metadata`.
- `source_coverage: HashMap<CoverageKey, CoverageVariant>` — from the Coverage-derivation pass.

Attach to a `ResolvedBinding { binding_id, sources: Vec<ResolvedPhysicalSource>, column_mapping: ResolvedColumnMapping }`.

### 10.7 Step 7 — Manifest-index contribution

The Binding's `ResolvedBinding` is handed off to the Manifest-index-construction stage. The `ResolvedExprTable` (per `14b §4`) absorbs every Computed entry as `(SemanticsName, BindingId) → PhysicalExpr`. The per-DataKind Binding index is populated (`DataKindId → Vec<BindingId>` — vector length 1 for `SimpleDataKind`). These are `33`-owned Manifest structures; `15`'s flow just feeds them.

### 10.8 Error-handling posture

The entire §10 flow is **fail-fast** (per `30 §7` / `10 §3.3`). A compile error in step 2 halts the whole Binding's resolution; subsequent steps are not attempted. Warnings accumulate throughout and are attached to the eventual `CompileError` or to the success `Manifest`.

Some steps CAN collect multiple errors before returning (step 4's completeness check reports every missing / spurious entry in one pass before failing). Others are strictly fail-fast (step 2's glob expansion returns the first failure). The convention: structural well-formedness checks accumulate; I/O-bound and dependency-chain checks fail-fast.

### 10.9 Placement within `10 §3.3`

`10 §3.3` enumerates `compile`'s sub-steps as: **catalog fetch → glob expand → schema resolve → name resolve → ExprSource compile → index build**. `15 §10`'s per-Binding flow is a refinement: it enumerates the sub-structure of the "ExprSource compile → index build" cluster specifically for Binding work. The catalog-fetch and glob-expand sub-steps are shared across Bindings (a single catalog snapshot is taken for the whole compile invocation); `15 §10 step 2` calls into those shared resources per-Binding.

## 11. Error Model

### 11.1 Binding-owned `CompileError` variants

All compile-time error variants introduced or re-surfaced by `15`, with proposed stable codes per `30 §6.2`. The `COMP_E_0200-0299` sub-range (catalog/source resolution) hosts source-level errors; the `COMP_E_0300-0399` sub-range (schema/binding) hosts ColumnMapping / reconciliation errors.

| Code | Variant | Sub-range | Trigger |
|---|---|---|---|
| `COMP_E_0203` | `CatalogUnavailable { catalog_id, cause }` | 0200–0299 | catalog whose `CatalogId` was named on the binding is unreachable / not registered |
| `COMP_E_0301` | `NoSourcesMatched { binding_id, pattern }` | 0300–0399 | glob / table-glob produced zero matches |
| `COMP_E_0302` | `GlobExpansionFailed { binding_id, pattern, cause }` | 0300–0399 | filesystem / catalog raised during glob enumeration |
| `COMP_E_0303` | `UnrecognizedFileFormat { path }` | 0300–0399 | file extension does not map to a known `FileFormat` |
| `COMP_E_0304` | `MixedFormatsInGlob { pattern }` | 0300–0399 | glob resolved to files with different inferred formats |
| `COMP_E_0305` | `LiteralOverflow { name, value, data_type }` | 0300–0399 | `ColumnMappingValue::Literal` value does not fit the declared type |
| `COMP_E_0306` | `NullLiteralForNonNullableSemantics { name }` | 0300–0399 | `LiteralValue::Null` declared on a non-nullable Semantics |
| `COMP_E_0307` | `LiteralParseFailed { name, value, target_type }` | 0300–0399 | string literal failed to parse into target temporal type |
| `COMP_E_0308` | `MissingBindingEntry { semantics, binding_id }` | 0300–0399 | Semantics in interface, absent from `ColumnMapping` |
| `COMP_E_0309` | `SpuriousBindingEntry { name, binding_id }` | 0300–0399 | `ColumnMapping` key not in Semantics |
| `COMP_E_0310` | `UnusableNullFillInNonUnionContext { binding_id, source_index, semantics }` | 0300–0399 | NullFill derived for a Binding whose owning DataKind is not tolerance-consumed |
| `COMP_E_0311` | `MetadataTokenOutOfRange { binding_id, source_index, token_index, path }` | 0300–0399 | path token index ≥ segment count |
| `COMP_E_0312` | `MetadataTokenOnNonFileSource { binding_id, source_index, semantics }` | 0300–0399 | `path.token` Semantics on Table/Snapshot source |
| `COMP_E_0313` | `MetadataPartitionUnavailable { binding_id, source_index, semantics, level }` | 0300–0399 | source has no partitions, or fewer than level |
| `COMP_E_0314` | `InconsistentPartitioning { binding_id, sources, semantics }` | 0300–0399 | sources disagree on referenced partition structure |
| `COMP_E_0315` | `IncompatiblePhysicalType { semantics, declared, physical }` | 0300–0399 | declared and physical `DataType` not cast-compatible per `14 §6.4` |
| `COMP_E_0316` | `ComputedTypeMismatch { semantics, inferred, declared }` | 0300–0399 | Computed expression's `inferred_type` not cast-compatible with declared Semantics type |
| `COMP_E_0317` | `CrossSourceTypeDisagreement { binding_id, column, types }` | 0300–0399 | a column has different logical types in different sources of one Binding |
| `COMP_E_0318` | `ColumnMissingOnAllSources { binding_id, semantics, column }` | 0300–0399 | `Column`-valued Semantics's physical column is absent from every source |
| `COMP_E_0319` | `SchemaFetchFailed { source, cause }` | 0300–0399 | format-specific schema-resolution step failed (Parquet footer unreadable, JSON sample I/O error, etc.) |

### 11.2 Binding-adjacent warnings

| Code | Variant | Trigger |
|---|---|---|
| `COMP_I_0301` | `ImplicitWideningCast { semantics, from, to }` | §9.1 widening-cast wrapping |
| `COMP_W_0301` | `SchemaInferenceUsed { source }` | CSV-without-declared / JSON-without-declared schema inference |
| `COMP_W_0302` | `ImplicitNarrowingCast { semantics, from, to }` | §9.1 narrowing-cast wrapping |
| `COMP_W_0303` | `FloatDecimalCrossCast { semantics, from, to }` | §9.1 float/decimal cross cast |
| `COMP_I_0304` | `ImplicitWideningCastOnComputed { semantics, from, to }` | §9.2 widening on Computed |
| `COMP_W_0305` | `ImplicitNarrowingCastOnComputed { semantics, from, to }` | §9.2 narrowing on Computed |
| `COMP_W_0306` | `NullableSourceForNonNullableSemantics { semantics, source_index }` | §9.4 nullability mismatch |

### 11.3 Re-surfaced errors from `14` / `14b`

Errors raised by `14` / `14a` / `14b` during Computed-entry compilation pass through `15`'s resolution flow unmodified; `15` does not re-codify them. Examples:

- `EXPR_E_0201 EntityRefNotResolved` (from `14b`'s substitution)
- `EXPR_E_0206 ComputedCycle` (from `14b §5`'s SCC pass)
- `EXPR_E_0401 TypeInferenceFailed` (from `14 §7`)
- `EXPR_E_0301 FunctionArityMismatch` (from `14a §8`)

These are reported by the owning doc's code ranges; `15` ensures its error-reporting context includes the `binding_id` where relevant (adapters of the `Diagnostic` location field fill this in).

### 11.4 Error location discipline

Every `15`-owned `CompileError` SHOULD carry a `Diagnostic.location: Option<Location>` pointing into the Model YAML source:

- Binding-shaped errors (`NoSourcesMatched`, `GlobExpansionFailed`, `CatalogUnavailable`) → point at the YAML `binding:` block or its `sources:` sub-key.
- ColumnMapping-shaped errors (`MissingBindingEntry`, `SpuriousBindingEntry`, `IncompatiblePhysicalType`) → point at the specific `column_mapping[<name>]:` entry.
- Reconciliation-shaped errors (`CrossSourceTypeDisagreement`, `CrossSourceTypeDisagreement`) → point at the Binding; the `types: Vec<(usize, DataType)>` field enumerates per-source divergence.

The precise `Location` / `ByteSpan` shape is ratified in `30 §5` and `32 §?` (SourceId variant). `15` stipulates only the semantic target.

### 11.5 Code-range governance

`15`'s allocation adds eighteen `COMP_E_*` codes (`COMP_E_0203, 0301-0319`) and seven advisory codes (three `COMP_I_*`, four `COMP_W_*`). All are within the `COMP_E_0300-0399` / `COMP_W_0300-0399` reservation for "schema / binding (per `15`)" in `30 §6.2`. Adding further `15`-owned codes is MINOR per `30 §6.3`; the sub-range has remaining space for roughly 50 additional codes before coming close to the `COMP_E_0400-0499` neighbor (relationships / index build).

The `CatalogUnavailable (COMP_E_0203)` code sits in the `COMP_E_0200-0299` catalog/source-resolution range per `30 §6.2`; `15` owns the schema/binding range but is a **consumer** of catalog-resolution errors, so it re-surfaces them at their owning range rather than re-numbering.

## 12. Interaction with Other Documents

### 12.1 `14b §2` — `ResolvedExprTable` keying

`14b §2` keys its global `ResolvedExprTable` on `(SemanticsName, BindingId)`. `15 §2.2` ratifies the `BindingId`'s shape, allocation, and uniqueness; the two docs are tightly coupled and should be read together. Every ratified property of `BindingId` in §2.2 is honored by `14b`'s table construction; conversely, `14b`'s requirement that its table's keys are stable-within-a-Manifest is exactly the allocation discipline `15` ratifies.

### 12.2 `16` — Coverage at the `ComposedSemanticInterface` level

`16` extends `Coverage` from the Binding-level X-axis (per-source) to the composition-level axis (per-constituent-DataKind). Concretely:

- `15`'s `Coverage` answers "for Binding *B* with sources `[s0, s1, s2]`, which Semantics does `s1` cover natively vs NullFill?"
- `16`'s `Coverage` answers "for Unionset *U* with constituents `[D0, D1, D2]`, which fields of the composed interface does `D1` cover natively vs NullFill?"

`16` consumes `15`'s Coverage as input: for a composition-level "D1 covers field *f*" decision, `16` looks up the constituent's Binding's coverage of the underlying Semantics. The decision logic is `16`'s; the input data is `15`'s.

### 12.3 `20` — DataKind lifecycle integration

`20` ratifies how a `ResolvedDataKind` is constructed and where its `ResolvedBinding` attaches. `15 §10`'s flow produces a `ResolvedBinding`; `20`'s `compile` driver splices it into the `ResolvedDataKind::Simple` variant's `bindings: Vec<ResolvedBinding>` field (which, per `15 §2.1`, has length exactly 1).

### 12.4 `33` — Manifest persistence of `ResolvedColumnMapping`

`33`'s crate contract ratifies the Manifest struct shape, including the persisted form of `ResolvedBinding` / `ResolvedColumnMapping`. Serialization format (whether `serde_json`, a custom binary, or both) is `33`'s choice; `15` only stipulates that the structural shape in §7.2 is preserved round-trip (per I4).

The v1 design pencils in `serde` derivations on all `15`-ratified types (with the `serde` feature in `semstrait-core` per `31 §10`). The `PhysicalExpr` inside `ResolvedColumnMapping.computed` serializes through `14`'s `Expr` serialization (`31` / `14 §9`).

### 12.5 `37` — `CatalogProvider` integration

`15 §3`'s `CatalogRef` is an opaque handle into `37`'s `CatalogProvider` registry. `15 §3.5.1`'s step 2 makes the provider calls; the async posture and error-enum shape are ratified in `37`. `15`'s `CompileError::CatalogUnavailable` wraps `37`'s `CatalogError` via `IntoDiagnostic` (per `30 §5`).

### 12.6 `21`–`25` — Per-DataKind strategies consume Bindings

- `21 Dataset` — a Dataset (Simple) has exactly one Binding; its strategy is "scan the Binding's sources, read the `ResolvedColumnMapping`, emit a `Scan → Project` sub-plan."
- `22 Grainset` — levels resolve to child DataKinds; each level reads its child's Bindings. Grain-selection reads per-child Binding Coverage.
- `23 Unionset` — branches have Bindings; the per-branch NULL-fill is emitted based on Binding Coverage.
- `24 Joinset` — members have Bindings; the join path is composed via `16`'s Relationships; per-member scan plans read from member Bindings.
- `25 Applicability matrix` — the per-variant table explicitly notes which Binding properties are consumed by which strategy.

### 12.7 `10 §3.3` — Placement in `compile`

`15 §10` is the per-Binding sub-sequence of `10 §3.3`'s "source resolution → schema resolution → name resolution → expression compile → index build" pipeline. `15 §10.9` locates the sub-sequence precisely.

## 13. Ratified Decisions Index

A Q-numbered roll-up of every choice `15` ratifies in Round 1. Each entry cross-references the owning section; the `status` column marks whether the decision is fully ratified (`✓`) or has a parked companion question (`?` → see `open_questions/15_open_questions.md`).

| # | Decision | Ratified in | Status |
|---|---|---|---|
| R1 | `Binding` is a struct with `binding_id`, `sources`, `column_mapping`, `coverage` fields; `#[non_exhaustive]`. | §2.1 | ✓ |
| R2 | `BindingId(pub u32)` is unique within a Manifest (per-compile scope), not across Manifests. | §2.2 | ? Q-MAP-001 |
| R3 | `PhysicalSource` has three variants: `File`, `Table`, `Snapshot`. Enum is `#[non_exhaustive]`. | §3.1 | ✓ |
| R4 | Every `PhysicalSource` variant carries `schema: Schema` and `partitions: Vec<PartitionColumn>`. | §3.1 | ✓ |
| R5 | `Schema.columns` is an ordered `Vec`; order is source-native. | §3.2 | ✓ |
| R6 | `CatalogRef` is opaque; `catalog_id` routes to a provider, `fqn` names the table. | §3.3 | ✓ |
| R7 | `PartitionColumn.position` is 1-indexed. | §3.4 | ✓ |
| R8 | Partition-transform record lives in `37`'s catalog response, not on `PartitionColumn`. | §3.4 | ? Q-MAP-002 |
| R9 | Glob expansion is deterministic (lexical sort after provider enumeration). | §3.5.1 | ✓ |
| R10 | Zero glob matches is a compile error (`COMP_E_0301`). | §3.5.2 | ✓ |
| R11 | Cross-source schema hard-agreement is a compile error (`COMP_E_0317`); soft-agreement is `Coverage::NullFill`. | §3.5.3 / §6.2 | ✓ |
| R12 | `FileFormat` v1 set: `Parquet`, `Csv(CsvOptions)`, `Json(JsonOptions)`, `Orc`, `Avro`. | §4.1 | ✓ |
| R13 | CSV schema resolution: declared-first, then header-derived; columns are `String` unless declared. | §4.4 | ✓ |
| R14 | JSON schema resolution: declared-first, then sample-inference (scalar-only). | §4.4 | ? Q-MAP-004 |
| R15 | Format is inferred from file extension when glob spec has no explicit format. | §4.5 | ✓ |
| R16 | Mixed formats in one glob are a compile error (`COMP_E_0304`). | §4.5 | ✓ |
| R17 | `ColumnMapping.entries` is a `BTreeMap<SemanticsName, ColumnMappingValue>`. | §5.1 | ✓ |
| R18 | `ColumnMappingValue` has four variants: `Column`, `Literal`, `Computed`, `Metadata`. Enum is `#[non_exhaustive]`. | §5.1 | ✓ |
| R19 | Every Semantics in `SemanticInterface` appears exactly once in `ColumnMapping`. | §5.6 | ✓ |
| R20 | Derived Measures synthesized from `Constraint::DerivesFrom(Key)` are filled in at compile time. | §5.6 | ? Q-MAP-003 |
| R21 | `Coverage` keyed on `(source_index, semantics)`. Default `Native` is not stored. | §6.1 | ✓ |
| R22 | `CoverageVariant`: `Native`, `NullFill`, `Derived`; enum `#[non_exhaustive]`. | §6.1 | ✓ |
| R23 | `Derived` is a distinct variant from `Native`. | §6.3 | ? Q-MAP-005 |
| R24 | `NullFill` on a non-Unionset-tolerant consumer is a compile error (`COMP_E_0310`). | §6.1 | ✓ |
| R25 | Binding-level Coverage is `15`'s; composition-level Coverage is `16`'s. | §6.4 | ✓ |
| R26 | `ResolvedColumnMapping` splits variants into four flat HashMaps. | §7.2 | ✓ |
| R27 | Per-Binding `computed` HashMap duplicates `ResolvedExprTable` entries (storage choice). | §7.5 | ? Q-MAP-006 |
| R28 | `path.token` is 0-indexed post-scheme; segments are slash-delimited. | §8.1 | ? Q-MAP-007 |
| R29 | `path.token` extraction returns the whole segment, not the `=`-suffix value. | §8.1.1 | ? Q-MAP-008 |
| R30 | `partition.level` is 1-indexed. | §8.2 | ✓ |
| R31 | Hive-style partition extraction yields raw value (post-`=`), typed `String`. | §8.2.1 | ? Q-MAP-009 |
| R32 | Partitioning agreement is strict across Binding sources for referenced levels. | §8.2.2 | ✓ |
| R33 | Widening casts emit `COMP_I_0301`; narrowing emit `COMP_W_0302`. | §9.1 | ✓ |
| R34 | `Cast` is wrapped into `ColumnMappingValue::Computed`; no separate cast-needed flag. | §9.1 | ✓ |
| R35 | Computed-entry type inference reconciles via `Cast` wrap or `COMP_E_0316`. | §9.2 | ✓ |
| R36 | Cross-source type agreement is strict (`COMP_E_0317`). | §9.3 | ✓ |
| R37 | Nullability mismatch is a warning (`COMP_W_0306`), not an error. | §9.4 | ? Q-MAP-010 |
| R38 | `compile` flow for Bindings is a 7-step sequence (§10.1–§10.7). | §10 | ✓ |
| R39 | Structural checks accumulate; I/O and dependency checks fail-fast. | §10.8 | ✓ |
| R40 | `15`'s code range is `COMP_E_0300-0399` (schema/binding); catalog-availability errors re-surface in `COMP_E_0200-0299`. | §11 | ✓ |
| R41 | Every `15`-owned `CompileError` carries a `Diagnostic.location` pointing into the Model YAML. | §11.4 | ✓ |
| R42 | Expression / function / resolution errors from `14` / `14a` / `14b` pass through without re-codification. | §11.3 | ✓ |

## 14. Non-Goals

Explicit non-goals of `15`. Authors searching for these topics should look elsewhere:

- **Query-time execution** — engines execute; semstrait emits; `15` compiles the input to `adapt`, nothing more.
- **Schema drift against the Manifest** — drift detection is a **query-time** concern, covered by `CatalogProvider::check_schema_drift` per I11 and ratified in `37` / `38`. `15` freezes a schema at `compile` time; any post-compile drift is detected by a separate entry point.
- **Partition pruning** — the `Coverage` captured in `15` records per-source applicability; partition-predicate pushdown is a planner optimization covered in `34 §5`.
- **Per-engine register hooks** — `DataFusionConnector::register_manifest_sources` and peers are adapter-layer concerns (`36`). `15` produces a `ResolvedBinding`; the adapter reads it and decides how to register it.
- **Catalog-provider-specific snapshot semantics** — Iceberg REST snapshot pinning is described in `37`; `15` consumes the resulting `SnapshotId` as an opaque value.
- **Statistics-driven optimization** — `15` never surfaces row counts, histograms, or cardinality estimates. These are future planner concerns.
- **Write paths** — every `PhysicalSource` variant is read-oriented. Write-side semantics (materialized-view refresh, pre-aggregation persistence) are outside semstrait's mandate per `00 §10`.
- **Multi-format heterogeneous sources in one Binding** — a Binding's glob resolves to one format; mixed Parquet + CSV in one glob is rejected by `COMP_E_0304`. A Unionset of two Bindings, each with its own format, is the supported pattern.
- **Column renaming across sources** — a `ColumnMappingValue::Column { name }` resolves to a single physical name across every source. Per-source name mapping is a `Coverage` / NullFill question; semstrait's v1 answer is "rename upstream, in your ingestion job."

## 15. Summary of Vocabulary Anchors

For quick lookup when other docs reference `15`:

| Term | Shape (§ref) |
|---|---|
| `Binding` | struct §2.1 |
| `BindingId` | `struct(pub u32)` §2.2 |
| `PhysicalSource` | `enum` File/Table/Snapshot §3.1 |
| `Schema` | struct with `Vec<SchemaColumn>` §3.2 |
| `SchemaColumn` | struct `{ name, data_type, nullable }` §3.2 |
| `CatalogRef` | struct `{ catalog_id, fqn }` §3.3 |
| `PartitionColumn` | struct `{ name, position (1-indexed), data_type, nullable }` §3.4 |
| `FileFormat` | `enum` Parquet/Csv/Json/Orc/Avro §4.1 |
| `CsvOptions` | struct §4.2 |
| `JsonOptions` | struct §4.3 |
| `ColumnMapping` | struct holding `BTreeMap<SemanticsName, ColumnMappingValue>` §5.1 |
| `ColumnMappingValue` | `enum` Column/Literal/Computed/Metadata §5.1 |
| `Coverage` | struct holding `HashMap<CoverageKey, CoverageVariant>` §6.1 |
| `CoverageVariant` | `enum` Native/NullFill/Derived §6.1 |
| `ResolvedBinding` | struct §7.6 |
| `ResolvedColumnMapping` | struct with four flat HashMaps + coverage §7.2 |
| `ResolvedLiteral` | struct `{ value, data_type }` §7.2 |

Everything in this table is `pub`-visible in `semstrait-manifest` (post-resolve) or `semstrait-model` (pre-resolve), per `30 §4` / `33`'s final roster.

---

**End of document.** Open reconciliation items and decisions parked for round-2 review are in `docs/design/open_questions/15_open_questions.md`.
