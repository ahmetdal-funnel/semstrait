# 14b — Open Questions (Round 1)

This companion file tracks decisions from `docs/design/foundations/14b_expression_resolution.md` that were **deferred** or **proposed as Round 1 defaults with a review owed later**. Each entry has:

- **Question** — what is open.
- **14b default (Round 1)** — the working answer the body of 14b uses so downstream docs can reference a concrete contract.
- **Rationale** — why this default was chosen.
- **Review trigger** — when the decision should be revisited.
- **Tech-debt tag** — the `[TD-14B-*]` label referenced in 14b §12.1.

Decisions in this file are **not** ratified. The Ratified Decisions Index in 14b §12 covers the 18 ratified decisions Q1–Q18.

---

## OQ-1. Manifest-level `PhysicalExpr` interning

**Question.** Should `ResolvedExprTable` entries store their `PhysicalExpr` inline (one tree per entry) or intern them into a separate expression pool with ID-based references?

**14b default (Round 1).** Inline. No interning. An entry's `physical_expr: PhysicalExpr` is a standalone tree.

**Rationale.**
- Resolved expressions are typically small (1–20 nodes); duplication across entries is moderate.
- Interning adds a second indirection layer every planner pass must chase (ID → pool lookup → node).
- Manifests are produced once and consumed many times; decode-time simplicity beats encode-time size reduction.
- A first-cut implementation is simpler to audit for correctness.

**Review trigger.** If Manifest sizes pass a comfort threshold (proposed: >10 MB compressed for typical Models) or planner decode is measurably dominated by expression-tree allocation, revisit.

**Tech-debt tag.** `[TD-14B-EXPR-INTERN]`

**Linked docs.** `33` (Manifest), §2.4.

---

## OQ-2. Multi-`EntityRef` path composition

**Question.** When a single `SemanticExpr` contains two or more `EntityRef`s that each require **different** cross-kind paths (e.g. measure A references measure B one hop away and measure C two hops away), how should `path_signature.paths` represent the combined requirement?

Two options:
- **(A) Distinct paths.** `path_signature.paths: BTreeSet<RelationshipPath>` stores one entry per discovered path; the planner dedupes / composes at plan time.
- **(B) Canonicalized / intersected paths.** 14b attempts to merge paths that share a prefix so the planner sees a single composed chain.

**14b default (Round 1).** (A) distinct paths. `path_signature.paths` is a set of whatever `RelationshipPath`s each `EntityRef` contributed.

**Rationale.**
- 14b does not know the planner's join-subgraph canonicalization rules (those live in `16`). Letting the planner see raw per-`EntityRef` paths keeps 14b's responsibility crisp.
- Set semantics already deduplicate identical paths.
- Shared-prefix canonicalization is a non-trivial graph operation with its own correctness questions (e.g. does two paths sharing a prefix imply one join subgraph or two?).

**Review trigger.** When `16`'s join-subgraph canonicalization ratifies its input shape. If `16` wants pre-canonicalized paths, 14b can add a canonicalization sub-pass.

**Tech-debt tag.** `[TD-14B-PATH-UNIFICATION]`

**Linked docs.** `16` (composition), 14b §4.5.

---

## OQ-3. Provenance — per-`EntityRef`-site granularity

**Question.** Should the `Provenance` struct on `ResolvedExprEntry` include a per-`EntityRef`-site location trail (useful for diagnosing "this cross-kind path error originated at line X column Y inside a deeply nested expression")?

Two options:
- **(A) Entry-level only.** `Provenance` carries per-entry Locations (merged-occurrence declarations, contributing variants). Per-node diagnostics use the `Location` the `14` AST already carries.
- **(B) Entry + per-`EntityRef`-site.** Augment `Provenance` with `Vec<EntityRefSite>` recording name, path walked, Location of the originating `EntityRef` node.

**14b default (Round 1).** (A) entry-level only.

**Rationale.**
- The `14` AST already carries a `Location` per node; diagnostics can reconstruct site-level trails from the expression tree itself.
- (B) duplicates information and enlarges every `ResolvedExprEntry`.
- Simpler Provenance is easier to serialize stably.

**Review trigger.** When the `--explain` tooling (`[TD-EXPLAIN-COMPILED]`) lands and needs to trace cross-kind path decisions through deep expression trees, revisit. If tree-walking to reconstruct trails is cumbersome, (B) may be worth the space.

**Tech-debt tag.** `[TD-14B-EXPR-PROVENANCE-SITES]`

**Linked docs.** 14b §2.6.

---

## OQ-4. Cycle-detection algorithm — Tarjan vs. linear scan

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

---

## OQ-5. Batch-diagnostic mode for resolution errors

**Question.** 14b Round-1 fails fast on the first resolution error (I12). Should there be an opt-in mode that collects **every** resolution error in one pass and returns them as a `Vec<Diagnostic>`?

**14b default (Round 1).** Fail-fast only. First error terminates compile.

**Rationale.**
- I12 is a binding invariant (fail-fast at compile).
- Batch aggregation requires plumbing through every compile sub-pass and handling the fact that some later errors may be caused by the first error's aftermath (e.g. if cycle detection fails, later type-inference errors are noise).
- Authors typically fix one error and re-run — the CLI round-trip is fast.

**Review trigger.** If author studies consistently show round-trip friction for models with many independent errors (e.g. a large migration that breaks dozens of Semantics simultaneously), revisit. A batch mode could be opt-in via a CLI flag without violating I12's default posture.

**Tech-debt tag.** `[TD-14B-BATCH-DIAGS]`

**Linked docs.** 14b §11.4.

---

## OQ-6. Join-key columns: inline vs. split on `ResolvedExprEntry`

**Question.** When cross-kind resolution traverses a `Relationship`, the join-key columns at each endpoint are added to `referenced_columns`. Should they instead live in a separate `required_join_keys: Vec<String>` field so that plan-time column pruning can distinguish "payload" from "join-keying" columns cheaply?

**14b default (Round 1).** Inline in `referenced_columns`. No separate field.

**Rationale.**
- The planner can still distinguish via `Relationship` metadata (the authoritative source for which columns are join keys).
- A single field keeps the entry shape smaller and simpler to serialize.
- Join-key columns are semantically "referenced" columns from the binding's perspective — the name is accurate.

**Review trigger.** If `16` or the optimizer's column-pruning pass gets materially faster with a pre-split field, revisit.

**Tech-debt tag.** (none — tracked implicitly by the Q12 decision in §12).

**Linked docs.** 14b §10.4.

---

## OQ-7. Stability of `BindingId` / `RelationshipId` across compiles

**Question.** 14b Round-1 assigns these IDs in parsed-Model iteration order; they shift if an upstream item is inserted. Should IDs be stabilized (e.g. a content hash of the declaring node) so Manifest diffs between compiles are minimized?

**14b default (Round 1).** Iteration-order IDs. Not stable across Model edits.

**Rationale.**
- No downstream component caches IDs across compile runs; they are internal to a single Manifest.
- Content-hash IDs add a build-time cost (hashing every Binding / Relationship) and a lookup cost at plan time (hash vs. `u32`).
- Manifest diffability is a separate concern that can be addressed at the serialization layer (`33`) without changing the in-memory ID scheme.

**Review trigger.** If diff-based Manifest caching becomes a feature (incremental re-compile, CI-level Manifest diff validation), revisit. A stable-ID scheme would be a prerequisite for content-addressable Manifest storage.

**Tech-debt tag.** (unnamed — will be opened when diffability becomes a ratified requirement).

**Linked docs.** 14b §2.1, §4.2.
