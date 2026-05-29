---
prereqs: [00, 10, 11, 12, 13, 14, 14a, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 30, 31, 31b, 35]
authoritative-for:
  - the root YAML shape for a `semstrait` model — `semantic_model:` wrapper, per-variant plural arrays, shared Semantics pools, `relationships:`
  - the in-memory `SemanticModel` root type — every entity collection (data kinds, shared pools, relationships) is an `EntityId`-keyed `BTreeMap` with name-ordered iteration/serialization
  - the DataKind type hierarchy — `DataKindBase<E>` common-fields struct generic over the per-axis extras flavor, per-variant `*Body` structs, `Public*` / `Nested*` concrete types, sealed `DataKind` trait hierarchy on structural + behavioral axes, and view enums for heterogeneous iteration
  - the named-entity identity policy at model boundary — an `id` field (`EntityId`, UUIDv7 string) carried directly on every named entity struct, optional at authoring, profile-controlled missing-id generation at parse, consumed by `33`
  - the per-axis extras shapes — `LeafExtras` (full set) and `ComplexExtras` (`temporal:` only)
  - structural rules (SR-*) that govern a valid root-level document
  - the `parse` and `validate` free-function signatures, the `ParseErrorKind` and `ValidateError` rosters (per `30 §5`), and their `Diagnose` impls
  - the `ExprSource` enum carried at every expression-bearing site — `Inline(String)` / `Block(Expr<L>)` variants per `14 §6.1` — and the parse-site dispatch (`parse_semantic` / `parse_physical`) per `14 §6.2`
  - the `semstrait-model::io` submodule — `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs` wrappers, `DumpMode`, and the load / dump error rosters (composes `31b` transport)
  - deterministic-ordering guarantees at the root level (I4)
  - crate boundaries for `semstrait-model`
refined-by:
  - 32b (`apis/32b_catalogs_yaml.md` — catalog YAML grammar and reference syntax)
  - 33 (`apis/33_semstrait_manifest.md` — how the `SemanticModel` tree lowers to a `SemanticManifest`)
# Upstream cross-references (see `prereqs:` above and §11 "Pointers to Child Docs"
# for full context): 18 (entity struct shapes), 15 (SemanticMapping compile
# semantics), 16 (composition), 17 (temporal-shape planner semantics), 21-24
# (per-DataKind YAML), 26 (nesting matrix), 31b (I/O transport). Per 00 §8
# directionality rule, those are prerequisites rather than downstream refinements;
# they are deliberately omitted from `refined-by:` to keep the field semantically
# pure. Section 11 of this doc is the authoritative human-facing navigation aid
# for the full cross-reference web.
---

# 32. `semstrait-model` — Root YAML Contract

`32` fixes the root-shape YAML surface of a `semstrait` model and the in-memory `SemanticModel` type. Per-variant grammar lives in `21`–`24`. Catalog authoring lives in `32b`. Nesting lives in `26`.

## 1. Root YAML Shape

A model YAML file has exactly one root key: `semantic_model:`. No other root-level key is recognized (SR-1).

```yaml
semantic_model:
  name: analytics-v1
  description: "Primary analytics model for the order pipeline."
  ai_context:
    synonyms: { order: [purchase, txn] }
  labels: [analytics, prod]

  # ── Data kinds — four per-variant plural arrays ─────────────────
  datasets:   [ ... ]                  # see 21_dataset.md
  grainsets:  [ ... ]                  # see 22_grainset.md
  unionsets:  [ ... ]                  # see 23_unionset.md
  joinsets:   [ ... ]                  # see 24_joinset.md

  # ── Shared Semantics pools — one map per carrier ────────────────
  # Full entity shapes + ref/override grammar — 18 §4–§9.
  dimensions: [ ... ]                  # see 18 §4
  measures:   [ ... ]                  # see 18 §6
  metrics:    [ ... ]                  # see 18 §7

  # ── Cross-entity relationships — see 18 §2 for full grammar ────
  relationships:
    - name: orders_to_customers
      from: orders
      to:   customers
      keys: [{ from: customer_id, to: id }]
      cardinality: many_to_one          # required
      integrity: assumed                # default; alternatives: enforced | none
      optional: none                    # default per 18 §2.7; required on 1:1 / m:m
      cross_filter: left                # default per 18 §2.7; required on 1:1 / m:m
```

Every child block is optional except `name:`. An empty model — `semantic_model: { name: ... }` — parses successfully. A non-empty model with zero data kinds is a `ValidateError::EmptyModel` at the validate stage (§9.5).

### 1.1 Per-variant plural arrays

Each data-kind variant is authored under its own plural tag: `datasets:`, `grainsets:`, `unionsets:`, `joinsets:`. There is no unified parent block and no `kind:` discriminator on individual entries.

### 1.2 Top-level vs nested data kinds

Only top-level data kinds — those authored directly under one of the four plural arrays — expose a `SemanticInterface` (dimensions / measures / metrics / keys / filters). Nested data kinds (authored inside a complex parent's child arrays per `26`) are structural shells: they carry `name`, `description`, `extras`, and variant-specific structural fields only. They do not author their own Semantics.

### 1.3 Catalog reference

Physical-source catalogs are authored in a sibling file, `catalogs.yaml`, per `32b`. A model references a catalog via the `catalog:` key inside an `extras:` block. The reference is a **bare alias string** — there is no namespace override or map form:

```yaml
extras:
  catalog: polaris_prod
```

`CatalogRef` is a transparent newtype around the alias; resolution against `catalogs.yaml` happens at compile. Full grammar lives in `32b §4`.

### 1.4 Optional named-entity `id`

Model authoring may include an optional `id` field on named-entity blocks:

- top-level and nested DataKinds,
- shared Semantics entries (`dimensions`, `measures`, `metrics`),
- `relationships` entries.

`id` is a string and, when present, MUST be a canonical UUIDv7 text form (lowercase, hyphenated). When omitted, missing-id behavior is profile-controlled (`§9.0.1`).

```yaml
semantic_model:
  name: analytics-v1
  datasets:
    - id: "0197cb6e-a63a-7d53-8d6f-35f4b1d67d4e"
      name: orders
      extras: { ... }
  relationships:
    - id: "0197cb6e-a63a-7d53-9e77-17f07341da5c"
      name: orders_to_customers
      from: orders
      to: customers
      keys: [{ from: customer_id, to: id }]
      cardinality: many_to_one
```

The `id` field is part of the public payload: it is a field on every named-entity struct (`DataKindBase` for data kinds; `Dimension` / `Measure` / `Metric` / `Relationship` / `Key` / `Filter` per `18`). When omitted at authoring, parse generates a UUIDv7 under the convenience profile (§9.0.1). It is the storage key for every model collection (§2).

---

## 2. `SemanticModel` Root Type

```rust
/// Canonical UUIDv7 text (lowercase, hyphenated). Authored optionally; parse
/// generates one per missing named entity under the convenience profile (§9.0.1).
pub type EntityId = String;

#[non_exhaustive]
pub struct SemanticModel {
    pub name: String,
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub labels: Vec<String>,

    // Data kinds — per-variant maps keyed by EntityId.
    pub datasets:  BTreeMap<EntityId, Dataset>,
    pub grainsets: BTreeMap<EntityId, Grainset>,
    pub unionsets: BTreeMap<EntityId, Unionset>,
    pub joinsets:  BTreeMap<EntityId, Joinset>,

    // Shared Semantics pools — keyed by EntityId.
    pub dimensions: BTreeMap<EntityId, Dimension>,
    pub measures:   BTreeMap<EntityId, Measure>,
    pub metrics:    BTreeMap<EntityId, Metric>,

    // Cross-entity relationships — keyed by EntityId.
    pub relationships: BTreeMap<EntityId, Relationship>,
}
```

All semantic-payload fields in the public roster are `pub` so consumers can destructure without getter boilerplate. Construction outside `parse` is permitted (test harnesses, tooling).

Every collection is keyed by the entity's own `id` (the `EntityId` carried inside each value). Keying by `id` rather than by name lets the model retain *every* authored entity — including two entries that share a name — so duplicate-by-name detection runs as an explicit per-layer scan at `.build()` (§9.0) instead of being silently masked by map-key collision. Name lookup is therefore a scan or a caller-built index, not a direct map hit (§2.2). Iteration and serialization are name-ordered (§7) so identity-keyed storage does not perturb deterministic output.

### 2.1 Global name uniqueness

Data-kind names are globally unique across the four top-level maps: a `Dataset` named `sales` and a `Grainset` named `sales` cannot both exist. Because the maps are keyed by `EntityId`, two same-named entities do *not* collide on insert — both are retained and the clash is surfaced by the global name scan at `SemanticModelBuilder::build`: `ValidateError::DuplicateDataKindName` (SR-3) — fired uniformly over single-source, code-built, and cross-source accumulations (D-10).

Shared pools use their own namespace per carrier: a Dimension named `region` and a Measure named `region` can coexist. Duplicate names within a carrier raise `ValidateError::DuplicateSharedSemanticsName` via the same `.build()` per-carrier name scan.

Nested data kinds use parent-scoped uniqueness (addressing becomes `grainsets[sales].unionsets[regional]`); see `26 §5`.

### 2.2 Cross-variant iteration helpers

For diagnostics and walks that treat top-level data kinds uniformly, `SemanticModel` exposes iterators that yield the view enums defined in §3.6.

```rust
impl SemanticModel {
    pub fn iter_all(&self)     -> impl Iterator<Item = AnyDataKindRef<'_>>;
    pub fn iter_public(&self)  -> impl Iterator<Item = PublicDataKindRef<'_>>;
    pub fn iter_simple(&self)  -> impl Iterator<Item = SimpleDataKindRef<'_>>;
    pub fn iter_complex(&self) -> impl Iterator<Item = ComplexDataKindRef<'_>>;

    pub fn find_public(&self, name: &str) -> Option<PublicDataKindRef<'_>>;
}
```

`iter_all` walks only top-level public data kinds (it does not descend into bodies); nested data kinds are reached through `ComplexDataKind::children_ref` on each visited public composer. All view enums are defined in §3.6.

### 2.3 Named-entity identity is an inline field

There is no separate identity sidecar. Each named entity carries its own `id: EntityId` (canonical UUIDv7 text) directly on its struct — `DataKindBase` for data kinds (§3.1), and `Dimension` / `Measure` / `Metric` / `Relationship` / `Key` / `Filter` per `18`. That `id` is also the storage key of the collection the entity lives in (§2), so identity, payload, and storage key are one and the same.

The model root itself is addressed by `name`; it carries no separate `id`. Compile (`33`) reads each entity's `id` to build the manifest stable-id propagation map (`33 §4.3.1`); no path-keyed index is materialized at the model layer.

---

## 3. DataKind Type Hierarchy

Six layers: a common-fields struct, per-variant shared bodies, concrete types in two forms, a sealed trait hierarchy on two axes, per-concrete trait impls, and view enums for heterogeneous iteration.

### 3.1 Common-fields struct — `DataKindBase<E>`

```rust
pub struct DataKindBase<E> {
    pub id:     EntityId,   // authored or parse-generated UUIDv7 text
    pub name:   String,
    pub extras: E,
}
```

Held inside every per-variant body (§3.2). Carries the universal fields every data kind exposes regardless of variant or form: an `id` (the entity's `EntityId`, also its storage key per §2), a `name` (anchoring + structural label per `26 §4`), and an `extras` block parameterized over the per-axis flavor — `LeafExtras` for the leaf body and `ComplexExtras` for the three composer bodies (§4). Because `id` lives on the base, all eight concrete DataKind types (Public + Nested) carry it through `body.base`.

`description`, `ai_context`, and `semantic_interface` are NOT on the base — they are Public-form-only, and live on each Public concrete type directly (§3.3).

### 3.2 Per-variant bodies

Each variant has a single `*Body` struct holding `base: DataKindBase<E>` (§3.1) plus variant-intrinsic structural fields. The `<E>` parameter is `LeafExtras` for the leaf body and `ComplexExtras` for the three composer bodies — the type-level expression of R-6 (`storage` / `catalog` / `semantic_mapping` are leaf-only). Public and Nested forms of the same variant wrap the same body (§3.3).

Self-nesting is type-level forbidden by field absence: no `grainsets:` field on `GrainsetBody`, no `unionsets:` field on `UnionsetBody`, no `joinsets:` field on `JoinsetBody`.

```rust
pub struct DatasetBody {
    pub base: DataKindBase<LeafExtras>,
}

pub struct GrainsetBody {
    pub base:      DataKindBase<ComplexExtras>,
    pub datasets:  BTreeMap<EntityId, NestedDataset>,
    pub unionsets: BTreeMap<EntityId, NestedUnionset>,
    pub joinsets:  BTreeMap<EntityId, NestedJoinset>,
}

pub struct UnionsetBody {
    pub base:      DataKindBase<ComplexExtras>,
    pub datasets:  BTreeMap<EntityId, NestedDataset>,
    pub grainsets: BTreeMap<EntityId, NestedGrainset>,
    pub joinsets:  BTreeMap<EntityId, NestedJoinset>,
    pub mode:      UnionMode,
}

pub struct JoinsetBody {
    pub base:          DataKindBase<ComplexExtras>,
    pub datasets:      BTreeMap<EntityId, NestedDataset>,
    pub grainsets:     BTreeMap<EntityId, NestedGrainset>,
    pub unionsets:     BTreeMap<EntityId, NestedUnionset>,
    /// Joinset-local relationships. Unified `Relationship` shape — same
    /// struct as `SemanticModel.relationships`. See `18 §2`.
    pub relationships: BTreeMap<EntityId, Relationship>,
}

#[non_exhaustive]
pub enum UnionMode { All, Unique }   // `All` is the default
```

The full nesting matrix is at `26 §1`; structural rules (R1 leaves don't nest; R2 no same-variant self-nesting; R3 ComplexDataKind ≥ 2 children) are at `26 §2`. `Relationship` shape is ratified at `18 §2`; `UnionMode` roster at `23 §4.1` (variant-local).

### 3.3 Concrete types — Public and Nested forms

Each variant has two concrete forms. Public wraps a body plus three Public-form-only fields — `description`, `ai_context`, `semantic_interface`. Nested wraps only the body.

```rust
// Public (top-level) forms — carry description + ai_context + interface.
pub struct Dataset {
    pub body:               DatasetBody,
    pub description:        Option<String>,
    pub ai_context:         Option<AiContext>,
    pub semantic_interface: SemanticInterface,
}
pub struct Grainset {
    pub body:               GrainsetBody,
    pub description:        Option<String>,
    pub ai_context:         Option<AiContext>,
    pub semantic_interface: SemanticInterface,
}
pub struct Unionset {
    pub body:               UnionsetBody,
    pub description:        Option<String>,
    pub ai_context:         Option<AiContext>,
    pub semantic_interface: SemanticInterface,
}
pub struct Joinset {
    pub body:               JoinsetBody,
    pub description:        Option<String>,
    pub ai_context:         Option<AiContext>,
    pub semantic_interface: SemanticInterface,
}

// Nested (structural) forms — body only.
pub struct NestedDataset  { pub body: DatasetBody }
pub struct NestedGrainset { pub body: GrainsetBody }
pub struct NestedUnionset { pub body: UnionsetBody }
pub struct NestedJoinset  { pub body: JoinsetBody }
```

YAML deserialization uses `#[serde(flatten)]` on the `body:` field so per-variant author YAML reads as one flat object.

### 3.4 Sealed trait hierarchy — `DataKind` + two axes

All traits are sealed inside the crate. The base trait `DataKind` carries only the universal name + tag accessors; two orthogonal axes of sub-traits classify *structural* shape (leaf vs composer) and *behavioral* shape (queryable top-level vs structural shell). Each axis trait owns the accessors specific to its axis.

```rust
mod sealed { pub trait Sealed {} }

pub trait DataKind: sealed::Sealed {
    fn name(&self) -> &str;
    fn variant(&self) -> DataKindVariant;
    fn form(&self) -> DataKindForm;
}

// ── Structural axis ──────────────────────────────────────────────
// Each subtype distributes its own `extras` flavor (LeafExtras vs ComplexExtras)
// per the `DataKindBase<E>` parameterization in §3.1 / §3.2.
pub trait SimpleDataKind: DataKind {
    fn extras(&self) -> &LeafExtras;
}

pub trait ComplexDataKind: DataKind {
    fn extras(&self) -> &ComplexExtras;
    fn allowed_child_variants(&self) -> &'static [DataKindVariant];
    fn child_count(&self) -> usize;
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_>;
}

// ── Behavioral axis ──────────────────────────────────────────────
// PublicDataKind owns the three Public-form-only accessors; NestedDataKind
// is a pure marker.
pub trait PublicDataKind: DataKind {
    fn description(&self) -> Option<&str>;
    fn ai_context(&self) -> Option<&AiContext>;
    fn semantic_interface(&self) -> &SemanticInterface;
}

pub trait NestedDataKind: DataKind {}

#[non_exhaustive]
pub enum DataKindVariant { Dataset, Grainset, Unionset, Joinset }

#[non_exhaustive]
pub enum DataKindForm { Public, Nested }
```

`NestedDataKind` is a pure marker: its contribution is the trait bound itself, which lets generic code require nested-ness without inspecting tags. The other three sub-traits carry axis-specific accessors. `SimpleDataKind::extras` and `ComplexDataKind::extras` differ in return type by design — leaf vs complex extras shapes are not interchangeable (§4).

### 3.5 Trait implementation matrix

Every concrete type implements `DataKind`, exactly one trait on the structural axis, and exactly one trait on the behavioral axis.


| Concrete         | `DataKind` | Structural axis   | Behavioral axis  |
| ---------------- | ---------- | ----------------- | ---------------- |
| `Dataset`        | ✓          | `SimpleDataKind`  | `PublicDataKind` |
| `NestedDataset`  | ✓          | `SimpleDataKind`  | `NestedDataKind` |
| `Grainset`       | ✓          | `ComplexDataKind` | `PublicDataKind` |
| `NestedGrainset` | ✓          | `ComplexDataKind` | `NestedDataKind` |
| `Unionset`       | ✓          | `ComplexDataKind` | `PublicDataKind` |
| `NestedUnionset` | ✓          | `ComplexDataKind` | `NestedDataKind` |
| `Joinset`        | ✓          | `ComplexDataKind` | `PublicDataKind` |
| `NestedJoinset`  | ✓          | `ComplexDataKind` | `NestedDataKind` |


### 3.6 View enums for heterogeneous iteration

Five view-only enums provide exhaustive-match access over borrowed concrete values. Each view implements the subset of the trait hierarchy common to its members, so generic trait-bounded functions accept views as first-class `DataKind` values.

```rust
#[non_exhaustive]
pub enum AnyDataKindRef<'a> {
    Dataset(&'a Dataset),
    NestedDataset(&'a NestedDataset),
    Grainset(&'a Grainset),
    NestedGrainset(&'a NestedGrainset),
    Unionset(&'a Unionset),
    NestedUnionset(&'a NestedUnionset),
    Joinset(&'a Joinset),
    NestedJoinset(&'a NestedJoinset),
}

#[non_exhaustive]
pub enum PublicDataKindRef<'a> {
    Dataset(&'a Dataset),
    Grainset(&'a Grainset),
    Unionset(&'a Unionset),
    Joinset(&'a Joinset),
}

#[non_exhaustive]
pub enum NestedDataKindRef<'a> {
    Dataset(&'a NestedDataset),
    Grainset(&'a NestedGrainset),
    Unionset(&'a NestedUnionset),
    Joinset(&'a NestedJoinset),
}

#[non_exhaustive]
pub enum SimpleDataKindRef<'a> {
    Public(&'a Dataset),
    Nested(&'a NestedDataset),
}

#[non_exhaustive]
pub enum ComplexDataKindRef<'a> {
    Grainset(&'a Grainset),
    NestedGrainset(&'a NestedGrainset),
    Unionset(&'a Unionset),
    NestedUnionset(&'a NestedUnionset),
    Joinset(&'a Joinset),
    NestedJoinset(&'a NestedJoinset),
}
```

Trait implementations for views:


| View enum            | Implements                     |
| -------------------- | ------------------------------ |
| `AnyDataKindRef`     | `DataKind`                     |
| `PublicDataKindRef`  | `DataKind` + `PublicDataKind`  |
| `NestedDataKindRef`  | `DataKind` + `NestedDataKind`  |
| `SimpleDataKindRef`  | `DataKind` + `SimpleDataKind`  |
| `ComplexDataKindRef` | `DataKind` + `ComplexDataKind` |


Every view method dispatches via `match` to the underlying concrete type. Views own no data and are never persisted; they are constructed on demand by the iterators in §2.2 and by `ComplexDataKind::children_ref` on complex bodies.

### 3.7 Storage layout

- `SemanticModel.{datasets, grainsets, unionsets, joinsets}` — `BTreeMap<EntityId, _>` keyed by entity `id`, holding the four **Public** concrete types.
- `SemanticModel.{dimensions, measures, metrics}` and `SemanticModel.relationships` — `BTreeMap<EntityId, _>` keyed by entity `id`.
- `*Body.{datasets, grainsets, unionsets, joinsets}` and `JoinsetBody.relationships` — `BTreeMap<EntityId, _>` holding the **Nested** concrete types (and Joinset-local `Relationship`s), constrained by each body's declared child set.
- `SemanticModelBuilder` lowers decoded YAML straight into these id-keyed maps (§9.0); there is no separate accumulation layout, handle table, or name index — duplicate-by-name detection is an explicit per-layer scan at `.build()`.

The Public / Nested split is enforced at the type level: no Public-form value can appear inside a body's child vector, and no Nested-form value can appear at the top-level map.

---

## 4. The `Extras` Blocks — `LeafExtras` and `ComplexExtras`

Two `Extras` types, one per structural axis. The split is the type-level expression of R-6 (`storage` / `catalog` / `semantic_mapping` are leaf-only — never authored on a Complex variant):

```rust
#[non_exhaustive]
pub struct LeafExtras {
    pub catalog:          Option<CatalogRef>,
    pub storage:          Option<StorageConfig>,
    pub semantic_mapping: Option<SemanticMapping>,
    pub temporal:         Option<TemporalShape>,
}

#[non_exhaustive]
pub struct ComplexExtras {
    pub temporal:         Option<TemporalShape>,
}
```

Both apply `#[serde(deny_unknown_fields)]`. A YAML author who places `catalog:` / `storage:` / `semantic_mapping:` under a Complex variant's `extras:` block hits `ParseErrorKind::UnknownField` at parse time — the field has no slot to deserialize into.

`StorageConfig` carries the physical-source list (`paths:` for files / folders / globs; `tables:` for catalog FQNs / table-name globs) and an optional declared partition layout (`partition_def:`):

```rust
#[non_exhaustive]
pub struct StorageConfig {
    /// Storage format for path-based sources (`Parquet`, `Csv`, `Json`,
    /// `Orc`, `Avro`). Required when `paths` is non-empty; ignored when
    /// only `tables` is authored (catalog metadata supplies the format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<StorageFormat>,

    /// File / folder / glob URIs. Each entry resolves at compile to one
    /// or more `PhysicalSource::File` per `15 §3.5`: a concrete path or
    /// folder URI produces one `PhysicalSource`; a wildcard path produces
    /// one `PhysicalSource` per resolved variation. Each `PhysicalSource`
    /// is an engine-level LogicalRelation — the engine handles file
    /// consolidation, schema merge, and Hive-partition discovery
    /// internally per `35 §5.2.1`. Empty by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,

    /// Catalog FQNs or table-name globs. Each entry resolves at compile
    /// to one or more `PhysicalSource::Table` per `15 §3.5`: a concrete
    /// FQN produces one `PhysicalSource`; a table-name glob produces one
    /// `PhysicalSource` per FQN enumerated by `CatalogProvider::list_tables`.
    /// Empty by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<String>,

    /// Catalog-less partition declaration for file sources (Range / List).
    /// First-class v1 author surface; **runtime-dormant in v1** — parsed,
    /// schema-validated, and carried through compile for v2+ partition-
    /// aware planning. v1 adapters defer partition pruning to engine-side
    /// discovery from filter predicates per `35 §5.2.1`. See `Q-MAP-002`
    /// for the layered partition-info plumbing decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_def: Option<PartitionDef>,
}
```

`PartitionDef` describes the on-disk partitioning layout for a physical source. At least one of `paths:` / `tables:` must be non-empty; both being empty is a structural error caught at validate (per the leaf-source rule in `15 §2.1`).

YAML tag names mirror field names 1:1. `TemporalShape` uses the collapsed variant-wrapper form ratified in `18 §3.2`. Three example shapes:

```yaml
# Path-based source (recommended form: table-root prefix; engine handles
# Hive-partition discovery and file consolidation internally).
extras:
  catalog: polaris_prod
  storage:
    format: parquet
    paths:
      - "s3://bucket/orders/"
    partition_def:
      type: range
      column: order_date
  semantic_mapping: auto
  temporal:
    timeseries:
      occurred_at: order_date
    grain: day
```

```yaml
# Table-based source (catalog-resolved; format omitted — supplied by catalog).
extras:
  catalog: polaris_prod
  storage:
    tables:
      - "warehouse.sales.orders"
```

```yaml
# Mixed source list (Union ALL across one path-based and one table-based
# `PhysicalSource`; per `21 §3.2 / §4.5`).
extras:
  catalog: polaris_prod
  storage:
    format: parquet
    paths:
      - "s3://bucket/legacy_orders/"
    tables:
      - "warehouse.sales.orders_current"
```

**Authoring guidance — table-root preferred for file sources.** When a path is a Hive-partitioned table or a single-table folder containing many files, the recommended form is the **table-root prefix only** (e.g. `"s3://bucket/orders/"`), not a Hive-partition glob (e.g. `"s3://bucket/orders/year=*/month=*/*.parquet"`). The latter is wrong usage of the wildcarding surface — it forces compile to enumerate per file or per partition, when the engine can do the same far more efficiently from the table-root alone (per `35 §5.2.1`'s 4-consumer alignment). Compile resolves whatever the author writes literally; it does not detect or reject this pattern. See `15 §3.5` for the resolution rule.

### 4.1 Per-effective-level validity

Each extras field is constrained by **two independent type-level rules**: which extras flavor (Leaf / Complex) carries the field, and which structural-axis trait owns it. Cascade-from-ancestor remains in v1 only for `temporal.<variant>:` (the shape kind); no other extras field cascades.


| Field                                    | Carrier                                               | Effective at                              | Cascadable from ancestor?                                 |
| ---------------------------------------- | ----------------------------------------------------- | ----------------------------------------- | --------------------------------------------------------- |
| `catalog`                                | `LeafExtras`                                          | Leaf (`Dataset` / `NestedDataset`)        | **No** — type-level (not in `ComplexExtras`)              |
| `storage` (incl. nested `partition_def`) | `LeafExtras`                                          | Leaf                                      | **No** — type-level                                       |
| `semantic_mapping`                       | `LeafExtras`                                          | Leaf (default `auto` when absent)         | **No** — type-level                                       |
| `temporal.<variant>:` (shape kind)       | both                                                  | Leaf; inherited from any ancestor complex | **Yes** (more-specific-overrides-default merge)           |
| `temporal.grain:`                        | `LeafExtras` only — **forbidden** on Complex (SR-E-7) | Leaf `Dataset` only                       | **No** (SR-E-8: Grainset children must author explicitly) |


The cascade rule applies to the variant-tag layer of `temporal:` only. An author may declare `temporal: { timeseries: {...} }` on a root grainset and have that shape kind cascade to every leaf descendant; the `grain:` value never cascades.

Authoring `temporal:` at a scope with no eligible descendant is a structural warning (dead config, not fatal). Entity-level invariants (SR-E-6 through SR-E-8) live at `18 §11`.

### 4.2 Variant-intrinsic fields that are NOT in either `Extras` flavor

- `UnionMode` is a direct field on `UnionsetBody`. Always required; default `All`. Roster `{All, Unique}` — see `23 §4.1` for the full enum and `16 §5` for composition semantics.
- `relationships` on `JoinsetBody` is the variant's intrinsic structural field — never overridable through extras. Uses the unified `Relationship` shape per `18 §2`.
- `SemanticInterface` (dimensions / measures / metrics / keys / filters) is authored directly on every Public form, never in extras. Entry grammar (inline vs `ref`, override scope) lives at `18 §1`.

---

## 5. Semantic Mapping and Binding

### 5.1 `semantic_mapping` — authoring grammar

`semantic_mapping` lives in `extras`. Two authoring forms:

**Implicit default (`auto`).** Absent value or `semantic_mapping: auto` means the compiler treats every Semantic name as a 1:1 match to a physical column by the same name. Referential integrity is the author's responsibility.

```yaml
# Both are equivalent — `auto` is the default when omitted.
extras: {}
extras:
  semantic_mapping: auto
```

**Explicit map.** The author provides a `{ semantic_name: <SemanticMappingValue> }` mapping. Each entry's value is one of **three author-facing variants** (`Column` / `Literal` / `Expr`) — the full `SemanticMappingValue` enum is **4-variant** at the type level (`Column` / `Literal` / `Expr` / `Metadata`), with the 4th `Metadata(MetadataDimensionRecipe)` variant **compile-synthesized only** from the Dimension's own `type: { metadata: ... }` block (per `13 §4.7` / `18 §10.4` / `15 §5.5`). The 4th variant has no `semantic_mapping:` YAML form and is therefore not authored here. The full enum (shape + roster + `LiteralValue` grammar + `MetadataDimensionRecipe`) is ratified in `[18 §10](../foundations/18_entities.md#10-semanticmapping-value-shape)` and consumed by the `Binding` process in `[15 §5](../foundations/15_mapping_and_binding.md)`:

```yaml
extras:
  semantic_mapping:
    # Variant 1 — Column (bare string).
    revenue: net_revenue_cents
    country: region_lookup

    # Variant 2 — Literal broadcast.
    currency: { literal: "USD" }

    # Variant 3 — PhysicalExpr.
    hour_bucket:
      expr:
        trunc:
          column: event_ts
          unit: hour
```

Single-string values dispatch to `Column`; mapping values with `literal:` / `expr:` keys dispatch to `Literal` / `Expr`. `PhysicalExpr` grammar is at `14 §3`.

### 5.2 Leaf-only effective scope

`semantic_mapping` is effective only at leaves (`Dataset` / `NestedDataset`). Complex data kinds may carry `semantic_mapping` in their `extras` as a default for descendant leaves, but never as the complex kind's own mapping (complex kinds have no direct physical surface).

### 5.3 The `Binding` process

`Binding` is the internal compile-time process that consumes `semantic_mapping` plus the manifest-resolved `PhysicalSource` (schemas, partition metadata, catalog lookups) to produce executable physical expressions. It is a process, not a model-layer type.

- **Input:** `semantic_mapping` from the model; `PhysicalSource` from manifest-layer catalog resolution.
- **Output:** a resolved mapping of each Semantic name to a `PhysicalExpr` tree, folded into the manifest's per-leaf resolved interface.
- **Stage:** runs at `compile` (per `10 §4`); never at parse or plan time.

See `15` for the full binding algorithm; `33` for where the binding output lives.

### 5.4 Aggregate synthesis from `(agg:, expr:)`

Measures and Metrics carry separate `agg:` and `expr:` fields in YAML. At **parse time**, `semstrait-model` wraps these into a single `Aggregate`-rooted `SemanticExpr`:

```text
agg: sum
expr: "revenue * quantity"
  ─→  SemanticExpr::Aggregate {
         op:       AggregationOp::Sum,
         args:     [parse_semantic("revenue * quantity")],
         distinct: false,  // default; `distinct: true` is an explicit author field
         filter:   None,   // populated from `filter:` if present
       }
```

By the time `[19 §3](../foundations/19_expression_flow.md)`'s `resolve` runs, the `SemanticExpr` already carries the `Aggregate` root if a Measure or Metric authored it. The resolution algorithm does not synthesize `Aggregate` — it only resolves the inner `args` tree.

This is a parse-time structural assembly, not a compile-time transformation. The dual-field `(agg:, expr:)` surface is author ergonomics; the canonical representation is always `Expr<L>::Aggregate`.

---

## 6. Structural Rules (SR-*)

Root-level invariants enforced at `parse` and the `validate` stage. Each rule maps to a typed-kind variant in `ParseErrorKind` (§9.2) or `ValidateError` (§9.5) per `30 §5`.


| ID        | Rule                                                                                                                                                                                                                                                                                                                                                                                                          | Kind                                                     |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| **SR-1**  | Exactly one `semantic_model:` root key; `deny_unknown_fields` at root.                                                                                                                                                                                                                                                                                                                                        | `ParseErrorKind::UnknownField`                           |
| **SR-2**  | Nested data kinds MUST NOT carry `description`, `ai_context`, `dimensions`, `measures`, `metrics`, `keys`, `filters`. Enforced at the type level: `Nested`* structs (§3.3) wrap only a `*Body` — they have no `description`, `ai_context`, or `semantic_interface` fields — and implement `NestedDataKind` (§3.4) as the behavioral marker; `deny_unknown_fields` then rejects the Public-only tags at parse. | `ParseErrorKind::UnknownField`                           |
| **SR-3**  | Names are globally unique across the four top-level data-kind maps (§2.1). Fired at `SemanticModelBuilder::build` (D-10).                                                                                                                                                                                                                                                                                     | `ValidateError::DuplicateDataKindName`               |
| **SR-4**  | Same-variant self-nesting is forbidden: no grainset inside a grainset, no unionset inside a unionset, no joinset inside a joinset. Dataset leaves do not nest. Enforced at the type level by each `*Body` struct's child-field set (§3.2); `deny_unknown_fields` then rejects same-variant tags at parse.                                                                                                     | `ParseErrorKind::UnknownField`                           |
| **SR-5**  | `catalog`, `storage`, `semantic_mapping` are leaf-only — they live on `LeafExtras` and have no slot in `ComplexExtras`. Authoring any of them under a Complex variant's `extras:` block is a parse error. Cascade-from-ancestor does not apply to these fields (R-6 / §4.1).                                                                                                                                  | `ParseErrorKind::UnknownField`                           |
| **SR-6**  | _Retired._ Per-variant required-extras rules live in `21 §7` / `22` / `23` / `24` (each chapter owns its own `VALID_E_2[1-4]xx` code band); the Grainset-child-temporal case is covered end-to-end by SR-E-8 (`18 §11`).                                                                                                                                                                                      | —                                                        |
| **SR-7**  | `deny_unknown_fields` is applied at every struct parse site (model root, data-kind blocks, extras, relationships, semantic elements).                                                                                                                                                                                                                                                                         | `ParseErrorKind::UnknownField`                           |
| **SR-8**  | Identifier rules: data-kind names and semantic-element names follow `11 §13`.                                                                                                                                                                                                                                                                                                                                 | `ParseErrorKind::InvalidIdentifier`                      |
| **SR-9**  | `${VAR}` substitution is applied before YAML decoding; unset variables are fatal parse errors (§8).                                                                                                                                                                                                                                                                                                           | `ParseErrorKind::UnsetEnvVar`                            |
| **SR-10** | Every `ComplexDataKind` (`Grainset` / `Unionset` / `Joinset`, Public or Nested) MUST have at least **2 children** across its allowed child-variant arrays. A composer with 0 or 1 children is degenerate — 0 collapses to nothing; 1 collapses to the single child's interface and should be authored as that child directly. Enforced at `validate`.                                                         | `ValidateError::ComplexDataKindInsufficientChildren` |
| **SR-11** | Optional named-entity `id`, when authored, MUST be canonical UUIDv7 text (`xxxxxxxx-xxxx-7xxx-xxxx-xxxxxxxxxxxx`, lowercase).                                                                                                                                                                                                                                                                               | `ParseErrorKind::InvalidEntityId`                        |
| **SR-12** | Named-entity `id` values are globally unique across the model (`data kinds + semantics + relationships`). Duplicate ids are rejected at build/validate boundary.                                                                                                                                                                                                                                             | `ValidateError::DuplicateEntityId`                       |


SR-* numbering is append-only. Adding a rule is a MINOR change per `30 §2`.

Entity-level invariants (`SR-E-`*) — reference-site overrides, Semantics orphan policy, relationship cardinality, TemporalShape grain placement, filter-kind disjointness — live at `18 §11`. SR-E-* is a separate, independently-numbered series that extends the root-level SR-* roster.

---

## 7. Deterministic Ordering (I4)

Given the same input YAML plus the same environment, `parse` produces a byte-identical serialized `SemanticModel` under any canonical serializer.

Every entity collection is keyed by `EntityId`, but the `EntityId` is **never** the ordering axis for output. Iteration helpers (§2.2) and serde serialization project each map into a **name-ordered** sequence, so deterministic output does not depend on id values. This keeps I4 byte-stable even under the convenience profile, where generated UUIDv7 ids vary across fresh parses.


| Collection                                                  | Type                             | Ordering rule (iteration / serialization)                                                            |
| ----------------------------------------------------------- | -------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `SemanticModel.datasets`                                    | `BTreeMap<EntityId, Dataset>`    | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.grainsets`                                   | `BTreeMap<EntityId, Grainset>`   | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.unionsets`                                   | `BTreeMap<EntityId, Unionset>`   | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.joinsets`                                    | `BTreeMap<EntityId, Joinset>`    | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.dimensions`                                  | `BTreeMap<EntityId, Dimension>`  | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.measures`                                    | `BTreeMap<EntityId, Measure>`    | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.metrics`                                     | `BTreeMap<EntityId, Metric>`     | Alphabetical by name (name-ordered projection)                                                       |
| `SemanticModel.labels`                                      | `Vec<String>`                    | YAML author order                                                                                    |
| `SemanticModel.relationships`                               | `BTreeMap<EntityId, Relationship>` | Alphabetical by name (name-ordered projection); first-match-wins now resolves in name order — see note below and `16 §11` |
| `GrainsetBody.{datasets, unionsets, joinsets}`              | `BTreeMap<EntityId, Nested*>`    | Alphabetical by name (name-ordered projection)                                                       |
| `UnionsetBody.{datasets, grainsets, joinsets}`              | `BTreeMap<EntityId, Nested*>`    | Alphabetical by name (name-ordered projection)                                                       |
| `JoinsetBody.{datasets, grainsets, unionsets}`              | `BTreeMap<EntityId, Nested*>`    | Alphabetical by name (name-ordered projection)                                                       |
| `JoinsetBody.relationships`                                 | `BTreeMap<EntityId, Relationship>` | Alphabetical by name (name-ordered projection)                                                     |
| `iter_all` / `iter_public` / `iter_simple` / `iter_complex` | iterators                        | Alphabetical by `(variant-tag, name)`; variants in fixed order: Dataset, Grainset, Unionset, Joinset |


**Relationship ordering change.** Relationships were previously a `Vec` carrying YAML author order, and first-match-wins (`16 §11`) plus compile-time `RelationshipId` allocation (`18 §2.1`) consumed that author order. With relationships now stored in an `EntityId`-keyed map projected in **name order**, both first-match-wins resolution and `RelationshipId` allocation proceed in name order. This is a deliberate consequence of the id-first rework; `16 §11` and `18 §2.1` are flagged for reconciliation (`STATUS.md`).

Identity determinism by profile (`§9.0.1`):

- **Strict deterministic** — all named entities carry authored ids; repeated parses of identical source produce byte-identical models *including* id values.
- **Convenience** — missing ids are auto-generated as UUIDv7; name-ordered output stays deterministic, but the embedded id values vary across fresh parses unless persisted and reused from manifest (`33 §4.3.1`).

`HashMap` is banned from the entire public surface. A CI check fails on any public `HashMap<_, _>` reachable from `SemanticModel`.

---

## 8. `${VAR}` Substitution

Before YAML decoding, `parse` rewrites every `${IDENT}` token in the input string to the value of the environment variable `IDENT` (via `std::env::var`). Unset variables raise `ParseErrorKind::UnsetEnvVar`.

```yaml
semantic_model:
  name: analytics-${ENVIRONMENT}
  datasets:
    - name: orders
      extras:
        storage:
          paths: ["s3://${DATA_BUCKET}/orders/*.parquet"]
```

Syntax: only `${IDENT}` is recognized. Bare `$VAR` is treated as literal text. Identifier grammar follows standard Unix env-var rules (ASCII letters, digits, underscores; no leading digit).

---

## 9. Stage APIs: `parse` and `validate`

The model crate hosts two accumulating stages per `30 §7.1`: `parse` (YAML → `SemanticModelBuilder`) and `validate` (structural-precondition pass over a parsed `SemanticModel`). Each owns its own kind enum (`ParseErrorKind`, `ValidateError`), each implements `Diagnose` per `30 §5.4`, and each uses the accumulating return shape per `30 §7.2`. The fluent loader (§9.6) composes the two behind one async-or-sync entry point.

### 9.0 Parse/build materialization pipeline (internal; shape-preserving)

`semstrait-model` separates authoring decode from canonical model materialization:

1. `${VAR}` substitution on raw YAML text (§8).
2. A single `serde` decode pass into parse-layer DTOs (`YamlRoot` and nested serde carriers), capturing the optional authored `id` on each named entity.
3. Missing-id resolution per `IdentityProfile` (`§9.0.1`): generate a UUIDv7 for every named entity that lacks an authored `id`.
4. Lower decoded entities **directly** into the canonical `SemanticModel` id-keyed maps (§2 / §3.7) — no intermediate accumulation store, no handle table.
5. `.build()` runs validate-stage checks, including the per-layer duplicate scans below.

There is no separate builder accumulation layout, `EntityHandle`, or `SemanticModelStorage`. The id-keyed maps **are** the storage: because the map key is the entity's own `id` (unique by construction — authored ids are checked, generated ids are fresh UUIDv7), every authored entity is retained without insert-time collision, and duplicate-by-name is detected by scanning each map at `.build()`:

- **Global layer** — data-kind name uniqueness (`SR-3`) across the four top-level data-kind maps; `id` global uniqueness (`SR-12`) across all named entities.
- **Per-carrier layer** — shared-pool name uniqueness within `dimensions` / `measures` / `metrics`.
- **Parent-scoped layer** — nested-child name uniqueness within each complex body's child maps (`26 §5`).

```rust
pub type EntityId = String; // canonical UUIDv7 text (lowercase, hyphenated)
```

Rules:

- `EntityId` is a boundary string. Canonical form is lowercase, hyphenated UUIDv7 text (`SR-11`).
- Named-entity IDs may be authored (`id`) or generated; generated IDs MUST still satisfy UUIDv7 canonical format.
- Dedup of *names* is by explicit scan (`SR-3` / `SR-E-3`); the map key is the `id`, so name collisions are reported, not silently overwritten.
- Public/nested structural envelopes (§3.3) and nesting rules (`26`) remain unchanged.
- Compile-owned IDs (`DataKindId`, `RelationshipId`, `SemanticsId`, …) remain out of `semstrait-model` scope; this section does not move those boundaries.

### 9.0.1 Identity profiles

`semstrait-model` defines two profiles for missing named-entity IDs:

```rust
#[non_exhaustive]
pub enum IdentityProfile {
    /// Missing `id` is generated as UUIDv7.
    ConvenienceGenerateMissing,
    /// Every named entity must provide `id`; missing is an error.
    StrictRequireProvided,
}
```

Default profile is `ConvenienceGenerateMissing` for `parse(&str)` and `SemanticModel::loader()`.

- **Strict deterministic profile** (`StrictRequireProvided`) requires authored `id` for every named entity.
- **Convenience profile** (`ConvenienceGenerateMissing`) generates UUIDv7 for missing named-entity `id`.
- Cross-run identity stability from source alone requires `StrictRequireProvided` or persisted id reuse from manifest (`33 §4.3.1`).

### 9.1 `parse` signature

```rust
use semstrait_common::diagnostic::{Diagnostic, Diagnostics};

/// YAML `&str` → `SemanticModelBuilder`. Pure and synchronous. Accumulating
/// stage per `30 §7.1`: every independent parse error is collected; warnings
/// found during the pass surface on the success arm.
///
/// Returns a builder rather than a finalised model so single-source,
/// code-built, and cross-source accumulations can share one materialise
/// path. Callers chain `.build()` (§9.7.6) to obtain the canonical
/// `SemanticModel` together with any validate-stage diagnostics.
pub fn parse(
    input: &str,
) -> Result<SemanticModelBuilder, Diagnostics<ParseErrorKind>>;

pub fn parse_with_profile(
    input: &str,
    profile: IdentityProfile,
) -> Result<SemanticModelBuilder, Diagnostics<ParseErrorKind>>;
```

`parse` is shorthand for `parse_with_profile(input, IdentityProfile::ConvenienceGenerateMissing)`.

Per D-10, dup-name diagnostics (SR-3 / SR-E-3) are emitted at `.build()` so single-source, multi-source, and code-built paths share one output contract. Implementations may track names incrementally during lowering into the id-keyed maps (§9.0), but they must not surface final `Duplicate*` diagnostics before `.build()`.

### 9.2 `ParseErrorKind` roster

```rust
use semstrait_common::diagnostic::{Diagnose, Severity};
use semstrait_common::Location;

#[non_exhaustive]
pub enum ParseErrorKind {
    // — YAML surface —
    YamlSyntax                    { message: String },
    UnsetEnvVar                   { var: String },
    UnknownField                  { field: String, parent: String },

    // — Structural rules (SR-*) —
    InvalidIdentifier             { raw: String, reason: String },           // SR-8
    InvalidEntityId               { raw: String, reason: String },           // SR-11

    // — Entity-level invariants (SR-E-*) — fired at parse —
    RelationshipMissingCardinality { relationship: String },                 // SR-E-4
    MeasureMissingAgg             { carrier: String, name: String },         // SR-E-9
    SemanticsMissingDataType      { carrier: String, name: String },         // SR-E-10
}

impl Diagnose for ParseErrorKind {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}
```

SR-1 / SR-2 / SR-4 (root-key, nested-data-kind, self-nesting) collapse into `UnknownField` — each is enforced by the absence of the offending field at the type level plus serde's `deny_unknown_fields`, so the surfaced kind is uniform.

`Duplicate*` variants migrated to `ValidateError` (§9.5) per D-10 so cross-source accumulations share the single-source enforcement path.

**Per-variant location.** The primary source span of an error lives on the wrapping `Diagnostic<ParseErrorKind>` (`30 §5.1`'s `location: Option<Location>` field), not on the variant. Variants carry payload data only.

**No `code()` accessor.** Identification of a parse error is by **variant identity** (`matches!(diag.kind, ParseErrorKind::InvalidIdentifier { .. })`), not by a string code. Renaming a variant is a MAJOR change per `30 §2.1`; adding a variant inside `#[non_exhaustive]` is MINOR per `30 §2.2`.

**Accumulation.** `parse` collects every recoverable error into the returned `Diagnostics<ParseErrorKind>` vector before deciding success vs failure. Catastrophic syntactic errors (the YAML parser cannot continue past byte N) short-circuit with a single `YamlSyntax` entry plus any warnings emitted before that point.

### 9.3 What `parse` does NOT do

- **No name resolution.** `SemanticsName` references inside `ExprSource` strings stay as opaque identifiers; resolution happens at `compile` (`11 §7`).
- **No type inference.** A derived Dimension / Measure / Metric with no `data_type:` stays `None` at parse; `compile` infers per `13 §6`.
- **No composition materialization.** `ComposedSemanticInterface` is a compile-time output.
- **No catalog I/O.** Catalog references are parsed as `CatalogRef` values and left unresolved; `catalogs.yaml` is a separate file loaded by the caller.
- **No multi-file loading.** `parse` takes one `&str`.

### 9.4 `validate` signature

```rust
/// Structural-precondition pass over a parsed `SemanticModel`. Pure and
/// synchronous. Accumulating stage per `30 §7.1`'s `validate` special
/// case: produces no value on success — the success arm carries only the
/// warnings vector (no tuple wrapper).
pub fn validate(
    model: &SemanticModel,
) -> Result<Diagnostics<ValidateError>, Diagnostics<ValidateError>>;
```

`validate` runs the SR-* rules whose enforcement column in §6 reads "Enforced at `validate`" (SR-10, SR-12) plus the entity-level `SR-E-`* invariants from `18 §11`. It does not transform the model; it is a pure precondition checker. Call it as a separate step before `compile` (`33`) when per-stage error routing matters; the fluent loader (§9.6) chains `parse` and `validate` for callers that don't need that granularity.

### 9.5 `ValidateError` roster

```rust
#[non_exhaustive]
pub enum ValidateError {
    // — Composition shape (SR-10) —
    ComplexDataKindInsufficientChildren { parent: String, child_count: usize },

    // — Empty model —
    EmptyModel,

    // — Cross-source / single-file dup detection (SR-3, D-10) —
    DuplicateDataKindName            { name: String, occurrences: Vec<Location> },
    DuplicateSharedSemanticsName     { carrier: String, name: String, occurrences: Vec<Location> },
    DuplicateEntityId                { id: String, occurrences: Vec<Location> },   // SR-12
    MissingEntityId                  { path: String },                              // strict profile only

    // — Entity-level invariants (SR-E-*) per `18 §11` —
    OrphanSharedSemantics            { carrier: String, name: String },
    TemporalGrainOnComplex           { data_kind: String },
    GrainsetChildMissingGrain        { grainset: String, child: String },
    /* … additional SR-E-* variants per `18 §11` … */

    // — Shadowing warning (`18 §1.5`) —
    SemanticsShadowRootPool          { carrier: String, name: String },

    // — IR-emitted construction-boundary failure (D.ii kind-nesting per `30 §7.4`).
    //   Surfaces `Tree::with_new_children` / `Rewriter::f_*` failures raised by
    //   `semstrait-ir`'s `Expr<L>` machinery during the `Block(Expr<L>)` serde
    //   round-trip in `ExprSource::Block(...)` (`14 §6.1`).
    Ir(ir::ValidateError),
}

impl Diagnose for ValidateError {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}

impl From<ir::ValidateError> for ValidateError {
    fn from(e: ir::ValidateError) -> Self { Self::Ir(e) }
}
```

The `Duplicate*` variants migrated from `ParseErrorKind` per D-10: dedup runs uniformly at `.build()` as a per-layer name scan over the id-keyed maps (§9.0), so single-source, code-built, and cross-source accumulations all surface the same diagnostic with every occurrence's `Location`.

`DuplicateEntityId` is emitted by the same `.build()` pass over resolved named-entity IDs. `MissingEntityId` is emitted only under `IdentityProfile::StrictRequireProvided`.

The `SemanticsShadowRootPool` variant carries `Severity::Warning` (the only warning-class variant in v1) and rides through on the `Ok` arm.

The `Ir(ir::ValidateError)` variant covers the narrow class of construction-boundary failures raised by `semstrait-ir`'s trait-machinery contract (`35 §16.1`): when `ExprSource::Block(Expr<L>)` deserializes a YAML tree, the serde shape *is* `Expr<L>` so any `Tree::with_new_children` violation surfaces as an `ir::ValidateError` value that the model crate wraps via D.ii nesting (`30 §7.4`).

The full `SR-E-*` variant roster is enumerated alongside the entity-level invariants in `18 §11` (the canonical home of SR-E-*); variants here mirror those rules 1-to-1 and land MINOR per `30 §2.2` as new SR-E-* numbers are appended.

### 9.6 Fluent loader: `SemanticModel::loader()`

`parse` / `parse_with_profile` and `validate` are the primitives — pure inputs, pure outputs, with explicit profile control only for missing-id policy. Most callers want **parse + validate** together, returning a `SemanticModel` ready for `compile` consumption. The fluent loader is the ergonomic entry that fuses the two **plus** filesystem reads.

The loader is parametrized by a **filesystem strategy** (not a phase marker). The type parameter `F: SourceFs` selects how the loader reads source bytes — `LocalFs` for production, `InMemoryFs` for tests. This matches Rust idiom for I/O-strategy parametrization (see `figment`, `config-rs`, `tower`). Compile-time enforcement of "did the caller attach a source?" is dropped in favour of a runtime `ModelBuildErrorKind::NoSource` diagnostic; the cost (one extra runtime check) buys real testability via `InMemoryFs`.

```rust
/// Filesystem-strategy trait. Implementors decide how a logical path
/// resolves to bytes. Sync only — async I/O is not in v1 scope per §10.4.
pub trait SourceFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, std::io::Error>;
}

/// Production strategy — delegates to `std::fs::read`.
pub struct LocalFs;
impl SourceFs for LocalFs { /* std::fs::read */ }

/// Test strategy — `HashMap<path, bytes>` lookup, miss => `ErrorKind::NotFound`.
pub struct InMemoryFs { /* ... */ }
impl InMemoryFs {
    pub fn new() -> Self;
    pub fn insert(&mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>);
}
impl SourceFs for InMemoryFs { /* HashMap lookup */ }

/// Loader for `SemanticModel`. Configured via fluent setters, terminated by `build`.
pub struct SemanticModelLoader<F: SourceFs = LocalFs> { /* fields pub(crate) */ }

impl SemanticModelLoader<LocalFs> {
    /// Default-strategy entry. Equivalent to `SemanticModel::loader()`.
    pub fn new() -> Self;
}

impl<F: SourceFs> SemanticModelLoader<F> {
    /// Swap the filesystem strategy. Returns a loader with the new `F2`.
    pub fn with_fs<F2: SourceFs>(self, fs: F2) -> SemanticModelLoader<F2>;

    /// Attach an in-memory YAML payload tagged with a logical `SourceId`.
    /// Multiple calls accumulate; sources are merged in append order.
    pub fn from_yaml_str(self, yaml: impl Into<String>, source: SourceId) -> Self;

    /// Resolve `path` via `self.fs.read(path)` at `build()` time.
    /// `SourceId` defaults to `path` (string form).
    pub fn from_yaml_file(self, path: impl Into<String>) -> Self;

    /// Attach a pre-parsed catalogs config. Mutually exclusive with
    /// `from_catalogs_yaml_*` (calls overwrite, last-write-wins).
    pub fn with_catalogs(self, c: CatalogsConfig) -> Self;
    pub fn from_catalogs_yaml_str(self, yaml: impl Into<String>, source: SourceId) -> Self;
    pub fn from_catalogs_yaml_file(self, path: impl Into<String>) -> Self;

    /// Skip the validate pass (default: validate runs). The returned
    /// model is parsed-only; SR-* / SR-E-* are not enforced. Intended
    /// for inspector / round-trip tooling.
    pub fn skip_validate(self) -> Self;

    /// Configure missing-id policy for named entities (§9.0.1).
    /// Default is `IdentityProfile::ConvenienceGenerateMissing`.
    pub fn with_identity_profile(self, profile: IdentityProfile) -> Self;

    /// Run the configured pipeline.
    pub fn build(self) -> Result<
        (SemanticModel, Diagnostics<ModelBuildErrorKind>),
        Diagnostics<ModelBuildErrorKind>,
    >;
}

impl SemanticModel {
    /// Convenience: `SemanticModelLoader::<LocalFs>::new()`.
    pub fn loader() -> SemanticModelLoader<LocalFs> { SemanticModelLoader::new() }
}

/// Fused per-stage kind for the loader pipeline. Implements `Diagnose`
/// by delegating to the wrapped stage kind. Per `30 §5.6` the model crate
/// owns this fused sum because the loader composes stages whose kinds
/// live in this same crate (`ParseErrorKind`, `ValidateError`,
/// `CatalogsParseErrorKind`) plus loader-internal kinds.
#[non_exhaustive]
pub enum ModelBuildErrorKind {
    /// No source attached — `build()` was called on an empty loader.
    NoSource,
    /// `self.fs.read(path)` failed.
    SourceIo { path: String, error: std::io::ErrorKind },
    Parse(ParseErrorKind),
    CatalogsParse(CatalogsParseErrorKind),
    Validate(ValidateError),
    /// Per-field builder-internal error (e.g. invalid newtype payload).
    BuilderField { struct_name: &'static str, field: &'static str, message: String },
}

impl From<ParseErrorKind>             for ModelBuildErrorKind { /* … */ }
impl From<ValidateError>          for ModelBuildErrorKind { /* … */ }
impl From<CatalogsParseErrorKind>     for ModelBuildErrorKind { /* … */ }

impl Diagnose for ModelBuildErrorKind { /* delegates to wrapped variant */ }
```

**Stages composed by `build`** (in order, fail-fast across stages, accumulating within each stage):

1. **Read** — for each `from_yaml_file`/`from_catalogs_yaml_file` source, call `self.fs.read(path)`. `Err` ⇒ `ModelBuildErrorKind::SourceIo`.
2. **Parse model** — `parse_with_profile(&yaml, self.identity_profile)` per attached model source(s). `Err` ⇒ `ModelBuildErrorKind::Parse(_)`.
3. **Parse catalogs** — when present, `parse_catalogs(&yaml, source)`. `Err` ⇒ `ModelBuildErrorKind::CatalogsParse(_)`.
4. **Identity finalize** — confirm every named entity carries an `id` (authored or generated) and enforce profile-specific constraints via the per-layer scans (`DuplicateEntityId`, strict `MissingEntityId`).
5. **Validate** — when `skip_validate()` was NOT called, run the structural-precondition pass (§9.4–§9.5). `Err` ⇒ `ModelBuildErrorKind::Validate(_)`.

Within a stage, every diagnostic the stage produces is collected (parse / validate are accumulating per `30 §7.1`); across stages, the loader halts at the first stage whose Err arm fires and lifts that stage's accumulated set into `ModelBuildErrorKind`. Warnings from earlier stages that completed successfully ride through on the failing stage's Err vector — never silently dropped per `30 §7.3`.

**Usage.**

```rust
// Production
let (model, diags) = SemanticModel::loader()
    .from_yaml_file("model.yaml")
    .from_catalogs_yaml_file("catalogs.yaml")
    .build()?;

// Testing
let mut fs = InMemoryFs::new();
fs.insert("model.yaml", REFERENCE_YAML);
let (model, _) = SemanticModelLoader::<InMemoryFs>::new()
    .with_fs(fs)
    .from_yaml_file("model.yaml")
    .build()?;
```

**FS-strategy contract.** Source attachment is runtime-checked; empty loader build returns `ModelBuildErrorKind::NoSource`. `SourceFs` is the filesystem abstraction, with `LocalFs` and `InMemoryFs` as built-ins.

**Stage API contract.** `parse` / `parse_with_profile` and `validate` remain primary stage APIs. The fluent loader is an additive wrapper that composes them without changing stage semantics.

**Composition with `semstrait-api`.** `SemStrait::compile_from_yaml` (`38 §3.3`) is the parallel API at the orchestration layer (it adds compile on top). The two lanes don't compete: in-process callers reach for `SemanticModel::loader()` to obtain a `SemanticModel` they can pass to a separate compile call (e.g. for caching mid-pipeline); end-to-end callers reach for `SemStrait::compile_from_yaml` to skip the intermediate handle. The fused `SemStraitErrorKind` (`30 §5.6`) at `semstrait-api` is parallel to `ModelBuildErrorKind` here — same cross-stage aggregation pattern, broader scope.

#### 9.6.2 Stability

- `SemanticModelLoader<F>`, the `SourceFs` trait, `LocalFs`, `InMemoryFs`, the `SemanticModel::loader()` entry, the `from_yaml_*` / `from_catalogs_yaml_*` / `with_fs` / `with_catalogs` / `with_identity_profile` / `skip_validate` setters, and `build` are **Stable in v1**.
- `ModelBuildErrorKind` is `#[non_exhaustive]`; new stage variants land as MINOR per `30 §2.2`. Removing or renaming a variant is MAJOR per `30 §2.1`.
- The relationship "loader composes `parse_with_profile + parse_catalogs + validate`" is a public contract — `build` MUST NOT alter parse / validate semantics; any divergence is a v1 bug.
- `LocalFs` is the documented default and is intended to remain so. Adding new built-in `SourceFs` implementations (e.g. an `S3Fs` behind a feature flag) is MINOR.

### 9.7 Builder API

`semstrait-model` exposes a per-struct fluent builder for every public type so callers can construct a `SemanticModel` entirely from code (no YAML in the loop). The builder API is a faithful structural projection of the spec — every method maps 1:1 to a Rust field; every adapter helper maps 1:1 to a structurally-defined variant body. Builders are generated via the `bon` derive macro (`m11-ecosystem` ratified).

#### 9.7.1 Two-surface principle

The builder API exposes two surfaces over the spec-defined struct tree:

1. **Primary structural surface** — generated by `bon` from each public struct. Method names equal field names (`.dim_type(...)`, `.data_type(...)`, `.agg(...)`, `.semantic_mapping(...)`); enum-variant 1:1 helpers (`DimensionType::temporal(grains)`, `TemporalShape::events(...)`) are co-located on the variant type itself per `§9.7.3`. The primary surface is the canonical construction substrate — round-trip helpers, schema generators, and code-gen tools target the primary surface.

2. **Ergonomic facade surface** — additive sugar layered on top of the primary surface (see `§9.7.8`). Facade methods may rename or aggregate primary calls for authoring convenience but never bypass the primitive types — every facade call delegates to one or more primary calls.

The structural source of truth is the type tree (`§2`–`§5` and the per-entity types in `18 §*`), not the builder. The two surfaces are aligned by a hard rule: every facade method must delegate to primary methods on the same builder; no facade method invents a non-spec field, alters round-trip representation, or shadows the primary surface for type queries.

Three rules govern what is allowed on each surface:

- **R1 — primary surface field-name parity (Stable in v1).** On the primary surface, every builder method name equals the Rust field name on the underlying struct. No abbreviations, no synonyms. If a struct field renames in the spec, the primary method renames with it (MAJOR per `30 §2.1`).
- **R2 — variant-body 1:1 constructors (Stable in v1).** A type-level helper that constructs an enum variant takes exactly that variant body's fields, in declaration order, with no inferred or synthesized values.
- **R3 — facade surface delegates only (Stable in v1).** The ergonomic facade may rename and aggregate. It MAY NOT introduce non-spec fields, mutate round-trip representation, or bypass primary methods. Round-trip uses the primary surface only.

Violations of R1, R2, or R3 are spec bugs, not implementation bugs. The structural-fidelity audit is planned in CI as `tests/builder_structural_fidelity.rs` and verifies that every facade method has a delegate-only implementation.

#### 9.7.2 Public-vs-Nested envelope split

Per `[26 §3](../data-kinds/26_nesting_matrix.md)`, the Public form carries `description`, `ai_context`, `semantic_interface`; the Nested form does NOT. The builder API enforces this at the **type level** by structural absence — `NestedDataset`/`NestedGrainset`/`NestedUnionset`/`NestedJoinset` simply do not have those fields, so their `bon`-generated builders do not have those methods. No bespoke typestate machinery is involved.

```rust
use bon::Builder;

#[derive(Debug, Clone, Builder)]
pub struct Dataset {
    #[builder(start_fn)] pub name: String,                // body.base.name
    pub extras: LeafExtras,                                // body.base.extras
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub semantic_interface: SemanticInterface,
}

#[derive(Debug, Clone, Builder)]
pub struct NestedDataset {
    #[builder(start_fn)] pub name: String,
    pub extras: LeafExtras,
    // No description / ai_context / semantic_interface — type-level absence.
}
```

#### 9.7.3 Variant-body 1:1 constructors

Authors can either name a variant + body pair fully, or use a 1:1 constructor on the parent type. Constructors map directly to spec-defined variant bodies — they take exactly the body's fields, in declaration order, no inventions.

```rust
impl DimensionType {
    /// 1:1 with `Temporal(TemporalDimensionBody { grains })` per `18 §4.1`.
    pub fn temporal(grains: impl Into<Vec<Grain>>) -> Self;
    /// 1:1 with `Categorical` (no body).
    pub fn categorical() -> Self;
    /// 1:1 with `Binary` (no body).
    pub fn binary() -> Self;
    /// 1:1 with `Geo` (no body).
    pub fn geo() -> Self;
    /// 1:1 with `Bucketed(BucketedDimensionBody { buckets })`.
    pub fn bucketed(buckets: impl Into<Vec<BucketSpec>>) -> Self;
    /// 1:1 with `Metadata(MetadataDimensionBody { source })`.
    pub fn metadata(source: MetadataSource) -> Self;
}

impl TemporalShape {
    /// 1:1 with `TemporalShape { kind: Timeseries(TimeseriesBody { occurred_at }), grain }` per `18 §3.1`.
    pub fn timeseries(occurred_at: impl Into<SemanticsName>, grain: impl Into<Option<Grain>>) -> Self;
    pub fn events(event_time: impl Into<SemanticsName>, grain: impl Into<Option<Grain>>) -> Self;
    pub fn snapshot(snapshotted_at: impl Into<SemanticsName>, grain: impl Into<Option<Grain>>) -> Self;
    pub fn scd(
        scd_type: ScdType,
        valid_from: impl Into<SemanticsName>,
        valid_to: impl Into<SemanticsName>,
        grain: impl Into<Option<Grain>>,
    ) -> Self;
}

impl JoinKeyExprPair {
    /// 1:1 with the bare-Semantic-field case where both sides are
    /// `ExprSource::Inline(name)`. For non-bare cases, construct the
    /// struct with explicit `from`/`to` `ExprSource`s.
    pub fn fields(from: impl Into<String>, to: impl Into<String>) -> Self;
}

impl DimensionEntry {
    /// 1:1 with `Inline(Dimension)` per `18 §1.2`.
    pub fn inline(d: Dimension) -> Self;
    /// 1:1 with `Ref(DimensionRef { name, expr: None })`.
    pub fn r#ref(name: impl Into<SemanticsName>) -> Self;
    /// 1:1 with `Ref(DimensionRef { name, expr: Some(expr) })`.
    pub fn ref_with_expr(name: impl Into<SemanticsName>, expr: impl Into<ExprSource>) -> Self;
}
// MeasureEntry, MetricEntry: analogous.
```

#### 9.7.4 SemanticMapping authoring

`SemanticMapping` is `BTreeMap<SemanticsName, SemanticMappingValue>` per `[18 §10](../foundations/18_entities.md)`. Its builder offers per-variant inserters keyed by semantic name, each 1:1 with a `SemanticMappingValue` variant:

```rust
impl SemanticMapping {
    pub fn builder() -> SemanticMappingBuilder;
}

impl SemanticMappingBuilder {
    /// 1:1 with `Column(String)`.
    pub fn column(self, semantic: impl Into<SemanticsName>, column: impl Into<String>) -> Self;
    /// 1:1 with `Literal(LiteralValue)`.
    pub fn literal(self, semantic: impl Into<SemanticsName>, value: LiteralValue) -> Self;
    /// 1:1 with `Expr(PhysicalExpr)`.
    pub fn expr(self, semantic: impl Into<SemanticsName>, expr: PhysicalExpr) -> Self;
    // No `metadata(...)` method — `Metadata(MetadataDimensionRecipe)` is
    // compile-synthesized only per SR-10 / `32 §10`.

    /// Adds an explicit semantic→source mapping entry without naming a
    /// specific `SemanticMappingValue` variant up front. Transitions the
    /// builder to `Explicit` form on first call. Last write wins for a
    /// given `name` within one builder. Useful when the variant is
    /// chosen dynamically (loader / round-trip) rather than statically
    /// per-call site.
    pub fn with_semantic(
        self,
        name: impl Into<SemanticsName>,
        value: SemanticMappingValue,
    ) -> Self;

    pub fn build(self) -> SemanticMapping;
}
```

Example mixing variant-specific inserters with the generic `with_semantic` opening:

```rust
let mapping = SemanticMapping::builder()
    .column("revenue", "amount_cents")
    .with_semantic("currency", SemanticMappingValue::Literal(LiteralValue::String("USD".into())))
    .build();
```

#### 9.7.5 Bespoke validation in `.build()`

Most field-level required-vs-optional is enforced by `bon` at compile time (missing required fields ⇒ compile error). A small set of *cross-field* invariants are enforced inside `.build()` because they are derived rather than raw-required:

- `Relationship::builder().build()` — when `cardinality ∈ {OneToOne, ManyToMany}`, both `optional` and `cross_filter` MUST be authored (SR-E-13). When `cardinality == ManyToMany`, `cross_filter` MUST NOT be `Left | Right` (SR-E-14). Violations ⇒ `ModelBuildErrorKind::Validate(...)`.
- `SemanticMappingValue::Metadata` rejection — the variant exists in the enum (compile-synthesized) but the builder has no `.metadata(...)` method (R3 above).
- `SemanticModel::builder().build()` — runs the full `validate` pipeline on the constructed model, so all SR-* / SR-E-* rules apply uniformly to YAML-loaded and code-built models.

#### 9.7.6 Resulting usage — spec-faithful end-to-end

```rust
use semstrait_common::{DataType, Grain};
use semstrait_model::{ExprSource, *};

let order_ts = Dimension::builder("order_ts")
    .data_type(DataType::Timestamp)
    .dim_type(DimensionType::temporal([Grain::Minute, Grain::Hour, Grain::Day]))
    .build();

let revenue = Measure::builder("revenue")
    .data_type(DataType::Decimal { precision: 18, scale: 2 })
    .agg(AggregationType::Sum)
    .expr(ExprSource::Inline("amount_cents * 0.01".into()))
    .additivity(AdditivityType::Full)
    .build();

let extras = LeafExtras::builder()
    .catalog(CatalogRef::new("polaris_prod"))
    .storage(StorageConfig::builder()
        .format(StorageFormat::Parquet)
        .paths(["s3://bucket/orders/"])
        .build())
    .semantic_mapping(SemanticMapping::builder()
        .column("revenue", "net_revenue_cents")
        .literal("currency", LiteralValue::String("USD".into()))
        .build())
    .temporal(TemporalShape::events("order_ts", Some(Grain::Minute)))
    .build();

let interface = SemanticInterface::builder()
    .dimension(DimensionEntry::inline(order_ts))
    .measure(MeasureEntry::r#ref("revenue"))
    .keys(Keys::builder()
        .primary(KeyDecl::builder().fields(["order_id"]).build())
        .build())
    .build();

let orders = Dataset::builder("orders")
    .extras(extras)
    .description("Order-line fact dataset.")
    .semantic_interface(interface)
    .build();

let orders_to_customers = Relationship::builder()
    .name("orders_to_customers")
    .from("orders").to("customers")
    .keys([JoinKeyExprPair::fields("customer_id", "id")])
    .cardinality(Cardinality::ManyToOne)
    .build()?;

let model = SemanticModel::builder()
    .name("analytics-v1")
    .description("Primary analytics model for the order pipeline.")
    .dataset(orders)
    .measure(revenue)
    .relationship(orders_to_customers)
    .build()?;
```

#### 9.7.6.1 Same model authored via the ergonomic facade

Both forms produce identical `SemanticModel` values. The facade (§9.7.8) is an additive wrapper over the primary surface above; round-trip and code-gen tools continue to use the primary surface.

```rust
use semstrait_common::{DataType, Grain};
use semstrait_model::*;

let order_ts = Dimension::builder("order_ts")
    .data_type(DataType::Timestamp { precision: 6 })
    .temporal([Grain::Minute, Grain::Hour, Grain::Day])
    .build();

let revenue = Measure::builder("revenue")
    .data_type(DataType::Decimal { precision: 18, scale: 2 })
    .sum()
    .full()
    .build();

let orders = Dataset::builder("orders")
    .catalog("polaris_prod")
    .format(StorageFormat::Parquet)
    .path("s3://bucket/orders/")
    .semantic_mapping(SemanticMapping::builder().column("revenue", "amount_cents").build())
    .temporal(TemporalShape::events("order_ts", Some(Grain::Minute)))
    .description("Order-line fact dataset.")
    .dimension(DimensionEntry::r#ref("order_ts"))
    .measure(MeasureEntry::r#ref("revenue"))
    .primary_key(KeyDecl::builder().fields(vec!["order_id".into()]).build())
    .build();

let orders_to_customers = Relationship::builder()
    .name("orders_to_customers")
    .from("orders").to("customers")
    .field("customer_id", "id")
    .many_to_one()
    .build()?;

let (model, _) = SemanticModel::builder()
    .name("analytics-v1")
    .description("Primary analytics model for the order pipeline.")
    .dataset(orders)
    .dimension(order_ts)
    .measure(revenue)
    .relationship(orders_to_customers)
    .build()?;
```

#### 9.7.7 Stability

- The set of public builder types (one per public struct in §2 / §3 / §4 / §5 / `18 §*`) is **Stable in v1**.
- The structural-fidelity rules R1 / R2 / R3 (§9.7.1) govern the primary structural surface (R1 = primary field-name parity; R2 = variant-body 1:1 constructors; R3 = facade delegates only). All three are Stable in v1.
- Variant-body 1:1 constructors (§9.7.3) are **Stable in v1**. Adding a new constructor is MINOR; removing or renaming one is MAJOR.
- The ergonomic facade surface (§9.7.8) is **Stable in v1**. Adding a facade method is MINOR per `30 §2.2`; removing or renaming one is MAJOR per `30 §2.1`.
- `bon` is an implementation detail. Switching to a different builder generator (or hand-rolled builders) is internal — the public method-name and method-signature surface (both primary and facade) stays per R1 / R2 / R3 / §9.7.8.
- The `state_mod` visibility rule on facade-supporting builders (§9.7.8.1) is **Stable in v1**. Renaming a publicly-exposed state module is MAJOR.

#### 9.7.8 Ergonomic facade (additive)

The ergonomic facade adds authoring-convenience methods on top of the primary structural builders (`§9.7.2`–`§9.7.5`). It is in-scope for v1.

**Goals:** reduce verbosity at common code-built model authoring sites; keep the YAML↔Rust round-trip primary surface untouched; stay strictly delegate-only.

**Non-goals:** replace the primary surface; introduce parallel types; introduce alternative semantics for any spec field.

##### 9.7.8.1 Surface placement

Facade methods are inherent methods on the existing `*Builder<S>` typestate types, defined in `impl<S: state_mod::State> XBuilder<S>` blocks adjacent to the primary `bon`-derived implementation. They share the autocomplete surface with the primary methods. The primary methods remain canonical; an author may always reach for `.dim_type(...)` in preference to `.temporal(...)`.

Facade-supporting builders that expose typestate-transitioning facade methods (whose return type names a `state_mod::Set*<S>` marker, e.g. `dimension_builder::SetDimType<S>`) MUST configure `bon`'s `state_mod` with `vis = "pub"` so external test crates and downstream consumers can name those marker types. Builders whose facade methods all return `Self` (no typestate transition) MAY use `vis = "pub(crate)"`. Publicly-exposed state modules are part of the public crate surface from that point onward; renaming a publicly-exposed state module is MAJOR per `30 §2.1`.

##### 9.7.8.2 Delegation rule

Every facade method's body manipulates only the fields exposed through the primary surface (whether via a bon-generated setter or a hand-written `Self`-returning setter on a `#[builder(field)]` slot). A facade method MUST NOT introduce a hidden writable field that the primary surface does not also reach; if it must, the primary surface is missing a method (file a spec bug).

##### 9.7.8.3 Conflict semantics

Within a single builder chain, a field may be set by both a primary call and one or more facade calls that target a sub-field of the same struct. Two rules resolve conflict:

- **Field-level last-write-wins.** When the same `Option<T>` or scalar field is set more than once, the last call wins. Matches `bon`'s default setter semantics.
- **Sub-struct read-modify-write (cross-struct flatteners only).** When a facade method targets a *sub-field* of a struct (e.g. `.catalog(...)` targets `extras.catalog: Option<CatalogRef>` on `Dataset::builder()`), the facade reads the current value of the parent struct (or its `Default::default()` if unset), mutates the targeted sub-field, and writes the parent struct back. A `.extras(my_extras)` call followed by `.catalog("polaris")` preserves every other field of `my_extras` while setting `catalog`. Symmetrically for `.semantic_interface(my_iface)` followed by `.dimension(entry)`.

RMW behaviour is enabled by switching the carrier field from a bon-generated setter slot (`Option<T>`) to a `#[builder(field)] T` slot, with a custom inherent setter for backward compatibility. Builders with sub-struct flatteners (`Dataset`, `NestedDataset`, `Grainset`, `NestedGrainset`, `Unionset`, `NestedUnionset`, `Joinset`, `NestedJoinset`, `SemanticInterface`) follow this pattern.

These rules apply uniformly to every facade method. No facade method emits a runtime warning or error on conflict.

##### 9.7.8.4 Stability

- Adding a facade method is MINOR per `30 §2.2`.
- Removing or renaming a facade method is MAJOR per `30 §2.1`.
- The conflict-resolution rules in `§9.7.8.3` are **Stable in v1**.
- The `state_mod` visibility rule (§9.7.8.1) is **Stable in v1**.

##### 9.7.8.5 v1 facade method roster

The following facade additions are landed in v1:

**Per-entity (`Dimension` / `Measure` / `Metric` / `Relationship`):**
- `Dimension::builder()` — `.temporal(grains)`, `.categorical()`, `.binary()`, `.geo()`, `.bucketed(buckets)`, `.metadata(source)` (each delegates to `.dim_type(DimensionType::*(...))`)
- `Measure::builder()` — agg shortcuts `.sum()` / `.avg()` / `.count()` / `.count_distinct()` / `.min()` / `.max()` / `.median()` / `.std_dev()` / `.variance()`; additivity shortcuts `.full()` / `.semi(s)` / `.non()`; per-item `.filter(f)`
- `Metric::builder()` — same agg / additivity / filter shortcuts as Measure (note: `Metric.agg` is `Option<AggregationType>`)
- `Relationship::builder()` — `.field(from, to)` per-key shortcut; cardinality shortcuts `.one_to_one()` / `.one_to_many()` / `.many_to_one()` / `.many_to_many()`

**Container builders (`SemanticInterface` / `SemanticMapping`):**
- `SemanticInterface::builder()` — per-item inserters `.dimension(e)` / `.measure(e)` / `.metric(e)` / `.filter(f)`; per-key shortcuts `.primary_key(k)` / `.unique_key(k)` / `.foreign_key(k)`
- `SemanticMapping::builder()` — bulk `.entries(items)`

**DataKind builders (with cross-struct flatteners per §9.7.8.3):**
- `Dataset::builder(name)` and `NestedDataset::builder(name)` — `LeafExtras` flatteners `.catalog(s)` / `.storage(c)` / `.format(f)` / `.path(p)` / `.paths(it)` / `.table(t)` / `.tables(it)` / `.partition_def(p)` / `.semantic_mapping(m)` / `.temporal(s)`. Public `Dataset` additionally exposes `SemanticInterface` flatteners `.dimension(e)` / `.dimensions(it)` / `.measure(e)` / `.measures(it)` / `.metric(e)` / `.metrics(it)` / `.filter(f)` / `.filters(it)` / `.keys(k)` / `.primary_key(k)` / `.unique_key(k)` / `.foreign_key(k)`.
- `Grainset::builder(name)` / `NestedGrainset` / `Unionset::builder(name)` / `NestedUnionset` / `Joinset::builder(name)` / `NestedJoinset` — `ComplexExtras` flattener `.temporal(s)`. Public forms additionally expose the same `SemanticInterface` flatteners as `Dataset`. `Unionset` and `NestedUnionset` additionally expose `.union_all()` / `.union_unique()` (mode shortcuts).

Adding a facade method is MINOR per `§9.7.8.4`. The roster grows append-only.

#### 9.7.9 Entity `id` in builders

`id` is an ordinary payload field, so each entity builder exposes an optional `.id(EntityId)` setter. When unset, `.build()` generates a UUIDv7 under the active `IdentityProfile` (`§9.0.1`) — the same generation path used by `parse`.

- Scope: every named-entity builder (`Dataset` / `Grainset` / `Unionset` / `Joinset`, `Dimension` / `Measure` / `Metric`, `Relationship`, `Key` / `Filter`).
- Dedup: name uniqueness is the per-layer scan over the id-keyed maps (`Duplicate*` validate variants); `DuplicateEntityId` (`SR-12`) is the additive id-uniqueness check.
- Ordering: `id` never participates in canonical ordering; `I4` output is name-ordered per §7 regardless of id values.
- Insertion: the entity's `id` is the map key in the collection it joins; there is no separate handle.

---

## 10. Crate Boundaries

`semstrait-model` is the thinnest authoring-surface crate. It sits two levels above `semstrait-common` in the workspace DAG (I7), via `semstrait-ir` per the second-cascade landing (`STATUS.md` item Q):

```
semstrait-common      (leaf: DataType / Schema / Diagnostic<K> / Diagnose / constraints / io)
    ↑
semstrait-ir        (Expr<L> + Tree/Visitor/Rewriter/ExprLeaf + leaves + accessors
                     + BinaryOpKind/…/Literal + ColumnRef/SemanticsName
                     + CanonicalFn/FunctionRegistry + ValidateError/CompileError + PlanNode)
    ↑
semstrait-model     (parse + validate + SemanticModel + ExprSource + ParseErrorKind + ValidateError)
    ↑
semstrait-manifest, semstrait-planner, semstrait-adapter, …
```

Dependencies: `semstrait-common`, `semstrait-ir`, `serde`, `serde_yaml`, `tracing` (`30 §6.2`). No other `semstrait-*` crate. No `async`, no `arrow`, no engine-specific deps. The `semstrait-ir` dep is what allows `ExprSource::Block(...)` to carry `Expr<L>` directly via serde — there is no parallel `ExprBlock` AST owned by this crate (`14 §6.1`).

### 10.1 No direct I/O in `parse`

`parse` takes a `&str` and is synchronous. It performs no file opens and no network calls. `std::env::var` is the single allowlisted syscall (used for `${VAR}` substitution per §8). A CI check enumerates direct `std::fs`, `std::net`, `std::process` imports in `semstrait-model` source and fails on any match.

The optional `::io` submodule (§10.4) adds async load / dump wrappers. It does not relax this check: `model::io` never imports `std::fs` / `std::net` / `std::process` directly either — all transport goes through `semstrait-common::io`'s `Source` / `Sink` traits (`31b §3` / `§4`). The CI check applies uniformly to the whole `semstrait-model` source tree.

### 10.2 No resolution

Name resolution, reference expansion, and cross-kind path resolution are `compile`'s responsibility (`33`, `11 §7`). `parse` records identifiers verbatim.
Named-entity `EntityId` values are identity metadata propagated to manifest stability maps (`33 §4.3.1`); they are storage keys, not semantic-name resolution keys. Cross-references inside the model (`ref:`, relationship `from`/`to`) are by name and resolved at compile.

### 10.3 No planning

`Request`, `PlanNode`, `SemanticPlan`, and `SemanticManifest` types never appear in `semstrait-model`.

### 10.4 Model-Level I/O Surface (`semstrait-model::io`)

The optional `::io` submodule provides async wrappers that combine `semstrait-common::io` transport (`31b`) with `parse` / `serialize` for both the `semantic_model:` file and the sibling `catalogs.yaml` file (`32b`). It is feature-gated behind `io` (see §10.5) so callers that only want the sync `parse(&str)` surface don't pull in the async runtime or transport dependencies.

#### 10.4.1 Entry points

```rust
use semstrait_common::io::{Source, Sink};
use semstrait_common::diagnostic::{Diagnostic, Diagnostics};
use semstrait_model::{SemanticModel, CatalogsConfig};

pub mod io {
    /// Read the payload as UTF-8 via `src.read::<String>()`, then `parse` it
    /// into a `SemanticModel`. YAML-only — no format argument, no sniffing.
    pub async fn load_model<S: Source + ?Sized>(
        src: &S,
    ) -> Result<
        (SemanticModel, Diagnostics<ModelLoadErrorKind>),
        Diagnostics<ModelLoadErrorKind>,
    >;

    /// Canonically render `m` per `DumpMode`, then write to `sink`.
    pub async fn dump_model<S: Sink + ?Sized>(
        m: &SemanticModel,
        sink: &S,
        mode: DumpMode,
    ) -> Result<
        Diagnostics<ModelDumpErrorKind>,
        (Diagnostic<ModelDumpErrorKind>, Diagnostics<ModelDumpErrorKind>),
    >;

    /// Catalogs counterpart — reads the payload via `src.read::<String>()`,
    /// then `parse_catalogs` it into a `CatalogsConfig` (`32b §5`).
    pub async fn load_catalogs<S: Source + ?Sized>(
        src: &S,
    ) -> Result<
        (CatalogsConfig, Diagnostics<CatalogsLoadErrorKind>),
        Diagnostics<CatalogsLoadErrorKind>,
    >;

    pub async fn dump_catalogs<S: Sink + ?Sized>(
        c: &CatalogsConfig,
        sink: &S,
        mode: DumpMode,
    ) -> Result<
        Diagnostics<CatalogsDumpErrorKind>,
        (Diagnostic<CatalogsDumpErrorKind>, Diagnostics<CatalogsDumpErrorKind>),
    >;

    #[non_exhaustive]
    pub enum DumpMode {
        /// Canonical render: alphabetised maps (I4), normalised whitespace,
        /// no comment preservation. Round-trip stable against repeated dumps.
        Canonical,
    }
}
```

`load_model` wraps `src.read::<String>().await.and_then(|text| parse(&text))` — UTF-8 validation (`FromIoBytes for String`, `31b §5`) surfaces as `Diagnostic<ModelLoadErrorKind>` with `Io(IoErrorKind::Malformed)` when bytes are non-UTF-8. `dump_model` wraps `sink.write(canonical_render(m)).await` where `canonical_render` yields a `String` (`IntoIoBytes for String`). Per `30 §7.1`, `load_*` are accumulating helpers (wrapped parse stages are accumulating); `dump_*` are fail-fast.

#### 10.4.2 Error roster

```rust
use semstrait_common::diagnostic::Diagnose;
use semstrait_common::io::IoErrorKind;

#[non_exhaustive]
pub enum ModelLoadErrorKind {
    Io(IoErrorKind),
    Parse(ParseErrorKind),
}

#[non_exhaustive]
pub enum ModelDumpErrorKind {
    Io(IoErrorKind),
    NotRoundTrippable { path: String, reason: String },
}

#[non_exhaustive]
pub enum CatalogsLoadErrorKind {
    Io(IoErrorKind),
    Parse(CatalogsParseErrorKind),
}

#[non_exhaustive]
pub enum CatalogsDumpErrorKind {
    Io(IoErrorKind),
    NotRoundTrippable { alias: String, reason: String },
}

impl Diagnose for ModelLoadErrorKind     { /* delegates */ }
impl Diagnose for ModelDumpErrorKind     { /* delegates + round-trip message */ }
impl Diagnose for CatalogsLoadErrorKind  { /* delegates */ }
impl Diagnose for CatalogsDumpErrorKind  { /* delegates + round-trip message */ }

impl From<IoErrorKind>          for ModelLoadErrorKind     { /* … */ }
impl From<ParseErrorKind>       for ModelLoadErrorKind     { /* … */ }
impl From<IoErrorKind>          for ModelDumpErrorKind     { /* … */ }
impl From<IoErrorKind>          for CatalogsLoadErrorKind  { /* … */ }
impl From<CatalogsParseErrorKind> for CatalogsLoadErrorKind { /* … */ }
impl From<IoErrorKind>          for CatalogsDumpErrorKind  { /* … */ }
```

Each fused kind composes with the transport or parser kind underneath and carries no new error state of its own beyond the round-trip guard. Identification is by variant identity per `30 §5`; there is no string-code accessor.

Both `IoErrorKind` and every domain wrapper enum are `#[non_exhaustive]` per `31b §7` and `30 §4.1`. Adding an `IoErrorKind` variant (e.g. `RateLimited`) therefore propagates through `ModelLoadErrorKind::Io(IoErrorKind)` as a MINOR change — downstream `match` arms must already carry a `_ => ...` catch-all per `30 §4.4`.

#### 10.4.3 `NotRoundTrippable` guard

`dump_model` refuses to emit YAML that would not round-trip back into the same `SemanticModel`. The canonical render walks the tree and validates:

- Every `String` field (`name`, `description`, `alias`, path segments, constraint tokens) is YAML-safe — no control characters, no embedded `---` document separators, no bytes that `serde_yaml` cannot quote.
- Every identifier obeys `00 §4.1`'s identifier grammar.
- Every authored named-entity `id` (if present in input projection metadata) remains canonical UUIDv7 text.
- Every `Expr` round-trips through its `ExprSource` YAML form (`14 §4`).
- Every `LeafExtras` / `ComplexExtras` field that `32b §5` accepts as input is exposed on the output shape.

Failures surface as `Diagnostic<ModelDumpErrorKind>` whose kind is `NotRoundTrippable { path, reason }` where `path` is a dotted-plural addressing expression per `26 §3` (e.g. `datasets.orders.dimensions.weird_name`). Callers rename the offending identifier, strip the offending character from a description, or pre-validate with an author-owned linter before calling `dump_model`.

The guard is **strict** — there is no "try-best-effort" mode in v1; the caller either gets a clean canonical dump or a pinpointed error. Faithful / comment-preserving dump modes are retired (`Q-IO-001`, closed).

#### 10.4.4 What the wrappers do NOT do

- **No multi-file loading.** `load_model` reads exactly one `Source` payload. Directory walks, `$include` expansion, and cross-file merges are **out of scope forever** (`Q-IO-003`, closed). Callers that need multi-source aggregation enumerate blobs on their own side and call `load_model` per blob.
- **No network tooling.** Retries, caching, CDN failover, and credential rotation are `object_store`'s internal concerns (transient retries) or the caller's responsibility (higher-level policies). `semstrait-common::io` exposes primitives only.
- **No comment preservation.** `DumpMode::Canonical` is the only variant. Comment- / anchor-preserving dump is retired (`Q-IO-001`, closed).
- **No implicit format detection.** `load_model` assumes the payload is a `semantic_model:` YAML document and will fail with a `Diagnostic<ModelLoadErrorKind>` whose kind is `Parse(_)` if it is not. `load_catalogs` similarly assumes a `catalogs:` document. Callers dispatch based on filename or an explicit argument. YAML is the only format, forever (`Q-IO-H`, resolved).

#### 10.4.5 Composition with `parse`

The sync parser remains primary for in-memory / already-read payloads:

```rust
let text: String = obtain_somehow(); // e.g. from an HTTP body the caller already drained
let (model, warnings) = semstrait_model::parse(&text)?;

// Deterministic-profile lane:
let (model, warnings) = semstrait_model::parse_with_profile(
    &text,
    IdentityProfile::StrictRequireProvided,
)?;
```

The async wrapper is the "read then parse" composition:

```rust
use semstrait_common::io::Location;
use semstrait_model::io::load_model;

let loc: Location = "./model.yaml".parse()?;
let (model, warnings) = load_model(&loc).await?; // = parse(&loc.read::<String>().await?)

// Or directly against an S3 URL (requires io-aws):
let loc: Location = "s3://my-bucket/models/prod.yaml".parse()?;
let (model, warnings) = load_model(&loc).await?;
```

Both paths produce the same `SemanticModel`. Neither path performs resolution (§10.2) or planning (§10.3).

### 10.5 Feature flags


| Feature  | Gates                                                                                                               | Default | Forwards                      |
| -------- | ------------------------------------------------------------------------------------------------------------------- | ------- | ----------------------------- |
| `serde`  | `Serialize` + `Deserialize` on every public type                                                                    | ON      | —                             |
| `io`     | The `::io` submodule — `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs` / `DumpMode` / error rosters | OFF     | `semstrait-common/io`           |
| `io-aws` | Makes `Location::S3` reachable through `load_*` / `dump_*`; no new model-level surface                              | OFF     | `io`, `semstrait-common/io-aws` |


Per I11. `io` is default-off so the historical pure-type consumer of `semstrait-model` (`parse(&str)` only) pays no async-runtime cost. Callers that want the wrappers enable `io` explicitly; the CLI, `semstrait-api`, and `semstrait-facade` do so by default.

---

## 11. Pointers to Child Docs


| Scope                    | Doc                                                                                    | What lives there                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------------------ | -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Canonical entities**   | `[../foundations/18_entities.md](../foundations/18_entities.md)`                       | `**Relationship`, `RelationshipId`, `Cardinality`, `Integrity`, `Optional`, `CrossFilter`, derived `JoinType`, `TemporalShape`, `ScdType`, `Dimension` / `Measure` / `Metric`, `DimensionType` + body structs, `Additivity`, filter taxonomy, `AiContext`, `Keys`, `SemanticMappingValue` shape, root-pool reference / override grammar, `SR-E-`* entity-level rules. Authoritative for every entity struct shape embedded in 32.** |
| Dataset interior         | `[../data-kinds/21_dataset.md](../data-kinds/21_dataset.md)`                           | Per-Dataset YAML: `dimensions:`, `measures:`, `metrics:`, `filters:`, `keys:`, leaf-only `extras` semantics                                                                                                                                                                                                                                                                                           |
| Grainset interior        | `[../data-kinds/22_grainset.md](../data-kinds/22_grainset.md)`                         | Per-Grainset YAML: child composition, grain-axis, `temporal:` in extras                                                                                                                                                                                                                                                                                                                               |
| Unionset interior        | `[../data-kinds/23_unionset.md](../data-kinds/23_unionset.md)`                         | Per-Unionset YAML: children, `mode:`, coverage                                                                                                                                                                                                                                                                                                                                                        |
| Joinset interior         | `[../data-kinds/24_joinset.md](../data-kinds/24_joinset.md)`                           | Per-Joinset YAML: members, `relationships:` (join graph), anchor                                                                                                                                                                                                                                                                                                                                      |
| Nesting matrix           | `[../data-kinds/26_nesting_matrix.md](../data-kinds/26_nesting_matrix.md)`             | Which parent variant contains which nested variants; `SR-10` + Grainset-child grain rule                                                                                                                                                                                                                                                                                                              |
| Applicability            | `[../data-kinds/25_applicability_matrix.md](../data-kinds/25_applicability_matrix.md)` | Per-variant × foundation-rule cross-cuts                                                                                                                                                                                                                                                                                                                                                              |
| Semantic mapping grammar | `[../foundations/15_mapping_and_binding.md](../foundations/15_mapping_and_binding.md)` | `SemanticMapping` values in detail; the `Binding` process                                                                                                                                                                                                                                                                                                                                             |
| Relationships (planner)  | `[../foundations/16_composition.md](../foundations/16_composition.md)`                 | Composition graph, implicit Joinset synthesis                                                                                                                                                                                                                                                                                                                                                         |
| Temporal shape (planner) | `[../foundations/17_temporal_shape.md](../foundations/17_temporal_shape.md)`           | Planner-level variant semantics, rollup matrix                                                                                                                                                                                                                                                                                                                                                        |
| Catalogs file            | `[./32b_catalogs_yaml.md](./32b_catalogs_yaml.md)`                                     | `catalogs.yaml` grammar; `CatalogRef` reference syntax                                                                                                                                                                                                                                                                                                                                                |
| SemanticManifest         | `[./33_semstrait_manifest.md](./33_semstrait_manifest.md)`                             | How the `SemanticModel` tree lowers to a `SemanticManifest`                                                                                                                                                                                                                                                                                                                                           |
| Core I/O transport       | `[./31b_semstrait_common_io.md](./31b_semstrait_common_io.md)`                             | `Source` / `Sink` / `Location` / `IoErrorKind` that §10.4 composes                                                                                                                                                                                                                                                                                                                                    |


---

*Cross-references use `NN §M.K` for internal sections and full relative paths for other docs.*