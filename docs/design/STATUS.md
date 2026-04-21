# Spec-Driven Development — Status

Living session-handoff file. Updated at the end of each working session (agent proposes; human approves).

**Authoritative spec root**: [`00_overview.md`](00_overview.md). Read `00_overview.md` first, then this file.

---

## 1. Current phase

**Phase**: Reconciliation (Phase-1 audit pending, Phase-2 Q&A partially deferred, Phase-3 writes blocked on Phase-2)

The first-pass drafting of all numbered documents under `docs/design/` is complete. Reconciliation against ground-truth code is in progress after several rounds identified that the initial drafts introduced abstractions that did not exist in the codebase. The current working principle is: **describe, clean, and name what the code actually implements; do not fabricate parallel models**.

**Phase map**:

| Phase | Scope | State |
|---|---|---|
| Phase 0 | First-pass drafting of all numbered spec docs | Complete |
| Phase 1 | Ground-truth audit: per-doc delta list vs. current code | Pending |
| Phase 2 | Scoped Q&A rounds on genuinely-open design items | Partial — constraints deferred (see §3) |
| Phase 3 | Apply Phase-1 deltas + Phase-2 decisions across the doc set | Blocked on Phase 1 + Phase 2 |
| Finalization | Cross-doc consistency pass, ratified spec freeze | Not started |

---

## 2. Active reconciliation items

Five themes surfaced by the ground-truth audit. None are "rewrite everything"; each has a bounded scope of docs to touch.

| Item | Summary | Primary docs affected |
|---|---|---|
| **A. YAML surface** | Top-level uses per-variant plural tags: `datasets:`, `grainsets:`, `unionsets:`, `joinsets:` (no unified `data_kinds:` parent, no `kind:` discriminator). Internally merged into `SemanticModel.entities: BTreeMap<String, DataKind>`. | `32_semstrait_model.md`, `20_taxonomy.md`–`25_applicability_matrix.md`, `33_semstrait_manifest.md` |
| **B. `Binding` → `SemanticMapping`** | The `Binding` type I introduced in the spec does not exist in code. The actual type is `ColumnMapping { Auto \| Inherited \| Explicit(...) }`, to be renamed to `SemanticMapping` and extended to carry pre-transformations on a `PhysicalSource` before a `SemanticExpr` resolves. | `15_mapping_and_binding.md`, `14b_expression_resolution.md`, `32`, `33`, `21_dataset.md` |
| **C. Adapter / Catalog framing** | `semstrait-adapter` and `semstrait-catalog` are single crates with **feature-gated modules** (engines: `datafusion` / `duckdb` / `spark`; providers: `local` / `iceberg` / `unity` / `aws`). Not per-engine/per-provider crates. Crate-level split is a post-v1 TD. | `36_semstrait_adapter.md`, `37_semstrait_catalog.md`, `39_semstrait_facade.md`, `42_migration_notes.md` |
| **D. "Dataset" vs "Simple" spelling** | `DataKindVariant::Simple.as_str() == "dataset"`. Internal Rust variant is `Simple`; YAML/prose spelling is `dataset`. Both must coexist: use `Dataset` when talking about the YAML-facing concept, `SimpleDataKind` / `DataKind::Simple` when talking about code. Prose drafts conflated the two. | All data-kinds docs (`20`–`25`), `32`, `33` |
| **E. Constraints model** | **Deferred — see §3**. The `constraints:` shape has concrete v1 realizers (Measure, Metric) and an open ratification item (Filter). Implicit (role-derived) vs explicit (authored) axis confirmed. Full Q&A to resume in a dedicated session. | `11_names_and_scopes.md §8`, `10_resolution_pipeline.md §3.4`, `13_types_and_grain.md §5.3`, `32` |

---

## 3. Deferred topics

### 3.1 Constraints design

**Status**: Deferred to a dedicated session.

**Frozen context**: [`open_questions/11_constraints_deferred.md`](open_questions/11_constraints_deferred.md) — contains the full in-flight Q-R4 thread (ratified axis, open shape questions, last concrete example, and three specific ratification items).

**Resume from**: the three open ratification items captured in `11_constraints_deferred.md` — `aggregation:` sub-block semantics, key-naming (`aggregation` vs `aggregations`, `all` vs `all_of`), and whether `constraints.filter:` sub-blocks or `Filter.constraints` entity-level fields (or both) are in scope for v1.

**Do not** proceed with a fourth rewrite of `11 §8` until this session lands.

---

## 4. Open Q&A rounds per document

Per-document open-question files live under [`open_questions/`](open_questions/). Index:

| Doc | Open-questions file | Notable themes |
|---|---|---|
| 11 (names & scopes) | `open_questions/11_constraints_deferred.md` (snapshot) | Constraints — deferred |
| 14b (expression resolution) | `open_questions/14b_open_questions.md` | Cross-DataKind path pre-resolution |
| 15 (mapping & binding) | `open_questions/15_open_questions.md` | Blocked on item B (SemanticMapping) |
| 16 (composition) | `open_questions/16_open_questions.md` | Relationship-driven composition edges |
| 17 (temporal shape) | `open_questions/17_open_questions.md` | 14 deferred items, incl. AsOf planner impl |
| 20–25 (data-kinds) | `open_questions/{20..25}_open_questions.md` | 37 questions total, all blocked on item A + D |
| 30 (API contracts) | `open_questions/30_open_questions.md` | Error-code range finalization |
| 31 (core) | `open_questions/31_open_questions.md` | Public-surface scope |
| 32 (model) | `open_questions/32_open_questions.md` | YAML surface — blocked on item A |
| 33 (manifest) | `open_questions/33_open_questions.md` | Storage, repository, integrity |
| 34 (planner) | `open_questions/34_open_questions.md` | Strategy dispatch, optimizer API |
| 35 (IR) | `open_questions/35_open_questions.md` | Substrait round-trip, visitor API |
| 36 (adapter) | `open_questions/36_open_questions.md` | Blocked on item C (module framing) |
| 37 (catalog) | `open_questions/37_open_questions.md` | Blocked on item C (module framing) |
| 38 (api) | `open_questions/38_open_questions.md` | Unified builder, warning propagation |
| 39 (facade) | `open_questions/39_open_questions.md` | Feature-flag table — blocked on item C |
| 40 (refactor plan) | `open_questions/40_open_questions.md` | Phased rollout decisions |
| 41 (deprecations) | `open_questions/41_open_questions.md` | Tombstone horizons, alias mechanism |
| 42 (migration notes) | `open_questions/42_open_questions.md` | Recipe rendering, per-MAJOR ordering |
| registry/functions | `open_questions/functions_mapping_open_questions.md` | Per-engine function parity |
| registry/temporal | `open_questions/temporal_shape_mapping_open_questions.md` | AsOf rewrite-tier matrix |
| registry/joins | `open_questions/join_types_mapping_open_questions.md` | Cardinality-informed emission |

---

## 5. Last checkpoint

**Session**: 2026-04-17 (approximate)

**Accomplished**:

- Flagged five reconciliation items (A–E) after ground-truth code audit (`crates/semstrait-model/src/types/`, `crates/semstrait-adapter/Cargo.toml`, `crates/semstrait-catalog/Cargo.toml`).
- Confirmed implicit/explicit axis for constraints (`11 §8` fourth rewrite plan ratified structurally but not written).
- Deferred full constraints design to a dedicated session; frozen snapshot captured under `open_questions/11_constraints_deferred.md`.
- Established durable-context plumbing: `AGENTS.md` (project-root entry), `STATUS.md` (this file), `CLAUDE.md` routing rewrite, banner notes on `README.md` and `DECISION_LOG.md`.

**Next-session starting point**:

1. Read `00_overview.md` + `STATUS.md` (mandatory).
2. Start **Phase 1 ground-truth audit** — scan each drafted doc under `docs/design/` for occurrences of issues A-D. Produce a per-doc delta checklist under `open_questions/reconciliation_ground_truth.md` (single master checklist). Do not rewrite doc content at this stage.
3. After the checklist is reviewed and approved, resume Phase-2 Q&A on any items that have residual design content (items A, B, C, D). Constraints (item E) waits for its own session.

---

## 6. Session update rule

STATUS.md is updated at the end of each working session. Discipline:

1. Agent proposes the diff to the human at session close.
2. Human approves or edits inline.
3. Approved STATUS.md change is committed alongside the spec edits that caused it.

Changes to §5 (last checkpoint) are mandatory on every spec session. Changes to §§1–4 are only when the underlying state actually changed.

---

## 7. Lessons learned (append-only)

Short notes on recurring failure modes observed during this exercise. New entries append; existing entries are not edited.

- **L1 (2026-04-17)** — The spec tree must ground every abstraction in current code. Introducing parallel models (e.g. `Binding` where only `ColumnMapping` exists; a unified `data_kinds:` YAML tag where only per-variant plural tags exist; a generic constraint framework with reserved future carriers where only Measure/Metric carry them) is the recurring failure mode. The job is *describe, clean, and name what exists* — not propose alternate abstractions to sugar over existing shapes. Any abstraction that is not in code must be flagged as a proposed extension with explicit justification, not smuggled in as a "natural" generalization.
- **L2 (2026-04-17)** — Prose spelling discipline: YAML-facing noun ≠ internal Rust variant. Conflate them and cross-doc references drift. Maintain both spellings explicitly.
- **L3 (2026-04-17)** — When drafting against a feature-gated crate, distinguish module-level feature flags from crate-level separation. The former is a v1 concern; the latter is a migration-plan concern.
