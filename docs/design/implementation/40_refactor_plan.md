---
prereqs:
  - 00
  - foundations/10-17 (all)
  - data-kinds/20-25 (all)
  - apis/30-39 (all)
  - legacy docs/TECH_DEBT.md
  - DECISION_LOG.md
authoritative-for:
  - phased refactor roadmap from current code ("before") to ratified design ("after")
  - deviation register organized by crate (§2)
  - milestone / phase naming policy (§5)
  - inventory of `[TD-*]` tags aggregated across the design tree (§3)
  - inventory of `[CODE-DIVERGES-FROM-SPEC]` flags (§4)
  - deprecation-pipeline entry contract (the doc forwards to `41_deprecations.md`; §9)
  - cross-phase breaking-change discipline: when a MAJOR is justified, how MAJORs are sequenced, what shim surface is permitted (§6)
  - per-phase rollback discipline and feature-gate policy (§8)
  - per-phase testing exit criteria (§7)
refined-by:
  - 41 (`implementation/41_deprecations.md` — per-symbol deprecation / retirement tombstones)
  - 42 (`implementation/42_migration_notes.md` — per-version user-facing migration notes)
---

# 40. Refactor Plan

> **Status:** ratified Round-1 sequencing. This is the **first** document under `implementation/`. Per `00 §9 I9`, the current code is the "before" state, the design docs are the "after" state, and this document catalogs the delta and orders the refactor. No section of this document re-opens a ratified design decision; every divergence between code and design is a migration task.

## 1. Purpose and Scope

### 1.1 Who reads this doc

Implementers. Reviewers sequencing a multi-PR refactor. Agents that need to know which phase a proposed change belongs to. Not YAML authors, not end users of the engines semstrait targets.

### 1.2 What this doc is

`40` is the **migration contract** from the code shipped at the start of the design exercise to the code the design tree describes. It:

- Enumerates the **delta** between "before" and "after" at the crate level (§2).
- Aggregates every `[TD-*]` tag mentioned in any ratified design doc into a single inventory (§3).
- Aggregates every `[CODE-DIVERGES-FROM-SPEC]` flag into a parallel inventory (§4).
- Orders the work into **phases** with exit criteria, owning crates, risk level, and dependency chain (§5).
- Fixes the **breaking-change discipline** governing MAJOR sequencing across phases (§6).
- Fixes the **testing exit gates** (§7), **rollback triggers** (§8), and the **deprecation pipeline** (§9).

### 1.3 What this doc is NOT

`40` is **not** design. It does not override, relax, or re-litigate any ratified decision in `0x`–`3x`. A reader who disagrees with a design call must amend the relevant design doc (per `00 §8`'s directionality rule) and only then update this plan.

`40` is **not** per-PR scheduling. It fixes phase boundaries and dependency chains; per-PR splits inside a phase are a PR-author concern.

`40` is **not** implementation-detail specification. It does not name functions, module paths, internal types, or specific branches. The design docs already fix public surfaces; this plan sequences the work that implements them.

### 1.4 Contrast with sibling `implementation/` docs

| Doc | Scope |
|---|---|
| `40_refactor_plan.md` (this) | **Forward-looking phased plan.** What work is required and in what order. Updated as phases open / close / reorder. |
| `41_deprecations.md` | **Per-symbol deprecation tombstone.** One entry per symbol that ever gains a `#[deprecated]` attribute, tracking `since` / replacement / target-removal version. Append-only; entries move into `42` when the symbol is finally removed. |
| `42_migration_notes.md` | **Per-version user-facing migration notes.** Released alongside every MAJOR (and MINOR that carries provisional-crate breaks per `30 §13`). Before / after examples, replacement guidance. |

`40` owns sequencing and scope. `41` owns the deprecation roster. `42` owns the user-visible release notes.

### 1.5 Layering posture

This doc is layered strictly below `00` / `1x` / `2x` / `3x` — it may reference anything ratified above, and may not carve exceptions. It is layered **above** `41` and `42`, which refine the deprecation-roster and migration-note surfaces this doc describes at a policy level.

## 2. The Delta — per-crate deviation catalog

Each subsection fixes, for one crate: (a) what the current code substantially matches in the ratified design, and (b) where it diverges. "Diverge" is intentionally neutral — it does not imply the code is wrong, only that it is not yet the after state. Granular references are to the design section that ratifies the after state; if a ratified section disagrees with the summary here, the ratified section wins.

### 2.1 `semstrait-core`

**Matches the design.**

- Zero internal-workspace dependencies (matches `30 §13` stable-in-v1 posture).
- Central home for `DataType`, `Grain`, `Expr`, `Schema`, error primitives (matches `31 §2`).
- `Expr` already unified across IR / planner / adapter per `DECISION_LOG.md DL-020` (matches `14 §4`'s single-AST rule and I1).
- `DataType` reduced to logical variants per `DECISION_LOG.md DL-044` (matches `13 §2.1` / `31 §4.1` in spirit).

**Diverges from the design.**

- `DataType` variant list is smaller than the canonical 14-variant set ratified in `13 §2.1` / `31 §4.1` — `Byte`, `Short`, `Integer`, `Long`, `Float`, `Double`, `Time { precision }`, `Interval`, `Binary` are not yet enumerated (`32 §14.4` row 4). Variant additions are MINOR per `30 §2.2`; this is additive migration.
- No first-class `SemanticExpr` / `PhysicalExpr` newtypes over `Expr`; code uses bare `Expr` everywhere. `14 §4` / `31 §4.4` mandate the two-newtype discipline with context-enforcing invariants (`Column` forbidden in `SemanticExpr`; `EntityRef` forbidden in `PhysicalExpr`; no `Aggregate` in `PhysicalExpr`).
- No `CanonicalFn` newtype + `pub const` identity pattern; current code uses strings (and a small closed enum in places). `14a §2` / `31 §5` mandate the newtype-with-constants pattern, Copy + Eq + Hash ergonomics, and crate-private construction.
- No `FunctionRegistry` exposed as a `&'static` process-global with a sealed initializer (`14a §2.1` / `31 §9`). Current code carries a local registry inside `semstrait-manifest` (see §2.3).
- No `Diagnostic` / `Severity` / `Span` / `ContextLine` canonical types exposed from core (`30 §5` authoritative; `31 §6` hosts the types). Current code surfaces errors through per-stage enums without a cross-stage Diagnostic shape.
- Serde derives are unconditional on core types; `31 §11` requires `#[cfg(feature = "serde")]` gating (`[TD-CORE-SERDE-GATING]`).
- `Grain` is temporal-only today; extensibility hook for non-temporal grain (geographic, entity) is future (`TD-GRAIN-NON-TEMPORAL` — not a v1 blocker).

### 2.2 `semstrait-model`

**Matches the design.**

- YAML parsing via serde_yaml; hand-written `Deserialize` for `ExprSource` per `DECISION_LOG.md DL-049` (matches `32 §6` / `32 §3.3` declarative-block handling).
- `Relationship` lives as a top-level Model entry (matches `16 §2.1` / `32 §7`).
- `TemporalHistorization` + `ScdType` vocabulary present end-to-end (matches `17 §2`'s intent — subject to the rename).

**Diverges from the design.**

- Top-level blocks are split as `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` with **implicit kind** inferred from the containing block (`32 §14.4` row 1 / explicit `[CODE-DIVERGES-FROM-SPEC]` at `32 §15`). `32 §4.2` ratifies a single `data_kinds:` block with a `kind:` discriminator.
- `ChildEntry` / `datasets:` sub-block on complex kinds → `children:` with `ref:` entries (`32 §14.4` row 2).
- `YamlJoinset.associativity: JoinAssociativity` → `JoinsetSpec.anchor` + `JoinsetSpec.path: JoinPath` per `24 §2.2` / `32 §5.4` (`32 §14.4` row 3).
- `ColumnMappingValue` variant names diverge from the ratified `{ Column, Literal, Computed, Metadata }` set (`32 §14.4` row 4); the `Anchored` variant collapses into `Computed` (anchored composition is a computed expression).
- `DataType` enum in model references the old 11-variant set (`32 §14.4` row 5); it must defer to `semstrait-core::DataType` per `31 §4.1`.
- `TemporalGrain` locally defined → must resolve to `semstrait-core::Grain` (`32 §14.4` row 6).
- `Relationship` shape uses `relationship_type:` / `source_set:` / `target_set:` → `from:` / `to:` / `cardinality:` / `join_type:` / `directionality:` per `16 §2.1` / `32 §7` (`32 §14.4` row 7).
- `ModelError` collapsed into `ParseError` + `ValidateError` per `31 §8` / `32 §10.2` (`32 §14.4` row 8).
- `TemporalHistorization` symbol → `TemporalShape` per `00 §4.3` banned-terms table (rename-only; `17 §1.3` records the trajectory).
- `MeasureConstraints` reused across Measures and Metrics (`11 §8.4.3`); rename deferred to the broader Manifest-schema revision pass (`[TD-CONSTRAINT-RENAME]`).
- No Diagnostic-shaped error surface on the public API; error types are raw Rust enums.
- Directory-loader helper (`parse_dir`) absent (`[TD-MODEL-DIR-LOADER]`); out-of-scope for v1.
- `serde_yaml` in upstream maintenance mode; migration tracked as `[TD-MODEL-YAML-CRATE]`.

### 2.3 `semstrait-manifest`

**Matches the design.**

- Distinct compile-time (async permitted) and repository-persist (async permitted) surfaces with everything else synchronous (matches `33 §1` / I11).
- `CompiledManifest` carries `data_kinds: IndexMap` per `DECISION_LOG.md DL-063` (matches `33 §3`'s single-container posture).
- Dedicated `CompiledSimpleKind` fast path for single-dataset kinds (`DECISION_LOG.md DL-064`); spiritually matches `21`'s simple-path discipline.
- `CatalogSnapshot` as drift boundary (`DECISION_LOG.md DL-065`) matches `33 §12` / `37 §4`'s catalog-contract posture.
- I/O out of the `plan` hot path (matches I11 / `33 §11`).

**Diverges from the design.**

- Naming: `Compiled*` prefix on every resolved manifest type (`DL-062`). `00 §4.1` and `00 §4.3` banned-terms table rename this to `Resolved*` (`ResolvedDataKind`, `ResolvedInterface`, `ResolvedSource`, `ResolvedColumnMapping`). This is the single largest naming churn in the refactor.
- No `ResolvedExprTable` as a compile-time-populated, per-`(Semantics name, Binding id)` → `PhysicalExpr` pre-computation (`14b §2` / `33 §6`). Current code re-walks expressions at plan time through per-dataset pathways.
- Expression resolution does not pre-walk cross-DataKind paths via `Relationship`s; current code resolves path at plan time (`14b §5`).
- `SemanticGraph` marked `#[serde(skip)]` (`TD-002`), loses shape on roundtrip. Parallel: `KindRef.variant` resets on deserialize (`TD-001`). `33 §14.1` requires the Manifest to be byte-stable round-trippable via `Repository::{save, load}`; these tech-debt items block that property.
- `RelationshipGraph` + `FieldIndex` both still in the planner's hot path despite being deprecated by `SemanticGraph` (`TD-007`); the ratified Coverage + Composition indices (`33 §7` / `33 §8`) have no mapping yet.
- Coverage indices live at the Binding level only (`15`); the ComposedSemanticInterface-level Coverage ratified in `16 §9` is not materialized.
- Index set is ad-hoc; the ratified deterministic name / coverage / relationship indices (`33 §7`) are absent.
- Deterministic serialization form not contractual — Manifest bytes are not I4-stable. `33 §14` ratifies MessagePack + JSON encoders; bincode reserved (`[TD-33-BINCODE]`).
- `io.rs` living in `semstrait-manifest` per `TD-008`; per `33`'s narrower scope, the I/O utility will move (trigger: three or more I/O utilities per `TD-008`).
- `load_text()` is a generic text loader (local + S3); not manifest-specific. Extract to `semstrait-io` when three or more I/O utilities accumulate (per `TD-008`).
- `FunctionRegistry` lives in `semstrait-manifest`; must move to `semstrait-core` per `14a §2` / `31 §9`.
- `CompileError` not unified with `semstrait-core::CompileError` shared variants (`[TD-33-ERROR-UNIFY]`).

### 2.4 `semstrait-planner`

**Matches the design.**

- Per-DataKind strategy dispatch exists (`simple` / `grainset` / `unionset` / `joinset`), aligning with the `34`-implied strategy-trait posture (`00 §4.2`'s "internal detail, not a vocabulary-level verb").
- Synchronous `plan()` entry (`DL-013`) matches I6 / I11.
- Optimizer is empty by default with an `OptimizerPass` extension seam (`DL-041` / `DL-042`) matches `34`'s "rule-based, extension-friendly" posture.

**Diverges from the design.**

- No step-0 **Constraint validator** as a first-class plan step (`11 §8.4` / `34 §*`). Constraint checks are currently scattered; the ratified plan-time step-0 discipline mandates a single validator pass per-`Request` against all realized Constraint carriers with structured `Diagnostic` output.
- Constraint-error fan-out is a single-variant message today (per `10 §5`'s `ConstraintViolation`). Typed per-rule enum fan-out deferred as `[TD-CONSTRAINT-ERROR-FANOUT]`.
- No **field-first resolution** — current code requires a `Request.from` target (or a heuristic replacement). `34` / `16` ratify field-first: when `Request.from = None`, the planner maps requested Semantics to their owning DataKinds via Manifest indices and — if Semantics span multiple DataKinds — traverses the top-level Relationship graph to form a `ComposedSemanticInterface`.
- Implicit composition depth limit not enforced. `16 §9.1` ratifies `MAX_IMPLICIT_COMPOSITION_DEPTH = 4`; planner has no constant (see `questions/open/16` `Q-COMP-001`).
- Ambiguous-path tie → error discipline (`16 §11.4`) not materialized; current code is non-deterministic in edge cases.
- `AdditivityResolver` is a v1 stub (`DL-058`) — semi/non-additive Measures do not restructure plans; `34`-era planner work must reconcile with `11 §7`'s Additivity classification.
- `SCDRollupWithoutAsOf` and Unionset cross-shape error surface (`22 §5`, `23 §6.3`) not yet emitted.
- Planner consumes `RelationshipGraph` + `FieldIndex` (deprecated per `TD-007`) rather than the Manifest's authoritative indices.
- `Request.from = None` not accepted; field-first resolution is the blocker.
- `Request.temporal` block is DEFERRED per `17 §10`; no vocabulary present in code today.
- Strategy trait is informal — per-DataKind dispatch happens via module routing, not a named trait with a stable surface (`34`-ratified shape is the open-item).
- `Planner::cost` hook (`22 §4.4` / `34`) absent; stats-backed cost is `[TD-GRAINSET-COST-STATS]`.
- `Grainset` partial-coverage fallback error vs split-and-delegate is not materialized (`[TD-GRAINSET-PARTIAL-COVERAGE]`).
- `active_bindings()` allocates every call (`TD-006`); non-blocking.
- Expect-panic messages (`TD-004`) assume compile guarantee — programming-error panics; re-phrase when touched.

### 2.5 `semstrait-ir`

**Matches the design.**

- `Expr` reuses the shared `semstrait-core::Expr` (matches `35 §4`'s "no ad-hoc IR expression types" posture).
- Substrait emission present (matches `35 §5` / `DL-004`).
- Schema type lives here (matches `35 §3` scope).

**Diverges from the design.**

- **Top-level plan type named `LogicalPlan`**, not `SemanticPlan` (`35 §3.1` / `[TD-IR-RENAME]`). Rename is a MINOR via `pub type LogicalPlan = SemanticPlan;` transition kept one MINOR cycle.
- `PlanNode` variants not uniformly `#[non_exhaustive]` (`[TD-IR-NONEXHAUSTIVE]`). `00` I10 / `30 §4.2` mandate across the board.
- `JoinNode.on: Vec<KeyPair>` without reserved non-equi `residual` field (`35 §4.6` / `[TD-IR-NON-EQUI-JOIN]`).
- Aggregate filter carriage not uniform; some code paths lower to `Case` rather than preserving `filter: Option<PhysicalExpr>` (`[TD-IR-AGG-FILTER]`).
- Plan-layer `Schema` is its own type, not shared with Manifest-layer `Schema` (`[TD-IR-SCHEMA-SHARING]`). Sharing is a future consolidation; not a v1 blocker.
- Error codes use the old prefix/range scheme; `IR_E_3500`–`3599` reserved per `35 §10.2` with `[TD-IR-CODE-TABLE-AMEND]` against `30 §6.2`.
- `JoinType::AsOf` variant not yet in the code-level enum (vocabulary ratified in `17 §5`; `[TD-COMPOSITION-ASOF]` tracks planner emission).

### 2.6 `semstrait-adapter`

**Matches the design.**

- `EngineAdapter` trait routes per-engine emission (matches `36 §2`).
- `EngineAdapter` has-a `EngineProfile` per `DL-055` (structurally close to `36 §2`'s dialect machinery).
- `PlanArtifact` (SQL / Substrait) matches `36 §3`'s `EngineArtifact` intent (`DL-023`).
- Feature-gated per-engine deps (`duckdb`, `spark`, `datafusion`) matches `30 §10`.

**Diverges from the design.**

- Adapter surface naming / errors have not absorbed the `36` renames (`[TD-ADAPTER-RENAME]`, `[TD-ADAPTER-ERROR-MIGRATION]`).
- `Dialect` split from `EngineAdapter` is informal (`[TD-ADAPTER-DIALECT-SPLIT]`); `36 §2` ratifies `Dialect` as its own axis.
- `debug_sql` is a trait method (with default impl); `36 §3.5` ratifies it as a free function (`[TD-ADAPTER-DEBUG-SQL-FREE-FN]`).
- `PlanBuilder` construction path (`DL-030`) will need retirement for the Substrait-side path (`[TD-ADAPTER-PLAN-BUILDER-RETIRE]`) — mechanism TBD with the adapter rewrite.
- Substrait function-anchor URN override hook absent (`[TD-ADAPTER-SUBSTRAIT-ANCHOR]`).
- No `JoinType::AsOf` emission — Substrait has no standard as-of; per-engine path via extension is `[TD-ADAPTER-SUBSTRAIT-ASOF]`.
- `DialectId` carriage not on every `SqlArtifact` (the carry-the-dialect contract is `36 §3.2` / `00 §4.1` `SqlArtifact` row).
- Capability flags (`Cte`, `DistinctAggregate`, `AsOfJoin`, `GroupingSets`, `StructAccess`) not a first-class per-adapter declaration (`36 §4`). Emission-time coverage is ad-hoc.
- `SafeDivide` → `Divide` Substrait round-trip (`DL-024`) is a SQL-emission-only concern today; planner-level sequencing of the rewrite is an adapter-owned concern per `36`.

### 2.7 `semstrait-catalog`

**Matches the design.**

- `CatalogProvider` trait for structured metadata sources (matches `37 §2`).
- Local-filesystem + S3 + Iceberg + Unity impls separated by feature flags (matches `37`-era per-provider split intent).
- I/O confined behind traits (matches I11 / `37 §6`).
- Schema drift runs against `CatalogSnapshot` (`DL-037`), aligning with `37 §4`'s narrow-drift posture.

**Diverges from the design.**

- `StorageProvider` legacy name → `FileSystem` per `00 §4.3` banned-terms table.
- `CatalogProvider` method roster does not match `37 §3`'s ratified surface (schema, snapshot, partition, drift-check granularity).
- Schema drift today covers `manifest.datasets` only; kind-bound datasets have no `compiled_schema` (`DL-037`). Extending to cover every resolved source per `37 §4` is future work.
- `FileSystem` and `CatalogProvider` are not fully independent axes in the code; `37 §1.1`'s orthogonality contract is the target.
- `CAT_E_*` / `FS_E_*` error-code tables are proposed against `30 §6.2` but not yet registered (`[TD-CAT-CODE-TABLE-AMEND]`).
- Glob-matching home (semstrait-core vs semstrait-catalog) is `questions/open/37` `Q-CAT-002`.
- Per-provider error-variant fan-out does not yet match `37 §8`'s proposed `CAT_E_*` / `FS_E_*` ranges.

### 2.8 `semstrait-api` / `semstrait` (facade)

**Matches the design.**

- `semstrait-api` with feature-gated `grpc` / `rest` / `cli` transports (`DL-040`) aligns with `38 §2`'s one-crate-with-feature-gates posture.
- `semstrait` facade crate exists as a thin wrapper for one-shot use (`DL-059`) — aligns with `39 §1` / `39 §2`.
- Facade bridges planner + adapter: `with_adapter()` on facade; `with_profile()` on planner (`DL-059`) — aligns with `39 §3`.

**Diverges from the design.**

- Public surfaces not yet aligned with `38` (API entry point) and `39` (facade) rosters — diff is structural (signatures, re-export set, error surface).
- Warning propagation (`Result<(Output, Vec<Diagnostic>), (Error, Vec<Diagnostic>)>` per `questions/open/30` `Q-API-002`) not materialized.
- Diagnostic-shaped error surface absent on public entry points (`30 §5` / `I12`).
- CLI entry-point crate split (`semstrait-cli`) if ever extracted sits under `38 §10`; today is flat under `semstrait-api`.

## 3. Known `[TD-*]` tags — inventory

Aggregated from every ratified design doc under `docs/design/`. This roster is the authoritative index of deferred work surfaced by the design exercise. Each entry names the tag, the source doc / section, and a one-line description. When an item lands, its row moves to `42_migration_notes.md` with a completion cross-reference; this inventory keeps a tombstone.

Legacy `docs/TECH_DEBT.md` entries (`TD-001`…`TD-009`) are retained at the bottom with their pre-design-exercise provenance; they will be normalized into the `[TD-*]` style and moved into the main table as each is either absorbed into the design (ratified) or closed by phase work.

### 3.1 Core / registry

| Tag | Source | One-liner |
|---|---|---|
| `[TD-REGISTRY-MULTI-CONFIG]` | `14a §2.1`, `31 §9` | Per-invocation registry configurability (v1 is process-global static). |
| `[TD-REGISTRY-ALIASES]` | `14a §2.3` | Spec-level alias support on canonical functions. |
| `[TD-REGISTRY-DETERMINISM]` | `14a §3.1` | `deterministic: bool` flag to drive optimizer constant-folding. |
| `[TD-REGISTRY-SUBCATEGORY]` | `14a §3.2` | Scalar sub-categorization (`String` / `Math` / …). |
| `[TD-REGISTRY-TYPECLASS]` | `14a §3.3`, `31 §5.5` | TypeClass-parameterized generic signatures. |
| `[TD-REGISTRY-MID-VARIADIC]` | `14a §3.5` | Mid-signature repeated params. |
| `[TD-REGISTRY-BINOP-LATTICE]` | `14a §5.2` | Canonical BinaryOp promotion lattice. |
| `[TD-REGISTRY-EXTENSION-WIRING]` | `14a §7.1` | Aggregation mechanism for adapter `FUNCTIONS` arrays. |
| `[TD-CORE-SERDE-GATING]` | `31 §11` | Gate `serde` derives behind feature flag on core. |
| `[TD-GRAIN-NON-TEMPORAL]` | `13 §3.4`, `17 §1.2` | Non-temporal `Grain` (geographic, entity). |

### 3.2 Expressions / foundations

| Tag | Source | One-liner |
|---|---|---|
| `[TD-14B-EXPR-INTERN]` | `14b §2`, `14b_questions §Q-14B-01` | Opt-in `PhysicalExpr` interning for large Manifests. |
| `[TD-14B-PATH-UNIFICATION]` | `14b §5` | Canonicalization of multi-path resolution. |
| `[TD-14B-EXPR-PROVENANCE-SITES]` | `14b §2.6` | Per-`EntityRef`-site provenance trails. |
| `[TD-14B-BATCH-DIAGS]` | `14b §7` | Multi-error aggregation mode for resolution cycles. |
| `[TD-14B-RELATIONSHIP-ROLE-HINTS]` | `14b §6` | Role-hints at `EntityRef` when multiple Relationships coexist. |
| `[TD-14B-TYPECLASS-UNIFY]` | `14b §10` | Richer unification gated on `[TD-REGISTRY-TYPECLASS]`. |
| `[TD-EXPLAIN-COMPILED]` | `14b §2.6` (several), `questions/open/14b` | `--explain`-style tooling over compiled artifacts. |

### 3.3 Names and scopes / constraints

| Tag | Source | One-liner |
|---|---|---|
| `[TD-REQUIRES-MECHANISM]` | `11 §8.5.2`, `11 §9` | Filter-injection mechanism (`requires:`). |
| `[TD-AGG-ON-METRIC]` | `11 §8.7` | Hard-reject vs silent-skip on `aggregations:` for Metric without `agg:`. |
| `[TD-CONSTRAINT-RENAME]` | `11 §8.4.3`, `31 §6.1` | `MeasureConstraints` → cross-carrier-neutral name. |
| `[TD-CONSTRAINT-ERROR-FANOUT]` | `11 §8.7`, `10 §5.1` | Typed per-rule `ConstraintError::*` variants. |
| `[TD-CARDINALITY-CONSTRAINT]` | `11 §8.5.4` | DataKind `row_count:` / `null_fraction:` gated on CatalogProvider stats. |
| `[TD-MANIFEST-INCR-CACHE]` | `11 §13` | Incremental-compile caching. |

### 3.4 Composition

| Tag | Source | One-liner |
|---|---|---|
| `[TD-COMPOSITION-JOINSET-REUSE]` | `16 §13.5` | Implicit-composition reuse of existing `Joinset` surfaces. |
| `[TD-COMPOSITION-SELFJOIN]` | `16 §12.4` | Self-referential Relationships (forbidden in v1). |
| `[TD-COMPOSITION-SEMI-ANTI]` | `16 §4.3` | `JoinType::Semi` / `Anti` variants. |
| `[TD-COMPOSITION-ASOF]` | `16 §4.4.2`, `17 §5` | Planner emission of `JoinType::AsOf` (vocabulary-only in v1). |
| `[TD-JOINSET-NARY]` | `16 §13.2`, `24 §13` | N-ary Joinsets. |
| `[TD-NESTING-NARY-JOIN]` | `12 §5.2` | Nesting-policy-level n-ary-join gate. |
| `[TD-JOINSET-HYBRID-PATH]` | `24 §7.2`, `questions/open/24` | Partial explicit + implicit join-path hybrid. |

### 3.5 Temporal shape

| Tag | Source | One-liner |
|---|---|---|
| `[TD-SCD-TYPE4-HISTORY-REF]` | `17 §2.2`, `17 §10 D7` | `Type4`'s history-table DataKind ref surface. |
| `[TD-SCD-TYPE4-ASOF]` | `17 §5.2`, `17 §10 D7` | As-of traversal on `Type4`. |
| `[TD-BITEMPORAL]` | `17 §1.2`, `17 §10 D14` | Bi-temporal `valid_time` + `system_time` shape. |

### 3.6 DataKind variants

| Tag | Source | One-liner |
|---|---|---|
| `[TD-GRAINSET-NESTED]` | `22 §3.4`, `25 §2.4` | Grainset-of-Grainset nesting. |
| `[TD-GRAINSET-PARTIAL-COVERAGE]` | `22 §4.2` | Cross-child column-wise partial coverage split. |
| `[TD-GRAINSET-COST-STATS]` | `22 §4.4` | Stats-backed cost function. |
| `[TD-GRAINSET-MERGE]` | `22 §4.5` | Compile-time merging of author-written Grainsets. |
| `[TD-UNIONSET-SHAPE-PLANNING]` | `23 §6.4`, `25 §2.9` | Planner-side shape rewrite for Unionset branches. |
| `[TD-UNIONSET-CODERANGE]` | `23 §8` | `2300`–`2399` per-DataKind code allocation reconciliation with `30 §6.2`. |
| `[TD-UNIONSET-DISTINCT-SEMANTICS]` | `23 §4.3` | `Distinct` subsection. |
| `[TD-UNIONSET-AVG-WEIGHTED]` | `23 §5`, `questions/open/23` | Weighted-average promotion. |
| `[TD-UNIONSET-STRICT-SHAPES]` | `23 §6.1` | Strict shape reconciliation for union branches. |
| `[TD-UNIONSET-FUTURE-MODES]` | `23 §4.1` | Additional `UnionMode` variants beyond `All` / `Distinct`. |
| `[TD-UNIONSET-SINGLE-CHILD]` | `23`, `questions/open/23` | Single-child Unionset accept vs reject (asymmetry with Grainset). |
| `[TD-UNIONSET-DERIVED]` | `23` / `questions/open/23` | Unionset-level derived-Semantics declarations. |
| `[TD-UNIONSET-AGG-COLLAPSE]` | `23`, `34` | Optimizer pass for aggregate collapsing across Unionset branches. |

### 3.7 API contracts / cross-cutting

| Tag | Source | One-liner |
|---|---|---|
| `[TD-DIAG-ALIGN-10]` | `questions/open/30 §Q-API-001` | Amend `10 §5.1`'s Diagnostic sketch to import `30 §5`'s ratified shape. |
| `[TD-CAT-CODE-TABLE-AMEND]` | `37 §8.3`, `questions/open/37 §Q-CAT-001` | Register `CAT_E_*` / `FS_E_*` in `30 §6.2`. |
| `[TD-IR-CODE-TABLE-AMEND]` | `35 §10.2`, `questions/open/35` | Register `IR_E_3500`–`3599` in `30 §6.2`. |

### 3.8 Per-crate API

| Tag | Source | One-liner |
|---|---|---|
| `[TD-MODEL-DIR-LOADER]` | `32 §10.3`, `questions/open/32 §Q-MODEL-001` | `parse_dir` / `parse_files` multi-file loader. |
| `[TD-INLINE-HOIST-LAZY]` | `32 §3.3`, `questions/open/32 §Q-MODEL-004` | Deferred hoisting for incremental re-parse. |
| `[TD-MODEL-YAML-CRATE]` | `32 §15`, `questions/open/32 §Q-MODEL-007` | Migration from `serde_yaml` to `yaml-rust2` / `saphyr`. |
| `[TD-MODEL-FUNCTIONS-BLOCK]` | `questions/open/32 §Q-MODEL-008` | Per-Model `functions:` YAML block fate. |
| `[TD-EXPR-PARSE-SITE-AUDIT]` | `questions/open/32 §Q-MODEL-005` | Exhaustive parse-site audit appendix. |
| `[TD-33-ERROR-UNIFY]` | `33 §10.1`, `questions/open/33` | `CompileError` re-export contract (shared variants from core). |
| `[TD-33-CANONICAL-JSON]` | `33 §14.3`, `questions/open/33` | Canonical JSON as secondary encoding. |
| `[TD-33-BINCODE]` | `33 §14.2`, `questions/open/33` | Bincode encoder behind a feature flag. |
| `[TD-33-INCREMENTAL-COMPILE]` | `33 §17` | Streaming / incremental compile API. |
| `[TD-33-CLIPPY-ASYNC-GUARD]` | `33 §11` | CI lint forbidding new `async fn` outside the two ratified entry points. |
| `[TD-IR-RENAME]` | `35 §3.1`, `35 §13` | `LogicalPlan` → `SemanticPlan` rename. |
| `[TD-IR-NONEXHAUSTIVE]` | `35 §13` | Non-exhaustive discipline across `PlanNode` variants. |
| `[TD-IR-NON-EQUI-JOIN]` | `35 §4.6`, `questions/open/35` | Non-equi-join `residual` field. |
| `[TD-IR-AGG-FILTER]` | `35 §4.5`, `questions/open/35` | Uniform `filter: Option<PhysicalExpr>` carriage. |
| `[TD-IR-SCHEMA-SHARING]` | `questions/open/35` | Schema-type unification with Manifest-layer Schema. |
| `[TD-ADAPTER-DEBUG-SQL-FREE-FN]` | `36 §3.5` | `debug_sql` as free function (not trait method). |
| `[TD-ADAPTER-SUBSTRAIT-ANCHOR]` | `36 §5` | Per-function Substrait URN override. |
| `[TD-ADAPTER-SUBSTRAIT-ASOF]` | `36 §5` | AsOf join extension path for Substrait. |
| `[TD-ADAPTER-RENAME]` | `36 §13` | Adapter-trait / error naming migration. |
| `[TD-ADAPTER-ERROR-MIGRATION]` | `36 §13` | `AdaptError` roster refresh. |
| `[TD-ADAPTER-DIALECT-SPLIT]` | `36 §13` | Split `Dialect` axis from `EngineAdapter`. |
| `[TD-ADAPTER-PLAN-BUILDER-RETIRE]` | `36 §13` | Retire adapter-side `PlanBuilder` after Substrait path stabilizes. |

### 3.9 Legacy `docs/TECH_DEBT.md` entries

Legacy items prefixed `TD-0NN` (no brackets in original). They are retained here as-is; each will either migrate into the bracketed `[TD-*]` scheme during its resolving phase, or close outright when ratified-design work absorbs it.

| Tag | Source | Absorption / closure |
|---|---|---|
| `TD-001` (KindRef.variant serde) | `docs/TECH_DEBT.md` | Closed by Phase 2 (`ResolvedDataKind` rewrite; deterministic Manifest). |
| `TD-002` (SemanticGraph serde skip) | `docs/TECH_DEBT.md` | Closed by Phase 2 (Manifest indices materialization). |
| `TD-003` (SemanticGraph duplicates) | `docs/TECH_DEBT.md` | Closed by Phase 2 (Coverage index dedup contract). |
| `TD-004` (expect-panic messages) | `docs/TECH_DEBT.md` | Opportunistic when touched during Phase 2 / 3; not a phase gate. |
| `TD-005` (AVG always Number) | `docs/TECH_DEBT.md` | Re-homed under `13 §2.1`'s `DataType::Decimal` variant ratification (Phase 1). |
| `TD-006` (active_bindings allocates Vec) | `docs/TECH_DEBT.md` | Non-blocking; closed opportunistically in Phase 3. |
| `TD-007` (RelationshipGraph / FieldIndex not removed) | `docs/TECH_DEBT.md` | Closed by Phase 2 (Manifest-layer rewrite); depends on `TD-002`. |
| `TD-008` (I/O utilities in manifest) | `docs/TECH_DEBT.md` | Trigger-based — when three I/O utilities accumulate, extract `semstrait-io`. Not a phase gate. |
| `TD-009` (Unreachable metadata values) | `docs/TECH_DEBT.md` | Closed by Phase 1 (`FunctionRegistry` + expression validation pass). |

## 4. Known `[CODE-DIVERGES-FROM-SPEC]` flags

The `[CODE-DIVERGES-FROM-SPEC]` tag is reserved for places where a subagent (during design drafting) explicitly called out that the ratified design diverges from the current code in a way that constitutes a migration task. It is a **stricter** marker than "the code will need updating" — it denotes a divergence the design doc itself flags and sequences the fix for.

### 4.1 Explicitly tagged flags

| Doc / section | Description | Fix phase |
|---|---|---|
| `32 §14.4` / `32 §15` | YAML surface: top-level `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` blocks replaced by unified `data_kinds:` with explicit `kind:` discriminator; `children:` structure; `JoinsetSpec.anchor` + `JoinsetSpec.path`; `ColumnMappingValue` variant set; DataType / Grain re-homed to `semstrait-core`; `Relationship` field renames; `ModelError` → `ParseError` + `ValidateError`. | Phase 1 + Phase 2 |

This is the only tag literally spelled `[CODE-DIVERGES-FROM-SPEC]` in the tree as of Round-1 freeze. Authors drafting subsequent API docs SHOULD apply the tag more aggressively at per-divergence granularity; absent that, the §2 per-crate delta catalog (above) is the mechanical equivalent.

### 4.2 Narrated divergences (not literally tagged)

Several design docs narrate code-vs-spec divergence without applying the tag. They are catalogued here so they are not lost; they MUST be promoted to explicit `[CODE-DIVERGES-FROM-SPEC]` tags as the relevant docs are amended. Each row below duplicates the §2 per-crate entries in a doc-first view.

| Doc / section | Description |
|---|---|
| `17 §1.3` | `TemporalHistorization` enum + `ScdType` enum need renaming to `TemporalShape` + `ScdSubtype`; `Type0` absent; window-payload carriage per-subtype. |
| `31 §11` (`[TD-CORE-SERDE-GATING]`) | Core types derive serde unconditionally; must be `#[cfg(feature = "serde")]`-gated. |
| `33 §14.1` | Manifest bytes not I4-stable; no canonical encoder selected. |
| `35 §13` (`[TD-IR-RENAME]`) | `LogicalPlan` → `SemanticPlan` rename; `PlanNode` non-exhaustive gap. |
| `36 §13` (`[TD-ADAPTER-*]`) | Adapter naming / errors / Dialect split / debug-sql free-function. |
| `00 §4.3` banned-terms | `Compiled*` prefix on manifest types → `Resolved*`; `StorageProvider` → `FileSystem`; `simplify` → `optimize`; `Entity` → "named DataKind instance"; `TemporalHistorization` → `TemporalShape`. |

## 5. Phased roadmap

Eight phases. Each phase has exit criteria, owning crates, high-level migration steps, and a risk tier. Phases may parallelize **within** a phase (e.g. `TemporalHistorization` rename can land alongside type-enum widening); phases are **strictly ordered** at exit gates.

**Phase ordering invariant.** A phase may only begin when every preceding phase's exit criteria are green. Exceptions are called out in the per-phase "Concurrent-safe with" row; by default, each phase's exit is a hard gate.

### 5.1 Phase 0 — Preparation

**Purpose.** Non-behavioral scaffolding. No public API moves. Green CI throughout.

**Exit criteria.**

- Every crate listed in `30 §13` exists in the workspace with a `Cargo.toml` that matches the target name roster (`semstrait-core`, `semstrait-model`, `semstrait-manifest`, `semstrait-planner`, `semstrait-ir`, `semstrait-adapter`, `semstrait-catalog`, `semstrait-api`, `semstrait`). All exist today; confirm naming / stability tier assignment.
- `TemporalHistorization` renamed to `TemporalShape` with a `#[deprecated]` alias (`00 §4.3` + `17 §1.3`). Downstream crates use the new name.
- Any rename already ratified by the banned-terms table has a `#[deprecated]` alias in place (`Compiled*` → `Resolved*` alias, `StorageProvider` → `FileSystem` alias, `simplify` → `optimize`). Aliases are one-MINOR-cycle transitions per `30 §12`.
- Every deprecated symbol has a `41_deprecations.md` entry.
- Test harness scaffolding: golden-file infrastructure for Manifest determinism checks (Phase 2); snapshot infrastructure for adapter emission (Phase 5). Stubs only in Phase 0.
- Any lingering numbering / module-path inconsistencies flagged by reviewers during Round-1 drafting are fixed.

**Owning crates.** All. Mostly touches `Cargo.toml`, doc headers, public `pub use`s, test-support modules.

**Migration steps (high-level).**

- Add alias / deprecation shims; do NOT remove legacy symbols yet.
- Bootstrap `tests/golden/` and `tests/snapshot/` directories (empty populate).
- Inventory every public symbol that will rename in a later phase; record in `41_deprecations.md`.
- Audit `pub` surface — every type on the `30 §4.2` non-exhaustive roster gets `#[non_exhaustive]` if absent (this is a no-op for already-compliant types; MINOR for newly-stamped ones).
- Confirm CI runs `cargo-semver-checks` (or equivalent).

**Risk tier.** Low (no behavioral change). Backwards-compat impact: none.

**Rollback trigger.** Any unexplained CI red. Back out the alias / deprecation layer, investigate.

### 5.2 Phase 1 — Foundations alignment (`semstrait-core` + YAML surface)

**Purpose.** Align `semstrait-core` with `31` and the foundation docs (`13`, `14`, `14a`); align `semstrait-model`'s YAML surface with `32`.

**Exit criteria.**

- `DataType` enumerates the 14 variants ratified in `13 §2.1` / `31 §4.1`. Legacy variant names remain as aliases for one MINOR cycle where feasible; removed at the next MAJOR.
- `Diagnostic`, `Severity`, `Span`, `ContextLine` exist as `semstrait-core` public types with `30 §5`'s shape.
- `CanonicalFn` newtype with `pub const` identities is in `semstrait-core` (`14a §2` / `31 §5`).
- `FunctionRegistry` is accessible through `semstrait-core::function_registry()` returning `&'static FunctionRegistry` (`14a §2.1` / `31 §9`).
- `SemanticExpr` / `PhysicalExpr` newtypes over `Expr` exist with context-enforcing invariants (`14 §4` / `31 §4.4`). Round-1 default: runtime checks at construction; compile-time enforcement via type-state is an open-item (per-crate `31`).
- `ExprSource` YAML surface matches `32 §6` — Inline DSL + Declarative-block, hoisting-eager per `[TD-INLINE-HOIST-LAZY]`'s Round-1 default.
- YAML top-level `data_kinds:` block with explicit `kind:` discriminator; legacy `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` grouping retained behind a parser-level deprecation warning for one MINOR cycle. After the cycle, rejection.
- `Relationship` YAML shape updated per `16 §2.1` / `32 §7`.
- `ColumnMappingValue` variant set updated per `32 §8.3`.
- Every new / refreshed public symbol has a deprecation / migration note in `41`.
- Error codes surfaced by `parse` / `validate` follow the `PARSE_*` / `VALID_*` prefixes per `30 §6`.
- Closes legacy `TD-005` (AVG → Decimal via new `Decimal` variant).
- Advances `[TD-CORE-SERDE-GATING]` to either resolved (serde-gated) or explicit parked.

**Owning crates.** `semstrait-core`, `semstrait-model`.

**Migration steps (high-level).**

- Widen `DataType` additively (MINOR per I10); stage the removal of legacy-only variants behind a one-cycle deprecation.
- Introduce `Diagnostic` / `Severity` / `Span` / `ContextLine` types; downstream crates adopt the conversion layer.
- Move `FunctionRegistry` from `semstrait-manifest` to `semstrait-core`; re-export from `semstrait-manifest` for one MINOR cycle.
- Introduce `SemanticExpr` / `PhysicalExpr` newtypes. Call sites adopt incrementally; bare `Expr` is accepted behind compatibility paths for one MINOR cycle.
- YAML grammar: parser accepts legacy and new top-level layout simultaneously; emits `PARSE_W_*` on legacy. Flip the warning to an error at the next MAJOR.
- Wire `ExprSource` custom-deserialize path across all authoring scopes.
- Rewrite `Relationship` YAML parsing to the new field names; accept legacy names as aliases for one cycle.

**Risk tier.** Medium. Wide-blast-radius type changes (DataType, Diagnostic). Contained by additive-MINOR discipline.

**Concurrent-safe with.** Phase 0 scaffolding. Not concurrent-safe with Phase 2 (Phase 2 depends on Phase 1's type surface).

**Rollback trigger.** Legacy YAML corpus fails to parse under both old + new grammar for more than 2% of ratified test fixtures. Or: downstream crate churn from `Diagnostic` introduction exceeds a reviewer-set threshold in a single PR batch.

### 5.3 Phase 2 — Manifest-layer rewrite

**Purpose.** Rewrite `semstrait-manifest` to the `33` contract: `Resolved*` naming, `ResolvedExprTable`, Coverage + Composition indices, deterministic serialization.

**Exit criteria.**

- Every `Compiled*` type is renamed `Resolved*` per `00 §4.3` / `33 §3`. Legacy names remain aliased one cycle.
- `ResolvedExprTable` materialized per `14b §2` / `33 §6` — every (Semantics, Binding) pair pre-computed into a `PhysicalExpr`.
- Cross-DataKind path pre-resolution per `14b §5` — plan-time lookup is O(1); no `Relationship`-graph walking at plan time.
- Coverage indices at the Binding level (`15`) and the ComposedSemanticInterface level (`16 §9`) are materialized in the Manifest per `33 §7`.
- Manifest round-trip preserves all state (`TD-001` / `TD-002` closed). `SemanticGraph` either has bespoke serde impls or is rebuilt post-deserialize per a documented `rebuild_semantic_graph()` hook.
- Deterministic serialization: running `compile` twice on identical `(Model, Catalog)` produces byte-identical bytes (I4 green; `33 §14`). Golden-file tests land.
- Manifest encoder supports MessagePack + JSON per `33 §14`. Bincode parked per `[TD-33-BINCODE]`.
- `CompileError` re-exports shared variants from `semstrait-core::CompileError` per `[TD-33-ERROR-UNIFY]`.
- `RelationshipGraph` + `FieldIndex` removed (`TD-007` closed).
- `io.rs` placement reconsidered per `TD-008`; if the utility-count threshold is crossed during Phase 2, extract to a new `semstrait-io` crate ahead of Phase 3.

**Owning crates.** `semstrait-manifest`, `semstrait-catalog` (for `CatalogSnapshot` hooks), `semstrait-core` (shared error variants).

**Migration steps (high-level).**

- Land `Resolved*` aliases; begin migrating call sites off `Compiled*` names.
- Introduce `ResolvedExprTable` alongside the current compiled-kind types; populate it during `compile`; planner gradually adopts.
- Land Manifest indices; planner reads them instead of `RelationshipGraph` / `FieldIndex`.
- Bespoke serde for `SemanticGraph` (adjacency-list + rebuild).
- Golden-file harness populated.
- Remove `Compiled*` names at the next MAJOR cut after one MINOR cycle.

**Risk tier.** High. The Manifest is the contract between compile-time and plan-time; every consumer downstream feels this phase. Golden-file determinism is the only way to safely complete it.

**Concurrent-safe with.** None. Phase 2 is a hard sequence point for Phase 3.

**Rollback trigger.** Manifest byte-identical round-trip fails on any ratified test corpus after Phase 2's exit. Flip back to the `Compiled*` alias path, investigate. Any open-item that requires re-design (not re-implementation) bumps back to the relevant design doc.

### 5.4 Phase 3 — Planner-layer rewrite

**Purpose.** Align `semstrait-planner` with `34`: strategy-trait surface, step-0 `ConstraintValidator`, field-first resolution, implicit-composition algorithm.

**Exit criteria.**

- `Request.from = None` accepted via field-first resolution (`16` / `34`).
- `MAX_IMPLICIT_COMPOSITION_DEPTH = 4` constant enforced with `PLAN_E_0502 CompositionDepthExceeded`.
- Ambiguous-path ties produce `PLAN_E_0500 AmbiguousImplicitComposition` (no heuristic; `16 §11.4`).
- Step-0 `ConstraintValidator` runs at the start of every plan per `11 §8.4`. Failures emit `ConstraintViolation` per `10 §5` (typed fan-out deferred per `[TD-CONSTRAINT-ERROR-FANOUT]`).
- Strategy trait (per-DataKind dispatch) exposes a named surface aligned with `34`'s Round-1 ratification.
- Planner consumes Manifest indices directly (Phase 2's output); zero `RelationshipGraph` / `FieldIndex` references.
- `CatalogProvider::check_schema_drift` hook wired per I11b; optional gate, pre-plan.
- `Additivity`-driven plan restructuring still stubbed (`DL-058`) — explicitly carried forward as parked work; `34`-era planner does not close the semi/non-additive gap in Phase 3.

**Owning crates.** `semstrait-planner`, `semstrait-manifest` (for index refactors), `semstrait-catalog` (drift hook).

**Migration steps (high-level).**

- Convert per-DataKind strategy modules into implementers of a named trait.
- Introduce `ConstraintValidator` as a plan step-0; wire every Constraint carrier.
- Implement field-first resolution atop the Manifest's indices.
- Implement the implicit-composition BFS with depth limit + tie rule per `16 §11`.
- Move Additivity-aware plan shapes to a named placeholder with `TODO(`[additivity-resolver-v2]`)` markers inline, clearly scoped out of Phase 3.

**Risk tier.** High. Plan output byte-equivalence is not a determinism goal in Phase 3 (planner rewrites legitimately change output shape); regression control is via test-fixture comparison at the adapter-emission level (Phase 5 finalizes this contract).

**Concurrent-safe with.** None. Depends on Phase 2.

**Rollback trigger.** Loss of plan-level byte equivalence on a ratified test corpus without a corresponding design-tracked justification. (I.e., a shape change must trace to a `34` decision, or it is a regression.) If the trace fails, revert and re-plan.

### 5.5 Phase 4 — IR canonicalization

**Purpose.** Align `semstrait-ir` with `35`: `SemanticPlan` naming, uniform `#[non_exhaustive]` discipline, reserved fields for non-equi joins and aggregate filters, error-code range alignment.

**Exit criteria.**

- `LogicalPlan` → `SemanticPlan` rename (`[TD-IR-RENAME]`); `pub type LogicalPlan = SemanticPlan;` alias maintained one MINOR cycle.
- Every `PlanNode` variant and every public-facing enum / struct on the `30 §4.2` roster is `#[non_exhaustive]` (`[TD-IR-NONEXHAUSTIVE]`).
- `JoinNode.on: Vec<KeyPair>` stable; `residual: Option<PhysicalExpr>` reserved per `[TD-IR-NON-EQUI-JOIN]` (always `None` in v1).
- `AggNode.filter: Option<PhysicalExpr>` uniform per `[TD-IR-AGG-FILTER]`.
- `JoinType::AsOf` variant present (non-exhaustive enum addition is MINOR); planner-side emission remains DEFERRED per `17 §10`.
- `DialectId` carried on every `SqlArtifact` (ties into Phase 5).
- `IR_E_3500`–`3599` registered in `30 §6.2` (`[TD-IR-CODE-TABLE-AMEND]`).
- `Schema` type still lives in `semstrait-ir`; consolidation with manifest-layer `Schema` parked as `[TD-IR-SCHEMA-SHARING]`.

**Owning crates.** `semstrait-ir`.

**Migration steps (high-level).**

- Name-level rename with alias.
- Stamp `#[non_exhaustive]` across the roster; audit downstream match arms (wildcard arms mandatory per I10).
- Add reserved optional fields (`residual`, `filter`) to existing node variants; wire them through `Default::default()` + bespoke constructors.
- Align error-code prefixes and register.

**Risk tier.** Low-medium. Most changes are structural additions. The rename is mechanical.

**Concurrent-safe with.** Phase 5 preparation (capability-flag scaffolding).

**Rollback trigger.** A non-exhaustive stamping reveals a downstream consumer without a wildcard arm — that is a design violation, not a Phase 4 problem; escalate to the owning crate's `3x` doc for amendment.

### 5.6 Phase 5 — Adapter rewrite

**Purpose.** Align `semstrait-adapter` with `36`: dialect machinery split, capability flags, per-engine emission discipline, Substrait anchor infrastructure.

**Exit criteria.**

- `EngineAdapter` + `Dialect` split per `36 §2` (`[TD-ADAPTER-DIALECT-SPLIT]`).
- `EngineAdapter::adapt` returns `EngineArtifact` (sum type) per `36 §3`.
- `debug_sql` is a free function (`[TD-ADAPTER-DEBUG-SQL-FREE-FN]`).
- Every SQL-emitting adapter carries `DialectId` on every `SqlArtifact`.
- Per-adapter capability flags declared (`Cte`, `DistinctAggregate`, `AsOfJoin`, `GroupingSets`, `StructAccess`) per `36 §4`. Gating logic live-paths against them.
- Per-engine adapter crate split (`semstrait-adapter-datafusion`, `semstrait-adapter-duckdb`, `semstrait-adapter-spark`, `semstrait-adapter-substrait`) is the v1 target per `30 §13`; in-crate split is the Round-1 transitional form. Final split may migrate post-v1.
- Substrait function anchors defaulted to `CanonicalFn::as_str()`; URN overrides hookable (`[TD-ADAPTER-SUBSTRAIT-ANCHOR]`).
- `JoinType::AsOf` emission path planned but DEFERRED (`[TD-ADAPTER-SUBSTRAIT-ASOF]` / `[TD-COMPOSITION-ASOF]`). Vocabulary is present in IR; emission is explicit error or TODO, not silent incorrect output.
- `PlanBuilder` path retired per `[TD-ADAPTER-PLAN-BUILDER-RETIRE]`.
- Snapshot-test suite covers every SQL adapter's emission for the v1 test-request corpus.

**Owning crates.** `semstrait-adapter` (and per-engine subcrates if promoted).

**Migration steps (high-level).**

- Factor `EngineProfile` / dialect machinery into the `Dialect` trait per `36 §2`.
- Wire `DialectId` onto every emitted `SqlArtifact`.
- Implement capability-flag gating around CTE / distinct-agg / grouping-sets / as-of-join emission.
- Snapshot golden-fixtures for every adapter-request pair.
- Substrait: align every `FunctionCall` / `Aggregation` to the canonical-function URNs; leave override hook.

**Risk tier.** High. Every user-observable artifact passes through this phase.

**Concurrent-safe with.** Phase 4 (IR stabilization must complete before emission changes).

**Rollback trigger.** Snapshot divergence on the v1 test corpus without a design-tracked justification. Re-emission differences traceable to `36`'s ratifications are expected and tracked in `42_migration_notes.md`.

### 5.7 Phase 6 — Catalog integration

**Purpose.** Align `semstrait-catalog` with `37`: provider / filesystem trait surface, drift-check granularity, per-provider error fan-out.

**Exit criteria.**

- `CatalogProvider` method roster matches `37 §3`.
- `FileSystem` trait replaces `StorageProvider` fully (one-MINOR-cycle alias bridge per `30 §12`).
- Per-provider impls (`semstrait-catalog-iceberg`, `semstrait-catalog-unity`, local FS, S3) sit behind the ratified traits cleanly.
- Schema drift covers every resolved source, not just `manifest.datasets` (`DL-037` deprecated and removed).
- `CAT_E_*` / `FS_E_*` codes registered in `30 §6.2` (`[TD-CAT-CODE-TABLE-AMEND]`).
- Glob-matching home resolved (`questions/open/37 §Q-CAT-002`) — either stays in `semstrait-core::GlobPattern` or moves to catalog.

**Owning crates.** `semstrait-catalog`, `semstrait-manifest` (drift-check call sites), `semstrait-core` (optional glob migration).

**Migration steps (high-level).**

- Refresh trait signatures; add alias methods where `30 §12` permits.
- Widen drift coverage to every resolved source.
- Register codes.

**Risk tier.** Medium. Catalog integrations are covered by integration tests; drift is a narrow hot path.

**Concurrent-safe with.** Phase 5.

**Rollback trigger.** Any Iceberg / Unity integration test regresses under the new trait surface.

### 5.8 Phase 7 — Facade / API crates

**Purpose.** Align `semstrait-api` (`38`) and `semstrait` facade (`39`) with the ratified entry-point surfaces. Wire `Diagnostic`-shaped error-carriage per I12.

**Exit criteria.**

- `semstrait-api` exposes the unified pipeline entry with warning propagation per `questions/open/30 §Q-API-002`: every pipeline verb returns `Result<(Output, Vec<Diagnostic>), (Error, Vec<Diagnostic>)>`.
- `semstrait` facade re-exports the minimum useful `semstrait-*` surface per `39 §2` / `39 §4`.
- Feature flags align with `30 §10` — per-adapter / per-provider feature flags on both `semstrait-api` and `semstrait`.
- gRPC / REST / CLI transports each consume the unified pipeline (no private copies of pipeline plumbing).
- Diagnostic-shaped errors surface on every public entry point.

**Owning crates.** `semstrait-api`, `semstrait`.

**Migration steps (high-level).**

- Adopt the warning-propagation signature at every public API entry.
- Audit re-exports; trim / expand per `39`.
- Refresh transport-specific error handling to consume Diagnostic.

**Risk tier.** Medium. User-facing; every caller updates. Provided `38` / `39` are ratified, the migration is mechanical.

**Concurrent-safe with.** Phase 6.

**Rollback trigger.** Loss of transport-feature-flag correctness on any published feature matrix.

### 5.9 Phase 8 — Documentation + migration polish

**Purpose.** Retire superseded design documents; finalize `41` / `42`; refresh crate READMEs.

**Exit criteria.**

- `docs/ARCHITECTURE.md`, `docs/{DATASET,GRAINSET,UNIONSET,JOINSET}.md`, `docs/CATALOG_RESOLUTION.md`, `docs/COMPUTED_EXPRESSIONS.md`, `docs/FUNCTION_CATALOG.md`, `docs/SEMANTIC_RESOLUTION.md` each reduced to short redirects into `docs/design/` per `00 §11`.
- `docs/TECH_DEBT.md` legacy entries either absorbed into §3.9 of this doc (kept as tombstone) or closed by phase work (removed from the legacy file, retained as tombstone in `41`).
- `CLAUDE.md` updated to route into `docs/design/` first for semstrait-related work, per `00 §11`.
- Crate READMEs trimmed to "what's in this crate"; each links to its `apis/3x_*.md`.
- `41_deprecations.md` fully populated for every `#[deprecated]` symbol still active.
- `42_migration_notes.md` contains one entry per MAJOR cut taken during phases 1–7.
- Banned-terms audit (`00 §4.3`) clean: no remaining `TemporalHistorization`, `Kind` (bare), `connector`, `Compiled*`, `StorageProvider`, `simplify`, engine-terminology references in public surfaces.

**Owning crates.** All. Doc-only changes.

**Migration steps (high-level).**

- Inventory; trim; link.

**Risk tier.** Low.

**Rollback trigger.** n/a. Doc-only.

### 5.10 Phase dependency diagram

```mermaid
flowchart LR
    P0(Phase 0<br/>Preparation) --> P1(Phase 1<br/>Foundations)
    P1 --> P2(Phase 2<br/>Manifest)
    P2 --> P3(Phase 3<br/>Planner)
    P3 --> P4(Phase 4<br/>IR)
    P4 --> P5(Phase 5<br/>Adapter)
    P5 --> P6(Phase 6<br/>Catalog)
    P6 --> P7(Phase 7<br/>Facade/API)
    P7 --> P8(Phase 8<br/>Docs)
```

Hard sequencing at P1→P2, P2→P3, P4→P5. P3↔P4 may interleave at the PR level if strict IR/planner contracts stay in sync. P6 can begin as soon as P5's trait surface is stable (capability-flag scaffolding); it does not require P5's full snapshot-green state.

## 6. Breaking-change discipline

### 6.1 What counts as MAJOR

Per `30 §2.1`: any non-additive change to a `pub` type / function / trait / trait method; removing a variant; adding a required field to a non-`#[non_exhaustive]` struct; changing a function signature other than relaxing bounds; retiring a stable error code; changing `Diagnostic` shape; changing pipeline-stage error policy.

Per `30 §11.2`, every MAJOR requires:

- A MAJOR changelog entry.
- An `implementation/42_migration_notes.md` entry with before-and-after examples.
- A deprecation window of at least one MINOR cycle where feasible.
- `cargo-semver-checks` green (or a documented waiver).

### 6.2 How to minimize MAJORs across this plan

The phases above are sequenced so that most deltas land as MINORs (additive `#[non_exhaustive]` variants, new public types, new traits, `#[deprecated]` aliases). MAJORs are concentrated at two points:

- **End of Phase 1** — removal of legacy `DataType` variant names (if any), removal of `datasets:` / `grainsets:` / … YAML grammar, removal of the `Compiled*` prefix aliases that were parked behind `#[deprecated]` throughout Phase 0–2.
- **End of Phase 5** — removal of the legacy adapter surface (`PlanBuilder`, `debug_sql` trait method, old-form `EngineProfile` as a supertrait).

No other phase carries a planned MAJOR. If a phase discovers that a delta cannot be additively staged, the PR batch ends early — the remaining work re-queues behind a planned MAJOR cut.

### 6.3 Sequencing MAJORs

Per `30 §11.3`, each MAJOR flows through a coordinated workspace release. The two planned MAJOR cuts above are intentionally spaced: the Phase-1 MAJOR lets the entire downstream tree adopt the new foundation before Phase 5's adapter surface shifts. Consumers ingest one MAJOR per workspace release; back-to-back MAJORs across releases are allowed only when a phase discovers unscheduled churn.

Phase-7 MAJOR risk: if `38` / `39` drafting surfaces unexpected shape changes, a third MAJOR may be required. Mitigate by finalizing `38` / `39` before Phase 7 enters.

### 6.4 Compatibility shims

Shims are the Round-1 escape valve for spreading a MAJOR across several workspace releases:

- **Type aliases** — `pub type LegacyName = NewName;` kept one MINOR cycle after rename.
- **Re-exports** — `pub use new_module::Symbol as LegacySymbol;`.
- **Deprecated wrapper fns** — `#[deprecated] pub fn legacy_verb(...) -> ... { new_verb(...) }`.
- **Parser-level duplicate grammars** — accept both legacy and new YAML shapes; `PARSE_W_*` on legacy; hard-reject at the MAJOR.
- **Trait default-methods** — new default bodies let impls stay on the legacy method for one cycle.

Shims MUST be registered in `41_deprecations.md` at the moment they land. Unregistered shims are a discipline violation.

### 6.5 What is NOT a shim

- **Silently coexisting two shapes in the Manifest.** I4 determinism requires byte-stability; two-shapes-in-one Manifest breaks it.
- **Runtime feature flags that change public-API return types.** Use feature flags for dependency / optional-feature gating, not for surface-shape toggles.
- **Environment-variable-driven behavior switches.** Never in the public API.

## 7. Testing strategy

### 7.1 Per-phase exit gates

Every phase exits with green CI against the current test suite, plus the phase-specific additions below.

| Phase | Test addition |
|---|---|
| 0 | Scaffolding only: empty `tests/golden/`, `tests/snapshot/` directories; lint green. |
| 1 | YAML parse / validate regression corpus expanded for new `data_kinds:` grammar; legacy-grammar corpus still green. Serde round-trip tests on new `DataType` variants. |
| 2 | **Golden-file Manifest determinism.** For every fixture in `tests/golden/manifests/`, running `parse → validate → compile` twice produces byte-identical output. Gate: all green. |
| 3 | Plan-level fixtures under `tests/planner/` extended for field-first resolution, step-0 ConstraintValidator, ambiguous-path, depth-limit. Byte-stable `SemanticPlan` output per (Manifest, Request). |
| 4 | IR round-trip tests (SemanticPlan → bytes → SemanticPlan) byte-stable. All `#[non_exhaustive]` variants have wildcard-arm handling in every consuming crate's match statements (enforced via clippy / manual audit). |
| 5 | **Snapshot tests per adapter.** For every (adapter, request) in `tests/snapshot/adapter/`, emission is byte-identical to the checked-in snapshot. Snapshot updates require reviewer sign-off. |
| 6 | Iceberg / Unity integration tests refactored against the new trait surface. Drift-check coverage tests. |
| 7 | API-layer integration tests for each transport (gRPC, REST, CLI) against the unified pipeline. Warning-propagation assertions. |
| 8 | Documentation linter: every banned term (`00 §4.3`) produces a CI error if found in `docs/design/`, crate READMEs, or public rustdoc. |

### 7.2 Determinism (I4) contracts

Two byte-stable surfaces:

- **`Manifest`** — Phase 2 onward.
- **`SemanticPlan`** — Phase 3 onward (planner output, pre-adapter).

A third surface — **`EngineArtifact`** — is deterministic per (`Manifest`, `Request`, adapter), enforced via snapshot tests from Phase 5. `EngineArtifact` byte-stability is not an I4 invariant; it is a snapshot contract.

### 7.3 Regression ladder

Each phase's tests are additive on top of the prior phase's. The full CI ladder after Phase 8:

```
parse → validate → compile → plan → optimize → adapt
  ↓        ↓          ↓         ↓        ↓         ↓
YAML    struct    Manifest   Plan   Plan-2    Artifact
corpus  corpus    golden    fixtures fixtures  snapshot
```

### 7.4 Tooling

- `cargo-semver-checks` — surface-stability check at every workspace release boundary.
- `cargo-hakari` / workspace-hack — kept healthy during Phase 2–3 (heavy cross-crate churn).
- Golden-file harness — per-crate `tests/golden/` + `tests/snapshot/` directories.
- Clippy / rustfmt — clean across every phase.
- Post-Phase-8 documentation linter — inline banned-term audit.

## 8. Rollback discipline

### 8.1 Per-phase rollback triggers

Each phase declares a named rollback trigger in §5; this section reiterates the category framework.

| Category | Examples | Action |
|---|---|---|
| Determinism loss | Manifest byte-identical round-trip red; SemanticPlan shape drift without a ratified-design justification. | Revert to prior phase's state; investigate with the design author; amend the design doc if required; re-attempt. |
| Test corpus breakage | ≥ 2% of ratified fixtures regress under new behavior without a design-tracked cause. | Same as above. |
| Cross-crate compile red lasting > 1 day | A refactor lands that blocks downstream work. | Revert; divide the refactor into smaller PR batches. |
| Snapshot divergence (Phase 5+) | Adapter emission changes in unexpected ways. | Revert the offending commit; investigate; either justify + update the snapshot or back out the change. |

### 8.2 Feature gates during transition

During any phase where the old shape and the new shape both compile simultaneously (Phases 1–5 typically), the old shape is gated behind either:

- `#[deprecated]` attribute on the public symbol (rustc warning only); or
- A default-off Cargo feature named `legacy-<area>` (opt-in for consumers that need the old shape on a specific release).

`legacy-*` features MUST be removed at the phase's MAJOR cut. They are not long-term.

### 8.3 Flag bits

`SessionContext` does **not** carry per-feature flag bits. Feature gating lives at the Cargo-feature level; per-Request / per-Session flags are reserved for genuinely-per-invocation signals (query clock, correlation IDs, …) per `00 §4.1`'s `SessionContext` row.

### 8.4 Rollback at phase exit

Once a phase's exit criteria are green, its legacy-shim removal at the associated MAJOR cut is **irreversible**. Downstream consumers track the release in `42_migration_notes.md`. A post-MAJOR rollback requires a new MAJOR (cannot revert the removal; must re-add with a new name).

## 9. Deprecation pipeline

This doc forwards to `41_deprecations.md` for the per-symbol roster. At a policy level:

- **Entry criterion.** The moment a `#[deprecated]` attribute lands on any public symbol, an entry in `41_deprecations.md` is mandatory, per `30 §12.2`.
- **Entry content.** Fully-qualified path, `since` version, suggested replacement, target removal version (best estimate).
- **Exit criterion.** Symbol is removed in a MAJOR bump; the `41` entry moves to `42_migration_notes.md`. `41` retains a tombstone for one MAJOR cycle after removal.

### 9.1 Banned-terms master list (from `00 §4.3`)

Every term in this table WILL produce at least one deprecation cycle in the relevant crate during phases 0–5. All entries fold into `41` as their `#[deprecated]` attribute lands.

| Current | Target | Phase |
|---|---|---|
| `Kind` (bare) | `DataKind` / `SimpleDataKind` / `ComplexDataKind` | Phase 0–1 (rename-heavy) |
| `Entity` (as a distinct term) | "named DataKind instance" (prose only) | Phase 8 (doc-only) |
| `connector` | `EngineAdapter` / `CatalogProvider` | Phase 5 / Phase 6 |
| `CompiledDataKind`, `CompiledInterface`, `Compiled*` prefix broadly | `ResolvedDataKind`, `ResolvedInterface`, `Resolved*` | Phase 2 |
| `StorageProvider` | `FileSystem` | Phase 6 |
| `simplify` (as pipeline verb) | `optimize` | Phase 3 |
| `dispatch` (as vocabulary-level verb) | removed | Phase 3 |
| Engine terminology in docs ("RelNode", "Binder Analyzer", "Convention") | canonical term per `00 §4.1` | Phase 8 |
| `physical type` in non-adapter docs | `DataType` (logical) | Phase 8 |
| `TemporalHistorization` | `TemporalShape` | Phase 0 |

### 9.2 Error-code retirements

Per `30 §6.7` / `30 §12.3`, retired error-code literals are MAJOR. v1 introduces no retirements; every reserved code is forward-looking. If any phase discovers the need to retire a code, the retirement lands at the next MAJOR cut and its `41` tombstone moves to `42`.

## 10. Round-1 open items

See `docs/design/questions/open/40_questions.md`. Each item is a policy / process question for this plan, not a design re-open.

| # | Title | Parked item |
|---|---|---|
| Q-40-001 | PR-boundary policy within phases | Round-1 silent; PR-author discretion subject to phase-exit gates. |
| Q-40-002 | Per-phase staff allocation | Out of scope for design docs; owned by release management. |
| Q-40-003 | CI pipeline-change schedule | Tooling additions land in Phase 0; each phase declares its gate in `§7.1`. |
| Q-40-004 | `semstrait-io` extraction trigger | Follow `TD-008`'s "three utilities" rule; not a phase-exit criterion. |
| Q-40-005 | Per-engine adapter crate split timing | Phase 5 target per `30 §13`; may slip to post-v1 if PR churn exceeds budget. |
| Q-40-006 | `[TD-*]` tag discipline | Every divergence surfaced by later amendments SHOULD be tagged `[CODE-DIVERGES-FROM-SPEC]` at the design-doc amendment moment, not retrospectively. |
| Q-40-007 | Legacy `TD-0NN` migration into bracketed scheme | Closed opportunistically during each item's resolving phase; no blanket renormalization pass. |
| Q-40-008 | Banned-term audit tooling | A rustdoc / markdown linter is scaffolded in Phase 0; its rule set grows as each ban lands. |

---

*Cross-references in this document are by section (e.g. `00 §9`, `17 §1.3`, `30 §6.2`). No code-path references are used, per `00 §8`.*
