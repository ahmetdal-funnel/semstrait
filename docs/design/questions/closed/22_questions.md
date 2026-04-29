---
doc: design/questions/closed/22_questions
status: Closed
purpose: Resolved questions originally raised against `data-kinds/22_grainset.md`
---

# Closed Questions — `data-kinds/22_grainset.md`

> Historical record of ratified Grainset decisions. Live items are in [`../open/22_questions.md`](../open/22_questions.md); deferred items in [`../deferred/22_questions.md`](../deferred/22_questions.md).

---

## Q-GRN-004 — Grainset-of-Grainset nesting

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `data-kinds/26_nesting_matrix.md §R2`: no same-variant self-nesting at any depth. Grainset-of-Grainset is forbidden structurally (not just via `COMP_E_2207`). `[TD-GRAINSET-NESTED]` is retired — admitting nested Grainsets becomes a post-v1 matrix relaxation, not a Round-2 item.

**Question.** `22 §3.4` / `COMP_E_2207` currently forbids a `Grainset` as a child of another `Grainset` (`[TD-GRAINSET-NESTED]`). Should Round 2 admit nested Grainsets? What is the semantic?

**Refs.**

- `22 §3.4` — deferred; `COMP_E_2207` fires at compile.
- `12 §2` — nesting matrix; the current cell is "Grainset ⇨ Grainset: forbidden."
- `25` — applicability matrix; the canonical location for ratifying the cell.
- `16 §5` — `ComposedSemanticInterface`; nesting works structurally, but the semantics need to be defined.

**Arguments for forbidding (Round-1 default).**

- The author's use case is unclear: a nested Grainset is semantically equivalent to a flat Grainset with the union of children.
- Nesting compounds the cost-rank and tie-break axes.
- Avoid premature abstraction; ratify when a concrete use case appears.

**Arguments for admitting.**

- Use case: an author might want to group "daily sources" in one inner Grainset (to share a rollup policy across them) and "monthly sources" in another, composed under an outer Grainset.
- Matches the open-extension philosophy.

**Current position.** Forbidden via `COMP_E_2207` and now structurally via `26 §R2`. Body retained as historical resolution context.

---

## Q-GRN-006 — Single-child Grainset degeneracy: lint or accept?

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `data-kinds/26_nesting_matrix.md §R3`: every `ComplexDataKind` (including `Grainset`) REQUIRES ≥ 2 children. Single-child is now a structural rejection, not a silent-accept. This unifies the policy across `Unionset`, `Grainset`, and `Joinset` (all three require ≥ 2 children). `[TD-GRAINSET-SINGLE-CHILD]` and the parallel `[TD-UNIONSET-SINGLE-CHILD]` are retired.

**Question.** A Grainset with exactly one child is structurally valid per `22 §2` but semantically degenerate — it is a one-child wrapper that adds nothing over the underlying DataKind. Should Round 1 accept silently, emit a lint, or reject?

**Refs.**

- `22 §2.1` — Round-1: `children: Vec<GrainsetChild>` with `VALID_E_2201` firing only on empty.
- `22 §9.2` — no current advisory for single-child degeneracy.
- Similar pattern: a Unionset with one branch, a Joinset with one member — same shape of question for `23` / `24`.

**Arguments for silent accept (Round-1 default).**

- Useful during Model evolution.
- Symmetric with a Grainset that *loses* children via refactoring down to one.

**Arguments for lint (`PLAN_W_22xx`).**

- Signals "you probably meant to either add more children or replace with the underlying DataKind."

**Arguments for reject (`VALID_E_22xx`).**

- Keeps the Model sharp.

**Current position.** **CLOSED — single-child Grainset is rejected structurally** via `26 §R3`. Body retained as historical resolution context.
