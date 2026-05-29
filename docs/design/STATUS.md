# Spec-Driven Development — Status

Living handoff file for active design work.

Read order for spec sessions:

1. `[00_overview.md](00_overview.md)`
2. `[STATUS.md](STATUS.md)`
3. `[INDEX.md](INDEX.md)` for task routing
4. `[DOCS_MAINTENANCE.md](DOCS_MAINTENANCE.md)` for editing discipline

Long-form narrative archive: `[_archive/STATUS_HISTORY.md](_archive/STATUS_HISTORY.md)`.

---

## 1) Current phase

**Phase:** Reconciliation / consolidation (post-ratification cleanup).

**Stable:**

| Area | State |
| --- | --- |
| DataKind taxonomy + trait axes | Ratified |
| Typed diagnostics + `tracing` discipline (`30`–`39`) | Ratified |
| Per-Q-ID question-directory split | In place |
| `Dataset` (`21`), `Unionset` (`23`), `Grainset` (`22`) variant chapters | Slim, algorithm bodies in `_drafts/34_*_strategy.md` |
| `UnionMode { All, Unique }` v1 roster | Re-confirmed 2026-05-03 |
| `CompositionKind { Joinset, Grainset }` (Unionset variant retired) | Ratified 2026-05-03 |
| Grainset cross-grain LEFT OUTER JOIN composition (G-2) | Ratified 2026-05-03 |
| Expression-flow `foundations/19_expression_flow.md` | Promoted 2026-05-12 |
| Builder ergonomic facade (`32 §9.7.8`) | Landed 2026-05-13 |
| IR implementation review (post-Q cascade) — `AggregateKind` carrier + n-ary `AggregateExpr.args` + typed `Literal` width discriminators + structural validation extensions + decimal-family catalog | Complete 2026-05-26 (item S) |
| Manifest contract ratification (lightweight-manifest design pass) — 20 clauses C1–C18 + CCK + CX1; identity primitives on disk, SemanticGraph builds runtime structure in memory | Complete 2026-05-28 (item T) |

**Active:**

- Id-first identity rework (item U) — spec landed in `32`/`18` + cascades; the manifest is now unified on a single `EntityId` lane (item U.2 below). Pending: reconcile first-match-wins / `RelationshipId` allocation (`16 §11`, `18 §2.1`) and Unionset/Grainset child order (`22 §3.1`, `23 §3.1`) to the new name-ordered basis; finish the compile-internal `BindingId` cleanup in `15`/`19` (entangled with pre-existing `ResolvedExprTable`↔`ManifestExpressions` drift); ratify the UUID-format relaxation (authored UUIDv7 vs deterministic generated variant).
- Adapter/catalog framing reconciliation (item C).
- Residual cross-doc vocabulary cleanup (retired error-code language).
- v1 backlog trimming in `open/` sidecars.
- `Joinset` (`24`) variant rebase (last remaining; `33`/`34` follow).
- Algorithm-body sidecars (`_drafts/34_*_strategy.md`) await lift into `34_semstrait_planner.md` Strategy chapter.
- `16 §9.3` / `§10.5` / `§13` deeper cleanup parked under `Q-COMP-006`.
- Stale `CompositionKind` / `ComposedSemanticInterface` references in `33` pending its rebase.
- `18 §5.2 AdditivityType` → `Additivity` rename (planner-doc rebase).
- `[TD-19-ADVISORY-FIELDS]` advisory payload schema — deferred to `34` rebase.
- `[TD-19-ADVISORY-SPECIALISATION]` per-DataKind advisory split — flagged.
- `30 §6` typed-diagnostics codification (numeric codes as adjacent comments, not runtime fields) — next session.
- **Phase 3.5b — spec 16 structural drift sweep.** Wave 2 Agent E flagged broader prose drift outside the targeted-edit scope (item T). §10 preamble (lines 1417-1423), §10.1 (1430), §10.2 (1446-1460), §11 preamble (1626-1630), §11.1 inputs, §15.5 (2250-2253) still describe compositions as manifest-persisted; frontmatter `refined-by:` line 27 still says 33 "persists Relationship, Joinset, ComposedSemanticInterface". Targeted amendment notes (C9.2 / C10.2) landed at §9.1, §10.4, §11.4 only; full sweep needed.
- **Phase 2 research candidates queued** (see `_research/manifest/RATIFICATION_LOG.md` Phase 2 table). Highest leverage: Target C (Iceberg / Delta-Lake schema-evolution) → informs C3, C14, C15. Then Target A (small-cardinality bitmap registries) → C4, C5–C8; Target B (dual expression-form persistence) → C11, C12; Target D (hop-depth caps) → C10; Target E (composition-graph cycle detection) → C13 G1.

---

## 2) Active reconciliation items

| Item | Summary | Status | Primary docs |
| --- | --- | --- | --- |
| A | YAML surface and type hierarchy alignment | Ratified | `32`, `32b`, `26`, `20`–`25` |
| B | `Binding` → `SemanticMapping` framing | Ratified (authoring level) | `15`, `18`, `32`, `33` |
| C | Adapter/catalog architecture framing | **Open** | `30`, `36`, `37`, `39`, `42` |
| D | `Dataset` naming consistency | Ratified | `20`–`25`, `32`, `33` |
| E | Constraints model depth | Deferred | `11`, `10`, `13`, `18`, `32` |
| F | Nesting shape rules (R1/R2/R3) | Ratified | `26`, `32`, `22`–`24` |
| G | I/O transport + `semstrait-common::io` posture | Ratified | `31b`, `31`, `32`, `33` |
| H | Canonical entity type set | Ratified | `18` (+ cascades) |
| I | Typed diagnostics + `tracing` observability | Ratified; older-prose cleanup pending | `30`–`39`, selected `10`/`13`/`14*`/`15`/registry |
| J | `14 §2` type-shape rebase (parameterized `Expr<L>` + per-kind typed `SemanticLeaf`) | Ratified 2026-05-18 (cascade in N) | `14_expressions.md` |
| K | Relationship-block rebase (semantic-first; drop `directionality` + per-hop overrides; derive `JoinType` from `optional`) | Ratified 2026-05-12 | `18`, `16`, `24`, `32`, `33` |
| L | `semstrait-model` spec implementation | Complete 2026-05-12 | `31 §6`, `32`, `32b`, `crates/semstrait-{common,model}` |
| M | Builder ergonomic facade | Complete 2026-05-13 | `32 §9.7.*`, `crates/semstrait-model/` |
| N | Expression compile-pipeline cascade — `14b`+`19` merged into `19_expression_flow.md`; vocabulary rebased to typed-leaf shape; `35` absorbs expression vocabulary; `31` shrinks to primitives + traits + diagnostics + constraints + `io` | Complete 2026-05-18 | `14b` (retired stub), `19`, `14`, `35`, `31`, `15`, `33`, `34`, `INDEX.md` |
| O | Expression-spec consolidation pass — `14` / `14a` / `19` / `33` / `34` trim; function-level `Additivity` ratified at `14a §3.6`; ownership moves: `Provenance` → `33 §6.3.1`, `RequestDimensionRef` → `34 §3.10`; line-count delta: `14` 794→743, `14a` 365→244, `19` 1384→849 | Complete 2026-05-18 | `14`, `14a`, `19`, `18`, `33`, `34` (+ cross-ref cleanups) |
| P | Expression architecture cleanup, first cascade — non-coercion rule at `14 §5.4`; ~15 dangling `14 §5.6` retargets; Aggregate synthesis at `32 §5.4`; Option A confirmed (no `ExprBlock` parallel AST) | Complete 2026-05-18 (superseded-in-spirit by Q) | `14 §5.4`, `32 §5.4`, ref retargets across `10`/`11`/`13`/`14a`/`19`/`23` |
| Q | Expression architecture, second cascade (full Option A landing) — every expression-tree-tied type moved to `semstrait-ir` (trait family, structural support enums, `Literal`, identifier carriers, narrow `ValidateError`/`CompileError`); `ExprBlock` deleted; `ExprSource::Block(Expr<L>)` carries `Expr<L>` directly. `*ErrorKind` global rename remains a separate post-v1 sweep. | Complete 2026-05-19 | `31`, `35`, `14 §9`, `32`, `33`, `19`, `30`, `37`, `38`, `39`, `39b`, `INDEX.md`, `questions/closed/31_questions.md` |
| R | IR redesign Phase 0–6 — Phase-0/1 ratified S1–S8 (`35 §1.5`) + R1–R4 (`§1.6`); `§10.1.1` `SemAnnotation` inventory; `§14.3` annotation Substrait carrier; Q4.A–Q4.F ratified; Q4.G withdrawn. Phase-2: closed Q-IR-002/006/007/010/014. Phase-3: `35` consistency sweep (RegistryExtension declarative shape; `IR_E_3510`→`IrErrorKind::FetchValueOutOfRange`; `BindingId` stripped from public surface). Phase-4: 47 canonical functions ratified at `14a §4.2`–`§4.6`; per-engine rows in `registry/functions_mapping.md`; Spark floor 3.4+. Phase-5: `Capability` split — adapter-internal rewrite strategies removed (CTE/GROUPING SETS/DISTINCT-aggregate); kept irreducible cross-boundary features (RegexpMatch/RegexpExtract/IntervalLiteral/AsOfJoin/StructAccess); `Q-ADAPT-002` closed. Phase-6: 8 verification cleanups (INDEX +5 rows; `Expr<L>` body deduplicated to forward pointer in `14 §3.3`; `36 §12.4` Delta deletion; opening-status compression in `35`/`31`/`19`/`14a`). | Complete 2026-05-21 | `35`, `36`, `14`, `14a`, `19`, `31`, `INDEX.md`, `questions/{open,closed}/{35,36}_questions.md`, `registry/functions_mapping.md` |
| S | IR implementation review (post-Q cascade) — three tactical waves landed in `crates/semstrait-ir`: (W1) `AggregateKind { Builtin, Extension }` hybrid carrier preserving closed-five `AggregationOp`; `AggregateExpr.args: Vec<PhysicalExpr>` (n-ary admits extension aggregates); typed `Literal::{Null, Integer, Float}` with `IntegerWidth` / `FloatWidth` discriminators; `closed_five_*` lookup helpers reserved for Phase B Strategy. (W2) `ValuesNode.rows: Vec<Vec<Literal>>` narrowed; new structural validations: `values_row_arity`, `agg_duplicate_name` across `group_by ∪ aggregates`, `join_nullability` widening on outer joins (metadata-only, V-7 preserved); `SemanticExprAccessorExt` deleted; dead `time_default` removed; `RegistryExtension` removed from crate-root re-export. (W3) `ReturnTypeRule::DecimalScaleZero` + `ParamType::DecimalFamily` for ceil/floor/round/median Decimal overloads; `PartialEq` dropped from `ReturnTypeRule` and `FunctionSpec` (fn-pointer non-comparability); `FunctionSpec.description` field removed; `Arc<Schema>` value-shared invariant doc on `NodeMeta.output_schema`; 41 derive-driven tests deleted with no behavior loss. Spec sync: `35 §3.4 / §11.7` and `14a §3.1 / §3.4 / §3.5 / §4.3` reconciled. Cumulative state: 303 lib tests pass, clippy clean, V-5 / V-7 hold; 10 deferred `Q-IR-IMPL-*` / `Q-IR-SPEC-*` items opened in `questions/deferred/35_questions.md`. | Complete 2026-05-26 | `crates/semstrait-ir`, `35`, `14a`, `questions/deferred/35_questions.md`, `docs/design/implementation/35_ir_review*.md` |
| U | Id-first named-entity identity rework (`32` / `18` + cascades) — supersedes the `SemanticModelIdentityIndex` sidecar + `EntityHandle`/`SemanticModelStorage` builder machinery introduced in commit `0b519c3`. Identity is now an `id: EntityId` field carried directly on every named entity (`DataKindBase`; `Dimension`/`Measure`/`Metric`/`Relationship`/`Key`/`Filter`; `*Ref` ref-sites). Every model collection — top-level data kinds, shared pools, nested children, relationships, and `SemanticInterface`/`Keys`/aggregation-filter member collections — is a `BTreeMap<EntityId, _>`; the `id` is the storage key. Iteration/serialization is name-ordered (`32 §7`) so I4 stays byte-stable. Duplicate-by-name is an explicit per-layer scan at `.build()` (`32 §9.0`). **Open follow-ups:** (1) relationship first-match-wins (`16 §11`) and `RelationshipId` allocation (`18 §2.1`) now resolve in name order rather than YAML author order — flagged for `16`/`18` reconciliation; (2) Unionset/Grainset intra-variant child order for `PlanNode::Union` inputs (`22 §3.1` / `23 §3.1`) is now name-ordered rather than author-ordered. Status: spec landed; consequence reconciliation pending. | In progress | `apis/32_semstrait_model.md`, `foundations/18_entities.md`, `apis/33_semstrait_manifest.md`, `data-kinds/{22,23,24,26}_*.md`, `INDEX.md` |
| U.2 | Manifest single-id-lane unification (`33` + cascades) — extends item U into the manifest. Deletes the `ManifestStableIds` side-map and removes the per-kind compile-id newtypes (`DataKindId`/`SemanticsId`/`SemanticInterfaceId`/`BindingId`/`SourceId`/`RelationshipId`) from the manifest. Every manifest collection is `BTreeMap<EntityId, _>`, every cross-reference is an `EntityId`, and each entity carries `id: EntityId` (its key); top-level `model_id` added. The lone surviving integer is the bitmap `bit_position` ordinal (`SemanticBitmap` keyed by `EntityId`, reverse `bit_position→EntityId` derived at load; word-math unchanged). Compile-synthesised entities (sources, bindings, interfaces, implicit Unionsets, synthesised fields, `model_id`) get **deterministic content-derived** ids to preserve I4 — requiring a UUID-format relaxation (authored = UUIDv7, generated = deterministic variant). Expression-pool ids (`SemanticExprId`/`PhysicalExprId`) are the one deliberate exception (content-dedup handles). Cascades: `35 §2A` graph payload refs + `§15.4` wire line → `EntityId`; `18 §2.1` `RelationshipId` reframed as runtime/graph-build handle (not manifest key); `15 §2.2` binding identity = `EntityId` (compile-internal `BindingId` cleanup flagged). **Open follow-ups:** UUID-format relaxation ratification; compile-internal `BindingId`/`ResolvedExprTable` cleanup in `15`/`19` (entangled with pre-existing item-T expression-storage drift). | In progress | `apis/33_semstrait_manifest.md`, `apis/35_semstrait_ir.md`, `foundations/15_mapping_and_binding.md`, `foundations/18_entities.md`, `apis/32_semstrait_model.md`, `INDEX.md` |
| T | Manifest contract ratification (lightweight-manifest design pass) — Phase 1 closed all 20 clauses (C1–C18 + CCK + CX1); see `_research/manifest/RATIFICATION_LOG.md` for clause-by-clause picks. Major direction: identity primitives on disk (interfaces, bindings, sources, expressions, relationships, bitmaps); SemanticGraph constructs nodes+edges + JOIN-tree + ComposedSemanticInterface + implicit-composition enumeration at build time. Phase 3 cascade landed: `33` full rewrite (Wave 1A); `35` additive C12.3 newtype IDs + C18 serde derive contract + CX1 load-time integrity (Wave 1B); `15 §2.5` model-as-truth fields when catalog absent (Wave 1C / Thread A); `20` CCK skeleton cross-references (Wave 2D); `22 §1.3` I8 amendment for C8.2 JOIN-tree-deferred-to-graph-build (Wave 2D); `23 §2.1` row A confirmation of implicit-Unionset top-level per C6.5/C9.4 (Wave 2D); `24 §1.4 / §2.4` I8 + C7.4 ComposedSemanticInterface-deferred amendment (Wave 2D); `16 §9.1 / §10.4 / §11.4` C9.2 + C10.2 implicit-composition-enumeration-moves-to-graph-build amendments (Wave 2E); 3 questions closed citing clause IDs (`closed/15`, `closed/16`, new `closed/33`); deferred threads + Phase 2 research candidates recorded across `deferred/15` (appended), `deferred/16` (new), `deferred/32` (appended), new `deferred/33`; INDEX.md dashboard counts updated. **Open follow-up: Phase 3.5b spec-16 structural drift sweep** — Wave 2 Agent E flagged broader prose drift in §10 preamble, §10.1, §10.2, §11 preamble, §11.1, §15.5, and frontmatter `refined-by:` outside the targeted-amendment scope. **Open follow-up: Phase 2 research not yet launched** — Targets A–E queued in RATIFICATION_LOG.md. | Complete 2026-05-28 (Phase 1 + Phase 3 cascades from Phase 1); Phase 3.5b sweep + Phase 2 research pending | `_research/manifest/RATIFICATION_LOG.md`, `apis/33_semstrait_manifest.md`, `apis/35_semstrait_ir.md`, `foundations/15_mapping_and_binding.md`, `foundations/16_composition.md`, `data-kinds/{20,22,23,24}_*.md`, `questions/{closed,deferred}/{15,16,32,33}_questions.md`, `INDEX.md` |

---

## 3) Deferred topics

### 3.1 Constraints design

Status: deferred to dedicated session. Working context: `[questions/deferred/11_questions.md](questions/deferred/11_questions.md)`. Resume points: `aggregation` sub-block semantics; `aggregation` vs `aggregations` / `all` vs `all_of`; `constraints.filter` scope vs entity-level fields.

### 3.2 Manifest deferred threads (post-ratification, item T)

Carried forward from `_research/manifest/RATIFICATION_LOG.md`'s "Deferred Threads" table:

- **Thread A — model-as-truth posture** (foundations cascade beyond `15 §2.5` targeted edit; foundations sweep deferred).
- **Thread B — glob expansion semantics** (compile-time vs runtime; synthetic table name shape; resurfaces at C14/C15 catalog/versioning).
- **G5 workflow-friction watch** — orphan-detection-as-error friction in iterative authoring; revisit if reported.
- **G6 relationship-graph cycle detection** — separate from C13/G1 (which scopes to SemanticExpr cycles).
- **Duplicate-authoring diagnostic** — C18.5 cascade; whether duplicate authoring is a diagnostic surface is a downstream call.
- **File-payload refinement of `PhysicalSourceType::File`** — whether `File` carries `glob_root` payload; deferred to C14/C15.
- **Request-shaped pruning consideration** — C7 `hop_coverage` is currently request-independent (computed at compile from each hop's `to`-side member's full coverage); recorded for downstream review if request-shaped pruning enters the manifest layer.

---

## 4) Questions state snapshot

Question sidecars are stateful by directory:

- Active v1 backlog: `[questions/open/](questions/open/)`
- Ratified history: `[questions/closed/](questions/closed/)`
- Parked/post-v1: `[questions/deferred/](questions/deferred/)`

Footprint after balanced pruning:

| Directory | Files | Lines |
| --- | --- | --- |
| `open/` | 23 | ~2140 |
| `closed/` | 21 | ~1740 |
| `deferred/` | 21 | ~1287 |

Recent moves:

- Registry sidecars (`functions`, `join-types`, `temporal-shape`) → deferred.
- Facade ergonomics → deferred.
- Adapter/catalog operational-depth → split open + deferred.
- Stale numeric-code-era entries (`17/20/23/30/31/35`) → closed.
- `21` Q-DS-001 + `23` Q-UNI-003/-005/-007/-008/-010/-011 → closed (variant rebases, 2026-05-03).
- `22` Q-GRN-001/-002/-005 → closed (Grainset rebase); `Q-COMP-006` opened for `16` cleanup.
- `Q-COMP-007` (directionality) and `Q-COMP-017` (`join_type` default) → closed under item K. `Q-COMP-016` (m:m policy) updated.
- `Q-IR-IMPL-01..06` + `Q-IR-SPEC-01..04` → opened in `deferred/35_questions.md` 2026-05-26 covering W1-N-2/W2-N-1/W2-N-2/W3-N-1/W3-N-2/XC-1/XC-2 surface and the post-impl spec drift. Code-side already reconciled into `35` and `14a`; deferred questions track open shape choices (e.g. `IntoLiteral for f32`, `RegistryExtension` re-export hiding, `widen_for_join` allocation).
- Manifest ratification (item T, 2026-05-28): 3 questions ratified-and-closed citing clause IDs in new `closed/33_questions.md`, `closed/15_questions.md`, `closed/16_questions.md`. Deferred threads + Phase 2 research candidates landed in new `deferred/33_questions.md`, new `deferred/16_questions.md`, appended to `deferred/15_questions.md`, appended to `deferred/32_questions.md`. INDEX.md dashboard counts refreshed (open 23, closed 21, deferred 21).
- Semstrait-model round follow-up (2026-05-28): added deferred `Q-MODEL-D04` in `deferred/32_questions.md` to park core-contract OpenAPI/JSON Pointer/decorator adoption pending a dedicated interoperability ratification pass.

Focused v1 questions in `open/` should remain: architecture-impacting (`30`/`36`/`37`); compile/plan correctness-critical (`15`, `32`, `33`, parts of `22`/`23`); cross-stage primitive decisions (`38` Q-API-012). Non-blocking ergonomics and adapter empirics → deferred.

---

## 5) Last checkpoint (concise)

**Type:** Manifest contract ratification — Phase 1 close-all + Phase 3 cascades from Phase 1 (item T closure) on `feautre/semstrait-manifest-graph` (2026-05-28). Source of truth: `[_research/manifest/RATIFICATION_LOG.md](_research/manifest/RATIFICATION_LOG.md)`.

| Phase | Landed |
| --- | --- |
| Phase 1 (Ratification) | All 20 clauses closed: C1 (PhysicalSource roster), C2 (DataKind ↔ source linkage), C3 (PhysicalSource fields), C4 (global SemanticBitmap registry), C5 (Per-Dataset coverage), C6 (Per-Unionset coverage), C7 (Per-Joinset hop coverage), C8 (Per-Grainset level coverage), C9 (implicit composition), C10 (composition cap), C11 (both Sem+Phys persisted), C12 (split typed pools), C13 (compile-time gates), C14 (catalog fetch), C15 (schema fingerprint), C16 (Expr × bitmap conjunction), C17 (drop SemanticNode/Edge from disk), C18 (JSON serde), CCK (coverage-kernel meta-shape), CX1 (load-time integrity). Major direction: identity primitives on disk; SemanticGraph builds runtime structure (nodes/edges, JOIN-trees, ComposedSemanticInterface, implicit-composition enumeration) in memory. |
| Phase 3 Wave 1A | `apis/33_semstrait_manifest.md` — full rewrite per closed clauses. |
| Phase 3 Wave 1B | `apis/35_semstrait_ir.md` — additive C12.3 (newtype `SemanticExprId` / `PhysicalExprId`) + C18 (serde derive contract on `Expr<L>` + leaves) + CX1 (load-time integrity hooks). |
| Phase 3 Wave 1C / Thread A | `foundations/15_mapping_and_binding.md §2.5` — model-as-truth fields (locator, source_type, projected schema, optional version_ref) when catalog absent. |
| Phase 3 Wave 2D | `data-kinds/20_taxonomy.md` CCK skeleton cross-references; `data-kinds/22_grainset.md §1.3` I8 amendment (C8.2 — JOIN-tree deferred to graph build); `data-kinds/23_unionset.md §2.1` row A confirmation of implicit-Unionset top-level (C6.5/C9.4); `data-kinds/24_joinset.md §1.4 / §2.4` I8 + C7.4 amendment (ComposedSemanticInterface deferred). |
| Phase 3 Wave 2E | `foundations/16_composition.md §9.1 / §10.4 / §11.4` — C9.2 + C10.2 amendments (implicit-composition enumeration moves to graph build). |
| Question hygiene | 3 questions ratified-and-closed citing clause IDs (`closed/15`, `closed/16`, new `closed/33`). Deferred threads + Phase 2 research candidates: new `deferred/33`, new `deferred/16`, appended `deferred/15`, appended `deferred/32`. |
| Dashboard | `INDEX.md` snapshot table refreshed (open 23 / closed 21 / deferred 21). |

**Open follow-up.** Phase 3.5b spec-16 structural drift sweep — Wave 2 Agent E flagged broader prose drift outside the targeted-amendment scope (§10 preamble L1417-1423, §10.1 L1430, §10.2 L1446-1460, §11 preamble L1626-1630, §11.1 inputs, §15.5 L2250-2253, frontmatter `refined-by:` L27 still says 33 "persists Relationship, Joinset, ComposedSemanticInterface"). Targeted amendment notes (C9.2 / C10.2) landed at §9.1, §10.4, §11.4 only; full sweep needed. Phase 2 research (Targets A–E) queued in RATIFICATION_LOG.md, not yet launched.

For pass-by-pass chronology, use `[_archive/STATUS_HISTORY.md](_archive/STATUS_HISTORY.md)`.

---

## 6) Next-session starting point

1. Read `[00_overview.md](00_overview.md)`, `[STATUS.md](STATUS.md)`, `[INDEX.md](INDEX.md)`. For manifest-touching work, read `[_research/manifest/RATIFICATION_LOG.md](_research/manifest/RATIFICATION_LOG.md)` (clause-level source of truth) and `[apis/33_semstrait_manifest.md](apis/33_semstrait_manifest.md)` (post-rewrite contract). For expression-flow / IR-touching work, `[foundations/19_expression_flow.md](foundations/19_expression_flow.md)` and `[apis/35_semstrait_ir.md](apis/35_semstrait_ir.md)` remain the entry points; `[apis/31_semstrait_common.md](apis/31_semstrait_common.md)` holds non-expression shared vocabulary.
2. **Phase 3.5b — spec 16 structural drift sweep.** Item T open follow-up. Reconcile broader composition prose to lightweight posture (graph-build-time, not manifest-persisted): §10 preamble (L1417-1423), §10.1 (L1430), §10.2 (L1446-1460), §11 preamble (L1626-1630), §11.1 inputs, §15.5 (L2250-2253), frontmatter `refined-by:` line 27.
3. **Phase 2 research — Target C (Iceberg / Delta-Lake schema-evolution model).** Highest-leverage queued target; informs C3 (PhysicalSource fields), C14 (catalog fetch), C15 (schema fingerprint). Then Targets A (small-cardinality bitmap registries; informs C4–C8), B (dual expression-form persistence; informs C11/C12), D (hop-depth caps; informs C10), E (composition-graph cycle detection; informs C13 G1).
4. **`30 §6` typed-diagnostics codification.** Numeric codes as adjacent comments on enum variants, not runtime fields. Inventory: `30`, `34`, `36`, `37`, advisory-emitting Strategy chapters.
5. **Joinset (`24`) variant rebase.** Algorithm body extracts to `_drafts/34_joinset_strategy.md`.
6. **Model-level `AdditivityType` → `Additivity` rename** (`18 §5.2` cascade per `41_deprecations.md`). Variants align 1:1 with `14a §3.6`; the model-level enum extends `SemiAdditive { axes: Vec<SemanticsName>, strategy }`.
7. Parallel: item C (adapter/catalog framing) + `[TD-30-ADAPTER-CAPABILITY]`; stale `CompositionKind` cleanup in `33` (largely subsumed by item T's full rewrite — verify); `Q-COMP-006`; `[TD-19-ADDITIVITY-COMPOSITION]`; `[TD-19-ADVISORY-FIELDS]`; `[TD-REQUEST-DIM-VARIATION]`.

---

## 7) Session update rule

At end of each spec session:

1. Update this file with state changes only (phase, active items, deferred, snapshot, checkpoint).
2. Keep checkpoint concise; long narratives go to archive or commit history.
3. Propose updates for human approval before committing.
