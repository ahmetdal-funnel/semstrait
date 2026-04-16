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

All Rust skills are enabled by default for this workspace. For any Rust-touching task:

1. **Always** load `rust-router` first to analyze intent and route to specific skills.
2. **Always** consult `coding-guidelines` (naming, rustfmt, clippy) and `m15-anti-pattern` (code review) before finalizing changes.
3. Load on-demand when triggers apply:

| Area | Skills |
|---|---|
| Compile errors, ownership, borrow, lifetime | `m01-ownership`, `m03-mutability` |
| Smart pointers, RAII, resource lifecycle | `m02-resource`, `m12-lifecycle` |
| Generics, traits, dyn, type-driven design | `m04-zero-cost`, `m05-type-driven` |
| Error handling (`Result`/`Option`/panics) | `m06-error-handling`, `m13-domain-error` |
| Async, concurrency, `Send`/`Sync` | `m07-concurrency` |
| Domain modeling, DDD, invariants | `m09-domain` |
| Performance, benchmarks, profiling | `m10-performance` |
| Crates, features, workspace, FFI | `m11-ecosystem`, `unsafe-checker` |
| Learning / onboarding / analogies | `m14-mental-model` |
| Crate/std version and API lookup | `rust-learner`, `rust-daily` |
| Domain-specific (web, CLI, cloud-native, fintech, embedded, IoT, ML) | `domain-web`, `domain-cli`, `domain-cloud-native`, `domain-fintech`, `domain-embedded`, `domain-iot`, `domain-ml` |
| Codebase navigation via LSP | `rust-code-navigator`, `rust-symbol-analyzer`, `rust-call-graph`, `rust-trait-explorer`, `rust-deps-visualizer` |
| Safe refactors | `rust-refactor-helper` |
| Authoring new skills from docs | `rust-skill-creator` |

Never skip `rust-router` — it is the dispatcher. If a Rust task feels trivial, still run the router once to confirm no deeper skill applies.

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
