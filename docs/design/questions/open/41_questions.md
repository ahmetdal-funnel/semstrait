---
doc: design/questions/open/41_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `implementation/41_deprecations.md`
depends-on:
  - implementation/41_deprecations.md
  - implementation/40_refactor_plan.md
  - apis/30_api_contracts.md
  - 00_overview.md
---

# Open Questions — `implementation/41_deprecations.md`

> Items surfaced during Round-1 drafting of the deprecation register. Each entry restates the question, lists its ratified references, and records the Round-1 default currently used. Entries migrate out of this file as they resolve through phase work, release management, or subsequent amendments to `40` / `41` / `42`. None of these items block the register's authoritative rows in `41 §3`.
>
> Policy note: every item below is a **policy / process** question. None of them re-open a ratified deprecation in `00 §4.3`, `40 §9`, or the scanned `[TD-*]` tags; if one does, it has been miscategorized and belongs in the originating doc's open-questions file.

---

## Q-41-001 — Tombstone retention horizon

**Question.** `41 §2.6` ratifies a minimum retention horizon of one MAJOR cycle past removal. Should the horizon be extended (two MAJOR cycles? indefinite?) to support long-lived external audits that reconstruct the migration story from git alone?

**Refs.**

- `41 §2.6` — tombstone policy.
- `30 §12.2` — `41` tombstone retention at least one MAJOR past removal.
- `40 §3` — `[TD-*]` inventory (parallel tombstone discipline).

**Proposed (Round 1):** One MAJOR past removal as the binding minimum; longer is permissible at the author's discretion, and the default is "leave the row in place unless a reviewer asks for it to be pruned."

**Arguments for a longer (two MAJOR+) horizon.**

- Forensic value: auditing what a legacy symbol once meant remains possible without cross-referencing a git log that may have rewritten history.
- Low cost: tombstone rows are markdown, not code; storage impact is negligible.

**Arguments against extending.**

- Register readability degrades as the tombstone count grows.
- The `42` migration-note entries already carry the full caller-facing story; `41` tombstones duplicate that content for the symbol roster.

**Current position in `41`.** One MAJOR minimum, with a soft bias toward keeping tombstones indefinitely.

**Next step.** Revisit post-v2.0. If `41` has grown past a readability threshold (reviewer-set), introduce a retention sweep.

---

## Q-41-002 — Alias mechanism preference: `pub type` vs `pub use`

**Question.** `41 §6.1` lists several name-renames that ship with a one-MINOR-cycle alias. The alias mechanism can be either `pub type LegacyName = NewName;` (type alias) or `pub use new_module::NewName as LegacyName;` (re-export). Should `41` fix a default, or leave it to per-rename authors?

**Refs.**

- `41 §6.1` — codemod-compatible renames.
- `30 §12.1` — `#[deprecated]` lifecycle.
- `30 §11.4` — behavior-preserving refactors (PATCH).

**Proposed (Round 1):** Per-rename discretion. `pub type` is preferred when the symbol is a struct or enum and the alias appears in the same module; `pub use` is preferred when the symbol moves across modules (the re-export documents the move explicitly).

**Arguments for fixing a single mechanism.**

- Register uniformity: every row cites the same alias pattern.
- CI tooling (`cargo-semver-checks`, clippy lints) can be tuned against a single pattern.

**Arguments for per-rename discretion.**

- `pub type` fails for items that are not types (e.g. `const` items, modules).
- `pub use` with `as` clauses interacts with rustdoc re-export discipline differently from `pub type` aliases.
- Forcing one pattern introduces noise at the few sites where the other is more natural.

**Current position in `41`.** Per-rename discretion; `41 §6.1` enumerates the actually-used pattern per row.

**Next step.** If a CI-tooling concern surfaces during Phase 2 (the heavy `Compiled*` → `Resolved*` renaming cycle), revisit and fix a default.

---

## Q-41-003 — YAML legacy-grammar cycle length

**Question.** `40 §5.2` and `41 §3.3` ratify a one-MINOR-cycle parser dual-accept window for the YAML grammar retirements (`datasets:` → `data_kinds:`, `Relationship.source_set:` → `from:`, etc.). Is one cycle long enough given the externally-authored Model corpus at risk?

**Refs.**

- `40 §5.2` — Phase 1 exit criteria.
- `41 §3.3` — YAML retirement table.
- `32 §14.4` — `[CODE-DIVERGES-FROM-SPEC]` row.
- `30 §12.4` — minimum-window policy.

**Proposed (Round 1):** One MINOR cycle with `PARSE_W_*` on every legacy form, rejection at the next MAJOR. If an author-facing audit surfaces pre-v1 that the externally-authored Model corpus is large and slow-moving, the cycle extends to two MINOR cycles as a documented exception per `30 §12.4`.

**Arguments for extending to two cycles.**

- Model YAML is edited by data-modeling authors, not Rust engineers; the audience has a different upgrade cadence than the workspace crate consumers.
- `serde_yaml` migration (`[TD-MODEL-YAML-CRATE]`) may surface parser-quality concerns mid-Phase-1 that argue for keeping a known-good legacy path longer.

**Arguments against extending.**

- Dual-accept inflates parser-state complexity; every extra cycle extends the surface area an author could hit a legacy-only edge case on.
- `42`-side migration recipes are already required regardless of window length.

**Current position in `41`.** One MINOR cycle. Extension is a phase-boundary decision at Phase 1 exit.

**Next step.** Evaluate the externally-authored-Model corpus at Phase 1 midpoint; if the upgrade cadence argues for extension, land the exception note in `41 §3.3`.

---

## Q-41-004 — `MeasureConstraints` grandfathering vs a scheduled v2 rename

**Question.** `41 §5` grandfathers `MeasureConstraints` per `[TD-CONSTRAINT-RENAME]`, citing "deferred to the broader SemanticManifest-schema revision pass." Should the deferral carry a concrete v2 target, or remain open-ended?

**Refs.**

- `41 §5` — exceptions roster.
- `[TD-CONSTRAINT-RENAME]` — `11 §8.4.3` / `31 §6.1`.
- `11 §8.4.3` — shared carrier surface across Measure + Metric.

**Proposed (Round 1):** Open-ended. The name persists for v1; any v2 renaming lands when the SemanticManifest-schema revision pass is scheduled, not before.

**Arguments for a concrete v2 target (e.g. "retired in v2.0").**

- Downstream consumers get a removal date to plan against.
- The grandfathered roster stays finite.

**Arguments against a fixed target.**

- The "SemanticManifest-schema revision pass" is not yet on the phased roadmap; scheduling its retirement now would couple two unrelated decisions.
- `11 §8.4.3` phrases the rename as carrier-neutralization, not as a one-line name swap — implementation detail matters.

**Current position in `41`.** Open-ended; `41 §5` marks the row "NEVER retired in v1."

**Next step.** Revisit when the SemanticManifest-schema revision pass enters the roadmap (post-v1). At that point, `41` amends the row with a concrete retirement release.

---

## Q-41-005 — Error-code retirement cadence

**Question.** `41 §3.4` records NO error-code retirements in v1 per `30 §6.7` / `40 §9.2`. When does the first retirement land — on an ad-hoc basis driven by a phase discovery, or coordinated with a workspace MAJOR cut?

**Refs.**

- `30 §6.3` — retirement is MAJOR.
- `30 §6.7` — retired-codes table reserved.
- `40 §9.2` — "every reserved code is forward-looking."

**Proposed (Round 1):** Ad-hoc — whenever a phase discovers the need to retire a code, the retirement lands at the next available MAJOR cut. No proactive retirement-sweep pass.

**Arguments for a coordinated retirement pass.**

- Batches retirements to reduce `42` churn per release.
- Gives downstream log-scraping tools a single checkpoint to adapt against.

**Arguments for ad-hoc.**

- Retirements are expected to be rare (`30 §6.3`'s stability promise).
- Coordinating a sweep introduces process overhead without clear benefit.

**Current position in `41`.** Ad-hoc. `41 §3.4`'s table activates when the first retirement surfaces.

**Next step.** Revisit if Phase 5 / Phase 6 emission / catalog work surfaces more than one retirement candidate in a single phase.

---

## Q-41-006 — Rustfix / IDE quick-fix suggestion opt-in

**Question.** Rust 1.77+ supports machine-applicable suggestions on `#[deprecated]` attributes; rust-analyzer surfaces them as quick-fixes. Should every rename in `41 §6.1` opt into suggestion annotations, or only the renames that a caller is most likely to hit?

**Refs.**

- `41 §6.1` — codemod-compatible renames.
- `41 §6` — migration-aid tiers.
- `30 §12.1` — `#[deprecated]` lifecycle.

**Proposed (Round 1):** Opt-in per rename. The heavy-hitter renames (`TemporalHistorization` → `TemporalShape`; `Compiled*` → `Resolved*`; `LogicalPlan` → `SemanticPlan`; `StorageProvider` → `FileSystem`) ship with suggestion annotations. The remainder carry plain `#[deprecated]` notes only.

**Arguments for blanket opt-in.**

- Every caller benefits from quick-fixes; opt-out requires a reason.
- The maintenance cost is per-rename authoring, not per-invocation.

**Arguments for opt-in per rename.**

- Some renames pair with a subtle signature change (`41 §6.2`) where an automatic suggestion would be wrong.
- Rustfix annotations carry an MSRV constraint; opt-in lets `41 §6.1` evolve as the workspace MSRV advances.

**Current position in `41`.** Opt-in per rename. `41 §6.1` lists the actually-annotated rows; every row there ships with a suggestion.

**Next step.** Re-evaluate at v1.0 cut. If the heavy-hitter set covers >80% of caller-hit renames empirically, keep the opt-in; else expand.

---

## Q-41-007 — Retrospective `#[deprecated]` backfill on pre-Phase-0 renames

**Question.** Some renames in `41 §3` were performed in code before the design tree was ratified (the pre-exercise code already uses `SemanticGraph` in place of `RelationshipGraph` / `FieldIndex`; `SemanticGraph` itself is the replacement). Should `41` retroactively declare `#[deprecated]` on the legacy names even though they landed before the formal register existed?

**Refs.**

- `40 §3.9` — legacy `TD-0NN` entries.
- `41 §1.4` — "not a `[TD-*]` inventory."
- `Q-40-006` — forward-only `[CODE-DIVERGES-FROM-SPEC]` tagging discipline.

**Proposed (Round 1):** No retrospective backfill. Pre-design-exercise renames land through `40 §3.9`'s legacy-entry absorption (which is tracked, not re-deprecated), and `41` only registers renames that land `#[deprecated]` at Phase-0-or-later. The legacy `RelationshipGraph` / `FieldIndex` rows in `41 §3.2.3` ARE registered because their removal is scheduled for Phase-2 MAJOR — the register tracks the **forthcoming retirement**, not the past deprecation.

**Arguments for a backfill pass.**

- Completeness: every public rename across the entire migration is indexed in one place.
- Parity with `41 §7`'s tracking table.

**Arguments against a backfill pass.**

- Churns every design doc retroactively; mirrors the `Q-40-006` concern.
- The absorbed-legacy entries in `40 §3.9` already serve the completeness goal.

**Current position in `41`.** No backfill. Absorbed-legacy entries trace through `40 §3.9` to `41 §3.2.3` where applicable, no further.

**Next step.** Revisit only if a downstream consumer reports that a legacy rename's absence from `41` produced a traceability gap.

---

## Q-41-008 — Retirement register pruning policy

**Question.** `41 §2.6` says tombstones stay for at least one MAJOR past removal. Does `41` ever prune tombstones proactively (e.g. at a v2.0 cut), or do tombstones accumulate indefinitely?

**Refs.**

- `41 §2.6` — tombstone retention.
- `Q-41-001` — retention horizon (related but distinct).
- `40 §8.4` — post-MAJOR-cut irreversibility.

**Proposed (Round 1):** Never prune proactively. Tombstones are append-only past the one-MAJOR minimum; a pruning pass requires explicit reviewer sign-off at a workspace major release.

**Arguments for periodic pruning.**

- Register readability improves as the workspace ages.
- Very old tombstones (three+ MAJORs past removal) serve almost no caller.

**Arguments against pruning.**

- Pruning is itself a reviewable change; the review burden exceeds the marginal readability gain.
- Git history already prunes itself.

**Current position in `41`.** Append-only past the one-MAJOR minimum.

**Next step.** Revisit at v2.0 cut. If `41`'s tombstone section has grown past a reviewer-set readability threshold, approve a pruning pass.

---

*Cross-references in this document are by section (e.g. `00 §4.3`, `30 §12.1`, `40 §9.1`, `41 §3.3`). No code-path references are used, per `00 §8`.*
