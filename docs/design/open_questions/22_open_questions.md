---
doc: design/open_questions/22_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `data-kinds/22_grainset.md`
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

> Items surfaced during Round-1 drafting of the Grainset specification. Each entry restates the question, lists its ratified references, and records the Round-1 default `22` currently uses. Entries migrate out of this file as later docs (`17`, `20`, `23`–`25`, `33`, `34`) make decisions that confirm or amend `22`'s defaults.

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

## Q-GRN-003 — Cost function pluggability hook site: planner trait or adapter hook?

**Question.** `22 §4.4` ratifies the v1 cost function as source-count. A future stats-backed cost function is `[TD-GRAINSET-COST-STATS]`. When it lands, should the hook site be:
- **A** — a method on `34`'s `Planner` trait (`fn grainset_cost(&self, child: &ResolvedGrainsetChild, request: &Request) -> Cost`);
- **B** — a method on the `37` catalog-adapter trait (each adapter reports stats; the planner consumes them uniformly);
- **C** — a separate `CostEstimator` trait injected into the `plan` call site (third-axis of extensibility)?

**Refs.**
- `22 §4.4` — Round-1: source-count proxy.
- `34` (pending) — planner entry-point; owns the Manifest-to-Plan strategy dispatch.
- `37` — catalog adapter; owns source metadata (file sizes, partition counts, row-count estimates).
- `30 §6.2` — the `22xx` code range is fixed regardless of cost-function placement.

**Arguments for A (on `Planner`).**
- Cost is a planner concern — it composes with the rest of the plan strategy (join-ordering, push-down) and is not uniquely Grainset's.
- Keeps adapter surface narrow; adapters supply raw stats, planner derives cost.

**Arguments for B (on adapter).**
- The numbers live in the catalog; asking each adapter to report cost directly avoids a round-trip through stat-fetching + planner-internal computation.
- Simpler for adapter authors who already know their own numeric characteristics.

**Arguments for C (separate `CostEstimator`).**
- Decouples cost from both planner and adapter; lets users inject a custom estimator without reimplementing either.
- Matches Calcite / DataFusion patterns.

**Current position in `22`.** Deferred. The hook site is a `34`-drafting decision; `22` does not commit.

**Next step.** Confirm at `34` drafting time; `22 §4.4` will cross-reference whichever trait lands.

---

## Q-GRN-004 — Grainset-of-Grainset nesting

**Question.** `22 §3.4` / `COMP_E_2207` currently forbids a `Grainset` as a child of another `Grainset` (`[TD-GRAINSET-NESTED]`). Should Round 2 admit nested Grainsets? What is the semantic?

**Refs.**
- `22 §3.4` — deferred; `COMP_E_2207` fires at compile.
- `12 §2` — nesting matrix; the current cell is "Grainset ⇨ Grainset: forbidden."
- `25` — applicability matrix; the canonical location for ratifying the cell.
- `16 §5` — `ComposedSemanticInterface`; nesting works structurally, but the semantics need to be defined.

**Arguments for forbidding (Round-1 default).**
- The author's use case is unclear: a nested Grainset is semantically equivalent to a flat Grainset with the union of children. If the inner Grainset has children {A, B, C} and the outer Grainset has children {X, inner, Z}, why not just declare {A, B, C, X, Z} flat?
- Nesting compounds the cost-rank and tie-break axes; the inner selection happens per-invocation, and the outer selection chooses between "scan X," "select one of {A, B, C}," "scan Z." The author can reason about three children more easily than a nested tree.
- Avoid premature abstraction; ratify when a concrete use case appears.

**Arguments for admitting.**
- Use case: an author might want to group "daily sources" in one inner Grainset (to share a rollup policy across them) and "monthly sources" in another, composed under an outer Grainset. Flattening loses the per-group rollup policy.
- Matches the open-extension philosophy: a `ComplexDataKind` is a tree, and Grainset is one of the nodes; disallowing it as a child of itself is an artificial hole in the matrix.

**Current position in `22`.** Forbidden via `COMP_E_2207`. The error message should point the author at `25` / the flattening alternative.

**Next step.** Revisit at `25` drafting if a concrete use case arises. The typed-variant surface is already `#[non_exhaustive]`; admitting nesting in Round 2 is backward-compatible.

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

---

## Q-GRN-006 — Single-child Grainset degeneracy: lint or accept?

**Question.** A Grainset with exactly one child is structurally valid per `22 §2` but semantically degenerate — it is a one-child wrapper that adds nothing over the underlying DataKind. Should Round 1 accept silently, emit a lint, or reject?

**Refs.**
- `22 §2.1` — Round-1: `children: Vec<GrainsetChild>` with `VALID_E_2201` firing only on empty.
- `22 §9.2` — no current advisory for single-child degeneracy.
- Similar pattern: a Unionset with one branch, a Joinset with one member — same shape of question for `23` / `24`.

**Arguments for silent accept (Round-1 default).**
- Useful during Model evolution: an author may start with one child and plan to add more. Rejecting single-child forces a temporary scaffold.
- Symmetric with a Grainset that *loses* children via refactoring down to one: no accidental breakage.

**Arguments for lint (`PLAN_W_22xx`).**
- Signals "you probably meant to either add more children or replace with the underlying DataKind." Low-cost nudge.

**Arguments for reject (`VALID_E_22xx`).**
- Keeps the Model sharp; single-child Grainsets have no planner-visible effect.

**Current position in `22`.** Silent accept in v1 (matches `VALID_E_2201`'s "empty only" check).

**Next step.** Gather feedback; if authoring-facing feedback suggests confusion, promote to a lint. Advisory-only; do not elevate to error without a stronger signal.

---
