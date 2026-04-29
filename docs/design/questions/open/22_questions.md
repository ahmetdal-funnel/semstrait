---
doc: design/questions/open/22_questions
status: Living
purpose: Open questions surfaced while drafting `data-kinds/22_grainset.md`
depends-on:
  - data-kinds/22_grainset.md
  - data-kinds/20_complex_datakinds.md
  - foundations/13_types_and_grain.md
  - foundations/15_mapping_and_binding.md
  - foundations/16_composition.md
  - foundations/17_temporal_shape.md
  - data-kinds/23_unionset.md
  - data-kinds/24_joinset.md
  - data-kinds/25_applicability_matrix.md
  - apis/33_semstrait_manifest.md
  - apis/34_semstrait_planner.md
---

# Open Questions — `data-kinds/22_grainset.md`

> Three questions remain open: Q-GRN-001 (inheritance default for child grain), Q-GRN-002 (cross-child partial coverage), Q-GRN-005 (mixed-shape warnings). Closed items moved to [`../closed/22_questions.md`](../closed/22_questions.md); deferred items in [`../deferred/22_questions.md`](../deferred/22_questions.md).

---

## Q-GRN-001 — Inheritance default for child `grain`: finest vs declared

**Question.** When a `GrainsetChild` omits `grain:` in YAML, `22 §3.2` inherits from the child's own temporal-Dimension `grains:` list and picks **the finest** grain. Is "finest" the right default, or should inheritance require an explicit pick (e.g. "the first entry in the list" / "error if more than one grain declared")?

**Refs.**
- `22 §3.2` — Round-1 default: finest.
- `13 §4.2` — `TemporalDimension { grains: Vec<Grain> }`; the `grains:` list is "which coarser grains this dimension can be rolled to," ordered conventionally finest-first.
- `11 §6` — Semantics-level declarations; the child's own Dimension is fully declared at compile.
- `32` (pending) — Model-parse semantics; if `grains:` declares an ordered list, "first" vs "finest" may diverge.

**Arguments for "finest" (Round-1 default).**
- Matches the common case: the author wrote the child's Binding against a fact-level source (minute-level events, day-level snapshots). Finest is what the physical source actually carries; coarser entries in `grains:` are "this dimension can be aggregated to these grains," not "the source is at this grain."
- Conservative for the planner: a finer-grain child CAN be rolled up to any coarser grain (shape permitting); a coarser-grain child CANNOT be dis-aggregated. Wrong-direction errors surface clearly via the §4.2 eligibility predicate.
- Keeps the YAML minimal for the common case.

**Arguments for requiring explicit.**
- "Finest" is a hidden policy — the author may have written a `grains: [day, week, month]` declaration where `day` is the query-time default, not the source grain (e.g. a daily snapshot that the author wants to expose only at day-or-coarser). Implicit "finest" then picks `day` correctly, but if they later add `minute` to support finer-grained exposure, the grain silently shifts without an intended declaration change.
- Explicit `grain:` on every child is one more line of YAML but eliminates ambiguity.

**Current position in `22`.** Finest-grain inheritance, with a crisp failure (`VALID_E_2204 GrainsetChildGrainUnresolvable`) when inheritance is unresolvable (no `grains:` list on the child's temporal Dimension).

**Next step.** Revisit after early-usage feedback on Round-2 example Models. If authors hit silent grain shifts or surprise at the inherited value, tighten the default to "require explicit" and emit a lint (`PLAN_W_22xx` or a new `VALID_W_2200`) when omitted.

---

## Q-GRN-002 — Cross-child partial coverage: error in v1, or split-and-delegate?

**Question.** When a Request names Semantics that no **single** child of a Grainset covers Natively/Derivedly — but the **union** of children's Coverage does — should the planner split the Request into per-child sub-Requests and combine the results, or report `PLAN_E_2201 NoEligibleChild` (the Round-1 default)?

**Refs.**
- `22 §4.2` — Round-1 default: empty candidate set → `PLAN_E_2201`.
- `22 §6.3` — fallback rule; explicit non-silence.
- `22 §9.1` — `PLAN_E_2208 GrainsetPartialCoverageNotSupported` (reserved for when the split-and-delegate feature lands).
- `23` — Unionset; the "multiple branches contributing rows" variant already handles one form of cross-source assembly.
- `16 §11` — implicit composition via `Relationship`; partially-overlapping Semantics across Grainsets may already be answerable via this path.
- `[TD-GRAINSET-PARTIAL-COVERAGE]` — deferred.

**Arguments for error-in-v1 (Round-1 default).**
- Keeps Grainset's planner strategy crisp: one child, one plan. Split-and-delegate blurs the boundary with Unionset.
- The author's alternatives are clear: (a) add a child whose Coverage is a superset; (b) express the query as a Unionset, which is the variant designed for cross-source composition; (c) express via `Relationship` traversal so `16 §11` handles the composition.
- Avoids a hidden join/re-aggregation that Grainset is not designed to surface.

**Arguments for split-and-delegate.**
- Ergonomic: if the author's Model happens to have a Grainset whose children cover complementary Semantics, it is surprising that `paid_media_rollups.{cost from monthly, clicks from events}` is a planner failure when both values are present in the Grainset's declared surface.
- Could be implemented as a post-eligibility combinator: "if no single child covers, form N sub-plans, one per Semantics-partition, and UNION ALL with NULL-fill." This is exactly what `23` does, but within a Grainset's composed surface.

**Current position in `22`.** Error in v1 via `PLAN_E_2201`; `PLAN_E_2208` is reserved to make the intent explicit once the partial-coverage feature lands.

**Next step.** Park until `23` lands. If `23`'s cross-source assembly strategy generalizes cleanly to "within-Grainset" composition, promote to a Round-2 feature with a new `RollupPolicy::AllowPartialCoverage` variant. If the semantics are too different (e.g. Unionset is row-wise, partial coverage would be column-wise), keep Grainset as "one child, one plan."

---

## Q-GRN-005 — Mixed-shape Grainsets: warning vs error

**Question.** `22 §5` ratifies mixed `TemporalShape`s across children as a **warning** (`PLAN_W_2202 MixedShapeAdvisoryChildren`), not an error. Should Round 2 promote to error, or relax further (silent)?

**Refs.**
- `22 §5` — Round-1: warning.
- `22 §9.2` — `PLAN_W_2202`.
- `17` (parallel) — shape-rollup matrix; may ratify a "compatible subset" for mixed-shape Grainsets (e.g. `{Timeseries, Events}` is safe; `{Snapshot, SCD}` is not).

**Arguments for warning (Round-1 default).**
- The author often has legitimate reasons to mix shapes: an Events child at fine grain + Snapshot children at coarser grains is the canonical Grainset use case. Erroring would break the primary example.
- The per-child rollup-legality check (§4.3) already prevents unsafe rollups; the warning is surface-level feedback, not a correctness gate.

**Arguments for error.**
- Some shape combinations are always incorrect (e.g. a Grainset combining SCD without as-of anchors and Events without — the planner can never coherently pick between them). Auto-detecting and erroring avoids confused runtime behavior.

**Arguments for silent.**
- If §4.3's rollup legality is sufficient, the warning is noise.

**Current position in `22`.** Warning. Promote-to-error or demote-to-silent is a `17`-ratification decision.

**Next step.** Resolve at `17` ratification. `17` may ratify a per-combination compatibility table; `22`'s warning/error policy follows from whatever `17` declares.
