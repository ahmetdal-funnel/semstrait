# Spec-Driven Development — Status

Living handoff file for active design work.

Read order for spec sessions:

1. `[00_overview.md](00_overview.md)`
2. `[STATUS.md](STATUS.md)`
3. `[INDEX.md](INDEX.md)` for task routing
4. `[DOCS_MAINTENANCE.md](DOCS_MAINTENANCE.md)` for editing discipline

Historical long-form narrative is archived in:

- `[_archive/STATUS_HISTORY.md](_archive/STATUS_HISTORY.md)`

---

## 1) Current phase

**Phase:** Reconciliation / consolidation (post-ratification cleanup)

**What is stable now**

- DataKind taxonomy and trait axes are ratified.
- Typed diagnostic-kind discipline and `tracing` observability are ratified across `30`-`39`.
- Per-Q-ID question-directory split (`open` / `closed` / `deferred`) is in place.
- Variant chapter rebases — `Dataset` (`21`), `Unionset` (`23`), and `Grainset` (`22`) — complete in slim form (algorithm bodies extracted to `_drafts/34_*_strategy.md` sidecars; cascade-aligned Coverage-inference, TemporalShape-equivalence, and plan-time observable behavior contracts).
- `UnionMode { All, Unique }` v1 roster re-confirmed (C6, 2026-05-03).
- `CompositionKind` shrunk to `{Joinset, Grainset}` V1 (Unionset variant retired post-thirteenth-pass cascade rebase 2026-05-03; Unionset uses bare `SemanticInterface` per `23 §3.2`); `ChildCoverageOverride { provides }` and YAML `coverage:` block retired; `ComposedSemanticInterface` broadened to cover both Joinset per-hop and **Grainset cross-grain LEFT OUTER JOIN composition** on shared `Key`s per `18 §2.5` (per G-2 ratification 2026-05-03).
- Grainset cross-grain JOIN composition mechanism ratified (G-2): driver = most-covering grain-eligible routing unit (declaration-order tie-break per G-2b); attached units in declaration order (G-2c); LEFT OUTER (G-2a); hard compile error `COMP_E_2204 GrainsetCrossGrainKeysAbsent` on missing shared Keys (G-2d). Internal `RollupPolicy { ShapeDefault, PinOnly, PreferFinest }` per G-4 (planner knob, NOT authored in V1 YAML).

**What remains active**

- Adapter/catalog framing reconciliation (item C).
- Residual cross-doc vocabulary cleanup where retired error-code language still appears.
- v1 backlog trimming in open question sidecars.
- Variant chapter rebases — `Joinset` (`24`) pending (the last remaining; `33`/`34` come after).
- Algorithm-body sidecars (`_drafts/34_simple_strategy.md`, `_drafts/34_unionset_strategy.md`, `_drafts/34_grainset_strategy.md`) pending lift into `34_semstrait_planner.md §<XStrategy>` when the planner doc opens its Strategy chapter.
- Deeper structural cleanup of `16 §9.3` / `§10.5` / `§13` (inert post-Unionset-retirement) parked behind new `Q-COMP-006` for a Round-4 framework cleanup pass.
- Stale `CompositionKind` / `ComposedSemanticInterface` references in `33` pending cleanup at that chapter's rebase.

---

## 2) Active reconciliation items


| Item | Summary                                                                                     | Status                                         | Primary docs                                           |
| ---- | ------------------------------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------ |
| A    | YAML surface and type hierarchy alignment                                                   | Ratified                                       | `32`, `32b`, `26`, pointers in `20`-`25`               |
| B    | `Binding` -> `SemanticMapping` framing and metadata synthesis                               | Ratified at authoring level                    | `15`, `18`, `32`, `33`                                 |
| C    | Adapter/catalog architecture framing (single crate + feature-gated modules vs alternatives) | **Open**                                       | `30`, `36`, `37`, `39`, `42`                           |
| D    | `Dataset` naming consistency                                                                | Ratified                                       | `20`-`25`, `32`, `33`                                  |
| E    | Constraints model depth                                                                     | Deferred                                       | `11`, `10`, `13`, `18`, `32`                           |
| F    | Nesting shape rules (`R1`/`R2`/`R3`)                                                        | Ratified                                       | `26`, `32`, `22`-`24`                                  |
| G    | I/O transport and `semstrait-core::io` posture                                              | Ratified                                       | `31b`, `31`, `32`, `33`                                |
| H    | Canonical entity type set                                                                   | Ratified                                       | `18` (+ cascades)                                      |
| I    | Typed diagnostics + `tracing` observability                                                 | Ratified; cleanup still pending in older prose | `30`-`39`, selected `10`/`13`/`14`*/`15`/registry docs |


---

## 3) Deferred topics

### 3.1 Constraints design

Status: deferred to dedicated session.

Working context:

- `[questions/deferred/11_questions.md](questions/deferred/11_questions.md)`

Resume points:

- `aggregation` sub-block semantics
- key naming choices (`aggregation` vs `aggregations`, `all` vs `all_of`)
- `constraints.filter` scope choice vs entity-level fields

---

## 4) Questions state snapshot

Question sidecars are stateful by directory:

- Active v1 backlog: `[questions/open/](questions/open/)`
- Ratified history: `[questions/closed/](questions/closed/)`
- Parked/post-v1: `[questions/deferred/](questions/deferred/)`

Current footprint after balanced pruning:


| Directory   | Files | Lines |
| ----------- | ----- | ----- |
| `open/`     | 23    | ~2580 |
| `closed/`   | 19    | ~1430 |
| `deferred/` | 18    | 797   |

(Approximate after this round; precise counts will refresh on the next sweep.)


Recent pruning moves:

- registry sidecars (`functions`, `join-types`, `temporal-shape`) moved to deferred;
- facade ergonomics sidecar moved to deferred;
- adapter/catalog operational-depth sidecars split into focused open + deferred remainder;
- stale numeric-code-era entries in `17/20/23/30/31/35` moved to closed;
- `21` (Q-DS-001) and `23` (Q-UNI-003 / -005 / -007 / -008 / -010 / -011) closed as part of variant-chapter rebases (2026-05-03).
- `22` (Q-GRN-001 / -002 / -005) closed as part of `Grainset` rebase (2026-05-03); `Q-COMP-006` opened to track post-rebase `16 §9.3` / `§10.5` cleanup.

Focused v1 questions in `open/` should remain:

- architecture-impacting (`30`/`36`/`37` framing, strategy openness coupling),
- compile/plan correctness-critical (`15`, `32`, `33`, selected `22`/`23`),
- explicitly queued cross-stage primitive decisions (`38` Q-API-012 class).

Non-blocking ergonomics and deep adapter empirics should be deferred unless they directly block v1 implementation planning.

---

## 5) Last checkpoint (concise)

**Checkpoint type:** Variant chapter rebase pass — `Dataset` (`21`), `Unionset` (`23`), and `Grainset` (`22`).

Delivered outcomes:

- `21_dataset.md` slim-rebase (~50% reduction; algorithm body in `_drafts/34_simple_strategy.md` sidecar).
- `23_unionset.md` slim-rebase (~53% reduction; algorithm body in `_drafts/34_unionset_strategy.md` sidecar).
- `22_grainset.md` slim-rebase (535 lines vs pre-rebase 942; algorithm body in `_drafts/34_grainset_strategy.md` sidecar; **new** cross-grain LEFT OUTER JOIN composition mechanism via `ComposedSemanticInterface` + shared `Key`s).
- `16 §5` ratification block + `CompositionKind { Joinset, Grainset }` shrink; `25 §3.2` GrainsetStrategy + Composition cells re-spec'd.
- 7 + 3 questions closed (Q-DS-001; Q-UNI-003 / -005 / -007 / -008 / -010 / -011; Q-GRN-001 / -002 / -005).
- `CompositionKind` re-scoped to `{Joinset, Grainset}` (Unionset retired); `ComposedSemanticInterface` broadened to cover Grainset cross-grain LEFT OUTER JOIN composition (in addition to Joinset per-hop). Pre-cascade `MixedShapeAdvisoryChildren` (`PLAN_W_2202`), `RollupPolicy::Auto/AsOfRequired` exposed-as-author-knob, and `grain_axis` field all retired.

For pass-by-pass chronology and prior long-form diffs, use:

- `[_archive/STATUS_HISTORY.md](_archive/STATUS_HISTORY.md)`

---

## 6) Next-session starting point

1. Read `[00_overview.md](00_overview.md)`, `[STATUS.md](STATUS.md)`, then `[INDEX.md](INDEX.md)`.
2. Continue variant chapter rebases: `Joinset` (`24`) is the last remaining.
3. Resolve item C (adapter/catalog framing) across `30`, `36`, `37`, `39`.
4. Continue tightening open sidecars to decision-oriented, implementation-impacting entries only.
5. Sweep stale `CompositionKind` / `ComposedSemanticInterface` references in `33` as part of the `Joinset` rebase / `33` rebase. Address `Q-COMP-006` (deeper `16 §9.3` / `§10.5` cleanup) in a Round-4 framework pass.

---

## 7) Session update rule

At end of each spec session:

1. Update this file with only state changes (phase, active items, deferred, snapshot, checkpoint).
2. Keep checkpoint concise; place long narratives in archive or commit history.
3. Propose updates for human approval before committing.