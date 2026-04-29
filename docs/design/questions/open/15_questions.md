---
doc: design/questions/open/15_questions
status: Living
purpose: Open questions surfaced while drafting `foundations/15_mapping_and_binding.md`
depends-on:
  - foundations/15_mapping_and_binding.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/14b_expression_resolution.md
  - apis/30_api_contracts.md
  - apis/33_semstrait_manifest.md
  - apis/37_semstrait_catalog.md
---

# Open Questions — `foundations/15_mapping_and_binding.md`

> **Reconciliation (Phase-3 / 2026-04-27 consolidation).** When this doc says `ColumnMapping` / `ColumnMappingValue` / `ColumnMappingValue::Computed`, read **`SemanticMapping` / `SemanticMappingValue`** with the v1 4-variant roster `{Column(String), Literal(LiteralValue), Expr(PhysicalExpr), Metadata(MetadataDimensionRecipe)}` per [`../../foundations/18_entities.md §10`](../../foundations/18_entities.md). The earlier `[TD-MAP-METADATA-FOLD]` tech-debt marker is **resolved (2026-04-27)** — `Metadata` is restored as a distinct 4th variant; v1 metadata extraction scope is **path-only**; partition extraction is deferred to v2. The `Computed` variant was renamed to `Expr`.

> Round-1 framing for the four live items below; closed items moved to [`../closed/15_questions.md`](../closed/15_questions.md); deferred items in [`../deferred/15_questions.md`](../deferred/15_questions.md). Each entry restates the question, lists its ratified references, and records the Round-1 default `15` currently uses. Entries migrate out of this file as later docs (`16`, `33`, `37`, per-DataKind `21`–`25`) confirm or amend `15`'s defaults.

---

## Q-MAP-001 — `BindingId` uniqueness: per-SemanticManifest or cross-SemanticManifest?

> **Cross-link (2026-04-28).** This entry is the **authoritative home** for the `BindingId` scope-and-stability decision (per `15`'s `authoritative-for: BindingId` claim). The 14b consumer-side restatement [`OQ-7`](14b_questions.md#oq-7-stability-of-bindingid--relationshipid-across-compiles) tracks the same surface; both retire together when this entry resolves.

**Question.** `15 §2.2` ratifies `BindingId(pub u32)` as unique **within a SemanticManifest** (per-compile counter; identical Models produce identical IDs IF the compile driver's iteration order is deterministic; recompile of a modified Model shifts IDs). Should `BindingId` instead carry a cross-SemanticManifest identity — e.g. by including the SemanticManifest's content hash into the ID? That would let two SemanticManifests be compared on a per-Binding basis without ambiguity.

**Refs.**

- `15 §2.2` — per-SemanticManifest scope.
- `14b §2` — `ResolvedExprKey { semantics_name, binding_id }`; assumes `binding_id` is valid within the SemanticManifest it came from.
- `00 §4.1` (`BindingId` row) — not explicitly defined; inherits from `15`.
- `33` (pending) — SemanticManifest persistence and cross-SemanticManifest comparability.

**Arguments for per-SemanticManifest (current Round-1 default).**

- `u32` shape is simple, small, cheap. Matches `14b`'s keying shape.
- Two SemanticManifests are distinct artifacts; the DataKind identity (`DataKindId` per `11`) already provides cross-SemanticManifest comparability for what matters — "is this the same kind?". `BindingId` per-SemanticManifest is the Resolved-layer analogue of "the N-th Binding I built this time."
- Re-`compile` of a modified Model SHOULD be expected to produce a different SemanticManifest; ID drift is not a leak.

**Arguments for cross-SemanticManifest (would amend `15 §2.2`).**

- Enables differential tooling: "diff SemanticManifest A vs B, show which Bindings changed." Per-SemanticManifest IDs make this hard (IDs shift for unrelated reasons).
- Content-hash-derived IDs auto-invalidate consumers holding stale IDs.

**Current position in `15`.** Per-SemanticManifest. A future `33` ratification can override by redefining `BindingId` to include a SemanticManifest hash; `14b` would follow.

**Next step.** Revisit at `33` drafting time. If `33` ratifies a cross-SemanticManifest diff operator, the ID shape may tighten.

---

## Q-MAP-003 — Compile-synthesis of derived-Measure `ColumnMapping` entries

**Question.** When a `Measure(Count, DerivesFrom(Key))` is declared per `11 §8.4`, should the author have to write a `ColumnMapping` entry for it, or should `compile` synthesize `ColumnMappingValue::Computed { expr: Count(Column(key_col)) }` from the Constraint automatically?

**Refs.**

- `11 §8.4` — `Constraint::DerivesFrom(Key)` on a Measure.
- `15 §5.6` — completeness rule; currently proposes compile-synthesis.
- `32` (pending) — Model-parse vs. compile-synthesis division of labor.

**Arguments for compile-synthesis (Round-1 default).**

- Matches author expectation: a derived Measure is logically "already defined" by the Constraint; the ColumnMapping entry is redundant ceremony.
- The key column is already mapped; the Measure's physical recipe is mechanically derivable.

**Arguments against.**

- Implicit synthesis complicates error messages: "Measure X is missing a `ColumnMapping` entry" is clear; "Measure X's Constraint synthesizes to `Count(...)` but the Key `Y`'s column is itself a `NullFill` source" is subtler.
- Violates the "Model contents equals SemanticManifest contents, flattened" mental model by inserting compile-synthesized entries into the SemanticManifest.

**Current position in `15`.** Compile-synthesis; the authored `ColumnMapping` may omit derived-Measure entries and the compile step fills them in before the completeness check (§10 step 4.3).

**Next step.** Confirm at `32` drafting — if the Model parser prefers to materialize the synthesis at parse time (making it a `SemanticModel`-level operation, not a `compile`-level one), amend §5.6 / §10.4.

---

## Q-MAP-006 — `ResolvedColumnMapping.computed` storage: duplicate or alias?

**Question.** `14b §4`'s `ResolvedExprTable` is a global `(SemanticsName, BindingId) → PhysicalExpr` map. `15 §7.5`'s per-Binding `computed: HashMap<SemanticsName, PhysicalExpr>` serves the same data for per-Binding lookup. Does the SemanticManifest store the `PhysicalExpr` twice (duplicated), or do the per-Binding values alias into the global table?

**Refs.**

- `14b §4` — global `ResolvedExprTable`.
- `15 §7.5` — per-Binding denormalization.
- `33` (pending) — SemanticManifest storage strategy.

**Arguments for duplication (Round-1 default).**

- Simpler. The planner always reads from the per-Binding map; no pointer-following, no lifetime gymnastics.
- `PhysicalExpr` is an owned tree — duplicate storage has a real-but-small overhead.
- Rust's ownership model is simpler without `Arc<PhysicalExpr>` / indirect lookups.

**Arguments for aliasing.**

- Memory overhead on huge Models (thousands of Semantics × many Bindings) could matter.
- Single source of truth: editing the expression in one place updates both views.

**Current position.** Duplicate storage by default; `33` may override.

**Next step.** `33` benchmarks the SemanticManifest's in-memory footprint; if duplication is material, switch to `Arc<PhysicalExpr>` shared between table and per-Binding map.

---

## Q-MAP-010 — Nullability mismatch: warning or error?

**Question.** `15 §9.4` treats a non-nullable declared Semantics bound to a nullable physical column as a **warning** (`COMP_W_0306`). Should it be an **error**?

**Refs.**

- `15 §9.4` — Round-1 default: warning.
- `14 §5.2` — type inference on nullability.
- `11 §6` — Semantics-level nullability declaration.

**Arguments for warning (Round-1 default).**

- Source-reported nullability is often conservative (Parquet marking `optional` for a column that is always populated in practice).
- The runtime engine will raise on actual null occurrence; compile-time rejection would block legitimate workflows.
- Authors can override with an explicit `filter: IS NOT NULL` Dimension / Measure to tighten the constraint.

**Arguments for error.**

- Type system rigor: declared Non-nullable should mean Non-nullable. Accepting a nullable source silently erodes the invariant.
- Surprising runtime failures on a "production-grade" semantic layer are worse than a verbose compile-time rejection.

**Current position.** Warning. Promotion to error is a v2 conversation.

**Next step.** Gather early-usage feedback; if authors consistently hit runtime nullability errors after ignoring the warning, promote to error with a migration note.
