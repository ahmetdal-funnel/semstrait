## Project mode

`semstrait` is currently in `**spec-driven-dev` mode**. The authoritative specification for the target architecture lives under `[docs/design/](docs/design/)`. The current code does not yet match the spec; a phased migration (`[docs/design/implementation/40_refactor_plan.md](docs/design/implementation/40_refactor_plan.md)`) is planned but not yet executed.

See `[AGENTS.md](AGENTS.md)` for the tool-neutral project-mode signpost.

Every new design/spec session starts with:

1. `[docs/design/00_overview.md](docs/design/00_overview.md)`
2. `[docs/design/STATUS.md](docs/design/STATUS.md)`
3. `[docs/design/INDEX.md](docs/design/INDEX.md)`
4. `[docs/design/DOCS_MAINTENANCE.md](docs/design/DOCS_MAINTENANCE.md)`

## Workstyle

Never bypass the workstyle:

1. **Root cause analysis** -- identify and explain the root cause before proposing any fix
2. **Present results** -- show findings and proposed solution to the human
3. **Wait for confirmation** -- do NOT apply code changes until the human approves the fix
4. **Design / spec edits** -- in spec-driven-dev mode the design docs under `docs/design/` are the artifact, so the rule-3 discipline applies equally to design or spec edits there. Approval is clause-level: a directional pick (option letter, "yes" to a shape, "go") does NOT authorize derived implementation clauses (validation rules, error codes, edge cases). Each derived clause is its own decision.

For spec / design work specifically: describe, clean, and name what the code actually implements. Do not fabricate parallel abstractions. Any new concept in the spec tree must ground in current code or be explicitly flagged as a proposed extension. Use `[docs/design/DOCS_MAINTENANCE.md](docs/design/DOCS_MAINTENANCE.md)` for anti-drift editing discipline.

## Documentation Routing — Spec & Design Work (primary)

Any session touching design, specification, or open Q&A MUST read, in this exact order:

1. `[docs/design/00_overview.md](docs/design/00_overview.md)` — the spec contract (canonical vocabulary, document map, invariants)
2. `[docs/design/STATUS.md](docs/design/STATUS.md)` — living session-handoff (current phase, active items, deferred topics)
3. `[docs/design/INDEX.md](docs/design/INDEX.md)` — navigation map (concept + topic routing)
4. `[docs/design/DOCS_MAINTENANCE.md](docs/design/DOCS_MAINTENANCE.md)` — maintenance/editing rules

Then route by topic:


| Working on                                                                                   | Read first (after `00_overview.md` + `STATUS.md`)                                                                                                                                                                                                                                                                                                                            |
| -------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cross-cutting foundations (pipeline, names, types, expressions, composition, temporal shape) | `docs/design/foundations/` (start from `docs/design/INDEX.md` topic routing)                                                                                                                                                                                                                                                                                                 |
| DataKind variants (Dataset / Grainset / Unionset / Joinset), applicability matrix            | `docs/design/data-kinds/` (start from `docs/design/INDEX.md` topic routing)                                                                                                                                                                                                                                                                                                  |
| Per-crate public API contracts                                                               | `docs/design/apis/` (start from `docs/design/INDEX.md` topic routing)                                                                                                                                                                                                                                                                                                        |
| Refactor plan, deprecations, migration notes                                                 | `docs/design/implementation/` (start from `docs/design/INDEX.md` topic routing)                                                                                                                                                                                                                                                                                              |
| Per-engine mapping catalogs (types, functions, temporal, joins)                              | `docs/design/registry/` (start from `docs/design/INDEX.md` topic routing)                                                                                                                                                                                                                                                                                                    |
| Questions for any doc `N`                                                                    | `docs/design/questions/open/N_questions.md` (active v1 backlog); ratified items in `docs/design/questions/closed/N_questions.md`; post-v1 items in `docs/design/questions/deferred/N_questions.md`. The same `N_questions.md` filename can appear in multiple directories — each carries only the Q-IDs in that state. See the question dashboard in `docs/design/INDEX.md`. |


## Documentation Update Rule

Every code change that modifies types, adds/removes abstractions, or changes architectural patterns MUST update the relevant spec docs under `docs/design/` before the task is complete. Stale specs cause future sessions to operate on removed abstractions.

## Rust Skills

All Rust skills are enabled by default for this workspace. For any Rust-touching task:

1. **Always** load `rust-router` first to analyze intent and route to specific skills.
2. **Always** consult `coding-guidelines` (naming, rustfmt, clippy) and `m15-anti-pattern` (code review) before finalizing changes.
3. Load on-demand when triggers apply:


| Area                                                                 | Skills                                                                                                            |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Compile errors, ownership, borrow, lifetime                          | `m01-ownership`, `m03-mutability`                                                                                 |
| Smart pointers, RAII, resource lifecycle                             | `m02-resource`, `m12-lifecycle`                                                                                   |
| Generics, traits, dyn, type-driven design                            | `m04-zero-cost`, `m05-type-driven`                                                                                |
| Error handling (`Result`/`Option`/panics)                            | `m06-error-handling`, `m13-domain-error`                                                                          |
| Async, concurrency, `Send`/`Sync`                                    | `m07-concurrency`                                                                                                 |
| Domain modeling, DDD, invariants                                     | `m09-domain`                                                                                                      |
| Performance, benchmarks, profiling                                   | `m10-performance`                                                                                                 |
| Crates, features, workspace, FFI                                     | `m11-ecosystem`, `unsafe-checker`                                                                                 |
| Learning / onboarding / analogies                                    | `m14-mental-model`                                                                                                |
| Crate/std version and API lookup                                     | `rust-learner`, `rust-daily`                                                                                      |
| Domain-specific (web, CLI, cloud-native, fintech, embedded, IoT, ML) | `domain-web`, `domain-cli`, `domain-cloud-native`, `domain-fintech`, `domain-embedded`, `domain-iot`, `domain-ml` |
| Codebase navigation via LSP                                          | `rust-code-navigator`, `rust-symbol-analyzer`, `rust-call-graph`, `rust-trait-explorer`, `rust-deps-visualizer`   |
| Safe refactors                                                       | `rust-refactor-helper`                                                                                            |
| Authoring new skills from docs                                       | `rust-skill-creator`                                                                                              |


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
i1
3. **Data engineering** -- data model correctness, query pipeline integrity

