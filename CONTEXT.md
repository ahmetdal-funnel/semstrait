# Context

The authoritative project-mode signpost is `[AGENTS.md](AGENTS.md)` (tool-neutral: Cursor / Claude Code / Codex).

Project stage:

- `spec-driven-dev` is active.
- Priority is to cleanly define and ratify design specs.
- Implementation/migration follows after spec closure.

## Quick links

- **Design/spec sessions (mandatory start order)**:
  1. `[docs/design/00_overview.md](docs/design/00_overview.md)`
  2. `[docs/design/STATUS.md](docs/design/STATUS.md)`
  3. `[docs/design/INDEX.md](docs/design/INDEX.md)`
  4. `[docs/design/DOCS_MAINTENANCE.md](docs/design/DOCS_MAINTENANCE.md)`
- **Code/refactor session routing** -> `[CLAUDE.md](CLAUDE.md)`

## Authority rule

- `docs/design/` is authoritative for target-state architecture.
- Other docs (including `DECISION_LOG.md`, root `README.md`, and legacy `docs/*.md` outside `docs/design/`) are **current-code reference only**.
- When current-state docs conflict with spec docs, follow `docs/design/` and reconcile through spec artifacts.