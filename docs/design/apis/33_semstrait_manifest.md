---
prereqs: [10, 11, 13, 14, 14a, 14b, 15, 16, 17, 18, 20, 30, 31, 31b]
authoritative-for:
  - the `semstrait-manifest` public-API surface — crate boundary, module layout, re-export posture
  - the `Manifest` struct: top-level field roster, `#[non_exhaustive]` status, serde/persistence posture
  - the `ResolvedDataKind` family: `Simple | Complex(Unionset | Grainset | Joinset)` split at the Manifest layer
  - `ResolvedBinding` / `ResolvedPhysicalSource` / `ResolvedColumnMapping` Manifest-layer shape (refines `15 §9`)
  - `ResolvedExprTable` Manifest-facing surface (`lookup`, `lookup_all`, `iter`; the map is owned here, shape in `14b §2`)
  - `ResolvedRelationship` Manifest-layer shape (refines `16 §3` for the compiled form)
  - `CoverageIndex` and `CompositionIndex` — planner lookup indices materialized at compile time
  - `ManifestMetadata` — compile timestamp, source-hash, schema version, Manifest format version
  - the `compile` function signature — `pub async fn compile(...) -> Result<Manifest, CompileErrors>`; the async boundary is I11a
  - `CompileError` / `CompileErrors` — structured fail-fast error types with stable `COMP_E_*` / `EXPR_E_*` codes
  - `Repository` trait — persistence surface (`save` / `load` / `list`); `async fn load` is the I11b gated entry
  - `InMemoryRepository`, `FileSystemRepository` — the two bundled impls
  - `CatalogProvider::check_schema_drift` — the I11b gated entry for drift validation (pointer forward to `37`)
  - the `semstrait-manifest::io` convenience submodule — `load_manifest` / `dump_manifest` free functions composing `31b` transport (§16.5)
  - per-crate async posture at Manifest layer (compile-time async; post-compile sync for accessors)
  - determinism / I4 upholds at the Manifest byte level
  - Serde / persistence-format policy (shape-stable; encoder adapter-selectable via `Repository`)
  - stability tier: MINOR vs. MAJOR cases per `30 §2` for every public leaf in this doc
  - crate boundaries — no planner code, no I/O except through provider traits and `core::io`, no raw SQL
refined-by:
  - 31b (`semstrait-core::io` — transport vocabulary used by §16.5 and future `Repository` impls)
  - 34 (`semstrait-planner` — consumes `Manifest` synchronously at plan time; never re-resolves)
  - 35 (`semstrait-ir` — consumes `ResolvedExprTable` entries while lowering to `PlanNode`s)
  - 36 (`semstrait-adapter` — consumes `PhysicalExpr` from `ResolvedExprTable` entries at `adapt`)
  - 37 (`semstrait-catalog` — authoritative for `CatalogProvider` / `FileSystem` trait surfaces; `33` only names them)
  - 38 (`semstrait-api` — orchestrates `compile` and exposes `Manifest` through the unified entry)
  - 40 (`implementation/40_refactor_plan.md` — current-vs-target delta for `crates/semstrait-manifest/src/`)
---

# 33. semstrait-manifest

> **Note.** Root-shape authoritative spec: [`32_semstrait_model.md`](32_semstrait_model.md) + [`../data-kinds/26_nesting_matrix.md`](../data-kinds/26_nesting_matrix.md) + [`32b_catalogs_yaml.md`](32b_catalogs_yaml.md). This document predates that spec and is pending refactor.

## Table of Contents

1. [Purpose, Scope, Layering](#1-purpose-scope-layering)
2. [Public Crate Surface](#2-public-crate-surface)
3. [The `Manifest` struct](#3-the-manifest-struct)
4. [`ResolvedDataKind`](#4-resolveddatakind)
5. [`ResolvedBinding` / `ResolvedPhysicalSource` / `ResolvedColumnMapping`](#5-resolvedbinding--resolvedphysicalsource--resolvedcolumnmapping)
6. [`ResolvedExprTable`](#6-resolvedexprtable)
7. [`CoverageIndex` / `CompositionIndex`](#7-coverageindex--compositionindex)
8. [`ResolvedRelationship`](#8-resolvedrelationship)
9. [The `compile` function](#9-the-compile-function)
10. [`CompileError` / `CompileErrors`](#10-compileerror--compileerrors)
11. [`Repository` Trait](#11-repository-trait)
12. [`CatalogProvider::check_schema_drift`](#12-catalogprovidercheck_schema_drift)
13. [Determinism — I4 Uphold](#13-determinism--i4-uphold)
14. [Serde / Persistence Format](#14-serde--persistence-format)
15. [Stability](#15-stability)
16. [Crate Boundaries](#16-crate-boundaries)
17. [Round-1 Open Items](#17-round-1-open-items)

---

## 1. Purpose, Scope, Layering

### 1.1 Crate responsibility

`semstrait-manifest` sits one layer above `semstrait-core` / `semstrait-model` in the workspace DAG (`30 §13`; I7). It owns exactly two things:

1. The **`compile` stage** — the `SemanticModel + Catalog → Manifest` transformation ratified in `10 §3.3`. This is the only stage in the `semstrait-*` pipeline where async I/O is permitted (per I11a).
2. The **`Manifest`** — the sealed, planner-complete, engine-agnostic artifact that `compile` produces and that every stage from `plan` onward consumes synchronously (per I8 / I6).

Persistence (`Repository` trait + two bundled impls, `InMemoryRepository` and `FileSystemRepository`) rides along because Manifests survive across compile invocations in common deployments; that surface is the **second** I11-gated entry (I11b; `Repository::load`).

### 1.2 Scope

`33` ratifies the public-crate surface (§2), the `Manifest` struct and its `Resolved*` family (§3–§8), the `compile` signature (§9), the `CompileError` / `CompileErrors` types with stable codes (§10), the `Repository` trait and bundled impls (§11), the I11b gate for `CatalogProvider::check_schema_drift` (§12), determinism discipline (§13), serde / persistence-format policy (§14), per-leaf stability (§15), and crate boundaries (§16). Round-1 open items are parked per §17.

`33` does NOT ratify: per-variant authoring YAML (→ `32` via `20`–`24`); expression resolution algorithm (→ `14b`); binding resolution algorithm (→ `15 §10`); composition resolution algorithm (→ `16`); per-variant planner strategy (→ `20 §5`, `21`–`24`); `CatalogProvider` / `FileSystem` method rosters (→ `37`); `Repository` byte-level encoding (the shape-stable contract is §14; encoders are caller-chosen); planner entry types (→ `34` / `35`); deprecated symbols (→ `41`).

### 1.3 Design posture — sealed artifact, gated I/O

- **The `Manifest` is a sealed artifact.** Once `compile` returns `Ok(Manifest)`, every field is immutable through `&self` accessors. There is no `insert` / `remove` / `set`. Mutation on a loaded Manifest is a `Repository`-level operation (delete-then-save) and produces a fresh Manifest.
- **Async is confined to `compile` and `Repository`.** Per I11a, `compile` is `async` solely because it awaits `CatalogProvider` and `FileSystem` I/O. Per I11b, `Repository::{save, load, list, delete}` are `async` because restoration may fetch from remote object stores. Every other public function on the Manifest surface is **synchronous**.
- **Post-compile consumption is synchronous.** `plan` / `optimize` / `adapt` consume the `Manifest` through `&` references; `Arc<Manifest>` is the conventional carrier inside `semstrait-api`. Re-entrant lookup is O(log n) per `14b §2.3`.
- **Determinism is cross-cutting.** Every ordered map in the Manifest is a `BTreeMap`; every serialized output is byte-stable given the same input bytes. §13 ratifies the testing discipline.

Per I7, the crate's workspace dependencies are exactly three: `semstrait-core`, `semstrait-model`, `semstrait-catalog`. No dep on `semstrait-planner`, `semstrait-ir`, or any adapter / engine crate. A Manifest artifact flows downward; the manifest crate never reaches back up.

### 1.4 Guardrails — how `33` upholds `00 §9` invariants

| Invariant | Where `33` keeps it |
|---|---|
| **I4** — Manifests are deterministic | `§13` ratifies the testing discipline. Every ordered container is a `BTreeMap`; every index is populated in a deterministic iteration order; serde encoders MUST preserve iteration order. |
| **I5** — resolution completes at compile time | Every `EntityRef` is substituted away in `ResolvedExprTable` per `14b §3`; every `Binding` is fully resolved into `ResolvedBinding` per `15 §10`; no post-compile stage triggers resolution. |
| **I6** — plan-time is synchronous | No public method on `Manifest` or its `Resolved*` substructures is `async`. `Repository::load` and `compile` are the only `async fn` on this crate's surface. |
| **I8** — Manifests are planner-complete | `§3`'s six-field roster covers every lookup the planner requires: resolved data kinds, resolved relationships, expression table, coverage index, composition index, metadata. Nothing is deferred to a post-compile fetch. |
| **I10** — public sum types are `#[non_exhaustive]` | `Manifest`, every `Resolved*` struct and enum, `CompileError`, `Repository*Error`, `CoverageIndex` entry variants — all `#[non_exhaustive]`. The roster is §15's. |
| **I11** — I/O is gated | I11a: `compile` is the only place I/O fires during the main pipeline. I11b: `Repository::load` and `CatalogProvider::check_schema_drift` are the two out-of-band entries. No other public function touches I/O. |
| **I12** — diagnostics are first-class | `CompileError` variants carry stable codes in the `COMP_E_*` / `EXPR_E_*` ranges per `30 §6.2`. Every variant implements `IntoDiagnostic` per `31 §7.4`. |

I1 / I2 / I3 / I7 / I9 apply transitively — `33`'s surface exposes no raw SQL, no physical types beyond the engine-agnostic `DataType` / `PhysicalSource` carriers, no engine identity, no upward deps, no `Compiled*` prefixed types (per `00 §4.3`).

---

## 2. Public Crate Surface

Top-level `pub mod` structure. The compile driver, the Manifest data types, the error enums, and the Repository surface each live in their own module; no cross-module cycles.

```
semstrait-manifest
├── manifest              // Manifest, ManifestMetadata, ManifestId, ManifestFormatVersion
│   ├── datakind          // ResolvedDataKind + variant structs (Simple/Grainset/Unionset/Joinset)
│   ├── binding           // ResolvedBinding, ResolvedPhysicalSource, ResolvedColumnMapping
│   ├── relationship      // ResolvedRelationship, ResolvedRelationshipGraph
│   ├── composition       // ResolvedComposedSemanticInterface, CompositionIndex
│   ├── coverage          // CoverageIndex, CompositionCoverageIndex
│   └── expr              // re-exports `ResolvedExprTable` from `semstrait-core`
├── compile               // compile fn, CompileCtx, sub-pass drivers (pub(crate) below the fn)
├── error                 // CompileError, CompileErrors (extends core's CompileError)
└── repository            // Repository trait, RepositoryError, InMemoryRepository,
                          //   FileSystemRepository, ManifestFormatVersion
```

**Re-exports (crate root `lib.rs`).** The curated public surface:

| Symbol | Module | Purpose |
|---|---|---|
| `Manifest` | `manifest` | the sealed artifact |
| `ManifestId` | `manifest` | content-addressable handle |
| `ManifestMetadata` | `manifest` | compile timestamp, source hash, format version |
| `ManifestFormatVersion` | `repository` | on-disk format discriminator |
| `ResolvedDataKind` | `manifest::datakind` | sum type: `Simple | Complex(Unionset | Grainset | Joinset)` |
| `ResolvedSimpleDataKind`, `ResolvedUnionset`, `ResolvedGrainset`, `ResolvedJoinset` | `manifest::datakind` | per-variant resolved shape |
| `ResolvedBinding` | `manifest::binding` | Manifest-layer Binding |
| `ResolvedPhysicalSource` | `manifest::binding` | Manifest-layer PhysicalSource |
| `ResolvedColumnMapping` | `manifest::binding` | flattened binding map |
| `ResolvedRelationship` | `manifest::relationship` | Manifest-layer Relationship |
| `ResolvedRelationshipGraph` | `manifest::relationship` | adjacency struct retained in the Manifest |
| `ResolvedComposedSemanticInterface` | `manifest::composition` | Manifest-layer composed interface |
| `CompositionIndex` | `manifest::composition` | planner lookup index per `16 §8` |
| `CoverageIndex` | `manifest::coverage` | planner lookup index per `15 §6` |
| `ResolvedExprTable` | `semstrait-core` re-export | expression table (owned by Manifest; shape in `14b §2`) |
| `compile` | `compile` | the async compile entry point |
| `CompileError` | `error` | typed error enum (extends `semstrait-core`'s `CompileError` variant roster) |
| `CompileErrors` | `error` | single-error fail-fast carrier with accumulated warnings |
| `Repository` | `repository` | persistence trait |
| `RepositoryError` | `repository` | typed repository error |
| `InMemoryRepository` | `repository` | HashMap-backed impl |
| `FileSystemRepository` | `repository` | local-fs-backed impl |

No other `pub use` re-exports. Per `30 §3.4`, the facade crate (`semstrait-facade`) is where further re-export convenience lives; `33` is the authoritative list.

**Visibility discipline.** Per `30 §3.1`, every `pub` symbol in this table carries a doc comment, is listed in this document, and has documented invariants. The compile-internal helpers (e.g. `ResolveContext`, `ReferenceGraphBuilder`, `SchemaDriftChecker`) are `pub(crate)` and are not part of `33`'s surface.

---

## 3. The `Manifest` struct

### 3.1 Top-level shape

```rust
#[non_exhaustive]
pub struct Manifest {
    pub resolved_datakinds: BTreeMap<DataKindName, ResolvedDataKind>,
    pub resolved_relationships: BTreeMap<RelationshipId, ResolvedRelationship>,
    pub expr_table: ResolvedExprTable,
    pub coverage_index: CoverageIndex,
    pub composition_index: CompositionIndex,
    pub metadata: ManifestMetadata,
}
```

Per `00 §4.1` (Manifest row) and `10 §3.3` (compile contract). `#[non_exhaustive]` per I10 — future additions (e.g. a `temporal_index`, a pre-computed `grain_matrix`) are MINOR per `30 §2.2`. Every ordered map is a `BTreeMap` per I4 (§13).

**Invariants** (upheld by `compile`):

- Every `BindingId` referenced from `expr_table` / `coverage_index` / `composition_index` appears in exactly one `ResolvedSimpleDataKind` under `resolved_datakinds`.
- Every `RelationshipId` referenced from `expr_table`'s `PathSignature` entries appears in `resolved_relationships`.
- Every `SemanticsName` that any `ResolvedDataKind` exposes has at least one `(name, binding_id)` entry in `expr_table` (I8 / `14b §2.3`'s completeness guarantee).
- The Manifest is byte-deterministic per identical `SemanticModel` + catalog snapshot (§13).

### 3.2 `ManifestMetadata`

```rust
#[non_exhaustive]
pub struct ManifestMetadata {
    pub format_version: ManifestFormatVersion,
    pub source_hash: [u8; 32],
    pub compiled_at: CompileTimestamp,
    pub semstrait_version: &'static str,
    pub warnings: Vec<Diagnostic>,
}
```

Audit + version discipline; never consumed by plan / optimize / adapt. `source_hash` is the deterministic hash of the canonicalized input SemanticModel + catalog-snapshot handle (§13.2), used by content-addressable caches and `ManifestId` derivation. `compiled_at` is canonicalized to whole-seconds before hashing per §13.3. `warnings` mirror the `Vec<Diagnostic>` carried through the fail-fast success arm per `30 §7` (never `Severity::Error`).

**`CompileTimestamp`.** A `#[non_exhaustive]` newtype over `u64` UTC seconds since Unix epoch. `Display` renders ISO-8601.

**Format-version policy.** `ManifestFormatVersion` is a `#[non_exhaustive]` enum starting at `V1`. Any change invalidating a stored Manifest's byte layout (field removal, reorder, variant rename on a persisted enum) bumps the discriminator and is MAJOR per `30 §2.1`; additive growth under `#[non_exhaustive]` is MINOR per `30 §2.2`.

### 3.3 `ManifestId`

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ManifestId(pub [u8; 32]);

impl ManifestId {
    pub fn from_manifest(m: &Manifest) -> Self;
    pub fn from_bytes(bytes: [u8; 32]) -> Self;
    pub fn as_bytes(&self) -> &[u8; 32];
}
```

Content-addressable handle; derived from `ManifestMetadata.source_hash` (§3.2). `Display` renders 64-char lowercase-hex (the form `FileSystemRepository` uses as filename stem); `FromStr` accepts the same. Because it derives from `source_hash`, identical `(SemanticModel, catalog snapshot)` pairs produce the same `ManifestId` regardless of when or where `compile` ran (I4).

### 3.4 Access patterns

```rust
impl Manifest {
    pub fn id(&self) -> ManifestId;
    pub fn datakind(&self, name: &DataKindName) -> Option<&ResolvedDataKind>;
    pub fn relationship(&self, id: RelationshipId) -> Option<&ResolvedRelationship>;
    pub fn expr_table(&self) -> &ResolvedExprTable;
    pub fn coverage_index(&self) -> &CoverageIndex;
    pub fn composition_index(&self) -> &CompositionIndex;
    pub fn metadata(&self) -> &ManifestMetadata;
    pub fn datakinds(&self) -> impl Iterator<Item = (&DataKindName, &ResolvedDataKind)>;
    pub fn relationships(&self) -> impl Iterator<Item = (RelationshipId, &ResolvedRelationship)>;
}
```

Every accessor is `&self` and read-only; iteration is in `BTreeMap` order. No `insert` / `remove` / `set_*` — mutation is a `Repository`-level operation (re-`compile` → `save` → `load`).

---

## 4. `ResolvedDataKind`

### 4.1 Top-level sum type

Mirrors `20 §2.1`'s two-level sum type at the Manifest layer:

```rust
#[non_exhaustive]
pub enum ResolvedDataKind {
    Simple(ResolvedSimpleDataKind),
    Complex(ResolvedComplexDataKind),
}

#[non_exhaustive]
pub enum ResolvedComplexDataKind {
    Unionset(ResolvedUnionset),
    Grainset(ResolvedGrainset),
    Joinset(ResolvedJoinset),
}
```

The split rationale is `20 §2.1`'s: Simple carries a Binding, Complex does not. Downstream code branches on Simple-vs-Complex once, then on the Complex variant. Both enums are `#[non_exhaustive]` per I10.

### 4.2 `ResolvedSimpleDataKind`

```rust
#[non_exhaustive]
pub struct ResolvedSimpleDataKind {
    pub name: DataKindName,
    pub interface: ResolvedSemanticInterface,
    pub binding: ResolvedBinding,
    pub temporal_shape: Option<ResolvedTemporalShape>,
    pub grain: Option<Grain>,
}
```

Fields per `21 §4` and `20 §4.1`. `interface` is the resolved (type-inferred, data-type-finalized) Semantics-facing surface per `11 §6`; `binding` is the single `ResolvedBinding` that `15 §2.1` requires Simple kinds to carry. `temporal_shape` / `grain` are `Some` iff the author-level kind declares or derives the respective axis.

**`ResolvedSemanticInterface`.** Mirrors `SemanticInterface` from `semstrait-model` but every slot carries a concrete `DataType` rather than an `Option<DataType>` (per `14 §6.4`). Full field roster is in `32`'s model-layer chapter and is mirrored here.

### 4.3 `ResolvedUnionset`

```rust
#[non_exhaustive]
pub struct ResolvedUnionset {
    pub name: DataKindName,
    pub composed_interface: ResolvedComposedSemanticInterface,
    pub branches: Vec<DataKindName>,
    pub branch_coverage: BTreeMap<(DataKindName, SemanticsName), CoverageVariant>,
    pub temporal_shape: Option<ResolvedTemporalShape>,
}
```

Per `20 §3` row "Unionset" and `23`. `branches` is the declaration-ordered branch list per `23 §4`; `branch_coverage` matches `23 §6`'s union-side coverage shape; `temporal_shape` is union-level per `17 §6` / `23 §7`.

### 4.4 `ResolvedGrainset`

```rust
#[non_exhaustive]
pub struct ResolvedGrainset {
    pub name: DataKindName,
    pub composed_interface: ResolvedComposedSemanticInterface,
    pub axis: Grain,
    pub levels: Vec<ResolvedGrainsetLevel>,
    pub temporal_shape: Option<ResolvedTemporalShape>,
}

#[non_exhaustive]
pub struct ResolvedGrainsetLevel {
    pub datakind: DataKindName,
    pub grain: Grain,
    /// Non-negative ordinal; `0` = coarsest. Per `22 §4.3`.
    pub ordinal: u32,
}
```

Per `20 §3` row "Grainset" and `22`. `axis` is the shared grain axis; `levels` is in coarsest-to-finest order per `22 §4.2`.

### 4.5 `ResolvedJoinset`

```rust
#[non_exhaustive]
pub struct ResolvedJoinset {
    pub name: DataKindName,
    pub composed_interface: ResolvedComposedSemanticInterface,
    pub anchor: DataKindName,
    pub path: Vec<RelationshipId>,
    pub constituents: Vec<DataKindName>,
    pub as_of_gate: Option<ResolvedAsOfGate>,
    pub temporal_shape: Option<ResolvedTemporalShape>,
}
```

Per `20 §3` row "Joinset" and `24`. v1 ratifies binary joinsets only (per `12 §5.2`); `path` is the anchor-rooted relationship traversal materialized at compile time per `24 §5`; `constituents` is in `path`-traversal order and includes the anchor at index 0. `as_of_gate` is populated iff any constituent is `AsOf`-gated per `17 §8.2` / `24 §5.4` (shape is deferred to `17 §8.2`).

### 4.6 Common accessor — `DataKindOps` at the Manifest layer

```rust
pub trait ResolvedDataKindOps {
    fn name(&self) -> &DataKindName;
    fn interface(&self) -> ResolvedInterfaceView<'_>;
    fn binding(&self) -> Option<&ResolvedBinding>;
    fn temporal_shape(&self) -> Option<&ResolvedTemporalShape>;
}

#[non_exhaustive]
pub enum ResolvedInterfaceView<'a> {
    Bare(&'a ResolvedSemanticInterface),
    Composed(&'a ResolvedComposedSemanticInterface),
}
```

Mirrors `DataKindOps` from `semstrait-model` per `20 §2.2`. Consumed by the planner and audit tooling; external crates may `impl` it per `30 §8` (open trait), but in v1 only the four variants implement it.

---

## 5. `ResolvedBinding` / `ResolvedPhysicalSource` / `ResolvedColumnMapping`

### 5.1 `ResolvedBinding`

Manifest-layer counterpart to `Binding` from `15 §2`. The key differences: `binding_id` is assigned at compile-time (per `14b §2.1` Q2), `sources` are resolved against the catalog, and `column_mapping` is flattened into an O(1)-lookup shape (§5.3).

```rust
/// A compiled `Binding` — links a `SimpleDataKind`'s
/// `SemanticInterface` to one or more `ResolvedPhysicalSource`s through
/// a pre-indexed `ResolvedColumnMapping`. Per `15 §9`.
///
/// Every `BindingId` in a `Manifest` is unique to that Manifest. IDs
/// are not stable across recompiles (per `14b §2.1` Q2).
///
/// `#[non_exhaustive]` per I10.
#[non_exhaustive]
pub struct ResolvedBinding {
    /// Unique within the containing Manifest. Assigned by compile in
    /// parsed-Model iteration order.
    pub binding_id: BindingId,

    /// Resolved physical targets. One or more; multi-source bindings
    /// feed Unionset-style compositions via `15 §5`.
    pub sources: Vec<ResolvedPhysicalSource>,

    /// Flattened O(1)-lookup form of the author-declared
    /// `SemanticMapping` (Model-layer; ratified in `18 §10`). Per §5.3.
    pub column_mapping: ResolvedColumnMapping,
}
```

`BindingId` is re-exported from `semstrait-core` (ratified in `14b §2.1`):

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingId(pub u32);
```

### 5.2 `ResolvedPhysicalSource`

```rust
/// A resolved physical target: a concrete file, table, or snapshot with
/// a catalog-fetched schema and partition discipline. Per `15 §3`.
///
/// In v1 the structural shape is identical to the Model-layer
/// `PhysicalSource`; the Manifest-layer variant exists so future MINOR
/// extensions (e.g. a catalog-side `CatalogEntryId` back-reference)
/// don't force a Model-layer change.
///
/// `#[non_exhaustive]` per I10.
#[non_exhaustive]
pub enum ResolvedPhysicalSource {
    File {
        path: PathBuf,
        format: FileFormat,
        schema: Schema,
        partition_columns: Vec<PartitionColumn>,
    },
    Table {
        catalog: Option<String>,
        database: Option<String>,
        name: String,
        schema: Schema,
        partition_columns: Vec<PartitionColumn>,
    },
    Snapshot {
        table_ref: SnapshotRef,
        schema: Schema,
        partition_columns: Vec<PartitionColumn>,
    },
}
```

`Schema`, `PartitionColumn`, `FileFormat`, `SnapshotRef` are ratified in `15 §3.2`–`§3.4` and in `semstrait-catalog` (`37`); `33` re-exports the ones used on its public surface via the `manifest::binding` module.

**Determinism note.** `partition_columns` and `schema.columns` are `Vec`s ordered by catalog-iteration order (stable per the catalog's own contract; `37`). `BTreeMap` ordering is not imposed here because physical schemas carry a native column order that the adapter must preserve when emitting engine-side types.

### 5.3 `ResolvedColumnMapping`

Flattened from the Model-layer `SemanticMapping` (ratified in `18 §10`) per `15 §9`. Four disjoint category-maps so plan-time code can look up any Semantics's resolution in O(log n):

> **Naming note.** `33` keeps the Manifest-layer name `ResolvedColumnMapping` unchanged even though the Model-layer source was renamed from `ColumnMapping` → `SemanticMapping` in the 2026-04-17 consolidation. Rationale: (a) `Resolved…` types are already namespaced under `manifest::binding`, so the prefix disambiguates without repeating the Model-layer term; (b) renaming this field would be a BREAKING public-API change on a frozen surface; (c) the four-bucket shape (`columns` / `literals` / `computed` / `metadata`) is itself a Manifest-layer refinement of `SemanticMappingValue`'s three-variant roster, with metadata-synthesized `Expr` entries re-homed into `metadata` per `15 §7.3`.

```rust
#[non_exhaustive]
pub struct ResolvedColumnMapping {
    pub columns: BTreeMap<SemanticsName, ColumnName>,
    pub literals: BTreeMap<SemanticsName, ResolvedLiteral>,
    pub computed: BTreeMap<SemanticsName, PhysicalExpr>,
    pub metadata: BTreeMap<SemanticsName, MetadataDimension>,
    pub source_coverage: BTreeMap<CoverageKey, CoverageVariant>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
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

Every `SemanticsName` on the owning DataKind's `SemanticInterface` appears in exactly one of the first four maps. `columns` carries simple references (`expr: column_name`); `literals` carries literal slots; `computed` carries fully-resolved `PhysicalExpr` from `14b §3` (duplicated here in addition to `ResolvedExprTable` so binding-local planner passes can avoid iterating the whole table); `metadata` carries metadata dimensions per `15 §4.3`; `source_coverage` is populated from the author-declared `Coverage` per `15 §6`.

`CoverageKey.source_index` is per-source within a binding (not `BindingId`); the planner needs that granularity when materializing multi-source bindings per `23 §6`. The Manifest-level `CoverageIndex` (§7) adds the `BindingId` dimension. `CoverageVariant` distinguishes Semantics native to the source (`Native`), projected via `NULL`-padding (`NullFill`), or produced via the `computed` map (`Derived`).

`ResolvedLiteral`, `MetadataDimension`, `ColumnName`, `PartitionColumn` are re-exported from `semstrait-core` / `semstrait-model`. `PhysicalExpr` is the `semstrait-core` type from `31 §3.3`.

---

## 6. `ResolvedExprTable`

### 6.1 Placement and ownership

The `ResolvedExprTable` shape is authoritative in `14b §2`. `33`'s role is to document ownership and access at the Manifest layer; all shape details are cross-referenced.

```rust
pub use semstrait_core::expr::{
    ResolvedExprTable,
    ResolvedExprKey,
    ResolvedExprEntry,
    PathSignature,
    RelationshipPath,
    Provenance,
};
```

### 6.2 Keying

Per `14b §2.1`, every entry is keyed by `(SemanticsName, BindingId)`:

```rust
pub struct ResolvedExprKey {
    pub semantics_name: SemanticsName,
    pub binding_id: BindingId,
}
```

The two-dimensional key is the minimal faithful encoding — a single Semantics can resolve against multiple Bindings when a `ComplexDataKind` composes sources (rationale in `14b §2.5`). `BindingId` values are Manifest-unique (per `14b §2.1` Q2), so `(SemanticsName, BindingId)` pairs are globally unique within a Manifest.

### 6.3 Entry shape

Per `14b §2.1`:

```rust
pub struct ResolvedExprEntry {
    pub physical_expr: PhysicalExpr,
    pub inferred_type: DataType,
    pub referenced_columns: Vec<String>,
    pub path_signature: Option<PathSignature>,
    pub provenance: Provenance,
}
```

`physical_expr` is `EntityRef`-free (per `14 §3.6`) and fully type-inferred (per `14b §6`); `inferred_type` is the root type pre-computed for O(1) lookup; `referenced_columns` is the lex-ordered column-name union per `14b §3.8`–`§3.9` (consumed by adapter column-projection per `14b §10`); `path_signature` is `Some` iff a cross-DataKind `EntityRef` was encountered (per `14b §4`); `provenance` is diagnostic-only per `14b §2.6`.

### 6.4 Lookup contract

`Manifest::expr_table()` returns `&ResolvedExprTable`; callers use the table's own methods per `14b §2.3`:

```rust
impl ResolvedExprTable {
    pub fn lookup(&self, name: &SemanticsName, binding_id: BindingId)
        -> Option<&ResolvedExprEntry>;
    pub fn lookup_all(&self, name: &SemanticsName)
        -> impl Iterator<Item = (BindingId, &ResolvedExprEntry)>;
    pub fn iter(&self)
        -> impl Iterator<Item = (&ResolvedExprKey, &ResolvedExprEntry)>;
}
```

- **`lookup` — O(log n)**. Returns `None` only for pairs compile's completeness check did not populate; `14 §6.3` guarantees `Some` for every `(name, binding_id)` the planner can reach.
- **`lookup_all` — O(log n + k)** where `k` is the number of Bindings sourcing the Semantics. Used by planner source-selection over `ComplexDataKind`.
- **`iter` — O(n)** in `(name, binding_id)` lex order. Used by `Repository::save`, adapter column-projection audit (`14b §10`), and debug tooling.

### 6.5 Immutability

Per `14b §2.3`: no `insert` / `remove` / `update` on the public surface. Construction happens inside compile; the table arrives at the Manifest layer sealed. `expr_table()` returns `&` only.

### 6.6 Relationship to `ResolvedColumnMapping.computed`

Per §5.3, `ResolvedBinding.column_mapping.computed` is a subset of `expr_table.iter()` filtered to binding-local Semantics. Compile keeps both in sync: every binding-side `expr` resolution is stored both in `computed` (for fast binding-local access) and in `expr_table` (for per-`(name, binding_id)` lookup).

Storing the same `PhysicalExpr` in two places is intentional; the Manifest is a read-many artifact and both access patterns are hot on the planner / adapter side. `14b §2.4`'s "no interning" choice keeps the duplication at the tree level (`PhysicalExpr` is not interned into a pool), and the serialization cost is small because most computed exprs are short.

---

## 7. `CoverageIndex` / `CompositionIndex`

### 7.1 `CoverageIndex`

Planner lookup over per-`(BindingId, SemanticsName)` coverage. Materializes `15 §6` as a flat map.

```rust
#[non_exhaustive]
pub struct CoverageIndex {
    entries: BTreeMap<BindingSemanticsKey, CoverageVariant>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingSemanticsKey {
    pub binding_id: BindingId,
    pub semantics: SemanticsName,
}

impl CoverageIndex {
    pub fn lookup(&self, binding_id: BindingId, name: &SemanticsName)
        -> Option<CoverageVariant>;
    pub fn lookup_by_binding(&self, binding_id: BindingId)
        -> impl Iterator<Item = (&SemanticsName, CoverageVariant)>;
    pub fn iter(&self)
        -> impl Iterator<Item = (&BindingSemanticsKey, CoverageVariant)>;
}
```

Consumed by the planner's source-selection pass: given a desired Semantics and a candidate Binding, look up the `CoverageVariant` in O(log n). `lookup_by_binding` is `BTreeMap::range` on the binding-prefix; `iter` is lex-ordered for `Repository::save`. Kept in sync with `ResolvedColumnMapping.source_coverage` at compile time.

### 7.2 `CompositionIndex`

Planner lookup over per-`(DataKindName, UnifiedName)` field provenance for every `ComposedSemanticInterface` the Model materialized explicitly (per `16 §8`). Implicit compositions synthesized on demand at plan time (per `16 §4`) do not populate this index.

```rust
#[non_exhaustive]
pub struct CompositionIndex {
    entries: BTreeMap<CompositionKey, CompositionEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CompositionKey {
    pub datakind: DataKindName,
    pub unified_name: UnifiedName,
}

#[non_exhaustive]
pub struct CompositionEntry {
    pub contributing_datakind: DataKindName,
    pub source_semantics: SemanticsName,
    pub coverage: CoverageVariant,
}

impl CompositionIndex {
    pub fn lookup(&self, datakind: &DataKindName, unified: &UnifiedName)
        -> Option<&CompositionEntry>;
    pub fn lookup_by_kind(&self, datakind: &DataKindName)
        -> impl Iterator<Item = (&UnifiedName, &CompositionEntry)>;
    pub fn iter(&self)
        -> impl Iterator<Item = (&CompositionKey, &CompositionEntry)>;
}
```

`UnifiedName` is the canonical per-composition field identity ratified in `16 §6`. Each `(DataKindName, UnifiedName)` pair resolves to exactly one contributor at compile time; ambiguous contributions fail compile per `16 §8`'s error sub-range.

### 7.3 Index-vs-embedded-access trade-off

Both indices duplicate information available by walking `resolved_datakinds` and each binding's `source_coverage`. The duplication is deliberate: the walk form is O(n) per lookup; the index form is O(log n), and the planner hits these hot paths per plan request. Indices are built once at compile time and their cost amortizes. Lazy construction at plan time was rejected because it would push compile-ownable work into the sync stage (I6).

---

## 8. `ResolvedRelationship`

### 8.1 Manifest-layer shape

Refines the Model-layer `Relationship` (ratified in `18 §2`) for the Manifest layer. Structural shape is nearly identical in v1; the Manifest-layer variant exists so future MINOR extensions (catalog-side back-refs, optimizer hints) don't force a Model-layer rev.

```rust
#[non_exhaustive]
pub struct ResolvedRelationship {
    pub id: RelationshipId,
    pub from: DataKindName,
    pub to: DataKindName,
    pub keys: Vec<JoinKeyExprPair>,
    pub cardinality: Cardinality,
    pub join_type: JoinType,
    pub directionality: Directionality,
}

// RelationshipId: ratified in `18 §2.1` as `pub struct RelationshipId(pub u32)`
// with `#[non_exhaustive]` + `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]`.
```

Every `RelationshipId` is Manifest-unique; IDs are not stable across recompiles (same rationale as `BindingId`, per `14b §4.2` Q6). `JoinKeyExprPair`, `Cardinality`, `JoinType`, and `Directionality` are ratified in `18 §2.3`–`§2.6` and re-exported via `manifest::relationship`. (Historical note: the Model-layer `KeyPair` name is retired in favor of `JoinKeyExprPair` per `18 §2.6`.)

### 8.2 `ResolvedRelationshipGraph`

The relationship graph used by `14b`'s cross-kind path resolution is retained in the Manifest so plan-time code does not rebuild it:

```rust
#[non_exhaustive]
pub struct ResolvedRelationshipGraph {
    kinds: BTreeMap<DataKindName, ResolvedRelationshipNode>,
    by_kind: BTreeMap<DataKindName, Vec<RelationshipId>>,
}

#[non_exhaustive]
pub struct ResolvedRelationshipNode {
    pub name: DataKindName,
    pub incident_relationships: Vec<RelationshipId>,
}

impl ResolvedRelationshipGraph {
    pub fn neighbors(&self, kind: &DataKindName) -> &[RelationshipId];
    pub fn other_endpoint(&self, rid: RelationshipId, kind: &DataKindName)
        -> &DataKindName;
}

impl Manifest {
    pub fn relationship_graph(&self) -> &ResolvedRelationshipGraph;
}
```

`incident_relationships` is sorted by ascending `RelationshipId` per `14b §4.2`'s deterministic-neighbor-iteration discipline. The graph is held as a `pub(crate)` field on `Manifest` (not a §3 public field) and surfaced through the accessor so MINOR additions (e.g. a transitive-closure cache) don't churn the field-level stability surface.

---

## 9. The `compile` function

### 9.1 Signature

```rust
pub async fn compile(
    model: SemanticModel,
    provider: &dyn CatalogProvider,
    fs: &dyn FileSystem,
) -> Result<Manifest, CompileErrors>;
```

Compile a validated `SemanticModel` into a sealed `Manifest`. This is the only stage in the `semstrait-*` pipeline where async I/O is permitted (per I11a). The async boundary exists solely to await catalog / filesystem trait methods at compile time; post-compile consumption is strictly synchronous.

Sub-passes run in the order ratified in `14b §9`: (1) reference-graph build + cycle detection; (2) catalog snapshot (schema fetch for every `PhysicalSource`); (3) binding resolution (`15 §10`); (4) relationship graph build (`14b §4.2`); (5) per-`(SemanticsName, BindingId)` expression resolution building `ResolvedExprTable` (`14b §3`); (6) coverage / composition index materialization (`15 §6`, `16 §8`); (7) metadata finalization.

### 9.2 Argument discipline

- **`model: SemanticModel`** — by value; consumed. Callers that need to retain the model clone first.
- **`provider: &dyn CatalogProvider`** — shared reference over a trait object. Static dispatch via generic `<P: CatalogProvider>` was rejected because the trait has async methods (per `30 §9`), the trait boundary is the natural I/O-injection seam, and `dyn` prevents specialization-driven binary bloat. Compile-time dispatch cost is immaterial; the hot path is plan-time.
- **`fs: &dyn FileSystem`** — same discipline. `FileSystem::{list, read, exists}` are the only methods compile invokes.

### 9.3 Return shape

`Result<Manifest, CompileErrors>` matches `30 §7`'s per-stage return-shape table: success returns the `Manifest` plus warnings on `metadata.warnings`; failure returns one fatal `CompileError` plus accumulated warnings via `CompileErrors` (shape per §10).

### 9.4 Async boundary (I11a)

`compile` is `async fn` for exactly one reason: it awaits `CatalogProvider::fetch_schema`, `CatalogProvider::list_objects`, `FileSystem::{list, read, exists}`. These are the only `.await` points in the function body. Sub-passes (reference graph, cycle detection, binding resolution, expression resolution, index build) are all synchronous per `14b §1`'s I6 framing. Awaits happen once at the top during catalog snapshot; after that, compile runs to completion without yielding. This lets compile run on any runtime (`tokio`, `async-std`, `smol`) without pinning one.

### 9.5 Single-shot vs streaming

`compile` is single-shot in v1: one call, one `Manifest`. The Model is consumed whole; no incremental compile API. I4's byte-stable Manifest + content-addressable caching at the `ManifestId` layer (`source_hash` is invariant under formatting-only changes per `13 §5`) covers most incremental workloads. Streaming / incremental compile is tracked as `[TD-33-INCREMENTAL-COMPILE]` (§17).

### 9.6 Warning discipline

Parse / validate warnings are not re-surfaced by `compile` — those are the caller's to route. Compile-stage warnings accumulate on `Manifest.metadata.warnings` on success and `CompileErrors.warnings` on failure. No warning is silently dropped (per `30 §7`).

### 9.7 Thread safety

`Manifest` is `Send + Sync`. `compile` can be called from any async task; `Arc<Manifest>` is the conventional shared carrier. Caller-supplied `CatalogProvider` / `FileSystem` impls must themselves be `Send + Sync` per `37`.

---

## 10. `CompileError` / `CompileErrors`

### 10.1 Typed error enum (`CompileError`)

The Manifest-layer `CompileError` **extends** `semstrait-core::CompileError` (per `31 §8.3`). Per `30`'s typed-error-carrier discipline, structured variants convert to `Diagnostic` at the crate boundary.

```rust
#[non_exhaustive]
pub enum CompileError {
    // -- name resolution (COMP_E_01xx; shared with core per `31 §8.3`)
    UnresolvedEntityRef            { name: String, location: Option<Location> },
    UnreachableSemanticsReference  { name: String, from_kind: String, location: Option<Location> },
    CircularSemanticsReference     { cycle: Vec<String>, location: Option<Location> },
    UnresolvedColumn               { name: String, binding: String, location: Option<Location> },
    UnresolvedCrossKindReference   { name: String, from_kind: String, location: Option<Location> },

    // -- catalog / source resolution (COMP_E_02xx)
    SourceNotFound                 { source: String, location: Option<Location> },
    CatalogUnavailable             { detail: String },
    SchemaResolutionFailed         { source: String, reason: String, location: Option<Location> },
    GlobExpansionFailed            { pattern: String, reason: String, location: Option<Location> },

    // -- schema / binding resolution (COMP_E_03xx; per `15 §10`)
    BindingColumnNotInSchema       { binding: String, column: String, location: Option<Location> },
    BindingCoverageConflict        { binding: String, semantics: String, reason: String },
    BindingLiteralTypeMismatch     { binding: String, semantics: String, declared: DataType, literal: String },
    BindingShapeMalformed          { binding: String, reason: String },
    PartitionColumnNotInSchema     { binding: String, column: String },

    // -- relationship / composition graph (COMP_E_04xx; per `16 §8`)
    CircularRelationship           { cycle: Vec<RelationshipId>, location: Option<Location> },
    IndexBuildFailed               { index: &'static str, reason: String },
    AmbiguousCompositionContributor { datakind: String, unified: String, contributors: Vec<String> },
    CompositionKeyMismatch         { datakind: String, relationship: RelationshipId, reason: String },

    // -- function resolution (EXPR_E_03xx; shared with core per `31 §8.3`)
    UnknownFunction                { name: String, location: Option<Location> },
    FunctionArityMismatch          { name: String, expected: String, got: usize, location: Option<Location> },
    NoMatchingSignature            { name: String, arg_types: Vec<DataType>, tried_signatures: Vec<String>, location: Option<Location> },

    // -- type resolution (EXPR_E_04xx; shared with core per `31 §8.3`)
    TypeInferenceFailure           { reason: String, location: Option<Location> },
    ComputedTypeUnifyConflict      { name: String, declared: DataType, inferred: DataType, location: Option<Location> },
    LiteralOverflow                { value: String, target: DataType, location: Option<Location> },
    LiteralPrecisionLoss           { value: String, target: DataType, location: Option<Location> },
}

impl CompileError {
    pub fn code(&self) -> &'static str;
    pub fn severity(&self) -> Severity;
    pub fn location(&self) -> Option<&Location>;
}

impl IntoDiagnostic for CompileError { fn into_diagnostic(self) -> Diagnostic; }
impl std::fmt::Display for CompileError {}
impl std::error::Error for CompileError {}
```

### 10.2 Stable code allocation

Per `30 §6.2`, ranges are structural and non-sequential. `33` allocates from:

| Sub-range | Category | Variants → codes |
|---|---|---|
| `COMP_E_0100`–`0199` | name resolution | `UnresolvedEntityRef`→`0101`, `UnreachableSemanticsReference`→`0102`, `CircularSemanticsReference`→`0103`, `UnresolvedColumn`→`0104`, `UnresolvedCrossKindReference`→`0105` |
| `COMP_E_0200`–`0299` | catalog / source | `SourceNotFound`→`0201`, `CatalogUnavailable`→`0202`, `SchemaResolutionFailed`→`0203`, `GlobExpansionFailed`→`0204` |
| `COMP_E_0300`–`0399` | schema / binding | `BindingColumnNotInSchema`→`0301`, `BindingCoverageConflict`→`0302`, `BindingLiteralTypeMismatch`→`0303`, `BindingShapeMalformed`→`0304`, `PartitionColumnNotInSchema`→`0305` |
| `COMP_E_0400`–`0499` | relationship / index / composition | `CircularRelationship`→`0401`, `IndexBuildFailed`→`0402`, `AmbiguousCompositionContributor`→`0403`, `CompositionKeyMismatch`→`0404` |
| `EXPR_E_0300`–`0399` | function resolution | `UnknownFunction`→`0301`, `FunctionArityMismatch`→`0302`, `NoMatchingSignature`→`0303` |
| `EXPR_E_0400`–`0499` | type resolution | `TypeInferenceFailure`→`0401`, `ComputedTypeUnifyConflict`→`0402`, `LiteralOverflow`→`0403`, `LiteralPrecisionLoss`→`0404` |

Gaps are intentional (reserved for future additions without renumbering). Shared variants (`UnresolvedEntityRef`, `UnknownFunction`, etc.) are re-exported verbatim from `semstrait-core::CompileError` with identical fields; `33` adds the catalog / binding / relationship / index variants that depend on Manifest-layer types. Downstream consumers pattern-match on one unified enum. Implementation via `#[non_exhaustive]` re-export is tracked as `[TD-33-ERROR-UNIFY]` (§17).

### 10.3 `CompileErrors` fail-fast carrier

```rust
#[non_exhaustive]
pub struct CompileErrors {
    pub fatal: CompileError,
    pub warnings: Vec<Diagnostic>,
}

impl CompileErrors {
    pub fn fatal(&self) -> &CompileError;
    pub fn warnings(&self) -> &[Diagnostic];
    pub fn into_diagnostics(self) -> Vec<Diagnostic>;
}

impl IntoDiagnostic for CompileErrors { fn into_diagnostic(self) -> Diagnostic; }
impl std::fmt::Display for CompileErrors {}
impl std::error::Error for CompileErrors {}
```

Carries exactly one fatal `CompileError` plus warnings / info accumulated prior to the fatal condition. Matches `30 §7`'s fail-fast return shape.

### 10.4 Fail-fast vs. accumulate — why

Per `10 §5` and `30 §7`: `compile` is fail-fast because dependency chains during expression resolution make continuation unreliable. A failed `UnresolvedEntityRef` invalidates every transitively-consuming expression; continuing yields cascades of pseudo-errors with no diagnostic value. `parse` and `validate` accumulate because their errors are structural / local and do not cascade. `14b §5`'s cycle-detection-first pass is a related choice — a single cycle report replaces tens of correlated expression-resolution errors.

### 10.5 Warnings are never silently dropped

Warnings live on `manifest.metadata.warnings` (success arm) or `compile_errors.warnings` (failure arm, rendered alongside the fatal error by `into_diagnostics()`). Per `30 §7`, dropping warnings is an invariant violation, not a caller error.

---

## 11. `Repository` Trait

### 11.1 Surface

```rust
pub trait Repository: Send + Sync {
    async fn save(&self, manifest: &Manifest) -> Result<ManifestId, RepositoryError>;
    async fn load(&self, id: ManifestId) -> Result<Manifest, RepositoryError>;
    async fn list(&self) -> Result<Vec<ManifestId>, RepositoryError>;
    async fn delete(&self, id: ManifestId) -> Result<(), RepositoryError>;
}
```

Persistence trait for `Manifest`s. `load` is one of the two out-of-band I/O entries permitted outside compile (I11b); `save` / `list` / `delete` share the async posture for symmetry. Implementations handle byte-level encoding, storage location, and content-addressable caching; the trait surface is encoding-independent. All four methods are `async fn` in trait per `30 §9`; the trait is **open** (third-party impls like S3-backed, GCS-backed, database-backed are expected per `30 §8.2`). `save` is idempotent (writing the same Manifest twice is a no-op); `delete` of a missing id is `Ok(())`; `list` order is implementation-defined.

### 11.2 `RepositoryError`

```rust
#[non_exhaustive]
pub enum RepositoryError {
    NotFound { id: ManifestId },
    IncompatibleFormat { stored: String, expected: String },
    DecodeFailed { id: ManifestId, reason: String },
    EncodeFailed { reason: String },
    IoFailed { context: String },
    IntegrityViolation { reason: String },
}

impl std::fmt::Display for RepositoryError {}
impl std::error::Error for RepositoryError {}
impl IntoDiagnostic for RepositoryError {}
```

**Code-range allocation.** `IO_E_0100`–`IO_E_0199` is allocated to `Repository` errors. The `IO` prefix is reserved per `30 §6.6`; `33` activates it (migration note per `30 §6.4`). Assignments: `NotFound`→`IO_E_0101`, `IncompatibleFormat`→`IO_E_0102`, `DecodeFailed`→`IO_E_0103`, `EncodeFailed`→`IO_E_0104`, `IoFailed`→`IO_E_0105`, `IntegrityViolation`→`IO_E_0106`. `IntegrityViolation` guards against hand-crafted test inputs whose `ManifestId` disagrees with content hash.

### 11.3 `InMemoryRepository`

```rust
#[non_exhaustive]
pub struct InMemoryRepository { /* crate-private */ }

impl InMemoryRepository { pub fn new() -> Self; }
impl Repository for InMemoryRepository {}
impl Default for InMemoryRepository {}
```

A `BTreeMap`-backed `Repository` intended for tests, single-process caching, and bench. `save` clones, `load` clones back; no serialization exercised. Not internally synchronized — callers wrap in `Arc<Mutex<_>>` if they need cross-thread writers.

### 11.4 `FileSystemRepository`

```rust
#[non_exhaustive]
pub struct FileSystemRepository { /* crate-private */ }

impl FileSystemRepository {
    pub fn new(root: impl Into<PathBuf>, encoding: ManifestEncoding) -> Self;
}

impl Repository for FileSystemRepository {}

#[non_exhaustive]
pub enum ManifestEncoding {
    MessagePack,
    Json,
    // Bincode — reserved; enable-on-demand per `[TD-33-BINCODE]`.
}
```

Local-filesystem-backed `Repository`. File layout: `{root}/{manifest_id.as_hex()}.{ext}` where `ext` is `mpk` / `json` per encoding; a sibling `.meta.json` (containing the `ManifestFormatVersion`) accompanies each primary file. In v1 encodings: `MessagePack` (compact, round-trips exactly) and `Json` (human-inspectable; slower / larger). `FileSystemRepository` is `Send + Sync`; concurrent saves of the same Manifest resolve as idempotent no-ops.

### 11.5 Why `Repository` is open, not sealed

Per `30 §8.1`. Third-party impls (`semstrait-repository-s3`, `-gcs`, `-azure`) are an expected extension axis. The trait's invariants (save-then-load round-trip equality, content-addressable `ManifestId`) are testable by the third party; there's no cross-trait invariant a misbehaving impl could violate to damage `semstrait-*` internals. Sealing would add migration cost without a concrete correctness benefit.

---

## 12. `CatalogProvider::check_schema_drift`

Per I11b there are exactly two out-of-band I/O entries: `Repository::load` (§11) and `CatalogProvider::check_schema_drift`. The latter is a post-compile validation: given an existing `Manifest`, ask the catalog whether the physical schemas that backed each `ResolvedPhysicalSource` have changed since compile. `yes` invalidates the Manifest (recompile recommended); `no` confirms consistency. Not called from the compile pipeline; `semstrait-api` (`38`) exposes it via `Session::validate_manifest`.

Authoritative shape lives in `37`. The expected signature:

```rust
// in semstrait-catalog::CatalogProvider:
async fn check_schema_drift(&self, manifest: &Manifest)
    -> Result<SchemaDriftReport, CatalogError>;
```

`SchemaDriftReport`'s variant roster (`NoDrift` / `ColumnAdded` / `ColumnRemoved` / `TypeChanged` / `SourceVanished`) lives in `37`. Drift is advisory; the caller decides whether to recompile.

**Why I11b-gated.** Performs I/O (catalog lookup against fresh metadata); not part of compile (it's called on an already-compiled Manifest); not part of plan (plan consumes without re-checking). Packaging it as a distinct async, explicitly-gated entry matches I11b's two-entries-total discipline.

**`33`'s forward-ref posture.** No code is exposed here — the method is defined on `CatalogProvider` (owned by `37`). `33` only names the method in its authoritative-for list and documents that callers pass `&Manifest` out of band. No struct / trait in `33` depends on `SchemaDriftReport` or `CatalogError`.

---

## 13. Determinism — I4 Uphold

### 13.1 Ordered-map everywhere

Per `00 §9 I4`: byte-identical Manifests for byte-identical inputs. Every key-keyed collection is a `BTreeMap`, not a `HashMap`:

- `Manifest.resolved_datakinds` / `resolved_relationships`.
- `CoverageIndex.entries` / `CompositionIndex.entries`.
- `ResolvedColumnMapping.{columns, literals, computed, metadata, source_coverage}`.
- `ResolvedExprTable.entries` (per `14b §2.1`).
- `ResolvedRelationshipGraph.{kinds, by_kind}`.

`BTreeMap` ordering is a pure function of the keys' `Ord` impls — no insertion-order dependence. `IndexMap` would force sub-passes to produce specific insertion sequences; `BTreeMap` absorbs the ordering at the container level. Ordered `Vec` fields (e.g. `ResolvedBinding.sources`, `ResolvedUnionset.branches`) preserve author-declared order, deterministic per the Model's parse order (`11 §11.3`).

### 13.2 Timestamp canonicalization

`Manifest.metadata.compiled_at` is wall-clock UTC and not stable across compiles. To keep I4 intact, the content hash used for `ManifestId` derivation excludes the timestamp:

```
source_hash = hash(canonicalize(SemanticModel) || canonicalize(catalog_snapshot))
```

where `canonicalize` is a byte-stable form of each input (ratified in `32` for the Model and `37` for the catalog). Serialized Manifest bytes via `Repository::save` do include the timestamp and therefore are **not** byte-identical across compiles — the determinism guarantee is specifically over `ManifestId` and the Resolved* content-bearing fields. Tests that compare Manifest bytes exactly must use `Manifest::canonical_bytes()` (§14.3).

### 13.3 Testing the I4 invariant

Per `30 §11.4`, CI fixtures cover: (1) a fixture set of Model YAMLs + catalog-snapshot mocks; (2) per fixture: run `compile` twice, assert `manifest.canonical_bytes()` is identical; (3) per fixture: run `compile` with a delay between invocations, assert `ManifestId::from_manifest(&m)` is identical; (4) per fixture: run `compile` on different machines / architectures, assert `canonical_bytes()` is identical (catches endianness-dependent hashes). (2) is per-crate; (3)–(4) are workspace CI.

### 13.4 Determinism across algorithm changes

A compile-internal algorithm change that preserves every `canonical_bytes()` output is PATCH per `30 §11.4`. An equivalent-but-not-byte-identical change (e.g. index-key renaming, `Vec` reordering) is MINOR and requires a `42` migration note. The PATCH/MINOR boundary is drawn at the byte level.

---

## 14. Serde / Persistence Format

### 14.1 Serde posture

Per `30 §10.4`, `serde` support is opt-in via a `serde` feature. `semstrait-manifest`:

- Default-off in v1.
- `serde` feature enables `Serialize` / `Deserialize` on every public type in §2 (`Manifest`, every `Resolved*`, `CoverageIndex`, `CompositionIndex`, `ManifestMetadata`, `ManifestId`, `ManifestFormatVersion`) and transitively enables `semstrait-core`'s `serde` feature.
- `FileSystemRepository` requires `serde`; `InMemoryRepository` does not.

### 14.2 Format choice is `Repository`-selectable

Byte-level encoding is a `FileSystemRepository`-construction choice via `ManifestEncoding`. Third-party `Repository` impls pick their own (bincode, capnp, database-columnar). The Manifest **shape** is serde-derived and stable; the Manifest **wire format** is encoder-dependent. v1 bundled encodings: **MessagePack** (compact, exact round-trip; recommended default), **JSON** (human-inspectable, slower, larger; debugging / `--explain`). **Bincode** reserved per `[TD-33-BINCODE]` — not exposed in v1 because its non-self-describing form makes schema migrations painful.

### 14.3 `canonical_bytes()`

```rust
impl Manifest {
    pub fn canonical_bytes(&self) -> Vec<u8>;
}
```

Canonical byte form for equality comparison and content-addressable hashing. Excludes `metadata.compiled_at` per §13.2. Encoding: bincode with sorted-field emission, no version stamp, no length prefix. Not a wire format; `Repository` impls use their chosen encoder. `#[cfg(feature = "serde")]`-gated. Two Manifests have `canonical_bytes()` equal iff their content fields are pairwise-equal under §13's ordering rules.

### 14.4 Format-version policy revisited

`ManifestFormatVersion` is `#[non_exhaustive]` starting at `V1` (§3.2). Discriminator bumps — each of which requires a `42_migration_notes.md` entry — are triggered by: removing a field from any `#[non_exhaustive]` struct; reordering fields in a way that changes serde's default key emission; renaming a variant on a persisted enum; changing the serde representation (e.g. tagged → untagged). Additive growth stays at the current discriminator per `30 §2.2`.

On `Repository::load`, impls MUST check the stored discriminator against the running crate's max; mismatches surface as `RepositoryError::IncompatibleFormat` (§11.2). Forward-compatible reads (newer stored on older crate) are NOT supported in v1 — a MINOR bump is still a break in the load direction.

### 14.5 What `serde` does NOT gate

Public struct layout (field visibility, method signatures, non-serde trait impls) is identical with or without the feature. Determinism (ordered-map invariants) holds regardless. Default-off is an opt-in for consumers who treat `Manifest` as in-memory-only (e.g. via `InMemoryRepository`).

---

## 15. Stability

### 15.1 Crate tier

Per `30 §13`: `semstrait-manifest` is **Stable in v1**. The `Manifest`, `Resolved*` family, `CompileError`, `Repository` trait are all ratified in this document and carry the workspace-wide MAJOR cadence discipline.

Pre-1.0 rules apply until the synchronized v1.0 cut per `30 §2.3`.

### 15.2 MAJOR cases

Per `30 §2.1`, each of the following is MAJOR:

- Removing a variant from any `ResolvedDataKind::*` / `ResolvedComplexDataKind::*` / `CompileError::*` / `RepositoryError::*` enum.
- Changing the type or meaning of any existing public field on `Manifest`, `ResolvedSimpleDataKind`, `ResolvedBinding`, `ResolvedColumnMapping`, `ResolvedRelationship`, `CoverageIndex`, `CompositionIndex`, `ManifestMetadata`, or any enum variant.
- Changing the `compile` function signature (adding a required argument, changing return type).
- Retiring any `COMP_E_*` / `EXPR_E_*` / `IO_E_*` error code used by this crate.
- Bumping `ManifestFormatVersion` when the new version rejects v1-stored Manifests (a load-direction break).
- Removing the `Repository` trait, renaming it, or changing any of its method signatures in a way that breaks existing impls.
- Changing `canonical_bytes()`'s encoding to produce different bytes for existing Manifests (callers using it for content-addressable caching observe a cache miss).

### 15.3 MINOR cases

Per `30 §2.2`, additive changes:

- Adding a new variant to `ResolvedComplexDataKind` (future `Snapshotset`, `Windowset`, etc.) — the outer `ResolvedDataKind` is `#[non_exhaustive]`.
- Adding a new field to `Manifest` or any `Resolved*` struct (all are `#[non_exhaustive]`).
- Adding a new variant to `CompileError` (e.g. when a new validation category lands in future docs).
- Adding a new `IO_E_*` code within the `IO_E_0100`–`IO_E_0199` range or a new `RepositoryError` variant.
- Adding a new field to `ManifestMetadata` (e.g. a hash of the function-registry extensions).
- Adding a new `ManifestEncoding` variant.
- Adding a new method to `Repository` that carries a `provided` default body (e.g. a batch-load variant). Adding a method without a default is MAJOR.
- Adding a new public free function or type in §2's roster.

### 15.4 PATCH cases

Per `30 §2.1`:

- Internal algorithm improvements that preserve `canonical_bytes()` for every test fixture.
- Doc-comment corrections.
- Improvements to error-message rendering (the `Display` impl) that preserve the stable code.
- Dependency bumps that do not change public types.

### 15.5 Deprecation policy

Per `30 §12`: any symbol slated for removal passes through `#[deprecated]` for at least one MINOR cycle. The `41_deprecations.md` file tracks each deprecation. v1 introduces no deprecations in this crate.

---

## 16. Crate Boundaries

### 16.1 What `semstrait-manifest` does NOT contain

- **No planner code.** `SemanticPlan`, `PlanNode`, `PlanError`, `Request`, `SessionContext` live in `semstrait-planner` / `semstrait-ir`.
- **No adapter code.** `EngineAdapter`, `DialectId`, `AdaptError`, `EngineArtifact` live in `semstrait-adapter`. The Manifest carries only engine-agnostic `DataType` and `PhysicalExpr` (`13 §2`, `14 §2`).
- **No catalog I/O logic.** Catalog / filesystem trait methods are consumed by `compile` but provided by `semstrait-catalog`. No catalog impl is bundled here.
- **No raw SQL.** `ResolvedPhysicalSource` is engine-agnostic; dialect rendering is `semstrait-adapter`'s job.
- **No YAML parser.** `SemanticModel` arrives parsed; parsing lives in `semstrait-model`.
- **No validation logic.** `validate` completes before `compile`; the input `SemanticModel` is structurally sound on entry.

### 16.2 What `semstrait-manifest` DOES contain

The `compile` function and its sub-passes; the `Manifest` struct and every `Resolved*` type; the `CompileError` / `CompileErrors` types with stable codes in the `COMP_E_*` / `EXPR_E_*` / `IO_E_*` ranges; the `Repository` trait and the two bundled impls; the convenience `::io` submodule (§16.5); determinism discipline (BTreeMap everywhere, timestamp canonicalization, `canonical_bytes()`).

### 16.3 Dependency direction

Depends on exactly three workspace crates: `semstrait-core` (for `DataType`, `PhysicalExpr`, `CompileError` core variants, `Diagnostic`, `FunctionRegistry`, `ResolvedExprTable`, and the `io` transport traits from `31b`); `semstrait-model` (for `SemanticModel` and Model-layer names); `semstrait-catalog` (for the `CatalogProvider` / `FileSystem` trait surfaces and `Schema` / `PartitionColumn`). Per I7, no upward dep on `semstrait-planner`, `-ir`, `-adapter`, `-api`, or `-facade`.

### 16.4 Async boundary discipline

Three async surfaces cross this crate's public boundary: `compile` (I11a), `Repository::{save, load, list, delete}` (I11b), and the `::io` convenience wrappers (§16.5, composing `31b` transport). Everything else (accessors, iterators, lookups) is synchronous. The boundary is enforceable by a doc-comment discipline (every `async fn` MUST carry an I11 justification) and a CI audit (tracked as `[TD-33-CLIPPY-ASYNC-GUARD]`).

### 16.5 Manifest-level I/O convenience wrappers (`semstrait-manifest::io`)

A small feature-gated submodule exposes one-shot load / dump helpers that compose `semstrait-core::io` (`31b`) with manifest byte-level encoding, for callers that want single-function ergonomics rather than constructing a full `Repository`:

```rust
use semstrait_core::io::{Source, Sink};

pub mod io {
    pub async fn load_manifest<S: Source + ?Sized>(
        src: &S,
        encoding: ManifestEncoding,
    ) -> Result<Manifest, ManifestLoadError>;

    pub async fn dump_manifest<S: Sink + ?Sized>(
        m: &Manifest,
        sink: &S,
        encoding: ManifestEncoding,
    ) -> Result<(), ManifestDumpError>;

    #[non_exhaustive]
    pub enum ManifestLoadError {
        Io(IoError),
        Decode { encoding: ManifestEncoding, reason: String },
        FormatVersion { found: ManifestFormatVersion, expected: ManifestFormatVersion },
    }

    #[non_exhaustive]
    pub enum ManifestDumpError {
        Io(IoError),
        Encode { encoding: ManifestEncoding, reason: String },
    }
}
```

**Binary transport.** The manifest is a binary artifact (MessagePack by default, JSON as the human-inspectable alternative, both carried at byte level). `load_manifest` calls `src.read_raw().await?` (returning `Bytes`) and hands the result to the encoding's decoder; `dump_manifest` encodes to `Bytes` and calls `sink.write_raw(bytes).await`. Unlike the model wrappers (`32 §10.4`), there is no UTF-8 validation step — manifest bytes are not required to be valid UTF-8 and are never materialized as a `String`.

**Relationship to `Repository`.** `Repository` is the full-fat persistence contract with content-addressable IDs (`ManifestId`), sibling `.meta.json` files, and format-version checks. `manifest::io` is the lightweight "I have a `Source` pointing at manifest bytes; give me a `Manifest`" path. A `FileSystemRepository` (or future `S3Repository`) internally uses the same `core::io` transport via the `object_store`-backed back-ends (`31b §8`); callers that only need one-shot load / dump skip the Repository machinery entirely.

**Error roster.** `ManifestLoadError` / `ManifestDumpError` are `#[non_exhaustive]` enums over `IoError` and the encoding's decode / encode errors, each implementing `IntoDiagnostic`. Stable codes: `manifest.load.io`, `manifest.load.decode`, `manifest.load.format-version`, `manifest.dump.io`, `manifest.dump.encode`. Because `IoError` itself is `#[non_exhaustive]` (`31b §7`), adding `IoError` variants propagates as a MINOR through this layer.

**Feature flag.** Gated behind `manifest`'s `io` feature (default off), which forwards to `semstrait-core/io`. `aws` feature forwards to `semstrait-core/io-aws`.

**Migration note.** Pre-`31b` the manifest crate shipped a `load_text` helper for loading YAML *model* text. Under the ratified layout that utility is superseded by `semstrait-core::io` + `semstrait-model::io::load_model` (`32 §10.4`). Removal of `semstrait-manifest::io::load_text` is the closing step of `TD-008`.

---

## 17. Round-1 Open Items

Parked in `docs/design/open_questions/33_open_questions.md`. Titles:

- **Q1.** `compile` — `SemanticModel` by value vs `&SemanticModel`.
- **Q2.** `CompileError` — unified enum vs split `CoreCompileError` + `ManifestCompileError`.
- **Q3.** `canonical_bytes()` — bincode with sorted fields vs `serde_json` with `preserve_order`.
- **Q4.** `Repository::save` — pre-condition check on `ManifestId::from_manifest(&m)`.
- **Q5.** `ManifestEncoding::Bincode` — enable in v1 or defer.
- **Q6.** `FileSystemRepository` — POSIX advisory locks / tempfile-and-rename discipline.
- **Q7.** `check_schema_drift` — on `CatalogProvider` vs on a separate `DriftChecker` trait.
- **Q8.** `Manifest::datakind` / `Manifest::relationship` — `Option<&T>` vs `Result<&T, ManifestLookupError>`.
- **Q9.** `ResolvedRelationshipGraph` — public field vs accessor-only.
- **Q10.** Compile catalog-error accumulation — narrow exception to fail-fast vs strict uniformity.

---

*Cross-references in this document are by section (e.g. `14b §2.1`, `15 §9`, `16 §8`, `30 §6.2`). No code-path references are used, per `00 §8`.*
