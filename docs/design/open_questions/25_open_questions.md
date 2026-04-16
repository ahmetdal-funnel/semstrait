---
doc: design/open_questions/25_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `data-kinds/25_applicability_matrix.md`
depends-on:
  - data-kinds/25_applicability_matrix.md
  - data-kinds/20_taxonomy.md
  - data-kinds/21_dataset.md
  - data-kinds/22_grainset.md
  - data-kinds/23_unionset.md
  - data-kinds/24_joinset.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/14a_function_catalog.md
  - foundations/14b_expression_resolution.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
  - apis/30_api_contracts.md
  - apis/34_semstrait_planner.md
---

# 25 — Open Questions

> Items surfaced during Round-1 drafting of the cross-cutting applicability matrix. Each entry restates the question, lists its ratified references, and records the Round-1 default `25` currently uses. Entries migrate out of this file as `17`, `30`, `33`, `34`, or a revision of the per-variant docs (`21`–`24`) make decisions that either confirm or amend `25`'s indexing. None of these open items block `25`'s Round-1 matrix (`§§1`–`§7`).

---

## Q1. Matrix as snapshot vs living reference — maintenance discipline

**Section anchor:** `25 §2`

**Context.** `25`'s matrix indexes `10`–`17` and `20`–`24`. Every future MINOR to those docs (a new Semantics element in `11`, a new `CompositionKind` variant in `16`, a new `TemporalShape` subtype in `17`, or a new `DataKind` variant under `20`) adds at least one row to `25 §2` and possibly new cells across `§§3`–`§5`.

The question is how `25` stays consistent with its indexed docs. Two doctrines are viable:

- **Snapshot.** `25`'s matrix is pinned at ratification time. Every MINOR to an indexed doc requires a paired doc-edit to `25`. Readers can trust `25` to be exhaustive at any given version, but the maintenance burden falls on every indexed doc's drafter.
- **Living reference.** `25`'s matrix is acknowledged as incomplete between paired edits; readers fall back to the indexed docs for authoritative cells. `25`'s cells carry a "last synced at: <hash>" stamp; consumers MUST NOT treat `25` as exhaustive.

The tension is the same one that motivates `20 §3`'s at-a-glance matrix vs the exhaustive per-variant docs, but amplified because `25` crosses eight source docs instead of four.

**Options.**

- **A — Snapshot. Paired-edit discipline.** Every MINOR to `10`–`17` or `21`–`24` adds a line item to the MINOR's changelog: "update `25 §2` cells for X". The release-gate script rejects a MINOR whose changelog lacks the paired edit. `25` stays exhaustive.

  **Pros.** Readers trust `25`. The matrix is a first-class artifact for downstream consumers (tooling / docs-site / consumer-code that walks applicability programmatically).

  **Cons.** Drafters touching `10`–`17` must know to update `25`. Misses happen; the release-gate adds friction.

- **B — Living reference. `25` is lossy.** `25`'s cells carry a git-hash stamp per row indicating "this cell reflects the state of doc N at commit <hash>". A reader who finds the stamp older than the indexed doc's latest commit MUST defer to the indexed doc. `25` auto-regenerates from `authoritative-for:` front-matter via tooling when someone gets around to it (see Q4).

  **Pros.** Zero maintenance burden on indexed docs' drafters. `25` can grow stale without blocking a MINOR.

  **Cons.** Readers can't trust `25` without checking stamps; the stamp discipline itself is maintenance. Stale `25` defeats the doc's purpose ("ONE place to see applicability").

- **C — Hybrid. Snapshot at MAJOR, lossy at MINOR.** `25` is exhaustively re-synced at every MAJOR release. Between MAJORs, `25` may lag behind MINORs — consumers who need sub-MAJOR accuracy fall back to the indexed doc.

  **Pros.** Balances drafter ergonomics and reader trust. MAJOR release discipline already includes doc-set refresh; `25` sync joins that refresh.

  **Cons.** During sub-MAJOR windows, `25` is lossy; most consumers never hit a MINOR window, but when they do, the lag is confusing.

**Drafter recommendation.** **Option C (hybrid).** Rationale:

- `25` is a convenience index; treating it as a MAJOR-synced snapshot aligns with the doc's purpose (a reader's reference, not a machine-consumed artifact).
- MINOR changes to `11` / `13` / `17` that ripple into `25` tend to come in waves (a round of `TemporalShape` subtype additions, then a round of new `DimensionType` variants, etc.); batching the `25` sync into the MAJOR cadence means one re-sync covers a wave, not per-MINOR churn.
- Per-row git-hash stamps (option B) are infrastructure-heavy for modest benefit — the consumer population that needs sub-MAJOR `25` accuracy is, in practice, the drafters of subsequent docs, who already read indexed docs directly.
- Preserves option-A's release-gate discipline at MAJOR milestones (where it matters most).

**Blocking?** No. Any of A / B / C produces a correct `25 §2` in Round 1; the choice only affects maintenance cadence.

**Expected resolution point.** `00 §6` — the doc-set governance section, when it grows a "`25` sync discipline" paragraph. Alternatively resolved at the first MAJOR release that triggers a `25` re-sync (empirical discipline wins).

---

## Q2. `PLAN_W_25xx` unified cross-variant advisory band vs per-variant emission

**Section anchor:** `25 §6.2`

**Context.** `17 §7.3`'s `Additivity × TemporalShape` advisory roster applies across every variant that carries Measures. Round 1 emits variant-specific advisory codes at each emission site:

- `21 §9 PLAN_W_2102 ShapeAdditivityMismatch` — on `Simple`.
- `22 §9.2 PLAN_W_2202 MixedShapeAdvisoryChildren` — on `Grainset`.
- `23 §10.2 COMP_W_2302`–`W_2306` — on `Unionset` branches.
- `24 §11.2 PLAN_W_2404 AsOfActivation` — on `Joinset` hops (`AsOf`-specific but advisory in spirit).

The question is whether the cross-variant `Additivity × TemporalShape` advisory is best served by four distinct codes (one per variant) or by a single unified code in `25`'s `PLAN_W_25xx` band.

**Options.**

- **A — Keep per-variant emission.** The current roster stands. Four codes, four emission sites; no unified cross-variant advisory in `25 §6.2`'s `PLAN_W_2560`–`2589` band.

  **Pros.** Matches `30 §6`'s subsystem-prefix-by-stage-and-doc convention. A reader filtering for `21`-level advisories gets the Simple-level shape × additivity advisory without having to synthesize from a cross-doc code. Emission-site labels are stable — refactoring an advisory's emission does not renumber it.

  **Cons.** Four codes for one advisory class. Consumer code that wants "any shape × additivity advisory, regardless of variant" must pattern-match on four codes rather than a single cross-variant one.

- **B — Unify into a single `PLAN_W_25xx` code.** Allocate `PLAN_W_2560 ShapeAdditivityMismatch` in `25 §6.2` and retire `PLAN_W_2102`, `PLAN_W_2202`, the `COMP_W_2302`–`W_2306` cluster, and `PLAN_W_2404` in favor of it.

  **Pros.** One code, one rule. Cross-variant advisory filters become trivial.

  **Cons.** Breaks `30 §6`'s "subsystem prefix == stage, doc-range == doc" convention. Retirement of existing per-variant codes is a MAJOR (error-code numbers are in external downstream tooling). Synthesizes cross-doc information that the per-variant docs are currently well-served by keeping local.

- **C — Hybrid. Keep per-variant codes AND add a `PLAN_W_25xx` aggregate code.** `PLAN_W_2560 ShapeAdditivityMismatchAggregate` fires whenever at least one per-variant advisory in the `PLAN_W_2102` / `2202` / `COMP_W_23xx` / `PLAN_W_2404` cluster fires; the aggregate's `diagnostic.related` field carries the per-variant codes.

  **Pros.** Cross-variant filter works via the aggregate; per-variant filter works via the per-variant codes. No retirement; no MAJOR.

  **Cons.** Two advisory emissions per event (the per-variant + the aggregate); consumers have to de-dup. Aggregate-code-driven tooling must co-exist with per-variant-code-driven tooling.

**Drafter recommendation.** **Option A (keep per-variant).** Rationale:

- Option B's retirement cost is not justified by the cross-variant filtering convenience. The population of consumers that needs "every shape × additivity advisory, irrespective of variant" is small; per-variant docs' readers typically want per-variant emission.
- Option C's aggregate-emission pattern is a well-known anti-pattern: double-emission is confusing and fragile.
- Per-variant codes preserve `30 §6`'s prefix convention with no lost information.

**Blocking?** No. Round-1 emits per-variant codes and leaves `PLAN_W_2560`–`2589` reserved. The choice can be revisited at `17`'s Round-2 if the empirical multi-variant-advisory population turns out larger than expected.

**Expected resolution point.** `17 §7.3` Round-2, or whichever revision of `30 §6.2` first touches the data-kinds-block error-code allocations.

---

## Q3. `13 §7` cast-matrix reference chase — retarget vs grow a `13 §7` subsection

**Section anchor:** `25 §1.3 CDF-23-01`; `25 §2.5`

**Context.** `23 §1.1` and `23 §4.4` cite `13 §7`'s "cast matrix" / "widening rules" as the authoritative source for cross-child type reconciliation in a `Unionset`. `13 §7`, per its current outline, is "Interaction with Other Docs" — not a cast matrix. The widening rules `23` consumes are in fact distributed across `13 §2.4` (shape unification for DataType) and `14a` (function catalog's promotion lattice / cast policy).

This leaves `23`'s references dangling. Either `23` should be retargeted, or `13` should grow a `§7 Cast Matrix` subsection that consolidates the rules.

**Options.**

- **A — Retarget `23`'s refs.** Update `23 §1.1` and `23 §4.4` to cite `13 §2.4` + `14a §*` instead of `13 §7`. No structural change to `13`. `23` emits an editorial MINOR.

  **Pros.** Minimal change. Acknowledges that cast rules are inherently distributed (type-shape unification lives with `DataType` because that's where unification happens; cast policy lives with `14a` because that's where functions consume it).

  **Cons.** Consumers of `23` have to chase two citations instead of one. `13 §2.4` and `14a` never explicitly label themselves as "the cast matrix", so a reader looking for "the cast matrix" still has to reason about what they want.

- **B — Grow a `13 §7` cast matrix subsection.** Reorganize `13` so `§7` becomes "Cast Matrix" (promotion lattice, widening rules, cast-legality check) with content pulled from `13 §2.4` and `14a`'s cast-policy discussion. The existing `13 §7` "Interaction with Other Docs" moves to `§8`.

  **Pros.** One citation, one authoritative section. `13 §7` becomes the canonical anchor a future `24` / `25` can link to.

  **Cons.** Structural change to `13` — section-renumbering is a MAJOR because downstream docs' citations all shift. `14a` loses the cast-policy content (or duplicates it).

- **C — Add a `13 §7` Cast Matrix **cross-reference** subsection, without pulling content.** `13 §7` becomes a short pointer-only subsection that enumerates the cast-rule homes (`13 §2.4`, `14a`) and provides a stable anchor for consumers. The existing `13 §7` moves to `§8`.

  **Pros.** Stable anchor for "the cast matrix" without content duplication or content relocation. MINOR-compatible if the old `§7` content is preserved under `§8`.

  **Cons.** Another indirection (`13 §7` → `13 §2.4` + `14a`). Still two chases for a reader, just with one anchor at the start.

**Drafter recommendation.** **Option A (retarget).** Rationale:

- Cast rules ARE inherently distributed: DataType-shape unification is about identifying when two types are unifiable; function-catalog cast policy is about how the resulting cast is emitted. Consolidating them into `13 §7` risks pretending the rules are simpler than they are.
- The change to `23` is editorial — per `00 §6.3`, `23` already cites forward to later sections whose numbering is in flux. Retargeting is standard MINOR discipline.
- Option B's section-renumbering cost is real; option C's pointer-only subsection is a compromise that trades a little clarity for a little stability but does not resolve the underlying distribution of rules.

**Blocking?** No. Round-1 `25 §2.5`'s row records both citations and flags the inconsistency; a reader who lands on `23 §4.4`'s dangling ref and asks `25` gets the right answer (`13 §2.4` + `14a`).

**Expected resolution point.** `23`'s Round-2 (retarget) or `13`'s Round-2 (grow `§7`).

---

## Q4. Auto-generation of `§2`'s matrix from per-doc `authoritative-for:` front-matter

**Section anchor:** `25 §2`

**Context.** Every ratified doc in `10`–`17` and `20`–`24` carries a YAML front-matter block with an `authoritative-for:` list. In principle, `25 §2`'s matrix could be auto-generated from those lists: every cell in the matrix corresponds to some row in some doc's `authoritative-for:`.

But `authoritative-for:` entries are written in prose; converting them mechanically to matrix cells requires structure. The question is whether to:

1. Impose a machine-readable schema on `authoritative-for:` across `10`–`24` so `25 §2` auto-generates.
2. Keep `25 §2` hand-written but authored alongside each doc's ratification.
3. Hand-write `25 §2` once at Round 1 and re-sync per the `§2` maintenance discipline chosen in Q1.

**Options.**

- **A — Machine-readable `authoritative-for:`. Auto-generate `25 §2`.** Define a schema: each `authoritative-for:` entry is a structured record `{ clause: "N §M.K", variants: [...], disposition: "always|conditional|via-simple-children|n/a", qualifier: "…" }`. `25 §2` is a generated artifact. `25`'s body owns only `§§1`, `§3`–`§8` + the front-matter; `§2` is tooling output.

  **Pros.** `25 §2` cannot drift from the per-doc truth; drift becomes a compile-time error in the doc-build pipeline.

  **Cons.** Requires a schema rev to every ratified doc (MAJOR-level editorial change). Tooling investment. Loss of editorial flexibility (a cell that needs a prose qualifier becomes a schema extension).

- **B — Hand-written with per-doc ownership.** Every doc in `10`–`24` ratifies its own per-variant applicability cells inline (one subsection per doc, e.g. `17 §9 Applicability cells for the 17-owned clauses`). `25 §2` aggregates those subsections verbatim with no paraphrase.

  **Pros.** Keeps authority in the source doc. `25 §2` becomes a mechanical copy; maintenance burden is on the per-doc subsection.

  **Cons.** Every doc grows an `Applicability` subsection. `25`'s cross-cut organization (rows-as-foundation-clauses, columns-as-variants) may not fit every source doc's prose flow.

- **C — Hand-written centrally, maintained per Q1.** The current Round-1 approach. `25 §2` is hand-authored in `25`; maintenance cadence is whatever Q1 resolves to.

  **Pros.** Lowest friction for Round-1 ratification. No schema or per-doc subsection rework.

  **Cons.** `25 §2` is the single point of drift (mitigated only by Q1's cadence choice).

**Drafter recommendation.** **Option C (hand-written, Q1 cadence).** Rationale:

- Option A's tooling cost is substantial; the consumer population of `25 §2` as a machine-readable artifact is not yet established (no downstream tooling consumes it; `25` is primarily a human reference).
- Option B distributes authoring correctly but fights the `25` doc's organizing principle — rows are foundation clauses, columns are variants. A foundation doc (`10`–`17`) knows its row but not its column semantics well; a data-kind doc (`20`–`24`) knows its column but not its row semantics well. Central aggregation is more natural.
- Option C with Q1's hybrid cadence is low-cost; if `25 §2` drifts materially between MAJORs, tooling investment per option A becomes justifiable and can land as a future MINOR.

**Blocking?** No. Round-1 `25 §2` is hand-written; the choice only affects long-run maintenance strategy.

**Expected resolution point.** `00 §6` doc-governance section, or the first MAJOR where `25 §2` drift forces the question.

---

## Summary

| ID | Title | Round-1 default | Tracking marker |
|---|---|---|---|
| Q1 | Matrix as snapshot vs living reference | Hybrid (C) — MAJOR-synced snapshot, lossy at MINOR | — |
| Q2 | `PLAN_W_25xx` unified cross-variant advisory band | Per-variant emission (A); `PLAN_W_2560`–`2589` reserved | — |
| Q3 | `13 §7` cast-matrix reference chase | Retarget `23` refs to `13 §2.4` + `14a` (A) | `CDF-23-01` |
| Q4 | Auto-generation of `§2` from `authoritative-for:` | Hand-written centrally (C) | — |

---

**End of `25_open_questions.md`.**
