---
doc: design/open_questions/15_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `foundations/15_mapping_and_binding.md`
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

> **Reconciliation (Phase-3 / 2026-04-17 consolidation).** When this doc says `ColumnMapping` / `ColumnMappingValue` / `ColumnMappingValue::Computed` / `ColumnMappingValue::Metadata`, read **`SemanticMapping` / `SemanticMappingValue`** with the v1 roster `{Column(String), Literal(LiteralValue), Expr(PhysicalExpr)}` per [`../foundations/18_entities.md §10`](../foundations/18_entities.md) (promoted from the earlier `apis/32c_entities.md` home in the same pass). The `Metadata` variant is folded into `Expr` under the tech-debt marker `[TD-MAP-METADATA-FOLD]` in [`../foundations/15_mapping_and_binding.md`](../foundations/15_mapping_and_binding.md); see §1's reconciliation banner there for the sketched authoring path. The `Computed` variant was renamed to `Expr`. The Manifest-layer `ResolvedColumnMapping` name is intentionally **retained** at the `33 §5.3` surface pending a follow-up rename decision, so `Q-MAP-006` ("`ResolvedColumnMapping.computed` storage") remains authoritative for the Resolved layer.

> Items surfaced during Round-1 drafting of the mapping-and-binding foundations doc. Each entry restates the question, lists its ratified references, and records the Round-1 default `15` currently uses. Entries migrate out of this file as later docs (`16`, `33`, `37`, per-DataKind `21`–`25`) make decisions that either confirm or amend `15`'s defaults.

---

## Q-MAP-001 — `BindingId` uniqueness: per-Manifest or cross-Manifest?

**Question.** `15 §2.2` ratifies `BindingId(pub u32)` as unique **within a Manifest** (per-compile counter; identical Models produce identical IDs IF the compile driver's iteration order is deterministic; recompile of a modified Model shifts IDs). Should `BindingId` instead carry a cross-Manifest identity — e.g. by including the Manifest's content hash into the ID? That would let two Manifests be compared on a per-Binding basis without ambiguity.

**Refs.**
- `15 §2.2` — per-Manifest scope.
- `14b §2` — `ResolvedExprKey { semantics_name, binding_id }`; assumes `binding_id` is valid within the Manifest it came from.
- `00 §4.1` (`BindingId` row) — not explicitly defined; inherits from `15`.
- `33` (pending) — Manifest persistence and cross-Manifest comparability.

**Arguments for per-Manifest (current Round-1 default).**
- `u32` shape is simple, small, cheap. Matches `14b`'s keying shape.
- Two Manifests are distinct artifacts; the DataKind identity (`DataKindId` per `11`) already provides cross-Manifest comparability for what matters — "is this the same kind?". `BindingId` per-Manifest is the Resolved-layer analogue of "the N-th Binding I built this time."
- Re-`compile` of a modified Model SHOULD be expected to produce a different Manifest; ID drift is not a leak.

**Arguments for cross-Manifest (would amend `15 §2.2`).**
- Enables differential tooling: "diff Manifest A vs B, show which Bindings changed." Per-Manifest IDs make this hard (IDs shift for unrelated reasons).
- Content-hash-derived IDs auto-invalidate consumers holding stale IDs.

**Current position in `15`.** Per-Manifest. A future `33` ratification can override by redefining `BindingId` to include a Manifest hash; `14b` would follow.

**Next step.** Revisit at `33` drafting time. If `33` ratifies a cross-Manifest diff operator, the ID shape may tighten.

---

## Q-MAP-002 — Partition-transform record placement

**Question.** Iceberg partition columns have transforms (`identity`, `year`, `month`, `hour`, `bucket[N]`, `truncate[N]`). Where should the transform live? On `PartitionColumn` in `15`, or only on `37`'s catalog-response type?

**Refs.**
- `15 §3.4` — `PartitionColumn { name, position, data_type, nullable }`. No `transform` field in v1.
- Legacy `CATALOG_RESOLUTION.md §4` — `PartitionTransform` enum on `PartitionField`.
- `37` (pending) — `CatalogProvider::load_table_metadata` response includes partition transforms.
- `22 Grainset` + `17 TemporalShape` — consume partition transforms for grain-aware planning.

**Arguments for leaving transforms in `37`.**
- `15` is engine-agnostic; partition transform is a catalog-flavored concept (Iceberg-ish; generalizes less cleanly to Hive-style path partitioning where the "transform" is implicit in the path structure).
- Grain inference from partition transforms is a planner concern (`22`, `17`), not a Binding concern. The `Coverage` machinery in `15 §6` does not need to know the transform.
- Keeping `15` minimal — one field fewer on `PartitionColumn` — keeps the cross-crate contract narrower.

**Arguments for promoting to `15`.**
- Manifest persistence (`33`) is cleaner if all partition-related data lives in one place. A consumer reading a `ResolvedBinding` can answer "what's the grain of source *i*'s month-partition?" without calling back into `37`.
- Currently the planner must cross-reference `ResolvedBinding` + the catalog response; co-locating reduces plan-time state.

**Current position in `15`.** Transform lives in `37`'s response; `PartitionColumn` omits it in v1. `22` / `17`'s planner steps consume the transform from a `ResolvedBinding`-adjacent side-channel (shape to be decided in `33`).

**Next step.** Decide at `17 TemporalShape` ratification — if `17`'s as-of / snapshot logic demands per-partition grain access without round-tripping `37`, promote the transform to a `15 §3.4` field.

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
- Violates the "Model contents equals Manifest contents, flattened" mental model by inserting compile-synthesized entries into the Manifest.

**Current position in `15`.** Compile-synthesis; the authored `ColumnMapping` may omit derived-Measure entries and the compile step fills them in before the completeness check (§10 step 4.3).

**Next step.** Confirm at `32` drafting — if the Model parser prefers to materialize the synthesis at parse time (making it a `SemanticModel`-level operation, not a `compile`-level one), amend §5.6 / §10.4.

---

## Q-MAP-004 — JSON nested-field inference

**Question.** For JSON sources without a declared schema, `15 §4.4` infers scalar fields from a sample but does NOT recurse into nested objects (they fall through as `String`). Is that the right v1 stance?

**Refs.**
- `15 §4.4` — Round-1 default: no recursion.
- `13 §3` — complex types out of scope per `00 §10`.
- `15 §4.3` — `JsonShape { Ndjson, JsonArray }`.

**Arguments for no-recursion (Round-1 default).**
- Matches `00 §10` non-goal (complex types out of scope).
- Nested-object inference is a rabbit hole (`depth N?`, recursive structures, polymorphic unions).
- Authors wanting nested access can unnest upstream (the ingestion-layer answer).

**Arguments for recursion.**
- Lake JSON is often nested (event streams, API captures); flattening at ingest is extra work.
- Peer engines (DuckDB, DataFusion) infer nested types readily.

**Current position.** No recursion; nested-object fields become `String`. Authors unnest upstream.

**Next step.** Revisit when / if `DataType` grows complex-type variants (`Array`, `Struct`); currently blocked by `00 §10`.

---

## Q-MAP-005 — Is `Derived` distinct from `Native`?

**Question.** `15 §6.3` ratifies `Coverage::Derived` as distinct from `Native` — distinguishing "physical column is directly present" from "upstream columns for a Computed expression are present." Is the distinction pulling its weight, or could `Derived` collapse into `Native`?

**Refs.**
- `15 §6.1` — variant definition.
- `15 §6.3` — current ratification.
- `34 §5` (pending) — pushdown reasoning consumes the distinction.

**Arguments for keeping `Derived` distinct (Round-1 default).**
- Pushdown reasoning in `34 §5` benefits: `Native` reads are cheap (direct column scan); `Derived` reads push a computation into the scan (more expensive / subject to predicate-pushdown differences).
- `16`'s composition-level Coverage may need the distinction to surface provenance accurately on a `ComposedSemanticInterface`.

**Arguments for collapsing.**
- Simpler: the planner sees "source covers this Semantics natively" — the engine is responsible for efficient retrieval, not the canonical layer.
- `34` does not yet exist; the pushdown-reasoning use case is speculative.

**Current position.** Distinct. `Derived` remains its own variant.

**Next step.** Revisit at `34` drafting. If `34`'s pushdown logic does not materially consume the distinction, collapse `Derived` → `Native` on `Computed`-valued Semantics and simplify §6.

---

## Q-MAP-006 — `ResolvedColumnMapping.computed` storage: duplicate or alias?

**Question.** `14b §4`'s `ResolvedExprTable` is a global `(SemanticsName, BindingId) → PhysicalExpr` map. `15 §7.5`'s per-Binding `computed: HashMap<SemanticsName, PhysicalExpr>` serves the same data for per-Binding lookup. Does the Manifest store the `PhysicalExpr` twice (duplicated), or do the per-Binding values alias into the global table?

**Refs.**
- `14b §4` — global `ResolvedExprTable`.
- `15 §7.5` — per-Binding denormalization.
- `33` (pending) — Manifest storage strategy.

**Arguments for duplication (Round-1 default).**
- Simpler. The planner always reads from the per-Binding map; no pointer-following, no lifetime gymnastics.
- `PhysicalExpr` is an owned tree — duplicate storage has a real-but-small overhead.
- Rust's ownership model is simpler without `Arc<PhysicalExpr>` / indirect lookups.

**Arguments for aliasing.**
- Memory overhead on huge Models (thousands of Semantics × many Bindings) could matter.
- Single source of truth: editing the expression in one place updates both views.

**Current position.** Duplicate storage by default; `33` may override.

**Next step.** `33` benchmarks the Manifest's in-memory footprint; if duplication is material, switch to `Arc<PhysicalExpr>` shared between table and per-Binding map.

---

## Q-MAP-007 — `path.token` index convention

**Question.** For `path.token: N` extraction, what are the segments and how are they counted?

**Refs.**
- `13 §4.7` — `PathExtraction { token: usize }`.
- `15 §8.1` — Round-1 default: 0-indexed post-scheme.

**Options.**
- **A (Round-1 default):** 0-indexed, scheme-stripped. `"s3://bucket/year=2024/month=01/data.parquet"` → tokens `[bucket, year=2024, month=01, data.parquet]`; token 0 = `"bucket"`, token 1 = `"year=2024"`.
- **B:** 0-indexed, scheme included. Same path → tokens `[s3:, , bucket, year=2024, ...]` after split; empty tokens from `//` are ugly.
- **C:** 1-indexed, scheme-stripped. Same path → token 1 = `"bucket"`, token 2 = `"year=2024"`. Matches the `partition.level` 1-indexing from §8.2 for internal consistency.

**Arguments for Option A (current).**
- Matches common path-indexing conventions (`Path::components` in Rust, `pathlib.Path.parts` in Python — both post-scheme).
- 0-indexed is the norm for list-indexed access in Rust.

**Arguments for Option C (alternative).**
- Internal consistency with `partition.level: N` (1-indexed). Having one 0-indexed and one 1-indexed extraction type is a papercut.

**Current position.** Option A (0-indexed, scheme-stripped). The internal-consistency critique is noted; the rationale for asymmetry is "arrays are 0-indexed, levels are 1-indexed" — matches the author's mental model (path tokens feel like array access; partition levels feel like hierarchical positions).

**Next step.** Confirm in Round 2; if the asymmetry collects author-facing bug reports in early usage, flip to Option C.

---

## Q-MAP-008 — Path-token extraction: raw segment vs value-after-`=`

**Question.** `path.token: N` extracts the raw segment (`"year=2024"`) by Round-1 default. Should a companion extraction that returns only the value (`"2024"`) be ratified in v1?

**Refs.**
- `15 §8.1.1` — Round-1 default: raw segment.
- `14a` — function catalog includes `substring_after(str, delim)` which composes the value-only form.

**Arguments for raw-only (Round-1 default).**
- Simpler surface; one extraction kind, one semantic.
- Path structures vary (`year=2024`, `y2024`, `2024`); forcing a `=` split baked-in would fail on the latter two.
- Value extraction is always expressible via a `Computed` wrapping (`substring_after(Metadata(path.token = 1), "=")`) with explicit parse behavior.

**Arguments for a second extraction.**
- Common case; every Hive-style path has the same `=`-suffix structure. Having to wrap every usage in `substring_after` is noise.
- Declarative is cleaner than imperative for a well-defined pattern.

**Current position.** Raw-only. Authors compose `substring_after` explicitly.

**Next step.** Round 2 — if early-usage feedback shows authors routinely wrap the raw extraction with `substring_after`, ratify a `path.token_value: N` variant.

---

## Q-MAP-009 — Hive-style partition value type

**Question.** For Hive-style path partitioning (`year=2024/month=01`), what is the `PartitionColumn.data_type` and what is the value extraction type?

**Refs.**
- `15 §8.2.1` — Round-1 default: raw value post-`=`, typed `String`.

**Options.**
- **A (current):** raw value, typed `String`. The author is responsible for downstream casting in a `Computed` expression.
- **B:** raw value, typed per an author-declared type (new YAML surface: `partitions: [year { type: Integer }, month { type: Integer }]`). Compile-time parses and errors if path values don't match.
- **C:** auto-detect type from the first encountered value. Integers, dates (`YYYY-MM-DD`), and booleans parse; else `String`.

**Arguments for A (Round-1 default).**
- Minimal surface. `String` is always safe. Explicit cast in `Computed` is the escape.
- Matches `37`'s handling of declared partitions from Iceberg (where the type is explicit) — the Hive-style path case is the "no catalog" case where type inference is on the author.

**Arguments for B / C.**
- Every real Model wants partition values as their natural type. Forcing a `Computed` cast everywhere is friction.
- B (explicit declaration) is more reliable than C (auto-detection).

**Current position.** Option A. B is a future extension (YAML-surface addition on the Binding spec; `32` would own the surface, `15` would consume the declared type into `PartitionColumn.data_type`).

**Next step.** Punt to `32` during its drafting; a declared-partition-type YAML surface is additive and MINOR-compatible.

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

---
