---
prereqs: [10, 11, 13, 14, 14a, 15, 16, 17, 18, 19, 20, 30, 31, 31b, 35]
authoritative-for:
  - the `semstrait-manifest` public-API surface — crate boundary, module layout, re-export posture
  - the `SemanticManifest` struct: top-level field roster, `#[non_exhaustive]` status, serde/persistence posture
  - the `ResolvedDataKind` family: `Simple | Complex(Unionset | Grainset | Joinset)` split at the SemanticManifest layer
  - `ResolvedJoinset` / `ResolvedUnionset` carriage of `origin: Origin` per `16 §5.6`; `Grainset` is always `Origin::Explicit`
  - `Origin` / `ImplicitId` — re-exported from `semstrait-common::composition` per `16 §5.6` / `§5.7`; carry the explicit-vs-implicit axis for compositions
  - `ResolvedBinding` / `ResolvedPhysicalSource` / `ResolvedColumnMapping` SemanticManifest-layer shape (refines `15 §9`)
  - `ResolvedExprTable` SemanticManifest-facing surface (`lookup`, `lookup_all`, `iter`; the map is owned here, shape in `19 §3.2`)
  - `ResolvedRelationship` SemanticManifest-layer shape (refines `16 §3` for the compiled form)
  - `CoverageIndex` and `CompositionIndex` — planner lookup indices materialized at compile time, including `CompositionIndex.by_constituent_set` and `by_canonical` per `16 §10` (uniform explicit + implicit lookup)
  - `SemanticManifestMetadata` — compile timestamp, source-hash, schema version, SemanticManifest format version
  - the `compile` function signature — `pub async fn compile(...) -> Result<(SemanticManifest, Diagnostics<CompileError>), (Diagnostic<CompileError>, Diagnostics<CompileError>)>`; the async boundary is I11a
  - `CompileError` — typed-kind enum for the compile stage per `30 §5`; implements `Diagnose`; identification by variant identity (no string-code surface)
  - `Repository` trait — persistence surface (`save` / `load` / `list`); `async fn load` is the I11b gated entry
  - `InMemoryRepository`, `FileSystemRepository` — the two bundled impls
  - `CatalogProvider::check_schema_drift` — the I11b gated entry for drift validation (pointer forward to `37`)
  - the `semstrait-manifest::io` convenience submodule — `load_manifest` / `dump_manifest` free functions composing `31b` transport (§16.5)
  - per-crate async posture at SemanticManifest layer (compile-time async; post-compile sync for accessors)
  - determinism / I4 upholds at the SemanticManifest byte level
  - Serde / persistence-format policy (shape-stable; encoder adapter-selectable via `Repository`)
  - stability tier: MINOR vs. MAJOR cases per `30 §2` for every public leaf in this doc
  - crate boundaries — no planner code, no I/O except through provider traits and `core::io`, no raw SQL
refined-by:
  - 31b (`semstrait-common::io` — transport vocabulary used by §16.5 and future `Repository` impls)
  - 34 (`semstrait-planner` — consumes `SemanticManifest` synchronously at plan time; never re-resolves)
  - 36 (`semstrait-adapter` — consumes `PhysicalExpr` from `ResolvedExprTable` entries at `adapt`)
  - 37 (`semstrait-catalog` — authoritative for `CatalogProvider` / `FileSystem` trait surfaces; `33` only names them)
  - 38 (`semstrait-api` — orchestrates `compile` and exposes `SemanticManifest` through the unified entry)
  - 40 (`implementation/40_refactor_plan.md` — current-vs-target delta for `crates/semstrait-manifest/src/`)
# Note: `35` (`semstrait-ir`) is now a **prerequisite** (above), not a refinement,
# under the second-cascade landing (`STATUS.md` item Q): manifest depends on ir
# for `Expr<L>`, `PhysicalExpr`, `FunctionRegistry`, `CanonicalFn`, and the
# `ir::CompileError` narrow kind. The planner — not ir — consumes
# `ResolvedExprTable` entries when lowering to `PlanNode`s (the `PlanNode`
# container itself is defined in ir but populated by planner).
---

# 33. semstrait-manifest

> **Note.** Root-shape authoritative spec: `[32_semstrait_model.md](32_semstrait_model.md)` + `[../data-kinds/26_nesting_matrix.md](../data-kinds/26_nesting_matrix.md)` + `[32b_catalogs_yaml.md](32b_catalogs_yaml.md)`. This document predates that spec and is pending refactor.

## 1. Purpose, Scope, Layering

### 1.1 Crate responsibility

`semstrait-manifest` sits above `semstrait-common`, `semstrait-ir`, and `semstrait-model` in the workspace DAG (`30 §13`; I7) — post-second-cascade landing (`STATUS.md` item Q) the manifest crate depends on `semstrait-ir` for the full expression vocabulary (`Expr<L>`, `PhysicalExpr`, `FunctionRegistry`, `CanonicalFn`, `ir::CompileError` per `35`). It owns exactly two things:

1. The `**compile` stage** — the `SemanticModel + Catalog → SemanticManifest` transformation ratified in `10 §3.3`. This is the only stage in the `semstrait-*` pipeline where async I/O is permitted (per I11a).
2. The `**SemanticManifest`** — the sealed, planner-complete, engine-agnostic artifact that `compile` produces and that every stage from `plan` onward consumes synchronously (per I8 / I6).

Persistence (`Repository` trait + two bundled impls, `InMemoryRepository` and `FileSystemRepository`) rides along because SemanticManifests survive across compile invocations in common deployments; that surface is the **second** I11-gated entry (I11b; `Repository::load`).

### 1.2 Scope

`33` ratifies the public-crate surface (§2), the `SemanticManifest` struct and its `Resolved`* family (§3–§8), the `compile` signature (§9), the `CompileError` typed-kind enum and its `Diagnose` impl (§10), the `Repository` trait + `RepositoryErrorKind` (§11), the I11b gate for `CatalogProvider::check_schema_drift` (§12), determinism discipline (§13), serde / persistence-format policy (§14), per-leaf stability (§15), and crate boundaries (§16). Round-1 open items are parked per §17.

`33` does NOT ratify: per-variant authoring YAML (→ `32` via `20`–`24`); expression resolution algorithm (→ `19 §3`); binding resolution algorithm (→ `15 §10`); composition resolution algorithm (→ `16`); per-variant planner strategy (→ `20 §5`, `21`–`24`); `CatalogProvider` / `FileSystem` method rosters (→ `37`); `Repository` byte-level encoding (the shape-stable contract is §14; encoders are caller-chosen); planner entry types (→ `34` / `35`); deprecated symbols (→ `41`).

### 1.3 Design posture — sealed artifact, gated I/O

- **The `SemanticManifest` is a sealed artifact.** Once `compile` returns `Ok(SemanticManifest)`, every field is immutable through `&self` accessors. There is no `insert` / `remove` / `set`. Mutation on a loaded SemanticManifest is a `Repository`-level operation (delete-then-save) and produces a fresh SemanticManifest.
- **Async is confined to `compile` and `Repository`.** Per I11a, `compile` is `async` solely because it awaits `CatalogProvider` and `FileSystem` I/O. Per I11b, `Repository::{save, load, list, delete}` are `async` because restoration may fetch from remote object stores. Every other public function on the SemanticManifest surface is **synchronous**.
- **Post-compile consumption is synchronous.** `plan` / `optimize` / `adapt` consume the `SemanticManifest` through `&` references; `Arc<SemanticManifest>` is the conventional carrier inside `semstrait-api`. Re-entrant lookup is O(log n) per `19 §3.2.3`.
- **Determinism is cross-cutting.** Every ordered map in the SemanticManifest is a `BTreeMap`; every serialized output is byte-stable given the same input bytes. §13 ratifies the testing discipline.

Per I7, the crate's workspace dependencies are exactly four: `semstrait-common`, `semstrait-ir`, `semstrait-model`, `semstrait-catalog`. No dep on `semstrait-planner`, `semstrait-adapter`, or any engine crate. A SemanticManifest artifact flows downward; the manifest crate never reaches back up. The `semstrait-ir` dep is the canonical-IR layer producing `Expr<L>`, `PhysicalExpr`, `FunctionRegistry`, and the narrow `ir::CompileError` that `compile` embeds via D.ii nesting (§10).

### 1.4 Guardrails — how `33` upholds `00 §9` invariants


| Invariant                                          | Where `33` keeps it                                                                                                                                                                                                                                                          |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **I4** — SemanticManifests are deterministic       | `§13` ratifies the testing discipline. Every ordered container is a `BTreeMap`; every index is populated in a deterministic iteration order; serde encoders MUST preserve iteration order.                                                                                   |
| **I5** — resolution completes at compile time      | Every `EntityRef` is substituted away in `ResolvedExprTable` per `19 §3.3`; every `Binding` is fully resolved into `ResolvedBinding` per `15 §10`; no post-compile stage triggers resolution.                                                                                 |
| **I6** — plan-time is synchronous                  | No public method on `SemanticManifest` or its `Resolved`* substructures is `async`. `Repository::load` and `compile` are the only `async fn` on this crate's surface.                                                                                                        |
| **I8** — SemanticManifests are planner-complete    | `§3`'s six-field roster covers every lookup the planner requires: resolved data kinds, resolved relationships, expression table, coverage index, composition index, metadata. Nothing is deferred to a post-compile fetch.                                                   |
| **I10** — public sum types are `#[non_exhaustive]` | `SemanticManifest`, every `Resolved`* struct and enum, `CompileError`, `RepositoryErrorKind`, `CoverageIndex` entry variants — all `#[non_exhaustive]`. The roster is §15's.                                                                                             |
| **I11** — I/O is gated                             | I11a: `compile` is the only place I/O fires during the main pipeline. I11b: `Repository::load` and `CatalogProvider::check_schema_drift` are the two out-of-band entries. No other public function touches I/O.                                                              |
| **I12** — diagnostics are first-class              | `CompileError` and `RepositoryErrorKind` implement `Diagnose` per `31 §10`; identification is by variant identity per `30 §5.4` (no string codes). Every public stage entry-point carries `#[tracing::instrument]` per `30 §6.2` for the parallel observability channel. |


I1 / I2 / I3 / I7 / I9 apply transitively — `33`'s surface exposes no raw SQL, no physical types beyond the engine-agnostic `DataType` / `PhysicalSource` carriers, no engine identity, no upward deps, no `Compiled`* prefixed types (per `00 §4.3`).

---

## 2. Public Crate Surface

Top-level `pub mod` structure. The compile driver, the SemanticManifest data types, the error enums, and the Repository surface each live in their own module; no cross-module cycles.

```
semstrait-manifest
├── manifest              // SemanticManifest, SemanticManifestMetadata, SemanticManifestId, SemanticManifestFormatVersion
│   ├── datakind          // ResolvedDataKind + variant structs (Simple/Grainset/Unionset/Joinset)
│   ├── binding           // ResolvedBinding, ResolvedPhysicalSource, ResolvedColumnMapping
│   ├── relationship      // ResolvedRelationship, ResolvedRelationshipGraph
│   ├── composition       // ResolvedComposedSemanticInterface, CompositionIndex
│   ├── coverage          // CoverageIndex, CompositionCoverageIndex
│   └── expr              // owns `ResolvedExprTable` (`PhysicalExpr` entries from `semstrait-ir`)
├── compile               // compile fn, CompileCtx, sub-pass drivers (pub(crate) below the fn)
├── error                 // CompileError (embeds `ir::CompileError` via D.ii); Diagnose impl
└── repository            // Repository trait, RepositoryErrorKind, InMemoryRepository,
                          //   FileSystemRepository, SemanticManifestFormatVersion
```

**Re-exports (crate root `lib.rs`).** The curated public surface:


| Symbol                                                                              | Module                     | Purpose                                                                                                                             |
| ----------------------------------------------------------------------------------- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `SemanticManifest`                                                                  | `manifest`                 | the sealed artifact                                                                                                                 |
| `SemanticManifestId`                                                                | `manifest`                 | content-addressable handle                                                                                                          |
| `SemanticManifestMetadata`                                                          | `manifest`                 | compile timestamp, source hash, format version                                                                                      |
| `SemanticManifestFormatVersion`                                                     | `repository`               | on-disk format discriminator                                                                                                        |
| `ResolvedDataKind`                                                                  | `manifest::datakind`       | sum type: `Simple                                                                                                                   |
| `ResolvedSimpleDataKind`, `ResolvedUnionset`, `ResolvedGrainset`, `ResolvedJoinset` | `manifest::datakind`       | per-variant resolved shape (Joinset / Unionset carry `origin: Origin` per `16 §5.6`)                                                |
| `Origin`, `ImplicitId`                                                              | `manifest::datakind`       | re-exports from `semstrait-common::composition` per `16 §5.6` / `§5.7`; carry the explicit-vs-implicit axis on Joinsets and Unionsets |
| `ResolvedBinding`                                                                   | `manifest::binding`        | SemanticManifest-layer Binding                                                                                                      |
| `ResolvedPhysicalSource`                                                            | `manifest::binding`        | SemanticManifest-layer PhysicalSource                                                                                               |
| `ResolvedColumnMapping`                                                             | `manifest::binding`        | flattened binding map                                                                                                               |
| `ResolvedRelationship`                                                              | `manifest::relationship`   | SemanticManifest-layer Relationship                                                                                                 |
| `ResolvedRelationshipGraph`                                                         | `manifest::relationship`   | adjacency struct retained in the SemanticManifest                                                                                   |
| `ResolvedComposedSemanticInterface`                                                 | `manifest::composition`    | SemanticManifest-layer composed interface                                                                                           |
| `CompositionIndex`                                                                  | `manifest::composition`    | planner lookup index per `16 §8`                                                                                                    |
| `CoverageIndex`                                                                     | `manifest::coverage`       | planner lookup index per `15 §6`                                                                                                    |
| `ResolvedExprTable`                                                                 | `manifest::expr`           | expression table owned by SemanticManifest; entries carry `PhysicalExpr` from `semstrait-ir` (shape in `19 §3.2`)                    |
| `compile`                                                                           | `compile`                  | the async compile entry point                                                                                                       |
| `CompileError`                                                                      | `error`                    | typed-kind enum for the manifest compile stage; embeds `Ir(ir::CompileError)` for narrow function-return-rule failures raised by `semstrait-ir` (D.ii kind-nesting per `30 §7.4`); implements `Diagnose` |
| `Repository`                                                                        | `repository`               | persistence trait                                                                                                                   |
| `RepositoryErrorKind`                                                               | `repository`               | typed-kind enum for repository errors; implements `Diagnose`                                                                        |
| `InMemoryRepository`                                                                | `repository`               | HashMap-backed impl                                                                                                                 |
| `FileSystemRepository`                                                              | `repository`               | local-fs-backed impl                                                                                                                |


No other `pub use` re-exports. Per `30 §3.4`, the facade crate (`semstrait-facade`) is where further re-export convenience lives; `33` is the authoritative list.

**Visibility discipline.** Per `30 §3.1`, every `pub` symbol in this table carries a doc comment, is listed in this document, and has documented invariants. The compile-internal helpers (e.g. `ResolveContext`, `ReferenceGraphBuilder`, `SchemaDriftChecker`) are `pub(crate)` and are not part of `33`'s surface.

---

## 3. The `SemanticManifest` struct

### 3.1 Top-level shape

```rust
#[non_exhaustive]
pub struct SemanticManifest {
    pub resolved_datakinds: BTreeMap<DataKindName, ResolvedDataKind>,
    pub resolved_relationships: BTreeMap<RelationshipId, ResolvedRelationship>,
    pub expr_table: ResolvedExprTable,
    pub coverage_index: CoverageIndex,
    pub composition_index: CompositionIndex,
    pub metadata: SemanticManifestMetadata,
}
```

Per `00 §4.1` (SemanticManifest row) and `10 §3.3` (compile contract). `#[non_exhaustive]` per I10 — future additions (e.g. a `temporal_index`, a pre-computed `grain_matrix`) are MINOR per `30 §2.2`. Every ordered map is a `BTreeMap` per I4 (§13).

**Invariants** (upheld by `compile`):

- Every `BindingId` referenced from `expr_table` / `coverage_index` / `composition_index` appears in exactly one `ResolvedSimpleDataKind` under `resolved_datakinds`.
- Every `RelationshipId` referenced from `expr_table`'s `PathSignature` entries appears in `resolved_relationships`.
- Every `SemanticsName` that any `ResolvedDataKind` exposes has at least one `(name, binding_id)` entry in `expr_table` (I8 / `19 §3.2.3`'s completeness guarantee).
- **Composition completeness** (per `16 §10`). `resolved_datakinds` contains every author-declared composition (`origin: Origin::Explicit`) plus every implicit composition enumerated within the depth/count caps (`Origin::Implicit { id }`). The two populations are disjoint by canonical form per `16 §10.6` — the implicit-explicit clash check guarantees no two `Origin::Implicit { id: a }` and `Origin::Explicit` shadows share an `ImplicitId`.
- **Composition-index uniformity.** `composition_index.by_canonical` is populated for every composition with a canonical form (Joinsets and Unionsets, both explicit and implicit); `composition_index.by_constituent_set` covers all of them. Plan-time lookup never distinguishes explicit from implicit per `34 §7`.
- **Implicit-cap discipline.** Total `Origin::Implicit` compositions across `resolved_datakinds` ≤ `MAX_IMPLICIT_ENUMERATION_COUNT = 2000`. Exceeding the cap fails compile with `CompileError::ImplicitEnumerationExploded` (`§10.1`); a sealed SemanticManifest never observes the breach.
- The SemanticManifest is byte-deterministic per identical `SemanticModel` + catalog snapshot (§13).

### 3.2 `SemanticManifestMetadata`

```rust
#[non_exhaustive]
pub struct SemanticManifestMetadata {
    pub format_version: SemanticManifestFormatVersion,
    pub source_hash: [u8; 32],
    pub compiled_at: CompileTimestamp,
    pub semstrait_version: &'static str,
    pub warnings: Vec<Diagnostic>,
}
```

Audit + version discipline; never consumed by plan / optimize / adapt. `source_hash` is the deterministic hash of the canonicalized input SemanticModel + catalog-snapshot handle (§13.2), used by content-addressable caches and `SemanticManifestId` derivation. `compiled_at` is canonicalized to whole-seconds before hashing per §13.3. `warnings` mirror the `Vec<Diagnostic>` carried through the fail-fast success arm per `30 §7` (never `Severity::Error`).

`**CompileTimestamp`.** A `#[non_exhaustive]` newtype over `u64` UTC seconds since Unix epoch. `Display` renders ISO-8601.

**Format-version policy.** `SemanticManifestFormatVersion` is a `#[non_exhaustive]` enum starting at `V1`. Any change invalidating a stored SemanticManifest's byte layout (field removal, reorder, variant rename on a persisted enum) bumps the discriminator and is MAJOR per `30 §2.1`; additive growth under `#[non_exhaustive]` is MINOR per `30 §2.2`.

### 3.3 `SemanticManifestId`

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SemanticManifestId(pub [u8; 32]);

impl SemanticManifestId {
    pub fn from_manifest(m: &SemanticManifest) -> Self;
    pub fn from_bytes(bytes: [u8; 32]) -> Self;
    pub fn as_bytes(&self) -> &[u8; 32];
}
```

Content-addressable handle; derived from `SemanticManifestMetadata.source_hash` (§3.2). `Display` renders 64-char lowercase-hex (the form `FileSystemRepository` uses as filename stem); `FromStr` accepts the same. Because it derives from `source_hash`, identical `(SemanticModel, catalog snapshot)` pairs produce the same `SemanticManifestId` regardless of when or where `compile` ran (I4).

### 3.4 Access patterns

```rust
impl SemanticManifest {
    pub fn id(&self) -> SemanticManifestId;
    pub fn datakind(&self, name: &DataKindName) -> Option<&ResolvedDataKind>;
    pub fn relationship(&self, id: RelationshipId) -> Option<&ResolvedRelationship>;
    pub fn expr_table(&self) -> &ResolvedExprTable;
    pub fn coverage_index(&self) -> &CoverageIndex;
    pub fn composition_index(&self) -> &CompositionIndex;
    pub fn metadata(&self) -> &SemanticManifestMetadata;
    pub fn datakinds(&self) -> impl Iterator<Item = (&DataKindName, &ResolvedDataKind)>;
    pub fn relationships(&self) -> impl Iterator<Item = (RelationshipId, &ResolvedRelationship)>;
}
```

Every accessor is `&self` and read-only; iteration is in `BTreeMap` order. No `insert` / `remove` / `set_*` — mutation is a `Repository`-level operation (re-`compile` → `save` → `load`).

---

## 4. `ResolvedDataKind`

### 4.1 Top-level sum type

Mirrors `20 §2.1`'s two-level sum type at the SemanticManifest layer:

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

`**ResolvedSemanticInterface`.** Mirrors `SemanticInterface` from `semstrait-model` but every slot carries a concrete `DataType` rather than an `Option<DataType>` (per `14 §6.4`). Full field roster is in `32`'s model-layer chapter and is mirrored here.

### 4.3 `ResolvedUnionset`

```rust
#[non_exhaustive]
pub struct ResolvedUnionset {
    pub name: DataKindName,
    pub origin: Origin,
    pub composed_interface: ResolvedComposedSemanticInterface,
    pub branches: Vec<DataKindName>,
    pub branch_coverage: BTreeMap<(DataKindName, SemanticsName), CoverageVariant>,
    pub temporal_shape: Option<ResolvedTemporalShape>,
}
```

Per `20 §3` row "Unionset" and `23`. `branches` is the declaration-ordered branch list per `23 §4`; `branch_coverage` matches `23 §6`'s union-side coverage shape; `temporal_shape` is union-level per `17 §6` / `23 §7`.

`**origin` carriage (per `16 §5.6` / `§10.5`).** `Origin::Explicit` for author-declared `unionsets:` blocks; `Origin::Implicit { id: ImplicitId }` for compile-enumerated implicit Unionsets (coverage overlap, `16 §10.5`). For `Origin::Implicit`, `name` is a synthetic `__implicit_unionset_<8-hex>` per `16 §5.7`; `branches` is canonically sorted `Vec<DataKindName>` (lex order); `branch_coverage` reflects the per-branch coverage of the implicit fold.

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
    pub origin: Origin,
    pub composed_interface: ResolvedComposedSemanticInterface,
    pub anchor: DataKindName,
    pub path: Vec<RelationshipId>,
    pub constituents: Vec<DataKindName>,
    pub as_of_gate: Option<ResolvedAsOfGate>,
    pub temporal_shape: Option<ResolvedTemporalShape>,
}
```

Per `20 §3` row "Joinset" and `24`. v1 ratifies binary joinsets only (per `12 §5.2`); `path` is the anchor-rooted relationship traversal materialized at compile time per `24 §5`; `constituents` is in `path`-traversal order and includes the anchor at index 0. `as_of_gate` is populated iff any constituent is `AsOf`-gated per `17 §8.2` / `24 §5.4` (shape is deferred to `17 §8.2`).

`**origin` carriage (per `16 §5.6` / `§10.4`).** `Origin::Explicit` for author-declared `joinsets:` blocks; `Origin::Implicit { id: ImplicitId }` for compile-enumerated implicit Joinsets (`16 §10.4`). For `Origin::Implicit`, `name` is a synthetic `__implicit_joinset_<8-hex>` per `16 §5.7`; `anchor` is the first `DataKindName` in the canonical `constituents` order (corresponds to the canonical-form starting node); `path` is the canonically sorted relationship traversal that hashed to the `ImplicitId`. The implicit-explicit clash check (`16 §10.6`) runs at compile before materialization — an explicit `ResolvedJoinset` whose canonical form matches an implicit one is rejected with `COMP_E_0414` per §10's `CompileError` extensions.

### 4.6 Common accessor — `DataKindOps` at the SemanticManifest layer

```rust
pub trait ResolvedDataKindOps {
    fn name(&self) -> &DataKindName;
    fn interface(&self) -> ResolvedInterfaceView<'_>;
    fn binding(&self) -> Option<&ResolvedBinding>;
    fn temporal_shape(&self) -> Option<&ResolvedTemporalShape>;

    /// `Origin::Explicit` for author-declared compositions; `Origin::Implicit { id }`
    /// for compile-enumerated compositions. Returns `Origin::Explicit` for `Simple` /
    /// `Grainset` (always explicit per `16 §5.6`).
    fn origin(&self) -> Origin;
}

#[non_exhaustive]
pub enum ResolvedInterfaceView<'a> {
    Bare(&'a ResolvedSemanticInterface),
    Composed(&'a ResolvedComposedSemanticInterface),
}
```

Mirrors `DataKindOps` from `semstrait-model` per `20 §2.2`. Consumed by the planner and audit tooling; external crates may `impl` it per `30 §8` (open trait), but in v1 only the four variants implement it.

### 4.7 `Origin` and `ImplicitId`

Re-exported from `semstrait-common::composition` per `16 §5.6` / `§5.7`. The shape ratification is owned by `16`; `33` carries it on `ResolvedJoinset` / `ResolvedUnionset`.

```rust
pub use semstrait_common::composition::{Origin, ImplicitId};

// For reference — definitions in `semstrait-common::composition`:
//
// #[non_exhaustive]
// pub enum Origin {
//     Explicit,
//     Implicit { id: ImplicitId },
// }
//
// #[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
// pub struct ImplicitId(pub [u8; 32]);
```

`Origin::Explicit` for author-declared compositions; `Origin::Implicit { id }` for compile-enumerated compositions. `ImplicitId` is the BLAKE3-256 canonical-form hash per `16 §5.7`. SemanticManifest-stable but not stable across recompiles (`RelationshipId` instability per `15 §2.2`).

**Synthetic name pattern.** Implicit compositions are indexed under `DataKindName` of the form `__implicit_{joinset|unionset}_{first-8-hex-of-id}` per `16 §5.7`. The `__` prefix is informally reserved per `11 §X` (current rule: authors SHOULD avoid; `validate` does not currently reject); collisions on the 8-hex prefix are extremely rare at v1 scale. If a collision occurs at compile, `33`'s indexing extends the suffix to the full 64-hex `ImplicitId` to disambiguate.

---

## 5. `ResolvedBinding` / `ResolvedPhysicalSource` / `ResolvedColumnMapping`

### 5.1 `ResolvedBinding`

SemanticManifest-layer counterpart to `Binding` from `15 §2`. The key differences: `binding_id` is assigned at compile-time (per `19 §3.2.1` Q2), `sources` are resolved against the catalog, and `column_mapping` is flattened into an O(1)-lookup shape (§5.3).

```rust
/// A compiled `Binding` — links a `SimpleDataKind`'s
/// `SemanticInterface` to one or more `ResolvedPhysicalSource`s through
/// a pre-indexed `ResolvedColumnMapping`. Per `15 §9`.
///
/// Every `BindingId` in a `SemanticManifest` is unique to that SemanticManifest. IDs
/// are not stable across recompiles (per `19 §3.2.1` Q2).
///
/// `#[non_exhaustive]` per I10.
#[non_exhaustive]
pub struct ResolvedBinding {
    /// Unique within the containing SemanticManifest. Assigned by compile in
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

`BindingId` is re-exported from `semstrait-common` (ratified in `19 §3.2.1`):

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingId(pub u32);
```

### 5.2 `ResolvedPhysicalSource`

```rust
/// A resolved physical target plus per-source compile-time-resolved
/// metadata literals. Per `15 §7.6`.
///
/// `#[non_exhaustive]` per I10.
#[non_exhaustive]
pub struct ResolvedPhysicalSource {
    /// The resolved physical target itself: a concrete file, table, or
    /// snapshot with a catalog-fetched schema and partition discipline
    /// (`15 §3`). The variant set is `PhysicalSource::{File, Table,
    /// Snapshot}` per `15 §3.1`.
    pub source: PhysicalSource,

    /// Per-source resolved metadata literals — one entry per
    /// metadata-bound Semantic in the Binding's interface. Populated
    /// during Coverage derivation (`15 §10.5`) by running the layer-3
    /// `path_token` mechanic (`15 §8.1`) plus a `Cast` to the recipe's
    /// declared `data_type` (`15 §5.5` / §8.1.2). Empty when the Binding
    /// has no metadata-typed Semantics. Keys are exactly the keys of
    /// `ResolvedColumnMapping.metadata` (§5.3); values may differ
    /// across sources.
    pub metadata_values: HashMap<SemanticsName, LiteralValue>,
}

/// Resolved physical target. Variant shape per `15 §3.1`.
#[non_exhaustive]
pub enum PhysicalSource {
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

`Schema`, `PartitionColumn`, `FileFormat`, `SnapshotRef` are ratified in `15 §3.2`–`§3.4` and in `semstrait-catalog` (`37`); `LiteralValue` is from `18 §10.2`; `33` re-exports the ones used on its public surface via the `manifest::binding` module.

**Determinism note.** `partition_columns` and `schema.columns` are `Vec`s ordered by catalog-iteration order (stable per the catalog's own contract; `37`). `BTreeMap` ordering is not imposed on `metadata_values` keys at the SemanticManifest surface because the keys are exactly the `ResolvedColumnMapping.metadata` keys (which are `BTreeMap`-ordered upstream); a `HashMap` here is a planner-speed choice (O(1) lookup at scan time). Physical schemas carry a native column order that the adapter must preserve when emitting engine-side types.

### 5.3 `ResolvedColumnMapping`

Flattened from the Model-layer `SemanticMapping` (ratified in `18 §10`) per `15 §9`. Four disjoint category-maps so plan-time code can look up any Semantics's resolution in O(log n):

> **Naming note.** `33` keeps the SemanticManifest-layer name `ResolvedColumnMapping` unchanged even though the Model-layer source was renamed from `ColumnMapping` → `SemanticMapping` in the 2026-04-17 consolidation. Rationale: (a) `Resolved…` types are already namespaced under `manifest::binding`, so the prefix disambiguates without repeating the Model-layer term; (b) renaming this field would be a BREAKING public-API change on a frozen surface; (c) the four-bucket shape (`columns` / `literals` / `computed` / `metadata`) is the SemanticManifest-layer refinement of `SemanticMappingValue`'s 4-variant roster (`Column` / `Literal` / `Expr` / `Metadata` per `18 §10` / `15 §13 R18`), with each variant routing 1:1 to its dedicated bucket per `15 §7.3`.

```rust
#[non_exhaustive]
pub struct ResolvedColumnMapping {
    pub columns: BTreeMap<SemanticsName, ColumnName>,
    pub literals: BTreeMap<SemanticsName, ResolvedLiteral>,
    pub computed: BTreeMap<SemanticsName, PhysicalExpr>,
    pub metadata: BTreeMap<SemanticsName, MetadataDimensionRecipe>,
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
    Metadata,
}
```

Every `SemanticsName` on the owning DataKind's `SemanticInterface` appears in exactly one of the first four maps. `columns` carries simple references (`expr: column_name`); `literals` carries literal slots; `computed` carries fully-resolved `PhysicalExpr` from `19 §3.3` (duplicated here in addition to `ResolvedExprTable` so binding-local planner passes can avoid iterating the whole table); `metadata` carries the per-Binding metadata-extraction recipe per `15 §5.5` / `18 §10.4` (`MetadataDimensionRecipe` shape, recipe global to the Binding; per-source resolved `LiteralValue`s live on each `ResolvedPhysicalSource.metadata_values` — §5.2); `source_coverage` is populated from the compile-derived `Coverage` per `15 §6`.

`CoverageKey.source_index` is per-source within a binding (not `BindingId`); the planner needs that granularity when materializing multi-source bindings per `23 §6`. The SemanticManifest-level `CoverageIndex` (§7) adds the `BindingId` dimension. `CoverageVariant` distinguishes Semantics native to the source (`Native`), projected via `NULL`-padding (`NullFill`), produced via the `computed` map (`Derived`), or read from the source's per-source `metadata_values` map for metadata-bound Semantics (`Metadata`, per `15 §6.1` 4-variant roster).

`ResolvedLiteral`, `MetadataDimensionRecipe`, `MetadataExtraction`, `ColumnName`, `PartitionColumn`, `LiteralValue` are re-exported from `semstrait-common` / `semstrait-model`. `PhysicalExpr` is the `semstrait-ir` type from `35 §3.6` (sourced via `14 §2`).

---

## 6. `ResolvedExprTable`

### 6.1 Placement and ownership

The `ResolvedExprTable` shape is authoritative in `19 §3.2`. `33`'s role is to document ownership and access at the SemanticManifest layer; all shape details are cross-referenced.

```rust
pub use semstrait_common::expr::{
    ResolvedExprTable,
    ResolvedExprKey,
    ResolvedExprEntry,
    PathSignature,
    RelationshipPath,
    Provenance,
};
```

### 6.2 Keying

Per `19 §3.2.1`, every entry is keyed by `(SemanticsName, BindingId)`:

```rust
pub struct ResolvedExprKey {
    pub semantics_name: SemanticsName,
    pub binding_id: BindingId,
}
```

The two-dimensional key is the minimal faithful encoding — a single Semantics can resolve against multiple Bindings when a `ComplexDataKind` composes sources (rationale in `19 §3.2.5`). `BindingId` values are SemanticManifest-unique (per `19 §3.2.1` Q2), so `(SemanticsName, BindingId)` pairs are globally unique within a SemanticManifest.

### 6.3 Entry shape

Per `19 §3.2.1`:

```rust
pub struct ResolvedExprEntry {
    pub physical_expr:      PhysicalExpr,
    pub inferred_type:      DataType,
    pub referenced_columns: Vec<String>,
    pub path_signature:     Option<PathSignature>,
    pub provenance:         Provenance,
}
```

`physical_expr` carries no semantic-leaf variants per `[14 §3.7](../foundations/14_expressions.md)` and is fully type-inferred per `[19 §3.6](../foundations/19_expression_flow.md)`; `inferred_type` is the root type pre-computed for O(1) lookup; `referenced_columns` is the deduplicated column-name list per `[19 §3.10](../foundations/19_expression_flow.md)` (consumed by adapter column-projection); `path_signature` is `Some` iff a cross-DataKind reference was traversed per `[19 §3.4](../foundations/19_expression_flow.md)`.

#### 6.3.1 `Provenance` (diagnostic-only)

Per-entry source-provenance carrier populated by `compile`. **Never leaves the manifest** — no plan-time or adapt-time consumer reads it. Used by the diagnostic reporter to quote every author `Location` that contributed material to an entry, so a `CompileError` fired against an entry can point finger without re-walking the parse tree.

```rust
pub struct Provenance {
    pub declared_at:              Vec<Location>,        // non-empty
    pub contributing_occurrences: Vec<OccurrenceRef>,   // per 11 §6.3 Tier-1/Tier-2 merge
    pub resolved_from_variant:    Option<OccurrenceRef>,// Some when a local variant overrode the Tier-1 default
}

pub struct OccurrenceRef {
    pub data_kind:       DataKindName,
    pub occurrence_role: OccurrenceRole,
}

#[non_exhaustive]
pub enum OccurrenceRole {
    Tier1Default,
    LocalVariant,
    NestedKindLocal,
}
```

### 6.4 Lookup contract

`SemanticManifest::expr_table()` returns `&ResolvedExprTable`; callers use the table's own methods per `19 §3.2.3`:

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

- `**lookup` — O(log n)**. Returns `None` only for pairs compile's completeness check did not populate; `14 §6.3` guarantees `Some` for every `(name, binding_id)` the planner can reach.
- `**lookup_all` — O(log n + k)** where `k` is the number of Bindings sourcing the Semantics. Used by planner source-selection over `ComplexDataKind`.
- `**iter` — O(n)** in `(name, binding_id)` lex order. Used by `Repository::save`, adapter column-projection audit (`19 §3.10`), and debug tooling.

### 6.5 Immutability

Per `19 §3.2.3`: no `insert` / `remove` / `update` on the public surface. Construction happens inside compile; the table arrives at the SemanticManifest layer sealed. `expr_table()` returns `&` only.

### 6.6 Relationship to `ResolvedColumnMapping.computed`

Per §5.3, `ResolvedBinding.column_mapping.computed` is a subset of `expr_table.iter()` filtered to binding-local Semantics. Compile keeps both in sync: every binding-side `expr` resolution is stored both in `computed` (for fast binding-local access) and in `expr_table` (for per-`(name, binding_id)` lookup).

Storing the same `PhysicalExpr` in two places is intentional; the SemanticManifest is a read-many artifact and both access patterns are hot on the planner / adapter side. `19 §3.2.4`'s "no interning" choice keeps the duplication at the tree level (`PhysicalExpr` is not interned into a pool), and the serialization cost is small because most computed exprs are short.

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

Planner lookup over per-`(DataKindName, UnifiedName)` field provenance for every `ComposedSemanticInterface` materialized at compile — both explicit (`origin: Origin::Explicit`) and implicit (`origin: Origin::Implicit { id }`, per `16 §10.4` / `§10.5`). Plan-time consumption is **lookup-only** per `34 §7`; implicit compositions are *not* synthesized on demand.

```rust
#[non_exhaustive]
pub struct CompositionIndex {
    entries: BTreeMap<CompositionKey, CompositionEntry>,
    by_constituent_set: BTreeMap<ConstituentSet, Vec<DataKindName>>,
    by_canonical: BTreeMap<ImplicitId, DataKindName>,
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

/// Lex-sorted constituent set for plan-time field-first lookup per `16 §10.4` /
/// `34 §7.3`. For Joinsets this is the canonical sorted constituent list; for
/// Unionsets it's the canonical sorted branch list.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ConstituentSet(pub Vec<DataKindName>);

impl CompositionIndex {
    /// `(DataKindName, UnifiedName) → CompositionEntry`. Field-provenance lookup.
    pub fn lookup(&self, datakind: &DataKindName, unified: &UnifiedName)
        -> Option<&CompositionEntry>;

    /// All field provenances for one composition. Used by audit tooling.
    pub fn lookup_by_kind(&self, datakind: &DataKindName)
        -> impl Iterator<Item = (&UnifiedName, &CompositionEntry)>;

    /// Constituent-set → matching composition names. Returns `Vec` because
    /// directional Joinsets can share the same constituent set with different
    /// path orderings (per `16 §5.7`'s canonicalization rule). Used by `34 §7`
    /// field-first resolution for plan-time lookup over implicit + explicit.
    pub fn by_constituent_set(&self, set: &ConstituentSet)
        -> &[DataKindName];

    /// Canonical-form hash → composition name. Used at compile by the
    /// implicit-explicit clash check (`16 §10.6`) and by audit tooling.
    pub fn by_canonical(&self, id: &ImplicitId)
        -> Option<&DataKindName>;

    /// Lex iteration for `Repository::save`.
    pub fn iter(&self)
        -> impl Iterator<Item = (&CompositionKey, &CompositionEntry)>;
}
```

`UnifiedName` is the canonical per-composition field identity ratified in `16 §6`. Each `(DataKindName, UnifiedName)` pair resolves to exactly one contributor at compile time; ambiguous contributions fail compile per `16 §6.4`. Per-row entries cover both explicit and implicit compositions; `34 §7` consumes them uniformly.

`**by_constituent_set` semantics.** The `ConstituentSet` is the lex-sorted `Vec<DataKindName>` of the composition's constituents (Joinset: `constituents` field; Unionset: `branches` field). One set may map to multiple composition names when canonical-direction Joinsets differ from the explicit-declared anchor ordering — `34 §7.3` resolves the ambiguity by walking all candidates and applying the field-coverage match per `16 §11.2`.

`**by_canonical` semantics.** Maps `ImplicitId` to the materialized composition name. Used at compile by the clash check (`16 §10.6`) — if an explicit Joinset's canonical form (`16 §5.7`) hashes to an `ImplicitId` already populated by implicit enumeration, the clash check fires `COMP_E_0414` per §10. After clash resolution, both implicit and explicit compositions appear in this map, so plan-time lookup is uniform.

## 8. `ResolvedRelationship`

### 8.1 SemanticManifest-layer shape

Refines the Model-layer `Relationship` (ratified in `18 §2`) for the SemanticManifest layer. Structural shape is nearly identical in v1; the SemanticManifest-layer variant exists so future MINOR extensions (catalog-side back-refs, optimizer hints) don't force a Model-layer rev.

```rust
#[non_exhaustive]
pub struct ResolvedRelationship {
    pub id: RelationshipId,
    pub from: DataKindName,
    pub to: DataKindName,
    pub keys: Vec<JoinKeyExprPair>,
    pub filter: Option<crate::expr_block::ExprSource>,
    pub cardinality: Cardinality,
    pub integrity: Integrity,
    pub optional: Optional,
    pub cross_filter: CrossFilter,
    /// Derived at compile from `optional` per `18 §2.9`.
    /// Carried on the manifest for downstream consumers (`JoinsetStrategy`,
    /// `PlanNode::Join` emission) so they do not re-derive.
    pub join_type: JoinType,
}

// RelationshipId: ratified in `18 §2.1` as `pub struct RelationshipId(pub u32)`
// with `#[non_exhaustive]` + `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]`.
```

Every `RelationshipId` is SemanticManifest-unique; IDs are not stable across recompiles (same rationale as `BindingId`, per `19 §3.4.2` Q6). `JoinKeyExprPair`, `Cardinality`, `Integrity`, `Optional`, `CrossFilter`, and (derived) `JoinType` are ratified in `18 §2` and re-exported via `manifest::relationship`. **Note:** `optional` and `cross_filter` on the manifest layer are non-optional (`Optional`, `CrossFilter`) — the Model-layer `Option<Optional>` / `Option<CrossFilter>` are resolved to concrete values at compile per `18 §2.7`'s defaults matrix (or per author declaration for `OneToOne` / `ManyToMany`). The Model-layer authored `directionality:` field is retired (2026-05-12); every relationship is bidirectional at the manifest layer by construction. (Historical note: the Model-layer `KeyPair` name is retired in favor of `JoinKeyExprPair` per `18 §2.8`.)

### 8.2 `ResolvedRelationshipGraph`

The relationship graph used by `19 §3.4`'s cross-kind path resolution is retained in the SemanticManifest so plan-time code does not rebuild it:

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

impl SemanticManifest {
    pub fn relationship_graph(&self) -> &ResolvedRelationshipGraph;
}
```

`incident_relationships` is sorted by ascending `RelationshipId` per `19 §3.4.2`'s deterministic-neighbor-iteration discipline. The graph is held as a `pub(crate)` field on `SemanticManifest` (not a §3 public field) and surfaced through the accessor so MINOR additions (e.g. a transitive-closure cache) don't churn the field-level stability surface.

---

## 9. The `compile` function

### 9.1 Signature

```rust
use semstrait_common::diagnostic::{Diagnostic, Diagnostics};

pub async fn compile(
    model: SemanticModel,
    provider: &dyn CatalogProvider,
    fs: &dyn FileSystem,
) -> Result<
    (SemanticManifest, Diagnostics<CompileError>),
    (Diagnostic<CompileError>, Diagnostics<CompileError>),
>;
```

Compile a validated `SemanticModel` into a sealed `SemanticManifest`. Fail-fast stage per `30 §7.1`: the success arm carries the `SemanticManifest` plus any warnings emitted during compile; the failure arm carries the single fatal `Diagnostic<CompileError>` plus all warnings observed before that point. This is the only stage in the `semstrait-*` pipeline where async I/O is permitted (per I11a). The async boundary exists solely to await catalog / filesystem trait methods at compile time; post-compile consumption is strictly synchronous.

Sub-passes run in the order ratified in `19 §3.9`: (1) reference-graph build + cycle detection; (2) catalog snapshot (schema fetch for every `PhysicalSource`); (3) binding resolution (`15 §10`); (4) relationship graph build (`19 §3.4.2`); (5) per-`(SemanticsName, BindingId)` expression resolution building `ResolvedExprTable` (`19 §3.3`); (6) **explicit-composition materialization** (`16 §10.1`) — `ResolvedJoinset` / `ResolvedUnionset` / `ResolvedGrainset` for author-declared compositions, with `origin: Origin::Explicit`; (7) **implicit-composition enumeration** (`16 §10.4` Joinsets, `§10.5` Unionsets) — bounded by `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` and `MAX_IMPLICIT_ENUMERATION_COUNT = 2000`; emits `COMP_E_0409 ImplicitEnumerationExploded` if the count cap is exceeded; (8) **implicit-explicit clash check** (`16 §10.6`) — fails compile with `COMP_E_0414 ExplicitImplicitCompositionClash` when an explicit Joinset's canonical form (`16 §5.7`) hashes to an `ImplicitId` already populated by step 7; (9) coverage / composition index materialization (`15 §6`, `16 §8`) — populates `CompositionIndex.entries` + `by_constituent_set` + `by_canonical` over the unified explicit + implicit population; (10) metadata finalization.

### 9.2 Argument discipline

- `**model: SemanticModel`** — by value; consumed. Callers that need to retain the model clone first.
- `**provider: &dyn CatalogProvider**` — shared reference over a trait object. Static dispatch via generic `<P: CatalogProvider>` was rejected because the trait has async methods (per `30 §9`), the trait boundary is the natural I/O-injection seam, and `dyn` prevents specialization-driven binary bloat. Compile-time dispatch cost is immaterial; the hot path is plan-time.
- `**fs: &dyn FileSystem**` — same discipline. `FileSystem::{list, read, exists}` are the only methods compile invokes.

### 9.3 Return shape

The signature above matches the fail-fast row in `30 §7.1`'s per-stage table: the success arm is `(SemanticManifest, Diagnostics<CompileError>)` (warnings collected during the pass), the failure arm is `(Diagnostic<CompileError>, Diagnostics<CompileError>)` (one fatal kind + warnings preceding it). The `Diagnostic<CompileError>` envelope carries the `Location` per `30 §5.1`; the kind variant carries semantic payload only. The `SemanticManifest::metadata.warnings` field (`§3.2`) is a content-equivalent copy of the success-arm warnings vector kept on the artifact for callers that drop the warnings tuple after consuming it.

### 9.4 Async boundary (I11a)

`compile` is `async fn` for exactly one reason: it awaits `CatalogProvider::fetch_schema`, `CatalogProvider::list_objects`, `FileSystem::{list, read, exists}`. These are the only `.await` points in the function body. Sub-passes (reference graph, cycle detection, binding resolution, expression resolution, index build) are all synchronous per `19 §3`'s I6 framing. Awaits happen once at the top during catalog snapshot; after that, compile runs to completion without yielding. This lets compile run on any runtime (`tokio`, `async-std`, `smol`) without pinning one.

### 9.5 Single-shot vs streaming

`compile` is single-shot in v1: one call, one `SemanticManifest`. The Model is consumed whole; no incremental compile API. I4's byte-stable SemanticManifest + content-addressable caching at the `SemanticManifestId` layer (`source_hash` is invariant under formatting-only changes per `13 §5`) covers most incremental workloads. Streaming / incremental compile is tracked as `[TD-33-INCREMENTAL-COMPILE]` (§17).

### 9.6 Warning discipline

Parse / validate warnings are not re-surfaced by `compile` — those are the caller's to route. Compile-stage warnings ride on the `Diagnostics<CompileError>` vector of either return arm (success or failure tuple) per `30 §7.2` and are also retained on `SemanticManifest.metadata.warnings` on success. No warning is silently dropped (per `30 §7.3`).

### 9.7 Thread safety

`SemanticManifest` is `Send + Sync`. `compile` can be called from any async task; `Arc<SemanticManifest>` is the conventional shared carrier. Caller-supplied `CatalogProvider` / `FileSystem` impls must themselves be `Send + Sync` per `37`.

---

## 10. `CompileError`

### 10.1 Typed-kind enum

The SemanticManifest-layer `CompileError` **owns** the wider resolution-stage roster (name resolution, catalog resolution, binding resolution, relationship / composition graph, type resolution, function-signature matching) and **embeds** the narrow `ir::CompileError` (`35 §16.2`) via D.ii kind-nesting (`30 §7.4`) for function-return-rule failures raised by `ReturnTypeRule::Custom` callbacks inside `semstrait-ir`'s `FunctionRegistry`. Per `30 §5`'s typed-kind discipline, the kind enum carries semantic payload only — primary `Location` lives on the wrapping `Diagnostic<CompileError>` envelope per `30 §5.1`.

```rust
use semstrait_common::diagnostic::{Diagnose, Severity};
use semstrait_common::{DataType, Location};
use semstrait_ir as ir;

#[non_exhaustive]
pub enum CompileError {
    // -- name resolution
    UnresolvedEntityRef            { name: String },
    UnreachableSemanticsReference  { name: String, from_kind: String },
    CircularSemanticsReference     { cycle: Vec<String> },
    UnresolvedColumn               { name: String, binding: String },
    UnresolvedCrossKindReference   { name: String, from_kind: String },

    // -- catalog / source resolution
    SourceNotFound                 { source: String },
    CatalogUnavailable             { detail: String },
    SchemaResolutionFailed         { source: String, reason: String },
    GlobExpansionFailed            { pattern: String, reason: String },

    // -- schema / binding resolution (per `15 §10`)
    BindingColumnNotInSchema       { binding: String, column: String },
    BindingCoverageConflict        { binding: String, semantics: String, reason: String },
    BindingLiteralTypeMismatch     { binding: String, semantics: String, declared: DataType, literal: String },
    BindingShapeMalformed          { binding: String, reason: String },
    PartitionColumnNotInSchema     { binding: String, column: String },

    // -- relationship / composition graph (per `16 §8`, `§10.4`–`§10.6`, `§14.1`)
    CircularRelationship           { cycle: Vec<RelationshipId> },
    IndexBuildFailed               { index: &'static str, reason: String },
    AmbiguousCompositionContributor { datakind: String, unified: String, contributors: Vec<String> },
    CompositionKeyMismatch         { datakind: String, relationship: RelationshipId, reason: String },
    /// Implicit-composition enumeration exceeded `MAX_IMPLICIT_ENUMERATION_COUNT`
    /// (`16 §10.4`, `COMP_E_0409`). Emitted before partial materialization is
    /// retained — compile fails fast.
    ImplicitEnumerationExploded    { enumerated: u32, cap: u32, hint_largest_kind: Option<String> },
    /// Author-declared explicit Joinset whose canonical form (`16 §5.7`) collides
    /// with an enumerable implicit Joinset (`16 §10.6`, `COMP_E_0414`). Carries
    /// both `DataKindName`s and the canonical `ImplicitId` for diagnostic
    /// rendering. Author must add a differentiator (extra constituent, narrowed
    /// `as_of`, distinct `keys` override) or remove the explicit Joinset.
    ExplicitImplicitCompositionClash {
        explicit: String,
        implicit_synthetic: String,
        canonical_id_hex: String,
    },

    // -- function resolution
    UnknownFunction                { name: String },
    FunctionArityMismatch          { name: String, expected: String, got: usize },
    NoMatchingSignature            { name: String, arg_types: Vec<DataType>, tried_signatures: Vec<String> },

    // -- semantic-kind mismatch (raised when SemanticLeaf typed leaves disagree on kind)
    SemanticKindMismatch           { name: String, expected: String, got: String },

    // -- type resolution
    TypeInferenceFailure           { reason: String },
    ComputedTypeUnifyConflict      { name: String, declared: DataType, inferred: DataType },
    LiteralOverflow                { value: String, target: DataType },
    LiteralPrecisionLoss           { value: String, target: DataType },

    // -- IR-emitted function-return-rule failure (D.ii nesting per `30 §7.4`).
    //    Surfaces `ReturnTypeRule::Custom` callback failures raised inside
    //    `semstrait-ir`'s `FunctionRegistry` resolution. Defined in `35 §16.2`.
    Ir(ir::CompileError),
}

impl Diagnose for CompileError {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}

impl From<ir::CompileError> for CompileError {
    fn from(e: ir::CompileError) -> Self { Self::Ir(e) }
}
```

### 10.2 Variant identity, not codes

Identification of a compile error is by **variant identity** (`matches!(diag.kind, CompileError::UnresolvedEntityRef { .. })`), not by a string code. The retired `COMP_E_`* / `EXPR_E_*` numeric ranges from earlier drafts of this section are gone — adding a variant inside `#[non_exhaustive]` is MINOR per `30 §2.2`; renaming or removing a variant is MAJOR per `30 §2.1`.

The single nested variant `Ir(ir::CompileError)` is the **only** D.ii embed at this layer. Downstream consumers pattern-match on the unified `manifest::CompileError` and either treat the `Ir(_)` arm uniformly (most diagnostic routing) or descend into the inner variant when finer-grained handling is needed.

### 10.3 Return-shape unification

There is no separate `CompileErrors` struct in the new diagnostic shape. The fail-fast return type defined in `30 §7.2` already carries one fatal `Diagnostic<CompileError>` plus the preceding warnings:

```rust
Result<
    (SemanticManifest, Diagnostics<CompileError>),       // Ok: artifact + warnings
    (Diagnostic<CompileError>, Diagnostics<CompileError>),  // Err: fatal + warnings
>
```

Callers destructure directly; no helper carrier is needed. The retired `CompileErrors` struct from earlier drafts is gone.

### 10.5 Warnings are never silently dropped

Warnings live on `manifest.metadata.warnings` (success arm) or in the failure tuple's `Diagnostics<CompileError>` second element (failure arm). Per `30 §7.3`, dropping warnings is an invariant violation, not a caller error.

---

## 11. `Repository` Trait

### 11.1 Surface

```rust
use semstrait_common::diagnostic::{Diagnostic, Diagnostics};

pub trait Repository: Send + Sync {
    async fn save(
        &self,
        manifest: &SemanticManifest,
    ) -> Result<
        (SemanticManifestId, Diagnostics<RepositoryErrorKind>),
        (Diagnostic<RepositoryErrorKind>, Diagnostics<RepositoryErrorKind>),
    >;

    async fn load(
        &self,
        id: SemanticManifestId,
    ) -> Result<
        (SemanticManifest, Diagnostics<RepositoryErrorKind>),
        (Diagnostic<RepositoryErrorKind>, Diagnostics<RepositoryErrorKind>),
    >;

    async fn list(
        &self,
    ) -> Result<
        (Vec<SemanticManifestId>, Diagnostics<RepositoryErrorKind>),
        (Diagnostic<RepositoryErrorKind>, Diagnostics<RepositoryErrorKind>),
    >;

    async fn delete(
        &self,
        id: SemanticManifestId,
    ) -> Result<
        Diagnostics<RepositoryErrorKind>,
        (Diagnostic<RepositoryErrorKind>, Diagnostics<RepositoryErrorKind>),
    >;
}
```

Persistence trait for `SemanticManifest`s. `load` is one of the two out-of-band I/O entries permitted outside compile (I11b); `save` / `list` / `delete` share the async posture for symmetry. All four methods are fail-fast per `30 §7.1`'s last data row (`Repository::{load,save}` and analogues). Implementations handle byte-level encoding, storage location, and content-addressable caching; the trait surface is encoding-independent. All four methods are `async fn` in trait per `30 §9`; the trait is **open** (third-party impls like S3-backed, GCS-backed, database-backed are expected per `30 §8.2`). `save` is idempotent (writing the same SemanticManifest twice is a no-op); `delete` of a missing id is `Ok(empty-warnings)`; `list` order is implementation-defined.

### 11.2 `RepositoryErrorKind`

```rust
use semstrait_common::diagnostic::{Diagnose, Severity};
use semstrait_common::io::IoErrorKind;

#[non_exhaustive]
pub enum RepositoryErrorKind {
    NotFound            { id: SemanticManifestId },
    IncompatibleFormat  { stored: String, expected: String },
    DecodeFailed        { id: SemanticManifestId, reason: String },
    EncodeFailed        { reason: String },
    /// Underlying transport failure (object-store / filesystem). Embeds
    /// the core `IoErrorKind` per `30 §5.6`'s cross-crate kind-nesting
    /// pattern, so transport-level identification is preserved without
    /// cloning variants.
    Transport           { source: IoErrorKind },
    IntegrityViolation  { reason: String },
}

impl Diagnose for RepositoryErrorKind {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepositoryErrorKind::Transport { source } => source.cause(),
            _ => None,
        }
    }
}

impl From<IoErrorKind> for RepositoryErrorKind { /* … */ }
```

`Transport` replaces the older `IoFailed { context: String }` shape with proper kind-nesting per `30 §5.6` — `IoErrorKind` carries variant identity for the transport layer; the wrapping diagnostic adds the repository-side context via the `notes: Vec<String>` field on `Diagnostic<RepositoryErrorKind>` per `30 §5.1`. `IntegrityViolation` guards against hand-crafted test inputs whose `SemanticManifestId` disagrees with content hash. Identification is by variant identity per `30 §5.4`; there is no string-code accessor.

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
    pub fn new(root: impl Into<PathBuf>, encoding: SemanticManifestEncoding) -> Self;
}

impl Repository for FileSystemRepository {}

#[non_exhaustive]
pub enum SemanticManifestEncoding {
    MessagePack,
    Json,
    // Bincode — reserved; enable-on-demand per `[TD-33-BINCODE]`.
}
```

Local-filesystem-backed `Repository`. File layout: `{root}/{manifest_id.as_hex()}.{ext}` where `ext` is `mpk` / `json` per encoding; a sibling `.meta.json` (containing the `SemanticManifestFormatVersion`) accompanies each primary file. In v1 encodings: `MessagePack` (compact, round-trips exactly) and `Json` (human-inspectable; slower / larger). `FileSystemRepository` is `Send + Sync`; concurrent saves of the same SemanticManifest resolve as idempotent no-ops.

## 12. `CatalogProvider::check_schema_drift`

Per I11b there are exactly two out-of-band I/O entries: `Repository::load` (§11) and `CatalogProvider::check_schema_drift`. The latter is a post-compile validation: given an existing `SemanticManifest`, ask the catalog whether the physical schemas that backed each `ResolvedPhysicalSource` have changed since compile. `yes` invalidates the SemanticManifest (recompile recommended); `no` confirms consistency. Not called from the compile pipeline; `semstrait-api` (`38`) exposes it via `Session::validate_manifest`.

Authoritative shape lives in `37`. The expected signature:

```rust
// in semstrait-catalog::CatalogProvider:
async fn check_schema_drift(&self, manifest: &SemanticManifest)
    -> Result<SchemaDriftReport, CatalogError>;
```

`SchemaDriftReport`'s variant roster (`NoDrift` / `ColumnAdded` / `ColumnRemoved` / `TypeChanged` / `SourceVanished`) lives in `37`. Drift is advisory; the caller decides whether to recompile.

**Why I11b-gated.** Performs I/O (catalog lookup against fresh metadata); not part of compile (it's called on an already-compiled SemanticManifest); not part of plan (plan consumes without re-checking). Packaging it as a distinct async, explicitly-gated entry matches I11b's two-entries-total discipline.

`**33`'s forward-ref posture.** No code is exposed here — the method is defined on `CatalogProvider` (owned by `37`). `33` only names the method in its authoritative-for list and documents that callers pass `&SemanticManifest` out of band. No struct / trait in `33` depends on `SchemaDriftReport` or `CatalogError`.

---

## 13. Determinism — I4 Uphold

### 13.1 Ordered-map everywhere

Per `00 §9 I4`: byte-identical SemanticManifests for byte-identical inputs. Every key-keyed collection is a `BTreeMap`, not a `HashMap`:

- `SemanticManifest.resolved_datakinds` / `resolved_relationships`.
- `CoverageIndex.entries` / `CompositionIndex.{entries, by_constituent_set, by_canonical}`.
- `ResolvedColumnMapping.{columns, literals, computed, metadata, source_coverage}`.
- `ResolvedExprTable.entries` (per `19 §3.2.1`).
- `ResolvedRelationshipGraph.{kinds, by_kind}`.

`BTreeMap` ordering is a pure function of the keys' `Ord` impls — no insertion-order dependence. `IndexMap` would force sub-passes to produce specific insertion sequences; `BTreeMap` absorbs the ordering at the container level. Ordered `Vec` fields (e.g. `ResolvedBinding.sources`, `ResolvedUnionset.branches`) preserve author-declared order, deterministic per the Model's parse order (`11 §11.3`).

### 13.2 Timestamp canonicalization

`SemanticManifest.metadata.compiled_at` is wall-clock UTC and not stable across compiles. To keep I4 intact, the content hash used for `SemanticManifestId` derivation excludes the timestamp:

```
source_hash = hash(canonicalize(SemanticModel) || canonicalize(catalog_snapshot))
```

where `canonicalize` is a byte-stable form of each input (ratified in `32` for the Model and `37` for the catalog). Serialized SemanticManifest bytes via `Repository::save` do include the timestamp and therefore are **not** byte-identical across compiles — the determinism guarantee is specifically over `SemanticManifestId` and the Resolved* content-bearing fields. Tests that compare SemanticManifest bytes exactly must use `SemanticManifest::canonical_bytes()` (§14.3).

### 13.3 Testing the I4 invariant

Per `30 §11.4`, CI fixtures cover: (1) a fixture set of Model YAMLs + catalog-snapshot mocks; (2) per fixture: run `compile` twice, assert `manifest.canonical_bytes()` is identical; (3) per fixture: run `compile` with a delay between invocations, assert `SemanticManifestId::from_manifest(&m)` is identical; (4) per fixture: run `compile` on different machines / architectures, assert `canonical_bytes()` is identical (catches endianness-dependent hashes). (2) is per-crate; (3)–(4) are workspace CI.

### 13.4 Determinism across algorithm changes

A compile-internal algorithm change that preserves every `canonical_bytes()` output is PATCH per `30 §11.4`. An equivalent-but-not-byte-identical change (e.g. index-key renaming, `Vec` reordering) is MINOR and requires a `42` migration note. The PATCH/MINOR boundary is drawn at the byte level.

---

## 14. Serde / Persistence Format

### 14.1 Serde posture

Per `30 §10.4`, `serde` support is opt-in via a `serde` feature. `semstrait-manifest`:

- Default-off in v1.
- `serde` feature enables `Serialize` / `Deserialize` on every public type in §2 (`SemanticManifest`, every `Resolved`*, `CoverageIndex`, `CompositionIndex`, `SemanticManifestMetadata`, `SemanticManifestId`, `SemanticManifestFormatVersion`) and transitively enables `semstrait-common`'s `serde` feature.
- `FileSystemRepository` requires `serde`; `InMemoryRepository` does not.

### 14.2 Format choice is `Repository`-selectable

Byte-level encoding is a `FileSystemRepository`-construction choice via `SemanticManifestEncoding`. Third-party `Repository` impls pick their own (bincode, capnp, database-columnar). The SemanticManifest **shape** is serde-derived and stable; the SemanticManifest **wire format** is encoder-dependent. v1 bundled encodings: **MessagePack** (compact, exact round-trip; recommended default), **JSON** (human-inspectable, slower, larger; debugging / `--explain`). **Bincode** reserved per `[TD-33-BINCODE]` — not exposed in v1 because its non-self-describing form makes schema migrations painful.

### 14.3 `canonical_bytes()`

```rust
impl SemanticManifest {
    pub fn canonical_bytes(&self) -> Vec<u8>;
}
```

Canonical byte form for equality comparison and content-addressable hashing. Excludes `metadata.compiled_at` per §13.2. Encoding: bincode with sorted-field emission, no version stamp, no length prefix. Not a wire format; `Repository` impls use their chosen encoder. `#[cfg(feature = "serde")]`-gated. Two SemanticManifests have `canonical_bytes()` equal iff their content fields are pairwise-equal under §13's ordering rules.

### 14.4 Format-version policy revisited

`SemanticManifestFormatVersion` is `#[non_exhaustive]` starting at `V1` (§3.2). Discriminator bumps — each of which requires a `42_migration_notes.md` entry — are triggered by: removing a field from any `#[non_exhaustive]` struct; reordering fields in a way that changes serde's default key emission; renaming a variant on a persisted enum; changing the serde representation (e.g. tagged → untagged). Additive growth stays at the current discriminator per `30 §2.2`.

On `Repository::load`, impls MUST check the stored discriminator against the running crate's max; mismatches surface as `Diagnostic<RepositoryErrorKind>` whose kind is `IncompatibleFormat { stored, expected }` (§11.2). Forward-compatible reads (newer stored on older crate) are NOT supported in v1 — a MINOR bump is still a break in the load direction.

### 14.5 What `serde` does NOT gate

Public struct layout (field visibility, method signatures, non-serde trait impls) is identical with or without the feature. Determinism (ordered-map invariants) holds regardless. Default-off is an opt-in for consumers who treat `SemanticManifest` as in-memory-only (e.g. via `InMemoryRepository`).

---

## 15. Stability

### 15.1 Crate tier

Per `30 §13`: `semstrait-manifest` is **Stable in v1**. The `SemanticManifest`, `Resolved`* family, `CompileError`, `Repository` trait, and `RepositoryErrorKind` are all ratified in this document and carry the workspace-wide MAJOR cadence discipline.

Pre-1.0 rules apply until the synchronized v1.0 cut per `30 §2.3`.

### 15.2 MAJOR cases

Per `30 §2.1`, each of the following is MAJOR:

- Removing a variant from any `ResolvedDataKind::*` / `ResolvedComplexDataKind::*` / `CompileError::*` / `RepositoryErrorKind::*` enum.
- Renaming a variant of any of those enums (variant identity is the public-API surface per `30 §5.4`).
- Changing the type or meaning of any existing public field on `SemanticManifest`, `ResolvedSimpleDataKind`, `ResolvedBinding`, `ResolvedColumnMapping`, `ResolvedRelationship`, `CoverageIndex`, `CompositionIndex`, `SemanticManifestMetadata`, or any enum variant.
- Changing the `compile` function signature (adding a required argument, changing return type).
- Bumping `SemanticManifestFormatVersion` when the new version rejects v1-stored SemanticManifests (a load-direction break).
- Removing the `Repository` trait, renaming it, or changing any of its method signatures in a way that breaks existing impls.
- Changing `canonical_bytes()`'s encoding to produce different bytes for existing SemanticManifests (callers using it for content-addressable caching observe a cache miss).

### 15.3 MINOR cases

Per `30 §2.2`, additive changes:

- Adding a new variant to `ResolvedComplexDataKind` (future `Snapshotset`, `Windowset`, etc.) — the outer `ResolvedDataKind` is `#[non_exhaustive]`.
- Adding a new field to `SemanticManifest` or any `Resolved*` struct (all are `#[non_exhaustive]`).
- Adding a new variant to `CompileError` or `RepositoryErrorKind` (e.g. when a new validation category or transport class lands).
- Adding a new field to `SemanticManifestMetadata` (e.g. a hash of the function-registry extensions).
- Adding a new `SemanticManifestEncoding` variant.
- Adding a new method to `Repository` that carries a `provided` default body (e.g. a batch-load variant). Adding a method without a default is MAJOR.
- Adding a new public free function or type in §2's roster.

### 15.4 PATCH cases

Per `30 §2.1`:

- Internal algorithm improvements that preserve `canonical_bytes()` for every test fixture.
- Doc-comment corrections.
- Improvements to per-variant `Diagnose::message()` rendering that preserve variant identity.
- Dependency bumps that do not change public types.

### 15.5 Deprecation policy

Per `30 §12`: any symbol slated for removal passes through `#[deprecated]` for at least one MINOR cycle. The `41_deprecations.md` file tracks each deprecation. v1 introduces no deprecations in this crate.

---

## 16. Crate Boundaries

### 16.1 What `semstrait-manifest` does NOT contain

- **No planner code.** `SemanticPlan`, `PlanError`, `Request`, `SessionContext` live in `semstrait-planner`. The `PlanNode` container is **defined** in `semstrait-ir` (`35 §7` / `§7`) but populated by `semstrait-planner` — `33` neither defines it nor builds it.
- **No adapter code.** `EngineAdapter`, `DialectId`, `AdaptError`, `EngineArtifact` live in `semstrait-adapter`. The SemanticManifest carries only the engine-agnostic `DataType` (from `semstrait-common` per `13 §2`) and `PhysicalExpr` (from `semstrait-ir` per `35 §3.6`, sourced via `14 §2` ratification).
- **No catalog I/O logic.** Catalog / filesystem trait methods are consumed by `compile` but provided by `semstrait-catalog`. No catalog impl is bundled here.
- **No raw SQL.** `ResolvedPhysicalSource` is engine-agnostic; dialect rendering is `semstrait-adapter`'s job.
- **No YAML parser.** `SemanticModel` arrives parsed; parsing lives in `semstrait-model`.
- **No validation logic.** `validate` completes before `compile`; the input `SemanticModel` is structurally sound on entry.

### 16.2 What `semstrait-manifest` DOES contain

The `compile` function and its sub-passes; the `SemanticManifest` struct and every `Resolved`* type; the `CompileError` and `RepositoryErrorKind` typed-kind enums (each with `Diagnose` impls); the `Repository` trait and the two bundled impls; the convenience `::io` submodule (§16.5); determinism discipline (BTreeMap everywhere, timestamp canonicalization, `canonical_bytes()`).

### 16.3 Dependency direction

Depends on exactly four workspace crates per the second-cascade landing (`STATUS.md` item Q):

- `semstrait-common` — for `DataType`, `Schema`, `Diagnostic<K>` / `Diagnose`, the constraint DSL, `IoErrorKind`, and the `io` transport traits from `31b`.
- `semstrait-ir` — for `Expr<L>`, `PhysicalExpr`, `SemanticExpr`, the trait family (`Tree`, `Visitor`, `Rewriter`, `ExprLeaf`), the structural-variant support enums (`BinaryOpKind`, …), the identifier carriers (`ColumnRef`, `SemanticsName`), `CanonicalFn`, `FunctionRegistry`, `ir::CompileError` (embedded in §10 via D.ii), and the `PlanNode` container that the downstream planner populates.
- `semstrait-model` — for `SemanticModel`, `ExprSource`, and Model-layer names.
- `semstrait-catalog` — for the `CatalogProvider` / `FileSystem` trait surfaces and `Schema` / `PartitionColumn`.

Per I7, no upward dep on `semstrait-planner`, `-adapter`, `-api`, or `-facade`.

### 16.4 Async boundary discipline

Three async surfaces cross this crate's public boundary: `compile` (I11a), `Repository::{save, load, list, delete}` (I11b), and the `::io` convenience wrappers (§16.5, composing `31b` transport). Everything else (accessors, iterators, lookups) is synchronous. The boundary is enforceable by a doc-comment discipline (every `async fn` MUST carry an I11 justification) and a CI audit (tracked as `[TD-33-CLIPPY-ASYNC-GUARD]`).

### 16.5 SemanticManifest-level I/O convenience wrappers (`semstrait-manifest::io`)

A small feature-gated submodule exposes one-shot load / dump helpers that compose `semstrait-common::io` (`31b`) with manifest byte-level encoding, for callers that want single-function ergonomics rather than constructing a full `Repository`:

```rust
use semstrait_common::io::{Source, Sink, IoErrorKind};
use semstrait_common::diagnostic::{Diagnose, Diagnostic, Diagnostics, Severity};

pub mod io {
    pub async fn load_manifest<S: Source + ?Sized>(
        src: &S,
        encoding: SemanticManifestEncoding,
    ) -> Result<
        (SemanticManifest, Diagnostics<SemanticManifestLoadErrorKind>),
        (Diagnostic<SemanticManifestLoadErrorKind>, Diagnostics<SemanticManifestLoadErrorKind>),
    >;

    pub async fn dump_manifest<S: Sink + ?Sized>(
        m: &SemanticManifest,
        sink: &S,
        encoding: SemanticManifestEncoding,
    ) -> Result<
        Diagnostics<SemanticManifestDumpErrorKind>,
        (Diagnostic<SemanticManifestDumpErrorKind>, Diagnostics<SemanticManifestDumpErrorKind>),
    >;

    #[non_exhaustive]
    pub enum SemanticManifestLoadErrorKind {
        Io(IoErrorKind),
        Decode { encoding: SemanticManifestEncoding, reason: String },
        FormatVersion { found: SemanticManifestFormatVersion, expected: SemanticManifestFormatVersion },
    }

    #[non_exhaustive]
    pub enum SemanticManifestDumpErrorKind {
        Io(IoErrorKind),
        Encode { encoding: SemanticManifestEncoding, reason: String },
    }

    impl Diagnose for SemanticManifestLoadErrorKind { /* delegates */ }
    impl Diagnose for SemanticManifestDumpErrorKind { /* delegates */ }

    impl From<IoErrorKind> for SemanticManifestLoadErrorKind { /* … */ }
    impl From<IoErrorKind> for SemanticManifestDumpErrorKind { /* … */ }
}
```

**Binary transport.** The manifest is a binary artifact (MessagePack by default, JSON as the human-inspectable alternative, both carried at byte level). `load_manifest` calls `src.read_raw().await?` (returning `Bytes`) and hands the result to the encoding's decoder; `dump_manifest` encodes to `Bytes` and calls `sink.write_raw(bytes).await`. Unlike the model wrappers (`32 §10.4`), there is no UTF-8 validation step — manifest bytes are not required to be valid UTF-8 and are never materialized as a `String`.

**Relationship to `Repository`.** `Repository` is the full-fat persistence contract with content-addressable IDs (`SemanticManifestId`), sibling `.meta.json` files, and format-version checks. `manifest::io` is the lightweight "I have a `Source` pointing at manifest bytes; give me a `SemanticManifest`" path. A `FileSystemRepository` (or future `S3Repository`) internally uses the same `core::io` transport via the `object_store`-backed back-ends (`31b §8`); callers that only need one-shot load / dump skip the Repository machinery entirely.

**Fused-kind composition.** Both `SemanticManifestLoadErrorKind` and `SemanticManifestDumpErrorKind` follow the cross-crate kind-nesting pattern from `30 §5.6`: the `Io` variant embeds `IoErrorKind` directly so transport-level identification is preserved without cloning variants. Because `IoErrorKind` itself is `#[non_exhaustive]` (`31b §7`), adding an `IoErrorKind` variant propagates as a MINOR through this layer per `30 §4.4`'s match-discipline rule. Identification is by variant identity per `30 §5.4`; there is no string-code accessor.

**Feature flag.** Gated behind `manifest`'s `io` feature (default off), which forwards to `semstrait-common/io`. `aws` feature forwards to `semstrait-common/io-aws`.

**Migration note.** Pre-`31b` the manifest crate shipped a `load_text` helper for loading YAML *model* text. Under the ratified layout that utility is superseded by `semstrait-common::io` + `semstrait-model::io::load_model` (`32 §10.4`). Removal of `semstrait-manifest::io::load_text` is the closing step of `TD-008`.

---

