---
doc: design/questions/closed/23_questions
status: Closed
purpose: Resolved questions originally raised against `data-kinds/23_unionset.md`
---

# Closed Questions — `data-kinds/23_unionset.md`

> Historical record of ratified Unionset decisions. Live items are in [`../open/23_questions.md`](../open/23_questions.md).

---

## Q-UNI-002 — `UnionMode::Distinct` in v1 or deferred?

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `foundations/18_entities.md §2` (adjacency reference): v1 `UnionMode` roster is **`{All, Unique}`**, `#[non_exhaustive]`, default `All`. The previous spelling `Distinct` is renamed to `Unique` (authors who need SQL-`DISTINCT` semantics at Union-level select `Unique`; the three-valued-logic NULL-collision caveat documented in `23 §4.3` still applies as `PLAN_W_2303`). The "defer Distinct to post-v1" alternative is rejected; `Unique` is supported in v1.

**Question.** `23 §2.1` ratifies a `UnionMode` enum with two variants: `All` (default) and `Distinct`. Distinct semantics in the presence of NULL-fill are subtle (per `23 §4.3`'s three-valued-logic note): rows from different children differing only in NULL-fill positions do NOT dedupe under `DISTINCT`. Should Round 1 ship `Distinct` at all, or defer it to a later milestone?

**Refs.**
- `23 §2.1, §4.1, §4.3` — Round-1: `UnionMode::Distinct` supported, with advisory `PLAN_W_2303` for the NULL × NULL non-collision case.
- `UNIONSET.md` (legacy) — distinguishes `UNION ALL` vs. `UNION DISTINCT`.
- Cube.js `unionAll` — does not expose a DISTINCT mode at the cube level.
- dbt MetricFlow — does not have a direct Unionset analog.
- `[TD-UNIONSET-DISTINCT-SEMANTICS]` — deferred subsection under §4.3.

**Current position.** Both modes supported in v1; `UnionMode::All` is the default. Body retained as historical resolution context.

---

## Q-UNI-009 — Single-child Unionset acceptance: reject (Round-1) vs silent accept vs warning

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `data-kinds/26_nesting_matrix.md §R3`: every `ComplexDataKind` (including `Unionset`) REQUIRES ≥ 2 children. Rejection stands (via R3 + `VALID_E_2302`); the "silent accept" alternative from `22 Q-GRN-006` (now also closed by R3) no longer creates asymmetry. All three Complex variants (`Unionset`, `Grainset`, `Joinset`) share the ≥ 2 children rule. `[TD-UNIONSET-SINGLE-CHILD]` is retired.

**Question.** `23 §8.1` fires `VALID_E_2302 UnionsetSingleChild` on a Unionset with exactly one child. The rationale: "semantically the child itself; authors should replace." Is rejection right, or should Round 1 be more permissive (silent accept or warning)?

**Refs.**
- `23 §8.1` — Round-1: rejection via `VALID_E_2302`.
- `12 §3.2` — `UnionsetMustHaveMultipleChildren`; structural minimum.
- `22 Q-GRN-006` (parallel) — Grainset ratifies silent accept for single-child. Asymmetric decisions across sibling docs.
- `24` (pending) — Joinset; what does a one-member Joinset do?

**Current position.** Rejection (now structurally enforced via `26 §R3`). Body retained as historical resolution context.
