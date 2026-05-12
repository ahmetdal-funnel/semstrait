# AGENTS.md

Entry-point context for any AI coding agent (Cursor, Claude Code, Codex, etc.) working in this repository.

---

## Project mode

`semstrait` is currently in `**spec-driven-dev` mode**.

The authoritative target-state specification lives under `[docs/design/](docs/design/)`.  
The current code does **not** yet match that target; migration is planned in `[docs/design/implementation/40_refactor_plan.md](docs/design/implementation/40_refactor_plan.md)` but not yet executed.

Current stage policy:

1. Define and ratify design specs cleanly.
2. Execute implementation/migration after spec closure.

---

## Required first reads (in order)

Any session touching design/spec/open Q&A MUST read, in this exact order:

1. `[docs/design/00_overview.md](docs/design/00_overview.md)` — contract: vocabulary, invariants, directionality.
2. `[docs/design/STATUS.md](docs/design/STATUS.md)` — active handoff state.
3. `[docs/design/INDEX.md](docs/design/INDEX.md)` — concept/topic navigator.
4. `[docs/design/DOCS_MAINTENANCE.md](docs/design/DOCS_MAINTENANCE.md)` — authoring discipline.

Never skip steps 1-2.

---

## Documentation authority model


| Bucket                                              | Path                                                  | Role                                              |
| --------------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------- |
| **Spec (authoritative target state)**               | `[docs/design/](docs/design/)`                        | Design and ratification source of truth           |
| **Legacy docs (reference-only current code state)** | `docs/*.md` outside `docs/design/`, crate `README.md` | Understand current behavior only                  |
| **Code (migration target implementation)**          | `[crates/](crates/)`                                  | Implementation surface that will converge to spec |


Rule:

- Use `docs/design/` for target-state decisions.
- Use legacy docs only to understand current code behavior.
- Never treat legacy docs as authoritative for architecture direction.

---

## Branches


| Branch                                 | Role                                         |
| -------------------------------------- | -------------------------------------------- |
| `feature/base-semastrait-dev`          | Code-level work against current architecture |
| `feature/spec-driven-dev` (**active**) | Spec-driven design work                      |


Spec work belongs on `feature/spec-driven-dev`.

---

## Session-close discipline (spec sessions)

Before ending a spec session, propose a concise update to `[docs/design/STATUS.md](docs/design/STATUS.md)`:

- moved items (open/closed/deferred state changes),
- newly deferred items,
- next-session starting point.

Human approval is required before writing final status updates.

---

## Do not

- Rewrite legacy `docs/*.md` as part of spec work.
- Treat `[README.md](README.md)` or `[DECISION_LOG.md](DECISION_LOG.md)` as target-state specification sources.
- Introduce spec abstractions that do not ground in current code (unless explicitly marked as proposed extension).