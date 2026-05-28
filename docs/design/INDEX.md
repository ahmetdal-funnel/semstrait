# semstrait Design Index

Status: **Living**

This file is the primary navigator for `docs/design/`.

Use this file when:
- you know the concept and need its canonical home quickly;
- you need a first-read path for a specific task;
- you want to confirm question state and where a Q-ID currently lives.

Use `00_overview.md` when you need the governing contract (vocabulary, invariants, directionality rules).

---

## 1) First-Read Paths

### Any spec/design session
1. [`00_overview.md`](00_overview.md)
2. [`STATUS.md`](STATUS.md)
3. Relevant section below by topic

### Topic routing

| If you are working on... | Start here | Then read |
|---|---|---|
| Pipeline semantics and stage boundaries | [`foundations/10_resolution_pipeline.md`](foundations/10_resolution_pipeline.md) | [`apis/30_api_contracts.md`](apis/30_api_contracts.md) |
| Names, scopes, Semantics elements | [`foundations/11_names_and_scopes.md`](foundations/11_names_and_scopes.md) | [`foundations/16_composition.md`](foundations/16_composition.md) |
| Types, expressions, function semantics | [`foundations/13_types_and_grain.md`](foundations/13_types_and_grain.md) | [`foundations/14_expressions.md`](foundations/14_expressions.md) (type architecture), [`foundations/14a_function_catalog.md`](foundations/14a_function_catalog.md) (registry + function-level `Additivity`), [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) (compile pipeline) |
| Expression compile pipeline, resolution algorithm, sugar, placement | [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) | [`foundations/14_expressions.md`](foundations/14_expressions.md), [`foundations/14a_function_catalog.md`](foundations/14a_function_catalog.md), [`apis/34_semstrait_planner.md`](apis/34_semstrait_planner.md) |
| Mapping/binding and metadata synthesis | [`foundations/15_mapping_and_binding.md`](foundations/15_mapping_and_binding.md) | [`apis/32_semstrait_model.md`](apis/32_semstrait_model.md), [`apis/33_semstrait_manifest.md`](apis/33_semstrait_manifest.md) |
| Temporal semantics | [`foundations/17_temporal_shape.md`](foundations/17_temporal_shape.md) | [`data-kinds/22_grainset.md`](data-kinds/22_grainset.md), [`data-kinds/23_unionset.md`](data-kinds/23_unionset.md) |
| DataKind taxonomy and variant behavior | [`data-kinds/20_taxonomy.md`](data-kinds/20_taxonomy.md) | [`data-kinds/21_dataset.md`](data-kinds/21_dataset.md), [`data-kinds/22_grainset.md`](data-kinds/22_grainset.md), [`data-kinds/23_unionset.md`](data-kinds/23_unionset.md), [`data-kinds/24_joinset.md`](data-kinds/24_joinset.md), [`data-kinds/25_applicability_matrix.md`](data-kinds/25_applicability_matrix.md), [`data-kinds/26_nesting_matrix.md`](data-kinds/26_nesting_matrix.md) |
| Crate-level API contract | [`apis/30_api_contracts.md`](apis/30_api_contracts.md) | target crate doc in `31`-`39` |
| Engine/provider mapping details | [`registry/README.md`](registry/README.md) | concrete mapping catalog(s) |
| Migration/refactor planning | [`implementation/40_refactor_plan.md`](implementation/40_refactor_plan.md) | [`implementation/41_deprecations.md`](implementation/41_deprecations.md), [`implementation/42_migration_notes.md`](implementation/42_migration_notes.md) |

---

## 2) Canonical Document Map

### Foundations (1x)
- `10` [`foundations/10_resolution_pipeline.md`](foundations/10_resolution_pipeline.md) — stage contracts and pipeline flow.
- `11` [`foundations/11_names_and_scopes.md`](foundations/11_names_and_scopes.md) — naming, scope, Semantics definitions.
- `12` [`foundations/12_nesting_policy.md`](foundations/12_nesting_policy.md) — nesting constraints and structural matrix.
- `13` [`foundations/13_types_and_grain.md`](foundations/13_types_and_grain.md) — canonical logical types and Grain.
- `14` [`foundations/14_expressions.md`](foundations/14_expressions.md) — canonical expression model.
- `14a` [`foundations/14a_function_catalog.md`](foundations/14a_function_catalog.md) — canonical function identity/registry.
- `14b` — Retired 2026-05-18; merged into `19`. History: [`questions/closed/14b_questions.md`](questions/closed/14b_questions.md).
- `15` [`foundations/15_mapping_and_binding.md`](foundations/15_mapping_and_binding.md) — mapping/binding algorithm.
- `16` [`foundations/16_composition.md`](foundations/16_composition.md) — relationships and composed interfaces.
- `17` [`foundations/17_temporal_shape.md`](foundations/17_temporal_shape.md) — temporal shape model.
- `18` [`foundations/18_entities.md`](foundations/18_entities.md) — canonical entity type definitions.
- `19` [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) — unified compile pipeline (Phase A resolution + Phase B placement; sugar contract; advisory channel; error model).

### DataKinds (2x)
- `20` [`data-kinds/20_taxonomy.md`](data-kinds/20_taxonomy.md) — taxonomy and shared invariants.
- `21` [`data-kinds/21_dataset.md`](data-kinds/21_dataset.md) — Dataset behavior.
- `22` [`data-kinds/22_grainset.md`](data-kinds/22_grainset.md) — Grainset behavior.
- `23` [`data-kinds/23_unionset.md`](data-kinds/23_unionset.md) — Unionset behavior.
- `24` [`data-kinds/24_joinset.md`](data-kinds/24_joinset.md) — Joinset behavior.
- `25` [`data-kinds/25_applicability_matrix.md`](data-kinds/25_applicability_matrix.md) — cross-variant applicability matrix.
- `26` [`data-kinds/26_nesting_matrix.md`](data-kinds/26_nesting_matrix.md) — allowed nesting combinations.

### API contracts (3x)
- `30` [`apis/30_api_contracts.md`](apis/30_api_contracts.md) — cross-crate API and stability policy.
- `31` [`apis/31_semstrait_common.md`](apis/31_semstrait_common.md) — `semstrait-common`.
- `31b` [`apis/31b_semstrait_common_io.md`](apis/31b_semstrait_common_io.md) — `semstrait-common::io`.
- `32` [`apis/32_semstrait_model.md`](apis/32_semstrait_model.md) — `semstrait-model`.
- `32b` [`apis/32b_catalogs_yaml.md`](apis/32b_catalogs_yaml.md) — catalogs YAML side-surface.
- `33` [`apis/33_semstrait_manifest.md`](apis/33_semstrait_manifest.md) — `semstrait-manifest`.
- `34` [`apis/34_semstrait_planner.md`](apis/34_semstrait_planner.md) — `semstrait-planner`.
- `35` [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) — `semstrait-ir`.
- `36` [`apis/36_semstrait_adapter.md`](apis/36_semstrait_adapter.md) — `semstrait-adapter`.
- `37` [`apis/37_semstrait_catalog.md`](apis/37_semstrait_catalog.md) — `semstrait-catalog`.
- `38` [`apis/38_semstrait_api.md`](apis/38_semstrait_api.md) — `semstrait-api`.
- `39` [`apis/39_semstrait_facade.md`](apis/39_semstrait_facade.md) — top-level facade crate.

### Implementation stubs (4x)
- `40` [`implementation/40_refactor_plan.md`](implementation/40_refactor_plan.md)
- `41` [`implementation/41_deprecations.md`](implementation/41_deprecations.md)
- `42` [`implementation/42_migration_notes.md`](implementation/42_migration_notes.md)

### Registry catalogs (living)
- [`registry/README.md`](registry/README.md) — registry policy and catalog index.
- [`registry/types_mapping.md`](registry/types_mapping.md)
- [`registry/functions_mapping.md`](registry/functions_mapping.md)
- [`registry/temporal_shape_mapping.md`](registry/temporal_shape_mapping.md)
- [`registry/join_types_mapping.md`](registry/join_types_mapping.md)

---

## 3) High-Value Concept Map (single-home pointers)

| Concept | Canonical home |
|---|---|
| Pipeline (`parse -> validate -> compile -> plan -> optimize -> adapt`) | [`foundations/10_resolution_pipeline.md`](foundations/10_resolution_pipeline.md) |
| Semantics element types and naming constraints | [`foundations/11_names_and_scopes.md`](foundations/11_names_and_scopes.md) |
| `DataType`, `Grain` | [`foundations/13_types_and_grain.md`](foundations/13_types_and_grain.md) |
| `Expr<L>`, `SemanticExpr`, `PhysicalExpr`, `SemanticLeaf`, `PhysicalLeaf` | concept: [`foundations/14_expressions.md`](foundations/14_expressions.md); crate-of-record: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| `Tree`, `Visitor`, `Rewriter`, `ExprLeaf` (traversal trait family) | concept: [`foundations/14_expressions.md`](foundations/14_expressions.md) §3.1 / §3.2; crate-of-record: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| Structural-variant support enums (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`) and `Literal` typed-literal carrier | concept: [`foundations/14_expressions.md`](foundations/14_expressions.md) §3.3; crate-of-record: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| Identifier carriers (`ColumnRef`, `SemanticsName`) | concept: [`foundations/14_expressions.md`](foundations/14_expressions.md) §3.4 / §3.5; crate-of-record: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| `ExprSource` (Inline-DSL / `Block(Expr<L>)` authoring carrier — no separate `ExprBlock` AST) | concept: [`foundations/14_expressions.md`](foundations/14_expressions.md) §6.1; crate-of-record: [`apis/32_semstrait_model.md`](apis/32_semstrait_model.md) |
| Narrow expression-side error kinds (`ValidateError`, `CompileError`) | concept: [`foundations/14_expressions.md`](foundations/14_expressions.md) §10 (I12 row); crate-of-record: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| Non-coercion / pass-through posture (no implicit type promotion at canonical layer) | [`foundations/14_expressions.md`](foundations/14_expressions.md) §5.4 |
| `CanonicalFn`, `FunctionRegistry`, function-level `Additivity` | concept: [`foundations/14a_function_catalog.md`](foundations/14a_function_catalog.md); crate-of-record: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| `RegistryExtension` (per-adapter function-catalog extension) | concept: [`foundations/14a_function_catalog.md`](foundations/14a_function_catalog.md) §7; consumer: [`apis/36_semstrait_adapter.md`](apis/36_semstrait_adapter.md) §7 |
| `SemanticPlan`, `PlanNode` (closed sum: `Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch`) | [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) |
| `SemanticGraph`, `SemanticNode`, `SemanticEdge`, `SemanticGraphFragment`, `SegmentKey` (canonical graph type system) | [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md) §2A |
| Planner runtime graph lifecycle (TODO/provisional boundary; runtime DAG backend target = `daggy`) | [`apis/34_semstrait_planner.md`](apis/34_semstrait_planner.md) §1.4A |
| `EngineAdapter` (engine-dispatch trait) | [`apis/36_semstrait_adapter.md`](apis/36_semstrait_adapter.md) §3 |
| `DialectEmit` (per-dialect SQL rendering trait) | [`apis/36_semstrait_adapter.md`](apis/36_semstrait_adapter.md) §4 |
| `Capability` enum + `AdapterCapabilities` (cross-boundary capability vocabulary; SQL adapters = ergonomic, Substrait = handoff contract) | enum body: [`apis/35_semstrait_ir.md`](apis/35_semstrait_ir.md); per-adapter rosters: [`apis/36_semstrait_adapter.md`](apis/36_semstrait_adapter.md) §6 |
| Expression compile-pipeline (Phase A resolution + Phase B placement) | [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) |
| Two-phase expression flow, `resolve`, sugar (Family A/B) | [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) |
| Per-kind typed semantic leaves (`Field`, `Dimension`, `Measure`, `Metric`, `Key`); accessor enums (`MeasureAccessor`, `DimensionAccessor`, `MetricAccessor`, `KeyAccessor`); `Parameter` | [`foundations/14_expressions.md`](foundations/14_expressions.md) |
| Manifest expression persistence (`ManifestExpressions`, `SemanticExpr` storage; runtime physical realization boundary) | [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md), [`apis/33_semstrait_manifest.md`](apis/33_semstrait_manifest.md) §4.6 |
| Auto-mapping synthesis pre-step, `Column`-under-manual-mapping rejection | [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) §3.11 |
| `ManifestExpressions` (manifest expression storage shape) | [`apis/33_semstrait_manifest.md`](apis/33_semstrait_manifest.md) §4.6 |
| `RequestDimensionRef`, `DimensionVariation` (rollup-aware Dimension carrier) | [`apis/34_semstrait_planner.md`](apis/34_semstrait_planner.md) §3.10 (consumer cross-ref in `[foundations/19_expression_flow.md](foundations/19_expression_flow.md) §6.2`) |
| Effective-`Additivity` composition (function-level × model-level → planner-effective) | [`foundations/19_expression_flow.md`](foundations/19_expression_flow.md) §6.5 |
| `SemanticMapping` and binding flow | [`foundations/15_mapping_and_binding.md`](foundations/15_mapping_and_binding.md) |
| `Relationship`, composed interface semantics | [`foundations/16_composition.md`](foundations/16_composition.md) |
| `TemporalShape` and shape semantics | [`foundations/17_temporal_shape.md`](foundations/17_temporal_shape.md) |
| Canonical entities (`Cardinality`, `Integrity`, `Optional`, `CrossFilter`, derived `JoinType`, `DimensionType`, `AggregationType`, etc.) | [`foundations/18_entities.md`](foundations/18_entities.md) |
| `DataKind` taxonomy and trait axes | [`data-kinds/20_taxonomy.md`](data-kinds/20_taxonomy.md) |
| Typed diagnostics contract and observability policy | [`apis/30_api_contracts.md`](apis/30_api_contracts.md) |
| Unified API error sum (`SemStraitErrorKind`) | [`apis/38_semstrait_api.md`](apis/38_semstrait_api.md) |

---

## 4) Question-State Dashboard

Question sidecars are stateful by directory. Directory is authoritative for state.

- Open (active v1 backlog): [`questions/open/`](questions/open/)
- Closed (historical ratifications): [`questions/closed/`](questions/closed/)
- Deferred (post-v1 / parked): [`questions/deferred/`](questions/deferred/)

Current snapshot:

| Directory | Files | Lines |
|---|---:|---:|
| `open/` | 23 | ~2140 |
| `closed/` | 21 | ~1740 |
| `deferred/` | 20 | ~1225 |

For registry-specific questions, use the aggregate navigator:
- [`questions/open/registry_questions.md`](questions/open/registry_questions.md)

---

## 5) Sync Contract (must-follow)

When a canonical concept owner changes, update in the same commit:
1. The authoritative source doc.
2. This file (`INDEX.md`).
3. `STATUS.md` if phase/reconciliation/question state changed.

When question status changes:
1. Move/land the Q entry body into the correct state directory.
2. Leave only a short forwarding stub in the previous location when needed for discoverability.
3. Reflect state changes in `STATUS.md`.

For full authoring discipline and AI editing rules, see:
- [`DOCS_MAINTENANCE.md`](DOCS_MAINTENANCE.md)

