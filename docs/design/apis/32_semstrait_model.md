---
prereqs: [00, 10, 11, 12, 13, 14, 14a, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 30, 31, 31b]
authoritative-for:
  - the root YAML shape for a `semstrait` model — `semantic_model:` wrapper, per-variant plural arrays, shared Semantics pools, `relationships:`
  - the in-memory `SemanticModel` root type — per-variant typed maps, shared pools as `BTreeMap`, `relationships` as `Vec`
  - the DataKind type hierarchy — `DataKindBase` common-fields struct, per-variant `*Body` structs, `Public*` / `Nested*` concrete types, sealed `DataKind` trait hierarchy on structural + behavioral axes, and view enums for heterogeneous iteration
  - the unified `Extras` field set
  - structural rules (SR-*) that govern a valid root-level document
  - the `parse` and `validate` free-function signatures, the `ParseErrorKind` and `ValidateErrorKind` rosters (per `30 §5`), and their `Diagnose` impls
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
      join_type: left
      keys: [{ from: customer_id, to: id }]
      cardinality: many_to_one
      directionality: bidirectional
```

Every child block is optional except `name:`. An empty model — `semantic_model: { name: ... }` — parses successfully. A non-empty model with zero data kinds is a `ValidateErrorKind::EmptyModel` at the validate stage (§9.5).

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

---

## 2. `SemanticModel` Root Type

```rust
#[non_exhaustive]
pub struct SemanticModel {
    pub name: String,
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub labels: Vec<String>,

    // Data kinds — per-variant typed maps.
    pub datasets:  BTreeMap<String, Dataset>,
    pub grainsets: BTreeMap<String, Grainset>,
    pub unionsets: BTreeMap<String, Unionset>,
    pub joinsets:  BTreeMap<String, Joinset>,

    // Shared Semantics pools.
    pub dimensions: BTreeMap<String, Dimension>,
    pub measures:   BTreeMap<String, Measure>,
    pub metrics:    BTreeMap<String, Metric>,

    // Cross-entity relationships (position-significant).
    pub relationships: Vec<Relationship>,
}
```

All fields are `pub` so that consumers can destructure without getter boilerplate. Construction outside `parse` is permitted (test harnesses, tooling).

### 2.1 Global name uniqueness

Data-kind names are globally unique across the four top-level maps: `datasets["sales"]` and `grainsets["sales"]` cannot both exist. Kind: `ParseErrorKind::DuplicateDataKindName` (SR-3).

Shared pools use their own namespace per carrier: `dimensions["region"]` and `measures["region"]` can coexist. Duplicates within a carrier raise `ParseErrorKind::DuplicateSharedSemanticsName`.

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

---

## 3. DataKind Type Hierarchy

Six layers: a common-fields struct, per-variant shared bodies, concrete types in two forms, a sealed trait hierarchy on two axes, per-concrete trait impls, and view enums for heterogeneous iteration.

### 3.1 Common-fields struct — `DataKindBase`

```rust
pub struct DataKindBase {
    pub name: String,
    pub description: Option<String>,
    pub extras: Extras,
}
```

Held inside every per-variant body (§3.2). Carries the universal fields every data kind exposes regardless of variant or form.

### 3.2 Per-variant bodies

Each variant has a single `*Body` struct holding `base: DataKindBase` plus variant-intrinsic structural fields. Public and Nested forms of the same variant wrap the same body (§3.3).

Self-nesting is type-level forbidden by field absence: no `grainsets:` field on `GrainsetBody`, no `unionsets:` field on `UnionsetBody`, no `joinsets:` field on `JoinsetBody`.

```rust
pub struct DatasetBody {
    pub base: DataKindBase,
}

pub struct GrainsetBody {
    pub base:      DataKindBase,
    pub datasets:  Vec<NestedDataset>,
    pub unionsets: Vec<NestedUnionset>,
    pub joinsets:  Vec<NestedJoinset>,
}

pub struct UnionsetBody {
    pub base:      DataKindBase,
    pub datasets:  Vec<NestedDataset>,
    pub grainsets: Vec<NestedGrainset>,
    pub joinsets:  Vec<NestedJoinset>,
    pub mode:      UnionMode,
}

pub struct JoinsetBody {
    pub base:          DataKindBase,
    pub datasets:      Vec<NestedDataset>,
    pub grainsets:     Vec<NestedGrainset>,
    pub unionsets:     Vec<NestedUnionset>,
    /// Joinset-local relationships. Unified `Relationship` shape — same
    /// struct as `SemanticModel.relationships`. See `18 §2`.
    pub relationships: Vec<Relationship>,
}

#[non_exhaustive]
pub enum UnionMode { All, Unique }   // `All` is the default
```

The full nesting matrix is at `26 §1`; structural rules (R1 leaves don't nest; R2 no same-variant self-nesting; R3 ComplexDataKind ≥ 2 children) are at `26 §2`. `Relationship` shape is ratified at `18 §2`; `UnionMode` roster at `23 §4.1` (variant-local).

### 3.3 Concrete types — Public and Nested forms

Each variant has two concrete forms. Public wraps a body plus `ai_context` plus a `SemanticInterface`. Nested wraps only the body.

```rust
// Public (top-level) forms — carry interface + ai_context.
pub struct Dataset  { pub body: DatasetBody,  pub ai_context: Option<AiContext>, pub semantic_interface: SemanticInterface }
pub struct Grainset { pub body: GrainsetBody, pub ai_context: Option<AiContext>, pub semantic_interface: SemanticInterface }
pub struct Unionset { pub body: UnionsetBody, pub ai_context: Option<AiContext>, pub semantic_interface: SemanticInterface }
pub struct Joinset  { pub body: JoinsetBody,  pub ai_context: Option<AiContext>, pub semantic_interface: SemanticInterface }

// Nested (structural) forms — body only.
pub struct NestedDataset  { pub body: DatasetBody }
pub struct NestedGrainset { pub body: GrainsetBody }
pub struct NestedUnionset { pub body: UnionsetBody }
pub struct NestedJoinset  { pub body: JoinsetBody }
```

YAML deserialization uses `#[serde(flatten)]` on the `body:` field so per-variant author YAML reads as one flat object.

### 3.4 Sealed trait hierarchy — `DataKind` + two axes

All traits are sealed inside the crate. The base trait is `DataKind`; two orthogonal axes of sub-traits classify *structural* shape (leaf vs composer) and *behavioral* shape (queryable top-level vs structural shell).

```rust
mod sealed { pub trait Sealed {} }

pub trait DataKind: sealed::Sealed {
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str>;
    fn extras(&self) -> &Extras;
    fn variant(&self) -> DataKindVariant;
    fn form(&self) -> DataKindForm;
}

// ── Structural axis ──────────────────────────────────────────────
pub trait SimpleDataKind:  DataKind {}

pub trait ComplexDataKind: DataKind {
    fn allowed_child_variants(&self) -> &'static [DataKindVariant];
    fn child_count(&self) -> usize;
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_>;
}

// ── Behavioral axis ──────────────────────────────────────────────
pub trait PublicDataKind: DataKind {
    fn ai_context(&self) -> Option<&AiContext>;
    fn semantic_interface(&self) -> &SemanticInterface;
}

pub trait NestedDataKind: DataKind {}

#[non_exhaustive]
pub enum DataKindVariant { Dataset, Grainset, Unionset, Joinset }

#[non_exhaustive]
pub enum DataKindForm { Public, Nested }
```

`SimpleDataKind` and `NestedDataKind` are pure markers: their contribution is the trait bound itself, which lets generic code require leaf-ness or nested-ness without inspecting tags. `ComplexDataKind` and `PublicDataKind` carry axis-specific accessors.

### 3.5 Trait implementation matrix

Every concrete type implements `DataKind`, exactly one trait on the structural axis, and exactly one trait on the behavioral axis.

| Concrete | `DataKind` | Structural axis | Behavioral axis |
|---|---|---|---|
| `Dataset` | ✓ | `SimpleDataKind` | `PublicDataKind` |
| `NestedDataset` | ✓ | `SimpleDataKind` | `NestedDataKind` |
| `Grainset` | ✓ | `ComplexDataKind` | `PublicDataKind` |
| `NestedGrainset` | ✓ | `ComplexDataKind` | `NestedDataKind` |
| `Unionset` | ✓ | `ComplexDataKind` | `PublicDataKind` |
| `NestedUnionset` | ✓ | `ComplexDataKind` | `NestedDataKind` |
| `Joinset` | ✓ | `ComplexDataKind` | `PublicDataKind` |
| `NestedJoinset` | ✓ | `ComplexDataKind` | `NestedDataKind` |

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

| View enum | Implements |
|---|---|
| `AnyDataKindRef` | `DataKind` |
| `PublicDataKindRef` | `DataKind` + `PublicDataKind` |
| `NestedDataKindRef` | `DataKind` + `NestedDataKind` |
| `SimpleDataKindRef` | `DataKind` + `SimpleDataKind` |
| `ComplexDataKindRef` | `DataKind` + `ComplexDataKind` |

Every view method dispatches via `match` to the underlying concrete type. Views own no data and are never persisted; they are constructed on demand by the iterators in §2.2 and by `ComplexDataKind::children_ref` on complex bodies.

### 3.7 Storage layout

- `SemanticModel.{datasets, grainsets, unionsets, joinsets}` — `BTreeMap<String, _>` keyed by name, holding the four **Public** concrete types.
- `*Body.{datasets, grainsets, unionsets, joinsets}` — `Vec<Nested*>` holding the four **Nested** concrete types, constrained by each body's declared child set.

The Public / Nested split is enforced at the type level: no Public-form value can appear inside a body's child vector, and no Nested-form value can appear at the top-level map.

---

## 4. The `Extras` Block

One `Extras` type, used at every data-kind level:

```rust
#[non_exhaustive]
pub struct Extras {
    pub catalog:          Option<CatalogRef>,
    pub storage:          Option<StorageConfig>,
    pub semantic_mapping: Option<SemanticMapping>,
    pub temporal:         Option<TemporalShape>,
}
```

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
    /// internally per `35 §4.2.1`. Empty by default.
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
    /// discovery from filter predicates per `35 §4.2.1`. See `Q-MAP-002`
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

**Authoring guidance — table-root preferred for file sources.** When a path is a Hive-partitioned table or a single-table folder containing many files, the recommended form is the **table-root prefix only** (e.g. `"s3://bucket/orders/"`), not a Hive-partition glob (e.g. `"s3://bucket/orders/year=*/month=*/*.parquet"`). The latter is wrong usage of the wildcarding surface — it forces compile to enumerate per file or per partition, when the engine can do the same far more efficiently from the table-root alone (per `35 §4.2.1`'s 4-consumer alignment). Compile resolves whatever the author writes literally; it does not detect or reject this pattern. See `15 §3.5` for the resolution rule.

### 4.1 Per-effective-level validity

`Extras` fields have different effective levels. A field may be authored on an ancestor complex kind as a default; the effective value at a leaf is computed by walking from the data kind outward to the root, field-by-field, with **more specific overriding default**.

| Field | Effective at | Defaultable from ancestors |
|---|---|---|
| `catalog` | Leaf (`Dataset` / `NestedDataset`) | Any ancestor complex data kind |
| `storage` (incl. nested `partition_def`) | Leaf | Any ancestor |
| `semantic_mapping` | Leaf | Any ancestor (default is `auto` when absent entirely) |
| `temporal.<variant>:` (shape) | Leaf `Dataset`; inherited from any ancestor | Any ancestor complex data kind |
| `temporal.grain:` | Leaf `Dataset` only — **forbidden** on ComplexDataKinds (SR-E-7) | Never inherited (SR-E-8: Grainset children must author explicitly) |

Setting a field at a scope with no eligible descendant is a structural warning (dead config, not fatal). Entity-level invariants (SR-E-6 through SR-E-8) live at `18 §11`.

### 4.2 Variant-intrinsic fields that are NOT in `Extras`

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

**Explicit map.** The author provides a `{ semantic_name: <SemanticMappingValue> }` mapping. Each entry's value is one of **three author-facing variants** (`Column` / `Literal` / `Expr`) — the full `SemanticMappingValue` enum is **4-variant** at the type level (`Column` / `Literal` / `Expr` / `Metadata`), with the 4th `Metadata(MetadataDimensionRecipe)` variant **compile-synthesized only** from the Dimension's own `type: { metadata: ... }` block (per `13 §4.7` / `18 §10.4` / `15 §5.5`). The 4th variant has no `semantic_mapping:` YAML form and is therefore not authored here. The full enum (shape + roster + `LiteralValue` grammar + `MetadataDimensionRecipe`) is ratified in [`18 §10`](../foundations/18_entities.md#10-semanticmapping-value-shape) and consumed by the `Binding` process in [`15 §5`](../foundations/15_mapping_and_binding.md#5-semanticmapping-value-compile-semantics):

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

---

## 6. Structural Rules (SR-*)

Root-level invariants enforced at `parse` and the `validate` stage. Each rule maps to a typed-kind variant in `ParseErrorKind` (§9.2) or `ValidateErrorKind` (§9.5) per `30 §5`.

| ID | Rule | Kind |
|---|---|---|
| **SR-1** | Exactly one `semantic_model:` root key; `deny_unknown_fields` at root. | `ParseErrorKind::UnknownTopLevelBlock` |
| **SR-2** | Nested data kinds MUST NOT carry `ai_context`, `dimensions`, `measures`, `metrics`, `keys`, `filters`. Enforced at the type level: `Nested*` structs (§3.3) wrap only a `*Body` — they have no `ai_context` or `semantic_interface` fields — and implement `NestedDataKind` (§3.4) as the behavioral marker; `deny_unknown_fields` then rejects the interface tags at parse. | `ParseErrorKind::NestedDataKindCarriesInterface` |
| **SR-3** | Names are globally unique across the four top-level data-kind maps (§2.1). | `ParseErrorKind::DuplicateDataKindName` |
| **SR-4** | Same-variant self-nesting is forbidden: no grainset inside a grainset, no unionset inside a unionset, no joinset inside a joinset. Dataset leaves do not nest. Enforced at the type level by each `*Body` struct's child-field set (§3.2). | `ParseErrorKind::IllegalSelfNesting` |
| **SR-5** | `semantic_mapping` in `extras` is effective only at leaves. Presence on a complex kind is a default for descendant leaves, never the complex kind's own mapping. | (no error; semantic rule) |
| **SR-6** | Post-merge effective-level validation: the merged `Extras` at each data kind must satisfy variant-specific structural requirements (e.g. every grainset subtree must resolve a `temporal:` value). | `ValidateErrorKind::MissingRequiredExtras` |
| **SR-7** | `deny_unknown_fields` is applied at every struct parse site (model root, data-kind blocks, extras, relationships, semantic elements). | `ParseErrorKind::UnknownField` |
| **SR-8** | Identifier rules: data-kind names and semantic-element names follow `11 §4`. | `ParseErrorKind::InvalidIdentifier` |
| **SR-9** | `${VAR}` substitution is applied before YAML decoding; unset variables are fatal parse errors (§8). | `ParseErrorKind::UnsetEnvVar` |
| **SR-10** | Every `ComplexDataKind` (`Grainset` / `Unionset` / `Joinset`, Public or Nested) MUST have at least **2 children** across its allowed child-variant arrays. A composer with 0 or 1 children is degenerate — 0 collapses to nothing; 1 collapses to the single child's interface and should be authored as that child directly. Enforced at `validate`. | `ValidateErrorKind::ComplexDataKindInsufficientChildren` |

SR-* numbering is append-only. Adding a rule is a MINOR change per `30 §2`.

Entity-level invariants (`SR-E-*`) — reference-site overrides, Semantics orphan policy, relationship cardinality, TemporalShape grain placement, filter-kind disjointness — live at `18 §11`. SR-E-* is a separate, independently-numbered series that extends the root-level SR-* roster.

---

## 7. Deterministic Ordering (I4)

Given the same input YAML plus the same environment, `parse` produces a byte-identical `SemanticModel` under any canonical serializer.

| Collection | Type | Ordering rule |
|---|---|---|
| `SemanticModel.datasets` | `BTreeMap<String, Dataset>` | Alphabetical by name |
| `SemanticModel.grainsets` | `BTreeMap<String, Grainset>` | Alphabetical by name |
| `SemanticModel.unionsets` | `BTreeMap<String, Unionset>` | Alphabetical by name |
| `SemanticModel.joinsets` | `BTreeMap<String, Joinset>` | Alphabetical by name |
| `SemanticModel.dimensions` | `BTreeMap<String, Dimension>` | Alphabetical by name |
| `SemanticModel.measures` | `BTreeMap<String, Measure>` | Alphabetical by name |
| `SemanticModel.metrics` | `BTreeMap<String, Metric>` | Alphabetical by name |
| `SemanticModel.labels` | `Vec<String>` | YAML author order |
| `SemanticModel.relationships` | `Vec<Relationship>` | YAML author order; first-match-wins semantics per `16 §11` |
| `GrainsetBody.{datasets, unionsets, joinsets}` | `Vec<Nested*>` | YAML author order |
| `UnionsetBody.{datasets, grainsets, joinsets}` | `Vec<Nested*>` | YAML author order |
| `JoinsetBody.{datasets, grainsets, unionsets}` | `Vec<Nested*>` | YAML author order |
| `JoinsetBody.relationships` | `Vec<Relationship>` | YAML author order (unified shape per `18 §2`) |
| `iter_all` / `iter_public` / `iter_simple` / `iter_complex` | iterators | Alphabetical by `(variant-tag, name)`; variants in fixed order: Dataset, Grainset, Unionset, Joinset |

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

The model crate hosts two accumulating stages per `30 §7.1`: `parse` (YAML → `SemanticModel`) and `validate` (structural-precondition pass over a parsed `SemanticModel`). Each owns its own kind enum (`ParseErrorKind`, `ValidateErrorKind`), each implements `Diagnose` per `30 §5.4`, and each uses the accumulating return shape per `30 §7.2`. The fluent loader (§9.6) composes the two behind one async-or-sync entry point.

### 9.1 `parse` signature

```rust
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};

/// YAML `&str` → `SemanticModel`. Pure and synchronous. Accumulating stage
/// per `30 §7.1`: every independent parse error is collected; warnings
/// found during the pass surface on the success arm.
pub fn parse(
    input: &str,
) -> Result<(SemanticModel, Diagnostics<ParseErrorKind>), Diagnostics<ParseErrorKind>>;
```

One entry point. No options, no context, no engine / catalog handle.

### 9.2 `ParseErrorKind` roster

```rust
use semstrait_core::diagnostic::{Diagnose, Severity};
use semstrait_core::Location;

#[non_exhaustive]
pub enum ParseErrorKind {
    // — YAML surface —
    YamlSyntax                    { message: String },
    UnsetEnvVar                   { var: String },
    MalformedRoot                 { reason: String },
    UnknownTopLevelBlock          { block: String },
    UnknownField                  { field: String, parent: String },

    // — Structural rules (SR-*) —
    DuplicateDataKindName         { name: String, occurrences: Vec<Location> },
    NestedDataKindCarriesInterface { parent: String, nested: String, offending_field: String },
    IllegalSelfNesting            { parent_variant: String, nested_variant: String },
    InvalidIdentifier             { raw: String, reason: String },

    // — Shared-pool surface —
    DuplicateSharedSemanticsName  { carrier: String, name: String, occurrences: Vec<Location> },

    // — Semantic-mapping surface —
    MalformedSemanticMappingValue { data_kind: String, semantic_name: String, reason: String },

    // — Extras —
    MalformedCatalogRef           { raw: String, reason: String },
    MalformedTemporalBlock        { reason: String },
}

impl Diagnose for ParseErrorKind {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}
```

**Per-variant location.** The primary source span of an error lives on the wrapping `Diagnostic<ParseErrorKind>` (`30 §5.1`'s `location: Option<Location>` field), not on the variant. Variants carry payload data only — including any *secondary* spans like `occurrences: Vec<Location>` for the duplicate-name variants, which list every offending site beyond the primary.

**No `code()` accessor.** Identification of a parse error is by **variant identity** (`matches!(diag.kind, ParseErrorKind::DuplicateDataKindName { .. })`), not by a string code. Renaming a variant is a MAJOR change per `30 §2.1`; adding a variant inside `#[non_exhaustive]` is MINOR per `30 §2.2`.

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
) -> Result<Diagnostics<ValidateErrorKind>, Diagnostics<ValidateErrorKind>>;
```

`validate` runs the SR-* rules whose enforcement column in §6 reads "Enforced at `validate`" (SR-6, SR-10) plus the entity-level `SR-E-*` invariants from `18 §11`. It does not transform the model; it is a pure precondition checker. Call it as a separate step before `compile` (`33`) when per-stage error routing matters; the fluent loader (§9.6) chains `parse` and `validate` for callers that don't need that granularity.

### 9.5 `ValidateErrorKind` roster

```rust
#[non_exhaustive]
pub enum ValidateErrorKind {
    // — Required-extras presence (SR-6) —
    MissingRequiredExtras            { data_kind: String, missing: String },

    // — Composition shape (SR-10) —
    ComplexDataKindInsufficientChildren { parent: String, child_count: usize },

    // — Empty model —
    EmptyModel,

    // — Entity-level invariants (SR-E-*) per `18 §11` —
    OrphanSharedSemantics            { carrier: String, name: String },
    InvalidReferenceOverride         { /* … */ },
    TemporalGrainOnComplex           { data_kind: String },
    GrainsetChildMissingGrain        { grainset: String, child: String },
    /* … additional SR-E-* variants per `18 §11` … */
}

impl Diagnose for ValidateErrorKind {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}
```

The full `SR-E-*` variant roster is enumerated alongside the entity-level invariants in `18 §11` (the canonical home of SR-E-*); variants here mirror those rules 1-to-1 and land MINOR per `30 §2.2` as new SR-E-* numbers are appended.

### 9.6 Fluent loader: `SemanticModel::loader()`

`parse` and `validate` are the primitives — pure inputs, pure outputs, no config. Most callers want **parse + validate** together, returning a `SemanticModel` ready for `compile` consumption. The fluent loader is the ergonomic entry that fuses the two:

```rust
impl SemanticModel {
    /// Entry point for the fluent loader. Returns a typestate builder
    /// awaiting a source.
    pub fn loader() -> SemanticModelLoader<NoSource>;
}

/// Typestate marker — `NoSource` (no input attached yet) vs `HasSource`
/// (input attached, ready to load). Sealed at the `state` module.
pub struct NoSource(());
pub struct HasSource(()); // payload field is `pub(crate)`; not user-constructible

pub struct SemanticModelLoader<State> { /* state field is pub(crate) */ }

impl SemanticModelLoader<NoSource> {
    /// In-memory YAML payload. Synchronous path — no `io` feature needed.
    pub fn with_yaml_str(self, yaml: impl Into<String>) -> SemanticModelLoader<HasSource>;

    /// Filesystem / object-store payload via `semstrait-core::io::Source`.
    /// `io` feature required.
    #[cfg(feature = "io")]
    pub fn with_yaml_source<S: Source + ?Sized>(self, src: &S) -> SemanticModelLoader<HasSource>;
}

impl SemanticModelLoader<HasSource> {
    /// Async load: read (if a `Source`) → `parse` → `validate`. Async only when the
    /// loader was configured with `with_yaml_source`; otherwise synchronous path
    /// is selected via `load_blocking`.
    #[cfg(feature = "io")]
    pub async fn load(
        self,
    ) -> Result<
        (SemanticModel, Diagnostics<ModelBuildErrorKind>),
        Diagnostics<ModelBuildErrorKind>,
    >;

    /// Synchronous load. Available for `with_yaml_str`-configured loaders only;
    /// `with_yaml_source` requires `load` (async). Compile error if called on the
    /// async-path loader.
    pub fn load_blocking(
        self,
    ) -> Result<
        (SemanticModel, Diagnostics<ModelBuildErrorKind>),
        Diagnostics<ModelBuildErrorKind>,
    >;
}

/// Fused per-stage kind for the loader pipeline. Implements `Diagnose`
/// by delegating to the wrapped stage kind. Per `30 §5.6` the model crate
/// owns this fused sum because the loader composes stages whose kinds
/// live in this same crate (`ParseErrorKind`, `ValidateErrorKind`) plus
/// `IoErrorKind` from `semstrait-core::io`.
#[non_exhaustive]
pub enum ModelBuildErrorKind {
    Parse(ParseErrorKind),
    Validate(ValidateErrorKind),
    #[cfg(feature = "io")]
    Io(semstrait_core::io::IoErrorKind),
}

impl From<ParseErrorKind>    for ModelBuildErrorKind { /* … */ }
impl From<ValidateErrorKind> for ModelBuildErrorKind { /* … */ }
#[cfg(feature = "io")]
impl From<semstrait_core::io::IoErrorKind> for ModelBuildErrorKind { /* … */ }

impl Diagnose for ModelBuildErrorKind { /* delegates to wrapped variant */ }
```

**Stages composed by `load` / `load_blocking`** (in order, fail-fast across stages, accumulating within each stage):

1. **Read** — only when the source is a `Source`-trait input; reads the payload as `String` via `src.read::<String>().await` (`31b §5`). Errors surface as `Diagnostic<ModelBuildErrorKind>` whose kind is `Io(_)`.
2. **Parse** — `parse(&yaml)` (§9.1). Errors surface as `Diagnostic<ModelBuildErrorKind>` whose kind is `Parse(_)`.
3. **Validate** — runs the structural-precondition pass (§9.4–§9.5). Errors surface as `Diagnostic<ModelBuildErrorKind>` whose kind is `Validate(_)`.

Within a stage, every diagnostic the stage produces is collected into the returned `Diagnostics<ModelBuildErrorKind>` vector (parse / validate are accumulating per `30 §7.1`); across stages, the loader halts at the first stage whose Err arm fires and lifts that stage's accumulated set into `ModelBuildErrorKind`. Warnings from earlier stages that completed successfully ride through on the failing stage's Err vector — never silently dropped per `30 §7.3`.

**Why typestate?** A loader without a source attached is a programming error — the typestate machinery makes "call `load()` before attaching a source" a compile-time failure rather than a runtime panic / `Result`. The two states (`NoSource`, `HasSource`) are sealed: only this crate constructs the second-state values.

**Why retain `parse` and `validate` as primary?** The fluent loader is ergonomic sugar. Callers needing per-stage control (custom error routing per stage, cached parse tree reuse, programmatic post-parse mutations before validate) continue to call `parse(&str)` and `validate(&model)` directly. The two surfaces coexist as primitives (`parse`, `validate`) + ergonomic-fused (`loader().with_yaml_*().load[_blocking]()`).

**Composition with `semstrait-api`.** `SemStrait::compile_from_yaml` (`38 §3.3`) is the parallel API at the orchestration layer (it adds compile on top). The two lanes don't compete: in-process callers reach for `SemanticModel::loader()` to obtain a `SemanticModel` they can pass to a separate compile call (e.g. for caching mid-pipeline); end-to-end callers reach for `SemStrait::compile_from_yaml` to skip the intermediate handle. The fused `SemStraitErrorKind` (`30 §5.6`) at `semstrait-api` is parallel to `ModelBuildErrorKind` here — same cross-stage aggregation pattern, broader scope.

#### 9.6.1 Examples

```rust
// Sync path — caller has the YAML in memory.
let (model, warnings) = SemanticModel::loader()
    .with_yaml_str(yaml_text)
    .load_blocking()?;

// Async path — caller has a `Location` (`31b`).
let loc: Location = "s3://bucket/model.yaml".parse()?;
let (model, warnings) = SemanticModel::loader()
    .with_yaml_source(&loc)
    .load()
    .await?;

// Per-stage error routing — caller wants different handling per stage.
match SemanticModel::loader().with_yaml_str(text).load_blocking() {
    Ok((model, warnings))     => use_model(model, warnings),
    Err(diags)                => {
        for d in diags {
            match &d.kind {
                ModelBuildErrorKind::Parse(_)    => report_parse(&d),
                ModelBuildErrorKind::Validate(_) => report_validate(&d),
                ModelBuildErrorKind::Io(_)       => report_io(&d),
            }
        }
    }
}
```

#### 9.6.2 Stability

- `SemanticModelLoader<State>`, the typestate marker types `NoSource` / `HasSource`, the `SemanticModel::loader()` entry, the `with_yaml_*` setters, and `load` / `load_blocking` are **Stable in v1**.
- `ModelBuildErrorKind` is `#[non_exhaustive]`; new stage variants land as MINOR per `30 §2.2`. Removing or renaming a variant is MAJOR per `30 §2.1`.
- The relationship "loader is sugar over `parse + validate`" is a public contract — `load` MUST NOT alter parse / validate semantics; any divergence is a v1 bug.
- The async-only / sync-only split between `load` and `load_blocking` is intentional and stable. A future `with_yaml_source` consumer that wants a sync wrapper bridges via the caller's executor; `semstrait-model::io` does not provide `block_on`.

---

## 10. Crate Boundaries

`semstrait-model` is the thinnest authoring-surface crate. It sits one level above `semstrait-core` in the workspace DAG (I7):

```
semstrait-core      (leaf: Expr / DataType / Diagnostic<K> / Diagnose / …)
    ↑
semstrait-model     (parse + validate + SemanticModel + ParseErrorKind + ValidateErrorKind)
    ↑
semstrait-manifest, semstrait-planner, semstrait-adapter, …
```

Dependencies: `semstrait-core`, `serde`, `serde_yaml`, `tracing` (`30 §6.2`). No other `semstrait-*` crate. No `async`, no `arrow`, no engine-specific deps.

### 10.1 No direct I/O in `parse`

`parse` takes a `&str` and is synchronous. It performs no file opens and no network calls. `std::env::var` is the single allowlisted syscall (used for `${VAR}` substitution per §8). A CI check enumerates direct `std::fs`, `std::net`, `std::process` imports in `semstrait-model` source and fails on any match.

The optional `::io` submodule (§10.4) adds async load / dump wrappers. It does not relax this check: `model::io` never imports `std::fs` / `std::net` / `std::process` directly either — all transport goes through `semstrait-core::io`'s `Source` / `Sink` traits (`31b §3` / `§4`). The CI check applies uniformly to the whole `semstrait-model` source tree.

### 10.2 No resolution

Name resolution, reference expansion, and cross-kind path resolution are `compile`'s responsibility (`33`, `11 §7`). `parse` records identifiers verbatim.

### 10.3 No planning

`Request`, `PlanNode`, `SemanticPlan`, and `SemanticManifest` types never appear in `semstrait-model`.

### 10.4 Model-Level I/O Surface (`semstrait-model::io`)

The optional `::io` submodule provides async wrappers that combine `semstrait-core::io` transport (`31b`) with `parse` / `serialize` for both the `semantic_model:` file and the sibling `catalogs.yaml` file (`32b`). It is feature-gated behind `io` (see §10.5) so callers that only want the sync `parse(&str)` surface don't pull in the async runtime or transport dependencies.

#### 10.4.1 Entry points

```rust
use semstrait_core::io::{Source, Sink};
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};
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

`load_model` is sugar over `src.read::<String>().await.and_then(|text| parse(&text))` — the UTF-8 validation (`FromIoBytes for String`, `31b §5`) surfaces as `Diagnostic<ModelLoadErrorKind>` whose kind is `Io(IoErrorKind::Malformed)` when the source emits non-UTF-8 bytes. `dump_model` is sugar over `sink.write(canonical_render(m)).await` where `canonical_render` yields a `String` (`IntoIoBytes for String`). The value is a single ergonomic entry point that unifies transport (`31b`) with the format-specific parser / serializer owned by this crate. Per `30 §7.1`, `load_*` are accumulating fused helpers (the `parse` / `parse_catalogs` stage they wrap is accumulating); `dump_*` are fail-fast (one I/O failure or round-trip rejection halts the operation).

#### 10.4.2 Error roster

```rust
use semstrait_core::diagnostic::Diagnose;
use semstrait_core::io::IoErrorKind;

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
- Every `Expr` round-trips through its `ExprSource` YAML form (`14 §4`).
- Every `Extras` field that `32b §5` accepts as input is exposed on the output shape.

Failures surface as `Diagnostic<ModelDumpErrorKind>` whose kind is `NotRoundTrippable { path, reason }` where `path` is a dotted-plural addressing expression per `26 §3` (e.g. `datasets.orders.dimensions.weird_name`). Callers rename the offending identifier, strip the offending character from a description, or pre-validate with an author-owned linter before calling `dump_model`.

The guard is **strict** — there is no "try-best-effort" mode in v1; the caller either gets a clean canonical dump or a pinpointed error. Faithful / comment-preserving dump modes are retired (`Q-IO-001`, closed).

#### 10.4.4 What the wrappers do NOT do

- **No multi-file loading.** `load_model` reads exactly one `Source` payload. Directory walks, `$include` expansion, and cross-file merges are **out of scope forever** (`Q-IO-003`, closed). Callers that need multi-source aggregation enumerate blobs on their own side and call `load_model` per blob.
- **No network tooling.** Retries, caching, CDN failover, and credential rotation are `object_store`'s internal concerns (transient retries) or the caller's responsibility (higher-level policies). `semstrait-core::io` exposes primitives only.
- **No comment preservation.** `DumpMode::Canonical` is the only variant. Comment- / anchor-preserving dump is retired (`Q-IO-001`, closed).
- **No implicit format detection.** `load_model` assumes the payload is a `semantic_model:` YAML document and will fail with a `Diagnostic<ModelLoadErrorKind>` whose kind is `Parse(_)` if it is not. `load_catalogs` similarly assumes a `catalogs:` document. Callers dispatch based on filename or an explicit argument. YAML is the only format, forever (`Q-IO-H`, resolved).

#### 10.4.5 Composition with `parse`

The sync parser remains primary for in-memory / already-read payloads:

```rust
let text: String = obtain_somehow(); // e.g. from an HTTP body the caller already drained
let (model, warnings) = semstrait_model::parse(&text)?;
```

The async wrapper is pure sugar for the common "read then parse" pattern:

```rust
use semstrait_core::io::Location;
use semstrait_model::io::load_model;

let loc: Location = "./model.yaml".parse()?;
let (model, warnings) = load_model(&loc).await?; // = parse(&loc.read::<String>().await?)

// Or directly against an S3 URL (requires io-aws):
let loc: Location = "s3://my-bucket/models/prod.yaml".parse()?;
let (model, warnings) = load_model(&loc).await?;
```

Both paths produce the same `SemanticModel`. Neither path performs resolution (§10.2) or planning (§10.3).

### 10.5 Feature flags

| Feature | Gates | Default | Forwards |
|---|---|---|---|
| `serde` | `Serialize` + `Deserialize` on every public type | ON | — |
| `io` | The `::io` submodule — `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs` / `DumpMode` / error rosters | OFF | `semstrait-core/io` |
| `io-aws` | Makes `Location::S3` reachable through `load_*` / `dump_*`; no new model-level surface | OFF | `io`, `semstrait-core/io-aws` |

Per I11. `io` is default-off so the historical pure-type consumer of `semstrait-model` (`parse(&str)` only) pays no async-runtime cost. Callers that want the wrappers enable `io` explicitly; the CLI, `semstrait-api`, and `semstrait-facade` do so by default.

---

## 11. Pointers to Child Docs

| Scope | Doc | What lives there |
|---|---|---|
| **Canonical entities** | [`../foundations/18_entities.md`](../foundations/18_entities.md) | **`Relationship`, `RelationshipId`, `JoinType`, `Cardinality`, `Directionality`, `TemporalShape`, `ScdType`, `Dimension` / `Measure` / `Metric`, `DimensionType` + body structs, `Additivity`, filter taxonomy, `AiContext`, `Keys`, `SemanticMappingValue` shape, root-pool reference / override grammar, `SR-E-*` entity-level rules. Authoritative for every entity struct shape embedded in 32.** |
| Dataset interior | [`../data-kinds/21_dataset.md`](../data-kinds/21_dataset.md) | Per-Dataset YAML: `dimensions:`, `measures:`, `metrics:`, `filters:`, `keys:`, leaf-only `extras` semantics |
| Grainset interior | [`../data-kinds/22_grainset.md`](../data-kinds/22_grainset.md) | Per-Grainset YAML: child composition, grain-axis, `temporal:` in extras |
| Unionset interior | [`../data-kinds/23_unionset.md`](../data-kinds/23_unionset.md) | Per-Unionset YAML: children, `mode:`, coverage |
| Joinset interior | [`../data-kinds/24_joinset.md`](../data-kinds/24_joinset.md) | Per-Joinset YAML: members, `relationships:` (join graph), anchor |
| Nesting matrix | [`../data-kinds/26_nesting_matrix.md`](../data-kinds/26_nesting_matrix.md) | Which parent variant contains which nested variants; `SR-10` + Grainset-child grain rule |
| Applicability | [`../data-kinds/25_applicability_matrix.md`](../data-kinds/25_applicability_matrix.md) | Per-variant × foundation-rule cross-cuts |
| Semantic mapping grammar | [`../foundations/15_mapping_and_binding.md`](../foundations/15_mapping_and_binding.md) | `SemanticMapping` values in detail; the `Binding` process |
| Relationships (planner) | [`../foundations/16_composition.md`](../foundations/16_composition.md) | Composition graph, implicit Joinset synthesis |
| Temporal shape (planner) | [`../foundations/17_temporal_shape.md`](../foundations/17_temporal_shape.md) | Planner-level variant semantics, rollup matrix |
| Catalogs file | [`./32b_catalogs_yaml.md`](./32b_catalogs_yaml.md) | `catalogs.yaml` grammar; `CatalogRef` reference syntax |
| SemanticManifest | [`./33_semstrait_manifest.md`](./33_semstrait_manifest.md) | How the `SemanticModel` tree lowers to a `SemanticManifest` |
| Core I/O transport | [`./31b_semstrait_core_io.md`](./31b_semstrait_core_io.md) | `Source` / `Sink` / `Location` / `IoErrorKind` that §10.4 composes |

---

*Cross-references use `NN §M.K` for internal sections and full relative paths for other docs.*
