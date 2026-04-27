---
prereqs:
  - 40
authoritative-for:
  - the deprecation register (symbols / YAML keys / error codes / docs paths retired as part of the green-field migration)
  - the retirement lifecycle policy (how a symbol moves from `Active` → `Deprecated` → `Retired`, per-entry contract)
  - the per-symbol retirement schedule (which phase from `40 §5` a given symbol retires in)
  - exceptions-and-grandfathering roster (symbols that become `#[deprecated]` but are NEVER retired)
  - migration-aid discipline (which renames ship with `cargo fix`-compatible suggestions vs manual porting)
refined-by:
  - 42 (`implementation/42_migration_notes.md` — per-MAJOR caller-facing release notes; retirements move here when they ship)
---

# 41. Deprecations

> **Status:** ratified Round-1 register. Every row in this document is drawn from (i) `00 §4.3`'s banned-terms table, (ii) `40 §9`'s deprecation-pipeline summary, or (iii) a `[TD-*]` rename tag scanned from the ratified design tree. No entry in this document invents a deprecation the design tree has not already authorized. If a deprecation is discovered later, it lands through the `40 §1.4` channel (design amendment first, then refactor-plan amendment, then this register).

## 1. Purpose and Scope

### 1.1 Who reads this doc

Implementers landing a `#[deprecated]` attribute. Reviewers deciding whether a rename is "live" or "grandfathered." Release management drafting `42_migration_notes.md` entries. Agents pattern-matching against legacy symbols that should be flagged.

### 1.2 What this doc is

`41` is the **retirement register**: an append-only, per-symbol catalogue of every public name (type, function, trait, trait method, YAML key, error code, docs path) that the green-field migration retires. Every row carries the symbol's fully-qualified path, the replacement, the deprecation release, the target retirement release, and the tracking tag anchoring the work.

### 1.3 Contrast with sibling `implementation/` docs

| Doc | Scope | Question it answers |
|---|---|---|
| `40_refactor_plan.md` | **Forward-looking phased plan.** What work, in what order, with what exit gates. | "In which phase does this work land?" |
| `41_deprecations.md` (this) | **Per-symbol retirement register.** One row per symbol that ever gains `#[deprecated]`. Append-only; tombstones persist at least one MAJOR past removal. | "What is being retired, what replaces it, when does it disappear?" |
| `42_migration_notes.md` | **Per-version user-facing release notes.** Released alongside every MAJOR (and each MINOR that carries provisional-crate breaks per `30 §13`). Before / after examples, replacement guidance. | "How do I update my code for release `X`?" |

### 1.4 What this doc is NOT

- **Not a design doc.** `41` never re-opens a ratified decision (per `00 §8`'s directionality rule). A disagreement with a ratified rename is amended in the originating doc first.
- **Not a migration guide.** Caller-facing before/after code lives in `42`; `41` stops at the rename contract.
- **Not a `[TD-*]` inventory.** That is `40 §3`. `41` only indexes `[TD-*]` tags whose resolution produces a `#[deprecated]` attribute on a public symbol.

### 1.5 Layering posture

`41` sits directly below `40` and strictly above `42`: it consumes `40 §5` phase assignments and `40 §9` banned-term schedules, and it feeds per-row caller-impact narratives into `42`. It never contradicts an earlier doc; conflicts are design bugs to be amended upstream, not overridden here.

## 2. Deprecation Lifecycle Policy

This section is the policy layer; §3's register applies it.

### 2.1 Three states

Per `30 §12.1`, every retiring symbol passes through exactly three states:

| State | `#[deprecated]` present? | Symbol compiles? | Produces rustc warning? |
|---|---|---|---|
| **Active** | No | Yes | No |
| **Deprecated** | Yes (`since = "VERSION"`, `note = "use X instead; removed in VERSION"`) | Yes | Yes |
| **Retired** | n/a (symbol removed) | No | n/a (compile error) |

The `41` row contract below binds both transitions:

- **Active → Deprecated.** Happens in a MINOR release (per `30 §2.2`). The PR landing the `#[deprecated]` attribute also lands the `41` row.
- **Deprecated → Retired.** Happens in a MAJOR release (per `30 §2.1`, `30 §6.3`). The PR removing the symbol also lands the `42` row; the `41` row transitions to a **tombstone** with a `retired-in:` field.

### 2.2 Minimum window

Per `30 §12.4`: at least **one full MINOR cycle** between `Deprecated` and `Retired`. Per-crate docs MAY extend the window for widely-used symbols (e.g. two MINOR cycles for a core `CatalogProvider` method). Shorter windows require an explicit `41` note citing the exception condition (§2.4).

### 2.3 `#[deprecated]` attribute shape

Every deprecation lands with the canonical rustc attribute form:

```rust
#[deprecated(
    since  = "0.M.N",
    note   = "use `path::to::Replacement` instead; removed in 0.M'.0"
)]
```

- `since` — the exact workspace version in which the `#[deprecated]` attribute first shipped. Matches the MINOR release notes.
- `note` — free-form text that IDEs surface on hover. Every `note` MUST include (i) the fully-qualified replacement path and (ii) the target retirement version.

Non-Rust retirements (YAML keys, docs paths) cannot carry a rustc attribute; their `Deprecated` state is signaled through (a) a `PARSE_W_*` / `VALID_W_*` Diagnostic code at parse/validate time (YAML), or (b) an HTTP redirect / markdown-level `> **Deprecated:**` admonition at the top of the retired file (docs).

### 2.4 Exception conditions (shorter-than-one-cycle window)

A deprecation window shorter than one MINOR cycle is permitted **only** when all of the following hold:

1. The symbol was added in the same MINOR cycle it is being retired from. (i.e. the rename happened during a single cycle; no published caller could have depended on it.)
2. The symbol is either `pub(crate)` or lives behind a `Provisional` stability tier per `30 §13` (where pre-1.0 semver rules apply).
3. The `41` row carries an explicit `short-window:` field citing the exception clause above.

No other shortened-window exception is permitted. A symbol shipped at a `Stable in v1` tier per `30 §13` always gets the one-MINOR-cycle window.

### 2.5 Retirement vs deprecation — distinction

Per `30 §12.3`:

- **Deprecating** a symbol is **MINOR**. The symbol remains callable; callers see a rustc warning.
- **Retiring** a symbol is **MAJOR**. The symbol is removed from the public surface.

This distinction binds all four retirement axes in §3 (vocabulary, code symbols, YAML keys, error codes). For error codes specifically (§3.4), `30 §6.3` ratifies that a published code's meaning is **frozen at its first release**: a MAJOR may retire it but never repurpose it.

### 2.6 Tombstone policy

After a row moves to **Retired**, the `41` entry remains in place as a **tombstone** for at least one MAJOR cycle past removal, so callers reading old commit logs can still trace a retired name to its replacement. The tombstone row carries:

- The original `since:` (when the deprecation started).
- The `retired-in:` version.
- A cross-reference to the `42` entry carrying the full caller-facing migration story.

After the one-MAJOR-past-removal horizon, tombstones MAY be pruned from `41`; by default they stay.

## 3. The Retirement Register — by category

Register rows follow the §4 table shape. Sub-sections `§3.1`–`§3.6` partition the register by retirement axis.

### 3.1 Vocabulary (from `00 §4.3`)

Source: `00 §4.3`'s banned-terms table is the authoritative list. `40 §9.1` fixes the per-term phase assignment. Rows below index those two tables at per-symbol granularity; the `Kind` column distinguishes a **prose-only** retirement (docs + rustdoc only) from a **code-level** retirement (a public Rust symbol is renamed).

| Term / phrase | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `Kind` (bare) | Code symbol + prose | `DataKind` / `SimpleDataKind` / `ComplexDataKind` | Phase 0–1 (MINOR) | End-of-Phase-1 MAJOR | `40 §9.1` row 1 | "Kind" is overloaded across CS; always disambiguate (`00 §4.3`). |
| `kind` (bare, in documentation prose) | Prose-only | "data kind" / `DataKind` | Phase 8 (doc-only) | Phase 8 exit | `40 §9.1` row 1 | Same as above; bare `kind` in prose is ambiguous between DataKind and YAML `kind:` discriminator. |
| `Entity` (as a distinct vocabulary term) | Prose-only | "a named DataKind instance" (or the instance name directly) | Phase 8 (doc-only) | Phase 8 exit | `40 §9.1` row 2 | DDD / ORM overtones; the concept adds nothing over name + DataKind (`00 §4.3`). |
| `connector` | Code symbol + prose | `EngineAdapter` (engine axis) / `CatalogProvider` (metadata axis) | Phase 5 (engine side) / Phase 6 (catalog side) | End-of-Phase-5 MAJOR / End-of-Phase-6 MAJOR | `40 §9.1` row 3 + `[TD-ADAPTER-RENAME]` | Historically conflated two independent axes (`00 §4.3`). |
| `CompiledDataKind` | Code symbol | `ResolvedDataKind` | Phase 2 (MINOR, with alias) | End-of-Phase-2 MAJOR | `40 §9.1` row 4 | The `Compiled*` prefix implied a structural copy of the Model; Manifest types diverge structurally per I8 (`00 §4.3`, `33 §3`). |
| `CompiledInterface` | Code symbol | `ResolvedInterface` (per-Binding) / `ResolvedComposedInterface` (per-ComposedSemanticInterface, if materialized) | Phase 2 (MINOR, with alias) | End-of-Phase-2 MAJOR | `40 §9.1` row 4 | Same as above; name converges with `ResolvedSource` / `ResolvedColumnMapping` / `ResolvedQueryRequest` pattern (`00 §4.1`). |
| `CompiledManifest` | Code symbol | `Manifest` (top-level; the `Compiled` prefix is dropped entirely per `33 §3`) | Phase 2 (MINOR, with alias) | End-of-Phase-2 MAJOR | `40 §9.1` row 4 | The top-level artifact is `Manifest`; only internal / nested types keep the `Resolved*` prefix (`00 §4.1` note). |
| `CompiledSimpleKind` | Code symbol | `ResolvedSimpleKind` | Phase 2 (MINOR, with alias) | End-of-Phase-2 MAJOR | `40 §9.1` row 4 | Fast-path type documented in `DL-064`; rename tracks the class. |
| `CompiledColumnMapping` (if present) | Code symbol | `ResolvedColumnMapping` | Phase 2 (MINOR, with alias) | End-of-Phase-2 MAJOR | `40 §9.1` row 4 | Matches pre-existing `ResolvedColumnMapping` naming (`00 §4.1` row). |
| `Compiled*` (any remaining prefix instances) | Code symbol | `Resolved*` | Phase 2 (MINOR, with alias) | End-of-Phase-2 MAJOR | `40 §9.1` row 4 | Blanket catch-all; `40 §5.3` Phase-2 exit gate requires zero remaining `Compiled*` references. |
| `StorageProvider` | Code symbol + prose | `FileSystem` | Phase 6 (MINOR, with alias) | End-of-Phase-6 MAJOR | `40 §9.1` row 5 | Former name conflated generic I/O with format-aware schema reading; `FileSystem` is scoped to generic I/O only (`00 §4.3`, `37 §1.1`). |
| `simplify` (as pipeline verb) | Code symbol + prose | `optimize` | Phase 3 (MINOR, with alias) | End-of-Phase-3 MAJOR | `40 §9.1` row 6 | Single verb for plan rewriting (`00 §4.3`); `10 §3` ratifies the five-verb pipeline. |
| `dispatch` (as vocabulary-level verb) | Prose-only | removed (internal detail of `plan`; not a public verb) | Phase 3 (doc-only) | Phase 3 exit | `40 §9.1` row 7 | Strategy selection is an implementation detail, not a vocabulary-level verb (`00 §4.3`). |
| Positioning semstrait as "like Calcite / DataFusion / Trino / DuckDB" | Prose-only | "a semantic model / interface layer; peer group: dbt MetricFlow, Cube.js" | Phase 8 (doc-only) | Phase 8 exit | `40 §9.1` row 8 | Engines are consumers, not peers (`00 §3`, `00 §4.3`). |
| Engine IR terminology (`RelNode`, `Rel`, `Binder`, `Analyzer`, `Convention`, …) | Prose-only | Canonical term per `00 §4.1` | Phase 8 (doc-only) | Phase 8 exit | `40 §9.1` row 8 | Engine IR concepts inspire `PlanNode` structure where explicitly noted; engine terminology never enters our prose (`00 §4.3`). |
| `physical type` in any non-adapter doc | Prose-only | `DataType` (logical) | Phase 8 (doc-only) | Phase 8 exit | `40 §9.1` row 9 | I2: physical types live only in adapters. |
| `TemporalHistorization` | Code symbol + prose | `TemporalShape` | Phase 0 (MINOR, with alias) | End-of-Phase-1 MAJOR | `40 §9.1` row 10 + `17 §1.3` | Design vocabulary uses `TemporalShape`; `TemporalHistorization` was the internal code symbol (`00 §4.3`, `17 §1.3`). |

### 3.2 Code symbols — Rust type / function / trait names

Source: `40 §2`'s per-crate deviation catalogue (rows flagged "Diverges from the design") plus `40 §4` (`[CODE-DIVERGES-FROM-SPEC]` inventory). Each row below is a public Rust symbol slated for rename or removal; the register is **append-only** — a symbol added to §3.2 here is committed to at least one MINOR cycle of `#[deprecated]` existence before the MAJOR cut that removes it.

Rows are grouped by owning crate to match `40 §2`'s organization.

#### 3.2.1 `semstrait-core`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| Legacy `DataType` variant names (case-by-case; exact spelling tracked in Phase 1) | Enum variant | New 14-variant set per `13 §2.1` / `31 §4.1` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `40 §2.1` row 1 | Variant widening is MINOR via `#[non_exhaustive]`; removal of any legacy-only spelling that is not carried forward is MAJOR. |
| Unconditional `#[derive(Serialize, Deserialize)]` on core types | Attribute (not a symbol per se, but a public-surface break) | `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]` | Phase 1 (MINOR) | Not retired — gated behavior shift | `[TD-CORE-SERDE-GATING]` + `31 §11` | Per `30 §10.4`, serde is opt-in; `[TD-CORE-SERDE-GATING]` tracks the migration. Gating itself is a caller-visible change; see §5 (not a rename). |

#### 3.2.2 `semstrait-model`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `TemporalHistorization` enum | Enum | `TemporalShape` | Phase 0 (MINOR, with `pub use TemporalShape as TemporalHistorization;` shim) | End-of-Phase-1 MAJOR | `00 §4.3` + `17 §1.3` + `40 §9.1` row 10 | Vocabulary-rename; trivially `cargo fix`-compatible via `#[deprecated]` attribute. |
| `ScdType` enum | Enum | `ScdSubtype` | Phase 0 (MINOR, with alias) | End-of-Phase-1 MAJOR | `17 §1.3` (narrated divergence) | Matches `17 §2` ratified naming; aligns with `TemporalShape` rename. |
| `ModelError` | Enum | `ParseError` + `ValidateError` (split per stage) | Phase 1 (MINOR, conversion impl retained) | End-of-Phase-1 MAJOR | `32 §14.4` row 8 + `31 §8` | Single-error enum collapses two distinct stage concerns; split matches `10 §5`'s per-stage error shape. |
| `YamlJoinset.associativity: JoinAssociativity` field | Struct field + enum | `JoinsetSpec.anchor: DataKindRef` + `JoinsetSpec.path: JoinPath` | Phase 1 (MINOR, parser accepts both) | End-of-Phase-1 MAJOR | `32 §14.4` row 3 + `24 §2.2` | Associativity is implicit in anchor + path; explicit is author-clearer. |
| `ColumnMappingValue::Simple(String)` | Enum variant | `ColumnMappingValue::Column { column, grain }` | Phase 1 (MINOR, both accepted) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 + `32 §8.3` | Variant name standardization; shape carries grain forward explicitly. |
| `ColumnMappingValue::WithGrain { column, grain }` | Enum variant | `ColumnMappingValue::Column { column, grain }` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 | Collapsed into `::Column` with optional grain. |
| `ColumnMappingValue::Anchored(HashMap)` | Enum variant | `ColumnMappingValue::Computed { expr }` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 | Anchored composition IS a computed expression; no need for a distinct variant (`32 §8.3`). |
| Model-local `DataType` enum | Enum | `semstrait-core::DataType` (re-exported) | Phase 1 (MINOR, re-export alias) | End-of-Phase-1 MAJOR | `32 §14.4` row 5 + `31 §4.1` | Single source of truth for types; `semstrait-model` defers to `semstrait-core`. |
| `TemporalGrain` (model-local) | Enum | `semstrait-core::Grain` | Phase 1 (MINOR, re-export alias) | End-of-Phase-1 MAJOR | `32 §14.4` row 6 | Grain lives in core per `31 §4.2`; local duplication is legacy. |
| `Relationship.relationship_type:` field | Struct field (YAML-facing) | split into `cardinality:` + `join_type:` | Phase 1 (MINOR, parser accepts both) | End-of-Phase-1 MAJOR | `32 §14.4` row 7 + `16 §2.1` | `relationship_type:` conflated two independent axes per `16 §2.1`. |
| `Relationship.source_set:` field | Struct field (YAML-facing) | `from:` | Phase 1 (MINOR, parser accepts both) | End-of-Phase-1 MAJOR | `32 §14.4` row 7 | Idiomatic naming per `32 §7`. |
| `Relationship.target_set:` field | Struct field (YAML-facing) | `to:` | Phase 1 (MINOR, parser accepts both) | End-of-Phase-1 MAJOR | `32 §14.4` row 7 | Same as above. |

#### 3.2.3 `semstrait-manifest`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `CompiledManifest` | Struct | `Manifest` | Phase 2 (MINOR, with `pub type CompiledManifest = Manifest;` alias) | End-of-Phase-2 MAJOR | `00 §4.3` + `40 §9.1` row 4 | Top-level type drops `Compiled` prefix per `33 §3`. |
| `CompiledDataKind` | Struct | `ResolvedDataKind` | Phase 2 (MINOR, alias) | End-of-Phase-2 MAJOR | `00 §4.3` + `40 §9.1` row 4 | `Resolved*` prefix matches `ResolvedSource` / `ResolvedColumnMapping` (`00 §4.1`). |
| `CompiledInterface` | Struct | `ResolvedInterface` | Phase 2 (MINOR, alias) | End-of-Phase-2 MAJOR | `00 §4.3` + `40 §9.1` row 4 | Same as above. |
| `CompiledSimpleKind` | Struct | `ResolvedSimpleKind` | Phase 2 (MINOR, alias) | End-of-Phase-2 MAJOR | `40 §2.3` + `DL-064` | Fast-path type follows the prefix convention. |
| `RelationshipGraph` | Struct | `SemanticGraph` (pre-existing replacement; planner migration Phase 2) | Phase 2 (MINOR, already `#[deprecated]` in code) | End-of-Phase-2 MAJOR | `TD-007` (legacy) / `40 §2.3` | Deprecated in J4; removal gated on `TD-002`. |
| `FieldIndex` | Struct | Manifest name + Coverage indices per `33 §7` | Phase 2 (MINOR, already `#[deprecated]`) | End-of-Phase-2 MAJOR | `TD-007` (legacy) / `40 §2.3` | Same as above. |

#### 3.2.4 `semstrait-planner`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `SemanticPlanner` struct + its `.plan()` method | Struct + inherent method | free functions `plan(&Manifest, &Request) -> …` and `optimize(SemanticPlan, …) -> …` at crate root | Phase 3 (MINOR, struct retained as thin wrapper) | End-of-Phase-3 MAJOR | `[TD-PLANNER-SHAPE]` + `34 §17` | Free-function surface per `34 §6`; struct conflated shape with state. |
| `SemanticPlanner.catalog: Option<Arc<dyn CatalogProvider>>` field | Struct field | removed (planner does not hold a catalog handle; drift check is an `I11b` out-of-band entry) | Phase 3 (MINOR, field retained but unused) | End-of-Phase-3 MAJOR | `[TD-PLANNER-NO-CATALOG]` + `34 §16.4` | Planner operates through `Manifest` alone per I8 / I11. |
| `SessionVariables: HashMap<String, String>` | Type alias / struct | `SessionContext` (typed struct per `34 §4`) | Phase 3 (MINOR, alias retained) | End-of-Phase-3 MAJOR | `[TD-SESSION-CONTEXT]` + `34 §4.4` | Typed shape carries `now: DateTime`, `timezone`, `feature_toggles`, `correlation_id`. |
| `ResolvedQueryRequest` (public surface usage) | Struct | `Request` (caller surface) — `ResolvedQueryRequest` survives as crate-internal only | Phase 3 (MINOR, public alias retained) | End-of-Phase-3 MAJOR | `[TD-REQUEST-SHAPE]` + `34 §3` / `34 §5` | Caller surface is `Request`; post-lookup form remains internal. |
| `AdHocJoin` dispatch path (legacy `ad_hoc_join.rs`) | Module / type family | field-first resolution in `plan` per `34 §10` | Phase 3 (MINOR, path retained for one cycle) | End-of-Phase-3 MAJOR | `[TD-ADHOC-INTO-FIELD-FIRST]` + `34 §9.5` | Subsumed by field-first resolution; no distinct "ad-hoc strategy" in v1 taxonomy. |
| `simplify()` function | Function | `optimize()` | Phase 3 (MINOR, `#[deprecated]` wrapper forwards to `optimize`) | End-of-Phase-3 MAJOR | `40 §9.1` row 6 + `00 §4.3` | Verb rename; trivial forwarding shim. |

#### 3.2.5 `semstrait-ir`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `LogicalPlan` | Type | `SemanticPlan` (with `pub type LogicalPlan = SemanticPlan;` alias) | Phase 4 (MINOR) | Next MAJOR after Phase 4 (single-cycle alias per `30 §12`) | `[TD-IR-RENAME]` + `35 §3.1` / `35 §13` | Canonical vocabulary per `00 §4.1`; the alias transition is `cargo fix`-compatible. |
| `JoinNode.on: Vec<KeyPair>` without `residual` field (behavior, not a symbol) | Struct shape | `JoinNode.on: Vec<KeyPair>` + `JoinNode.residual: Option<PhysicalExpr>` (reserved, always `None` in v1) | Phase 4 (MINOR field addition; `#[non_exhaustive]` carries this freely) | n/a — additive | `[TD-IR-NON-EQUI-JOIN]` + `35 §4.6` | Additive extension; listed here because code reading `JoinNode` via exhaustive destructuring needs an update on the MINOR cycle. |

#### 3.2.6 `semstrait-adapter`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `PlanBuilder` (adapter-side construction path) | Struct / builder | Substrait-side path per `36 §5`; no direct replacement on the `EngineAdapter` surface | Phase 5 (MINOR, retained for the Substrait-transition cycle) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-PLAN-BUILDER-RETIRE]` + `40 §2.6` | Mechanism is Substrait-owned; removal is part of the Phase 5 MAJOR per `40 §6.2`. |
| `EngineAdapter::debug_sql` (trait method with default body) | Trait method | Free function `debug_sql(&SemanticPlan, …)` per `36 §3.5` | Phase 5 (MINOR, trait method `#[deprecated]`, default body forwards to free fn) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-DEBUG-SQL-FREE-FN]` + `36 §3.5` | Hoisted off the trait to avoid per-adapter override noise. |
| Legacy `EngineProfile` shape (as supertrait / mandatory `EngineAdapter` super-struct) | Trait / struct composition | `Dialect` as independent trait; `EngineAdapter::dialect() -> DialectId` where applicable | Phase 5 (MINOR, old shape accepted) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-DIALECT-SPLIT]` + `36 §2` | Dialect is an independent axis per `00 §4.1` / `36 §2`. |
| Adapter-trait / error naming (legacy naming from `DL-023`, `DL-055`) | Trait / enum renames (exact per-adapter symbols tracked in `40 §2.6`) | New naming per `36 §13` | Phase 5 (MINOR) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-RENAME]` + `[TD-ADAPTER-ERROR-MIGRATION]` | Absorbs the `36` rename pass; enumerated per-symbol at Phase 5 entry. |

#### 3.2.7 `semstrait-catalog`

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| `StorageProvider` trait | Trait | `FileSystem` trait | Phase 6 (MINOR, trait alias + blanket `impl FileSystem for T where T: StorageProvider`) | End-of-Phase-6 MAJOR | `40 §9.1` row 5 + `37 §1.1` | Scope rename; generic I/O only per `00 §4.3`. |
| Any `CatalogProvider` method signatures diverging from `37 §3` | Trait method signatures (enumerated at Phase 6 entry) | `37 §3` ratified surface | Phase 6 (MINOR, default-body bridges accepted) | End-of-Phase-6 MAJOR | `40 §2.7` + `37 §3` | Methods are renamed or re-scoped; default bodies ease the transition per `30 §11.4`. |

#### 3.2.8 `semstrait-api` / `semstrait` (facade)

| Symbol | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|---|
| Public-entry-point signatures that do not carry the `Result<(Output, Vec<Diagnostic>), (Error, Vec<Diagnostic>)>` shape | Function / method signatures | Warning-propagation shape per `questions/open/30 §Q-API-002` | Phase 7 (MINOR, old shape accepted via wrapper) | End-of-Phase-7 MAJOR (if `38`/`39` stabilize) | `40 §2.8` + `questions/open/30 §Q-API-002` | Diagnostic propagation is I12; the `Vec<Diagnostic>` carry is non-optional per `30 §7`. |

### 3.3 YAML surface keys

YAML keys do not carry `#[deprecated]` attributes; the `Deprecated` state is signaled through `PARSE_W_*` / `VALID_W_*` diagnostics at `parse` / `validate` time per `10 §5` and `30 §7`. Retirement is the moment the parser rejects the legacy key outright.

| Legacy YAML key / shape | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
|---|---|---|---|---|---|
| Top-level `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` blocks (implicit kind from block name) | Single top-level `data_kinds:` block with explicit `kind:` discriminator per `32 §4.2` | Phase 1 (MINOR, parser accepts both; `PARSE_W_*` on legacy) | End-of-Phase-1 MAJOR | `[CODE-DIVERGES-FROM-SPEC]` at `32 §14.4` row 1 / `40 §4.1` | Explicit discriminator is self-documenting; implicit-kind inference was a source of silent miscategorization. |
| `ChildEntry` / `datasets:` sub-block on a complex kind | `children:` list with `ref:` entries per `32 §5.2` / `32 §5.3` | Phase 1 (MINOR, parser accepts both) | End-of-Phase-1 MAJOR | `32 §14.4` row 2 | `children:` generalizes over every complex-kind composition. |
| `associativity: JoinAssociativity` on a Joinset | `anchor: DataKindRef` + `path: JoinPath` per `32 §5.4` / `24 §2.2` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 3 | Associativity is implicit in anchor + path. |
| `column_mapping.physical: <string>` (shorthand physical spelling) | `column_mapping[].expr: <ExprSource>` per `15` / `32 §8.3` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 | `expr:` unifies column-ref, literal, and computed forms; see `ColumnMappingValue` variant rename. |
| `ColumnMappingValue::Simple(String)` shorthand | `::Column { column, grain }` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 | See §3.2.2. |
| `ColumnMappingValue::WithGrain { column, grain }` | `::Column { column, grain }` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 | See §3.2.2. |
| `ColumnMappingValue::Anchored(HashMap)` | `::Computed { expr }` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 4 | Absorbs anchored-composition into Computed. |
| Legacy `DataType` spellings (`I8`, `I16`, `I32`, `I64`, `F32`, `F64`) in YAML `data_type:` fields | New 14-variant set per `13 §2.1` (`Byte`, `Short`, `Integer`, `Long`, `Float`, `Double`, …) | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 5 + `13 §2.1` | Spelling converges with author-facing docs. |
| `TemporalGrain: <variant>` YAML spelling | `grain: <variant>` per `13 §3.1` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 6 | Key simplification; type moves to `semstrait-core::Grain`. |
| `Relationship.relationship_type:` (YAML key) | split into `cardinality:` + `join_type:` per `16 §2.1` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 7 | Conflated two independent axes. |
| `Relationship.source_set:` (YAML key) | `from:` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 7 | Idiomatic naming per `32 §7`. |
| `Relationship.target_set:` (YAML key) | `to:` | Phase 1 (MINOR) | End-of-Phase-1 MAJOR | `32 §14.4` row 7 | Same as above. |

### 3.4 Error codes

Per `30 §6.3`: a published error code's meaning is **frozen at its first release**; a MAJOR may retire the code but never repurpose it. Per `30 §6.7` / `40 §9.2`: v1 introduces NO error-code retirements — every reserved code in `30 §6.2` is forward-looking, and every code that has shipped since the design tree was ratified survives v1 intact.

| Code | Retirement release | Replacement | Rationale |
|---|---|---|---|
| *(none)* | *(n/a)* | *(n/a)* | v1 introduces no error-code retirements; this table is reserved for retirements that surface in phase work per `40 §9.2`. |

If any phase discovers the need to retire a code, the retirement lands at the next MAJOR cut and this table gains a row, matching `30 §6.7`'s quick-lookup format. Deprecation (the code remains produced by the runtime, but a `#[deprecated]` attribute on its owning symbol signals forthcoming retirement) is MINOR per `30 §2.2` and lands a row of its own.

#### 3.4.1 Prefix-level moves (not retirements)

Two prefix-level reconciliation items from `40 §3.7` — `[TD-CAT-CODE-TABLE-AMEND]` (register `CAT_E_*` / `FS_E_*` in `30 §6.2`) and `[TD-IR-CODE-TABLE-AMEND]` (register `IR_E_3500`–`3599`) — are MINOR registrations, not retirements. No code is being retired; both items land a `42` entry noting the new prefix allocation.

### 3.5 APIs / traits — method deltas

Trait-method retirements are a subset of §3.2's code-symbol retirements but merit a focused view because a trait change cascades across every implementer.

| Trait | Retired method / signature | Replacement | Deprecation release | Retirement release | Tracking TD |
|---|---|---|---|---|---|
| `Repository` | legacy `save` / `load` signatures (exact roster per `33 §11`) | Signatures matching `33 §11`'s ratified surface (with `Diagnostic`-shaped errors per `30 §8.3`) | Phase 2 (MINOR, default-body bridges) | End-of-Phase-2 MAJOR | `40 §2.3` + `33 §11` |
| `CatalogProvider` | legacy method roster diverging from `37 §3` | `37 §3` ratified method set | Phase 6 (MINOR) | End-of-Phase-6 MAJOR | `40 §2.7` + `37 §3` |
| `FileSystem` (trait itself is new, replacing `StorageProvider`) | `StorageProvider` trait in full | `FileSystem` | Phase 6 (MINOR, blanket-impl bridge) | End-of-Phase-6 MAJOR | `40 §9.1` row 5 + `37 §1.1` |
| `EngineAdapter` | `debug_sql` trait method (default body) | Free function `debug_sql(&SemanticPlan, …)` | Phase 5 (MINOR, method `#[deprecated]`) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-DEBUG-SQL-FREE-FN]` + `36 §3.5` |
| `EngineAdapter` | `adapt` return-type variants that did not follow `EngineArtifact` sum-type shape | `EngineArtifact` (sum type) per `36 §3` | Phase 5 (MINOR) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-RENAME]` + `36 §3` |
| `EngineAdapter` | informal `Dialect` embedding in `EngineAdapter` | `Dialect` as independent trait; `DialectId` on every `SqlArtifact` per `36 §2` | Phase 5 (MINOR) | End-of-Phase-5 MAJOR | `[TD-ADAPTER-DIALECT-SPLIT]` + `36 §2` |

### 3.6 Docs paths — legacy-doc redirect roster

Per `00 §11`, legacy `docs/*.md` files are retired into the `docs/design/` tree. Retirement means the legacy file's body is reduced to a short redirect pointer; the file itself remains to avoid breaking external links.

| Legacy path | Target doc (redirect) | Retirement release | Tracking TD |
|---|---|---|---|
| `docs/ARCHITECTURE.md` | `docs/design/foundations/` + `docs/design/apis/` (short summary pointer) | Phase 8 | `00 §11` row 1 |
| `docs/DATASET.md` | `docs/design/data-kinds/21_dataset.md` | Phase 8 | `00 §11` row 2 |
| `docs/GRAINSET.md` | `docs/design/data-kinds/22_grainset.md` | Phase 8 | `00 §11` row 2 |
| `docs/UNIONSET.md` | `docs/design/data-kinds/23_unionset.md` | Phase 8 | `00 §11` row 2 |
| `docs/JOINSET.md` | `docs/design/data-kinds/24_joinset.md` | Phase 8 | `00 §11` row 2 |
| `docs/CATALOG_RESOLUTION.md` | `docs/design/foundations/15_mapping_and_binding.md` | Phase 8 | `00 §11` row 3 |
| `docs/COMPUTED_EXPRESSIONS.md` | `docs/design/foundations/14_expressions.md` | Phase 8 | `00 §11` row 4 |
| `docs/FUNCTION_CATALOG.md` | `docs/design/registry/functions_mapping.md` (per-engine mapping) + `docs/design/foundations/14a_function_catalog.md` (canonical catalog) | Phase 8 | `00 §11` row 5 |
| `docs/SEMANTIC_RESOLUTION.md` | `docs/design/foundations/11_names_and_scopes.md` + `12_nesting_policy.md` | Phase 8 | `00 §11` row 6 |
| `docs/TECH_DEBT.md` | Retained. Legacy `TD-0NN` entries migrate into `40 §3.9` (kept as tombstone) or are absorbed / closed by phase work. | Not retired | `00 §11` row 7 + `40 §3.9` |

## 4. Per-Retirement Table Format

Every row in `§3` conforms to the shape below. The column set is the minimum contract; per-category sub-sections (e.g. §3.4 error codes) may add columns, but never remove one.

```
| Symbol / term | Kind | Replacement | Deprecation release | Retirement release | Tracking TD | Rationale |
```

| Column | Definition | Source |
|---|---|---|
| `Symbol / term` | Fully-qualified path for code symbols (`crate::module::Symbol`); verbatim YAML key for YAML; stable error-code literal for codes; canonical phrase for prose. | Per-row authoritative. |
| `Kind` | One of: `Code symbol`, `Prose-only`, `YAML key`, `Error code`, `Trait method`, `Docs path`, `Code symbol + prose`. Enumerated in §§3.1–3.6. | This document. |
| `Replacement` | Fully-qualified path of the replacement, or `removed` (no replacement), or `n/a` (where retirement is conditional, e.g. error-code table). | Per-row authoritative. |
| `Deprecation release` | Workspace version (or phase, pre-1.0) in which `#[deprecated]` (or equivalent) first shipped. | `40 §5` phase schedule. |
| `Retirement release` | Workspace version (or phase) at which the symbol is removed. `Not retired` is a valid value for §5 grandfathering entries. | `40 §5` phase schedule. |
| `Tracking TD` | The `[TD-*]` tag in `40 §3` (or `00 §4.3`, or `40 §9.1`) anchoring the retirement. `40 §7` lists the authoritative tag roster. | Cross-referenced. |
| `Rationale` | One-line authoritative rationale. Longer-form discussion lives in the originating doc. | Per-row authoritative. |

### 4.1 Row-authoring discipline

- Every row lands in the **same PR** as the `#[deprecated]` attribute (or parser-warning landing, for YAML retirements; or doc-level admonition, for prose retirements). Per `30 §12.2`, an unregistered deprecation is a discipline violation.
- Rows are **append-only** during a phase. Editing a row in-place is permitted only to update the `Retirement release` column when a MAJOR cut slips or advances.
- Tombstone rows (post-retirement) gain a `retired-in:` footnote; the row otherwise remains.

## 5. Exceptions and Grandfathering

Symbols in this section carry a `#[deprecated]` attribute (or acquire the `Deprecated` state in a way that matches §2.1) but are **NEVER retired**. They persist as public-surface aliases indefinitely.

| Symbol | State | Why grandfathered | Tracking TD |
|---|---|---|---|
| `MeasureConstraints` struct name | Active (renamed internally to generalize across Measure + Metric carriers, but public name kept for v1) | Rename deferred to the broader Manifest-schema revision pass (post-v1) to avoid a breaking rename before `33` stabilizes. | `[TD-CONSTRAINT-RENAME]` + `11 §8.4.3` / `31 §6.1` |
| `SourceId::ModelInline { label: &'static str }` | Active | Test-harness / inline-string parse path; retained as `#[non_exhaustive]` member per `30 §5.3`. Not a rename target. | `30 §5.3` |
| `kind: dataset` YAML value (synonym of `kind: simple`) | Active — permanent synonym | Author-ergonomic; `32 §5.1` ratifies both spellings as permanent in v1. Retirement would break authored Models without benefit. | `questions/open/32 §Q-MODEL-003` |
| `pub use TemporalShape as TemporalHistorization;` alias in `semstrait-model` | Transitional (retired at end-of-Phase-1 MAJOR) | NOT grandfathered — listed here only to distinguish from the permanent aliases above. Row belongs to §3.2.2; included here to make the exception list unambiguous. | `[40 §9.1 row 10]` |
| `pub type LogicalPlan = SemanticPlan;` alias in `semstrait-ir` | Transitional (retired next MAJOR after Phase 4) | NOT grandfathered — one-MINOR-cycle transition per `30 §12`. Row belongs to §3.2.5. | `[TD-IR-RENAME]` |
| `TD-004` (expect-panic messages) | Active — not a rename | Programming-error panics; `40 §3.9` closes opportunistically when touched, not as a scheduled retirement. | `TD-004` (legacy) |
| `TD-006` (active_bindings allocates) | Active — not a rename | Non-blocking allocation pattern; closes opportunistically in Phase 3 per `40 §3.9`. | `TD-006` (legacy) |

**Audit clause.** The grandfathered roster above is **closed** as of Round-1 freeze. Additions to §5 require an explicit `41` amendment citing the exception condition. Adding a symbol to §5 post-hoc (after it was originally scheduled for retirement in §3) requires the originating design doc to be amended first (per `00 §8`).

## 6. Migration Aids

Migration aids reduce caller churn when a rename ships. Three tiers:

| Tier | Mechanism | When it applies | Cost to caller |
|---|---|---|---|
| **Automatic** | `#[deprecated(since, note)]` with `note` carrying the replacement path; rustc emits a fixable warning. `cargo fix --edition` / `cargo fix --broken-code` can auto-apply on imports. | Name-only renames where the new symbol has an identical shape (type alias; re-export). | Zero (if `cargo fix` is run); otherwise one warning-to-error cycle. |
| **Semi-automatic** | Rustfix / `suggestion:` hints on the `#[deprecated]` attribute (Rust 1.77+ MSRV permitting); IDE quick-fixes via rust-analyzer. | Renames that pair with a subtle signature change (e.g. `fn foo() -> X` → `fn foo() -> Result<X, Diagnostic>`). | Caller reviews the quick-fix; accepts or declines per call site. |
| **Manual** | Parser-level grammar duplication; `42_migration_notes.md` before/after examples; codemod scripts where feasible. | YAML grammar changes; trait-method-signature changes; enum-variant reshuffles. | Caller follows `42` examples; no rustc assist. |

### 6.1 Codemod-compatible renames

The following renames are pure name changes — the replacement has an identical shape and an automatic `#[deprecated]` path is sufficient. Callers running `cargo fix` on a post-rename MINOR release pick up every rewrite.

| Rename | Mechanism | Cycle |
|---|---|---|
| `TemporalHistorization` → `TemporalShape` | `pub use TemporalShape as TemporalHistorization;` + `#[deprecated]` | Phase 0 |
| `ScdType` → `ScdSubtype` | `pub use ScdSubtype as ScdType;` + `#[deprecated]` | Phase 0 |
| `StorageProvider` → `FileSystem` | trait alias + blanket impl + `#[deprecated]` | Phase 6 |
| `Compiled*` → `Resolved*` (every type in `40 §2.3`) | `pub type CompiledX = ResolvedX;` + `#[deprecated]` | Phase 2 |
| `LogicalPlan` → `SemanticPlan` | `pub type LogicalPlan = SemanticPlan;` + `#[deprecated]` | Phase 4 |
| `simplify()` → `optimize()` | `#[deprecated] pub fn simplify(...) { optimize(...) }` | Phase 3 |
| `SessionVariables` → `SessionContext` (type alias) | `#[deprecated] pub type SessionVariables = SessionContext;` — **rustc-compatible only if the two shapes coincide; see §6.2** | Phase 3 |

### 6.2 Renames that require manual migration

| Rename / change | Why manual | Caller impact |
|---|---|---|
| `SemanticPlanner` struct → free `plan()` / `optimize()` functions | Method-dispatch syntax (`planner.plan(req)`) vs function-call syntax (`plan(&manifest, &req)`); `cargo fix` cannot synthesize the borrow change. | Caller rewrites every call site; `42` ships a sed-style migration recipe. |
| `SessionVariables: HashMap<String, String>` → structured `SessionContext` | Field access (`.now`, `.timezone`, …) does not map from `HashMap::get`. | Caller rewrites every session-variable read; `42` maps each legacy string key to its `SessionContext` field. |
| `Relationship.relationship_type:` (YAML) → `cardinality:` + `join_type:` | YAML is not Rust; no `cargo fix` assist. | Model authors run a YAML codemod (shipped with `42`) or update manually; parser accepts both shapes for one MINOR cycle with `PARSE_W_*`. |
| `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` top-level blocks → `data_kinds:` + `kind:` discriminator | Same as above. | YAML codemod + parser dual-accept. |
| `ColumnMappingValue` variant rename (`Simple` / `WithGrain` / `Anchored` → `Column` / `Computed`) | Exhaustive match arms at caller sites break; `#[non_exhaustive]` protects MINOR additions but not removals. | `42` lists every match-arm shape; caller adds wildcard arms per I10. |
| `ModelError` → `ParseError` + `ValidateError` split | Error-type-routing logic at caller sites (`match e { ModelError::X => ... }`) does not auto-split. | Caller adopts per-stage error handling; `42` lists the split matrix. |
| `EngineAdapter` trait shape changes (Dialect split, `debug_sql` hoist, `EngineArtifact` return-type) | Trait-impl migration requires per-adapter changes. | Adapter authors rewrite `impl`s; `36 §13` + `42` walk through. |

### 6.3 CI assists

`40 §7.4` already lists the CI tooling that feeds `41` discipline:

- `cargo-semver-checks` — surface-stability check at every release-boundary; catches unregistered retirements.
- `clippy` / rustdoc — `#[deprecated]` warnings surface at build time.
- Post-Phase-8 documentation linter — scans rustdoc + markdown for banned terms per `00 §4.3`.
- YAML grammar dual-accept harness (Phase 1) — fixtures under `tests/golden/model/` exercise both legacy and new shapes; dropping a fixture from one corpus is the signal that the retirement gate is about to fire.

## 7. Tracking Table — cross-reference to `[TD-*]` tags

Every row in §3 anchors to a `[TD-*]` tag from `40 §3` (or to a banned-terms entry in `00 §4.3` and `40 §9.1`). This section is a flat index for rapid lookup.

| `[TD-*]` tag (from `40 §3`) | Register rows in §3 | Owning phase |
|---|---|---|
| `00 §4.3` + `40 §9.1` row 1 (`Kind` bare) | §3.1 row 1 | Phase 0–1 |
| `00 §4.3` + `40 §9.1` row 2 (`Entity`) | §3.1 row 3 | Phase 8 |
| `00 §4.3` + `40 §9.1` row 3 (`connector`) | §3.1 row 4 | Phase 5 + Phase 6 |
| `00 §4.3` + `40 §9.1` row 4 (`Compiled*`) | §3.1 rows 5–9; §3.2.3 rows 1–4 | Phase 2 |
| `00 §4.3` + `40 §9.1` row 5 (`StorageProvider`) | §3.1 row 10; §3.2.7 row 1; §3.5 row 3 | Phase 6 |
| `00 §4.3` + `40 §9.1` row 6 (`simplify`) | §3.1 row 11; §3.2.4 row 6 | Phase 3 |
| `00 §4.3` + `40 §9.1` row 7 (`dispatch`) | §3.1 row 12 | Phase 3 |
| `00 §4.3` + `40 §9.1` row 10 (`TemporalHistorization`) | §3.1 row 16; §3.2.2 row 1 | Phase 0 → Phase 1 MAJOR |
| `17 §1.3` (`ScdType`) | §3.2.2 row 2 | Phase 0 → Phase 1 MAJOR |
| `[CODE-DIVERGES-FROM-SPEC]` at `32 §14.4` | §3.2.2 rows 3–11; §3.3 rows 1–11 | Phase 1 |
| `[TD-CORE-SERDE-GATING]` | §3.2.1 row 2 | Phase 1 |
| `[TD-CONSTRAINT-RENAME]` | §5 row 1 (grandfathered — not retired in v1) | deferred post-v1 |
| `TD-001` (legacy `KindRef.variant` serde) | Absorbed by §3.2.3 (Phase 2 rewrite); tombstone in `40 §3.9`. | Phase 2 |
| `TD-002` (legacy `SemanticGraph` serde skip) | Absorbed by §3.2.3 (Phase 2 rewrite); tombstone in `40 §3.9`. | Phase 2 |
| `TD-005` (legacy AVG always Number) | Absorbed by Phase 1 `DataType::Decimal` variant (`13 §2.1`); not a rename. | Phase 1 |
| `TD-007` (legacy `RelationshipGraph` / `FieldIndex`) | §3.2.3 rows 5–6 | Phase 2 |
| `[TD-PLANNER-SHAPE]` | §3.2.4 row 1 | Phase 3 |
| `[TD-PLANNER-NO-CATALOG]` | §3.2.4 row 2 | Phase 3 |
| `[TD-SESSION-CONTEXT]` | §3.2.4 row 3; §6.2 row 2 | Phase 3 |
| `[TD-REQUEST-SHAPE]` | §3.2.4 row 4 | Phase 3 |
| `[TD-ADHOC-INTO-FIELD-FIRST]` | §3.2.4 row 5 | Phase 3 |
| `[TD-IR-RENAME]` | §3.2.5 row 1; §6.1 row 5 | Phase 4 |
| `[TD-IR-NON-EQUI-JOIN]` | §3.2.5 row 2 (additive, not a retirement) | Phase 4 |
| `[TD-ADAPTER-PLAN-BUILDER-RETIRE]` | §3.2.6 row 1 | Phase 5 |
| `[TD-ADAPTER-DEBUG-SQL-FREE-FN]` | §3.2.6 row 2; §3.5 row 4 | Phase 5 |
| `[TD-ADAPTER-DIALECT-SPLIT]` | §3.2.6 row 3; §3.5 row 6 | Phase 5 |
| `[TD-ADAPTER-RENAME]` + `[TD-ADAPTER-ERROR-MIGRATION]` | §3.2.6 row 4; §3.5 row 5 | Phase 5 |
| `[TD-CAT-CODE-TABLE-AMEND]` | §3.4.1 (prefix-level move; not a retirement) | Phase 6 |
| `[TD-IR-CODE-TABLE-AMEND]` | §3.4.1 (prefix-level move; not a retirement) | Phase 4 |
| `00 §11` row 1 (`docs/ARCHITECTURE.md`) | §3.6 row 1 | Phase 8 |
| `00 §11` row 2 (`docs/{DATASET,GRAINSET,UNIONSET,JOINSET}.md`) | §3.6 rows 2–5 | Phase 8 |
| `00 §11` row 3 (`docs/CATALOG_RESOLUTION.md`) | §3.6 row 6 | Phase 8 |
| `00 §11` row 4 (`docs/COMPUTED_EXPRESSIONS.md`) | §3.6 row 7 | Phase 8 |
| `00 §11` row 5 (`docs/FUNCTION_CATALOG.md`) | §3.6 row 8 | Phase 8 |
| `00 §11` row 6 (`docs/SEMANTIC_RESOLUTION.md`) | §3.6 row 9 | Phase 8 |

## 8. Round-1 Open Items

See `docs/design/questions/open/41_questions.md`. Each item is a policy question about the register's discipline, not a design re-open.

| # | Title | Parked item |
|---|---|---|
| Q-41-001 | Tombstone retention horizon | Round 1: one MAJOR past removal. Longer horizons deferred; see §2.6. |
| Q-41-002 | Alias mechanism preference — `pub type` vs `pub use` | Per-rename author discretion in Round 1; case-by-case; see §6.1. |
| Q-41-003 | YAML legacy-grammar cycle length | Round 1: one MINOR cycle; extension deferred per `40 §5.2`. |
| Q-41-004 | `MeasureConstraints` grandfathering vs v2 rename | Grandfathered in v1 (§5); v2 schedule pending `33` follow-up. |
| Q-41-005 | Error-code retirement cadence | Round 1: no retirements; policy reserved against future need per `30 §6.7`. |
| Q-41-006 | `cargo fix` rustfix-suggestion opt-in | Round 1: opt-in per rename; no blanket rule; see §6.1. |
| Q-41-007 | Retrospective `#[deprecated]` backfill on already-renamed-internally symbols | Round 1: no retrospective backfill; renames that shipped pre-Phase-0 are not re-deprecated. |
| Q-41-008 | Retirement register pruning policy | Round 1: never prune retired rows; tombstones stay past the one-MAJOR horizon unless explicitly reviewed out. |

---

*Cross-references in this document are by section (e.g. `00 §4.3`, `30 §12.2`, `40 §9.1`, `32 §14.4`). No code-path references are used, per `00 §8`.*
