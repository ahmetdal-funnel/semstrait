---
prereqs: [11, 13, 14, 15, 16, 17, 18, 19, 20, 30, 31, 35, 37]
authoritative-for:
  - the `semstrait-manifest` public API surface and crate boundary
  - the lightweight, model-aligned `SemanticManifest` contract used as planner input
  - the top-level `SemanticBitmap` registry and per-DataKind / per-constituent `SemanticBitmask` shape
  - the manifest-resident `DataKind` primitive and the closed `DataKindVariant { Dataset, Unionset, Grainset, Joinset }` taxonomy mirror
  - the `NestedDataKind` shape used inside Unionset / Grainset / Joinset variants
  - manifest persistence shape for `SemanticInterface`, `SemanticBinding`, `Relationship` (top-level, scope-root only)
  - manifest expression persistence contract — split typed pools `ManifestExpressions { semantic, physical }` with strongly-typed `SemanticExprId` / `PhysicalExprId` newtypes
  - `PhysicalSource` manifest contract (locator + version reference + inline `projected_schema` + SHA-256 `schema_fingerprint`); reduced `PhysicalSourceType { Table, File }` and `PhysicalSourceVersionRef { IcebergSnapshotId, MonotonicVersion }` rosters
  - compile and persistence boundary (`compile`, `Repository`); compile-time validation gates G1–G5 and the D2 desugar-at-compile policy
  - load-time integrity contract (CX1 single-pass cross-reference resolution)
  - canonical encoding rules used by `manifest_epoch` / `model_hash` / `catalog_fingerprint` and per-source `schema_fingerprint`
  - naming policy for `Resolved*` forms
refined-by:
  - 34 (`semstrait-planner` consumes manifest and constructs runtime `SemanticGraph`; planner-runtime details remain TODO/provisional)
  - 35 (`semstrait-ir` owns canonical `Expr<L>` family, leaves, and `Serialize`/`Deserialize` derives that manifest consumes)
  - 36 (`semstrait-adapter` consumes `SemanticPlan` output)
  - 37 (`semstrait-catalog` owns provider traits used during compile-time metadata fetch and drift probing)
---

# 33. semstrait-manifest

## 1. Purpose and Scope

`semstrait-manifest` owns:

1. `compile`: `SemanticModel + (Option<&dyn CatalogProvider>) + filesystem -> SemanticManifest`.
2. The persisted manifest artifact (loading and saving via `Repository`).
3. The lightweight, model-aligned manifest type system — identity primitives on disk, structure rebuilt in memory by the planner runtime.

The artifact is intentionally lightweight and human-readable. It carries semantic identity primitives (the `SemanticBitmap` registry, `DataKind` entries, `SemanticInterface`, `SemanticBinding`, `Relationship`, `PhysicalSource`, expression pools) and the local coverage masks needed for first-cut path search. It does **not** persist a runtime graph, planner cache state, or per-Request derived structures.

This shape lands the 20 ratified clauses C1–C18 + CCK + CX1 (see `_research/manifest/RATIFICATION_LOG.md`).

## 2. Design Posture

- **Identity on disk, structure in memory.** Every surveyed system with an on-disk artifact (dbt, Substrait) puts identity primitives on disk and rebuilds structure in memory. Manifest persists primitives; `SemanticGraph` reconstructs nodes / edges / per-Request structures at build time.
- **Closed taxonomy mirror.** `DataKindVariant` enumerates the four ratified DataKinds (`Dataset`, `Unionset`, `Grainset`, `Joinset`) and is `#[non_exhaustive]` per I10.
- **Bitmap vs Bitmask vocabulary.**
  - **`SemanticBitmap`** = the registry, the canonical map, single, top-level. Holds full `SemanticDefinition` per entry.
  - **`*Bitmask`** = a computed mask value over the bitmap (a coverage view), held on local entities. Suffix only; never standalone.
- **Public vs Nested DataKind split** mirrors spec 20: top-level `DataKind` carries an id; `NestedDataKind` inlined into a parent variant carries a `structural_label` per spec 26 addressing.
- **Implicit Unionsets are top-level.** Multi-source `Dataset` auto-synthesises a Unionset (spec 21 §3.2 + spec 23 §2.1 row A) with a content-derived hash `data_kind_id`. They live in `data_kinds`, not inside `NestedDataKind`.
- **Catalog-optional.** `compile` accepts `Option<&dyn CatalogProvider>`. When absent, model is the source of truth for physical shape (Thread A; foundations cascade in spec 14/15).
- **No runtime graph persistence.** Runtime graph fragments and cache policy are planner runtime concerns (spec 34, TODO/provisional).
- **IR owns canonical expression vocabulary.** Manifest persists IR-owned `Expr<L>` (`SemanticExpr` and `PhysicalExpr`), with `Serialize` / `Deserialize` derived inside `semstrait-ir` (spec 35).
- **Resolved naming policy.** Prefix `Resolved*` only for compile-complete forms; do not use it for author-level or deferred-runtime forms.

## 3. Public Crate Surface

```text
semstrait-manifest
├── manifest
│   ├── bitmap        // SemanticBitmap, SemanticDefinition, SemanticBitmask
│   ├── interface     // SemanticInterface, SemanticInterfaceBitmask
│   ├── data_kind     // DataKind, DataKindVariant, NestedDataKind, NestedDataKindVariant,
│   │                 //   DataKindRole, DataKindOrigin
│   ├── unionset      // UnionsetBranch (used inside DataKindVariant::Unionset)
│   ├── grainset      // GrainsetLevel, RoutingUnitRef
│   ├── joinset       // JoinsetHop, PathOrigin, HopDirection
│   ├── binding       // SemanticBinding, SemanticMappingValue
│   ├── relationship  // Relationship (re-export of canonical entity from spec 18)
│   ├── expr          // ManifestExpressions, SemanticExprId, PhysicalExprId
│   ├── source        // PhysicalSource, PhysicalSourceType,
│   │                 //   PhysicalSourceVersionRef, SourceColumn
│   └── metadata      // SemanticManifestMetadata
├── compiler          // compile entry + pass orchestration
├── error             // CompileError, LoadError, Diagnose
└── repository        // Repository + implementations
```

Primary root exports:

- `SemanticManifest`, `SemanticManifestMetadata`
- `SemanticBitmap`, `SemanticDefinition`, `SemanticBitmask`
- `SemanticInterface`, `SemanticInterfaceBitmask`
- `DataKind`, `DataKindVariant`, `NestedDataKind`, `NestedDataKindVariant`, `DataKindRole`, `DataKindOrigin`
- `UnionsetBranch`
- `GrainsetLevel`, `RoutingUnitRef`
- `JoinsetHop`, `PathOrigin`, `HopDirection`
- `SemanticBinding`, `SemanticMappingValue`
- `ManifestExpressions`, `SemanticExprId`, `PhysicalExprId`
- `PhysicalSource`, `PhysicalSourceType`, `PhysicalSourceVersionRef`, `SourceColumn`
- `compile`, `Repository`
- `CompileError`, `LoadError`

## 4. `SemanticManifest` Contract

### 4.1 Top-level shape

```rust
#[non_exhaustive]
pub struct SemanticManifest {
    pub manifest_epoch: u64,
    pub model_hash: [u8; 32],
    pub catalog_fingerprint: Option<[u8; 32]>,

    pub semantics:     SemanticBitmap,                                 // C4
    pub interfaces:    BTreeMap<SemanticInterfaceId, SemanticInterface>,
    pub data_kinds:    BTreeMap<DataKindId, DataKind>,                 // C5–C9 + CCK
    pub bindings:      BTreeMap<BindingId, SemanticBinding>,           // C2
    pub sources:       BTreeMap<SourceId, PhysicalSource>,             // C1, C3
    pub expressions:   ManifestExpressions,                            // C11, C12, C18
    pub relationships: BTreeMap<RelationshipId, Relationship>,         // root-scope only

    pub metadata:      SemanticManifestMetadata,
}
```

Rationale (per C9 / C17(d)):

- Identity primitives only — no `nodes`, no `edges`, no `compositions`. `SemanticGraph` reconstructs nodes, edges, and per-Request composition structures at build time.
- All entity collections are stable-id keyed `BTreeMap`s for deterministic ordering and direct lookup.
- Vectors remain inside entities where ordered lists are meaningful (`branches`, `levels`, `hops`, `bindings`, etc.).
- `relationships` holds **root-scope** Relationships only. Joinset-local (shadow) Relationships per spec 18 §2.10 live inline on the Joinset variant, not here (C7.6).

### 4.2 Top-level fingerprints

| Field | Width | Source |
|---|---|---|
| `manifest_epoch` | `u64` | Monotonic; bumps when canonical encoding of the manifest changes. |
| `model_hash` | `[u8; 32]` | SHA-256 over the source `SemanticModel`'s canonical encoding. |
| `catalog_fingerprint` | `Option<[u8; 32]>` | SHA-256 over fetched-content of catalog probe results (C14.6); `None` when catalog is absent. |

`catalog_fingerprint` is a fetched-content hash, not an identity-only hash. Provider-internal drift therefore propagates into `manifest_epoch` via canonical encoding (C18.2).

### 4.3 `SemanticManifestMetadata`

```rust
#[non_exhaustive]
pub struct SemanticManifestMetadata {
    pub created_at: SystemTime,
    pub semstrait_version: String,
    pub source_model_uri: Option<String>,
    pub annotations: BTreeMap<String, String>,
}
```

Free-form provenance for tooling; never load-bearing.

## 5. `SemanticBitmap` Registry (C4)

The `SemanticBitmap` is the canonical, single, top-level registry of every `SemanticsId` reachable from the model. All per-entity `*Bitmask` values are coverage views over this registry.

```rust
#[non_exhaustive]
pub struct SemanticBitmap {
    pub entries: BTreeMap<SemanticsId, SemanticDefinition>,
}

#[non_exhaustive]
pub struct SemanticDefinition {
    pub semantic_id: SemanticsId,
    pub name: SemanticsName,
    pub role: SemanticRole,
    pub data_type: Option<DataType>,
    pub bit_position: u32,
}

#[non_exhaustive]
pub enum SemanticRole {
    Dimension,
    Measure,
    Metric,
    Key,
    Field,
}

#[non_exhaustive]
pub struct SemanticBitmask {
    pub words: Vec<u64>,
}
```

Rules:

- **Scope is global.** A bit position spans all `SemanticsId`s in the manifest's set.
- **Wide registry.** Each entry carries the full `SemanticDefinition` (per-semantic name, role, optional `DataType`, and resolved `bit_position`). Per-semantic attributes that previously lived in a `SemanticNodePayload::Semantic` wrapper now live here.
- **Epoch-stable; cross-epoch renumber allowed.** Within one `manifest_epoch`, `bit_position` is stable. Bumping `manifest_epoch` rebuilds positions from canonical sort over `SemanticsId`. Position lookup at any read site is `bitmap.entries.get(id).bit_position`.
- **Bitmask encoding** (CCK.5): `Vec<u64>` words; bit `n` of word `n / 64` shifted by `n % 64` corresponds to the entry whose `bit_position == n`.

`SemanticInterfaceBitmask` (the per-interface mask projection) is structurally a `SemanticBitmask` and lives on `SemanticInterface`.

```rust
#[non_exhaustive]
pub struct SemanticInterface {
    pub dimensions: Vec<SemanticsId>,
    pub measures:   Vec<SemanticsId>,
    pub metrics:    Vec<SemanticsId>,
    pub keys:       Vec<SemanticsId>,
    pub fields:     Vec<SemanticsId>,
    pub bitmask:    SemanticInterfaceBitmask,
}

pub type SemanticInterfaceBitmask = SemanticBitmask;
```

The vector-of-ids fields preserve declared shape and ordering for human inspection; `bitmask` is the membership view used by graph-build path search.

## 6. `DataKind` and `DataKindVariant` (CCK + C5–C9)

### 6.1 Top-level `DataKind`

```rust
#[non_exhaustive]
pub struct DataKind {
    pub data_kind_id: DataKindId,
    pub name: DataKindName,
    pub role: DataKindRole,
    pub origin: DataKindOrigin,
    pub coverage: SemanticBitmask,
    pub variant: DataKindVariant,
}

#[non_exhaustive]
pub enum DataKindRole {
    Dataset,
    Unionset,
    Grainset,
    Joinset,
}

#[non_exhaustive]
pub enum DataKindOrigin {
    Explicit,
    Implicit,
}
```

Notes:

- `coverage` is the universal union view (CCK.1) — every DataKind carries it; satisfies spec 20 invariant D4.
- `role` is a flat tag mirroring `DataKindVariant`'s discriminant; useful for indexing without pattern-matching on the variant.
- `origin` distinguishes author-declared (`Explicit`) from compile-synthesised entries such as multi-source-Dataset auto-Unionsets (`Implicit`). Diagnostic-only — runtime semantics are identical (C9.5).

### 6.2 `DataKindVariant`

```rust
#[non_exhaustive]
pub enum DataKindVariant {
    Dataset {
        bindings: Vec<BindingId>,                                      // len >= 1
    },
    Unionset {
        mode: UnionMode,
        branches: Vec<UnionsetBranch>,                                 // len >= 2
    },
    Grainset {
        levels: Vec<GrainsetLevel>,                                    // len >= 2
    },
    Joinset {
        anchor: DataKindId,
        members: Vec<DataKindId>,                                      // anchor in members; len == 2 (binary v1)
        hops: Vec<JoinsetHop>,                                         // len == 1 (binary v1); cumulative
        path_origin: PathOrigin,
        scope_local_relationships: Vec<Relationship>,                  // §2.10 shadow
    },
}
```

### 6.3 `NestedDataKind`

Mirrors spec 20's Public/Nested split. Inlined into a parent variant — no separate id, just a structural address (spec 26 §4) and a coverage view.

```rust
#[non_exhaustive]
pub struct NestedDataKind {
    pub structural_label: String,
    pub coverage: SemanticBitmask,
    pub variant: NestedDataKindVariant,
}

#[non_exhaustive]
pub enum NestedDataKindVariant {
    Dataset {
        bindings: Vec<BindingId>,
    },
    Unionset {
        mode: UnionMode,
        branches: Vec<UnionsetBranch>,
    },
    Grainset {
        levels: Vec<GrainsetLevel>,
    },
}
```

`Joinset` is intentionally absent from `NestedDataKindVariant`: per spec 26 R2/R3 a Joinset never nests under another Joinset, and other parents project to top-level `data_kinds` entries by id.

### 6.4 `Dataset` variant (C5)

```rust
DataKindVariant::Dataset {
    bindings: Vec<BindingId>,                      // len >= 1
}
```

Rules:

- **Coverage origin.** Top-level `coverage` is the union of bindings' covered semantics. Only `Native` and `Derived` mapping coverage contribute bits (C5.1) — `NullFill` / `Metadata` mapping kinds are excluded (they don't represent real source-side coverage).
- **Cardinality.** `bindings.len() >= 1`. Empty is a compile error (cascade from spec 20 §3 and spec 15 §2.1: a leaf Dataset carries at least one Binding).
- **No special orphan-bit rule** — unresolved semantics are caught earlier by G2 (C13).

### 6.5 `Unionset` variant (C6)

```rust
DataKindVariant::Unionset {
    mode: UnionMode,
    branches: Vec<UnionsetBranch>,                 // len >= 2; declaration order
}

#[non_exhaustive]
pub struct UnionsetBranch {
    pub kind: NestedDataKind,
    pub branch_coverage: SemanticBitmask,
}
```

Rules:

- `mode` is the `UnionMode { All, Unique }` roster ratified for v1 (spec 23 §2.1).
- `branches.len() >= 2` per spec 26 R3.
- Vector preserves YAML declaration order; reordering is a real model edit.
- `branch_coverage` records each branch's locally-covered semantics (Native/Derived bits per C5.1 cascade). The complement `top_level_coverage \ branch_coverage` is the implicit NullFill mask, derived at graph-build (spec 23 §1.3 I1) — the manifest does not persist it (C6.2).
- **Implicit Unionset placement.** Multi-source `Dataset` auto-synthesises a Unionset per spec 21 §3.2; the result is a top-level entry in `data_kinds` with `origin = Implicit` and a content-derived hash `data_kind_id` (spec 23 §2.1 row A). It is **not** a `NestedDataKind`.

### 6.6 `Grainset` variant (C8)

```rust
DataKindVariant::Grainset {
    levels: Vec<GrainsetLevel>,                    // len >= 2; coarsest-first; distinct grains
}

#[non_exhaustive]
pub struct GrainsetLevel {
    pub grain: Grain,
    pub routing_unit: RoutingUnitRef,
    pub level_coverage: SemanticBitmask,
}

#[non_exhaustive]
pub enum RoutingUnitRef {
    Inline(NestedDataKind),                        // single same-grain child
    Synthesized(DataKindId),                       // implicit-Unionset top-level (>= 2 same-grain children)
}
```

Rules:

- `levels.len() >= 2` (>= 2 unique grains per spec 22 §5.2).
- Each `level.grain` is distinct.
- Coarsest-first level ordering (spec 22 §5.2 / spec 12 §4.2).
- `RoutingUnitRef::Inline` is used when a level has a single same-grain child; `RoutingUnitRef::Synthesized` references a top-level implicit Unionset when a level has two or more same-grain children (spec 22 §3.3 same-grain pre-merge + spec 23 §2.1 row A).
- **Cross-grain JOIN-tree is not persisted** — only per-level `level_coverage` and Keys (already in the level's interface). `SemanticGraph` synthesises the JOIN-tree at build (C8.2). Cascade: spec 22 §1.3 I8 must drop "JOIN-tree shape, per-pair JOIN-key index, ComposedSemanticInterface" from `ResolvedGrainset`'s manifest contract.

### 6.7 `Joinset` variant (C7)

```rust
DataKindVariant::Joinset {
    anchor: DataKindId,
    members: Vec<DataKindId>,                      // len == 2 (binary v1); anchor in members
    hops: Vec<JoinsetHop>,                         // len == 1 (binary v1); cumulative coverage
    path_origin: PathOrigin,
    scope_local_relationships: Vec<Relationship>,  // §2.10 shadow; bounded to this Joinset
}

#[non_exhaustive]
pub struct JoinsetHop {
    pub from: DataKindId,
    pub to: DataKindId,
    pub relationship: RelationshipId,
    pub direction: HopDirection,
    pub hop_coverage: SemanticBitmask,             // cumulative across hops [0..=i]
}

#[non_exhaustive]
pub enum PathOrigin { Explicit, Implicit }

#[non_exhaustive]
pub enum HopDirection { Forward, Reverse }
```

Rules:

- **Cumulative coverage semantics** (C7.2). `hops[i].hop_coverage` = union of semantics reachable starting at `anchor` and walking hops `[0..=i]`. SemanticGraph's first-cut path search reads `requested_mask & hops[i].hop_coverage == requested_mask` directly.
- **Binary v1 invariants** (C7.5). `members.len() == 2`, `hops.len() == 1`. Surfaced at load via CX1; defense-in-depth twin of canonical compile errors `VALID_E_2400` / `COMP_E_2408`.
- **Anchor invariant** (C7.7). `anchor in members`. Defense-in-depth twin of canonical `VALID_E_2402`.
- **Top-level `coverage` of a Joinset DataKind** equals `hops.last().hop_coverage` (the last cumulative hop is the full Joinset coverage).
- **`path_origin`.** Diagnostic / audit tag distinguishing explicit-author vs implicit-search-derived hop sequences (spec 24 §I5). Both resolve to identical `Vec<JoinsetHop>` shape.
- **Scope-local Relationships persisted inline** (C7.6). Joinset-bounded shadow Relationships per spec 18 §2.10 live on the variant; root-scope `relationships` only holds non-shadow Relationships. This avoids a `relationship.scope` discriminator and keeps shadow visibility cleanly bounded.
- **No `ComposedSemanticInterface`** (C7.4). Manifest carries hops + per-hop coverage; `SemanticGraph` synthesises `UnifiedSemantics` and `FieldProvenance` at build. Cascade: spec 24 §1.4 I8 must drop "resolved ComposedSemanticInterface" from `ResolvedJoinset`'s manifest contract; §2.4 must retire the `interface: ComposedSemanticInterface { … }` pseudo-shape.

### 6.8 Implicit composition (C9)

`compositions:` is **not** a top-level field (C9.2). The `SemanticGraph` runs BFS at build time over `relationships` and per-DataKind `coverage` to resolve a `CompositionKind::Relationship` request (spec 16 §11). Cascade: spec 16 §10.4's "compile-time-enumerated compositions" framing must be aligned to the lightweight posture — that index lives in `SemanticGraph`, not in manifest.

`MAX_IMPLICIT_COMPOSITION_DEPTH = 4` (C10.1) is a compile-time-only constant in `semstrait-ir` (or the graph crate when materialized) and is **not** persisted on manifest (C10.2 / C10.4).

## 7. `SemanticBinding` and `ManifestExpressions` (C2 + C11 + C12)

### 7.1 `SemanticBinding`

```rust
#[non_exhaustive]
pub struct SemanticBinding {
    pub data_kind_id: DataKindId,                                      // C2.4 — leaf-only direct linkage
    pub source_id: SourceId,
    pub mapping: BTreeMap<SemanticsId, SemanticMappingValue>,
}

#[non_exhaustive]
pub enum SemanticMappingValue {
    Column(ColumnRef),
    Literal(Literal),
    Expr(PhysicalExprId),                                              // C11
    MetadataRef(String),
}
```

Rules:

- **Leaf-only linkage** (C2.1). Bindings attach to leaf `Dataset` DataKinds; composite DataKinds (Unionset / Grainset / Joinset) traverse children to reach sources.
- **One-to-many** (C2.2). One Dataset can carry multiple bindings, each pointing to a distinct `PhysicalSource` (e.g., partitioned read, dual feed).
- **Forward only** (C2.3). Dataset variant carries `bindings: Vec<BindingId>`; binding carries `source_id`; reverse (`source_id -> [BindingId]`) is derived at load.
- **`SemanticMappingValue::Expr` references `PhysicalExprId`.** Manifest persists the post-desugar physical form referencing native columns (C11). The semantic / authoring form lives in the `semantic` pool below for diagnostic and round-trip purposes.

### 7.2 `ManifestExpressions` (split typed pools, C12)

```rust
#[non_exhaustive]
pub struct ManifestExpressions {
    pub semantic: BTreeMap<SemanticExprId, SemanticExpr>,
    pub physical: BTreeMap<PhysicalExprId, PhysicalExpr>,
}

#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticExprId(pub u64);

#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicalExprId(pub u64);
```

Rules:

- **Both forms persisted** (C11). `SemanticExpr` (= `Expr<SemanticLeaf>`, with sugar pre-resolved per D2 below) and `PhysicalExpr` (= `Expr<PhysicalLeaf>`, post-desugar canonical form referencing native columns) are first-class persisted artifacts.
- **Sugar layer is `SemanticExpr`-only** (C11). `PhysicalExpr` is post-desugar; sugar never crosses the persistence boundary into the physical pool.
- **Strongly-typed newtype IDs** (C12.3). `SemanticExprId` and `PhysicalExprId` are distinct types so reference sites preserve IR's type-level invariant (spec 35:698 / 35:702: "the static type system, not a runtime check, upholds this"). A unified `ManifestExpr { Semantic, Physical }` tagged enum was rejected because it would force a runtime `match` at every binding lookup site.
- **Compile-time content dedup, per pool** (C12.4). Two equivalent author-side `SemanticExpr` values resolve to one `SemanticExprId`; same for `PhysicalExpr`. (Whether duplicate authoring is itself a diagnostic surface is deferred — see §13.)
- **No cross-pool linkage at expr level** (C12.5). The semantic / physical pair is reconciled by upstream resolution context (`Provenance` per spec 33 §6.3.1, which lives on the resolved-expr table, not in `ManifestExpressions`).
- **Coverage views are derived, not persisted** (C16). A `SemanticExpr`'s coverage projects from its `SemanticLeaf`s' `SemanticsId`s into the bitmap at graph-build time; no per-Expr coverage view is persisted.

### 7.3 Serialization mechanism (C18)

- **Encoding format.** JSON via `serde` (C18.1). Matches §1's readability posture.
- **Determinism.** Deterministic encoding required (C18.2) — same `Expr` value renders identical bytes across compilations. Required for `manifest_epoch` / `model_hash` stability and for content dedup. Concretely: `BTreeMap` ordering, sorted `serde` field order, length-prefixed primitives, no NaN / Infinity in literal floats unless explicitly carried by typed Literal width discriminator.
- **Ownership of derives.** `impl Serialize` / `impl Deserialize` for `Expr<L>` and the leaf families lives in `semstrait-ir` (C18.3); manifest consumes them. Manifest does **not** vendor a parallel encoder.
- **Versioning.** No per-Expr format version field (C18.4). `manifest_epoch` is the migration signal; cross-epoch encoding changes are valid because graph-build re-derivations regenerate from primitives.
- **Explicit ID** (C18.5). Each persisted `Expr` is referenced by an explicit `SemanticExprId` / `PhysicalExprId`; compile-time content dedup is permitted under (C12.4).

## 8. `PhysicalSource` (C1 + C3 + C14 + C15)

### 8.1 Shape

```rust
#[non_exhaustive]
pub struct PhysicalSource {
    pub source_type: PhysicalSourceType,
    pub locator: String,
    pub version_ref: Option<PhysicalSourceVersionRef>,
    pub projected_schema: Vec<SourceColumn>,
    pub schema_fingerprint: Option<[u8; 32]>,
    pub provider_metadata: BTreeMap<String, String>,
}

#[non_exhaustive]
pub enum PhysicalSourceType {
    Table,
    File,
}

#[non_exhaustive]
pub enum PhysicalSourceVersionRef {
    IcebergSnapshotId(i64),
    MonotonicVersion(u64),
}

#[non_exhaustive]
pub struct SourceColumn {
    pub name: String,
    pub source_type: String,                     // native / engine-rendered string per C3.2
    pub nullable: bool,
}
```

### 8.2 Roster rules (C1)

- **Catalog-optional** (C1.1). When catalog is absent, model is the source of truth for physical shape (Thread A; foundations cascade in spec 14/15).
- **Per identity unit** (C1.2). One `PhysicalSource` per `(table | unique file path / glob root)` plus optional version reference for invalidation.
- **Referenced-only scope** (C1.3). Manifest is model-scoped — no catalog-wide scan; only sources reached by some binding are persisted.

### 8.3 Field set rules (C3)

- **Inline schema + fingerprint** (C3.1). `projected_schema` carries the per-binding-projected column list; `schema_fingerprint` is a SHA-256 over the canonical encoding of `projected_schema`. Drift checks then skip column iteration on unchanged sources, and planner-cache keys can use the fingerprint directly.
- **`SourceColumn` shape** (C3.2). `name` + `source_type: String` (native engine-rendered string, e.g., `int4`, `INTEGER`) + `nullable: bool`. Canonical mapping is deferred to planner / engine registry; manifest preserves the engine string verbatim.
- **`PhysicalSourceType` is minimised** (C3.3). `Table | File` only. `ObjectStore` and `Stream` were dropped as out-of-scope for v1.
- **`PhysicalSourceVersionRef` is simplified** (C3.4). `IcebergSnapshotId(i64) | MonotonicVersion(u64)`. `ProviderToken` was dropped (its sole driver — Stream — is out of scope).
- **Free-form provider metadata** (C3.5). `provider_metadata: BTreeMap<String, String>` carries provider-specific data not captured in the typed fields.
- **`locator: String`** (C3.6). Provider-interpreted; semstrait does not parse it.

### 8.4 `schema_fingerprint` rules (C15)

- **Algorithm: SHA-256** (C15.1). Width-aligned with `manifest_epoch` / `model_hash` / `catalog_fingerprint` (all `[u8; 32]`).
- **Canonical encoding** (C15.2). Length-prefixed concatenation of `SourceColumn` fields, in declared order:

  ```text
  for col in projected_schema (declared order):
      hash.update(col.name.len() as u32 LE)
      hash.update(col.name.bytes)
      hash.update(col.source_type.len() as u32 LE)
      hash.update(col.source_type.bytes)
      hash.update(col.nullable as u8)
  ```

  Length prefixes prevent ambiguity (`"ab" + "c"` vs `"a" + "bc"`).
- **Preserve declared order** (C15.3). Reorder is a real drift signal; sorted-hash would silently mask reorder-only diffs. Callers wanting reorder-insensitivity sort `projected_schema` at the input layer.
- **Independent of `version_ref`** (C15.4). `schema_fingerprint` answers "did the schema shape change?"; `version_ref` answers "did the underlying data change?". Both feed independent cache keys.
- **`None` permitted when `projected_schema` is empty** (C15.5) — avoids a synthetic all-zeroes hash collision across empty-schema sources.
- **No fingerprint-algorithm-version tag** (C15.6). Per-source fingerprints feed into `manifest_epoch` / `model_hash` implicitly via C18.2 deterministic encoding; drift in any source's schema flips the canonical bytes and bumps the manifest hash.

### 8.5 Catalog metadata fetch rules (C14)

- **Eager-all when catalog provided** (C14.1). Compile walks every reachable `PhysicalSource` and fetches metadata. Required cascade from C3.1: an inline `projected_schema` claim demands metadata at compile time.
- **Hard error on missing source** (C14.2). If catalog is reachable but a specific source is missing, compile fails. Diagnostic: `COMP_E_CATALOG_SOURCE_MISSING { source_id, locator }` (placeholder code, finalised in spec 30 / spec 34).
- **Hard error on multi-source Dataset partial fetch** (C14.3). Cascade from C14.2: an implicit Unionset (spec 23 §2.1 row A) over a multi-source Dataset assumes all branches are valid. Partial fetch = silently dropped branches, which is unacceptable.
- **Catalog-absent path** (C14.4). `catalog: Option<&dyn CatalogProvider>` parameter; when `None`, compile reads model's per-binding fields (Thread A) directly into `PhysicalSource` without any round-trip. `version_ref` is honoured if model provides it; otherwise `None`.
- **Mixed-mode override: model wins** (C14.5). When catalog is provided AND model declares overrides for specific sources, model wins. Diagnostic: `COMP_W_CATALOG_OVERRIDE { source_id, catalog_value, model_value }` (warning, not error).
- **`catalog_fingerprint` is fetched-content** (C14.6). Identity-only hashing would miss provider-internal drift. `None` when catalog is absent.

## 9. Compile and Persistence Boundary (C13 + C14)

### 9.1 `compile`

```rust
pub async fn compile(
    model: SemanticModel,
    catalog: Option<&dyn CatalogProvider>,
    fs: &dyn FileSystem,
) -> Result<
    (SemanticManifest, Diagnostics<CompileError>),
    (Diagnostic<CompileError>, Diagnostics<CompileError>),
>;
```

Notes:

- `catalog: Option<&dyn CatalogProvider>` reflects C1.1's catalog-optional posture.
- The two-channel return shape (`Diagnostics` carries warnings and informational items alongside the success or failure case) is unchanged from the previous spec.

### 9.2 Validation gates (C13)

The compile pass runs five gates. All gates emit **errors** unless explicitly noted otherwise.

| Gate | Scope | Notes |
|---|---|---|
| **G1** | Cycle detection across `SemanticExpr`. | Error. Scoped to expression DAGs; cycle detection across the Relationship graph is a separate gate (G6 candidate, deferred). |
| **G2** | Semantic-id resolution. | Error. Every `SemanticLeaf`'s `SemanticsId` must resolve in the registry. |
| **G3** | Type validation. | Error. Leaves match registry type; calls match function signature; operators match operand types. |
| **G4** | `PhysicalExpr` binding-side checks. | Error. Referenced `PhysicalExprId` exists; `ColumnRef` resolves in the bound `PhysicalSource.projected_schema`; inferred type matches the bound semantic. |
| **G5** | Orphan / dead-code detection. | **Error** (escalated from warning per ratification). Unreachable expressions risk propagating into plan as undefined behavior; strict-default starting place. Workflow-friction watch is recorded under deferred threads. |

### 9.3 Desugar policy (D2)

Sugar is rewritten **before persistence** (C13 D2). Persisted `SemanticExpr` is post-sugar canonical form; persisted `PhysicalExpr` is post-desugar physical form. Benefits `manifest_epoch` / `model_hash` stability across cosmetic reformulations.

### 9.4 `Repository`

`Repository` remains the persistence boundary and is the only out-of-band manifest I/O surface. Loading is contractually paired with §10's load-time integrity validation.

## 10. Load-time Integrity (CX1)

Manifest load performs a **single integrity pass**. Every cross-reference in the loaded artifact must resolve to an existing entry in its target collection. Failure surfaces as a typed `LoadError`:

```rust
#[non_exhaustive]
pub enum LoadError {
    DanglingReference {
        from: ReferenceSite,
        to_kind: ReferenceTargetKind,
        target_id: String,
    },
    StructuralViolation {
        site: ReferenceSite,
        rule: &'static str,
    },
    // ... (other variants)
}
```

Cross-references checked at load:

| Reference | From | To |
|---|---|---|
| `SemanticBinding.data_kind_id` | binding | `data_kinds` |
| `SemanticBinding.source_id` | binding | `sources` |
| `SemanticBinding.mapping[*]::Expr(id)` | binding mapping | `expressions.physical` |
| `DataKindVariant::Dataset.bindings[*]` | DataKind | `bindings` |
| `RoutingUnitRef::Synthesized(id)` | Grainset level | `data_kinds` |
| `JoinsetHop.relationship` | Joinset hop | `relationships` *or* `Joinset.scope_local_relationships` |
| `JoinsetHop.from` / `JoinsetHop.to` | Joinset hop | `data_kinds` |
| `Joinset.anchor`, `Joinset.members[*]` | Joinset variant | `data_kinds` |
| `SemanticInterface.{dimensions,measures,metrics,keys,fields}[*]` | interface | `semantics.entries` |
| `SemanticBitmask.words` (any set bit) | any bitmask | `semantics.entries` (some entry has matching `bit_position`) |
| `SemanticDefinition.bit_position` uniqueness | bitmap registry | within bitmap |

Structural violations also surface here as a defense-in-depth twin of compile-time gates:

- `Dataset.bindings.len() >= 1`
- `Unionset.branches.len() >= 2`
- `Grainset.levels.len() >= 2`; distinct grains; coarsest-first ordering
- `Joinset.members.len() == 2` (binary v1); `Joinset.hops.len() == 1` (binary v1); `Joinset.anchor in members`

This hardens C13's G1 / G2 / G4 from compile-time-only to compile-time + load-time. A hand-edited or repository-corrupted manifest cannot slip a dangling reference past load. The wire format remains an id-keyed map plus id reference (no OpenAPI-style `$ref` JSON pointers); CX1 preserves the integrity guarantee at less verbosity cost.

## 11. `Resolved*` Naming Rule

Use `Resolved*` only for compile-complete forms where uncertainty is eliminated (for example `ResolvedSemanticName`, resolved mapping, resolved schema-compatibility checks). Do **not** use `Resolved*` for author-level forms or for forms whose reconciliation is deferred to runtime / planner.

`SemanticExpr` and `PhysicalExpr` themselves are not `Resolved*`-prefixed even though they are persisted post-resolution; they live in IR and follow IR's naming policy.

## 12. Invariants

- I12.1 Manifest serialization does not contain runtime graph or planner cache state.
- I12.2 Top-level entity collections are id-keyed maps and do not duplicate ids inside values.
- I12.3 `SemanticBitmap.entries[id].bit_position` is unique within a `manifest_epoch`; cross-epoch renumber requires an epoch bump.
- I12.4 Every set bit in any `SemanticBitmask` corresponds to some `SemanticDefinition.bit_position` within the same manifest.
- I12.5 Every `BindingId` in a `Dataset.bindings` references an existing `bindings` entry; every binding in turn references existing `data_kinds`, `sources`, and `expressions.physical` entries.
- I12.6 `Dataset.bindings.len() >= 1`; `Unionset.branches.len() >= 2`; `Grainset.levels.len() >= 2` with distinct grains in coarsest-first order; `Joinset.members.len() == 2` and `Joinset.hops.len() == 1` (binary v1); `Joinset.anchor in Joinset.members`.
- I12.7 `Joinset.scope_local_relationships` is bounded to that Joinset's shadow scope; root-scope `relationships` does not contain shadow Relationships.
- I12.8 `data_kinds` includes implicit Unionsets (multi-source Dataset auto-synthesis) as top-level entries with content-derived hash `data_kind_id` and `origin = Implicit`.
- I12.9 Grainset same-grain merge — when a level has >= 2 same-grain children, `RoutingUnitRef::Synthesized` references a top-level implicit Unionset; single same-grain child uses `RoutingUnitRef::Inline`.
- I12.10 `JoinsetHop.hop_coverage` is cumulative across `[0..=i]`; `Joinset` top-level `coverage` equals `hops.last().hop_coverage`.
- I12.11 `ManifestExpressions.semantic` and `.physical` are independently dedup'd by content; no cross-pool linkage exists at expr level.
- I12.12 `PhysicalSource.schema_fingerprint`, when present, equals the SHA-256 of the canonical length-prefixed encoding of `projected_schema` per §8.4.
- I12.13 `PhysicalSource.schema_fingerprint` is `None` iff `projected_schema.is_empty()`.
- I12.14 `compile`'s `catalog: Option<&dyn CatalogProvider>` parameter selects between catalog-mediated and model-as-truth fetch paths; when both supply a value for the same source, model wins (with a warning).
- I12.15 No `compositions:`, `nodes:`, or `edges:` collection appears at manifest top level; those structures are reconstructed by `SemanticGraph` at build time.
- I12.16 `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` is a code-level constant in `semstrait-ir` (or graph crate) and is not persisted on manifest.
- I12.17 Manifest load runs CX1's single-pass cross-reference and structural integrity checks; failure surfaces as `LoadError` and prevents any planner consumption of the artifact.
- I12.18 `Resolved*` prefix is used only for compile-complete forms.

## 13. Deferred Threads and Open Considerations

The following items were carried out of ratification and remain open for downstream resolution. They do **not** block the current shape.

| Thread | Where it surfaces | Notes |
|---|---|---|
| Thread A — model-as-truth posture | C1.1 cascade | Foundations / model spec edits — model authoring surface needs explicit per-binding fields for `locator`, `source_type`, `projected_schema`, optional `version_ref` when catalog is absent. Wave 2 cascade (likely spec 14 / 15). |
| Thread B — glob expansion semantics | C1.2 / C3.3 / C14 | Compile-time vs runtime expansion; whether `PhysicalSourceType::File` carries `glob_root`. Resurfaces at downstream catalog/versioning work. |
| File-payload refinement of `PhysicalSourceType::File` | C3.3 | Whether `File` carries `glob_root: Option<String>`. Deferred. |
| G5 workflow-friction watch | C13 | Strict orphan-detection-as-error may produce friction in iterative authoring. Revisit if reported. |
| Duplicate-authoring diagnostic | C18.5 | Compile-time content dedup permits canonical single `ExprId`. Whether duplicate authoring is itself a diagnostic surface = downstream call. |
| G6 — Relationship-graph cycle detection | C9 / C10 | C13 G1 scopes to `SemanticExpr` cycles only; cycle detection across the Relationship graph is a separate gate. |
| Per-Request cap override for `MAX_IMPLICIT_COMPOSITION_DEPTH` | C10 | Post-v1 question. |
| Request-shaped pruning of `JoinsetHop.hop_coverage` | C7 | Today coverage is request-independent (compile-time first-cut). Whether request-shaped pruning ever enters the manifest layer = downstream review. |

## 14. Cascades for Wave 2 (cross-spec edits implied by this rewrite)

The following spec amendments are **implied** by the closed clauses but live in other documents and must be applied in Wave 2:

- **Spec 14 / 15** — Add author-side per-binding fields (`locator`, `source_type`, `projected_schema`, optional `version_ref`) for the model-as-truth (catalog-absent) path (Thread A).
- **Spec 16 §9.1** — Confirm `MAX_IMPLICIT_COMPOSITION_DEPTH` wording does not imply persistence.
- **Spec 16 §10.4** — Align "compile-time-enumerated compositions" framing with the lightweight posture (index lives in `SemanticGraph`, not manifest).
- **Spec 18** — `Relationship` is persisted both at root scope (`SemanticManifest.relationships`) and inline as Joinset-scoped shadow (`Joinset.scope_local_relationships`); no `scope` discriminator on the entity.
- **Spec 20** — Reflect bitmask-coverage layer cross-references; CCK skeleton (universal `coverage`); Public/Nested split alignment with `DataKind` / `NestedDataKind`.
- **Spec 21** — Clarify multi-source Dataset auto-synthesis surfaces as a top-level implicit `Unionset` (with `origin = Implicit`), referenced by id from `RoutingUnitRef::Synthesized` when applicable.
- **Spec 22 §1.3 I8** — Drop "JOIN-tree shape, per-pair JOIN-key index, ComposedSemanticInterface" from `ResolvedGrainset`'s manifest contract (C8.2 cascade).
- **Spec 23 §2.1 row A** — Confirm implicit-Unionset top-level placement with content-derived `data_kind_id`.
- **Spec 24 §1.4 I8 / §2.4** — Drop "resolved ComposedSemanticInterface" from `ResolvedJoinset`'s manifest contract; retire the `interface: ComposedSemanticInterface { … }` pseudo-shape (C7.4 cascade).
- **Spec 35** — Add `impl Serialize` / `impl Deserialize` on `Expr<L>` and the leaf families with deterministic ordering (C18.3); manifest depends on these derives.

These cascades are tracked here for navigability; the actual edits occur in their owning specs.
