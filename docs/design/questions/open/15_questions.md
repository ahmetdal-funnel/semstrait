---
doc: design/questions/open/15_questions
status: Living
purpose: Open questions surfaced while drafting `foundations/15_mapping_and_binding.md`
depends-on:
  - foundations/15_mapping_and_binding.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/19_expression_flow.md
  - apis/30_api_contracts.md
  - apis/33_semstrait_manifest.md
  - apis/37_semstrait_catalog.md
---

# Open Questions — `foundations/15_mapping_and_binding.md`

> **Reconciliation (Phase-3 / 2026-04-27 consolidation).** When this doc says `ColumnMapping` / `ColumnMappingValue` / `ColumnMappingValue::Computed`, read **`SemanticMapping` / `SemanticMappingValue`** with the v1 4-variant roster `{Column(String), Literal(LiteralValue), Expr(PhysicalExpr), Metadata(MetadataDimensionRecipe)}` per [`../../foundations/18_entities.md §10`](../../foundations/18_entities.md). The earlier `[TD-MAP-METADATA-FOLD]` tech-debt marker is **resolved (2026-04-27)** — `Metadata` is restored as a distinct 4th variant; v1 metadata extraction scope is **path-only**; partition extraction is deferred to v2. The `Computed` variant was renamed to `Expr`.

> Round-1 framing for the four live items below; closed items moved to [`../closed/15_questions.md`](../closed/15_questions.md); deferred items in [`../deferred/15_questions.md`](../deferred/15_questions.md). Each entry restates the question, lists its ratified references, and records the Round-1 default `15` currently uses. Entries migrate out of this file as later docs (`16`, `33`, `37`, per-DataKind `21`–`25`) confirm or amend `15`'s defaults.

---

## Q-MAP-001 — `BindingId` uniqueness — MOOTED → see `closed/15`

> **MOOTED by the id-first rework (STATUS item U.2).** `BindingId` is eliminated: bindings are identified by a deterministic content-derived `EntityId` (UUIDv5, `33 §9.1`), which is globally unique and cross-run/cross-edit stable — answering the original per-vs-cross-manifest scope question by construction (cross-manifest comparison by binding `EntityId` is meaningful for unchanged content). Full resolution recorded in [`../closed/15_questions.md`](../closed/15_questions.md).

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

## Q-MAP-006 — `ResolvedColumnMapping.computed` storage: duplicate or alias? — CLOSED (2026-05-28)

> **Moved to [`../closed/15_questions.md`](../closed/15_questions.md).** Settled by C11 + C12 (Manifest Ratification Log, 2026-05-28): expressions live in `ManifestExpressions` as split typed pools (`semantic: BTreeMap<SemanticExprId, SemanticExpr>` + `physical: BTreeMap<PhysicalExprId, PhysicalExpr>`); bindings reference into the physical pool by `PhysicalExprId`. Neither duplicated nor aliased — single source of truth via typed-id reference.

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
