---
prereqs:
  - 00
  - implementation/40_refactor_plan.md
  - implementation/41_deprecations.md
  - apis/30_api_contracts.md
authoritative-for:
  - per-version caller-facing migration notes (format + content)
  - upgrade-path documentation template for every MAJOR release
  - breaking-change communication format (table-of-deltas + recipes)
  - the v1.0 delta catalog from pre-1.0 legacy code to the green-field design
  - the post-v1 deferred-features milestone plan (v2.0, v2.x, v3.0)
refined-by:
  - future revisions of this file — each MAJOR cut appends a new `§N. vX.Y` section in the `§2` format
---

# 42. Migration Notes

> **Status:** Round-1 draft. This document is the **caller-facing release-note ledger**: for each MAJOR cut taken under `40`'s phased plan, what breaks, what migrates, what's new. Organized per-version. Per `30 §2.2`, `30 §11.2`, `30 §12`, every MAJOR and every provisional-crate MINOR that carries a break lands an entry here.

## 1. Purpose and Scope

### 1.1 Who reads this doc

Callers upgrading between MAJOR releases. Callers reviewing a `CHANGELOG.md` entry that says "see `42 §X.Y`". Release-management drafting public-facing release notes. Not designers; not YAML authors (whose migration is framed here in terms of YAML surface diffs, but whose day-to-day reference is the `21`–`24` data-kind docs and the model README).

### 1.2 What this doc is

`42` is the **structured migration guide**. It complements `CHANGELOG.md` at the following level of detail:

| Layer | Scope | Audience |
|---|---|---|
| `CHANGELOG.md` | Every commit / every PR | Release-management; close-grained diff review |
| `implementation/41_deprecations.md` | Every `#[deprecated]` symbol — tombstone ledger | Implementers tracking the deprecation lifecycle |
| `implementation/42_migration_notes.md` (this) | Every MAJOR — before-and-after for callers | Callers upgrading across MAJORs |

`42` is **not** an implementation plan (that is `40`). `42` is **not** a deprecation ledger (that is `41`). `42` does not re-open any ratified design decision; it narrates the caller-visible consequences of the decisions `40` has sequenced.

### 1.3 What lands here

A new section is appended here whenever:

- A coordinated workspace MAJOR cut is taken (every one produces an entry, per `30 §11.2`).
- A provisional-crate (`semstrait-adapter`, `semstrait-catalog`, or any per-adapter / per-provider subcrate) cuts a MINOR that carries a non-additive change (per `30 §13`'s note on provisional tiers).
- A deprecated symbol is finally removed (its `41` tombstone moves here alongside the MAJOR that removes it, per `30 §12.2`).
- An error code is retired (per `30 §6.3` / `30 §6.7`).

Sections are ordered newest-to-oldest at publish time; this document presents v1.0 first because v1.0 is the reference cut.

### 1.4 Layering posture

`42` is layered strictly below `40` (plan) and `41` (tombstone ledger). It refines both by rendering them caller-facing. It is layered below `30` (semver policy) — the format it uses is what `30 §11.2` mandates for every MAJOR.

## 2. Communication format

Every per-MAJOR section in this document follows the following structure. Sub-headings are mandatory; a sub-heading with no content carries the literal marker `None in this cut.`

### 2.1 Skeleton

```
## §N. vX.Y

### §N.1 Summary
  One to two sentences: what this MAJOR delivers, who is affected.

### §N.2 Breaking changes
  Table: BEFORE (symbol / YAML key / code) → AFTER (symbol / YAML key / code) → OWNING DOC (§).
  Grouped by caller-facing surface: YAML surface / Rust API / Error codes / Behavioral.

### §N.3 Deprecated-but-not-removed
  Table of `#[deprecated]` symbols kept alive as backward-compat shims. Each row
  references its `41` tombstone entry.

### §N.4 New features
  Bulleted list — not deltas, not renames; net-new surface ratified for this cut.

### §N.5 Migration recipes
  Concrete code / YAML transformations for the common patterns. Each recipe:
  a) problem one-liner; b) BEFORE snippet; c) AFTER snippet; d) pitfalls.

### §N.6 Required caller actions
  Short checklist of what every consumer must do to land the upgrade.

### §N.7 Automated migration aids
  Any machine-readable rename maps, `cargo-fix` suggestions, or scripted
  transformations that exist for this cut. `None` is acceptable.
```

### 2.2 Style notes

- Tables use `BEFORE` / `AFTER` / `OWNING DOC` columns. The owning-doc column is a cross-ref (e.g. `32 §14.4`) so the caller can trace a delta back to the ratified design.
- Code snippets in recipes are Rust-attributed (` ```rust`) or YAML-attributed (` ```yaml`) unless they reference existing code in the repository. The snippets are illustrative — they are not code-reference blocks into the actual codebase.
- Per `00 §8`, cross-refs are by section number (`N §M.K`), never by code path.
- No emojis, per the tree-wide convention.

### 2.3 Versioning granularity

`42` tracks MAJORs plus provisional-crate non-additive MINORs. For ordinary additive MINORs (new `#[non_exhaustive]` variants, new public symbols, new default-bodied trait methods), a brief "notable MINOR additions" bullet may be folded into the next-MAJOR entry; full sections are reserved for cuts that require caller action. Patch releases never appear here.

---

## 3. v1.0 — The ratified green-field release

### 3.1 Summary

v1.0 is the **first ratified release** of the `semstrait` workspace under the green-field design of `docs/design/`. Callers running against pre-1.0 code (the "before" state referenced throughout `40 §2`) are migrating across the largest single delta the workspace will ever emit: every crate's public surface has moved. This entry catalogs that delta.

The v1.0 cut is the point at which `30 §13`'s stability tiers lock. Post-v1.0, additive evolution is MINOR (per `30 §2.1`); non-additive evolution is MAJOR.

### 3.2 Breaking changes

The delta below is pulled from `40 §2` (per-crate deviation catalog) and `41` (symbol-level tombstones). Rows are grouped by caller-facing surface.

#### 3.2.1 YAML surface

| BEFORE (pre-1.0 YAML) | AFTER (v1.0 YAML) | OWNING DOC |
|---|---|---|
| Top-level `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` separate blocks; kind inferred from containing block | Single top-level `data_kinds:` block with explicit `kind:` discriminator (`kind: dataset` / `kind: grainset` / `kind: unionset` / `kind: joinset`) | `32 §4.2`, `32 §14.4` row 1 |
| `ChildEntry` sub-block on complex kinds (implicit per-kind nesting) | Uniform `children:` list with `ref:` entries pointing to declared top-level `data_kinds` | `32 §14.4` row 2 |
| `YamlJoinset.associativity: JoinAssociativity` | `JoinsetSpec.anchor` (root child) + `JoinsetSpec.path: JoinPath` | `24 §2.2`, `32 §5.4`, `32 §14.4` row 3 |
| `ColumnMappingValue` variants `{ Anchored, Computed, Literal, Column, Metadata }` | `ColumnMappingValue` variants `{ Column, Literal, Computed, Metadata }` (Anchored collapses into Computed) | `32 §8.3`, `32 §14.4` row 4 |
| Model-local `DataType` enum (11 variants) | `semstrait-core::DataType` (14 variants ratified in `13 §2.1` / `31 §4.1`) | `32 §14.4` row 5 |
| Model-local `TemporalGrain` | `semstrait-core::Grain` | `32 §14.4` row 6 |
| `Relationship` fields: `relationship_type:` / `source_set:` / `target_set:` | `Relationship` fields: `from:` / `to:` / `cardinality:` / `join_type:` / `directionality:` | `16 §2.1`, `32 §7`, `32 §14.4` row 7 |
| `TemporalHistorization: ...` on DataKind YAML | `temporal_shape: ...` on DataKind YAML | `00 §4.3`, `17 §1.3` |

#### 3.2.2 Rust API — top-level types and traits

| BEFORE (pre-1.0 symbol) | AFTER (v1.0 symbol) | OWNING DOC |
|---|---|---|
| `CompiledManifest`, `CompiledDataKind`, `CompiledSimpleKind`, `CompiledInterface`, `CompiledSource`, `CompiledColumnMapping` | `Manifest`, `ResolvedDataKind`, `ResolvedSimpleKind`, `ResolvedInterface`, `ResolvedSource`, `ResolvedColumnMapping` | `00 §4.3`, `33 §3` |
| `StorageProvider` trait | `FileSystem` trait | `00 §4.3`, `37 §1.1` |
| `LogicalPlan` (top-level plan type in `semstrait-ir`) | `SemanticPlan` | `35 §3.1`, `[TD-IR-RENAME]` |
| `PlanArtifact` | `EngineArtifact` (sum type: `Sql(SqlArtifact)` / `Plan(EnginePlan)`) | `36 §3`, `DL-023` |
| `TemporalHistorization` enum | `TemporalShape` enum | `00 §4.3`, `17 §1.3` |
| `ScdType` enum | `ScdSubtype` enum | `17 §2` |
| `ModelError` unified error | `ParseError` + `ValidateError` (split by stage) | `31 §8`, `32 §10.2`, `32 §14.4` row 8 |
| Raw `Expr` used at every site | First-class `SemanticExpr` / `PhysicalExpr` newtypes over `Expr` (context-enforcing invariants) | `14 §4`, `31 §4.4` |
| Function names as plain strings (and a small closed enum in places) | `CanonicalFn` newtype (`struct CanonicalFn(&'static str)`) with `pub const` identities (`CanonicalFn::UPPER`, …) | `00 §4.1`, `14a §2`, `31 §5` |
| `FunctionRegistry` inside `semstrait-manifest` | `FunctionRegistry` in `semstrait-core`, reached via `semstrait-core::function_registry()` | `14a §2.1`, `31 §9` |
| Per-stage error enums surfaced directly on public APIs | `Diagnostic` / `Severity` / `Span` / `ContextLine` canonical structured errors; typed enums convert via `IntoDiagnostic` at crate boundaries | `30 §5`, `31 §6` |
| `simplify` pipeline verb | `optimize` pipeline verb | `00 §4.3` |
| `EngineAdapter::debug_sql` trait method (with default impl) | `debug_sql` as a free function | `36 §3.5`, `[TD-ADAPTER-DEBUG-SQL-FREE-FN]` |
| `PlanBuilder` (adapter-side construction path) | Retired; adapters consume `SemanticPlan` directly and emit via `adapt` | `[TD-ADAPTER-PLAN-BUILDER-RETIRE]` |
| `EngineProfile` as a dialect-carrying supertrait on `EngineAdapter` | `Dialect` trait split off from `EngineAdapter`; adapters have-a `Dialect`, not are-a dialect | `36 §2`, `[TD-ADAPTER-DIALECT-SPLIT]` |
| `CompileError` as a bespoke enum per crate | `CompileError` in `semstrait-manifest` re-exporting the shared variants from `semstrait-core::CompileError` | `[TD-33-ERROR-UNIFY]` |
| `RelationshipGraph` + `FieldIndex` consumed by planner | Removed; planner consumes Manifest indices directly (Coverage + Composition + relationship graph materialized per `33 §7`) | `TD-007`, `33 §7` |
| `active_bindings()` allocating `Vec` every call | Kept signature; allocation moved off the hot path (non-blocking; opportunistic per `40 §5.4`) | `TD-006` |

#### 3.2.3 Error codes

| BEFORE (pre-1.0) | AFTER (v1.0) | OWNING DOC |
|---|---|---|
| Raw-string errors on public APIs (`Result<T, String>`, `Result<T, anyhow::Error>`) | `Result<T, Diagnostic>` or accumulate shapes per `30 §7`; `IntoDiagnostic` conversion boundary at every stage | `30 §5`, `30 §5.5` |
| Free-form per-stage enum codes (no uniform prefix) | `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` format (e.g. `PARSE_E_0001`, `EXPR_W_0001`, `PLAN_E_0500`, `ADAPT_E_0300`, `IR_E_3500`, `CAT_E_0100`, `FS_E_0100`, `IO_E_0100`) with reserved sub-ranges per category | `30 §6.1`, `30 §6.2` |
| Error enums not uniformly `#[non_exhaustive]` | Every public error enum (`ParseError`, `ValidateError`, `CompileError`, `PlanError`, `OptimizeError`, `AdaptError`) is `#[non_exhaustive]` | `30 §4.1` |
| Per-stage public-API shapes ad-hoc (some accumulate, some fail-fast, some drop warnings silently) | `parse` / `validate` accumulate; `compile` / `plan` / `optimize` / `adapt` fail-fast with warnings preserved in both success and failure arms (`Result<(Output, Vec<Diagnostic>), (Diagnostic, Vec<Diagnostic>)>`) | `10 §5`, `30 §7` |
| `Severity { Error, Warning, Note }` sketched at `10 §5.1` | `Severity { Info, Warning, Error }` (Note renamed to Info); `#[non_exhaustive]` | `30 §5.2` |

No stable error codes are retired in v1.0 (per `30 §6.7`, v1 introduces no retirements). Every pre-1.0 code maps to a `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` literal in the new scheme; the mapping is a `41` concern.

#### 3.2.4 Behavioral changes

| BEFORE (pre-1.0 behavior) | AFTER (v1.0 behavior) | OWNING DOC |
|---|---|---|
| Expression resolution re-walked at plan time per DataKind pathway | Every `(Semantics name, Binding id)` pair pre-resolved at compile into a `PhysicalExpr` stored in `ResolvedExprTable`; plan-time expression lookup is O(1); no `Relationship` walking at plan time | `14b §2`, `14b §5`, `33 §6` |
| Constraint checks scattered across planner internals | Step-0 `ConstraintValidator` runs at the start of every plan per `11 §8.4`; emits `ConstraintViolation` via structured `Diagnostic` | `11 §8.4`, `11 §8.7` |
| `compile` accumulated best-effort; some partial Manifests escaped | `compile` is fail-fast: first `Error` aborts; accumulated `Warning`/`Info` carried to caller via the `Vec<Diagnostic>` arm | `10 §5`, `30 §7` |
| `Request.from` was required (or a heuristic replacement used) | `Request.from = None` is accepted; planner performs **field-first resolution**, mapping requested Semantics to their owning DataKinds via Manifest indices and traversing the top-level `Relationship` graph when fields span multiple DataKinds | `16`, `34 §*`, `00 §4.1` Request row |
| Implicit composition depth unbounded; ambiguous paths resolved non-deterministically | `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` enforced with `PLAN_E_0502 CompositionDepthExceeded`; ambiguous-path ties produce `PLAN_E_0500 AmbiguousImplicitComposition` (no heuristic) | `16 §9.1`, `16 §11.4`, `34 §*` |
| Manifest bytes not stable across `compile` invocations | Manifest is deterministic (I4): identical `(Model YAML, Catalog snapshot)` produces byte-identical Manifest bytes via `Repository::save`; MessagePack + JSON encoders supported per `33 §14` | `00 §9 I4`, `33 §14` |
| `SemanticGraph` skipped on serde; `KindRef.variant` reset on deserialize | Bespoke serde impl preserves every state across `Repository::save` / `Repository::load` round-trip | `TD-001`, `TD-002`, `33 §14.1` |
| `SqlArtifact` carried dialect informally | Every `SqlArtifact` carries `DialectId` so consumers can route emitted text to the correct engine | `30 §4.2`, `36 §3.2` |
| `CatalogProvider` + `StorageProvider` method set overlap | `CatalogProvider` (structured metadata) and `FileSystem` (generic I/O) are independent axes per `37 §1.1`; neither conflated with the other | `37 §1.1`, `00 §4.1` |
| Schema drift covered `manifest.datasets` only | Drift check covers every resolved source; narrow, gated pre-plan hook via `CatalogProvider::check_schema_drift` (I11b) | `37 §4`, `00 §9 I11` |

### 3.3 Deprecated-but-not-removed

The following symbols gain `#[deprecated]` annotations at v1.0 and are retained as backward-compat shims for one MINOR cycle per `30 §12.4`. Each row points to its `41` tombstone entry; the shim is removed at v2.0.

| Deprecated symbol | Replacement | Shim removal target | `41` tombstone |
|---|---|---|---|
| `TemporalHistorization` (type alias + enum variants) | `TemporalShape` | v2.0 | `41 §TD-LEGACY-TEMPORAL-HISTORIZATION` |
| `Compiled*` family (`CompiledManifest`, `CompiledDataKind`, `CompiledInterface`, `CompiledSource`, `CompiledColumnMapping`) | `Manifest` / `Resolved*` family | v2.0 | `41 §TD-LEGACY-COMPILED-PREFIX` |
| `StorageProvider` trait (alias) | `FileSystem` | v2.0 | `41 §TD-LEGACY-STORAGE-PROVIDER` |
| `LogicalPlan` (`pub type LogicalPlan = SemanticPlan;`) | `SemanticPlan` | v2.0 | `41 §TD-IR-RENAME` |
| `simplify` (pipeline verb) | `optimize` | v2.0 | `41 §TD-LEGACY-SIMPLIFY-VERB` |
| Legacy YAML grammar: `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` blocks | Unified `data_kinds:` block | v2.0 (parser emits `PARSE_W_*` during the shim window) | `41 §TD-LEGACY-YAML-BLOCKS` |
| Legacy `Relationship` field names (`relationship_type:`, `source_set:`, `target_set:`) | `from:` / `to:` / `cardinality:` / `join_type:` / `directionality:` | v2.0 | `41 §TD-LEGACY-RELATIONSHIP-FIELDS` |
| Legacy DataType variant names (where they existed) | 14-variant set in `13 §2.1` | v2.0 | `41 §TD-LEGACY-DATATYPE-VARIANTS` |
| `EngineAdapter::debug_sql` (trait method with default impl) | `adapter::debug_sql(...)` free function | v2.0 | `41 §TD-ADAPTER-DEBUG-SQL-FREE-FN` |
| `PlanBuilder` adapter-side type | Adapters consume `SemanticPlan` directly | v2.0 | `41 §TD-ADAPTER-PLAN-BUILDER-RETIRE` |
| `FunctionRegistry` re-export from `semstrait-manifest` | Direct import from `semstrait-core::function_registry()` | v2.0 | `41 §TD-LEGACY-REGISTRY-HOME` |

Per `30 §12.1`, every entry above receives a rustc deprecation warning at first use; per `30 §12.2`, each entry is reflected in `41` as it lands. Callers should eliminate all deprecation warnings before upgrading to v2.0, where the shims are removed.

### 3.4 New features

Net-new surface ratified at v1.0 (not renames, not removals):

- **`TemporalShape` classification** — first-class `Timeseries` / `Events` / `Snapshot` / `SCD` axis on every DataKind, with SCD subtype taxonomy (`Type0` … `Type6`, common ones named at the top level); vocabulary ratified, planner support DEFERRED (see `§5`). (`17`)
- **`JoinType::AsOf` variant** — vocabulary-ratified for `SCD` / `Snapshot` × `Events` joins; planner emission DEFERRED to v2.0 (see `§5`). (`00 §4.1` `JoinType` row, `17 §5`)
- **`ResolvedExprTable`** — compile-time pre-computed `(Semantics, Binding) → PhysicalExpr` map enabling O(1) plan-time expression lookup. Callers reading a Manifest can treat expression resolution as a table lookup. (`14b §2`, `33 §6`)
- **`Dialect` machinery** — SQL dialect as a first-class trait axis separated from `EngineAdapter`. `DialectId` is carried on every `SqlArtifact`. `Ansi`, `DataFusion`, `DuckDb`, `Spark` ship as bundled `DialectId`s. (`36 §2`, `00 §4.1` `Dialect` + `SqlArtifact` rows)
- **`CatalogProvider` + `FileSystem` separation** — structured metadata access and generic I/O are independent trait axes. A single adapter may run against any metadata source; a single metadata source may back any adapter. (`37 §1.1`, `00 §3`)
- **Step-0 `ConstraintValidator`** — structured Constraint evaluation at the start of every plan, emitting `Diagnostic`s. Callers catch Constraint violations before any strategy dispatch runs. (`11 §8.4`, `34 §*`)
- **Field-first resolution** — `Request.from = None` is now a first-class request shape; the planner maps Semantics to their owning DataKinds via Manifest indices and forms a `ComposedSemanticInterface` implicitly when fields span related DataKinds. (`16`, `34 §*`)
- **`ComposedSemanticInterface`** — unified interface surface arising from `Relationship` chains and from `ComplexDataKind` declarations (Unionset / Grainset / Joinset), carrying namespace-aware Semantics, per-field provenance, and a composition-kind tag. (`00 §4.1` `ComposedSemanticInterface` row, `16`)
- **`SemanticExpr` / `PhysicalExpr` newtypes over `Expr`** — context-enforcing invariants at the type level: `EntityRef` forbidden in `PhysicalExpr`, `Column` forbidden in `SemanticExpr`, no `Aggregate` in `PhysicalExpr`. Authors of computed Semantics and of column mappings pick up compile-time guardrails they previously had to enforce by convention. (`14 §4`, `31 §4.4`)
- **`CanonicalFn` newtype + `FunctionRegistry` in `semstrait-core`** — stable canonical function identities via `pub const` constants (`CanonicalFn::UPPER`, …), extensible by adapter-provided registries via the `RegistryExtension` trait. (`14a §2`, `14a §7`, `31 §5`, `31 §9`)
- **`Diagnostic`-shaped errors everywhere** — every public entry point surfaces structured `Diagnostic`s with stable codes. The `IntoDiagnostic` trait is the open, cross-crate conversion boundary. (`30 §5`, `30 §8.2`)
- **Warning propagation** — fail-fast stages carry accumulated `Info` / `Warning` diagnostics back alongside their primary output, in both success and failure arms. No warning is silently dropped. (`30 §7`)
- **Determinism (I4) contract** — Manifest bytes are byte-stable across `compile` for identical `(Model YAML, Catalog snapshot)`; `SemanticPlan` output is byte-stable per `(Manifest, Request)`. Content-addressable caching is supported. (`00 §9 I4`, `33 §14`, `40 §7.2`)
- **Capability flags on adapters** — `Cte`, `DistinctAggregate`, `AsOfJoin`, `GroupingSets`, `StructAccess` are first-class per-adapter declarations. Callers can query an adapter's supported feature set before planning. (`36 §4`)

### 3.5 Migration recipes

Concrete code and YAML transformations for the common patterns.

#### Recipe 3.5.1 — Reading a compiled Manifest

A consumer that previously walked `CompiledManifest::datasets` now reads `Manifest::resolved_datakinds`.

BEFORE:

```rust
fn list_dataset_names(mf: &CompiledManifest) -> Vec<String> {
    mf.datasets
        .iter()
        .map(|(name, _kind)| name.clone())
        .collect()
}
```

AFTER:

```rust
use semstrait_manifest::{Manifest, ResolvedDataKind};

fn list_dataset_names(mf: &Manifest) -> Vec<String> {
    mf.resolved_datakinds
        .iter()
        .filter_map(|(name, rdk)| match rdk {
            ResolvedDataKind::Simple(_) => Some(name.clone()),
            _ => None,
        })
        .collect()
}
```

Pitfall: `ResolvedDataKind` is `#[non_exhaustive]` per `30 §4.1`; the match above carries a wildcard arm. A match without a wildcard arm fails to compile in v1.0.

#### Recipe 3.5.2 — Migrating a custom `StorageProvider` implementation

BEFORE: a single `StorageProvider` impl that served both generic I/O and schema-ish reads.

AFTER: split into (a) a `FileSystem` impl for generic I/O, and (b) optionally a `CatalogProvider` impl when the underlying source provides structured metadata.

```rust
use semstrait_catalog::{FileSystem, CatalogProvider};

struct S3Fs { /* bucket, client, ... */ }

#[async_trait::async_trait]
impl FileSystem for S3Fs {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Diagnostic> { /* ... */ }
    async fn read(&self, path: &str) -> Result<Vec<u8>, Diagnostic> { /* ... */ }
    /* write / exists / ... */
}

struct IcebergCatalog { /* endpoint, auth, ... */ }

#[async_trait::async_trait]
impl CatalogProvider for IcebergCatalog {
    async fn schema(&self, table: &TableRef) -> Result<Schema, Diagnostic> { /* ... */ }
    async fn check_schema_drift(&self, mf: &Manifest) -> Result<DriftReport, Diagnostic> { /* ... */ }
    /* snapshot / partition / ... */
}
```

Pitfall: pre-1.0 code that peeked at file format headers through `StorageProvider` has no equivalent in `FileSystem` — format-aware schema reading was removed per `00 §4.1` `FileSystem` row. Use `CatalogProvider::schema` if the source is structured; otherwise rely on adapter-level schema inference.

#### Recipe 3.5.3 — Migrating a custom SQL template engine

BEFORE: string-templated SQL emission coupled to a bespoke template registry.

AFTER: implement the `EngineAdapter` trait per `36 §3`. Return `EngineArtifact::Sql(SqlArtifact { text, dialect })` from `adapt`, with `DialectId` picked from the ratified list.

```rust
use semstrait_adapter::{EngineAdapter, Dialect, EngineArtifact, SqlArtifact, DialectId};
use semstrait_ir::SemanticPlan;

struct MyDuckDbAdapter { /* ... */ }

impl EngineAdapter for MyDuckDbAdapter {
    fn adapt(&self, plan: &SemanticPlan) -> Result<EngineArtifact, Diagnostic> {
        let text = self.emit_sql(plan)?;
        Ok(EngineArtifact::Sql(SqlArtifact {
            text,
            dialect: DialectId::DuckDb,
        }))
    }
    /* capabilities / function overrides / ... */
}
```

Pitfall: capability flags (`Cte`, `DistinctAggregate`, `AsOfJoin`, `GroupingSets`, `StructAccess`) are now declared per-adapter per `36 §4`; an adapter that silently mistranslates an unsupported plan shape violates the new contract and should return `ADAPT_E_0300 UnsupportedFeature` instead.

#### Recipe 3.5.4 — YAML migration from split blocks to `data_kinds:`

BEFORE:

```yaml
datasets:
  - name: orders
    binding: { ... }

joinsets:
  - name: orders_with_customers
    associativity: LeftDriven
    datasets: [orders, customers]
```

AFTER:

```yaml
data_kinds:
  - name: orders
    kind: dataset
    binding: { ... }

  - name: orders_with_customers
    kind: joinset
    anchor: orders
    path:
      - ref: customers
```

Pitfall: during the shim window (one MINOR cycle), the parser accepts both shapes and emits `PARSE_W_*` on the legacy form. At v2.0 the shim is removed and the legacy form becomes `PARSE_E_*`.

#### Recipe 3.5.5 — Renaming `TemporalHistorization` → `TemporalShape`

BEFORE:

```rust
use semstrait_model::{TemporalHistorization, ScdType};

fn classify(h: &TemporalHistorization) -> &'static str {
    match h {
        TemporalHistorization::Events(_) => "events",
        TemporalHistorization::Snapshot(_) => "snapshot",
        TemporalHistorization::Scd(ScdType::Type2) => "scd2",
        _ => "other",
    }
}
```

AFTER:

```rust
use semstrait_model::{TemporalShape, ScdSubtype};

fn classify(s: &TemporalShape) -> &'static str {
    match s {
        TemporalShape::Events(_) => "events",
        TemporalShape::Snapshot(_) => "snapshot",
        TemporalShape::Scd { subtype: ScdSubtype::Type2, .. } => "scd2",
        _ => "other",
    }
}
```

Pitfall: `TemporalShape` carries per-subtype window-payload fields (`valid_from` / `valid_to` on history-preserving SCD variants) that `TemporalHistorization` did not carry uniformly. Review the SCD match arms against `17 §2.2`.

#### Recipe 3.5.6 — Renaming `LogicalPlan` → `SemanticPlan`

BEFORE:

```rust
use semstrait_ir::LogicalPlan;

fn debug_plan(plan: &LogicalPlan) { /* ... */ }
```

AFTER:

```rust
use semstrait_ir::SemanticPlan;

fn debug_plan(plan: &SemanticPlan) { /* ... */ }
```

A one-cycle `pub type LogicalPlan = SemanticPlan;` alias is in place; the alias carries a `#[deprecated]` attribute and is removed at v2.0.

#### Recipe 3.5.7 — Adopting `Diagnostic` at API boundaries

BEFORE:

```rust
let model = parse_yaml(text).map_err(|e| format!("parse failed: {e}"))?;
```

AFTER:

```rust
use semstrait_core::{Diagnostic, Severity};

let (model, warnings) = parse_yaml(text).map_err(|diags: Vec<Diagnostic>| {
    for d in diags.iter().filter(|d| d.severity == Severity::Error) {
        eprintln!("[{}] {}", d.code, d.message);
    }
    SomeCallerError::ParseFailed
})?;
for w in warnings {
    eprintln!("[{}] {}", w.code, w.message);
}
```

Pitfall: `Severity` is `#[non_exhaustive]`. Every match on `Severity` requires a wildcard arm per `30 §4.4`.

#### Recipe 3.5.8 — Adopting field-first resolution (`Request.from = None`)

BEFORE: every `Request` carried a `from:` target, possibly a best-guess.

AFTER:

```rust
use semstrait_api::{Request, plan};

let req = Request {
    from: None,
    select: vec!["orders.total_amount", "customers.country"],
    filters: vec![],
    /* ... */
    ..Request::default()
};
let (plan, warnings) = plan(&manifest, &req, &session)?;
```

Pitfall: if the requested Semantics span multiple DataKinds and no explicit `Relationship` chain exists, `plan` returns `PLAN_E_0501 NoImplicitCompositionPath`. If multiple paths tie, it returns `PLAN_E_0500 AmbiguousImplicitComposition` per `16 §11.4` — callers either narrow the Request or introduce an explicit Joinset.

#### Recipe 3.5.9 — Replacing `debug_sql` trait method with the free function

BEFORE:

```rust
let text = adapter.debug_sql(&plan)?;
```

AFTER:

```rust
use semstrait_adapter::debug_sql;

let text = debug_sql(&*adapter, &plan)?;
```

Pitfall: the free function delegates to `adapter.adapt(...)` and formats the resulting `SqlArtifact::text`. Adapters that produce `EngineArtifact::Plan(_)` return `ADAPT_E_0300 UnsupportedFeature` for the `debug_sql` request — check the artifact variant before calling.

#### Recipe 3.5.10 — Matching on `EngineArtifact` / `PlanArtifact` rename

BEFORE:

```rust
match artifact {
    PlanArtifact::Sql(s) => { /* ... */ }
    PlanArtifact::Substrait(p) => { /* ... */ }
}
```

AFTER:

```rust
use semstrait_adapter::{EngineArtifact, EnginePlan};

match artifact {
    EngineArtifact::Sql(sql) => { /* sql.text, sql.dialect */ }
    EngineArtifact::Plan(EnginePlan::Substrait(p)) => { /* ... */ }
    _ => { /* future variants; I10 requires a wildcard */ }
}
```

Pitfall: `EngineArtifact` and `EnginePlan` are both `#[non_exhaustive]` per `30 §4.1`. Do NOT use `unreachable!()` on the wildcard arm in library code — return a `Diagnostic` with `ADAPT_E_0300` instead per `30 §4.4`.

### 3.6 Required caller actions

Consolidated checklist for a v1.0 upgrade:

1. Rename every `Compiled*` import to `Resolved*` (or rely on the one-cycle `#[deprecated]` alias and land the rename ahead of v2.0).
2. Rename every `TemporalHistorization` / `ScdType` import to `TemporalShape` / `ScdSubtype`.
3. Rename every `LogicalPlan` import to `SemanticPlan`.
4. Rename every `StorageProvider` impl / consumer to `FileSystem`; if the pre-1.0 impl carried schema-aware reads, split into `FileSystem` + `CatalogProvider` per Recipe 3.5.2.
5. Rewrite every YAML file from split `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` blocks to the unified `data_kinds:` form with explicit `kind:` discriminators.
6. Update every `Relationship` YAML entry to the new field names (`from:` / `to:` / `cardinality:` / `join_type:` / `directionality:`).
7. Migrate every public API caller from raw-string / `anyhow::Error` error types to the `Diagnostic` / `Vec<Diagnostic>` shapes of `30 §7`; add wildcard arms to every `Severity` match.
8. For custom adapters: split `EngineProfile`-style dialect handling into a `Dialect` impl and carry `DialectId` on every `SqlArtifact`; declare capability flags per `36 §4`; replace `debug_sql` trait impls with reliance on the free function; retire any `PlanBuilder` usage.
9. For custom function registries: move from strings / bespoke enums to `CanonicalFn` constants; register adapter-specific functions via `RegistryExtension`.
10. Verify Manifest round-trip: `Repository::save` → `Repository::load` now preserves all state; content-addressable caches can key on the Manifest bytes directly.
11. Adopt `Request.from = None` wherever callers had previously supplied a best-guess target; accept that `PLAN_E_0500` / `PLAN_E_0501` / `PLAN_E_0502` replace ad-hoc "no plan" outcomes.
12. Eliminate every rustc `#[deprecated]` warning before upgrading to v2.0; all shims in `§3.3` are scheduled for removal there.

### 3.7 Automated migration aids

Round-1 position: **no scripted tooling is shipped for the v1.0 migration.** The motivation is cost/benefit — the rename matrix is large but each delta is mechanical, and the YAML shape changes require authorial review (children / anchors / path construction) that a naive rename would botch.

Machine-assisted rename suggestions are captured as an informal rename map that reviewers can feed into their editor's bulk-replace tooling. The authoritative mapping is always the table in `§3.2.2` above; the map below is a non-binding convenience.

```text
# Rust API — bulk rename map (non-binding)
CompiledManifest        -> Manifest
CompiledDataKind        -> ResolvedDataKind
CompiledSimpleKind      -> ResolvedSimpleKind
CompiledInterface       -> ResolvedInterface
CompiledSource          -> ResolvedSource
CompiledColumnMapping   -> ResolvedColumnMapping
StorageProvider         -> FileSystem
TemporalHistorization   -> TemporalShape
ScdType                 -> ScdSubtype
LogicalPlan             -> SemanticPlan
PlanArtifact            -> EngineArtifact
simplify                -> optimize
```

`cargo-fix` support, a rustfmt-rules preset, or a `syn`-based codemod are all deferred; if a community-contributed codemod lands, it will be linked from this section in a future revision.

YAML transformations are tracked in `open_questions/42_open_questions.md` as `Q-42-003` (whether a scripted YAML migrator is worth the maintenance cost).

---

## 4. v1.1+ placeholders

No MINOR releases have been cut after v1.0 as of Round-1 freeze. The placeholders below are a **format template**: each future MINOR that carries a notable addition appends a section in this form, even if no break lands.

### 4.1 Template for an additive MINOR

```
## §4.N vX.Y

### §4.N.1 Summary
  One sentence: what the MINOR adds.

### §4.N.2 Notable additions
  Bullets: new public symbols, new `#[non_exhaustive]` variants, new default-bodied
  trait methods, new YAML keys, new error codes within reserved ranges.

### §4.N.3 Match-arm maintenance
  Every new `#[non_exhaustive]` variant requires consumer code to carry a
  wildcard arm per `30 §4.4`. List the affected types here.

### §4.N.4 Deprecations introduced (if any)
  Table format of `§3.3`; each row references its `41` tombstone.

### §4.N.5 Caller actions
  "No action required" is acceptable. If action is required, short checklist.
```

### 4.2 Anticipated v1.1 additions (indicative, non-binding)

These are forecast based on the `[TD-*]` roster of `40 §3`. Actual scheduling is a release-management concern.

- New `CanonicalFn::*` constants registered in `semstrait-core::function_registry()` as the canonical catalog grows.
- New `DialectId` variants as adapters land.
- New `PARSE_W_*` advisories as author-visible lint checks mature.
- Additional `FunctionSpec` entries per `[TD-REGISTRY-ALIASES]`, `[TD-REGISTRY-SUBCATEGORY]`.

### 4.3 Anticipated v1.2 additions (indicative, non-binding)

- `AsOf` planner-side support lands (closes `[TD-COMPOSITION-ASOF]`). Vocabulary is already present in v1.0; planner emission is the v1.2 add. Caller-visible consequence: `JoinType::AsOf` starts producing real `SemanticPlan` output in place of `PLAN_E_*` stubs.
- Incremental Manifest recompile (`[TD-MANIFEST-INCR-CACHE]`) if the caching substrate lands. Caller-visible consequence: `compile` may return a cached `Manifest` when the `(Model, Catalog)` pair is unchanged.

Neither is committed. Both are tracked in `40 §3` and will be re-scoped when they enter a phase plan.

---

## 5. Post-v1 deferred-features milestone plan

The following features are **vocabulary-ratified in v1.0** or **flagged as DEFERRED** in the relevant design doc. This section commits each to a target MAJOR and enumerates the caller-visible consequences. Scheduling is indicative; actual delivery is release-management's concern.

### 5.1 v2.0 candidates

| Feature | Current v1.0 posture | v2.0 delivery |
|---|---|---|
| Planner support for `AsOf` join execution | Vocabulary-ratified in `17 §5`; planner emission DEFERRED. A `Request` that requires as-of semantics returns `PLAN_E_*` with a clear "as-of DEFERRED" diagnostic. | Planner wires `JoinType::AsOf` into the strategy dispatch. Each SQL adapter grows a per-dialect emission path; Substrait uses an extension anchor per `[TD-ADAPTER-SUBSTRAIT-ASOF]`. |
| `SCD Type2` wide-composition as hard error | Advisory `PLAN_W_*` per `17 §8` (the planner warns but proceeds with the best-effort plan). | Promoted to `PLAN_E_*` hard error. Callers that relied on the best-effort plan must explicitly declare the composition via a Joinset or narrow the Request. |
| Removal of every v1.0 deprecation shim | Shims listed in `§3.3` remain live in v1.x MINORs. | All shims removed (see `§3.3` target-removal column). Callers still using the deprecated symbols will see hard compile errors at v2.0. |
| Shim removal for the legacy YAML grammar | Parser accepts both forms during v1.x with `PARSE_W_*` on the legacy shape. | Legacy grammar is rejected with `PARSE_E_*`. YAML corpora must be fully migrated before v2.0. |

Caller guidance: the pre-v2.0 window is the window to drain deprecation warnings. Every rustc `#[deprecated]` warning and every `PARSE_W_*` legacy-grammar warning is a v2.0 compile error.

### 5.2 v2.x candidates (non-MAJOR)

These land in provisional-crate MINORs or in workspace MINORs after v2.0:

| Feature | Tag | Delivery notes |
|---|---|---|
| Incremental Manifest recompile | `[TD-MANIFEST-INCR-CACHE]` | Additive surface on `semstrait-manifest`; caller-visible only as performance. No break. |
| `[TD-14B-EXPR-INTERN]` opt-in `PhysicalExpr` interning | `[TD-14B-EXPR-INTERN]` | Behind a Cargo feature flag; no default-surface change. |
| `[TD-GRAIN-NON-TEMPORAL]` non-temporal Grain (geographic, entity) | `[TD-GRAIN-NON-TEMPORAL]` | Additive variants on `Grain`; MINOR under `30 §2.2`. |
| N-ary Joinsets / nested Grainset | `[TD-JOINSET-NARY]`, `[TD-GRAINSET-NESTED]` | Additive under `#[non_exhaustive]`; YAML surface gains optional keys. |
| Semi / Anti JoinType variants | `[TD-COMPOSITION-SEMI-ANTI]` | Additive enum variant; adapter emission per-dialect. |
| Bi-temporal shape (`valid_time` + `system_time`) | `[TD-BITEMPORAL]` | Additive `TemporalShape` variant; planner-side gating. |

### 5.3 v3.0 candidates

| Feature | Source | Notes |
|---|---|---|
| MetricFlow-style conversion metrics, cumulative metrics as request-time constructs | `00 §10` (DEFERRED) | Currently expressible only through Manifest-time `Metric` declarations. v3.0 is the earliest a request-time algebra would ratify; requires a new foundations doc beyond the `1x` map. |
| Request-level ratio metrics | `00 §10` | Same delivery mechanism as conversion / cumulative. |
| Optional cost-based optimization | `00 §10` | `optimize` gains a statistics-driven branch; requires a separate statistics-source trait axis beyond `CatalogProvider`. |

All v3.0 items are speculative at Round-1 freeze and depend on follow-up design work. Each will land its own foundations doc before it enters a phase plan.

---

## 6. Round-1 open items

Parked in `docs/design/open_questions/42_open_questions.md`. Items in scope for this doc are **documentation-format / communication-format** questions only; any item that re-opens a ratified design decision belongs in the originating doc's open-questions file.

| # | Title |
|---|---|
| Q-42-001 | Per-MAJOR entry ordering: newest-first vs oldest-first |
| Q-42-002 | Coverage for provisional-crate-only MINORs (full section vs bullet) |
| Q-42-003 | Scripted YAML migrator for the v1.0 legacy-grammar → `data_kinds:` transformation |
| Q-42-004 | Rendering recipes: Rust snippets vs code-reference citations into the workspace |
| Q-42-005 | Retired-error-code rendering format (once any retirement occurs post-v1.0) |
| Q-42-006 | Cross-linking `42` entries from `CHANGELOG.md` (anchor scheme) |
| Q-42-007 | Caller-action checklist: single consolidated list vs per-crate checklists |

---

*Cross-references in this document are by section (e.g. `00 §9`, `30 §6.2`, `40 §5.3`). No code-path references are used, per `00 §8`. Code snippets in `§3.5` recipes are illustrative Rust / YAML and are not CODE-REFERENCES into the workspace.*
