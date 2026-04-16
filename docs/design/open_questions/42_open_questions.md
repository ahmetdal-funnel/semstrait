---
doc: design/open_questions/42_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `implementation/42_migration_notes.md`
depends-on:
  - implementation/42_migration_notes.md
  - implementation/41_deprecations.md
  - implementation/40_refactor_plan.md
  - apis/30_api_contracts.md
  - 00_overview.md
---

# Open Questions — `implementation/42_migration_notes.md`

> Items surfaced during Round-1 drafting of the per-version migration-notes template. Each entry restates the question, lists its ratified references, and records the Round-1 default currently used. Entries migrate out of this file as they resolve through amendments to `42`, through release-management tooling decisions, or through caller feedback once v1.0 actually ships.

Policy note: every item below is a **documentation-format or communication-format** question. None re-open a ratified design decision in `0x`–`3x`; if one does, it is miscategorized and belongs in the originating doc's open-questions file.

---

## Q-42-001 — Per-MAJOR entry ordering: newest-first vs oldest-first

**Question.** `42` will accumulate one section per MAJOR. Should sections appear newest-first (`v2.0` above `v1.0`) or oldest-first (`v1.0` above `v2.0`)?

**Refs.**

- `42 §1.3` — "Sections are ordered newest-to-oldest at publish time."
- `40 §6.2` — phased MAJOR cadence; new MAJORs become the most interesting entry for fresh upgraders.
- `30 §11.2` — every MAJOR gets an entry; no guidance on ordering.

**Arguments for newest-first.**

- Callers upgrading from the previous MAJOR land on the most relevant entry at the top.
- Matches typical `CHANGELOG.md` convention.

**Arguments for oldest-first.**

- The v1.0 entry is the richest by far (§3 is ~60% of the v1.0 file); putting it first matches its reference-like weight.
- A reader exploring the design tree without a specific upgrade in mind reads top-down; oldest-first matches narrative time.

**Proposed (Round 1).** Present v1.0 **first** as a reference section (because it is the green-field re-baseline), then subsequent MAJORs newest-first below. The v1.0 carveout is deliberate: v1.0 has special status as the "first ratified release" and serves as a reference surface for all subsequent entries.

**Current position in `42`.** Hybrid: §3 (v1.0) is first by privilege; §4+ are inserted newest-first.

**Next step.** Revisit at the first post-v1.0 MAJOR cut. If the hybrid rule produces confusing ToC ordering, flip to pure newest-first and relegate v1.0 to an appendix.

---

## Q-42-002 — Coverage for provisional-crate-only MINORs

**Question.** `30 §13` marks `semstrait-adapter` and `semstrait-catalog` (and their per-engine / per-provider subcrates) as `Provisional`. Per `30 §13`, these crates may carry non-additive changes in MINOR cycles. Does each such provisional-crate MINOR warrant a **full** `§2`-format entry in `42`, or does a single-bullet "provisional-crate churn" callout suffice?

**Refs.**

- `30 §13` — provisional-crate posture; MINORs may break.
- `42 §1.3` — lists "provisional-crate MINOR that carries a non-additive change" among the triggers for a `42` entry.
- `42 §2.3` — versioning granularity paragraph.

**Arguments for full entry.**

- Consistency: the table-of-deltas format makes a break easy to scan.
- Auditability: every break in provisional crates receives the same migration discipline as a workspace MAJOR.

**Arguments for bullet-only.**

- Volume: per-engine adapters may cycle faster than the workspace itself; a full section per cycle inflates the doc.
- Risk of dilution: callers skim past frequent updates, defeating the format's purpose.

**Proposed (Round 1).** Use the **full** `§2` format for every provisional-crate non-additive MINOR. Acknowledge the doc may grow faster than expected; if it does, Q-42-002 is re-opened with a threshold-based condensation rule (e.g. "after 3 cycles of a given provisional crate, condense historical entries into a rolling appendix").

**Current position in `42`.** Full-entry discipline.

**Next step.** Observe adapter / catalog release cadence for 6 months post-v1.0. Re-evaluate.

---

## Q-42-003 — Scripted YAML migrator for the v1.0 legacy-grammar transformation

**Question.** `§3.7` declares "no scripted tooling shipped" for v1.0. Is that the right posture, or should a `semstrait-migrate` binary (or `cargo semstrait migrate`) land alongside v1.0 to mechanize the YAML transformation?

**Refs.**

- `42 §3.2.1` — YAML surface deltas.
- `42 §3.5.4` — manual recipe for the split-blocks → `data_kinds:` transformation.
- `40 §5.2` Phase 1 — parser accepts both legacy and new grammar simultaneously with `PARSE_W_*` on legacy.

**Arguments for shipping a migrator.**

- YAML corpora at consumer sites may be large; manual editing is error-prone.
- The transformation is mechanical for ~80% of cases (block rename, discriminator injection, field rename); a 5-minute `awk` pipeline would cover most corpora.

**Arguments against.**

- The other 20% (Joinset anchor / path construction, `Relationship` directionality) requires authorial judgment; a partial migrator could lead callers to trust its output and miss the judgment-required cases.
- One-time migration; tooling maintenance cost post-v1.0 is high relative to benefit.
- The one-MINOR-cycle shim window (per `40 §5.2` Phase 1) buys time for hand-migration.

**Proposed (Round 1).** No migrator from the core team. If a community contributor ships one, link it from `§3.7` and track support separately. Per `40 §5.2`, the shim window is the migration window.

**Current position in `42`.** `§3.7` declares "no scripted tooling shipped."

**Next step.** Revisit if post-v1.0 consumer feedback reports high-error-rate manual migrations.

---

## Q-42-004 — Rendering recipes: Rust snippets vs code-reference citations

**Question.** `42 §3.5` recipes show BEFORE / AFTER Rust snippets in free-form Rust code blocks. Should these snippets instead use CODE-REFERENCE citations into the actual workspace codebase (`line:line:path` format per the writing conventions)?

**Refs.**

- `00 §8` — design docs do not reference code paths.
- `42 §2.2` — "Code snippets in recipes are Rust-attributed (` ```rust`) or YAML-attributed (` ```yaml`) unless they reference existing code in the repository. The snippets are illustrative — they are not code-reference blocks into the actual codebase."
- CLAUDE.md tree convention on CODE-REFERENCES vs MARKDOWN CODE BLOCKS.

**Arguments for CODE-REFERENCE citations.**

- Ties the recipe to the actual v1.0 shipping code.
- Survives code evolution better (the reference resolves to the checked-in code at that commit).

**Arguments for illustrative snippets.**

- `00 §8` is explicit: design docs do not reference code paths. `42` is an implementation-note doc but lives under `docs/design/`; applying the `00 §8` rule is consistent.
- Recipes illustrate the shape of the migration, not the exact line in the codebase. Over-specification to exact lines defeats the purpose.
- Pre-1.0 "BEFORE" code is already removed from the workspace at the moment v1.0 lands; a CODE-REFERENCE to it would not resolve.

**Proposed (Round 1).** Illustrative snippets only. Per `00 §8`.

**Current position in `42`.** `§2.2` ratifies illustrative snippets.

**Next step.** Revisit if a post-v1.0 MAJOR introduces a structural break that is easier to describe via pinpoint file references; in that case, consider embedding a `git` tag / commit hash alongside the `42` entry so references are stable over time.

---

## Q-42-005 — Retired-error-code rendering format

**Question.** `30 §6.7` reserves a "Retired codes" sub-section in `30` for a lookup table. `30 §6.3` declares that the full retirement story lives in `42`. What is the rendering format for a retired code in `42`?

**Refs.**

- `30 §6.3` — retirement is MAJOR; narrative lives in `42`.
- `30 §6.7` — `30`'s lookup-table format: `Code | Retired in | Replacement | Rationale`.
- `42 §3.2.3` — no retirements in v1.0.

**Proposed (Round 1).** At first retirement, `42` introduces a `§N.8 Retired error codes` sub-section (slot reserved; absent at v1.0). The format is the same four-column table as `30 §6.7`, extended with a fifth column `Migration recipe` pointing to a `§N.5` recipe. Both tables remain in sync.

**Current position in `42`.** No retirements yet; no format ratified. §N.8 slot reserved implicitly.

**Next step.** Ratify the `§N.8` slot the moment the first code is retired.

---

## Q-42-006 — Cross-linking `42` entries from `CHANGELOG.md`

**Question.** Every MAJOR gets a `CHANGELOG.md` entry (per `30 §11.2`) and a `42` section. How are the two cross-linked?

**Refs.**

- `30 §11.2` — MAJOR changelog requirement.
- `42 §1.2` — `CHANGELOG.md` is commit-level; `42` is structured migration guidance.

**Proposed (Round 1).** Every MAJOR `CHANGELOG.md` entry carries a pointer in the form `See docs/design/implementation/42_migration_notes.md §N.` The anchor for `§N` is the MAJOR version number rendered as a stable slug (e.g. `§3` = v1.0, `§4.N` = v1.N additive, `§5.1` = v2.0). Slugs do not change once published.

**Current position in `42`.** Not yet ratified. `§1.2` mentions `CHANGELOG.md` but does not fix the anchor scheme.

**Next step.** Ratify the anchor scheme at the first post-v1.0 MAJOR cut. Amend `42` to carry a per-section anchor convention (e.g. `## §5.1 v2.0 {#v2-0}` with explicit slugs).

---

## Q-42-007 — Caller-action checklist: single consolidated list vs per-crate checklists

**Question.** `42 §3.6` renders the v1.0 required caller actions as a **single consolidated checklist**. Should it instead be split into per-crate checklists — one for `semstrait-core` consumers, one for `semstrait-adapter` consumers, etc.?

**Refs.**

- `42 §3.6` — 12-item consolidated checklist.
- `40 §2` — per-crate delta catalog (a per-crate checklist would mirror `40 §2` directly).

**Arguments for consolidated.**

- Most v1.0 upgraders touch multiple crates; the consolidated view mirrors their work unit.
- The checklist is short enough (12 items) to avoid skim-fatigue.

**Arguments for per-crate.**

- Some consumers depend on exactly one `semstrait-*` crate (e.g. a client that only calls `semstrait-facade`); a per-crate checklist lets them skip the rest.
- Mirrors `40 §2`'s structure.

**Proposed (Round 1).** Consolidated. The v1.0 delta is large enough that most consumers are affected across multiple crates, and the consolidated form is easier to verify as a single unit. If a future MAJOR has a narrower delta concentrated in a single crate, a per-crate checklist MAY be used for that specific entry.

**Current position in `42`.** Consolidated. Per-crate format is allowed but not required.

**Next step.** Revisit at the first post-v1.0 MAJOR; if its delta is crate-narrow, the per-crate format becomes the worked example for the pattern.

---

*Cross-references in this document are by section (e.g. `00 §8`, `30 §6.3`, `42 §3.2`). No code-path references are used, per `00 §8`.*
