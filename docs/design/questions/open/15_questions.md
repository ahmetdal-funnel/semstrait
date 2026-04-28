---

## doc: design/questions/open/15_questions
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

# Open Questions — `foundations/15_mapping_and_binding.md`

> **Reconciliation (Phase-3 / 2026-04-27 consolidation).** When this doc says `ColumnMapping` / `ColumnMappingValue` / `ColumnMappingValue::Computed`, read `**SemanticMapping` / `SemanticMappingValue`** with the v1 4-variant roster `{Column(String), Literal(LiteralValue), Expr(PhysicalExpr), Metadata(MetadataDimensionRecipe)}` per `[../foundations/18_entities.md §10](../foundations/18_entities.md)` (promoted from the earlier `apis/32c_entities.md` home in the same pass). The earlier `[TD-MAP-METADATA-FOLD]` tech-debt marker (which folded `Metadata` into `Expr`) is **resolved (2026-04-27)** — `Metadata` is restored as a distinct 4th variant; see `[../foundations/15_mapping_and_binding.md](../foundations/15_mapping_and_binding.md)`'s top banner / §1.1 / §13 (R48) for the resolution narrative. v1 metadata extraction scope is **path-only**; partition extraction is deferred to v2. The `Computed` variant was renamed to `Expr`. The SemanticManifest-layer `ResolvedColumnMapping` name is intentionally **retained** at the `33 §5.3` surface pending a follow-up rename decision, so `Q-MAP-006` ("`ResolvedColumnMapping.computed` storage") remains authoritative for the Resolved layer.

> Items surfaced during Round-1 drafting of the mapping-and-binding foundations doc. Each entry restates the question, lists its ratified references, and records the Round-1 default `15` currently uses. Entries migrate out of this file as later docs (`16`, `33`, `37`, per-DataKind `21`–`25`) make decisions that either confirm or amend `15`'s defaults.

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

- SemanticManifest persistence (`33`) is cleaner if all partition-related data lives in one place. A consumer reading a `ResolvedBinding` can answer "what's the grain of source *i*'s month-partition?" without calling back into `37`.
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
- Violates the "Model contents equals SemanticManifest contents, flattened" mental model by inserting compile-synthesized entries into the SemanticManifest.

**Current position in `15`.** Compile-synthesis; the authored `ColumnMapping` may omit derived-Measure entries and the compile step fills them in before the completeness check (§10 step 4.3).

**Next step.** Confirm at `32` drafting — if the Model parser prefers to materialize the synthesis at parse time (making it a `SemanticModel`-level operation, not a `compile`-level one), amend §5.6 / §10.4.

---

## Q-MAP-004 — JSON nested-field inference — CLOSED (2026-04-28)

**Status: CLOSED.** v1 ratifies "no recursion" — nested-object inference is out of scope per `00 §10` (complex types out of scope) and `15 §4.4` (no recursion). Authors flatten / unnest at ingest. Round-1 framing retained for historical reference; the question reactivates only if `00 §10` is amended to admit complex `DataType` variants.

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

## Q-MAP-005 — Is `Derived` distinct from `Native`? — CLOSED (2026-04-28)

**Status: CLOSED.** `15 §6.3` ratifies `Derived` as a distinct variant from `Native`; the distinction surfaces in pushdown reasoning and composition-level `FieldOwnership::Derived(expr)` (`16 §7.3`). The 4-variant `CoverageVariant { Native, NullFill, Derived, Metadata }` is now load-bearing across `15 §13 R22`, `16 §8`, and the per-DataKind eligibility predicates (`22 §3.3`, `24 §8.4`). Round-1 framing retained for historical reference.

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

## Q-MAP-007 — `path.token` index convention — CLOSED (2026-04-27)

**Status: CLOSED.** Option A ratified — `path.token: N` is **0-indexed, scheme-stripped**. Segments are slash-delimited; leading `/` produces no empty token; consecutive `/` collapse. Example: `"s3://bucket/year=2024/month=01/data.parquet"` → tokens `[bucket, year=2024, month=01, data.parquet]`; `token: 1` = `"year=2024"`.

**Refs.**

- `13 §4.7` — `PathExtraction { token: u32 }` (v1 in scope per §4.7 v1-scope banner).
- `15 §8.1` — layer-3 mechanic, ratified.
- `15 §13 R28` — entry confirmed `✓ (Q-MAP-007 closed)`.
- `18 §10.4` — `MetadataExtraction::Path { token: u32 }` (v1).

**Resolution rationale.** Matches `Path::components` (Rust) / `pathlib.Path.parts` (Python). The `partition.level` 1-indexing asymmetry is itself v2-deferred (Q-MAP-009), so the consistency-with-partition-levels argument no longer applies in v1.

---

## Q-MAP-008 — Path-token extraction: raw segment vs value-after-`=` — CLOSED (2026-04-27)

**Status: CLOSED.** v1 ratifies **raw segment only**. `path_token` returns the whole slash-delimited segment (`"year=2024"`) as a `String`, never the `=`-suffix value. Authors who need value-after-`=` compose it via a separate `Expr`-mapped Dimension calling `substring_after(@source_segment, '=')` from the `14a` function catalog, where `@source_segment` is itself a metadata-bound Dimension. No companion `path.token_value: N` variant is ratified in v1.

**Refs.**

- `15 §8.1.1` — token extraction result type, ratified raw-only.
- `15 §13 R29` — entry confirmed `✓ (Q-MAP-008 closed)`.
- `14a` — function catalog hosts `substring_after(str, delim)` for the composition.
- `15 §1` — three-stratum note explains how constant-folding at the `SemanticExpr → PhysicalExpr` lowering boundary collapses the composition into a per-source `Literal` in practice.

**Resolution rationale.** Path structures vary (`year=2024`, `y2024`, `2024`); a `=`-split-baked-in extraction would fail on the latter two. Returning the whole segment keeps the layer-3 contract narrow and makes the value-extraction parse behavior explicit and auditable in author code.

**v2 consideration.** If early-usage feedback shows authors routinely wrap the raw extraction with `substring_after`, a `path.token_value: N` variant can be ratified additively as MINOR per `30 §6.3` (the `MetadataExtraction` enum is `#[non_exhaustive]`).

---

## Q-MAP-009 — Hive-style partition value type — DEFERRED to v2 (2026-04-27)

**Status: DEFERRED to v2.** Partition extraction (`partition.level: N`) is non-goal in v1 — the v1 metadata extraction surface is **path-only** per `15 §8.0` / `13 §4.7` v1-scope banner / `15 §13 R47`. The compile pass guards `partition: Some(_)` with `COMP_E_0322 MetadataPartitionDeferredV2`. The result-type contract for Hive-style partitions (raw segment vs value-after-`=`, declared override grammar, type-inference fallback) reactivates when v2 ratifies the partition arm.

**Refs.**

- `15 §8.0` — v1 scope: path-only.
- `15 §8.2 / §8.2.1` — v2 design parking with the original options preserved for future reference.
- `15 §11.1 COMP_E_0322` — v1 compile-time guard.
- `15 §13 R31` — entry confirmed `DEFERRED v2`.
- `13 §4.7` — author-side body retained for forward-compat with v1-scope banner.

**Original options (preserved for v2 ratification).**

- **A:** raw value post-`=`, typed `String`. Authors cast downstream via `Expr`.
- **B:** raw value, typed per author-declared partition-column type (new YAML surface).
- **C:** auto-detect from first encountered value.

**Next step.** Address during the v2 partition-extraction ratification pass; the Hive-style result-type decision rolls into the broader partition-arm design.

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