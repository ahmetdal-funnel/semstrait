---
prereqs: [13, 14, 14a, 16, 17]
authoritative-for:
  - the `semstrait-ir` public-API surface (types, traits, free functions)
  - the canonical type vocabulary — `DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn` (variant rosters and structural rules ratified by `13`; `35` is the crate-level home post-types-migration)
  - the universal-traversal trait family — `Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf` (variant ratified by `14 §3.1 / §3.2`; `35` is the crate-level home post-second-cascade)
  - the `Expr<L>` structural enum implementation (variant catalog ratified by `14 §3.3`; `35` carries the crate-level home)
  - the structural-variant support enums shared by every leaf set — `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`, `Literal` (rosters ratified by `14 §3.3`; `35` is the crate-level home post-second-cascade)
  - the shared identifier carriers `ColumnRef` and `SemanticsName` (consumed by both leaf sets per `14 §3.4 / §3.5`; `35` is the crate-level home post-second-cascade)
  - the `PhysicalLeaf` and `SemanticLeaf` enums — canonical-IR leaf set and per-kind typed semantic leaf set per `14 §3.4 / §3.5`
  - the `PhysicalExpr = Expr<PhysicalLeaf>` and `SemanticExpr = Expr<SemanticLeaf>` type aliases per `14 §3.6`
  - the per-kind accessor enums (`DimensionAccessor`, `MeasureAccessor`, `MetricAccessor`, `KeyAccessor`) carried as `Option<…>` fields on the typed semantic leaves per `14 §4.1`
  - the `Parameter` placeholder struct and the closed `ParameterKey` enum per `14 §5`
  - the authoring-surface DSL — the `expr_fn` module with `col`, `field`, `dim`, `measure`, `metric`, `key` free constructors; `std::ops` impls on `SemanticExpr` and `PhysicalExpr`; the `ExprFunctionExt` extension trait — per `14 §9.2`
  - the `CanonicalFn` newtype and the `FunctionRegistry` / `FunctionSpec` / `FnSignature` / `ParamType` / `ReturnTypeRule` / `FunctionCategory` / `RegistryExtension` / `function_registry()` surface, moved from `semstrait-common` per `14a §2`
  - the narrow ir-emitted error kinds — `ValidateError` (raised by `Tree::with_new_children` and `Rewriter<N>::f_*`) and `CompileError` (raised by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec`); each implements `Diagnose` per `30 §5`. Downstream stages embed via D.ii kind-nesting (`30 §7.4`)
  - `SemanticPlan` — the canonical, engine-agnostic query plan tree
  - `PlanNode` sum type — variant roster (`Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch`) and per-variant shape
  - `EngineArtifact` / `EnginePlan` / `SqlArtifact` adapter-consumable output types (structural shape owned here; emission semantics in `36`)
  - `DialectId` roster and `Dialect` trait surface
  - shared plan-level primitives: `SourceRef`, `ResolvedColumn`, `Name`, `KeyPair`, `SortDir`, `NullOrdering`, `AggregateExpr`, `NodeMeta`
  - well-formedness invariants across a `SemanticPlan` tree (predicates are `PhysicalExpr`; types resolved per `19 §3`; group-by / join / scan schema alignment)
  - `IrErrorKind` enum and its `Diagnose` impl per `30 §5`
  - serde posture for `SemanticPlan`, `Expr<L>`, and the Substrait mapping table (conversion is `36`'s concern; mapping declared here)
  - on-wire shape of `Cardinality` / `JoinType` when they appear on a `PlanNode::Join` (vocabulary ratified in `16 §5`)
refined-by:
  - 14 (`Expr<L>`, leaf-set catalogs, accessor catalogs, `Parameter` shape, DSL constructor list, trait scaffolding, support-enum rosters — the type-architecture contract `35` implements)
  - 14a (`CanonicalFn` semantics, signature polymorphism, return-type rules, registry sealing protocol)
  - 19 (compile-pipeline — Phase A `SemanticExpr::resolve` produces the `PhysicalExpr`s `35` stores in `PlanNode`s; Phase B placement consumes them)
  - 34 (`semstrait-planner` produces `SemanticPlan` values; consumes `PhysicalExpr` and the sealed `FunctionRegistry`)
  - 36 (`semstrait-adapter` consumes `SemanticPlan` and produces `EngineArtifact`; owns the Substrait / SQL emission logic referenced here as a mapping only)
  - 40 (`implementation/40_refactor_plan.md` — current `crates/semstrait-ir/src/plan` vs target layout delta is tracked here)
---

# 35. semstrait-ir

> **Status:** ratified. `35` is the crate-of-record for the post-compile IR vocabulary: `Expr<L>` and its leaf sets, traversal traits, support enums, identifier carriers, `Parameter`, the `expr_fn` DSL, `CanonicalFn` / `FunctionRegistry`, the narrow ir-emitted error kinds (`ValidateError` / `CompileError`), `SemanticPlan` + `PlanNode`, and the adapter-consumable output family. Every variant catalog and structural invariant exposed here is ratified upstream in `14`, `14a`, or `16`; `35` adds no new rosters — it implements the shapes those chapters own.

## 1. Purpose and Scope

`semstrait-ir` is the **canonical IR crate**. It carries every type the post-compile pipeline operates on: the engine-agnostic expression types (`Expr<L>` + both leaf sets), the function-identity catalog (`CanonicalFn` + `FunctionRegistry`), the in-memory plan tree (`SemanticPlan`), and the adapter-consumable output types. The producer side is split — `semstrait-model` parses YAML into the expression types, `semstrait-manifest::compile` resolves `SemanticExpr` into `PhysicalExpr` per `[19 §3](../foundations/19_expression_flow.md)`, and `semstrait-planner` (`34`) builds `SemanticPlan` from `Request × SemanticManifest`. The consumer side is the adapter family (`semstrait-adapter`, `36`+). No other crate in the workspace contains a plan-tree, expression-type, or function-registry vocabulary.

### 1.1 What `semstrait-ir` OWNS

| Surface | Section | Variant catalog ratified by |
|---|---|---|
| Canonical type vocabulary — `DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn` | §4 | `[13](../foundations/13_types_and_grain.md)` |
| Universal-traversal trait family — `Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf` | §3.2 | `[14 §3.1](../foundations/14_expressions.md)` / `[§3.2](../foundations/14_expressions.md)` |
| `Expr<L>` structural enum + `PhysicalLeaf` / `SemanticLeaf` + `PhysicalExpr` / `SemanticExpr` aliases + per-kind accessor enums + `Parameter` + `ParameterKey` | §3–§6 | `[14 §3](../foundations/14_expressions.md)` / `[§4](../foundations/14_expressions.md)` |
| Structural-variant support enums — `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound` | §3.4 | `[14 §3.3](../foundations/14_expressions.md)` |
| Typed-literal carrier `Literal` | §3.4 | `[14 §3.3](../foundations/14_expressions.md)` |
| Shared identifier carriers `ColumnRef`, `SemanticsName` | §3.4 | `[14 §3.4 / §3.5](../foundations/14_expressions.md)` |
| Authoring-surface DSL — `expr_fn` module, `std::ops` impls on `Expr<L>`, `ExprFunctionExt` | §7 | `[14 §9.2](../foundations/14_expressions.md)` |
| `CanonicalFn` newtype + `FunctionRegistry` / `FunctionSpec` / `FnSignature` / `ParamType` / `ReturnTypeRule` / `FunctionCategory` / `RegistryExtension` / `function_registry()` | §8 | `[14a §2](../foundations/14a_function_catalog.md)` |
| Narrow ir-emitted error kinds — `ValidateError`, `CompileError` | §16 | `[14 §3.1](../foundations/14_expressions.md)` / `[14a §3.5](../foundations/14a_function_catalog.md)` |
| `SemanticPlan` root + `PlanNode` sum (8 variants) + well-formedness invariants | §9 / §10 / §13 | `[00 §4.1](../00_overview.md)` |
| Plan-level primitives — `SourceRef`, `ResolvedColumn`, `Name`, `KeyPair`, `SortDir`, `NullOrdering`, `AggregateExpr`, `NodeMeta` | §11 | `[15](../foundations/15_mapping_and_binding.md)` / `[16 §5](../foundations/16_composition_and_relationships.md)` |
| Adapter-consumable output family — `EngineArtifact`, `EnginePlan`, `SqlArtifact`, `DialectId`, `Dialect`, `Capability` | §12 | `[00 §4.1](../00_overview.md)` (shape only; emission in `[36](36_semstrait_adapter.md)`) |
| Visitor / traversal API over `PlanNode` trees | §14 | unified with `Tree` (§3.2) |
| Serde derivations for `SemanticPlan`, `Expr<L>`, and every public IR type | §15 | — |
| `IrErrorKind` typed-kind enum + `Diagnose` impl | §16 | — |
| Substrait-mapping table (declarative `PlanNode` ↔ `substrait::proto::Rel`); conversion code lives in `[36](36_semstrait_adapter.md)` | §15.2 | — |

### 1.2 What `semstrait-ir` does NOT own

- **Planning strategy and per-DataKind plan assembly.** Every decision that "this `Request` against this `SemanticManifest` expands into a tree of `PlanNode`s in this order" lives in `semstrait-planner` per `34`. `35` ratifies only the **output shape** that planning must produce.
- **Optimization passes.** Rule-based rewrites over `SemanticPlan` live in `semstrait-planner` (`34`, stage 5 per `10 §3.5`). `35`'s `walk` / `transform` helpers (§14) are the substrate those rewrites run on, not the rewrites themselves.
- **Adapter emission.** Translating a `SemanticPlan` into an `EngineArtifact` (SQL text or Substrait proto) is `36`'s contract. `35` ratifies the artifact's structural shape and the Substrait mapping table; the rendering code, dialect-specific SQL, and capability checks all live above.
- **SemanticManifest shape.** `SemanticPlan` refers to the SemanticManifest through the opaque `SourceRef` handle (§11.2). SemanticManifest types live in `semstrait-manifest` per `33`; `35` never embeds them inline.
- **Expression-type variant catalogs and structural invariants.** While `semstrait-ir` now OWNS the *crate-level* placement of the expression types per §1.1, the **variant rosters** for each enum, the **structural invariants** between leaf sets, the **type aliases** discipline, and the **accessor catalogs** are ratified by `[14 §3](../foundations/14_expressions.md)` and `[14 §4](../foundations/14_expressions.md)`. `35`'s §3–§6 reference those rosters rather than re-ratifying them; per `[DOCS_MAINTENANCE.md §3](../DOCS_MAINTENANCE.md)`, the variant catalogs and structural rules live in `14` alone.
- **Compile-time `SemanticExpr` → `PhysicalExpr` resolution.** The algorithm that lowers `SemanticExpr` into `PhysicalExpr` (`SemanticExpr::resolve`, `ResolvedExprTable` keying, cross-DataKind path resolution, sugar-accessor elimination, type inference, Semantics-boundary reconciliation) lives in `semstrait-manifest::compile` per `[19 §3](../foundations/19_expression_flow.md)`. `35` owns the types that flow through that algorithm; it does not own the algorithm.
- **Phase B placement and `Parameter` binding.** The `Strategy`-driven plan-tree construction (filter splitting, `Aggregate` lift into `PlanNode::Agg`, `Parameter` binding against the `Request`, advisory channel) lives in `semstrait-planner` per `[19 §6](../foundations/19_expression_flow.md)` and `[34](34_semstrait_planner.md)`. `35` owns the `PlanNode` and `PhysicalExpr` types that the planner produces; the planning algorithm itself is `34`'s contract.
- **Canonical-function semantics and per-engine mapping.** What `coalesce` does to nulls, which engines support `regexp_match` natively, how `add` maps to DataFusion's `Add` operator — `[14a](../foundations/14a_function_catalog.md)` and `registry/functions_mapping.md` are authoritative. `35` owns the registry's *shape*, not its *contents*.
- **Engine / dialect identity outside the artifact family.** `DialectId` MUST appear only on `SqlArtifact.dialect` (§12.3), `Dialect::ID` (§12.5), and `Capability` membership where capability is dialect-keyed (§12.6). It MUST NOT appear on `SemanticPlan`, `SemanticPlan.meta`, any `PlanNode` variant, any `Expr<L>` variant, any leaf-set variant, `NodeMeta`, or any registry-side type. Per S7 (§1.5) and Q4.A (2026-05-21).

### 1.3 Design posture — pure, sync, canonical

`semstrait-ir` is deliberately **pure** (no I/O, no async, no engine identity). It is the data-only substrate every post-compile stage consumes:

- **Zero I/O surface.** Concrete I11 guarantee (per `30 §9`).
- **Zero async.** Every method on every public type is synchronous; `SemanticPlan` is built, walked, rewritten, and serialized on the caller's thread. `Expr<L>` traversal is synchronous. I6 guarantee.
- **Zero engine identity.** No `datafusion::*`, no `arrow::*`, no `spark::*`, no `duckdb::*` types are visible on any `semstrait-ir` public surface. `DialectId` is an opaque newtype; `substrait::proto::Plan` is the one exception and appears only inside `EnginePlan::Substrait(_)` (§12.2) as the adapter-consumable payload.
- **Single upstream dependency.** `semstrait-ir` depends on `semstrait-common` only — for diagnostic primitives (`Diagnostic<K>`, `Diagnose`, `Severity`, `Location`, `Span`, `SourceId`) and the constraint-DSL toolkit. Every other workspace crate depends on `semstrait-ir` directly or transitively. `Cargo.toml` audit per §18.2 enforces this.

### 1.4 Engine-IR concept inspiration

`PlanNode` borrows its **catalog of operators** and **tree composition** from engine IRs — DataFusion's `LogicalPlan`, Calcite's `RelNode`, Substrait's `Rel`. These are the shapes a planner naturally produces; re-inventing them would be a waste. Per `00 §3`, the inspiration is **structural only**:

- Borrowed: the set of operators (`Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch`), the box-per-child tree shape, the invariant that every non-leaf carries typed inputs.
- Rejected: cost / statistics fields on any `PlanNode` (cost lives in the engine, not the canonical plan); physical / distribution properties (`Partitioning`, `Exchange`, `Shuffle`, `Repartition`); dialect or adapter branching on node variants; vendor-specific rel kinds.

`Expr<L>`'s structural variants borrow the same way — the operator catalog from `[14 §3.3](../foundations/14_expressions.md)` mirrors the union of variants that engine ASTs naturally surface (arithmetic, comparison, `Case`, `Cast`, `FunctionCall`, `Aggregate`, `Window`) without admitting engine-specific operators (those land as `FunctionCall` entries via the registry per `[14a §7](../foundations/14a_function_catalog.md)`).

### 1.5 Eight ratified statements

**S1 — IR provides the canonical type vocabulary.** `35` carries the canonical shapes; the verbs *build* (model) and *plan* (planner) live in their owning crates.

**S2 — IR provides typed `PlanNode` implementations, Substrait-shaped.** `PlanNode { Scan, Filter, Project, Agg, Join, Union, Sort, Fetch }` is a closed sum mirroring Substrait's `Rel` family with name-based (`Name`, §11.4) column references.

**S3 — IR operates on canonical forms only.** Engine identity, dialect identity, SQL text, raw paths, and vendor types are forbidden on every public surface except the artifact family (§12); this makes `[00 §9](../00_overview.md)` `I1` / `I2` / `I3` an IR-internal contract.

**S4 — Substrait is the main inspiration AND a co-equal output target — not the canonical wire form.** Canonical wire form is the in-process Rust types; Serde and Substrait are derived surfaces.

**S5 — Plan nodes carry descriptive annotations for traceability.** Annotations are non-computational metadata, classified TRACE/PLAN at §11.1.1, with the Substrait carrier at §15.3.

**S6 — IR holds the surface listed in §1.1; `semstrait-common` holds diagnostic primitives + constraint-DSL + io transport per `[31 §1.1](31_semstrait_common.md)`.**

**S7 — IR holds zero engine information; adapters do.** `DialectId` is the only engine-identity vocabulary in `35`, and it appears only on `SqlArtifact.dialect` and `Dialect::ID`.

**S8 — IR is shape and traversal; computation lives in producers.** `35` provides `Tree::transform` and the typed shapes only; lowering and constant-folding per `[19](../foundations/19_expression_flow.md)` are producer-side rewrites.

### 1.6 Cross-cutting rules

Rules that apply across all of `35`'s content. Violations are spec defects.

**R1 — Canonical-only.** Every public surface of `35` carries canonical vocabulary only. Engine identity, dialect identity, SQL text, raw paths, and vendor types are forbidden on every public surface except the artifact family (§12) and `Dialect::ID`. Per S3.

**R2 — Naming readability.** Every public name (modules, types, traits, variants, fields, methods) MUST be guessable in isolation by a fresh reader. "Kind" is a modifier suffix on categorizing enums (`DataKind`, `LikeKind`, `IrErrorKind`); never a standalone noun. References are named after the referenced thing (`DataKindRef`, `SourceRef`), never as bare `KindRef`. Renaming a public name is MAJOR per `[30 §2.1](30_api_contracts.md)`.

**R3 — Producer / consumer roles.** The planner (`34`) is the producer of `SemanticPlan` values. Adapters (`36` and family) are the consumers. `35` defines the shape; `34` writes it; `36` reads it.

**R4 — Closed catalog, registry-mediated extension.** `PlanNode` variants, `Expr<L>` variants, leaf-set variants are `#[non_exhaustive]` for additive growth, but every variant lives in this spec. Engine-specific or strategy-specific extensions land as `FunctionRegistry` entries per `[14a §7](../foundations/14a_function_catalog.md)`, never as new `PlanNode` or `Expr<L>` variants. Optimizer passes rewrite over the closed catalog.

## 2. Module Layout

Top-level `pub mod` structure. One module per cohesive concept.

```
semstrait-ir
├── types                // DataType, Grain, TypeClass, Schema, SchemaColumn (13)
├── tree                 // Tree trait + Visitor<N> / Rewriter<N> / ExprLeaf companions (14 §3.1 / §3.2)
├── expr_kinds           // Structural-variant support enums: BinaryOpKind, UnaryOpKind,
│                        //   AggregationOp, LikeKind, CastFailure, WindowFn, WindowFrame,
│                        //   WindowFrameKind, WindowBound, Literal (14 §3.3); plus the
│                        //   shared identifier carriers ColumnRef, SemanticsName
├── expr                 // Expr<L>, leaf sets, type aliases, accessor enums, Parameter, DSL
│   ├── tree             //   Expr<L> structural enum (variant catalog owned by 14 §3.3)
│   ├── leaves           //   PhysicalLeaf, SemanticLeaf, PhysicalExpr, SemanticExpr (14 §3.4–§3.6)
│   ├── accessor         //   DimensionAccessor, MeasureAccessor, MetricAccessor, KeyAccessor (14 §4.1)
│   ├── parameter        //   Parameter struct, ParameterKey closed enum (14 §5)
│   └── expr_fn          //   col, field, dim, measure, metric, key free constructors;
│                        //     std::ops impls; ExprFunctionExt trait (14 §9.2)
├── functions            // CanonicalFn, FunctionRegistry, FunctionSpec, FnSignature,
│                        //   ParamType, ReturnTypeRule, FunctionCategory,
│                        //   RegistryExtension trait, function_registry() accessor (14a §2 / §3 / §8)
├── plan                 // SemanticPlan, PlanNode, per-variant structs, NodeMeta
│   ├── node             //   PlanNode enum + variant-struct shapes
│   └── traversal        //   PlanVisitor, walk_pre / walk_post / transform
├── primitives           // SourceRef, ResolvedColumn, Name, KeyPair, SortDir,
│                        //   NullOrdering, AggregateExpr
├── artifact             // EngineArtifact, EnginePlan, SqlArtifact, DialectId, Dialect
├── error                // ValidateError (raised by Tree::with_new_children + Rewriter<N>::f_*);
│                        //   CompileError (raised by ReturnTypeRule::Custom callbacks);
│                        //   IrErrorKind (plan-shape diagnostics)
└── substrait_map        // Substrait mapping TABLE only (no conversion code)
```

**Split rationale:**

| Module | Reason for separate top-level home |
|---|---|
| `types` | Universal reference: every other module here uses `DataType` / `Schema`. Flat-top placement matches `[13](../foundations/13_types_and_grain.md)`. |
| `tree` | Trait surface independent of the support-enum roster; `Tree` works for `PlanNode` as well as `Expr<L>`. |
| `expr_kinds` | Support enums isolated from traits to limit I10 blast radius when a variant lands. |
| `expr` | Referenced by `plan` but not vice-versa; consumers needing only expressions skip the plan-tree surface. |
| `expr::{tree, leaves, accessor, parameter, expr_fn}` | Per-type-family split for `cargo public-api` audit clarity; `expr_fn` isolates DSL from structural types. |
| `functions` | Adjacent to `expr` because every `FunctionCall` consumer resolves `name` against the registry. |
| `plan` vs `primitives` | `PlanNode` references every primitive but not vice-versa; primitives can be consumed without the full `PlanNode` surface. |
| `plan::node` vs `plan::traversal` | Traversal-API method count scales with variant count; isolating it limits I10 blast radius. |
| `artifact` | Output shape, decoupled from input shape; consumed by the engine layer above `semstrait-adapter`. |
| `error` | Co-locates `ValidateError`, `CompileError`, `IrErrorKind`. Distinct from manifest's wider `CompileError` (per `33 §10`), which embeds via D.ii. |
| `substrait_map` | Table reference only; conversion code lives in `[36](36_semstrait_adapter.md)`. |

**Re-exports.** The crate root re-exports a curated surface (§20). Non-root re-exports of internal helpers are forbidden.

## 3. `Expr<L>` Structural Type — Owned Here, Specified by `14`

### 3.1 Where the type architecture lives

The structural shape of canonical-IR expressions is ratified by `[14 §3](../foundations/14_expressions.md)`:

- `[14 §3.1](../foundations/14_expressions.md)` ratifies the universal `Tree` trait and its `Visitor` / `Rewriter` companions.
- `[14 §3.2](../foundations/14_expressions.md)` ratifies the `ExprLeaf` trait.
- `[14 §3.3](../foundations/14_expressions.md)` ratifies the structural-variant catalog of `Expr<L>` — every variant (`BinaryOp`, `UnaryOp`, `FunctionCall`, `Cast`, `Case`, `InList`, `Between`, `Like`, `IsNull`, `Coalesce`, `NullIf`, `Aggregate`, `Window`) and the `Leaf(L)` wrapper.

`35` is the **crate** that holds the implementation of `Expr<L>`. Per `[14 §9.2](../foundations/14_expressions.md)`, this ownership moved from `semstrait-common` to `semstrait-ir` at the `14` second-refinement landing. `35` does not re-ratify the variant catalog; the catalog is `[14 §3.3](../foundations/14_expressions.md)`'s contract and any change to it lands in `14` first, then cascades here.

### 3.2 The `Tree` / `Visitor` / `Rewriter` / `ExprLeaf` trait surface — owned here

Per `[14 §3.1](../foundations/14_expressions.md)` / `[§3.2](../foundations/14_expressions.md)` and the second-cascade placement in `[14 §9.2](../foundations/14_expressions.md)`, the universal-traversal trait family is owned by `semstrait-ir`. Both `Expr<L>` (§3.3) and `PlanNode` (§10) implement these traits; the natural home is alongside their producers, not upstream in core.

```rust
/// Universal traversal contract. Implemented by Expr<L> (§3.3) and PlanNode (§10).
/// Stage-agnostic. Per `14 §3.1`.
pub trait Tree: Sized {
    fn children(&self) -> Vec<&Self>;
    fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateError>;
}

impl<T: Tree> T {
    pub fn apply<V: Visitor<Self>>(&self, v: &mut V) -> V::Output { /* default body */ }
    pub fn transform<F>(self, f: F) -> Result<Self, ValidateError>
    where F: FnMut(Self) -> Result<Self, ValidateError> { /* default body */ }
}

/// Per `14 §3.1`.
pub trait Visitor<N> {
    type Output;
    fn f_down(&mut self, node: &N) -> ControlFlow<Self::Output>;
    fn f_up(&mut self,   node: &N) -> ControlFlow<Self::Output>;
}

/// Per `14 §3.1`.
pub trait Rewriter<N> {
    fn f_down(&mut self, node: N) -> Result<N, ValidateError>;
    fn f_up(&mut self,   node: N) -> Result<N, ValidateError>;
}

/// Per-leaf-set metadata contract. Implemented by PhysicalLeaf and SemanticLeaf (§5).
/// Per `14 §3.2`.
pub trait ExprLeaf: Sized + Clone + Debug {
    fn inferred_type(&self) -> Option<&DataType>;
}
```

Consumers write `use semstrait_ir::Tree` (no cross-crate hop to core). The single trait surface lets one generic algorithm operate on both expression trees and plan trees — e.g. the optimizer applies the same `transform` helper to predicates inside `FilterNode` and to subtrees rooted at `FilterNode` itself.

`ValidateError` is owned by `semstrait-ir` (§16) since it is raised entirely by `Tree::with_new_children` and the `Rewriter<N>::f_*` callbacks defined here.

### 3.3 The `Expr<L>` definition

The full variant catalog is per `[14 §3.3](../foundations/14_expressions.md)`. `35`'s exposed surface:

```rust
/// Canonical structural expression tree, parameterized over a leaf set `L`.
/// Variant catalog per `14 §3.3`. `#[non_exhaustive]` per I10.
///
/// Instantiated by the type aliases `PhysicalExpr` (with `PhysicalLeaf`)
/// and `SemanticExpr` (with `SemanticLeaf`) per §5.3.
#[non_exhaustive]
pub enum Expr<L: ExprLeaf> {
    Leaf(L),

    BinaryOp     { op: BinaryOpKind, left: Box<Self>, right: Box<Self> },
    UnaryOp      { op: UnaryOpKind,  operand: Box<Self> },
    FunctionCall { name: CanonicalFn, args: Vec<Self> },
    Cast         { input: Box<Self>, target: DataType, on_failure: CastFailure },
    Case         { whens: Vec<(Self, Self)>, else_: Option<Box<Self>> },
    InList       { value: Box<Self>, list: Vec<Self>, negated: bool },
    Between      { value: Box<Self>, low: Box<Self>, high: Box<Self>, negated: bool },
    Like         { value: Box<Self>, pattern: Box<Self>, kind: LikeKind },
    IsNull(Box<Self>),
    Coalesce(Vec<Self>),
    NullIf       { left: Box<Self>, right: Box<Self> },
    Aggregate    { op: AggregationOp, args: Vec<Self>, distinct: bool, filter: Option<Box<Self>> },
    Window       { function: WindowFn, args: Vec<Self>, partition_by: Vec<Self>, order_by: Vec<Self>, frame: Option<WindowFrame> },
}

impl<L: ExprLeaf> Tree for Expr<L> {
    fn children(&self) -> Vec<&Self> { /* per-variant */ }
    fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateError> { /* per-variant */ }
}
```

Per `[14 §3.3](../foundations/14_expressions.md)`'s notes:

- `Window` is **compile-emitted only** — author-facing parsers do not accept window syntax; `Window` nodes enter the tree exclusively through sugar-accessor elimination during compile (§6.1, `[14 §4.2](../foundations/14_expressions.md)`).
- Engine-specific operators do not add `Expr<L>` variants. They land as `FunctionCall` entries via `FunctionRegistry` extensions per `[14a §7](../foundations/14a_function_catalog.md)`.
- `Aggregate`'s `filter` carries the canonical `agg(expr) FILTER (WHERE p)` shape; adapter compensation for engines without native `FILTER` is the adapter's concern (`36`), not part of the canonical IR.

### 3.4 Structural-variant support enums + identifier carriers — owned here

`Expr<L>`'s structural variants reference a small set of support enums and identifier carriers, co-located with `Expr<L>`. Rosters per `[14 §3.3](../foundations/14_expressions.md)`.

```rust
// Operator discriminators carried by Expr<L>::BinaryOp / UnaryOp.
#[non_exhaustive]
pub enum BinaryOpKind {
    Add, Subtract, Multiply, Divide, SafeDivide, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

#[non_exhaustive]
pub enum UnaryOpKind { Negate, Not }

// Aggregation tag carried by Expr<L>::Aggregate. CountDistinct is encoded as
// Aggregate { op: Count, distinct: true, ... }.
#[non_exhaustive]
pub enum AggregationOp { Sum, Avg, Count, Min, Max }

// Like operator variant — case-sensitivity and negation profile.
#[non_exhaustive]
pub enum LikeKind { Like, NotLike, ILike, NotILike }

// Cast failure-mode discriminator. Adapters MAY emit different SQL forms per variant.
#[non_exhaustive]
pub enum CastFailure {
    /// Raise an engine-level error on cast failure.
    Error,
    /// Return NULL on cast failure (TRY_CAST semantics).
    Null,
}

// Window function identity + frame spec, carried by Expr<L>::Window.
// Window nodes are compile-emitted only via sugar-accessor elimination (14 §4.2).
#[non_exhaustive]
pub enum WindowFn { Lag, Lead, FirstValue, LastValue, RowNumber, Rank, DenseRank }

#[non_exhaustive]
pub struct WindowFrame { pub kind: WindowFrameKind, pub start: WindowBound, pub end: WindowBound }

#[non_exhaustive]
pub enum WindowFrameKind { Rows, Range, Groups }

#[non_exhaustive]
pub enum WindowBound {
    UnboundedPreceding, Preceding(u64), CurrentRow, Following(u64), UnboundedFollowing,
}

// Typed literal value — single carrier shared by PhysicalLeaf::Literal and
// SemanticLeaf::Literal. Variant list aligns 1:1 with DataType (13) plus Null.
#[non_exhaustive]
pub enum Literal {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Decimal  { value: String, precision: u8, scale: i8 },
    String(String),
    Date(String),
    Time     { value: String, precision: u8 },
    Timestamp{ value: String, precision: u8 },
    Interval(String),
    Binary(Vec<u8>),
    Null,
}

// Shared identifier carriers. Newtype-over-stable per 30 §4.3
// (no #[non_exhaustive]; .0 access is intentional).
pub struct ColumnRef(pub String);
pub struct SemanticsName(pub String);
```

`DataType` / `Grain` / `TypeClass` / `Schema` / `SchemaColumn` are owned in this crate at §4 (the canonical type vocabulary). `DataType` flows through both the structural variants (`Cast::target: DataType`) and the support enums (`Literal::Decimal { precision, scale }` aligns with `DataType::Decimal`); keeping them at the crate root means `use semstrait_ir::*` is sufficient for almost every consumer.

**Why `Vec<String>` matters here:** `Literal` parses bare YAML scalars into typed shape at parse time per `[32 §...](../apis/32_semstrait_model.md)`; `semstrait-ir` validates `Decimal { precision, scale }` range at construction.

## 4. Canonical Type Vocabulary

The canonical logical-type primitives — `DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn` — live in `semstrait-ir`. They are referenced by every layer of the post-compile pipeline (expression leaves, function signatures, plan-tree primitives, adapter renderers) and have no consumer outside that pipeline; placing them here removes the cross-crate hop downstream consumers previously paid. Variant rosters and their structural rules are ratified by `[13](../foundations/13_types_and_grain.md)`; `35` is the crate-level home.

### 4.1 `DataType` — canonical logical types

```rust
/// Canonical logical data types per `13 §2.1`. 14 scalar variants,
/// engine-neutral names. Complex types (arrays, structs, maps) are
/// out of scope for v1 per `13 §2.5`.
#[non_exhaustive]
pub enum DataType {
    Boolean,
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
    Decimal   { precision: u8, scale: i8 },
    String,
    Binary,
    Date,
    Time      { precision: u8 },
    Timestamp { precision: u8 },
    Interval,
}
```

Construction of `Decimal { precision, scale }` with out-of-range values is rejected at `semstrait-model` parse time (`ParseErrorKind::InvalidDecimalParameters`); `semstrait-ir` performs no validation at the constructor. `DataType` is referenced by every consumer above — expression leaves and `Cast { target }` (§3.4 / §6), function signatures (`ParamType::Exact` at §8.2), plan-tree primitives (`ResolvedColumn` at §11.3), and adapter renderers (§12).

### 4.2 `Grain` — temporal granularity lattice

```rust
/// Temporal Grain levels per `13 §3.1`. Total coarseness order per
/// `13 §3.2`; exposed via `Grain::coarseness() -> u8`.
#[non_exhaustive]
pub enum Grain {
    Minute, Hour, Day, Week, Month, Quarter, Year,
}

impl Grain {
    /// Selection-rank order: Minute(0) < ... < Year(6). Per `13 §3.2`.
    pub fn coarseness(self) -> u8;
}
```

### 4.3 `TypeClass` — type-class grouping (per `13 §4`)

```rust
/// Bounded type classes used by `FnSignature` parameter expression per
/// `14a §3.3`. Pure grouping vocabulary; no method surface. Per `13 §4`.
#[non_exhaustive]
pub enum TypeClass {
    Numeric,      // Byte | Short | Integer | Long | Float | Double | Decimal
    Integral,     // Byte | Short | Integer | Long
    FloatingPt,   // Float | Double
    Textual,      // String
    Temporal,     // Date | Time | Timestamp
    Comparable,   // everything except Binary in the canonical set
    Any,          // all canonical variants
}
```

`TypeClass` is exposed but **not** wired into the v1 `ParamType` activation — `[14a §3.3](../foundations/14a_function_catalog.md)` Q6 ratified overload-set polymorphism, not type-class generics, for v1. `TypeClass` exists as vocabulary for future registry evolution (`[TD-REGISTRY-TYPECLASS]`, `[14a §10.1](../foundations/14a_function_catalog.md)`) and for documentation / advisory diagnostics. The reserved `ParamType::TypeClass(TypeClass)` variant lives at §8.2.

### 4.4 `Schema` and `SchemaColumn` — physical-source schema

```rust
/// The compile-time snapshot of physical columns exposed by a source.
/// Per `15 §3.2`. Referenced by:
/// - `15 §3.1 PhysicalSource::{File, Stream, Table, Snapshot}` for the
///   resolved schema attached to every `PhysicalSource` variant.
/// - §11.1 `NodeMeta.output_schema` for the per-`PlanNode` output
///   schema (via plan-level `Schema`, which has the same shape).
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

pub struct SchemaColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}
```

`Schema.columns` is ordered in the source's native order (Parquet footer, Iceberg field order, CSV header order, …) per `[15 §3.2](../foundations/15_mapping_and_binding.md)`. Compile preserves the ordering for determinism (I4); the planner does not semantically depend on it. `nullable` is read from source metadata when available; it is not inferred. Both types are **field-stable** (no `#[non_exhaustive]`) per shared-vocabulary policy.

### 4.5 Nullability

Nullability is **NOT** exposed as a separate enum. Per `[13 §2](../foundations/13_types_and_grain.md)` and `[14a §3.4](../foundations/14a_function_catalog.md)` Q7, `DataType` is nullable-by-default; per-column nullability lives as the `SchemaColumn.nullable: bool` field. A standalone `Nullability` type was considered and rejected at `[14a §3.4](../foundations/14a_function_catalog.md)`.

### 4.6 Serialization feature flag

```rust
#[cfg(feature = "serde")]
impl serde::Serialize for DataType { /* ... */ }
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for DataType { /* ... */ }
// ... similarly for Grain, TypeClass, Schema, SchemaColumn.
```

Feature-gated per §15.1. Off by default; `semstrait-model` / `semstrait-manifest` / `semstrait-planner` enable it transitively (they require YAML / JSON round-tripping of values carried through these primitives).

## 5. Leaf Sets — `PhysicalLeaf` and `SemanticLeaf`

### 5.1 `PhysicalLeaf`

Canonical-IR leaf set per `[14 §3.4](../foundations/14_expressions.md)`. The full variant catalog and per-variant invariants are owned by `14`; `35` carries the implementation.

```rust
/// Canonical-IR leaf set. Carries exactly what the planner and adapters
/// need. Variant catalog per `14 §3.4`.
#[non_exhaustive]
pub enum PhysicalLeaf {
    /// Physical column reference (binding-resolved).
    Column(ColumnRef),

    /// Typed literal value.
    Literal(Literal),

    /// Compile-emitted, plan-bound parameter placeholder. Replaced with a
    /// concrete value during Phase B planning per `14 §5.3` / `19 §2.1`.
    Parameter(Parameter),
}

impl ExprLeaf for PhysicalLeaf { /* per-variant inferred_type */ }
```

Invariants on `PhysicalExpr` (per `[14 §3.4](../foundations/14_expressions.md)`):

- No `Field` / `Dimension` / `Measure` / `Metric` / `Key` — semantic references are eliminated during compile per `[19 §3](../foundations/19_expression_flow.md)`.
- No sugar accessors — typed-leaf-with-accessor leaves are eliminated during compile (lowered to `Window`-rooted subtrees per `[14 §4.2](../foundations/14_expressions.md)`).
- `Parameter` leaves are the only non-resolved state the canonical IR carries; they MUST be substituted before adapt time (`[14 §5.3](../foundations/14_expressions.md)` postcondition).

### 5.2 `SemanticLeaf`

Per-kind typed leaf set per `[14 §3.5](../foundations/14_expressions.md)`. Each variant tag encodes the entity kind; the optional `accessor` field carries per-kind sugar (§6.1). The full catalog and per-variant invariants are owned by `14`.

```rust
/// Authoring-form leaf set. Per `14 §3.5`. Compile substitutes per the
/// algorithm in `19 §3`.
#[non_exhaustive]
pub enum SemanticLeaf {
    /// Typed literal value.
    Literal(Literal),

    /// Physical column reference. Type-admissible inside `SemanticExpr`;
    /// LEGAL only under `semantic_mapping: auto` per `14 §8`'s
    /// compile-time rejection rule.
    Column(ColumnRef),

    /// Untyped semantic reference. Kind resolved at compile by registry
    /// lookup.
    Field(SemanticsName),

    /// Typed Dimension reference, optionally with sugar accessor.
    Dimension { name: SemanticsName, accessor: Option<DimensionAccessor> },

    /// Typed Measure reference, optionally with sugar accessor.
    Measure { name: SemanticsName, accessor: Option<MeasureAccessor> },

    /// Typed Metric reference, optionally with sugar accessor.
    Metric { name: SemanticsName, accessor: Option<MetricAccessor> },

    /// Typed Key reference, optionally with sugar accessor.
    Key { name: SemanticsName, accessor: Option<KeyAccessor> },
}

impl ExprLeaf for SemanticLeaf { /* per-variant inferred_type — None for unresolved Field */ }
```

Notable properties per `[14 §3.5](../foundations/14_expressions.md)`:

- **No `EntityRef` wrapper, no `Access` wrapper, no outer `Accessor` enum.** Every semantic reference is a typed leaf whose variant tag already encodes the entity kind. The per-kind accessor enums (§6.1) sit as `Option<…>` fields on the typed leaves. This shape replaces the earlier-draft `EntityRef` / `Access` / wrapping `Accessor` design at the `14` second-refinement landing.
- **`Field` is the untyped fallback.** When the author writes a bare identifier (at a semantic site) or explicit `field(name)`, the leaf carries no kind hint; compile resolves the kind by registry lookup.
- **`Dimension` / `Measure` / `Metric` / `Key` are kind-pinned.** Compile fails fast if the registered semantic at `name` has a different kind than the authored leaf variant — `manifest::CompileError::SemanticKindMismatch` per `[14 §8](../foundations/14_expressions.md)`.
- **`Column` is conditionally legal.** Type-admissible (the parser can construct it), but compile rejects it under manual mapping per `[14 §8](../foundations/14_expressions.md)`. Under `semantic_mapping: auto`, compile synthesizes `SemanticMapping` entries for `Column` leaves and the rest of resolution proceeds as with manual mapping.
- **No `Parameter`.** Parameters are exclusively compile-emitted and live only in `PhysicalLeaf`.

### 5.3 Type aliases

```rust
pub type PhysicalExpr = Expr<PhysicalLeaf>;
pub type SemanticExpr = Expr<SemanticLeaf>;
```

These are the spelled-out names used throughout downstream docs (`19`, `33`, `34`) and downstream-crate APIs. The generic `Expr<L>` form appears in trait bounds and shared algorithmic code (e.g., the optimizer's tree-walks).

### 5.4 Type-enforced forbidden combinations

Per `[14 §3.7](../foundations/14_expressions.md)`, the leaf-set boundary makes several invariants type-level:

- `PhysicalExpr` cannot contain `Field` / `Dimension` / `Measure` / `Metric` / `Key` — those variants do not exist in `PhysicalLeaf`. The static type system, not a runtime check, upholds this.
- `SemanticExpr` cannot contain `Parameter` — `Parameter` is `PhysicalLeaf`-only.
- A `Dimension`-tagged leaf cannot carry a `MeasureAccessor` (or any non-Dimension accessor) — the variant signature `Dimension { name, accessor: Option<DimensionAccessor> }` enforces kind agreement at construction.

There is no `try_into_physical` runtime check, no defensive `panic!` for "Field found in PhysicalExpr". `SemanticLeaf::Column` is type-admissible but context-validated by compile per §5.2's manual-vs-auto rule.

## 6. Per-Kind Accessor Enums and `Parameter`

### 6.1 Per-kind accessor enums

Per-entity sugar lets authors write shorthand like `measure("revenue").previous()` or `metric("conv_rate").delta()`. The mechanism is a kind-specific accessor enum carried as an `Option<…>` field on each typed semantic leaf (§5.2). The four enums and their variant catalogs are ratified in `[14 §4.1](../foundations/14_expressions.md)`. `35` carries the implementation:

```rust
#[non_exhaustive]
pub enum DimensionAccessor { First, Last, Lag(u32), Lead(u32) }

#[non_exhaustive]
pub enum MeasureAccessor   { Previous, Next, Lag(u32), Lead(u32), Delta, PercentChange }

#[non_exhaustive]
pub enum MetricAccessor    { Previous, Next, Lag(u32), Lead(u32), Delta, PercentChange }

#[non_exhaustive]
pub enum KeyAccessor       { First, Last, Lag(u32), Lead(u32) }
```

Two structural pairings emerge per `[14 §4.1](../foundations/14_expressions.md)`:

- `MetricAccessor` mirrors `MeasureAccessor` 1:1 — a Metric is a per-group already-aggregated value at access time, structurally identical to a Measure at the output projection stage.
- `KeyAccessor` mirrors `DimensionAccessor` 1:1 — a Key is a Dimension-shaped entity for sugar purposes; the windowed accessor surface is symmetric.

**No outer `Accessor` wrapping enum, no `EntityRef`, no `Access` structural variant.** Per `[14 §4.1](../foundations/14_expressions.md)`'s second refinement, kind agreement is type-enforced at construction by carrying each accessor enum directly on the matching typed leaf. A `SemanticLeaf::Dimension { accessor: Option<DimensionAccessor> }` simply has no way to hold a `MeasureAccessor`.

The `Field` leaf carries no accessor — it is the untyped semantic reference whose kind is resolved at compile. To apply sugar, authors use the typed accessor for the matching kind (`measure("x").delta()`, not `field("x").delta()`).

### 6.2 `Parameter` and `ParameterKey`

Compile-emitted, plan-bound placeholder per `[14 §5](../foundations/14_expressions.md)`:

```rust
/// Plan-bound parameter placeholder. Substituted by the planner during
/// Phase B per `19 §2.1`. Per `14 §5.1`.
pub struct Parameter {
    pub key: ParameterKey,
    pub data_type: DataType,
}

/// Closed set of typed parameter keys. Internal to the canonical pipeline —
/// not author-extensible. v1 carries the two keys needed by sugar-accessor
/// elimination (`14 §4.2`); future keys land additively per I10.
#[non_exhaustive]
pub enum ParameterKey {
    RequestDimensionsMinusTemporal,
    RequestTemporalAxis,
}
```

`Parameter` lives only in `PhysicalLeaf` (§5.1); `SemanticLeaf` carries no `Parameter`. The plan-time binding postcondition is per `[14 §5.3](../foundations/14_expressions.md)`: no `Parameter` survives into adapt time. Phase B substitution mechanics are per `[19 §6](../foundations/19_expression_flow.md)`; a `Parameter` reaching an adapter is a hard error owned by the planner (`PlanErrorKind`), not by `35`.

## 7. Authoring-Surface DSL — `expr_fn`, `std::ops`, `ExprFunctionExt`

Per `[14 §9.2](../foundations/14_expressions.md)` final paragraph, the canonical authoring-surface constructors live in `semstrait-ir::expr::expr_fn`. The Rust DSL mirrors the YAML reserved-tag catalog from `[14 §6.4.1](../foundations/14_expressions.md)` exactly — `dim("region")` in Rust corresponds to `{ dim: region }` in YAML and produces the same `SemanticLeaf::Dimension { name, accessor: None }` shape.

The DSL is **opt-in** ergonomic sugar; every value it produces is also constructible via direct struct / enum literal. Downstream tooling that prefers not to take the `expr_fn` dependency-surface (`semstrait-manifest::compile`'s lowering, `semstrait-planner`'s plan-tree builders) constructs `Expr<L>` values directly.

**Placement (Q4.F, 2026-05-21).** `expr_fn`, `std::ops` impls, and `ExprFunctionExt` live in `semstrait-ir` v1, co-located with the types they construct. Open free constructors — no sealing trait gates external usage.

### 7.1 `expr_fn` free constructors

The six canonical free functions, one per `[14 §6.4.1](../foundations/14_expressions.md)`'s leaf-tag catalog:

```rust
pub mod expr_fn {
    use crate::{Expr, PhysicalExpr, SemanticExpr, ColumnRef, SemanticsName,
                 PhysicalLeaf, SemanticLeaf};

    /// Sealed dispatch trait for `col` to return either `SemanticExpr` or
    /// `PhysicalExpr` based on the inferred call-site type. Per the
    /// `14 §6.4.1` site-legality table, `col` is legal at both site kinds.
    pub trait FromColumnRef: sealed::Sealed { fn from_column_ref(c: ColumnRef) -> Self; }
    impl FromColumnRef for PhysicalExpr { /* PhysicalLeaf::Column(c) */ }
    impl FromColumnRef for SemanticExpr { /* SemanticLeaf::Column(c) — legal only under `auto` per `14 §8` */ }

    /// `col("amount")` — physical column reference. Returns whichever
    /// expression type the call site requires; type-context driven.
    pub fn col<E: FromColumnRef>(name: impl Into<String>) -> E;

    /// `field("revenue")` — untyped semantic reference. `SemanticExpr` only.
    /// Kind resolved at compile by registry lookup per `19 §3`.
    pub fn field(name: impl Into<String>) -> SemanticExpr;

    /// `dim("region")` — typed Dimension reference. `SemanticExpr` only.
    /// Optional accessor attached via `ExprFunctionExt` methods (§7.3).
    pub fn dim(name: impl Into<String>) -> SemanticExpr;

    /// `measure("revenue")` — typed Measure reference. `SemanticExpr` only.
    pub fn measure(name: impl Into<String>) -> SemanticExpr;

    /// `metric("conv_rate")` — typed Metric reference. `SemanticExpr` only.
    pub fn metric(name: impl Into<String>) -> SemanticExpr;

    /// `key("order_id")` — typed Key reference. `SemanticExpr` only.
    pub fn key(name: impl Into<String>) -> SemanticExpr;

    mod sealed { pub trait Sealed {} impl Sealed for super::PhysicalExpr {} impl Sealed for super::SemanticExpr {} }
}
```

The exact spelling of `col`'s dispatch (sealed trait vs two named functions vs builder) is an implementation choice; `35` ratifies that **both target types are constructible from a single name surface** so the YAML ↔ Rust alignment from `[14 §6.4.1](../foundations/14_expressions.md)` holds. The other five constructors (`field`, `dim`, `measure`, `metric`, `key`) are `SemanticExpr`-only per the semantic-site site-legality rule.

### 7.2 `std::ops` impls

Operator overloading on `SemanticExpr` and `PhysicalExpr` mirrors `[14 §3.3](../foundations/14_expressions.md)`'s `BinaryOpKind` and `UnaryOpKind` rosters. Authors write `dim("revenue") + measure("cost")` and get an `Expr::BinaryOp { op: Add, .. }` value.

```rust
// Arithmetic
impl<L: ExprLeaf> Add for Expr<L>      { type Output = Expr<L>; /* BinaryOpKind::Add */ }
impl<L: ExprLeaf> Sub for Expr<L>      { type Output = Expr<L>; /* BinaryOpKind::Subtract */ }
impl<L: ExprLeaf> Mul for Expr<L>      { type Output = Expr<L>; /* BinaryOpKind::Multiply */ }
impl<L: ExprLeaf> Div for Expr<L>      { type Output = Expr<L>; /* BinaryOpKind::Divide */ }
impl<L: ExprLeaf> Rem for Expr<L>      { type Output = Expr<L>; /* BinaryOpKind::Mod */ }

// Logical (SQL And / Or — Rust doesn't allow overloading `&&` / `||`)
impl<L: ExprLeaf> BitAnd for Expr<L>   { type Output = Expr<L>; /* BinaryOpKind::And */ }
impl<L: ExprLeaf> BitOr for Expr<L>    { type Output = Expr<L>; /* BinaryOpKind::Or */ }

// Unary
impl<L: ExprLeaf> Neg for Expr<L>      { type Output = Expr<L>; /* UnaryOpKind::Negate */ }
impl<L: ExprLeaf> Not for Expr<L>      { type Output = Expr<L>; /* UnaryOpKind::Not */ }
```

`BitAnd` / `BitOr` carry SQL `AND` / `OR` semantics (not bitwise) because `Expr<L>` does not include bitwise operators in v1. Comparison operators (`==`, `<`, `>`, …) cannot be `std::ops`-overloaded in Rust because the `PartialEq` / `PartialOrd` traits return `bool`, not custom types; these surface via the `ExprFunctionExt` trait (§7.3) as `.eq(other)` / `.lt(other)` / etc., or via the `binary_op` Declarative form per `[14 §6.4](../foundations/14_expressions.md)`.

### 7.3 `ExprFunctionExt` extension trait

Builder-style sugar for predicates, casts, aggregates, and the per-kind accessor methods that `std::ops` cannot express directly:

```rust
/// Extension trait providing builder-style sugar on `SemanticExpr` and
/// `PhysicalExpr` for operations that `std::ops` cannot directly model.
///
/// Per `14 §9.2`. Method roster mirrors the `14 §3.3` structural variants
/// and the `14 §4.1` accessor sugar; specific method-naming surface is
/// implementation detail.
pub trait ExprFunctionExt: Sized {
    // Comparison constructors (PartialEq / PartialOrd return bool, not Expr)
    fn eq(self, other: Self) -> Self;     // BinaryOpKind::Eq
    fn neq(self, other: Self) -> Self;    // BinaryOpKind::NotEq
    fn lt(self, other: Self) -> Self;     // BinaryOpKind::Lt
    fn lt_eq(self, other: Self) -> Self;  // BinaryOpKind::LtEq
    fn gt(self, other: Self) -> Self;     // BinaryOpKind::Gt
    fn gt_eq(self, other: Self) -> Self;  // BinaryOpKind::GtEq

    // Predicate sugar
    fn is_null(self) -> Self;                                       // Expr::IsNull
    fn in_list(self, list: Vec<Self>, negated: bool) -> Self;       // Expr::InList
    fn between(self, low: Self, high: Self) -> Self;                // Expr::Between
    fn like(self, pattern: Self, kind: LikeKind) -> Self;           // Expr::Like

    // Type / null sugar
    fn cast(self, target: DataType, on_failure: CastFailure) -> Self;  // Expr::Cast
    fn coalesce(self, others: Vec<Self>) -> Self;                       // Expr::Coalesce
    fn null_if(self, right: Self) -> Self;                              // Expr::NullIf

    // Aggregate constructor (admitted only at aggregate-admitting sites
    // per `14 §7`; the trait method exists on both `SemanticExpr` and
    // `PhysicalExpr`, with site-admission enforced at parse / compile)
    fn aggregate(self, op: AggregationOp, distinct: bool) -> Self;     // Expr::Aggregate
}

impl ExprFunctionExt for SemanticExpr { /* full surface */ }
impl ExprFunctionExt for PhysicalExpr { /* full surface */ }
```

Per-kind accessor sugar (`.previous()`, `.next()`, `.delta()`, `.percent_change()`, `.first()`, `.last()`, `.lag(n)`, `.lead(n)`) is surfaced as methods on `SemanticExpr` whose visibility is gated by the inner `SemanticLeaf` variant — typically realized via thin per-kind newtype shims returned by `expr_fn::measure` / `expr_fn::dim` / etc. so that `measure("x").previous()` typechecks while `dim("x").previous()` does not. The exact shim discipline is an implementation choice; `35` ratifies the four accessor enums (§6.1) and the kind-agreement contract.

Sugar accessors lower to `Window`-rooted subtrees during compile per `[14 §4.2](../foundations/14_expressions.md)`; the DSL methods produce typed leaves with `accessor: Some(_)`, not `Window` nodes directly. `Window` is intentionally not author-constructible per `[14 §6.4.1](../foundations/14_expressions.md)`.

## 8. `CanonicalFn` and the `FunctionRegistry`

### 8.1 Where the function-catalog architecture lives

Per `[14a §2](../foundations/14a_function_catalog.md)`, the canonical function identity (`CanonicalFn`), the registry shape (`FunctionRegistry`), the specification shape (`FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`), and the extension hook (`RegistryExtension`) are ratified in `14a`. `35` is the **crate** that owns the implementation.

This ownership moved from `semstrait-common` to `semstrait-ir` at the `14` second-refinement landing per `[14 §9.2](../foundations/14_expressions.md)`. Rationale: every consumer of `Expr<L>::FunctionCall { name: CanonicalFn, ... }` needs the registry to resolve `name` to a `FunctionSpec`; placing the registry adjacent to `Expr<L>` (and to the leaf sets that reference it) keeps the dependency direction natural and removes the cross-crate hop downstream consumers previously paid.

### 8.2 Public surface

```rust
/// Canonical function identity newtype. Per `14a §2`.
///
/// Newtype-over-stable exception per `30 §4.3`: no `#[non_exhaustive]`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct CanonicalFn(/* crate-private: normalized interned name */);

impl CanonicalFn {
    pub fn new(name: impl Into<String>) -> Result<Self, ValidateError>;
    pub fn as_str(&self) -> &str;
}

/// The single canonical function catalog. Per `14a §2.2 / §2.3`.
///
/// Sealed at startup by every crate that registers signatures
/// (`semstrait-ir` bootstraps the built-ins; adapter crates contribute
/// engine-specific overloads via `RegistryExtension` per `14a §7`).
pub struct FunctionRegistry { /* crate-private */ }

impl FunctionRegistry {
    pub fn lookup(&self, name: &CanonicalFn) -> Option<&FunctionSpec>;
    pub fn contains(&self, name: &CanonicalFn) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = (&CanonicalFn, &FunctionSpec)>;
    pub fn is_sealed(&self) -> bool;
}

/// Process-wide accessor returning the sealed registry. Per `14a §2`.
pub fn function_registry() -> &'static FunctionRegistry;

/// Per-function specification — name, signature overloads, category,
/// return-type rule. Per `14a §3`.
#[non_exhaustive]
pub struct FunctionSpec {
    pub name:        CanonicalFn,
    pub signatures:  Vec<FnSignature>,
    pub category:    FunctionCategory,
    pub return_type: ReturnTypeRule,
}

/// One signature overload — argument types + arity discipline.
/// Per `14a §3.1`.
#[non_exhaustive]
pub struct FnSignature {
    pub params:        Vec<ParamType>,
    pub variadic_tail: Option<ParamType>,
}

/// Parameter-type carrier. Per `14a §3.2`.
#[non_exhaustive]
pub enum ParamType {
    Concrete(DataType),
    AnyOf(Vec<DataType>),
    NumericFamily,
    StringFamily,
    TemporalFamily,
    Any,
}

/// Return-type computation rule. Per `14a §3.3`.
#[non_exhaustive]
pub enum ReturnTypeRule {
    Fixed(DataType),
    SameAsFirstArg,
    SameAsArg(u32),
    Custom(fn(&[DataType]) -> Result<DataType, CompileError>),
}

/// Function-category axis (scalar / aggregate / window). Per `14a §3.4`.
#[non_exhaustive]
pub enum FunctionCategory {
    Scalar,
    Aggregate,
    Window,
}

/// Adapter / downstream-crate extension hook. Per `[14a §7](../foundations/14a_function_catalog.md)`.
///
/// Declarative: implementers expose a static spec list and an adapter
/// identity. `function_registry()` enumerates linked impls at startup
/// and folds their `FUNCTIONS` into the sealed registry.
///
/// Not sealed — adapter crates outside the workspace MAY contribute
/// their own overloads.
pub trait RegistryExtension {
    const ADAPTER_ID: &'static str;
    const FUNCTIONS: &'static [FunctionSpec];
}
```

`CompileError` and `ValidateError` are owned here in `semstrait-ir::error` (§16) — they are raised by the registry callbacks (`ReturnTypeRule::Custom`) and the trait machinery (`Tree::with_new_children`, `Rewriter<N>::f_*`) respectively. The plan-shape `IrErrorKind` (§16.3) is the third enum exposed by this crate, scoped to plan-tree concerns.

## 9. Public Types — `SemanticPlan` Root

### 9.1 Shape

```rust
/// The canonical, engine-agnostic query plan tree. Output of the planner
/// (`34`), input of every adapter (`36`). Per `00 §4.1`.
///
/// A `SemanticPlan` is a single rooted tree of `PlanNode`s plus
/// plan-wide metadata: the names in the final projection order, and any
/// warning-severity diagnostics the planner wishes to surface alongside
/// the plan (errors abort planning per `10 §3.4`).
#[non_exhaustive]
pub struct SemanticPlan {
    /// The root of the plan tree. Never empty; planning produces at
    /// least a `Scan` leaf.
    pub root: PlanNode,

    /// Output column names in the order they appear in `root`'s
    /// output schema. Length equals `root.meta().output_schema.len()`.
    /// Each entry is the user-visible column label the adapter emits
    /// (e.g. the SELECT-list alias in generated SQL).
    pub output_names: Vec<Name>,

    /// Warning-severity diagnostics surfaced by `plan` + `optimize`.
    /// Per `30 §7.3` and `34 §13.2`, warnings flow through BOTH the
    /// planner result tuple's `Diagnostics<PlanErrorKind>` second element
    /// AND this artifact-side `Vec<PlanDiagnostic>`. The tuple is the
    /// caller's operational contract; this field is the artifact's
    /// self-describing contract for downstream adapters and inspectors
    /// (Q-IR-007, 2026-05-21). Contains only `Severity::Warning` entries
    /// per `30 §5.2`; errors abort planning and never reach `SemanticPlan`
    /// per `10 §3.4`. The element type uses a heterogeneous envelope
    /// (boxed `Diagnostic<dyn Diagnose>` or an erased `PlanDiagnostic`
    /// wrapper per `34 §13.2`) so warnings from `PlanErrorKind` and
    /// `OptimizeErrorKind` can co-exist on the artifact. Adapters MAY
    /// append their own warning-severity entries during `adapt` but are
    /// not required to (see `36`).
    pub diagnostics: Vec<PlanDiagnostic>,
}
```

### 9.2 Invariants

A `SemanticPlan` is **well-formed** when:

1. `output_names.len() == root.meta().output_schema.len()` — every output column has a user-visible name.
2. Every `Name` in `output_names` is a valid identifier (see §11.4).
3. `root` and every descendant satisfy the tree invariants of §13.
4. Every `PlanDiagnostic` in `diagnostics` has `Severity::Warning`.

Construction does **not** re-check invariants 1–3 at runtime (planning established them; re-checking is a planner-regression catch, not a caller contract). An optional `SemanticPlan::validate()` method (§14.3) walks the tree and reports violations as `Diagnostic<IrErrorKind>` for debugging.

### 9.3 Serde

`SemanticPlan` derives `Serialize` / `Deserialize` under the crate-level `serde` feature. The wire form is the direct struct shape; no intermediate envelope. `PhysicalExpr` inside child nodes serializes through `35`'s own `Expr<L>` serde (§15). `PlanNode` (`#[non_exhaustive]`) uses serde's `untagged` policy with a discriminator field (`kind: "scan" | "filter" | ...`) so the wire form survives the addition of new variants per I10.

A serialized `SemanticPlan` is a format-stable portable plan artifact: two processes with the same compiled SemanticManifest can exchange a `SemanticPlan` and get identical adapter output. This is what makes the crate a faithful IR.

### 9.4 Construction patterns

Planners build a `SemanticPlan` from the bottom up:

```rust
let scan = PlanNode::Scan(ScanNode {
    meta: NodeMeta::new(output_schema),
    source: source_ref,
    columns: scan_columns,
    filters_pushdown: Vec::new(),
});

let filter = PlanNode::Filter(FilterNode {
    meta: NodeMeta::new_shared(scan.meta().output_schema.clone()),
    input: Box::new(scan),
    predicate: physical_expr_from_request(req)?,
});

let plan = SemanticPlan {
    root: filter,
    output_names: vec![Name::new("id")?, Name::new("amount")?],
    diagnostics: Vec::new(),
};
```

Builders in `semstrait-planner` (`34`) wrap these constructions in a higher-level fluent API; `35` exposes only the struct-literal form so any consumer can construct / inspect a plan without the planner linked.

### 9.5 Cloning

`SemanticPlan: Clone`. Every child node is `Box`-owned; `Clone` is a deep clone. For cheap structural sharing inside optimizer passes, use `walk_post` with in-place `transform` (§14) rather than successive `clone`s.

## 10. Public Types — `PlanNode` Sum

### 10.1 The sum type

```rust
/// A single node within a `SemanticPlan`. The variant set forms the
/// canonical operator catalog borrowed (structurally only) from
/// DataFusion's `LogicalPlan`, Calcite's `RelNode`, and Substrait's
/// `Rel`. Per `00 §4.1` and `35 §1.4`.
///
/// `#[non_exhaustive]` per I10: adding a variant (e.g. a future
/// `Distinct`, `Window`, `Limit` that departs from `Fetch`'s shape) is
/// MINOR.
#[non_exhaustive]
pub enum PlanNode {
    Scan    (ScanNode),
    Filter  (FilterNode),
    Project (ProjectNode),
    Agg     (AggNode),
    Join    (JoinNode),
    Union   (UnionNode),
    Sort    (SortNode),
    Fetch   (FetchNode),
}

impl PlanNode {
    /// Shared accessor for the `NodeMeta` that every variant carries.
    pub fn meta(&self) -> &NodeMeta;
    pub fn meta_mut(&mut self) -> &mut NodeMeta;

    /// Shared accessor for the set of child `PlanNode`s (0, 1, or 2+).
    pub fn children(&self) -> Vec<&PlanNode>;
    pub fn children_mut(&mut self) -> Vec<&mut PlanNode>;
}
```

Eight variants in v1. Every variant wraps a struct (not tuple / record form) so field additions inside a variant are MINOR per `30 §4.2` (non-exhaustive struct growth).

### 10.2 `Scan` — leaf source

```rust
/// Reads a resolved source. The only `PlanNode` variant without child
/// nodes. Each `ScanNode` corresponds to one engine-level LogicalRelation
/// (Substrait `ReadRel`, DataFusion `TableScan`, Spark `LogicalRelation`,
/// SQL `FROM`). Engine-internal mechanics (partition discovery, multi-file
/// consolidation, schema merge) live below the `ScanNode` boundary —
/// see §10.2.1.
#[non_exhaustive]
pub struct ScanNode {
    pub meta: NodeMeta,

    /// Opaque handle into the SemanticManifest. Resolves to a
    /// `ResolvedPhysicalSource` per `15 §7.1`. Adapters consume the
    /// SemanticManifest + this handle to learn the on-engine table / path /
    /// format. `35` never stores the expanded path.
    pub source: SourceRef,

    /// The projected columns in the order the `Scan` outputs them.
    /// Each `ResolvedColumn` carries `{ name, data_type, nullable,
    /// ordinal }` per `15 §4.2`. Non-empty.
    pub columns: Vec<ResolvedColumn>,

    /// Push-down predicates resolved at plan time. Each `PhysicalExpr`
    /// references only columns in `columns` (enforced by §13.4). Empty
    /// when no pushdown is applicable or the adapter does not support
    /// it. Optimizer-filled per `34`; adapters MAY further narrow
    /// (pushing deeper into the source) or MAY decline (pulling back
    /// up into a `Filter`) per their capabilities.
    pub filters_pushdown: Vec<PhysicalExpr>,
}
```

`ScanNode` carries **no raw path, no URL, no dialect**. Resolution from `SourceRef` to on-engine identity happens in the adapter via the SemanticManifest (I1).

#### 9.2.1 Partition info — manifest-side, never on `ScanNode`

`ScanNode` carries **no partition columns, no partition transforms, no `partition_def` declarations, and no per-source partition values**. All partition-related metadata, when present, is reachable on the SemanticManifest via `ScanNode.source` — specifically `15 §3.4 PartitionColumn` (structural metadata extracted at compile from Hive-style path segments or catalog partition specs) and `15 §3.5.4 partition_def` (catalog-less Range / List declaration carried verbatim from `extras.storage.partition_def:` per `32 §4`). Engine consumers handle partition pruning from `filters_pushdown` predicates against the source's output columns (which include Hive-derived partition columns when the source is file-based with partition discovery).

This is grounded in the 4-consumer alignment ratified at `Q-MAP-002` closure:

| Consumer | Logical scan rel | Partition info on the rel? |
|---|---|---|
| Substrait | `ReadRel { read_type: LocalFiles \| NamedTable \| IcebergTable, filter, best_effort_filter, ... }` | No — partition pruning derived engine-side from `filter` / `best_effort_filter` against partition columns; `FileOrFiles.partition_index` is work-unit partitioning (file slicing across executors), not Hive partitioning. |
| DataFusion | `TableScan { source: TableSource, filters: Vec<Expr>, ... }` | No — `ListingOptions.table_partition_cols` is config on the source provider, not on the rel. |
| Spark | `LogicalRelation { relation: BaseRelation, ... }` (`HadoopFsRelation` for files) | No — partition columns surface as regular output columns; `PartitionPruning` runs as an optimizer rule against `Filter` nodes above the relation. |
| SQL emit (DuckDB / Spark SQL / Trino) | one `FROM` reference (`read_parquet('glob', hive_partitioning=true)` / `parquet.\`path\`` / `<catalog>.<schema>.<table>`) | No — `WHERE` clause drives engine-side pruning. |

Adding partition fields to `ScanNode` would diverge from every primary consumer's expected shape and force adapters to translate semstrait-internal partition carriage into engine-native form on every emit. The IR stays minimal; partition handling is an adapter / engine concern reading from the SemanticManifest, not from the plan. v1 adapters defer pruning to engines; v2+ adapters MAY consult the manifest's `partition_def` / `PartitionColumn[]` for advanced planning hints (per `15 §3.5.4`'s forward-compat clause).

### 10.3 `Filter` — predicate

```rust
#[non_exhaustive]
pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Boolean-valued predicate. Operand type must be `Boolean` per
    /// `14 §7` / §13.10. Columns referenced must exist in
    /// `input.meta().output_schema`.
    pub predicate: PhysicalExpr,
}
```

Pass-through schema: `FilterNode.meta.output_schema` equals `input.meta().output_schema` (enforced at construction per §11.8; adapters rely on this).

### 10.4 `Project` — column list

```rust
#[non_exhaustive]
pub struct ProjectNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Ordered list of `(output_name, expression)` pairs. The result
    /// schema's field at ordinal `i` has name `projections[i].0` and
    /// `data_type` follows by inference from `projections[i].1`'s
    /// leaf-level `ExprLeaf::inferred_type` per §13.2. Empty list is
    /// rejected at construction (trivial `Project` collapses to
    /// `input`).
    pub projections: Vec<(Name, PhysicalExpr)>,
}
```

### 10.5 `Agg` — group-by + aggregates

```rust
#[non_exhaustive]
pub struct AggNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Group-by keys by output-schema column name. Each `Name` must
    /// resolve to a column in `input.meta().output_schema`.
    /// An empty `group_by` is a grand-total aggregation (equivalent
    /// to SQL `SELECT agg(...) FROM ...` with no `GROUP BY`).
    pub group_by: Vec<Name>,

    /// Aggregate expressions paired with their output-column names.
    /// Each `AggregateExpr` is an `Expr::Aggregate` kernel (see §11.7)
    /// lifted out of the input `PhysicalExpr` by Phase B per `19 §7`,
    /// whose inner expression references only columns in
    /// `input.meta().output_schema`.
    pub aggregates: Vec<(Name, AggregateExpr)>,
}
```

### 10.6 `Join` — binary composition

```rust
#[non_exhaustive]
pub struct JoinNode {
    pub meta: NodeMeta,
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,

    /// Join kind. Per `16 §5.2` and `00 §4.1`. `35` is authoritative
    /// for the on-wire shape; `16` is authoritative for the vocabulary
    /// (Inner / Left / Right / Full).
    pub join_type: JoinType,

    /// Cardinality annotation carried from the Relationship graph
    /// (`16 §5.1`). Planners fill this from the `Relationship`
    /// ratified in `16`; optimizers / adapters MAY use it for
    /// optimization hints (e.g. `OneToOne` → redundant-join
    /// elimination). Never elided; always reflects the authored
    /// Relationship.
    pub cardinality: Cardinality,

    /// Join keys. Each `KeyPair` names one column from `left`'s
    /// schema and one from `right`'s. Non-empty for equi-joins
    /// (`Inner`, `Left`, `Right`, `Full`) in v1; cross-join is not a
    /// v1 variant (see §17.1 TD-IR-CROSS-JOIN).
    pub on: Vec<KeyPair>,
}
```

Non-equi-join predicates (range joins, inequality joins) are deferred — a future `JoinNode.residual: Option<PhysicalExpr>` field is MINOR per §17.1.

### 10.7 `Union` — n-ary stack

```rust
#[non_exhaustive]
pub struct UnionNode {
    pub meta: NodeMeta,

    /// Two or more inputs. Every input's `output_schema` must be
    /// structurally compatible (same arity, same element types, same
    /// nullability — per §13.6).
    pub inputs: Vec<PlanNode>,

    /// Whether to apply `DISTINCT` deduplication after the union.
    /// `false` = `UNION ALL` (bag semantics); `true` = `UNION` (set
    /// semantics). Defaults to `false` — every engine natively
    /// supports `UNION ALL` without rewrite; `true` demands a
    /// post-hash-agg pass.
    pub distinct: bool,
}
```

### 10.8 `Sort` — ordering

```rust
#[non_exhaustive]
pub struct SortNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Sort keys in priority order (first entry is the primary key).
    /// Each `Name` resolves to a column in `input`'s schema.
    /// Each `SortDir` carries ascending/descending + null-ordering.
    pub order: Vec<(Name, SortDir)>,
}
```

### 10.9 `Fetch` — limit / offset

```rust
#[non_exhaustive]
pub struct FetchNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Row limit. `None` = unlimited (equivalent to no `LIMIT` clause
    /// in SQL). `Some(0)` is well-formed; adapters may short-circuit
    /// emission.
    pub limit: Option<u64>,

    /// Row offset. `None` = no offset. `Some(0)` is equivalent to
    /// `None` and kept as a distinct value for Substrait-roundtrip
    /// fidelity.
    pub offset: Option<u64>,
}
```

Unsigned integer (`u64`) deliberately rejects negative values at the type boundary; the `Option` shape keeps "no limit / no offset" distinct from "limit 0 / offset 0". This matches Substrait's `FetchRel.count` / `offset` (both `i64`) at the upper half of `u64`'s range; the rare `u64 > i64::MAX` case is rejected at adapter-emit time with `IrErrorKind::FetchValueOutOfRange`.

## 11. Public Types — Shared Primitives

### 11.1 `NodeMeta`

```rust
/// Metadata attached to every `PlanNode`. Per `15 §7` and `17 §3`.
#[non_exhaustive]
pub struct NodeMeta {
    /// Per-process opaque identifier for this node, scoped to a single
    /// `SemanticPlan` lifetime. Used by the optimizer for rule-engine
    /// source tracking and by the adapter for diagnostic correlation
    /// within one planning invocation. NOT a content hash — two
    /// structurally-equal plans MAY have different `NodeId`s, and a
    /// `transform` that rebuilds the same shape MAY produce a fresh
    /// `NodeId`. Comparing `NodeId`s across processes, across
    /// `SemanticPlan` instances, or across serialize-rehydrate cycles
    /// is undefined; consumers requiring cross-run diff MUST compare
    /// structurally. Per Q-IR-002 (2026-05-21).
    pub node_id: NodeId,

    /// Output schema after this node. `Arc` allows pass-through
    /// nodes (Filter, Sort, Fetch) to share the parent schema without
    /// deep-cloning.
    pub output_schema: std::sync::Arc<Schema>,

    /// Plan-level annotations per S5 (§1.5). Non-computational metadata
    /// only — `34` and `36` MUST NOT make planning or emission decisions
    /// based on `annotations` content. Two classes per §11.1.1:
    /// **TRACE** annotations carry descriptive provenance (DataKindRef,
    /// StrategyBoundary, …); **PLAN** annotations carry advisory hints
    /// for tools and pretty-printers (AggregateRole, FilterSource,
    /// Additivity). Both classes round-trip through Substrait per §15.3.
    pub annotations: Vec<SemAnnotation>,
}
```

`NodeId` is a newtype over `Uuid::new_v4()` in v1; external consumers should treat it as opaque per the refined doc-comment above. `Schema` is the canonical structural schema owned at §4.4 — `{ columns: Vec<SchemaColumn> }` where `SchemaColumn { name, data_type, nullable }`. Plan-level uses the same shape as physical-source schemas; the distinction is the production site (planner-derived vs source-metadata-derived), not the type. `Schema` lives here because every consumer above (manifest / planner / adapter) consumes it; ir owns no parallel plan-side `Schema` type (Q-IR-006, 2026-05-21).

#### 10.1.1 `SemAnnotation` — variant inventory and classification

`SemAnnotation` is an additive `#[non_exhaustive]` sum. Variants split into two classes per S5 (§1.5):

- **PLAN** class — read by `34` / `36` as part of the IR-to-consumer contract (advisory hints, never dispatch). Renaming or removing a PLAN variant is MAJOR per `[30 §2.1](30_api_contracts.md)`.
- **TRACE** class — descriptive only; never read by `34` / `36`. Adding, renaming, or removing a TRACE variant is MINOR (additive growth) since no consumer reads them.

Per R2 (§1.6), every variant name is independently guessable. References are named after the referenced thing.

```rust
/// Per-node annotation. Per S5 (§1.5).
///
/// Variants are classified TRACE or PLAN. See per-variant doc-comments.
#[non_exhaustive]
pub enum SemAnnotation {
    // ───── PLAN class ─────

    /// Role of an aggregate node in additivity resolution.
    /// PLAN — read by `34`'s additivity reasoning.
    AggregateRole(AggregateRole),

    /// Source of a filter — which authoring site introduced it.
    /// PLAN — read by `34`'s `19 §9.1` filter-placement logic.
    FilterSource(FilterSource),

    /// Effective-additivity hint for a measure subtree.
    /// PLAN — read by `34`'s lossy-reaggregation advisory channel
    /// per `19 §6.5`.
    Additivity(AdditivityAnnotation),

    // ───── TRACE class ─────

    /// DataKind that contributed this subtree.
    /// TRACE — for tools, debuggers, SQL pretty-printers.
    DataKindRef(SemanticsName),

    /// Boundary marker emitted by a planner strategy when one DataKind's
    /// strategy expansion stops contributing nodes and another begins.
    /// TRACE — for tools that visualize plan-tree composition.
    StrategyBoundary {
        type_: SemanticsName,
        position: BoundaryPosition,
    },
}

/// Position of a `StrategyBoundary` annotation relative to the strategy's
/// emitted subtree.
#[non_exhaustive]
pub enum BoundaryPosition {
    /// First node a strategy emitted; conceptually "begin".
    Entry,
    /// Last node a strategy emitted; conceptually "end".
    Exit,
}
```

PLAN-class variants `AggregateRole` / `FilterSource` / `Additivity` (and their support enums `AggregateRole`, `FilterSource`, `AdditivityAnnotation`) are ratified in `34`'s planner contract; `35` carries the structural shape only. TRACE-class variants `DataKindRef` / `StrategyBoundary` are emitted by planner strategies to mark subtree origin.

`KindRef(String)` (legacy variant in pre-Phase-1 code) is renamed to `DataKindRef(SemanticsName)` per R2 — bare `Kind` was opaque. Code-side rename lands per `STATUS.md` row R.

### 11.2 `SourceRef`

```rust
/// Opaque handle to a source in the SemanticManifest. Constructed by
/// the planner; resolved by the adapter against the SemanticManifest
/// it was handed alongside the `SemanticPlan`. No path, URL, catalog
/// name, or file format leaks into the plan tree. I1 guarantee.
///
/// The handle's inner shape is crate-private; consumers compare
/// `SourceRef`s for equality only. Newtype-over-stable exception per
/// `30 §4.3`: no `#[non_exhaustive]`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SourceRef(/* crate-private */);
```

### 11.3 `ResolvedColumn`

```rust
/// A column as projected by a `Scan`. Per `15 §4.2`.
#[non_exhaustive]
pub struct ResolvedColumn {
    pub name: Name,
    pub data_type: DataType,
    pub nullable: bool,
    /// Ordinal in the underlying source's native schema order. Adapters
    /// consume this when emitting stable column references.
    pub ordinal: u32,
}
```

### 11.4 `Name`

```rust
/// Identifier used for output-column names, group-by keys, sort keys,
/// and projection aliases. A plan-level newtype over `String` with a
/// construction boundary that enforces identifier well-formedness:
///
/// - Non-empty.
/// - UTF-8 (guaranteed by `String`).
/// - Not a reserved plan-tree tag (see §11.4.1).
///
/// `Name` is **not** normalized (case-folding, whitespace-collapsing)
/// — two distinct `Name`s may differ only in case, matching
/// case-preserving-but-case-sensitive semantics from `11 §5`.
///
/// Newtype-over-stable exception per `30 §4.3`: no
/// `#[non_exhaustive]`.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Name(String);

impl Name {
    /// Validates the identifier; returns `IrErrorKind::InvalidName` on
    /// violation. Bare-kind shape per `31 §3.1` construction-site convention.
    pub fn new(s: impl Into<String>) -> Result<Self, IrErrorKind>;

    pub fn as_str(&self) -> &str;
    pub fn into_string(self) -> String;
}
```

#### 10.4.1 Reserved plan-tree tags

Substrait-roundtrip fidelity reserves a small set of identifier prefixes for semstrait's own use: `__semstrait_`, `__plan_`, `__agg_`. Constructing a `Name` with one of these prefixes raises `IrErrorKind::ReservedName`. The reserved-prefix set is additive; adding new prefixes is MINOR.

### 11.5 `KeyPair`

```rust
/// One join-key pair on a `JoinNode.on`. Per `16 §5.1`.
///
/// Both `left` and `right` are column names resolving against the
/// join's corresponding child's `output_schema`. Column types must
/// match per §13.5 — planner-side reconciliation lives in
/// `19 §3` / `15 §10.5`; a mismatch reaching `35` is reported by
/// `SemanticPlan::validate` as `IrErrorKind::JoinKeyTypeMismatch`.
#[non_exhaustive]
pub struct KeyPair {
    pub left: Name,
    pub right: Name,
}
```

### 11.6 `SortDir` / `NullOrdering`

```rust
/// Sort direction + null-ordering bundle. Per Substrait's
/// `SortField.direction`.
#[non_exhaustive]
pub enum SortDir {
    /// Ascending order.
    Asc  { nulls: NullOrdering },
    /// Descending order.
    Desc { nulls: NullOrdering },
}

/// Where to place `NULL` values in a sort.
#[non_exhaustive]
pub enum NullOrdering {
    /// Place `NULL` values first.
    First,
    /// Place `NULL` values last.
    Last,
    /// Let the adapter choose (SQL default: ASC NULLS LAST, DESC NULLS
    /// FIRST — most engines; adapters MAY narrow). Carries zero
    /// semstrait-side constraint.
    Unspecified,
}

impl SortDir {
    pub const ASC_NULLS_FIRST:  SortDir = SortDir::Asc { nulls: NullOrdering::First };
    pub const ASC_NULLS_LAST:   SortDir = SortDir::Asc { nulls: NullOrdering::Last  };
    pub const DESC_NULLS_FIRST: SortDir = SortDir::Desc { nulls: NullOrdering::First };
    pub const DESC_NULLS_LAST:  SortDir = SortDir::Desc { nulls: NullOrdering::Last  };
}
```

Keeping null-ordering on `SortDir` (rather than a separate `Sort::null_order: NullOrdering` field next to each key) keeps the per-key shape tight and matches Substrait's wire form.

### 11.7 `AggregateExpr`

```rust
/// Plan-level carrier for an aggregate kernel on `AggNode.aggregates`,
/// hoisted from a `PhysicalExpr` by Phase B aggregate-lift per
/// `[19 §7](../foundations/19_expression_flow.md)`. Per §13.1, no
/// `Expr::Aggregate` survives inside a predicate/projection slot.
#[non_exhaustive]
pub struct AggregateExpr {
    pub aggregation:   AggregationOp,    // matches Expr::Aggregate.op (14 §3.3)
    pub input_expr:    PhysicalExpr,     // inner argument (e.g. Column("amount") for sum(amount))
    pub distinct:      bool,             // matches Expr::Aggregate.distinct
    pub filter:        Option<PhysicalExpr>,  // FILTER (WHERE ...) clause; v1 adapters MUST accept None
    pub inferred_type: DataType,         // populated by Phase B; adapters MAY read directly
}
```

### 11.8 Invariants enforced at construction

Each `PlanNode` variant's struct is constructed directly (no hidden builder); the variant's field combination is the contract. Schema invariants (§13) are *not* checked at construction — consumers rely on the planner to produce well-formed trees and rely on `SemanticPlan::validate()` (§14.3) for a debug-only full re-check.

## 12. Public Types — Adapter Artifact Family

The output types produced by `semstrait-adapter::adapt()` (`36`) from a `SemanticPlan`. `35` ratifies the structural shape; `36` owns the emission semantics.

### 12.1 `EngineArtifact`

```rust
/// Engine-ready plan artifact. Produced by `EngineAdapter::adapt()`
/// in `36`.
///
/// Two variants covering the two adapter emission modes:
/// - `Sql` for engines that consume SQL strings (DuckDB, Spark,
///   PostgreSQL, …).
/// - `Plan` for engines that consume Substrait plans natively
///   (DataFusion, some Spark paths, future Substrait-consuming
///   engines).
#[non_exhaustive]
pub enum EngineArtifact {
    Sql  (SqlArtifact),
    Plan (EnginePlan),
}
```

### 12.2 `EnginePlan`

```rust
/// Structured-IR engine output. Variants today: `Substrait(...)`.
/// Non-exhaustive per I10 — future engine IRs (e.g. direct
/// DataFusion-LogicalPlan emission, future vendor IRs) can be added
/// as MINOR variants.
#[non_exhaustive]
pub enum EnginePlan {
    /// A Substrait plan. Boxed to keep `EnginePlan`'s size moderate
    /// across platforms.
    Substrait(Box<substrait::proto::Plan>),
}

impl EnginePlan {
    /// Serialize the Substrait plan to protobuf bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, IrErrorKind>;
    /// Serialize the Substrait plan to pretty JSON.
    pub fn to_json(&self) -> Result<String, IrErrorKind>;
}
```

### 12.3 `SqlArtifact`

```rust
/// Text-based engine output. Per `00 §4.1`.
#[non_exhaustive]
pub struct SqlArtifact {
    /// The emitted SQL text. UTF-8.
    pub text: String,
    /// The dialect this text targets. Consumers use this to route the
    /// text to the correct engine.
    pub dialect: DialectId,
}
```

### 12.4 `DialectId` + `Dialect`

`DialectId` is **artifact-side identity only** (S7, R1). It identifies the dialect of an emitted `SqlArtifact` (§12.3) and the dialect a `Dialect` impl serves (§12.5); it does NOT travel on `SemanticPlan`, `PlanNode`, `Expr<L>`, `NodeMeta`, or any registry-side type. The pattern parallels dbt's `manifest.adapter_type`: a single artifact-level provenance tag that lets a downstream consumer route the artifact, without coupling the upstream model graph to the target adapter.

Per Q4.A (2026-05-21): the planner (`34`) produces a `SemanticPlan` whose entire payload is dialect-free; the adapter (`36`) is the producer of `EngineArtifact` and the only crate that stamps `DialectId`.

```rust
/// Stable identifier for a SQL dialect. Per `00 §4.1` and `36`.
///
/// Implemented as a newtype over a `&'static str` with `pub const`
/// identities per built-in adapter; adapters outside the workspace
/// register new dialects via the `Dialect` trait (§12.5).
///
/// Newtype-over-stable exception per `30 §4.3`: no
/// `#[non_exhaustive]`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct DialectId(&'static str);

impl DialectId {
    pub const fn name(self) -> &'static str { self.0 }

    // Canonical built-in dialect identities — one `pub const` per
    // in-workspace adapter per `00 §4.1`.
    pub const ANSI:       DialectId = DialectId("ansi");
    pub const DATAFUSION: DialectId = DialectId("datafusion");
    pub const DUCKDB:     DialectId = DialectId("duckdb");
    pub const SPARK:      DialectId = DialectId("spark");
}
```

### 12.5 `Dialect` trait

```rust
/// Capability / identity trait implemented by every SQL-emitting
/// adapter. Per `00 §4.1` and `36 §3`.
///
/// Not sealed — third-party adapter crates outside the workspace
/// (e.g. `semstrait-adapter-clickhouse`) MUST be able to impl
/// `Dialect` for their own `DialectId`.
pub trait Dialect {
    /// The dialect's stable identity.
    const ID: DialectId;

    /// Adapter-declared capability flags consumed by the planner's
    /// capability check (`36 §5`). Readers SHOULD NOT pattern-match
    /// exhaustively; the set is additive.
    fn capabilities(&self) -> &'static [Capability];
}
```

`Capability` (§12.6) is the dialect / adapter capability vocabulary. Type definition lives in `35`; per-adapter rosters and variant-addition drivers live in `36` (Q-IR-010, 2026-05-21).

### 12.6 `Capability`

```rust
/// Cross-boundary capability vocabulary. Type definition lives here;
/// per-adapter rosters and variant-addition drivers live in `36`.
///
/// Per Q-IR-010 (2026-05-21) and Q-ADAPT-002 (2026-05-21):
/// - This enum's body is the canonical roster (closed catalog rule R4).
/// - Adding a variant is a `35`-side MINOR edit driven by a concrete
///   adapter feature ratified in `[36 §4](36_semstrait_adapter.md)` /
///   `[§6](36_semstrait_adapter.md)`.
/// - **Scope rule:** `Capability` enumerates ONLY features that cannot
///   be synthesized at an adapter's PlanBuilder layer — features whose
///   absence in the consuming engine cannot be papered over without
///   changing semantics. Adapter-internal rewrite strategies (CTE
///   expansion, GROUPING SETS expansion, DISTINCT-aggregate emulation)
///   are NOT capabilities; they are private adapter strategy.
/// - **Load-bearing consumer:** the Substrait-handoff boundary, where
///   semstrait emits a Substrait plan and a foreign engine consumes it
///   without semstrait-side rewrite. SQL-emitting adapters
///   (`AnsiSqlAdapter`, `DataFusionSqlAdapter`, …) own their full
///   rewrite pipeline; their capability advertisement is ergonomic
///   (planner pre-flight, api pre-`adapt` UX), not contractual.
/// - Each adapter advertises its supported subset via its
///   `AdapterCapabilities { capabilities: &'static [Capability] }`
///   (`36 §6.1`).
#[non_exhaustive]
pub enum Capability {
    RegexpMatch,
    RegexpExtract,
    IntervalLiteral,
    AsOfJoin,
    StructAccess,
    // Variants added under `36`-driven rationale; current full roster
    // mirrored in `[36 §6](36_semstrait_adapter.md)`.
}
```

## 13. Tree Invariants

A **well-formed** `SemanticPlan` satisfies every invariant below. The planner (`34`) is the canonical producer; every invariant below is the planner's contract. `SemanticPlan::validate()` (§14.3) is an optional post-hoc walker that reports violations as `Diagnostic<IrErrorKind>`.

### 13.1 Expression-wrapper invariants

- Every predicate-valued expression on a `PlanNode` is a `PhysicalExpr` — never a `SemanticExpr`. This applies to `FilterNode.predicate`, `ScanNode.filters_pushdown[*]`, `ProjectNode.projections[*].1`, `AggregateExpr.input_expr`, `AggregateExpr.filter`. Invariant rationale: per `[14 §3.7](../foundations/14_expressions.md)`, the leaf-set boundary makes this a type-level invariant — `PhysicalExpr = Expr<PhysicalLeaf>` literally cannot contain `Field` / `Dimension` / `Measure` / `Metric` / `Key` because those variants do not exist in `PhysicalLeaf` (§5.1). Semantic-leaf resolution completed at `compile` per `[19 §3](../foundations/19_expression_flow.md)`.
- No `PhysicalExpr` stored on a `FilterNode.predicate`, `ProjectNode.projections[*].1`, `ScanNode.filters_pushdown[*]`, or future `JoinNode` residual carries an `Expr::Aggregate` node — aggregation is lifted into `AggNode.aggregates` as `AggregateExpr` (§11.7) by the planner's Phase B aggregate-lift per `[19 §7](../foundations/19_expression_flow.md)`. The `Expr::Aggregate` structural variant exists at the type level (per `[14 §3.3](../foundations/14_expressions.md)`); the no-aggregate-in-predicate rule is a plan-tree-level invariant enforced by `34`'s lift pass.
- No `PhysicalExpr` reaching a `PlanNode` carries an `Expr::Window` node directly authored — `Window` is compile-emitted only per `[14 §3.3](../foundations/14_expressions.md)`, entering the tree exclusively through sugar-accessor elimination at compile. A `Window` node in a `PhysicalExpr` stored on a `PlanNode` predicate / projection slot is acceptable as long as it came from the canonical sugar-elimination path; planner-side window placement (e.g. wrapping window functions into a future `PlanNode::Window` variant) is post-v1.

### 13.2 Type-resolution invariants

- Every `PhysicalExpr` stored on every `PlanNode` is fully type-resolved: every leaf returns `Some(_)` from `ExprLeaf::inferred_type()`, and every structural node's type follows by canonical inference from its children. Type inference is part of compile (`[19 §3.6](../foundations/19_expression_flow.md)`). A leaf reaching the plan tree with `inferred_type() == None` is `IrErrorKind::UnresolvedType` (reported by `SemanticPlan::validate`).
- Every `AggregateExpr.inferred_type` is populated per the aggregate-typing rules implied by the registered aggregate signatures in `FunctionRegistry` per `[14a §3.3](../foundations/14a_function_catalog.md)`.

### 13.3 Scan-schema invariants

- `ScanNode.columns[*]` references actual columns of the resolved source. The planner populates `columns` from the SemanticManifest entry that `SourceRef` resolves to; consistency between manifest and plan is the planner's contract.
- `ScanNode.meta.output_schema.len() == ScanNode.columns.len()`.
- `ScanNode.meta.output_schema.fields[i].name == ScanNode.columns[i].name` for all `i`.

### 13.4 Push-down invariants

- Every `PhysicalExpr` in `ScanNode.filters_pushdown` references only columns in `ScanNode.columns` (enforced by adapter at `36`, optimizer at `34`).
- `filters_pushdown` does not change `meta.output_schema` — it narrows row count, not column shape.

### 13.5 Join invariants

- `JoinNode.on` is non-empty. (Cross-joins deferred per §17.1.)
- For each `KeyPair`, `left` resolves to a column in `left.meta().output_schema` and `right` resolves to a column in `right.meta().output_schema`.
- For each `KeyPair`, `left`'s column `data_type` matches `right`'s (modulo nullability). Type reconciliation is a planner responsibility (`15 §10.5` Cast-wrapping at SemanticManifest compile time per `[19 §3](../foundations/19_expression_flow.md)`); a mismatch reaching `35` is `IrErrorKind::JoinKeyTypeMismatch`.
- `JoinNode.meta.output_schema` = structural concatenation of `left.meta().output_schema` and `right.meta().output_schema`, with nullability widened on the outer side per `join_type` (per SQL semantics).

### 13.6 Union invariants

- `UnionNode.inputs.len() >= 2`.
- All inputs have structurally compatible output schemas: same arity; same `DataType` at each ordinal; same nullability at each ordinal (after upward widening of nullable-to-non-nullable mismatches by the planner).
- `UnionNode.meta.output_schema` = first input's schema with nullability widened across inputs (per SQL semantics).

### 13.7 Agg invariants

- Every `Name` in `AggNode.group_by` resolves to a column in `input.meta().output_schema`.
- Every `(Name, AggregateExpr)` in `AggNode.aggregates` has a unique output `Name`. Duplicate output-name is `IrErrorKind::DuplicateAggName`.
- The inner `input_expr` of each `AggregateExpr` references only columns in `input.meta().output_schema`.
- `AggNode.meta.output_schema` = one column per `group_by` entry (in that order) followed by one column per `aggregates` entry (in that order).

### 13.8 Sort invariants

- Every `Name` in `SortNode.order[*].0` resolves to a column in `input.meta().output_schema`.
- Pass-through schema: `SortNode.meta.output_schema == input.meta().output_schema` (cheap via `Arc` share).

### 13.9 Fetch invariants

- If `FetchNode.limit == Some(0)`, the adapter MAY short-circuit to an empty-relation emission (e.g. `SELECT ... FROM ... WHERE false`); this is an adapter choice, not a plan-tree invariant.
- Pass-through schema: `FetchNode.meta.output_schema == input.meta().output_schema`.

### 13.10 Filter invariants

- The `FilterNode.predicate`'s inferred type is `DataType::Boolean` (derived from leaf-level inference per §13.2). A non-Boolean predicate reaching `35` is `IrErrorKind::FilterPredicateNotBoolean`.
- Pass-through schema: `FilterNode.meta.output_schema == input.meta().output_schema`.

## 14. Visitor / Traversal API

### 14.1 `PlanVisitor`

```rust
/// Tree walker over a `SemanticPlan`. Implementations provide node
/// handlers; `PlanNode::walk_pre` / `walk_post` dispatch.
///
/// Equivalent to `Visitor<PlanNode>` per the universal trait surface in
/// `semstrait-common` (`14 §3.1`); preserved as a named alias for
/// plan-tree consumer ergonomics. `PlanNode` also implements `Tree` so
/// the generic `Expr<L>`-side helpers (`apply`, `transform`) work over
/// plan trees too.
pub trait PlanVisitor {
    type Output;

    /// Called for each `PlanNode` encountered. The default
    /// implementation `()` for `Output = ()` descends children — a
    /// `walk_children` helper makes this one-liner-safe.
    fn visit(&mut self, node: &PlanNode) -> Self::Output;
}

/// Mutating variant for in-place analysis (e.g. schema enrichment).
pub trait PlanVisitorMut {
    type Output;
    fn visit_mut(&mut self, node: &mut PlanNode) -> Self::Output;
}
```

### 14.2 Walk / transform free functions

```rust
impl PlanNode {
    /// Pre-order traversal: visitor sees each node before its children.
    pub fn walk_pre<V: PlanVisitor>(&self, v: &mut V) -> V::Output;

    /// Post-order traversal: visitor sees each node after all children.
    pub fn walk_post<V: PlanVisitor>(&self, v: &mut V) -> V::Output;

    /// Bottom-up rewrite: each node is rewritten after its children.
    /// Propagates `Err` if the rewrite function fails on any node.
    /// Bare-kind shape per `31 §3.1` construction-site convention; callers
    /// wrap into `Diagnostic<IrErrorKind>` at the stage boundary if needed.
    pub fn transform<F>(self, f: F) -> Result<PlanNode, IrErrorKind>
    where F: FnMut(PlanNode) -> Result<PlanNode, IrErrorKind>;

    /// Iterator-style child access; used by generic tree algorithms.
    pub fn children(&self) -> impl Iterator<Item = &PlanNode>;
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut PlanNode>;
}
```

Wrapper-level delegation: `SemanticPlan::walk_pre` / `::walk_post` / `::transform` call the corresponding `PlanNode` method on `root`, returning any diagnostic the rewrite surfaces alongside the transformed tree.

### 14.3 `SemanticPlan::validate`

```rust
impl SemanticPlan {
    /// Full tree walk; re-checks every invariant in §13. Returns the
    /// first violation as a `Diagnostic<IrErrorKind>` (stage-boundary shape:
    /// every violation has a definite plan-tree location); `Ok(())` on
    /// well-formedness.
    ///
    /// Intended use: debug / test harnesses, planner-regression
    /// catches, audit tools. Production callers rely on the planner's
    /// well-formedness guarantee and SHOULD NOT validate on every plan.
    pub fn validate(&self) -> Result<(), Diagnostic<IrErrorKind>>;
}
```

### 14.4 Typical usage patterns

**Count nodes of a variant.** Implement `PlanVisitor` with `Output = ()` and a counter field; let the default descend-children implementation do the work.

**Extract all `Scan`'s sources.** Implement `PlanVisitor` collecting `&SourceRef` from every `PlanNode::Scan(ScanNode { source, .. })`.

**Push-down rewrite.** Implement `transform` with a closure that matches `Filter { input: box Scan { .. }, predicate }` and rebuilds the subtree with the predicate in `filters_pushdown`.

**Schema re-check.** Implement `PlanVisitor<Output = Result<(), Diagnostic<IrErrorKind>>>` that recomputes `output_schema` for each variant and compares to `meta().output_schema`; return the first mismatch.

**Generic expression rewrite reuse.** Because `PlanNode` and `Expr<L>` share the `Tree` trait surface (§3.2), an optimizer rule that, say, constant-folds an `Expr<L>` subtree can use the same `transform` helper to rewrite an entire `PlanNode` subtree — one trait, two scales.

## 15. Serde / Substrait Mapping

### 15.1 Serde

Every public IR type derives `Serialize` / `Deserialize` under the crate-level `serde` feature flag (§17). `SemanticPlan` is the intended portable form: a serialized plan can be round-tripped across processes sharing the same SemanticManifest. Wire-form stability rules:

- Every `#[non_exhaustive]` enum serializes with a `kind` discriminator field (serde-tagged). Adding a variant preserves round-trip of existing variants.
- Every `#[non_exhaustive]` struct serializes with absent-field-tolerant deserialization. Adding a field preserves round-trip of existing values (new field defaults to its `Default::default` or `None`).
- `PhysicalExpr` and `SemanticExpr` serialize through the `Expr<L>` `#[derive(Serialize, Deserialize)]` machinery owned by this crate (§3.3). `PlanNode` and `Expr<L>` are the two `semstrait-ir`-owned `#[non_exhaustive]` enums that require serde-tagged discriminator wire form; everything else uses direct `#[derive(...)]`.
- **Outbound-only Substrait emission (Q4.C, 2026-05-21).** The `Serialize` half of every IR type is fully populated for the canonical wire form. The `Deserialize` half is for **IR-internal round-trip** only — process-to-process plan transport between same-version `semstrait` peers, or test fixtures. It is NOT a contract for inbound conversion from arbitrary `substrait::proto::Plan` values produced outside `semstrait`. Cross-engine inbound deserialization is out of scope for v1; if needed in a future version, it lands as a separate `36`-side concern, not in `35`.

### 15.2 Substrait mapping table

The adapter crate (`36`) owns the **outbound** conversion from `SemanticPlan` to `substrait::proto::Plan`. `35` ratifies the **mapping** so `36`'s emitter and the crate's outbound round-trip tests agree on which `substrait::proto::Rel` corresponds to which `PlanNode`.

Per Q4.C (2026-05-21): the table is **unidirectional (outbound only)**. Inbound conversion (`substrait::proto::Plan` → `SemanticPlan`) is not a v1 contract; arbitrary external Substrait plans are not accepted as a `SemanticPlan` source.

| `PlanNode` variant | Substrait `Rel` kind           | Notes                                                                                                                                            |
|--------------------|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------|
| `Scan`             | `ReadRel`                      | `source` resolves to `ReadRel.read_type` via the adapter's SemanticManifest lookup. `filters_pushdown` → `ReadRel.filter` (one conjunction).             |
| `Filter`           | `FilterRel`                    | `predicate` → `FilterRel.condition`.                                                                                                             |
| `Project`          | `ProjectRel`                   | `projections` → `ProjectRel.expressions` (order-preserving). Output names carried in `RelRoot.names` at the plan root.                           |
| `Agg`              | `AggregateRel`                 | `group_by` → one `Grouping` with the referenced columns as `grouping_expressions`. `aggregates` → `AggregateRel.measures`.                       |
| `Join`             | `JoinRel`                      | `join_type` → `JoinRel.type`. `on` → `JoinRel.expression` (equijoin with conjunction of `KeyPair` equalities). `cardinality` → `AdvancedExtension.enhancement` with URN `urn:semstrait:join-cardinality:v1`. |
| `Union`            | `SetRel` with `op = UNION` / `UNION_DISTINCT` | `distinct = false` → `SET_OP_UNION_ALL`; `distinct = true` → `SET_OP_UNION_DISTINCT`.                                               |
| `Sort`             | `SortRel`                      | `order[*].0` → `SortField.expr` (resolved to the referenced column). `order[*].1` → `SortField.direction`.                                       |
| `Fetch`            | `FetchRel`                     | `limit` / `offset` → `FetchRel.count` / `FetchRel.offset` (`-1` when `None`).                                                                    |

The adapter is free to emit Substrait proto plans with extra hints (capacity, parallelism) in `AdvancedExtension.enhancement` slots; those hints are adapter-owned and not round-tripped through `35`.

### 15.3 Annotation Substrait carrier

1. **Carrier.** Every `SemAnnotation` on a `PlanNode` emits into `RelCommon.advanced_extension.optimization[]` of the corresponding `substrait::proto::Rel`, namespaced by URN `urn:semstrait:annotations:v1`; the `enhancement` slot is reserved for adapter execution hints and MUST NOT carry annotations.
2. **Roundtrip.** Binary `Plan.encode_to_vec` is fully roundtrip-safe; `36`'s emitter collects every annotation URN into the plan-root `Plan.expected_type_urls`. JSON emission is best-effort (proto3 JSON loses `Any` payload fidelity) and MAY elide annotations.
3. **Drop-safe.** Per Substrait `optimization[]` semantics, any consumer MAY discard entries it does not recognize; consumers MUST behave correctly with annotations stripped.

## 16. Error Types

### 16.1 `ValidateError` — raised by trait machinery

```rust
/// Construction-time invariants raised by `Tree::with_new_children`
/// and `Rewriter<N>::f_down` / `f_up` callbacks. Per `14 §3.1`.
///
/// Identification is by variant identity per `30 §5.4`.
#[non_exhaustive]
pub enum ValidateError {
    /// `with_new_children` received an arity that does not match the
    /// node's variant tag (e.g. a `BinaryOp` reconstructed with three
    /// children, an `Aggregate` reconstructed with zero args).
    StructuralArityMismatch { node_kind: &'static str, expected: ArityRange, got: usize },

    /// `Aggregate` authored outside an aggregate-admitting site
    /// per `[14 §7](../foundations/14_expressions.md)`.
    AggregateInScalarContext { location: Location },

    /// A rewriter callback produced a structurally invalid subtree.
    RewriteInvariantViolated { reason: String },
}

impl Diagnose for ValidateError { /* per-variant message / severity */ }
```

`ValidateError` is the construction-boundary diagnostic. Downstream stages embed via D.ii kind-nesting (`[30 §7.4](30_api_contracts.md)`): `model::ValidateError` carries an `Ir(ir::ValidateError)` variant for invariants raised during parse-time tree construction; the equivalent re-emission boundary is per `[32 §9.5](32_semstrait_model.md)`.

### 16.2 `CompileError` — raised by `FunctionSpec` machinery

```rust
/// Function-resolution diagnostic raised by `ReturnTypeRule::Custom`
/// callbacks wired into `FunctionSpec` (§8). Per `14a §3.5`.
///
/// Identification is by variant identity per `30 §5.4`.
#[non_exhaustive]
pub enum CompileError {
    /// `ReturnTypeRule::Custom` callback declined to produce a return
    /// type for the supplied argument types.
    CustomRuleRejected { fn_name: CanonicalFn, args: Vec<DataType>, reason: String },

    /// A registered `FunctionSpec` failed its own internal consistency
    /// check (e.g. signature overlaps another in the same registry).
    SpecInconsistent   { fn_name: CanonicalFn, reason: String },
}

impl Diagnose for CompileError { /* per-variant message / severity */ }
```

`CompileError` is the narrow function-resolution diagnostic. The wider compile-stage error surface (unknown references, ambiguous paths, cycles, type-inference failures, …) lives in `semstrait-manifest::CompileError` per `[33 §10](33_semstrait_manifest.md)` and embeds `Ir(ir::CompileError)` via D.ii kind-nesting (`[30 §7.4](30_api_contracts.md)`).

### 16.3 `IrErrorKind`

```rust
/// Typed-kind enum for `semstrait-ir`'s own operations: plan-tree
/// construction (`Name` validation), plan walking (`transform`
/// failures), plan validation (`validate`), and adapter-artifact
/// serialization (`EnginePlan::to_bytes` / `to_json`).
///
/// Per `30 §5`. Identification is by variant identity (`matches!`);
/// there is no string-code accessor.
#[non_exhaustive]
pub enum IrErrorKind {
    /// `Name::new` was called with an empty or invalid identifier.
    InvalidName        { supplied: String, reason: String },

    /// `Name::new` was called with a reserved plan-tree prefix (§11.4.1).
    ReservedName       { supplied: String, prefix: String },

    /// A `PhysicalExpr` reaching the plan tree lacks `inferred_type`.
    /// Only reported by `SemanticPlan::validate`.
    UnresolvedType     { location: String, expr_sketch: String },

    /// Two `KeyPair` columns have incompatible types on a `JoinNode`.
    /// Only reported by `SemanticPlan::validate`.
    JoinKeyTypeMismatch{ pair: KeyPair, left_ty: DataType, right_ty: DataType },

    /// `AggNode.aggregates` contains a duplicate output name.
    /// Only reported by `SemanticPlan::validate`.
    DuplicateAggName   { name: Name },

    /// `FilterNode.predicate` has non-Boolean type.
    /// Only reported by `SemanticPlan::validate`.
    FilterPredicateNotBoolean { actual: DataType },

    /// A `PlanNode`'s `meta.output_schema` disagrees with the schema
    /// computed from its children. Only reported by
    /// `SemanticPlan::validate`.
    SchemaMismatch     { node_kind: &'static str, expected: String, got: String },

    /// `UnionNode.inputs` schemas are not structurally compatible.
    UnionSchemaMismatch{ input_ix: usize, expected: String, got: String },

    /// `UnionNode.inputs.len() < 2`.
    UnionArityTooLow   { arity: usize },

    /// A `Name` referenced by `group_by` / `order` / `on` does not
    /// resolve to a column in the input schema.
    UnresolvedColumnRef{ name: Name, available: Vec<Name> },

    /// A `FetchNode.limit` / `FetchNode.offset` value is out of the
    /// adapter's representable range (typically `i64::MAX` for Substrait).
    FetchValueOutOfRange { field: &'static str, value: u64 },

    /// A `transform` / `walk` callback returned an error.
    TransformFailure   { reason: String },

    /// `EnginePlan::to_bytes` / `to_json` failed; wraps the underlying
    /// `prost::EncodeError` / `serde_json::Error` context as a string.
    ArtifactSerializationFailed { reason: String },

    /// A visitor-side invariant was violated (e.g. a transform produced
    /// a structurally invalid subtree that immediate post-check caught).
    TransformInvariantViolated  { reason: String },
}

impl Diagnose for IrErrorKind {
    fn message(&self) -> String { /* per-variant human text */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}
```

`IrErrorKind` is owned by `semstrait-ir`. It is distinct from `ir::CompileError` (§16.2, raised by `FunctionSpec` machinery), `ir::ValidateError` (§16.1, raised by trait machinery), and the downstream `manifest::CompileError` / `PlanErrorKind` / `AdaptErrorKind` — each has a different production site and a different lifecycle. A planner-side failure producing a malformed `SemanticPlan` is a `PlanErrorKind` (`34`); that same malformed plan caught by `SemanticPlan::validate()` on the consumer side becomes an `IrErrorKind`. All four surface as `Diagnostic<K>` envelopes for the appropriate `K` per `30 §5.1`.

### 16.4 Variant identity, not codes

The retired `IR_E_*` numeric range from earlier drafts is gone. Identification is by variant identity per `30 §5.4`; renaming or removing a variant is MAJOR per `30 §2.1`; adding a variant inside `#[non_exhaustive]` is MINOR per `30 §2.2`. The §2 module layout allocates a dedicated `error` module that co-locates `ValidateError`, `CompileError`, and `IrErrorKind` next to their production sites.

### 16.5 Warning posture

`semstrait-ir` itself, being pure data + validation, has no warning-emitting operation in v1. Warnings surfaced by planner or optimizer ride `PlanErrorKind` / `OptimizeErrorKind` envelopes per `34`; adapter warnings ride `AdaptErrorKind` per `36`. If a future v2 walker emits `Severity::Warning` `Diagnostic<IrErrorKind>` (e.g. an "unused PlanNode" advisory), it lands additively under `#[non_exhaustive]` per `30 §2.2`.

## 17. Stability

### 17.1 Stable parts

- **`PlanNode` variant set growth is non-breaking (I10).** Adding a variant (e.g. a future `Distinct`, `Window`, `Unnest`, `TopN`) is MINOR. Consumers that pattern-match exhaustively on `PlanNode` will compile-error by design — the `#[non_exhaustive]` attribute forces them to add a fallback arm.
- **Struct field addition inside a `PlanNode` variant is non-breaking.** Every variant's struct is `#[non_exhaustive]` per §10; adding a new field with a sensible default (`None`, `Vec::new()`, `0`, `false`) is MINOR. Examples: `JoinNode.residual: Option<PhysicalExpr>` for non-equi joins; `ScanNode.order_hint: Option<Vec<(Name, SortDir)>>` for order-preserving scans.
- **`Expr<L>` structural-variant additions are non-breaking** under the `14 §3.3` `#[non_exhaustive]` discipline. Adding e.g. a `Try` / `Filter` / `Match` variant in a future spec rev is MINOR; consumers pattern-matching `Expr<L>` must add a fallback arm.
- **`PhysicalLeaf` / `SemanticLeaf` variant additions are non-breaking** per the same discipline.
- **Per-kind accessor enum variant additions are non-breaking** (each accessor enum is `#[non_exhaustive]`).
- **`ParameterKey` variant additions are non-breaking** — adding new internal parameter keys for future sugar-elimination patterns is MINOR.
- **`FunctionRegistry` content growth is non-breaking** — new entries exposed via `RegistryExtension` impls at startup are part of the registry's runtime state, not its type surface.
- **`DialectId` const additions are non-breaking.** Adding a new `pub const` on `DialectId` is MINOR.
- **`SemAnnotation` variant additions are non-breaking** (annotation roster growth is expected as `34` matures).
- **Variant additions to `IrErrorKind` inside `#[non_exhaustive]` are non-breaking** per `30 §2.2`.
- **Substrait mapping table entries are non-breaking** — adding a new `PlanNode` variant with a corresponding Substrait `Rel` kind is MINOR; changing an existing mapping is MAJOR.

### 17.2 Internal parts

- **`NodeMeta.node_id` values** are not stable across planner invocations. Consumers relying on stable identity across runs should derive identity from the plan-tree shape (e.g. a tree-hash visitor), not from `node_id`.
- **`SemanticPlan::validate()`'s error-ordering** is not stable. The first violation reported may shift between releases as `validate` reorders its checks for performance; consumers SHOULD treat any `Diagnostic<IrErrorKind>` as a single bad-plan signal, not a "first problem is X" guarantee.
- **Serde's on-wire shape under `#[non_exhaustive]` enums** follows the serde-tagged convention (§15.1). The exact JSON spelling of a `kind` discriminator is stable across MINOR releases; deserializers MUST be tolerant to unknown variant tags (typically mapping unknowns to a skipped-node error rather than panicking).

### 17.3 Delta with current code

- `LogicalPlan` → rename to `SemanticPlan`.
- Local `JoinType` / `SortDirection` enums → drop in favor of canonical `[16 §5.2](../foundations/16_composition_and_relationships.md)` `JoinType` (re-exported via `semstrait-common`) and `SortDir` + `NullOrdering` (§11.6).
- Every `pub enum` in `plan/` → add `#[non_exhaustive]`.
- `ScanNode.location` / `ScanNode.format` → drop in favor of opaque `SourceRef` (§11.2); path + format resolution moves to `36`.
- Add `filters_pushdown: Vec<PhysicalExpr>` to `ScanNode`.
- Move `Expr` / `SemanticExpr` / `PhysicalExpr` / `ExprSource` from `semstrait-common` to `semstrait-ir` under new `expr` (with `tree`, `leaves`, `accessor`, `parameter`, `expr_fn` submodules) and `functions` modules.
- Replace flat-enum `Expr` + wrapper-struct pattern with parameterized `Expr<L>` + type aliases per `[14 §3.6](../foundations/14_expressions.md)`; retire wrapper `inferred_type` / `referenced_columns` fields in favor of `ExprLeaf::inferred_type()` + `ResolvedExprEntry` (per `[19 §3.2](../foundations/19_expression_flow.md)`).
- Replace retired `SemanticLeaf::EntityRef` / `Access` / outer `Accessor` with per-kind typed leaves + `Option<XxxAccessor>` fields per `[14 §3.5](../foundations/14_expressions.md)` / `[§4.1](../foundations/14_expressions.md)`.
- Move `CanonicalFn`, `FunctionRegistry`, `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`, `RegistryExtension`, `function_registry()` to `semstrait-ir::functions` per `[14a §2](../foundations/14a_function_catalog.md)`.
- Add authoring-surface DSL (`expr_fn`, `std::ops` impls, `ExprFunctionExt`) per §7.

## 18. Crate Boundaries

### 18.1 What `semstrait-ir` does NOT do

- **No verbs.** No `plan`, no `resolve`, no `optimize`, no `adapt`. Planning lives in `[34](34_semstrait_planner.md)`; compile-time resolution in `semstrait-manifest::compile` per `[19 §3](../foundations/19_expression_flow.md)`; optimizer passes in `[34 §5](34_semstrait_planner.md)`; adapter emission in `[36](36_semstrait_adapter.md)`.
- **No SemanticManifest construction.** `35` consumes `SourceRef` handles into an external SemanticManifest but never constructs one; that lives in `[33](33_semstrait_manifest.md)`.

### 18.2 Dependency posture

```toml
[dependencies]
semstrait-common = { path = "../semstrait-common" }
thiserror      = "^"
prost          = "^"
substrait      = "^"   # substrait::proto::Plan for EnginePlan::Substrait
uuid           = { version = "^", features = ["v4"] }

[dependencies.serde]
version  = "^"
optional = true
features = ["derive"]

[features]
default = []
serde   = ["dep:serde", "semstrait-common/serde"]
```

**No runtime-only dependencies.** No `tokio`, `async-trait`, `futures`, `reqwest`, `hyper`, `sqlx`.
**No engine dependencies.** `substrait` (a proto codegen crate) is permitted because `EnginePlan::Substrait(Box<substrait::proto::Plan>)` is structural, not engine-identity. No `datafusion`, no `arrow`, no `duckdb`, no `spark-*`.
**No in-workspace dependencies beyond `semstrait-common`.** CI-enforced manifest audit per `30 §9`.

## 19. Invariants Upheld by the Crate

| Invariant | `semstrait-ir` guarantee |
|---|---|
| **I1** — no raw SQL in canonical layer | `PlanNode` variants carry `PhysicalExpr` for every predicate; `Name` for every column / key identifier; `SourceRef` (opaque) for every source. `Expr<L>` and its leaf sets are typed trees — no `String`-as-SQL field exists on any structural variant or leaf. `SqlArtifact.text` exists, but it is an *adapter output*, not a *plan content*. |
| **I2** — physical types belong to adapters | `SemanticPlan` and `Expr<L>` reference only `DataType` (canonical, owned at §4). `EnginePlan::Substrait(Box<substrait::proto::Plan>)` carries engine-specific types, but it is an *adapter output*, not an input to or content of a `SemanticPlan`. |
| **I3** — no engine-identity branching in canonical types | `PlanNode` has zero variants keyed by adapter / dialect. `Expr<L>::FunctionCall` carries `CanonicalFn` (engine-neutral) — engine-specific operators land as registry-extension entries per `[14a §7](../foundations/14a_function_catalog.md)`, not as new `Expr<L>` variants. The only engine-identity value anywhere in `semstrait-ir` is `DialectId`, and it appears only on `SqlArtifact` (adapter output) / `Dialect::ID` (adapter-trait associated constant). |
| **I5** — name resolution at compile time | `SemanticLeaf::Field` / `Dimension` / `Measure` / `Metric` / `Key` carry unresolved names at parse and are resolved at compile per `[19 §3](../foundations/19_expression_flow.md)`. `PhysicalLeaf` carries no semantic names, only binding-resolved `ColumnRef`s and compile-emitted `Parameter`s. The leaf-set boundary makes the "no semantic refs in PhysicalExpr" rule a type-level invariant per `[14 §3.7](../foundations/14_expressions.md)`. |
| **I6** — plan hot path is synchronous | **No `pub async fn` exists on `semstrait-ir`.** Every method on every public type — including `Expr<L>` traversal, `FunctionRegistry` lookup, `PlanNode` walking, and `SemanticPlan::validate` — is synchronous. CI lint + `forbid_async_fn!` macro audit guard the crate. |
| **I7** — strict DAG | `Cargo.toml` lists `semstrait-common` as the only internal workspace dependency. CI check greps for any other `semstrait-*` entry. The expression types + registry absorbed from `semstrait-common` at the `14` second-refinement landing do not change the DAG — `semstrait-common` remains the workspace leaf carrying primitives + trait scaffolding + support enums. |
| **I10** — extensibility | Every `pub enum` and `pub struct` carries `#[non_exhaustive]` except the newtype-over-stable set: `Name`, `SourceRef`, `DialectId`, `NodeId`, `CanonicalFn`. An `integration-test` over `cargo public-api` enforces the rule. |
| **I11** — no downward I/O surprises | No `std::fs`, no `std::net`, no `tokio`, no `reqwest` anywhere in the crate. `substrait`'s `prost` dependency is bytes-encoding only, not I/O. |
| **I12** — first-class diagnostics | `Diagnose` implemented on `IrErrorKind`, `ValidateError`, and `CompileError` per `30 §5.4`; identification is by variant identity. The blanket `Display` and `std::error::Error` impls on `Diagnostic<K>` (per `30 §5.5`) make `Diagnostic<IrErrorKind>` directly usable as a `std::error::Error` value. Registry-side construction-time errors raised by trait / `FunctionSpec` machinery flow through `ir::ValidateError` / `ir::CompileError` (§16.1 / §16.2). |

## 20. Public API Surface Sketch

### 20.1 `types`

```
pub enum   DataType                                      // §4.1
pub enum   Grain                                         // §4.2
pub enum   TypeClass                                     // §4.3
pub struct Schema                                        // §4.4
pub struct SchemaColumn                                  // §4.4
```

### 20.2 `expr`

```
pub enum   Expr<L: ExprLeaf>                             // §3.3
pub type   PhysicalExpr = Expr<PhysicalLeaf>             // §5.3
pub type   SemanticExpr = Expr<SemanticLeaf>             // §5.3

pub enum   PhysicalLeaf                                  // §5.1
pub enum   SemanticLeaf                                  // §5.2

pub enum   DimensionAccessor                             // §6.1
pub enum   MeasureAccessor                               // §6.1
pub enum   MetricAccessor                                // §6.1
pub enum   KeyAccessor                                   // §6.1

pub struct Parameter                                     // §6.2
pub enum   ParameterKey                                  // §6.2

pub mod expr_fn {                                        // §7.1
    pub trait FromColumnRef;
    pub fn col<E: FromColumnRef>(name: impl Into<String>) -> E;
    pub fn field(name: impl Into<String>) -> SemanticExpr;
    pub fn dim(name: impl Into<String>) -> SemanticExpr;
    pub fn measure(name: impl Into<String>) -> SemanticExpr;
    pub fn metric(name: impl Into<String>) -> SemanticExpr;
    pub fn key(name: impl Into<String>) -> SemanticExpr;
}

pub trait ExprFunctionExt                                // §7.3
impl      ExprFunctionExt for SemanticExpr
impl      ExprFunctionExt for PhysicalExpr

// std::ops impls per §7.2
impl<L: ExprLeaf> Add | Sub | Mul | Div | Rem | BitAnd | BitOr | Neg | Not for Expr<L>
```

### 20.3 `functions`

```
pub struct CanonicalFn                                   // §8.2
pub struct FunctionRegistry                              // §8.2
pub struct FunctionSpec                                  // §8.2
pub struct FnSignature                                   // §8.2
pub enum   ParamType                                     // §8.2
pub enum   ReturnTypeRule                                // §8.2
pub enum   FunctionCategory                              // §8.2
pub trait  RegistryExtension                             // §8.2
pub fn     function_registry() -> &'static FunctionRegistry;  // §8.2
```

### 20.4 `plan`

```
pub struct SemanticPlan                                  // root; { root, output_names, diagnostics }
pub enum   PlanNode                                      // 8 variants per §10
pub struct ScanNode                                      // §10.2
pub struct FilterNode                                    // §10.3
pub struct ProjectNode                                   // §10.4
pub struct AggNode                                       // §10.5
pub struct JoinNode                                      // §10.6
pub struct UnionNode                                     // §10.7
pub struct SortNode                                      // §10.8
pub struct FetchNode                                     // §10.9
pub struct NodeMeta                                      // §11.1
pub struct NodeId                                        // newtype over Uuid
pub enum   SemAnnotation                                 // #[non_exhaustive]; AggregateRole, FilterSource, ...
```

### 20.5 `plan::traversal`

```
pub trait  PlanVisitor                                   // visit(&PlanNode) -> Self::Output
pub trait  PlanVisitorMut                                // visit_mut(&mut PlanNode) -> Self::Output
```

### 20.6 `primitives`

```
pub struct SourceRef                                     // opaque handle; §11.2
pub struct ResolvedColumn                                // §11.3
pub struct Name                                          // newtype over String; §11.4
pub struct KeyPair                                       // §11.5
pub enum   SortDir                                       // Asc | Desc with NullOrdering; §11.6
pub enum   NullOrdering                                  // First | Last | Unspecified
pub struct AggregateExpr                                 // §11.7
pub use    semstrait_common::{Cardinality, JoinType}       // re-exported from 16 §5 per `authoritative-for`
```

### 20.7 `artifact`

```
pub enum   EngineArtifact                                // Sql | Plan
pub enum   EnginePlan                                    // Substrait
pub struct SqlArtifact                                   // { text, dialect }
pub struct DialectId                                     // newtype; ANSI | DATAFUSION | DUCKDB | SPARK
pub trait  Dialect                                       // ID const + capabilities()
pub enum   Capability                                    // #[non_exhaustive]; roster owned by 36
```

### 20.8 `error`

```
pub enum   ValidateError                                 // raised by Tree::with_new_children + Rewriter<N>::f_*
pub enum   CompileError                                  // raised by ReturnTypeRule::Custom callbacks
pub enum   IrErrorKind                                   // plan-shape diagnostics; 14 variants in v1
impl       semstrait_common::diagnostic::Diagnose for ValidateError
impl       semstrait_common::diagnostic::Diagnose for CompileError
impl       semstrait_common::diagnostic::Diagnose for IrErrorKind
```

### 20.9 Free functions / inherent impl methods at crate root

```
impl<L: ExprLeaf> Tree for Expr<L> {
    fn children(&self) -> Vec<&Self>;
    fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateError>;
}

impl PlanNode {
    pub fn meta(&self) -> &NodeMeta;
    pub fn meta_mut(&mut self) -> &mut NodeMeta;
    pub fn children(&self) -> impl Iterator<Item = &PlanNode>;
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut PlanNode>;
    pub fn walk_pre<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn walk_post<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<PlanNode, IrErrorKind>
    where F: FnMut(PlanNode) -> Result<PlanNode, IrErrorKind>;
}

impl SemanticPlan {
    pub fn validate(&self) -> Result<(), Diagnostic<IrErrorKind>>;
    pub fn walk_pre<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn walk_post<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<SemanticPlan, IrErrorKind>
    where F: FnMut(PlanNode) -> Result<PlanNode, IrErrorKind>;
}

impl EnginePlan {
    pub fn to_bytes(&self) -> Result<Vec<u8>, IrErrorKind>;
    pub fn to_json(&self) -> Result<String, IrErrorKind>;
}
```

### 20.10 Crate-root re-exports

```rust
// lib.rs
pub use crate::types::{
    DataType, Grain, TypeClass, Schema, SchemaColumn,
};
pub use crate::expr::{
    Expr, PhysicalExpr, SemanticExpr, PhysicalLeaf, SemanticLeaf,
    DimensionAccessor, MeasureAccessor, MetricAccessor, KeyAccessor,
    Parameter, ParameterKey,
    expr_fn,
    ExprFunctionExt,
};
pub use crate::functions::{
    CanonicalFn, FunctionRegistry, FunctionSpec, FnSignature,
    ParamType, ReturnTypeRule, FunctionCategory, RegistryExtension,
    function_registry,
};
pub use crate::plan::{
    SemanticPlan, PlanNode, ScanNode, FilterNode, ProjectNode, AggNode,
    JoinNode, UnionNode, SortNode, FetchNode,
    NodeMeta, NodeId, SemAnnotation,
    traversal::{PlanVisitor, PlanVisitorMut},
};
pub use crate::primitives::{
    SourceRef, ResolvedColumn, Name, KeyPair, SortDir, NullOrdering,
    AggregateExpr,
};
pub use crate::artifact::{
    EngineArtifact, EnginePlan, SqlArtifact, DialectId, Dialect, Capability,
};
pub use crate::error::{IrErrorKind, ValidateError, CompileError};
pub use crate::tree::{Tree, Visitor, Rewriter, ExprLeaf};
pub use crate::expr_kinds::{
    BinaryOpKind, UnaryOpKind, AggregationOp, LikeKind, CastFailure,
    WindowFn, WindowFrame, WindowFrameKind, WindowBound,
    Literal, ColumnRef, SemanticsName,
};

// Re-exports from semstrait-common that `35`-authoritative surfaces rely on:
pub use semstrait_common::{
    Cardinality, JoinType,
};
```

