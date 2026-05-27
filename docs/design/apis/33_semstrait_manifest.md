---
prereqs: [10, 11, 13, 14, 15, 16, 17, 19, 30, 31, 35, 37]
authoritative-for:
  - the `semstrait-manifest` public API surface and crate boundary
  - the lightweight, model-aligned `SemanticManifest` contract used as planner input
  - manifest storage/index contract for IR-owned graph seeds: `SemanticNode`, `SemanticEdge`, `SemanticBinding`, `SemanticInterface`, `ComposedSemanticInterface`
  - manifest acceleration types: `SemanticInterfaceBitmap` and `SemanticInterfaceIndex`
  - manifest expression persistence contract (`SemanticExpr` persisted; `PhysicalExpr` resolved at graph-build/planning time)
  - `PhysicalSource` manifest contract (provider locator + version reference + schema fingerprint; no heavy provider snapshot payload)
  - compile/persistence boundaries, determinism posture, and naming policy for `Resolved*` forms
refined-by:
  - 34 (`semstrait-planner` consumes manifest and realizes runtime graph fragments; planner-runtime details are TODO/provisional)
  - 35 (`semstrait-ir` owns canonical graph and expression types)
  - 36 (`semstrait-adapter` consumes `SemanticPlan` output)
  - 37 (`semstrait-catalog` owns provider traits used during compile/drift probing)
---

# 33. semstrait-manifest

## 1. Purpose and Scope

`semstrait-manifest` owns:

1. `compile`: `SemanticModel + catalog/provider metadata -> SemanticManifest`
2. persisted manifest artifact loading/saving (`Repository`)

The artifact is intentionally lightweight and human-readable. It carries semantic graph seeds and indexes, not planner runtime cache state.

## 2. Design Posture

- **Model-aligned naming.** Manifest types follow semantic model entities (`SemanticNode`, `SemanticEdge`, `SemanticInterface`, `SemanticBinding`, `PhysicalSource`).
- **Readable contract over internal jargon.** Avoid stacked naming like `KindOfKind`; use explicit roles/types.
- **Resolved naming policy.** Prefix `Resolved*` only for values that are fully compile-resolved.
- **No runtime graph persistence.** Runtime graph fragments and cache policy are planner runtime concerns.
- **IR owns canonical graph vocabulary.** Manifest persists and indexes IR-owned graph-seed types; it does not redefine planner/runtime graph semantics.

## 3. Public Crate Surface

```text
semstrait-manifest
├── manifest
│   ├── node          // SemanticNode, SemanticNodePayload, SemanticRole, DataKindRole
│   ├── edge          // SemanticEdge, SemanticEdgeType
│   ├── interface     // SemanticInterface, ComposedSemanticInterface,
│   │                 //   SemanticInterfaceBitmap, SemanticInterfaceIndex
│   ├── binding       // SemanticBinding, SemanticMappingValue
│   ├── expr          // ManifestExpressions (SemanticExpr map)
│   ├── source        // PhysicalSource, PhysicalSourceType, PhysicalSourceVersionRef
│   └── index         // manifest lookup indices
├── compiler          // compile entry + pass orchestration
├── error             // CompileError + Diagnose
└── repository        // Repository + implementations
```

Primary root exports:

- `SemanticManifest`
- `SemanticNode`, `SemanticEdge`
- `SemanticInterface`, `ComposedSemanticInterface`
- `SemanticInterfaceBitmap`, `SemanticInterfaceIndex`
- `SemanticBinding`, `SemanticMappingValue`
- `ManifestExpressions`
- `PhysicalSource`, `PhysicalSourceVersionRef`
- `compile`
- `Repository`

## 4. `SemanticManifest` Contract

### 4.1 Top-level shape

```rust
#[non_exhaustive]
pub struct SemanticManifest {
    pub manifest_epoch: u64,
    pub model_hash: [u8; 32],
    pub catalog_fingerprint: Option<[u8; 32]>,

    pub nodes: BTreeMap<SemanticNodeId, SemanticNode>,
    pub edges: BTreeMap<SemanticEdgeId, SemanticEdge>,

    pub interfaces: BTreeMap<SemanticInterfaceId, SemanticInterface>,
    pub composed_interfaces: BTreeMap<ComposedSemanticInterfaceId, ComposedSemanticInterface>,
    pub interface_index: SemanticInterfaceIndex,

    pub bindings: BTreeMap<BindingId, SemanticBinding>,
    pub expressions: ManifestExpressions,
    pub sources: BTreeMap<SourceId, PhysicalSource>,

    pub metadata: SemanticManifestMetadata,
}
```

Rationale:

- graph/interface/binding/source entities are stored in stable-id keyed `BTreeMap`s for deterministic ordering and direct lookup;
- vectors are still used inside entities where ordered lists are meaningful (`dimensions`, `measures`, `metrics`, etc.).

### 4.2 `SemanticNode` (hybrid shape)

```rust
#[non_exhaustive]
pub struct SemanticNode {
    pub payload: SemanticNodePayload,
    pub interface_id: Option<SemanticInterfaceId>,
}

#[non_exhaustive]
pub enum SemanticNodePayload {
    DataKind {
        data_kind_id: DataKindId,
        name: DataKindName,
        role: DataKindRole,
    },
    Semantic {
        semantic_id: SemanticsId,
        name: SemanticsName,
        role: SemanticRole,
        data_type: Option<DataType>,
    },
    Expression {
        expr_id: ExprId,
    },
    Source {
        source_id: SourceId,
    },
}

#[non_exhaustive]
pub enum DataKindRole {
    Dataset,
    Grainset,
    Unionset,
    Joinset,
}

#[non_exhaustive]
pub enum SemanticRole {
    Dimension,
    Measure,
    Metric,
    Key,
    Field,
}
```

### 4.3 `SemanticEdge`

```rust
#[non_exhaustive]
pub struct SemanticEdge {
    pub from: SemanticNodeId,
    pub to: SemanticNodeId,
    pub edge_type: SemanticEdgeType,
    pub predicate_expr: Option<ExprId>,
}

#[non_exhaustive]
pub enum SemanticEdgeType {
    Relationship,
    Composition,
    Binding,
    DependsOn,
}
```

### 4.4 `SemanticInterface` and composed form

```rust
#[non_exhaustive]
pub struct SemanticInterface {
    pub dimensions: Vec<SemanticsId>,
    pub measures: Vec<SemanticsId>,
    pub metrics: Vec<SemanticsId>,
    pub keys: Vec<SemanticsId>,
    pub fields: Vec<SemanticsId>,
}

#[non_exhaustive]
pub struct ComposedSemanticInterface {
    pub components: Vec<SemanticInterfaceId>,
    pub bitmap: SemanticInterfaceBitmap,
}

#[non_exhaustive]
pub struct SemanticInterfaceBitmap {
    pub words: Vec<u64>,
}

#[non_exhaustive]
pub struct SemanticInterfaceIndex {
    pub by_semantic: BTreeMap<SemanticsId, BTreeSet<SemanticNodeId>>,
    pub by_interface: BTreeMap<SemanticInterfaceId, BTreeSet<SemanticNodeId>>,
    pub by_bitmap_hash: BTreeMap<u64, BTreeSet<SemanticInterfaceId>>,
}
```

### 4.5 `SemanticBinding`

```rust
#[non_exhaustive]
pub struct SemanticBinding {
    pub node_id: SemanticNodeId,
    pub source_id: SourceId,
    pub mapping: BTreeMap<SemanticsId, SemanticMappingValue>,
}

#[non_exhaustive]
pub enum SemanticMappingValue {
    Column(ColumnRef),
    Literal(Literal),
    Expr(ExprId),
    MetadataRef(String),
}
```

### 4.6 Expressions

```rust
#[non_exhaustive]
pub struct ManifestExpressions {
    pub semantic: BTreeMap<ExprId, SemanticExpr>,
}
```

Contract:

- Manifest persists canonical `SemanticExpr`.
- Manifest does **not** persist resolved `PhysicalExpr`.
- `SemanticExpr -> PhysicalExpr` resolution happens at graph-build/planning time (`34` TODO/provisional section).

### 4.7 `PhysicalSource`

```rust
#[non_exhaustive]
pub struct PhysicalSource {
    pub source_type: PhysicalSourceType,
    pub locator: String,
    pub version_ref: Option<PhysicalSourceVersionRef>,
    pub schema_fingerprint: Option<[u8; 32]>,
    pub provider_metadata: BTreeMap<String, String>,
}

#[non_exhaustive]
pub enum PhysicalSourceType {
    File,
    Table,
    ObjectStore,
    Stream,
}

#[non_exhaustive]
pub enum PhysicalSourceVersionRef {
    IcebergSnapshotId(i64),
    MonotonicVersion(u64),
    ProviderToken { provider: String, token: String },
}
```

Reasoning:

- Iceberg can use strongly typed snapshot ids.
- Other providers can use monotonic versions or explicit provider-native tokens.
- Full provider snapshot payloads are not part of manifest’s canonical contract.

## 5. `Resolved*` Naming Rule

Use `Resolved*` only for compile-complete forms where uncertainty is eliminated (for example resolved references, resolved mapping, resolved schema compatibility checks). Do not use `Resolved*` for author-level or deferred-runtime forms.

## 6. Compile and Persistence Boundary

```rust
pub async fn compile(
    model: SemanticModel,
    catalog: &dyn CatalogProvider,
    fs: &dyn FileSystem,
) -> Result<
    (SemanticManifest, Diagnostics<CompileError>),
    (Diagnostic<CompileError>, Diagnostics<CompileError>),
>;
```

`Repository` remains the persistence boundary and is the only out-of-band manifest I/O surface.

## 7. Invariants

- Manifest serialization does not contain planner runtime graph/cache objects.
- Every edge endpoint references an existing `SemanticNodeId`.
- Every binding references an existing node/source/expression id.
- Top-level graph/interface/binding/source collections are id-keyed maps and do not duplicate ids inside values.
- `SemanticInterfaceBitmap` coordinates are stable within a manifest epoch.
- `SemanticInterfaceIndex` sets are unique and deterministic (`BTreeSet` ordering).
- `by_bitmap_hash` is a prefilter only; callers must verify bitmap equality after hash hit.
- Expression ids in mappings must exist in `ManifestExpressions.semantic`.
- `PhysicalSource` stores version/hash invalidation signals, not heavy provider snapshots.

## 8. Planner Handoff Status (TODO in planner)

Manifest now provides clean graph seeds for planner runtime work. Planner-side runtime details (DAG construction policy, fragment cache policy, drift probe flow, graph-to-plan lowering pipeline) are intentionally marked TODO/provisional in `34` and will be finalized in the planner pass.

