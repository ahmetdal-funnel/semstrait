## Workstyle

Never bypass the workstyle:
1. **Root cause analysis** -- identify and explain the root cause before proposing any fix
2. **Present results** -- show findings and proposed solution to the human
3. **Wait for confirmation** -- do NOT apply code changes until the human approves the fix

## Documentation Routing

Before modifying any crate, read its `README.md`. For cross-crate changes, also read `docs/ARCHITECTURE.md`.

| Working on | Read first |
|---|---|
| Any single crate | That crate's `README.md` |
| Cross-crate changes, constraints, DAG | `docs/ARCHITECTURE.md` |
| Grainset/Unionset/Joinset/Dataset planning | `docs/{GRAINSET,UNIONSET,JOINSET,DATASET}.md` |
| Catalog, storage, source resolution | `docs/CATALOG_RESOLUTION.md` |
| Function mapping, expression rewriting | `docs/FUNCTION_CATALOG.md` |
| Semantic model scoping, ref/override | `docs/SEMANTIC_RESOLUTION.md` |
| Computed dimensions, expressions | `docs/COMPUTED_EXPRESSIONS.md` |
| Known tech debt | `docs/TECH_DEBT.md` |

## Documentation Update Rule

Every code change that modifies types, adds/removes abstractions, or changes architectural patterns MUST update the relevant crate's `README.md` before the task is complete. Cross-cutting changes must also update `docs/ARCHITECTURE.md`. Stale docs cause future sessions to operate on removed abstractions.

## Rust Skills

Use rust-skills for all Rust code changes -- routing, code review, design analysis. Load rust-router first to determine which skills apply.

## Memory & Context Recall

Two memory systems are available:
- **Native memory** (`~/.claude/.../memory/`) -- curated rules, feedback, design decisions. MEMORY.md index is always loaded; read topic files when relevant.
- **memsearch** (`.memsearch/memory/`) -- auto-captured session history. Use `/memory-recall <query>` when past session context would help (prior decisions, debugging history, what was tried before).

When a task references prior work or past decisions, use `/memory-recall` before starting.

## Code Review

After each implementation phase, run multi-agent code review covering:
1. **Rust** -- idiomatic patterns, ownership, error handling, clippy
2. **Software design** -- architecture, separation of concerns
3. **Data engineering** -- data model correctness, query pipeline integrity
