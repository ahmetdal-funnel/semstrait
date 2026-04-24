# Technical Debt Registry

Tracked issues from code reviews during Phase J (Type System Hardening & Data Structure Optimization). Each item includes severity, origin phase, and remediation guidance.

---

## TD-001: KindRef.variant silent reset on serde deserialization

**Phase:** J4 (Unified SemanticGraph + KindRef)
**Severity:** Medium
**Location:** `crates/semstrait-model/src/types.rs` — `KindRef` struct

**Problem:** The `variant` field on `KindRef` is set programmatically during parse flatten but resets to its default value on serde deserialization. If a `CompiledManifest` is serialized to JSON and later deserialized, `KindRef.variant` will be incorrect.

**Current mitigation:** Variant is always populated during `parse::flatten_*` — no code path reads it from deserialized JSON today.

**Risk:** Any future consumer that deserializes a `CompiledManifest` and reads `KindRef.variant` will get wrong values silently.

**Remediation:** Either:
- Add `#[serde(serialize, deserialize)]` for `KindVariant` so it roundtrips correctly, or
- Rebuild variant info in a post-deserialization step (similar to how SemanticGraph is rebuilt)

---

## TD-002: SemanticGraph lost on serde roundtrip

**Phase:** J4 (Unified SemanticGraph + KindRef)
**Severity:** Medium
**Location:** `crates/semstrait-manifest/src/compiled.rs` — `CompiledManifest.semantic_graph`

**Problem:** `SemanticGraph` is marked `#[serde(skip)]` because `petgraph::Graph` doesn't implement Serialize/Deserialize. After deserializing a `CompiledManifest` from JSON, `semantic_graph` will be `SemanticGraph::default()` (empty graph).

**Current mitigation:** No code consumes `SemanticGraph` post-deserialization yet. The planner uses `RelationshipGraph` and `FieldIndex` (the deprecated structures it's meant to replace).

**Risk:** When migration from `RelationshipGraph`/`FieldIndex` to `SemanticGraph` completes, any workflow that loads a pre-compiled manifest from disk will silently operate with an empty graph.

**Remediation:**
- Add a `rebuild_semantic_graph()` method on `CompiledManifest` called after deserialization, or
- Implement custom Serialize/Deserialize for `SemanticGraph` (serialize as adjacency list, rebuild petgraph on deserialize)

---

## TD-003: SemanticGraph field_providers/dataset_fields may return duplicates

**Phase:** J4 (Unified SemanticGraph + KindRef)
**Severity:** Low
**Location:** `crates/semstrait-manifest/src/acceleration.rs` — `SemanticGraph` methods

**Problem:** Graph traversal methods `field_providers()` and `dataset_fields()` don't deduplicate results. If the graph has multiple edges between the same nodes (e.g., a field provided by a dataset through multiple paths), results will contain duplicates.

**Current mitigation:** Planner doesn't consume `SemanticGraph` yet — these methods are unused in production.

**Remediation:** Add `.dedup()` or use `HashSet` in traversal methods before they become load-bearing.

---

## TD-004: expect() panic messages assume compilation guarantee

**Phase:** J2 (Remove silent defaults + type derivation chain)
**Severity:** Low
**Location:** `crates/semstrait-manifest/src/acceleration.rs:51-67` — `resolve_dim_type`, `resolve_measure_type`

**Problem:** Panic messages state "compiled interface guarantees dimension/measure data_type is present" but the guarantee only holds if the compilation pipeline ran correctly. A programming error that bypasses compilation (e.g., hand-constructed test fixture missing a field) will produce a confusing panic message that says the interface "guarantees" something it doesn't.

**Current mitigation:** All test fixtures include complete data_type fields. Production path always goes through compilation.

**Remediation:** Consider changing to `unwrap_or_else(|| panic!("resolve_dim_type: dimension '{}' not found in interface '{}'", name, self.name))` to include the missing field name and interface name in the panic message for debuggability.

---

## TD-005: AVG aggregation always derives Number type

**Phase:** J2 (Remove silent defaults + type derivation chain)
**Severity:** Low
**Location:** `crates/semstrait-manifest/src/function_registry.rs` — `derive_aggregate_type`

**Problem:** `derive_aggregate_type(AVG, any_input_type)` always returns `DataType::Number` (float). Some SQL engines (PostgreSQL) use `NUMERIC` to preserve precision for `AVG(integer_column)`. This means semstrait's type system may report a less precise type than the engine actually returns.

**Current mitigation:** Standard SQL behavior. Matches most BI tool expectations. No user-reported issue.

**Remediation:** If precision-preserving AVG is needed, add a `DataType::Decimal { precision, scale }` variant and make `derive_aggregate_type` engine-profile-aware (e.g., PostgreSQL profile returns Decimal for AVG of Integer). This is a v1.x enhancement, not a v1 blocker.

---

## TD-006: active_bindings() allocates Vec on every call

**Phase:** J5 (Planner borrow optimization)
**Severity:** Low
**Location:** `crates/semstrait-planner/src/data_kind/mod.rs` — `PrunedView::active_bindings()`

**Problem:** `active_bindings()` returns `Vec<&DatasetBinding>`, allocating a new Vec on every call. If called multiple times in a hot path, this creates unnecessary allocations.

**Current mitigation:** Called once per `resolve()` invocation (setup phase), not in any loop or hot path.

**Remediation:** Add `fn active_iter(&self) -> impl Iterator<Item = &DatasetBinding>` for zero-allocation iteration when needed. Keep `active_bindings()` for cases where a collected Vec is required.

---

## TD-007: RelationshipGraph and FieldIndex not yet removed

**Phase:** J4 (Unified SemanticGraph + KindRef)
**Severity:** Low
**Location:** `crates/semstrait-manifest/src/compiled.rs` — `CompiledManifest`

**Problem:** `RelationshipGraph` and `FieldIndex` were marked `#[deprecated]` in J4 when `SemanticGraph` was introduced as their replacement. Both structures still exist and are the active code path in the planner. The migration is incomplete.

**Dependency:** Requires TD-002 (SemanticGraph serde) to be resolved first, so the graph survives serialization roundtrips.

**Remediation:** Migrate planner code from `RelationshipGraph`/`FieldIndex` to `SemanticGraph`, then remove the deprecated structures.

---

## TD-008: Generic I/O utilities placed in semstrait-manifest

**Phase:** Phase 3 (API cleanup + S3 loading)
**Severity:** Low — **Status: RESOLVED by design in [`docs/design/apis/31b_semstrait_core_io.md`](design/apis/31b_semstrait_core_io.md); migration tracked as `[TD-008-MIGRATE]`.**
**Location:** `crates/semstrait-manifest/src/io.rs` — `load_text()`, `IoError`

**Problem:** `load_text()` is a generic text-loading utility (local filesystem + S3) that is not manifest-specific. It lives in `semstrait-manifest` pragmatically because both consumers (`semstrait-api/cli.rs` and `semstrait/builder.rs`) already depend on manifest, and the `aws` feature flag passthrough already exists.

**Ratified target placement (supersedes the original remediation).** Transport traits and generic byte-blob back-ends live in `semstrait-core::io` behind a feature flag, not in a separate `semstrait-io` crate. Back-ends thin-wrap the `object_store` crate (Apache Arrow) for the actual Local / InMemory / S3 machinery — hand-rolled adapters replaced by battle-tested `object_store::ObjectStore` impls.

```
semstrait-core                     (pure data types)
├── core::io (feature "io",  default ON)   ← Source/Sink + FromIoBytes/IntoIoBytes
│                                             + Location + IoError;
│                                             backends::{memory,local,s3}
│                                             each back-end is a thin wrapper
│                                             over object_store::ObjectStore;
│                                             transport-only; no domain wrappers
├── semstrait-model
│       └── model::io   (feature "io")     ← load_model / dump_model
│                                             load_catalogs / dump_catalogs
│                                             over core::io
└── semstrait-manifest
        └── manifest::io (feature "io")    ← load_manifest / dump_manifest
                                              over core::io (binary: Bytes path,
                                              not String)
```

**Why `object_store`.** `object_store` (Apache Arrow project) already implements Local / InMemory / S3 with atomic-replace, retry, credential chain, multipart uploads, and proxy support. Adopting it eliminates ~300 LOC of hand-rolled glue, aligns `semstrait` with the broader Arrow / DataFusion ecosystem, and makes future GCS / Azure / HTTP back-ends ~30 LOC each (all supported natively by `object_store`, each gated by its own feature flag). `object_store` is an *internal* dependency — not re-exported on public signatures except the one `S3SourceBuilder::with_object_store_builder` escape hatch. See `31b §1.4` for the adoption rationale.

**Why not a separate `semstrait-io` crate (revised rationale).** `Source` / `Sink` are small, stable trait vocabulary shared by every upstream crate; a sibling crate would add a build edge without any additional isolation. The original zero-dep concern is addressed by making `io` a default-on feature and `io-aws` strictly opt-in: `--no-default-features` on core restores the zero-runtime-dep posture.

**Why not in manifest any longer.** With three consumers (`model::io`, `manifest::io`, future adapter bundle export), the utility is no longer manifest-specific, and the `io.rs` module currently creates an upward dependency pressure that blocks `model` from loading YAML without pulling the entire manifest crate.

**Migration (`[TD-008-MIGRATE]`):**
1. Add `object_store` (Apache Arrow) with `default-features = false` as an opt-in dep on `semstrait-core`; gated by `io`. Enable `object_store/aws` under `io-aws`.
2. Land `semstrait-core::io` module per `31b` spec — `Source`, `Sink`, `FromIoBytes`, `IntoIoBytes`, `Location`, `IoError`, `backends::memory::InMemory`, `backends::local::LocalFile`, `backends::s3::{S3Source, S3SourceBuilder}`. Each back-end is a thin wrapper over the corresponding `object_store` impl.
3. Introduce `semstrait-model::io` with `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs` per `32 §10.4` and `32b §5.4`. These call `src.read::<String>()` (YAML is UTF-8 text).
4. Introduce `semstrait-manifest::io` convenience wrappers per `33 §16.5`. These call `src.read_raw()` (manifest is binary — MessagePack / JSON bytes).
5. Migrate `semstrait-api/cli.rs` and `semstrait/builder.rs` call sites from `semstrait-manifest::io::load_text` → `semstrait-model::io::load_model` (for YAML loading) or direct `Location::from_str(path)?.read::<String>()` (for raw text).
6. Remove `semstrait-manifest::io::load_text` and drop `aws-sdk-s3` / `aws-config` direct deps from `crates/semstrait-manifest/Cargo.toml` (they come in transitively via `semstrait-core` / `object_store/aws` behind `io-aws`).

---

## TD-009: Computed dimension expressions with unreachable metadata values not detected at compile time

**Phase:** SR-10 (Static Pushdown)
**Severity:** Low
**Location:** `crates/semstrait-planner/src/simplify.rs`, model YAML validation

**Problem:** When a computed dimension's CASE expression references metadata dimension values that don't match actual extraction results (e.g., expression checks `dataset_name = 'facebook'` but the catalog namespace is `facebookads`), the CASE silently falls through to the else branch (producing `''`). No compile-time or plan-time warning is raised.

**Example:** `alpinestars_eu_ad_platform_v2.yaml` — the `market` expression uses `lit: "facebook"` but metadata extraction from the facebookads Polaris namespace at token 5 yields `"facebookads"`.

**Current mitigation:** Plan output is inspectable via `explain --output plan`; the collapsed `'' AS market` is visible.

**Remediation:** Add a compiler validation pass that cross-references literal values in computed dimension CASE conditions against known metadata extraction results from resolved sources. Emit a `COMPILE_W001`-level warning when a CASE branch's metadata condition can never be true for any resolved source.
