---
title: Manifest–SemanticGraph Post-Ratification Verification
status: completed
review_target: docs/design/apis/33_semstrait_manifest.md + docs/design/apis/35_semstrait_ir.md
related: docs/design/apis/34_semstrait_planner.md, docs/design/foundations/16_composition.md
---

# Manifest + SemanticGraph Verification

Post-verification cleanup focused on boundary clarity, deterministic graph inputs, abstraction hygiene, and phase-owned validation.

## Hard-constraint verification

| Constraint | Result | Evidence |
|---|---|---|
| 1) No overuse of single-use abstractions | **Pass** | `33 §6.1/§6.7` removed the standalone `PathOrigin` abstraction and reuses `DataKindOrigin` directly. |
| 2) Shared behavior uses traits/enums | **Pass** | Shared behavior remains encoded in reusable enums/newtypes (`DataKindOrigin`, `GraphExprRef`, `ExprLayer`, `PhysicalExprId`; `SemanticExprId` retired by STATUS item V). |
| 3) Bitmap/bitmask semantics explicit | **Pass** | `33 §5` / `§10` / `§12` define bit-position mapping, canonical encoding, and load-time invalid-bit rejection. |
| 4) Manifest does not build graph | **Pass** | `33 §1` / `§2` / `§4.4` define primitives-only persistence and graph-build ownership in planner runtime. |
| 5) Expressions resolvable via graph | **Pass** | `35 §2A` requires pool-typed `GraphExprRef` and build-time reference resolution before fragment admission; `35 §5.3.2` keeps compile→graph→plan lookup-only flow. |

## Deterministic manifest payload (`33 §4.4`)

The manifest now states one deterministic graph-builder input contract:

- source identity/indexing inputs (`SourceId`, `locator`, `version_ref`, `schema_fingerprint`)
- composition/topology hints (`data_kinds`, `relationships`, `interfaces`, join/grain/union payloads)
- explicit bitmap/bitmask semantics (`SemanticBitmap`, canonical `SemanticBitmask`)
- physical-only expression pool with applicability layer (`ManifestExpression { expr, layer }`, `PhysicalExprId`, `GraphExprRef`)
- derived runtime indices (name/adjacency/composition lookups) are graph-build outputs, not manifest fields

## Validation ownership checkpoints

Explicit phase ownership is stated in `33 §9.5`:

- model parse (`32`)
- manifest compile gates (`33`, G1–G5)
- manifest load integrity (`33`, CX1)
- runtime graph build checks (`34` over `35` graph types, including cycle rejection and expression-ref resolution)
- planner plan validation (`34` + `35::SemanticPlan::validate`)

Cycle policy is explicit: expression cycles are compile-time; graph-fragment cycles are graph-build-time; relationship-graph global cycle gate (G6) remains deferred.

## Consistency cleanup across related docs

- `34` boundary sections (`§1.4A`, `§2`, legacy note) are consistent with the manifest-primitives + runtime-graph split.
- `16` remains aligned with the same posture: manifest persists primitives, while composition indices are synthesized at graph build.

## Remaining risks / open questions

- `Q-MAN-D03` / `Q-MAN-D10`: relationship-graph global cycle gate (G6) still deferred.
- `Q-MAN-D11`: nested-variant consolidation stays split for v1 readability.
- `Q-IR-SPEC-05`: accessor-enum dedup remains deferred; explicit enums favored for now.
