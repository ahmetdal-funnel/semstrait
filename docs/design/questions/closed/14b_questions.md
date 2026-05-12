---
doc: design/questions/closed/14b_questions
status: Closed
purpose: Resolved questions originally raised against `foundations/14b_expression_resolution.md`
---

# Closed Questions — `foundations/14b_expression_resolution.md`

This file holds Round-1 questions from `docs/design/foundations/14b_expression_resolution.md` whose decisions have been ratified. Live items are in [`../open/14b_questions.md`](../open/14b_questions.md).

Each entry preserves its full Round-1 framing for historical context. The status banner records the ratification.

---

## OQ-4. Cycle-detection algorithm — Tarjan vs. linear scan — CLOSED (2026-04-28)

**Status: CLOSED.** Option A (Tarjan SCC) ratified by `14b §5.3` + `14b §12 Q8`. The decision is stable; no review trigger is expected to fire (see Round-1 rationale below for context). Round-1 framing retained for historical reference.

**Question.** For cycle detection over the reference DAG, 14b Round-1 chose Tarjan's strongly-connected-components algorithm. A lighter alternative exists: a linear DFS with a "gray / black" visited set that aborts on a back-edge.

Two options:

- **(A) Tarjan SCC.** Detects **every** cycle in one pass; topological sort is a free side-product reused for resolution ordering.
- **(B) DFS with back-edge abort.** Terminates on the **first** cycle found; requires a separate topological sort pass.

**14b default (Round 1).** (A) Tarjan SCC.

**Rationale.**

- Tarjan is a well-known, well-tested algorithm; correctness is cheap to audit.
- Topological sort is a genuine requirement for §6.1's bottom-up inference; (A) gives it for free.
- Reporting the full SCC in `CompileError::CyclicReference { cycle }` gives the author the whole cycle at once.
- Performance is linear in graph size; no practical difference for realistic reference DAGs (hundreds to low thousands of Semantics).

**Review trigger.** Algorithmic simplification only becomes attractive if profiling demonstrates Tarjan overhead on unusually large Models, which is unlikely per the size bounds above. Likely never revisited.

**Tech-debt tag.** (none — stable default).

**Linked docs.** 14b §5.3.
