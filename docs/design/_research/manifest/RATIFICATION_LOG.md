# Manifest Ratification Log

**Purpose.** Single source of truth for clause-by-clause ratification state during the lightweight-manifest design pass. State only; not spec. Spec edits to `apis/33_semstrait_manifest.md` and `apis/35_semstrait_ir.md` happen in Phase 3 after all clauses are closed.

**Workstyle reminder (CLAUDE.md).** Approval is clause-level: a directional pick on one sub-decision does NOT authorize derived sub-decisions. Each derived clause is its own decision.

---

## Clause Index

| Clause | Title | Status |
|---|---|---|
| C1 | PhysicalSource roster (mode, identity, scope) | **Closed** |
| C2 | DataKind ↔ PhysicalSource linkage | **Closed** |
| C3 | PhysicalSource field set (inline schema, types) | **Closed** |
| C4 | Global SemanticBitmap registry | **Closed** |
| C5 | Per-Dataset coverage | **Closed** |
| C6 | Per-Unionset coverage | **Closed** |
| C7 | Per-Joinset per-hop coverage | **Closed** |
| C8 | Per-Grainset per-grain coverage | **Closed** |
| C9 | Implicit composition (Joinset / Unionset) | **Closed** |
| C10 | Implicit-composition enumeration cap | **Closed** |
| C11 | Expression forms persisted (both Sem + Phys) | **Closed** |
| C12 | Expression storage shape (split typed pools) | **Closed** |
| C13 | Compile-time validation gates | **Closed** |
| C14 | Compile-time catalog metadata fetch | **Closed** |
| C15 | Schema fingerprint persistence | **Closed** |
| C16 | Expression × SemanticBitmap conjunction | **Closed** |
| C17 | Edge persistence shape (top-level shape gate) | **Closed** |
| C18 | Expression serialization mechanism | **Closed** |
| CCK | Coverage-kernel meta-shape | **Closed** |
| CX1 | Load-time integrity validation | **Closed** |

**Closed: 20 / 20. ALL CLAUSES RATIFIED.** Phase 3 unblocked.

---

## Closed Clauses

### C17 — Edge persistence shape (gates top-level shape)

**Pick:** option (d) — drop SemanticNode AND SemanticEdge from manifest. Persist primitives only (interfaces, bindings, sources, expressions, relationships, compositions, bitmaps); SemanticGraph constructs nodes+edges at build time.

**Rationale.** Synthesis finding #1: every surveyed system with on-disk artifact (dbt, Substrait) puts identity primitives on disk, rebuilds structure in memory.

**Cascade.** Deletes spec 33 §4.2 (`SemanticNode`), §4.3 (`SemanticEdge`), top-level `nodes` / `edges` maps. Per-DataKind handle moves from SemanticNode wrapper to DataKind primitive itself.

---

### C4 — Global SemanticBitmap registry

**Picks:**
- C4.1 = global scope (bit position spans all SemanticsIds in the manifest's set)
- C4.2 = explicit registry block, **wide** scope — registry holds full `SemanticDefinition` per entry; interfaces become bitmap-only views
- C4.3 = epoch-stable; cross-epoch renumber allowed (epoch bump rebuilds positions from canonical sort over `SemanticsId`)

**Naming convention codified:**
- **Bitmap** = the actual map (canonical, single, top-level): `SemanticBitmap`
- **Bitmask** (suffix) = computed mask value over the bitmap, held on local definitions

**Cascade for Phase 3.**
- Spec 33 `SemanticInterfaceBitmap` → `SemanticInterfaceBitmask` (it's a mask, not the bitmap)
- All per-X local coverage values use `*Bitmask` suffix (DatasetBitmask, JoinsetHopBitmask, GrainBitmask, etc.)
- New top-level field: `semantics: SemanticBitmap` containing per-entry `SemanticDefinition { semantic_id, name, role, data_type, bit_position }`
- Per-semantic attributes that used to live in `SemanticNodePayload::Semantic { name, role, data_type }` move to `SemanticDefinition` (cascade-completes C17(d))

---

### C11 — Expression forms persisted

**Pick:** both `SemanticExpr` and `PhysicalExpr` are first-class persisted manifest artifacts.

**Sugar layer:** `SemanticExpr` only. `PhysicalExpr` is post-desugar canonical form referencing native columns.

**Cascade.** Reverses spec 33 §4.6's current claim "Manifest does NOT persist resolved PhysicalExpr." `PhysicalExpr` lives inside `SemanticBinding.mapping[SemanticsId] = SemanticMappingValue::Expr(PhysicalExprId)`.

---

### C12 — Expression storage shape

**Picks:**
- C12.1 = flat indexed pool
- C12.2 = (P1) split pools — `semantic: BTreeMap<SemanticExprId, SemanticExpr>` + `physical: BTreeMap<PhysicalExprId, PhysicalExpr>`
- C12.3 = strongly-typed `SemanticExprId` / `PhysicalExprId` newtypes (preserves IR type-level discipline at reference sites; reasoning anchored in spec 35:698, 35:702, I5)
- C12.4 = compile-time content dedup, per pool
- C12.5 = no cross-pool linkage at expr level

**Rationale for split pools (P1) over tagged enum (P2).** A manifest-level `ManifestExpr { Semantic, Physical }` enum would regress on IR's type-level invariant per 35:698 ("the static type system, not a runtime check, upholds this") and 35:702 ("no `try_into_physical` runtime check, no defensive panic"). Every binding lookup site would re-introduce a runtime match.

**Final shape:**
```rust
pub struct ManifestExpressions {
    pub semantic: BTreeMap<SemanticExprId, SemanticExpr>,  // = Expr<SemanticLeaf>
    pub physical: BTreeMap<PhysicalExprId, PhysicalExpr>,  // = Expr<PhysicalLeaf>
}
```

---

### C13 — Compile-time validation gates

**Picks (all errors except where noted):**
- G1 — cycle detection across SemanticExpr — **error**
- G2 — semantic-id resolution (every SemanticLeaf's id resolves in registry) — **error**
- G3 — type validation (leaves match registry type; calls match function signature; operators match operand types) — **error**
- G4 — PhysicalExpr binding-side checks (referenced PhysicalExprId exists; ColumnRef in PhysicalSource.schema; inferred_type matches bound semantic) — **error**
- G5 — orphan / dead-code detection — **error** (escalated from warning per user; rationale: unreachable expressions risk propagating into plan as undefined behavior)
- Desugaring policy = **D2 canonicalize at compile** (rewrite sugar before persisting; benefits manifest_epoch / model_hash stability across cosmetic reformulations)

**Downstream surfacing flag.** G5-as-error means iterative authoring (rename a measure, leave old definition behind) gets a hard fail until orphan is removed. Strict-default starting place; revisit if workflow friction shows up.

---

### C16 — Expression × SemanticBitmap conjunction

**Picks:**
- C16.1 = α — leaves carry `SemanticsId` (canonical); position lookup is `bitmap.get(id).bit_position`. Survives cross-epoch renumber per C4.3.
- C16.2 = (i) lazy / derived at graph-build — no per-Expr coverage view persisted; walk tree, project to mask.
- C16.3 = (i) reverse index `SemanticsId → ExprIds` derived at load, not persisted.
- C16.4 = no extra structure for PhysicalExpr; coverage = single SemanticsId of owning binding.

**Effect.** No new manifest fields beyond what C4 + C12 already give us; conjunction = derivation policy, not new persistence.

---

### C18 — Expression serialization mechanism

**Picks:**
- C18.1 = α JSON via serde (matches §1 readability posture)
- C18.2 = deterministic encoding required (same `Expr` value → same bytes; required for `manifest_epoch` / `model_hash` stability + dedup)
- C18.3 = trait derivation lives in `semstrait-ir` (`impl Serialize`/`Deserialize` on `Expr<L>`, leaves); manifest consumes
- C18.4 = manifest_epoch only (no per-Expr format version)
- C18.5 = explicit `ExprId` (compile-time content dedup permitted; whether duplicate authoring is a diagnostic concern is a downstream call)

---

### C1 — PhysicalSource roster

**Picks:**
- C1.1 = catalog-optional. If catalog absent, model is the source of truth for physical shape.
- C1.2 = `PhysicalSource` per (table | unique file path / glob root) with snapshot id for invalidation.
- C1.3 = referenced-only scope (no catalog-wide scan; manifest is model-scoped).

**Deferred threads (do NOT ratify implicitly):**
- **Thread A — model-as-truth posture.** When catalog absent, model authoring surface (foundations) needs explicit per-binding fields for: locator, source_type, projected schema, optional version_ref. Currently lives implicitly in catalog. Foundations / model spec edits in Phase 3.
- **Thread B — glob expansion semantics.** Spark logical-relation semantics for synthetic table name; whether glob expanded at compile (manifest holds expanded list) or at runtime (manifest holds glob pattern); whether synthetic table name is a `PhysicalSourceType` variant tag. Will resurface at C14 / C15 (catalog/versioning).

---

### C2 — DataKind ↔ PhysicalSource linkage

**Picks:**
- C2.1 = α leaf-only direct linkage; composites traverse to reach sources via children
- C2.2 = (ii) one-to-many; one Dataset can have multiple bindings → multiple PhysicalSources (e.g., partitioned read, dual feed)
- C2.3 = (a) forward only — Dataset carries `bindings: Vec<BindingId>`, binding carries `source_id`; reverse derived at load
- C2.4 = `SemanticBinding { data_kind_id, source_id, mapping }` (replaces spec 33's `node_id` per C17(d) cascade)

---

### C3 — PhysicalSource field set

**Picks:**
- C3.1 = β inline schema + fingerprint (drift check skips column iteration on unchanged sources; planner-cache keys can use fingerprint)
- C3.2 = `SourceColumn { name, source_type: String, nullable: bool }` — native-string types; canonical mapping deferred to planner / engine registry
- C3.3 = minimized `PhysicalSourceType { Table, File }` — dropped ObjectStore + Stream as out of scope for v1
- C3.4 = simplified `PhysicalSourceVersionRef { IcebergSnapshotId(i64), MonotonicVersion(u64) }` — dropped ProviderToken (its use case was Stream)
- C3.5 = keep `provider_metadata: BTreeMap<String, String>` for free-form provider-specific data
- C3.6 = `locator: String` (provider-interpreted)

**Final shape:**
```rust
pub struct PhysicalSource {
    pub source_type: PhysicalSourceType,
    pub locator: String,
    pub version_ref: Option<PhysicalSourceVersionRef>,
    pub projected_schema: Vec<SourceColumn>,
    pub schema_fingerprint: Option<[u8; 32]>,
    pub provider_metadata: BTreeMap<String, String>,
}

pub struct SourceColumn { pub name: String, pub source_type: String, pub nullable: bool }
pub enum PhysicalSourceType { Table, File }
pub enum PhysicalSourceVersionRef { IcebergSnapshotId(i64), MonotonicVersion(u64) }
```

---

### CCK — Coverage-kernel meta-shape (shared across C5–C8)

**Picks:**
- CCK.1 = top-level `coverage: SemanticBitmask` on every DataKind (universal; satisfies spec 20 D4 — every ResolvedDataKind carries coverage)
- CCK.2 = enum `DataKindVariant::{Dataset, Unionset, Grainset, Joinset}` mirrors taxonomy D9; `#[non_exhaustive]` for I10 extension
- CCK.3 = per-constituent local masks live on the variant struct (not top-level DataKind); symmetric with `CompositionCoverage` keyed by `(ConstituentRef, SemanticsName)`
- CCK.4 = constituent reference shape — `DataKindId` for top-level kinds; nested kinds **inlined** into parent's variant struct, no separate id
- CCK.5 = `Vec<u64>` words encoding for SemanticBitmask; positions correspond to SemanticBitmap bit positions
- CCK.6 = no load-time precomputed reverse index; derived at SemanticGraph build

**Skeleton (locks per-variant work):**
```
DataKind
├── data_kind_id, name, role
├── coverage: SemanticBitmask     // universal union view
└── variant: DataKindVariant      // per-variant structure
```

Per-variant scaffolds (placeholder shapes; ratified in C5–C8):
- C5 Dataset = `bindings: Vec<BindingId>` (coverage = union of binding coverages)
- C6 Unionset = `branches: Vec<{kind_inline, branch_coverage, null_fill_mask, mode}>`
- C7 Joinset = `members + hops: Vec<{from, to, relationship_id, coverage_after_hop}>`
- C8 Grainset = `levels: Vec<{kind_inline, grain, level_coverage}>`

---

### C5 — Per-Dataset coverage

**Picks:**
- C5.1 = (ii) only `Native` / `Derived` Coverage variants contribute bits to bitmask; `NullFill` / `Metadata` excluded (they don't represent real source-side coverage).
- C5.2 = (a) union across bindings (one-to-many cascade from C2.2); bit set if at least one binding covers the semantic.
- C5.3 = no special orphan-bit rule (handled by C13/G2 unresolved-semantic compile error).
- C5.4 = `bindings.len() >= 1` invariant; empty → compile error (cascade from spec 20 §3 / spec 15 §2.1: leaf carries exactly one Binding minimum).

**Final shape:**
```rust
pub struct DataKind {
    pub data_kind_id: DataKindId,
    pub name: DataKindName,
    pub role: DataKindRole,
    pub coverage: SemanticBitmask,
    pub variant: DataKindVariant,
}

pub enum DataKindVariant {
    Dataset { bindings: Vec<BindingId> },   // len >= 1
    Unionset { /* C6 */ },
    Grainset { /* C8 */ },
    Joinset  { /* C7 */ },
}
```

---

### C8 — Per-Grainset per-grain coverage

**Picks:**
- C8.1 = `GrainsetLevel { grain, routing_unit: RoutingUnitRef, level_coverage }`. Routing unit = mixed shape: inline `NestedDataKind` for single same-grain child; `DataKindId` reference to top-level synthesized implicit Unionset for ≥2 same-grain children (per spec 22 §3.3 same-grain pre-merge + spec 23 §2.1 row A — implicit Unionsets are top-level entries with content-derived hash ids).
- C8.2 = (ii) **defer cross-grain JOIN-tree to graph build.** Manifest carries only per-level coverage + Keys (already in semantic interface); SemanticGraph synthesizes JOIN-tree at build time. **Phase 3 cascade: amend spec 22 §1.3 I8 — drop "JOIN-tree shape, per-pair JOIN-key index, ComposedSemanticInterface" from `ResolvedGrainset`'s manifest contract.** Aligns with C17(d) + C16 lightweight posture.
- C8.3 = coarsest-first level ordering (per spec 22 §5.2 / spec 12 §4.2)
- C8.4 = invariants: `levels.len() >= 2` (≥ 2 unique grains per spec 22 §5.2); each `level.grain` distinct; coarsest-first ordering

**Final shape:**
```rust
DataKindVariant::Grainset {
    levels: Vec<GrainsetLevel>,          // len >= 2, distinct grains, coarsest-first
}

pub struct GrainsetLevel {
    pub grain: Grain,
    pub routing_unit: RoutingUnitRef,
    pub level_coverage: SemanticBitmask,
}

pub enum RoutingUnitRef {
    Inline(NestedDataKind),       // single same-grain child
    Synthesized(DataKindId),      // implicit-Unionset top-level entry (≥2 same-grain children)
}
```

---

### C15 — Schema fingerprint persistence

**User framing:** drift-detection signal without round-trip catalog fetch.

**Synthesis indirect evidence:** Pinot/Druid CRC/SHA at content level (not per-binding schema level); Iceberg snapshot manifest precedent (Phase 2 Target C — strongest external alignment).

**Picks:**
- C15.1 = **SHA-256**. Width consistency with `manifest_epoch` / `model_hash` / `catalog_fingerprint` (all `[u8; 32]`). Threat model is drift detection, not collision adversary; SHA-256 trivially satisfies both at negligible cost (~200ns per fingerprint). Ecosystem alignment via `sha2` crate.
- C15.2 = deterministic length-prefixed concatenation of SourceColumn fields:
  ```text
  for col in columns (in declared order):
      hash.update(col.name.len() as u32 LE)
      hash.update(col.name.bytes)
      hash.update(col.source_type.len() as u32 LE)
      hash.update(col.source_type.bytes)
      hash.update(col.nullable as u8)
  ```
  Length prefixes prevent ambiguity ("ab"+"c" vs "a"+"bc").
- C15.3 = (i) **preserve declared column order**. Reorder is a real drift signal — many sources are positionally meaningful (CSV positional reads, Parquet schema evolution, downstream cast positional ABIs). Sorted-hash would silently mask reorder-only diffs. Callers wanting reorder-insensitivity sort `projected_schema` at the input layer.
- C15.4 = (i) **independent of `version_ref`**. Different concerns: `schema_fingerprint` answers "did the schema shape change?"; `version_ref` answers "did the underlying data change?". Aligns with Iceberg precedent (separate `schema_id` + `snapshot_id`). Cache-key composability: planner can key on either independently.
- C15.5 = `None` permitted when `projected_schema` is empty. Avoids synthetic all-zeroes hash collision across empty-schema sources.
- C15.6 = per-source fingerprints feed into `manifest_epoch` / `model_hash` implicitly via C18.2 deterministic encoding. `schema_fingerprint: [u8; 32]` field is naturally part of canonical-encoded `PhysicalSource`. No new computation; drift in any source's schema → manifest hash bumps.

**Tensions / open considerations carried.**
- `source_type` is engine-rendered string per C3.2 (`int4` vs `INTEGER` hash differently). Correct for drift detection — different rendered types means catalog returns different data, worth flagging.
- Phase 2 (Iceberg/Delta-Lake) will validate independent schema-id + snapshot-id posture; revisit C15.4 if precedent goes composite.
- No fingerprint-algorithm-version tag. C18.4 settled (no per-Expr format version); same logic applies — manifest_epoch bump is the migration signal.

---

### C14 — Compile-time catalog metadata fetch

**User framing (cascade from C1.1):** catalog-optional. If catalog absent, model is source of truth.

**Synthesis counter-signal:** no surveyed system pre-fetches catalog metadata at compile (5/5 = runtime). Semstrait is one step stricter; cascade from C3.1's β inline-schema commitment forces this posture (manifest claims to carry schema → metadata must be fetched at compile, by definition).

**Picks:**
- C14.1 = (a) **eager-all** fetch when catalog provided. Walk every PhysicalSource in resolved set; fail compile if any source unreachable. Cascade-driven by C3.1 (inline `projected_schema` requires metadata at compile).
- C14.2 = (i) **hard error** if catalog reachable but specific source missing. Aligns with C13's strict-default. Placeholder error code `COMP_E_CATALOG_SOURCE_MISSING { source_id, locator }`.
- C14.3 = hard error on multi-source Dataset partial fetch (cascade from C14.2). Reason: implicit Unionset (spec 23 §2.1 row A) over multi-source Dataset assumes all branches valid; partial = silently dropped branches.
- C14.4 = catalog-absent path. `catalog: Option<&dyn CatalogProvider>` parameter; when `None`, compile reads model's per-binding fields (Thread A) directly into PhysicalSource without round-trip. `version_ref` honored if model provides it; otherwise `None`.
- C14.5 = (ii) **model wins** on mixed-mode override (catalog provided + model overrides specific sources). Reasons: model is authored artifact, catalog is metadata service, author intent dominates. Useful escape hatch for fixture/test scenarios. Diagnostic `COMP_W_CATALOG_OVERRIDE { source_id, catalog_value, model_value }` (warning, not error).
- C14.6 = (b) **fetched-content hash** for top-level `catalog_fingerprint`. Drift detection: if catalog returns different data on next compile, fingerprint changes → manifest_epoch should bump. Identity-only hashing (a) misses provider-internal drift. `None` when catalog absent.

**Tensions / open considerations carried.**
- Catalog as compile-blocker: eager-all puts catalog reachability on critical path. Mitigation is operational (caching in `CatalogProvider` impl), not manifest's concern.
- Iceberg/Delta-Lake research target (Phase 2 C) is highest leverage; will refine C15's fingerprint algorithm.
- Glob expansion (Thread B) intersects: when `PhysicalSourceType::File` carries glob, eager-all means glob expansion at compile. File-payload refinement deferred.

---

### C10 — Implicit-composition enumeration cap

**User scope clarification:** *"MAX_IMPLICIT_COMPOSITION_DEPTH — applicable for composing data kinds, relationship blocks are open, but might some pathing and optimization appear (cycled joins detection, fan-out detection and query rewrite, etc.)"*

The cap applies to **DataKind composition** (implicit composition resolving a Request over multiple DataKinds via Relationship traversal). The relationship graph itself is **open** — no inherent cap; future optimization passes (cycle detection, fan-out detection, query rewrite) operate on the open graph.

**Picks:**
- C10.1 = keep `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` (no revision). Per spec 16 §9.1.
- C10.2 = (a) **compile-time-only constant** in `semstrait-ir` (or graph crate when materialized). NOT persisted on manifest. Reason: cap is a graph-build algorithm parameter; persisting risks drift between two manifests built with different cap values. Manifest is data; cap is code-level invariant.
- C10.3 = (ii) **distinguish cap-exceeded** with remediation hint "declare Joinset to escape the cap." Specific error code allocation deferred to spec 16 / spec 34 error roster work. Aligns with spec 24 §1.3 framing.
- C10.4 = **no manifest-layer enforcement**. C9.2 dropped `compositions:` field; manifest carries no enumerated compositions to validate against the cap.

**Scope clarification (per user):** cap is for `CompositionKind::Relationship` over DataKinds (spec 16 §11). It does NOT apply to:
- Joinset implicit-path BFS (per spec 24 §4.1.3 — Joinset is the escape hatch).
- The Relationship graph itself, which remains open. Future optimization passes (cycle detection, fan-out detection, query rewrite) work on the open graph.

**Phase 3 cascade.** Verify spec 16 §9.1 wording does not imply persistence; align if needed.

**Deferred threads.**
- Per-Request cap override (post-v1 question).
- Relationship-graph cycle detection (G6 candidate, noted in C9; C10's cap acts as safety net).
- Fan-out detection + query rewrite (graph-level optimization, future work).

---

### C9 — Implicit composition (Joinset / Unionset)

**User framing:** *"Implicit Joinset — something that we build up from top-level relationship block — computing for each join hop its bitmask. Implicit Unionset — pretty much the same as classic unionset."*

**Picks:**
- C9.1 = terminology pin. "Implicit Joinset" in user framing = `CompositionKind::Relationship` (spec 24 §1.3 / spec 16 §11), NOT `DataKindVariant::Joinset` (which is C7). Different objects, different lifecycles. Phase 3 must keep this naming clean to avoid downstream confusion.
- C9.2 = (a) **drop `compositions:` top-level field entirely**. SemanticGraph runs BFS at build time over `relationships:` + per-DataKind `coverage`. Same lightweight pattern as C17(d), C7.4, C8.2 — graph structure synthesized at build, not persisted. **Cascade: drop `compositions: BTreeMap<CompositionId, Composition>` placeholder from manifest top-level skeleton.**
- C9.3 = no new manifest field for per-hop bitmasks of implicit Relationship-traversals. Manifest primitives (SemanticBitmap C4, per-DataKind coverage CCK.1, Relationship graph) suffice; SemanticGraph derives per-hop cumulative coverage at build time using same algorithm as C7.2.
- C9.4 = implicit Unionset persistence (multi-source Dataset auto-synthesis per spec 21 §3.2 + spec 23 §2.1 row A) confirmed unchanged from C6 closing note: top-level `data_kinds:` entries with content-derived hash `data_kind_id`; structurally identical to explicit `DataKindVariant::Unionset`.
- C9.5 = add `origin: DataKindOrigin { Explicit, Implicit }` to top-level `DataKind` wrapper for diagnostic distinguishability (cycle messages, drift reports, debug). Parallel to C7.3's `path_origin` on Joinset hops. Doesn't change runtime semantics.
- C9.6 = field-first algorithm (spec 16 §11.4) plan-time inputs from manifest already settled by prior clauses: `semantics: SemanticBitmap`, `data_kinds:`, `relationships:`. No `compositions:` field. No new top-level fields.

**Final shape (manifest top-level after C9 cascade):**
```rust
pub struct SemanticManifest {
    pub manifest_epoch: u64,
    pub model_hash: [u8; 32],
    pub catalog_fingerprint: Option<[u8; 32]>,

    pub semantics:    SemanticBitmap,                              // C4
    pub interfaces:   BTreeMap<SemanticInterfaceId, SemanticInterface>,
    pub data_kinds:   BTreeMap<DataKindId, DataKind>,              // C5–C8 + C9.4 + C9.5
    pub bindings:     BTreeMap<BindingId, SemanticBinding>,
    pub sources:      BTreeMap<SourceId, PhysicalSource>,
    pub expressions:  ManifestExpressions,                         // C11/C12
    pub relationships: BTreeMap<RelationshipId, Relationship>,
    // compositions: dropped per C9.2
    pub metadata:     SemanticManifestMetadata,
}

pub struct DataKind {
    pub data_kind_id: DataKindId,
    pub name: DataKindName,
    pub role: DataKindRole,
    pub origin: DataKindOrigin,                                    // NEW per C9.5
    pub coverage: SemanticBitmask,
    pub variant: DataKindVariant,
}

pub enum DataKindOrigin { Explicit, Implicit }
```

**Phase 3 cascade.** Spec 16 §10.4's "compile-time-enumerated compositions" framing must be aligned to lightweight posture — the index lives in SemanticGraph (build-time), not in manifest.

**Deferred thread.** Cycle detection across the Relationship graph is a separate gate (likely G6) not covered by C13's G1 (which scopes to SemanticExpr cycles). Carried for downstream review.

---

### C7 — Per-Joinset per-hop coverage (NOVEL)

**User framing:** *"mask is computed for each join hop, to find effective join path in the first shot of search, and then graph will find more optimal join path"*. Manifest stores hop-level masks for first-cut path search; SemanticGraph optimizes from there.

**Picks:**
- C7.1 = `JoinsetHop { from, to, relationship: RelationshipId, direction: HopDirection, hop_coverage: SemanticBitmask }`. Adds explicit `from` (redundant under binary; N-ary-ready). `direction` re-uses spec 24 §2.3 `HopDirection { Forward, Reverse }`.
- C7.2 = **cumulative** coverage semantics — `hop_coverage[i]` = union of semantics reachable starting at anchor and walking hops `[0..=i]`. SemanticGraph's first-cut path search reads `requested_mask & hop_coverage[i] == requested_mask` directly. Binary v1 collapses to top-level Joinset coverage on the single hop; cumulative encoding earns its keep at N-ary lift.
- C7.3 = explicit/implicit collapse post-compile. Both resolve to `Vec<JoinsetHop>` per spec 24 §I5; manifest carries `path_origin: PathOrigin { Explicit, Implicit }` as audit/diagnostic tag, not structural branch.
- C7.4 = (ii) **defer ComposedSemanticInterface to graph build** (parallel to C8.2). Manifest carries hops + per-hop coverage; SemanticGraph synthesizes UnifiedSemantics + FieldProvenance at build time. **Phase 3 cascade: amend spec 24 §1.4 I8 — drop "resolved ComposedSemanticInterface" from `ResolvedJoinset`'s manifest contract; retire `interface: ComposedSemanticInterface { … }` from §2.4's pseudo-shape.**
- C7.5 = binary-v1 invariants — `members.len() == 2`, `hops.len() == 1`. Defense-in-depth twin of canonical `VALID_E_2400` / `COMP_E_2408`. Surfaced at load-time per CX1.
- C7.6 = scope-local relationships persisted **inline on the Joinset variant**, NOT in top-level `relationships:` map. Reason: shadow visibility is Joinset-bounded per spec 18 §2.10; intermixing forces a `relationship.scope: Option<DataKindId>` discriminator. Inline keeps shadow-vs-root cleanly separated.
- C7.7 = `anchor: DataKindId`, `members: Vec<DataKindId>`. CX1 cascade: canonical `DataKindRef` → `DataKindId` reference at manifest layer. Anchor invariant `anchor ∈ members` is defense-in-depth for canonical `VALID_E_2402`.

**Final shape:**
```rust
DataKindVariant::Joinset {
    anchor: DataKindId,
    members: Vec<DataKindId>,                       // len == 2 (binary v1); anchor ∈ members
    hops: Vec<JoinsetHop>,                          // len == 1 (binary v1); cumulative coverage
    path_origin: PathOrigin,
    scope_local_relationships: Vec<Relationship>,   // §2.10 shadow; bounded to this Joinset
}

pub struct JoinsetHop {
    pub from: DataKindId,
    pub to: DataKindId,
    pub relationship: RelationshipId,
    pub direction: HopDirection,                    // Forward | Reverse (spec 24 §2.3)
    pub hop_coverage: SemanticBitmask,              // cumulative semantics-reachable up to this hop
}

pub enum PathOrigin { Explicit, Implicit }
pub enum HopDirection { Forward, Reverse }
```

Top-level `DataKind.coverage` for Joinset variant equals `hops.last().hop_coverage` (last cumulative hop = full Joinset coverage).

**Open consideration carried.** `hop_coverage` is request-independent (computed at compile from each hop's `to`-side member's full coverage). User's "first shot of search" is graph-build-time path search, NOT plan-time query-shaped pruning. Recorded for downstream review if request-shaped pruning ever needs to enter the manifest layer.

---

### C6 — Per-Unionset coverage

**Picks:**
- C6.1 = each branch carries `branch_coverage: SemanticBitmask` (Native/Derived bits per C5.1 cascade)
- C6.2 = no persisted null-fill mask; derive at graph build from coverage complement (`top_level_coverage \ branch_coverage`). Plan-time NullFill emission per spec 23 §1.3 I1.
- C6.3 = `mode: UnionMode` on Unionset variant struct (per spec 32 §3.2 / 23 §2.1 row E)
- C6.4 = `Vec<UnionsetBranch>` preserves YAML declaration order; `len >= 2` invariant per spec 26 R3
- C6.5 = manifest mirrors spec 20 Public/Nested split — `DataKind` (top-level, has id) vs `NestedDataKind` (inlined, structural label only). **Cascade-applies to C7 / C8.**

**Final shape:**
```rust
pub enum DataKindVariant {
    Dataset { bindings: Vec<BindingId> },
    Unionset {
        mode: UnionMode,
        branches: Vec<UnionsetBranch>,   // len >= 2
    },
    Grainset { /* C8 */ },
    Joinset  { /* C7 */ },
}

pub struct UnionsetBranch {
    pub kind: NestedDataKind,
    pub branch_coverage: SemanticBitmask,
}

pub struct NestedDataKind {
    pub structural_label: String,        // per spec 26 §4 addressing
    pub coverage: SemanticBitmask,
    pub variant: NestedDataKindVariant,
}
```

**Note.** Implicit Unionsets (multi-source `Dataset` per spec 21 §3.2) ARE top-level — they carry a content-derived hash `data_kind_id` per spec 23 §2.1 row A and live in `data_kinds: BTreeMap<DataKindId, DataKind>`, not in `NestedDataKind`.

---

### CX1 — Load-time integrity validation (cross-cutting invariant)

**Pick.** Manifest load performs a single integrity pass: every cross-reference (`source_id`, `data_kind_id`, `binding_id`, `*ExprId`, `SemanticsId`) must resolve to an existing entry in its target collection. Failure surfaces as `LoadError::DanglingReference{from, to_kind, target_id}`.

**Effect.** Hardens C13's G1/G2/G4 from compile-time-only to compile-time + load-time defense. Hand-edited or repository-corrupted manifest cannot slip a dangling ref past load.

**Ratified in lieu of OpenAPI-style `$ref` JSON pointers.** Wire format keeps id-keyed map + id-ref (less verbose, no serde complexity). Integrity guarantee preserved.

---

## Open Clauses

_(none — all clauses closed; ratification phase complete)_

---

## Deferred Threads

| Thread | Where it surfaces | Notes |
|---|---|---|
| Thread A — model-as-truth posture | C1.1 cascade | Foundations / model spec edits in Phase 3 — model authoring surface needs explicit per-binding fields for locator, source_type, projected schema, optional version_ref when catalog absent. |
| Thread B — glob expansion semantics | C1.2 / C3.3 / C14 | Spark logical-relation semantics; compile-time vs runtime expansion; synthetic table name shape. Resurfaces at catalog/versioning clauses. |
| File-payload refinement of `PhysicalSourceType::File` | C3.3 | Whether `File` carries `glob_root: Option<String>` payload. Deferred to C14 / C15 where glob mechanics belong. |
| G5 workflow-friction watch | C13 | Strict orphan-detection-as-error may produce friction in iterative authoring. Revisit if reported. |
| Duplicate-authoring diagnostic | C18.5 | Compile-time content dedup permits canonical single ExprId. Whether duplicate authoring is itself a diagnostic surface = downstream. |

---

## Phase 2 Research Candidates (queued; not yet launched)

| Target | Informs | Priority |
|---|---|---|
| C — Iceberg / Delta-Lake schema-evolution model | C3, C14, C15 | Highest leverage |
| A — Bitmap registries at small-cardinality membership scale (LaunchDarkly, BigQuery, Snowflake) | C4, C5–C8 | High |
| B — Dual expression-form persistence (LLVM bitcode + .ll, Haskell Core, Rust HIR/MIR) | C11, C12 | Medium |
| D — Hop-depth caps (GraphQL federation, Cypher) | C10 | Lower |
| E — Cycle detection in semantic-composition graphs (ORM association validators) | C13 G1 | Lower |

---

## Phase 3 Application Plan (after all clauses closed)

1. Spec 33 (`apis/33_semstrait_manifest.md`) — full rewrite per closed clauses
2. Spec 35 (`apis/35_semstrait_ir.md`) — serialization derive on `Expr<L>` + leaves; integrity hooks
3. Foundations — model-as-truth fields per Thread A (deferred file TBD; likely 14_expressions or per-binding entity)
4. Spec 20 (`data-kinds/20_taxonomy.md`) — bitmask coverage layer cross-references; CCK skeleton
5. Specs 21 / 22 / 23 / 24 — per-variant manifest projection
6. Open-question hygiene (`questions/open/33_questions.md` etc.) per CLAUDE.md routing
7. STATUS.md update at session boundary

---

**End of `RATIFICATION_LOG.md`.**
