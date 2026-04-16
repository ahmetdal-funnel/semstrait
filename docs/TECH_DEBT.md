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
**Location:** `crates/semstrait-planner/src/kind/mod.rs` — `PrunedView::active_bindings()`

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
**Severity:** Low
**Location:** `crates/semstrait-manifest/src/io.rs` — `load_text()`, `IoError`

**Problem:** `load_text()` is a generic text-loading utility (local filesystem + S3) that is not manifest-specific. It lives in `semstrait-manifest` pragmatically because both consumers (`semstrait-api/cli.rs` and `semstrait/builder.rs`) already depend on manifest, and the `aws` feature flag passthrough already exists.

**Why not semstrait-core:** Core is zero-dep foundation (no I/O, no async, no network). Adding `tokio` + `aws-sdk-s3` would contaminate all 9 downstream crates.

**Trigger to extract:** When 3+ I/O utilities accumulate (e.g., `load_bytes`, `load_yaml_multi`, `write_artifact`), extract `semstrait-manifest::io` into a dedicated `semstrait-io` crate at the same DAG level as `semstrait-model` and `semstrait-catalog`:

```
semstrait-core                     (pure data types, zero I/O)
    ├── semstrait-model
    ├── semstrait-catalog
    ├── semstrait-io     ← NEW    (load_text, S3, local fs — generic I/O)
    └── semstrait-ir
```

**Remediation:**
1. Create `crates/semstrait-io/` with `tokio` + `aws-sdk-s3` (behind `aws` feature)
2. Move `io.rs` content from manifest to the new crate
3. Update manifest, api, and facade to depend on `semstrait-io`
4. Remove `aws-sdk-s3` and `aws-config` from manifest's Cargo.toml
