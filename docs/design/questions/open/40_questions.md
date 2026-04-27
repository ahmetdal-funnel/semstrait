---
doc: design/questions/open/40_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `implementation/40_refactor_plan.md`
depends-on:
  - implementation/40_refactor_plan.md
  - apis/30_api_contracts.md
  - 00_overview.md
---

# Open Questions — `implementation/40_refactor_plan.md`

> Items surfaced during Round-1 drafting of the phased refactor plan. Each entry restates the question, lists its ratified references, and records the Round-1 default currently used. Entries migrate out of this file as they resolve through phase work, release management, or subsequent amendments to `40` / `41` / `42`. None of these items block the phase sequencing of `40 §5`.

Policy note: every item below is a **process or scheduling** question. None of them re-open a ratified design decision in `0x`–`3x`; if one does, it has been miscategorized and belongs in the originating doc's open-questions file.

---

## Q-40-001 — PR-boundary policy within a phase

**Question.** Does `40` prescribe a granularity for PR splits inside a phase (e.g. "one PR per `[TD-*]` tag") or leave it to the PR author subject only to phase-exit gates?

**Refs.**

- `40 §1.3` — "not per-PR scheduling."
- `40 §5` — per-phase exit criteria only; phase boundaries are hard, internal boundaries are soft.
- `30 §11.2` — every MAJOR requires a migration-note entry; independent of PR granularity.

**Proposed (Round 1):** PR granularity is author discretion, bounded by two rules: (a) no PR may straddle a phase boundary (crossing an exit gate in one atomic change); (b) no PR may introduce a public-API break without its `41` / `42` entries landing in the same PR.

**Arguments for tighter (e.g. "one PR per `[TD-*]` tag").**

- Traceability: a git-bisect against the `[TD-*]` inventory becomes mechanical.
- Review burden is bounded per PR.

**Arguments for looser (author discretion).**

- Some `[TD-*]` tags are atomic but touch many files (e.g. `TD-IR-NONEXHAUSTIVE`); forcing one PR per tag hides the real work unit.
- Some phases are fundamentally cross-cutting (Phase 2's `Compiled*` → `Resolved*` rename); breaking into per-tag PRs fragments a single conceptual refactor.

**Current position in `40`.** Author discretion with the two rules above.

**Next step.** Re-evaluate if Phase 2 / Phase 5 PR churn becomes unreviewable. If so, introduce a phase-specific max-files-per-PR threshold for the offending phase only.

---

## Q-40-002 — Per-phase staff allocation

**Question.** Should `40` declare staffing / owner assignment per phase?

**Refs.**

- `40 §5` — per-phase owning crates listed; no staff names / team assignment.

**Proposed (Round 1):** Out of scope for `40`. Owning crates are listed; staff assignment is a release-management concern.

**Current position in `40`.** Declared out of scope.

**Next step.** If Phase 3 / Phase 5 turns out to require specialist knowledge not shared across the implementer group, owning-crate-per-phase may be supplemented with an owner annotation in `41` at the symbol-roster level. Deferred.

---

## Q-40-003 — Exact CI pipeline-change schedule

**Question.** `40 §7.1` declares test additions per phase. What is the exact CI pipeline evolution (new job definitions, new runners, caching strategy changes) across the phases?

**Refs.**

- `40 §7.4` — tooling at a summary level.
- `40 §5.1` Phase 0 — test-harness scaffolding stubs.

**Proposed (Round 1):** Phase 0 lands the CI harness stubs (empty `tests/golden/`, `tests/snapshot/`). Each phase's exit criteria additionally describe the gate; the concrete CI YAML / runner changes are tooling decisions that belong with the release-management layer, not the design tree.

**Current position in `40`.** Phase-level testing gates declared; CI job-level details deferred.

**Next step.** Release-management creates a `RUNBOOK.md` (or equivalent) that tracks per-phase CI additions. `40` references it once it exists.

---

## Q-40-004 — `semstrait-io` extraction trigger

**Question.** Legacy `TD-008` proposes extracting `semstrait-manifest::io` into a new `semstrait-io` crate when three or more I/O utilities accumulate. Is that still the right trigger, and which phase owns it?

**Refs.**

- `docs/TECH_DEBT.md` `TD-008`.
- `40 §5.3` Phase 2 — mentions trigger-based extraction.
- `40 §2.3` — notes `io.rs` placement.

**Proposed (Round 1):** Keep the three-utilities trigger. Extraction lands ahead of Phase 3 if the trigger fires during Phase 2 (the likely moment, as Manifest rewrites touch I/O paths). If the trigger does not fire, carry `io.rs` into Phase 7 and re-evaluate against `38` / `39`.

**Current position in `40`.** Trigger-based. Not a phase-exit criterion.

**Next step.** Evaluate the utility count at Phase 2 exit. If ≥ 3, schedule the extraction as a Phase-3-preamble mini-phase.

---

## Q-40-005 — Per-engine adapter crate split timing

**Question.** `30 §13` targets per-engine adapter crates (`semstrait-adapter-datafusion`, `semstrait-adapter-duckdb`, `semstrait-adapter-spark`, `semstrait-adapter-substrait`) as separate crates. Phase 5 ratifies the in-crate split as Round-1 transitional. When does the physical split land?

**Refs.**

- `30 §13` — provisional-crate posture for per-engine adapters.
- `40 §5.6` Phase 5 — in-crate split as the Round-1 transitional form.

**Proposed (Round 1):** The physical crate split is NOT a Phase 5 exit criterion. If PR churn during Phase 5 exceeds a reviewer-set threshold, split mid-phase. Otherwise, schedule immediately post-v1 (pre-v1.1).

**Current position in `40`.** Deferred to release management with an explicit safety valve.

**Next step.** Re-evaluate at the v1 cut.

---

## Q-40-006 — `[TD-*]` tag discipline

**Question.** The Round-1 tree has exactly one `[CODE-DIVERGES-FROM-SPEC]` tag (in `32 §14.4`) but many narrated divergences. Should a retrospective tagging pass be scheduled, or should the tag be applied only going forward?

**Refs.**

- `40 §4.1` — explicitly tagged flags (one).
- `40 §4.2` — narrated divergences (many).

**Proposed (Round 1):** Apply the tag going forward only. The `40 §2` per-crate delta catalog plus `40 §4.2`'s narrated-divergence table are the mechanical equivalent; a retrospective tagging pass would churn every design doc without improving implementer traceability.

**Current position in `40`.** Forward-only discipline. Reviewers apply the tag when amending any `0x`–`3x` doc that surfaces a new code-vs-spec divergence.

**Next step.** Revisit once any phase-2-or-later amendment lands. If the tag count stays near 1, the narrated-divergence approach is adequate. If divergences surface faster than amendments catch them, a single bulk-tagging pass may be scheduled.

---

## Q-40-007 — Legacy `TD-0NN` migration into bracketed scheme

**Question.** Legacy `docs/TECH_DEBT.md` entries are numbered `TD-001` through `TD-009`. The design tree uses bracketed `[TD-NAME]` form. Should a blanket rename pass normalize the legacy entries into bracketed form, or should each legacy item be absorbed / closed on its own schedule?

**Refs.**

- `40 §3.9` — legacy entries cataloged with per-item absorbing phase.

**Proposed (Round 1):** Absorb / close on a per-item schedule. When a legacy `TD-0NN` is closed by phase work, its `41` tombstone uses the bracketed form `[TD-LEGACY-KINDREF-SERDE]` (or similar); when it is absorbed into a design-tracked item, it is listed as an alias.

**Current position in `40`.** Opportunistic per-phase absorption. No blanket rename pass.

**Next step.** After Phase 2 (which closes the bulk of the legacy entries), re-evaluate whether a final rename pass is worth the churn.

---

## Q-40-008 — Banned-term audit tooling

**Question.** `00 §4.3` lists banned terms. Phase 8 exits with a banned-term-audit green gate. What is the tool?

**Refs.**

- `00 §4.3` — banned-terms master list.
- `40 §7.1` Phase 8 — documentation-linter requirement.
- `40 §9.1` — per-term phase-owner mapping.

**Proposed (Round 1):** A rustdoc / markdown linter, scaffolded in Phase 0, grows its rule set as each ban's phase-owning work lands. Implementation is a tooling concern (bash + ripgrep + a per-term allowlist for quoted-narrative mentions is sufficient Round 1).

**Current position in `40`.** Scaffolded in Phase 0; rules accumulate phase by phase.

**Next step.** Implement a minimum-viable linter in Phase 0. Extend in each phase that lands a banned-term cleanup.

---

*Cross-references in this document are by section (e.g. `00 §4.3`, `30 §6.2`, `40 §5.6`). No code-path references are used, per `00 §8`.*
