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

**Active:**

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

---

## 3) Deferred topics

### 3.1 Constraints design

Status: deferred to dedicated session. Working context: `[questions/deferred/11_questions.md](questions/deferred/11_questions.md)`. Resume points: `aggregation` sub-block semantics; `aggregation` vs `aggregations` / `all` vs `all_of`; `constraints.filter` scope vs entity-level fields.

---

## 4) Questions state snapshot

Question sidecars are stateful by directory:

- Active v1 backlog: `[questions/open/](questions/open/)`
- Ratified history: `[questions/closed/](questions/closed/)`
- Parked/post-v1: `[questions/deferred/](questions/deferred/)`

Footprint after balanced pruning:

| Directory | Files | Lines |
| --- | --- | --- |
| `open/` | 23 | ~2580 |
| `closed/` | 19 | ~1430 |
| `deferred/` | 18 | 797 |

Recent moves:

- Registry sidecars (`functions`, `join-types`, `temporal-shape`) → deferred.
- Facade ergonomics → deferred.
- Adapter/catalog operational-depth → split open + deferred.
- Stale numeric-code-era entries (`17/20/23/30/31/35`) → closed.
- `21` Q-DS-001 + `23` Q-UNI-003/-005/-007/-008/-010/-011 → closed (variant rebases, 2026-05-03).
- `22` Q-GRN-001/-002/-005 → closed (Grainset rebase); `Q-COMP-006` opened for `16` cleanup.
- `Q-COMP-007` (directionality) and `Q-COMP-017` (`join_type` default) → closed under item K. `Q-COMP-016` (m:m policy) updated.

Focused v1 questions in `open/` should remain: architecture-impacting (`30`/`36`/`37`); compile/plan correctness-critical (`15`, `32`, `33`, parts of `22`/`23`); cross-stage primitive decisions (`38` Q-API-012). Non-blocking ergonomics and adapter empirics → deferred.

---

## 5) Last checkpoint (concise)

**Type:** `semstrait-model` spec implementation — consolidation pass (item L closure) on `feature/spec-driven-dev` (2026-05-12). Builds on the W1–W5 baseline (archived).

| Phase | Landed |
| --- | --- |
| P1a | `ExprBlock` archived to `expr_ast.rs` `#[doc(hidden)]`; `ExprSource::Declarative` carries `serde_yaml::Value`. `RelationshipId`/`JoinType::from_optional`/`PhysicalExpr` deleted as unused. |
| P1b | 12 stub variants pruned across `ParseErrorKind`/`ValidateErrorKind`/`CatalogsParseErrorKind`; 5 rules implemented (SR-8, SR-E-4/-9/-10, `SemanticsShadowRootPool`). SR-6 retired. SR-E-11 renamed `FilterWrongKind` → `WrongFilterError`. |
| P2 | `column` → `field` rename (semantic-side); `SemanticMappingBuilder::with_semantic`; `LiteralValue::Deserialize` widened. |
| P3 | `Diagnostic::map_kind` lifted to `semstrait-common`; `validate.rs` and `data_kind/mod.rs` split into folder modules; `walk_complex` extracted. |
| P4ab + P4c | All builders migrated to `bon`-derived typestate; root storage `Vec<(Location, T)>`; SR-3/SR-E-3 dedup at `.build()`. `parse(yaml)` returns `SemanticModelBuilder`; `loader::merge_models` deleted. `Duplicate*` moved parse → validate. |
| P5 | `LiteralValue::Serialize` hand-rolled (parity with `Deserialize`); loader's `catalogs_loaded` placeholder dropped; `Diagnose::cause` delegation on `ModelBuildErrorKind`. |

Final gates: `cargo clippy -p semstrait-common -p semstrait-model --all-targets -- -D warnings` clean; `cargo test -p semstrait-model` 146 pass; `semstrait-manifest` 220 baseline (no regression).

For pass-by-pass chronology, use `[_archive/STATUS_HISTORY.md](_archive/STATUS_HISTORY.md)`.

---

## 6) Next-session starting point

1. Read `[00_overview.md](00_overview.md)`, `[STATUS.md](STATUS.md)`, `[INDEX.md](INDEX.md)`, then `[foundations/19_expression_flow.md](foundations/19_expression_flow.md)`. Post-Q (item Q): `[apis/35_semstrait_ir.md](apis/35_semstrait_ir.md)` is the crate-of-record for the trait family, support enums, identifier carriers, leaves, `Expr<L>`, accessors, `Parameter`, `FunctionRegistry`, `ValidateError`/`CompileError`. `[apis/31_semstrait_common.md](apis/31_semstrait_common.md)` holds non-expression shared vocabulary (logical types, diagnostics, constraints, `io`).
2. **`30 §6` typed-diagnostics codification.** Numeric codes as adjacent comments on enum variants, not runtime fields. Inventory: `30`, `34`, `36`, `37`, advisory-emitting Strategy chapters.
3. **Joinset (`24`) variant rebase.** Algorithm body extracts to `_drafts/34_joinset_strategy.md`.
4. **Model-level `AdditivityType` → `Additivity` rename** (`18 §5.2` cascade per `41_deprecations.md`). Variants align 1:1 with `14a §3.6`; the model-level enum extends `SemiAdditive { axes: Vec<SemanticsName>, strategy }`.
5. Parallel: item C (adapter/catalog framing) + `[TD-30-ADAPTER-CAPABILITY]`; stale `CompositionKind` cleanup in `33`; `Q-COMP-006`; `[TD-19-ADDITIVITY-COMPOSITION]`; `[TD-19-ADVISORY-FIELDS]`; `[TD-REQUEST-DIM-VARIATION]`.

---

## 7) Session update rule

At end of each spec session:

1. Update this file with state changes only (phase, active items, deferred, snapshot, checkpoint).
2. Keep checkpoint concise; long narratives go to archive or commit history.
3. Propose updates for human approval before committing.
