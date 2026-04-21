# AGENTS.md

Entry-point context for any AI coding agent (Cursor, Claude Code, Codex, etc.) working in this repository.

---

## Project mode

`semstrait` is currently in **`spec-driven-dev` mode**.

The project is undergoing a green-field design exercise. The authoritative specification for the target architecture lives under [`docs/design/`](docs/design/). The current code does **not** yet match the spec; a phased migration (described in [`docs/design/implementation/40_refactor_plan.md`](docs/design/implementation/40_refactor_plan.md)) is planned but not yet executed.

---

## Required first reads (in order)

Any session touching design / specification / open Q&A MUST read, in this exact order:

1. **[`docs/design/00_overview.md`](docs/design/00_overview.md)** — the spec contract. Establishes canonical vocabulary, the document map, design invariants, and directionality rules for every other doc under `docs/design/`.
2. **[`docs/design/STATUS.md`](docs/design/STATUS.md)** — living session-handoff file. Current spec phase, active reconciliation items, deferred topics, open Q&A rounds, and the last-checkpoint summary.

Never skip step 1. Never skip step 2.

For code / implementation / refactor work (not spec work), [`CLAUDE.md`](CLAUDE.md) routes to the correct per-area documentation. The `docs/design/` tree remains the target-state source of truth; legacy `docs/*.md` describe the current code state until migration lands.

---

## Three documentation buckets

| Bucket | Path | Status | Use when |
|---|---|---|---|
| **Spec — target state (authoritative)** | [`docs/design/`](docs/design/) | Active, ratified in waves | Designing, ratifying, answering spec Q&A, writing new API contracts |
| **Legacy — current code state (reference-only)** | [`docs/*.md`](docs/) (outside `docs/design/`), per-crate `README.md` | Frozen; will be retired by migration | Understanding *what the code does today* (not what it should do) |
| **Code — migration-in-progress** | [`crates/`](crates/) | Diverges from spec; migration not started | Code edits until spec is finalized; cite both current behavior and target spec when changing code |

The mapping between legacy and spec documents is in [`docs/design/00_overview.md §11`](docs/design/00_overview.md). The retirement schedule is in [`docs/design/implementation/41_deprecations.md`](docs/design/implementation/41_deprecations.md).

---

## Branches

| Branch | Role |
|---|---|
| `feature/base-semastrait-dev` | Code-level development against the current architecture |
| `feature/spec-driven-dev` (**active**) | Spec-driven design work; the `docs/design/` tree is maintained here |

Spec work is committed to `feature/spec-driven-dev`. Code work that anticipates the spec (early migration work) should also target this branch once the refactor plan is ratified.

---

## Session-close discipline

Before ending a session that touched spec content, propose an update to [`docs/design/STATUS.md`](docs/design/STATUS.md) reflecting:

- Items moved from pending → completed
- New deferred items (with a pointer to any open-questions file)
- The next-session starting point (one or two sentences)

The human approves the proposed status update before it is written.

---

## Do not

- Rewrite entries in [`docs/*.md`](docs/) legacy docs as part of spec work. Legacy docs are archived; if spec content contradicts them, fix the spec, not the legacy.
- Treat [`README.md`](README.md) or [`DECISION_LOG.md`](DECISION_LOG.md) as specification sources. Both describe code state; the spec tree is authoritative for target-state decisions.
- Introduce abstractions in the spec tree that do not ground in current code (see the lesson captured in `STATUS.md`: desugaring already-implemented shapes is a recurring failure mode — describe, clean, and name what exists; do not fabricate parallel models).
