---
prereqs: [13, 14, 14a, 16, 17]
authoritative-for:
  - the `semstrait-ir` public-API surface (types, traits, free functions)
  - the universal-traversal trait family — `Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf` (variant ratified by `14 §3.1 / §3.2`; `35` is the crate-level home post-second-cascade)
  - the `Expr<L>` structural enum implementation (variant catalog ratified by `14 §3.3`; `35` carries the crate-level home)
  - the structural-variant support enums shared by every leaf set — `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`, `Literal` (rosters ratified by `14 §3.3`; `35` is the crate-level home post-second-cascade)
  - the shared identifier carriers `ColumnRef` and `SemanticsName` (consumed by both leaf sets per `14 §3.4 / §3.5`; `35` is the crate-level home post-second-cascade)
  - the `PhysicalLeaf` and `SemanticLeaf` enums — canonical-IR leaf set and per-kind typed semantic leaf set per `14 §3.4 / §3.5`
  - the `PhysicalExpr = Expr<PhysicalLeaf>` and `SemanticExpr = Expr<SemanticLeaf>` type aliases per `14 §3.6`
  - the per-kind accessor enums (`DimensionAccessor`, `MeasureAccessor`, `MetricAccessor`, `KeyAccessor`) carried as `Option<…>` fields on the typed semantic leaves per `14 §4.1`
  - the `Parameter` placeholder struct and the closed `ParameterKey` enum per `14 §5`
  - the authoring-surface DSL — the `expr_fn` module with `col`, `field`, `dim`, `measure`, `metric`, `key` free constructors; `std::ops` impls on `SemanticExpr` and `PhysicalExpr`; the `ExprFunctionExt` extension trait — per `14 §9.2`
  - the `CanonicalFn` newtype and the `FunctionRegistry` / `FunctionSpec` / `FnSignature` / `ParamType` / `ReturnTypeRule` / `FunctionCategory` / `RegistryExtension` / `function_registry()` surface, moved from `semstrait-core` per `14a §2`
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

> **Status:** second cascade landing (2026-05-19). Full expression-vocabulary absorption per the Option A direction (`STATUS.md` item Q): in addition to the first-cascade items already absorbed (`Expr<L>`, leaf sets, accessor enums, `Parameter`, `expr_fn` DSL, `CanonicalFn` / `FunctionRegistry`), `semstrait-ir` now also OWNS the **trait scaffolding** (`Tree`, `Visitor<N>`, `Rewriter<N>`, `ExprLeaf`), the **structural-variant support enums** (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`, `Literal`), the **identifier carriers** (`ColumnRef`, `SemanticsName`), and the **narrow ir-emitted error kinds** (`ValidateError`, `CompileError`) previously held by `semstrait-core`. Every variant catalog and type-architecture invariant exposed here is ratified upstream in `14`, `14a`, or `16`; `35` adds no new rosters or structural rules — it implements the shapes those chapters own. It ratifies the **plan-tree shape**, the **expression-type ownership at the crate level**, the **adapter-artifact shape**, and the **well-formedness invariants** the planner guarantees and the adapter relies on.

## 1. Purpose and Scope

`semstrait-ir` is the **canonical IR crate**. It carries every type the post-compile pipeline operates on: the engine-agnostic expression types (`Expr<L>` + both leaf sets), the function-identity catalog (`CanonicalFn` + `FunctionRegistry`), the in-memory plan tree (`SemanticPlan`), and the adapter-consumable output types. The producer side is split — `semstrait-model` parses YAML into the expression types, `semstrait-manifest::compile` resolves `SemanticExpr` into `PhysicalExpr` per `[19 §3](../foundations/19_expression_flow.md)`, and `semstrait-planner` (`34`) builds `SemanticPlan` from `Request × SemanticManifest`. The consumer side is the adapter family (`semstrait-adapter`, `36`+). No other crate in the workspace contains a plan-tree, expression-type, or function-registry vocabulary.

### 1.1 What `semstrait-ir` OWNS

- The **universal-traversal trait family** (§3.2): the `Tree` trait — implemented here by `Expr<L>` (§3.3) and `PlanNode` (§9) — and its `Visitor<N>` / `Rewriter<N>` / `ExprLeaf` companions. Per `[14 §3.1](../foundations/14_expressions.md)` / `[§3.2](../foundations/14_expressions.md)`. Moved from `semstrait-core` at the second cascade (`STATUS.md` item Q).
- The canonical-IR **expression types** (§3–§5): the `Expr<L>` structural enum implementation, the `PhysicalLeaf` and `SemanticLeaf` enums, the `PhysicalExpr` / `SemanticExpr` type aliases, the per-kind accessor enums (`DimensionAccessor`, `MeasureAccessor`, `MetricAccessor`, `KeyAccessor`), and the `Parameter` placeholder + `ParameterKey` closed enum. The variant catalogs and structural invariants are ratified by `[14 §3](../foundations/14_expressions.md)` and `[14 §4](../foundations/14_expressions.md)`; `35` is the *crate* that holds the implementation.
- The **structural-variant support enums** shared by every `Expr<L>` instantiation (§3.4): `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `WindowFrameKind`, `WindowBound`. Rosters per `[14 §3.3](../foundations/14_expressions.md)`. Moved from `semstrait-core` at the second cascade.
- The **typed-literal carrier** `Literal` (§3.4) — single value type shared by `PhysicalLeaf::Literal` and `SemanticLeaf::Literal`. Per `[14 §3.3](../foundations/14_expressions.md)`. Moved from `semstrait-core` at the second cascade.
- The **shared identifier carriers** referenced by both leaf sets (§3.4): `ColumnRef` and `SemanticsName`. Moved from `semstrait-core` at the second cascade.
- The **authoring-surface DSL** (§6): the `expr_fn` module with the six canonical free-function constructors (`col`, `field`, `dim`, `measure`, `metric`, `key`), `std::ops` impls on `SemanticExpr` and `PhysicalExpr` for operator overloading, and the `ExprFunctionExt` extension trait for comparison / predicate / aggregate / accessor builder methods. Per `[14 §9.2](../foundations/14_expressions.md)`.
- The **`CanonicalFn` newtype** and the **`FunctionRegistry` surface** (§7): `FunctionRegistry`, `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`, `RegistryExtension` trait, `function_registry()` accessor. Per `[14a §2](../foundations/14a_function_catalog.md)` and the `[14 §9.2](../foundations/14_expressions.md)` placement contract; `35` is the registry's owning crate.
- The **narrow ir-emitted error kinds** (§15): `ValidateError` (raised by `Tree::with_new_children` and `Rewriter<N>::f_*`) and `CompileError` (raised by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec`). Each implements `Diagnose` per `[30 §5](30_api_contracts.md)`; downstream stages MAY embed via D.ii kind-nesting (`[30 §7.4](30_api_contracts.md)`). Moved from `semstrait-core` at the second cascade. **Naming note:** the `Kind` suffix is dropped per the scoped error-naming cleanup tied to this move (`STATUS.md` item Q); broader `*ErrorKind` rename remains deferred.
- The `SemanticPlan` root type (§8) and the `PlanNode` sum type (§9) — every variant, its fields, and the well-formedness invariants its children must satisfy (§12).
- Plan-level primitive types (§10): `SourceRef`, `ResolvedColumn`, `Name`, `KeyPair`, `SortDir`, `NullOrdering`, `AggregateExpr`, `NodeMeta`.
- The adapter-consumable output family (§11): `EngineArtifact`, `EnginePlan`, `SqlArtifact`, `DialectId`, `Dialect`. `36` refines *emission* (how an adapter fills them in); `35` ratifies their *shape*.
- The visitor / traversal API over `PlanNode` trees (§13). The same `Tree` / `Visitor` / `Rewriter` traits owned here (§3.2) drive both `Expr<L>` and `PlanNode` traversal; `35` exposes one unified trait surface across both shapes (§3.2 / §13.1).
- Serde derivations for `SemanticPlan`, `Expr<L>`, and every public IR type (§14).
- The `IrErrorKind` typed-kind enum and its `Diagnose` impl (§15).
- The Substrait-mapping **table** (§14.2) — declarative correspondence between each `PlanNode` variant and the `substrait::proto::Rel` kind an adapter emits. The conversion *code* lives in `36`.

### 1.2 What `semstrait-ir` does NOT own

- **Planning strategy and per-DataKind plan assembly.** Every decision that "this `Request` against this `SemanticManifest` expands into a tree of `PlanNode`s in this order" lives in `semstrait-planner` per `34`. `35` ratifies only the **output shape** that planning must produce.
- **Optimization passes.** Rule-based rewrites over `SemanticPlan` live in `semstrait-planner` (`34`, stage 5 per `10 §3.5`). `35`'s `walk` / `transform` helpers (§13) are the substrate those rewrites run on, not the rewrites themselves.
- **Adapter emission.** Translating a `SemanticPlan` into an `EngineArtifact` (SQL text or Substrait proto) is `36`'s contract. `35` ratifies the artifact's structural shape and the Substrait mapping table; the rendering code, dialect-specific SQL, and capability checks all live above.
- **SemanticManifest shape.** `SemanticPlan` references the SemanticManifest for bindings and resolved expressions (§10.2) via opaque identifiers (`SourceRef`, `BindingId`); the SemanticManifest types themselves live in `semstrait-manifest` per `33`. `35` never embeds `ResolvedDataKind` / `ResolvedBinding` values inline.
- **Expression-type variant catalogs and structural invariants.** While `semstrait-ir` now OWNS the *crate-level* placement of the expression types per §1.1, the **variant rosters** for each enum, the **structural invariants** between leaf sets, the **type aliases** discipline, and the **accessor catalogs** are ratified by `[14 §3](../foundations/14_expressions.md)` and `[14 §4](../foundations/14_expressions.md)`. `35`'s §3–§5 reference those rosters rather than re-ratifying them; per `[DOCS_MAINTENANCE.md §3](../DOCS_MAINTENANCE.md)`, the variant catalogs and structural rules live in `14` alone.
- **Compile-time `SemanticExpr` → `PhysicalExpr` resolution.** The algorithm that lowers `SemanticExpr` into `PhysicalExpr` (`SemanticExpr::resolve`, `ResolvedExprTable` keying, cross-DataKind path resolution, sugar-accessor elimination, type inference, Semantics-boundary reconciliation) lives in `semstrait-manifest::compile` per `[19 §3](../foundations/19_expression_flow.md)`. `35` owns the types that flow through that algorithm; it does not own the algorithm.
- **Phase B placement and `Parameter` binding.** The `Strategy`-driven plan-tree construction (filter splitting, `Aggregate` lift into `PlanNode::Agg`, `Parameter` binding against the `Request`, advisory channel) lives in `semstrait-planner` per `[19 §6](../foundations/19_expression_flow.md)` and `[34](34_semstrait_planner.md)`. `35` owns the `PlanNode` and `PhysicalExpr` types that the planner produces; the planning algorithm itself is `34`'s contract.
- **Canonical-function semantics and per-engine mapping.** What `coalesce` does to nulls, which engines support `regexp_match` natively, how `add` maps to DataFusion's `Add` operator — `[14a](../foundations/14a_function_catalog.md)` and `registry/functions_mapping.md` are authoritative. `35` owns the registry's *shape*, not its *contents*.

### 1.3 Design posture — pure, sync, canonical

`semstrait-ir` is deliberately **pure** (no I/O, no async, no engine identity). It is the data-only substrate every post-compile stage consumes:

- **Zero I/O surface.** Concrete I11 guarantee (per `30 §9`).
- **Zero async.** Every method on every public type is synchronous; `SemanticPlan` is built, walked, rewritten, and serialized on the caller's thread. `Expr<L>` traversal is synchronous. I6 guarantee.
- **Zero engine identity.** No `datafusion::*`, no `arrow::*`, no `spark::*`, no `duckdb::*` types are visible on any `semstrait-ir` public surface. `DialectId` is an opaque newtype; `substrait::proto::Plan` is the one exception and appears only inside `EnginePlan::Substrait(_)` (§11.2) as the adapter-consumable payload.
- **Single upstream dependency.** `semstrait-ir` depends on `semstrait-core` only — for the canonical logical-type vocabulary (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`) per `[13](../foundations/13_types_and_grain.md)`, the cross-cutting diagnostic primitives (`Diagnostic<K>`, `Diagnose`, `Severity`, `Location`, `Span`, `SourceId`), and the constraint-DSL toolkit referenced by some `FunctionSpec`-adjacent diagnostics. The trait surface (`Tree` / `Visitor` / `Rewriter` / `ExprLeaf`), the structural-variant support enums (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `Literal`), the identifier carriers (`ColumnRef`, `SemanticsName`), and the narrow ir-emitted error kinds (`ValidateError`, `CompileError`) are owned here, not in core. Every other workspace crate depends on `semstrait-ir` directly (model, manifest, planner, adapter) or transitively (façade). `Cargo.toml` audit per §17.2 enforces this.

Following the second cascade (`STATUS.md` item Q, 2026-05-19), this crate is the **complete expression-vocabulary home**: trait scaffolding, structural-variant support enums, leaf carriers, expression types, accessor enums, `Parameter`, authoring-surface DSL, `FunctionRegistry`, and the two narrow error kinds emitted by the trait / registry machinery. Every downstream crate — `semstrait-model` (parse-site dispatch produces `Expr<SemanticLeaf>` values per `[14 §9.3](../foundations/14_expressions.md)`), `semstrait-manifest` (`compile` transforms `SemanticExpr` into `PhysicalExpr` per `[19 §3](../foundations/19_expression_flow.md)`; its wider `CompileError` embeds `Ir(ir::CompileError)` via D.ii), `semstrait-planner` (Phase B consumes `PhysicalExpr` and the sealed `FunctionRegistry` per `[19 §6](../foundations/19_expression_flow.md)`), and `semstrait-adapter` (renders `PhysicalExpr` to engine artifacts) — consumes these types from here.

### 1.4 Engine-IR concept inspiration

`PlanNode` borrows its **catalog of operators** and **tree composition** from engine IRs — DataFusion's `LogicalPlan`, Calcite's `RelNode`, Substrait's `Rel`. These are the shapes a planner naturally produces; re-inventing them would be a waste. Per `00 §3`, the inspiration is **structural only**:

- Borrowed: the set of operators (`Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch`), the box-per-child tree shape, the invariant that every non-leaf carries typed inputs.
- Rejected: cost / statistics fields on any `PlanNode` (cost lives in the engine, not the canonical plan); physical / distribution properties (`Partitioning`, `Exchange`, `Shuffle`, `Repartition`); dialect or adapter branching on node variants; vendor-specific rel kinds.

`Expr<L>`'s structural variants borrow the same way — the operator catalog from `[14 §3.3](../foundations/14_expressions.md)` mirrors the union of variants that engine ASTs naturally surface (arithmetic, comparison, `Case`, `Cast`, `FunctionCall`, `Aggregate`, `Window`) without admitting engine-specific operators (those land as `FunctionCall` entries via the registry per `[14a §7](../foundations/14a_function_catalog.md)`).

## 2. Module Layout

Top-level `pub mod` structure. One module per cohesive concept.

```
semstrait-ir
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
│                        //   RegistryExtension trait, function_registry() accessor (14a §2 / §3 / §7)
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

- `tree` vs `expr_kinds` — the trait surface (`Tree`, `Visitor`, `Rewriter`, `ExprLeaf`) is conceptually independent of the support-enum roster: `Tree` works for `PlanNode` (which never references `BinaryOpKind`) just as well as for `Expr<L>`. Both modules moved from `semstrait-core` at the second cascade; isolating them limits the I10 blast radius when a support-enum variant lands.
- `expr` vs everything else — the canonical-IR expression types are referenced by `plan` (`PlanNode` variants carry `PhysicalExpr` on predicate / projection / aggregate slots) but not vice-versa; isolating the expression module lets downstream consumers that only need expressions (parse-site code, registry-lookup tooling) skip the plan-tree surface entirely.
- Inside `expr`: `tree` / `leaves` / `accessor` / `parameter` split per-type-family for `cargo public-api` audit clarity and so a future doctool can list each family in isolation; `expr_fn` isolates the DSL so the structural types compile without the authoring vocabulary present.
- `functions` — every consumer of `Expr<L>::FunctionCall { name: CanonicalFn, ... }` needs the registry to resolve `name` to a `FunctionSpec`; placing the registry adjacent to `expr` keeps the dependency direction natural and removes the cross-crate hop downstream consumers previously paid.
- `plan` vs `primitives` — `PlanNode` references every primitive type, but not vice-versa. Keeping primitives alphabetically separate lets future crates (e.g. a plan-diff tool) depend only on `primitives` without linking the full `PlanNode` surface.
- `plan::node` vs `plan::traversal` — the traversal API's method count scales with the `PlanNode` variant count; isolating it limits I10 blast radius when a new variant lands.
- `artifact` as a separate module — `EngineArtifact` is the *output* shape. It is naturally decoupled from the *input* shape (`SemanticPlan`) and is consumed by the engine layer above `semstrait-adapter` (executors, wrappers, CLI); isolating it keeps the import graph clean.
- `error` — three kinds co-located: the narrow `ValidateError` and `CompileError` emitted by the trait / registry machinery (also moved from `semstrait-core` at the second cascade), plus the plan-shape `IrErrorKind`. Distinct from manifest's wider `CompileError` (resolution-stage errors per `33 §10`), which embeds `Ir(ir::CompileError)` via D.ii.
- `substrait_map` exists as a table reference, not a conversion module. The actual conversion code lives in `36` (which owns the substrait-proto emission and consumption logic).

**Re-exports.** The crate root re-exports a curated surface (§19). Non-root re-exports of internal helpers are forbidden.

## 3. `Expr<L>` Structural Type — Owned Here, Specified by `14`

### 3.1 Where the type architecture lives

The structural shape of canonical-IR expressions is ratified by `[14 §3](../foundations/14_expressions.md)`:

- `[14 §3.1](../foundations/14_expressions.md)` ratifies the universal `Tree` trait and its `Visitor` / `Rewriter` companions.
- `[14 §3.2](../foundations/14_expressions.md)` ratifies the `ExprLeaf` trait.
- `[14 §3.3](../foundations/14_expressions.md)` ratifies the structural-variant catalog of `Expr<L>` — every variant (`BinaryOp`, `UnaryOp`, `FunctionCall`, `Cast`, `Case`, `InList`, `Between`, `Like`, `IsNull`, `Coalesce`, `NullIf`, `Aggregate`, `Window`) and the `Leaf(L)` wrapper.

`35` is the **crate** that holds the implementation of `Expr<L>`. Per `[14 §9.2](../foundations/14_expressions.md)`, this ownership moved from `semstrait-core` to `semstrait-ir` at the `14` second-refinement landing. `35` does not re-ratify the variant catalog; the catalog is `[14 §3.3](../foundations/14_expressions.md)`'s contract and any change to it lands in `14` first, then cascades here.

### 3.2 The `Tree` / `Visitor` / `Rewriter` / `ExprLeaf` trait surface — owned here

Per `[14 §3.1](../foundations/14_expressions.md)` / `[§3.2](../foundations/14_expressions.md)` and the second-cascade placement in `[14 §9.2](../foundations/14_expressions.md)`, the universal-traversal trait family is owned by `semstrait-ir`. Both `Expr<L>` (§3.3) and `PlanNode` (§9) implement these traits; the natural home is alongside their producers, not upstream in core.

```rust
/// Universal traversal contract. Implemented by Expr<L> (§3.3) and PlanNode (§9).
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

/// Per-leaf-set metadata contract. Implemented by PhysicalLeaf and SemanticLeaf (§4).
/// Per `14 §3.2`.
pub trait ExprLeaf: Sized + Clone + Debug {
    fn inferred_type(&self) -> Option<&DataType>;
}
```

Consumers write `use semstrait_ir::Tree` (no cross-crate hop to core). The single trait surface lets one generic algorithm operate on both expression trees and plan trees — e.g. the optimizer applies the same `transform` helper to predicates inside `FilterNode` and to subtrees rooted at `FilterNode` itself.

`ValidateError` is owned by `semstrait-ir` (§15) since it is raised entirely by `Tree::with_new_children` and the `Rewriter<N>::f_*` callbacks defined here.

### 3.3 The `Expr<L>` definition

The full variant catalog is per `[14 §3.3](../foundations/14_expressions.md)`. `35`'s exposed surface:

```rust
/// Canonical structural expression tree, parameterized over a leaf set `L`.
/// Variant catalog per `14 §3.3`. `#[non_exhaustive]` per I10.
///
/// Instantiated by the type aliases `PhysicalExpr` (with `PhysicalLeaf`)
/// and `SemanticExpr` (with `SemanticLeaf`) per §4.3.
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

- `Window` is **compile-emitted only** — author-facing parsers do not accept window syntax; `Window` nodes enter the tree exclusively through sugar-accessor elimination during compile (§5.1, `[14 §4.2](../foundations/14_expressions.md)`).
- Engine-specific operators do not add `Expr<L>` variants. They land as `FunctionCall` entries via `FunctionRegistry` extensions per `[14a §7](../foundations/14a_function_catalog.md)`.
- `Aggregate`'s `filter` carries the canonical `agg(expr) FILTER (WHERE p)` shape; adapter compensation for engines without native `FILTER` is the adapter's concern (`36`), not part of the canonical IR.

### 3.4 Structural-variant support enums + identifier carriers — owned here

`Expr<L>`'s structural variants reference a small set of support enums and identifier carriers. Per `[14 §9.2](../foundations/14_expressions.md)` (second cascade), these live in `semstrait-ir` alongside `Expr<L>` itself. Rosters per `[14 §3.3](../foundations/14_expressions.md)`.

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

`DataType` / `Grain` / `Schema` / `SchemaColumn` are re-exported from `semstrait-core` (they remain there as shared logical-type vocabulary used by model / manifest / planner / adapter outside of expression contexts):

```rust
pub use semstrait_core::{DataType, Grain, TypeClass, Schema, SchemaColumn};
```

`DataType` flows through both the structural variants (`Cast::target: DataType`) and the support enums (`Literal::Decimal { precision, scale }` aligns with `DataType::Decimal`); keeping the re-export at the crate root means `use semstrait_ir::*` is sufficient for almost every consumer.

**Why `Vec<String>` matters here:** `Literal` parses bare YAML scalars into typed shape at parse time per `[32 §...](../apis/32_semstrait_model.md)`; `semstrait-ir` validates `Decimal { precision, scale }` range at construction.

### 3.5 What `35` does NOT re-ratify

- The structural-variant catalog of `Expr<L>` — owned by `[14 §3.3](../foundations/14_expressions.md)`.
- The trait-family contract (`Tree`, `Visitor`, `Rewriter`, `ExprLeaf` signatures and semantics) — owned by `[14 §3.1](../foundations/14_expressions.md)` / `[§3.2](../foundations/14_expressions.md)`. `35` carries the implementation per §3.2.
- The support-enum variant rosters — owned by `[14 §3.3](../foundations/14_expressions.md)`. `35` carries the implementation per §3.4.
- The traversal-helper semantics (`apply`, `transform` default bodies) — owned by `[14 §3.1](../foundations/14_expressions.md)`.

## 4. Leaf Sets — `PhysicalLeaf` and `SemanticLeaf`

### 4.1 `PhysicalLeaf`

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

### 4.2 `SemanticLeaf`

Per-kind typed leaf set per `[14 §3.5](../foundations/14_expressions.md)`. Each variant tag encodes the entity kind; the optional `accessor` field carries per-kind sugar (§5.1). The full catalog and per-variant invariants are owned by `14`.

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

- **No `EntityRef` wrapper, no `Access` wrapper, no outer `Accessor` enum.** Every semantic reference is a typed leaf whose variant tag already encodes the entity kind. The per-kind accessor enums (§5.1) sit as `Option<…>` fields on the typed leaves. This shape replaces the earlier-draft `EntityRef` / `Access` / wrapping `Accessor` design at the `14` second-refinement landing.
- **`Field` is the untyped fallback.** When the author writes a bare identifier (at a semantic site) or explicit `field(name)`, the leaf carries no kind hint; compile resolves the kind by registry lookup.
- **`Dimension` / `Measure` / `Metric` / `Key` are kind-pinned.** Compile fails fast if the registered semantic at `name` has a different kind than the authored leaf variant — `manifest::CompileError::SemanticKindMismatch` per `[14 §8](../foundations/14_expressions.md)`.
- **`Column` is conditionally legal.** Type-admissible (the parser can construct it), but compile rejects it under manual mapping per `[14 §8](../foundations/14_expressions.md)`. Under `semantic_mapping: auto`, compile synthesizes `SemanticMapping` entries for `Column` leaves and the rest of resolution proceeds as with manual mapping.
- **No `Parameter`.** Parameters are exclusively compile-emitted and live only in `PhysicalLeaf`.

### 4.3 Type aliases

```rust
pub type PhysicalExpr = Expr<PhysicalLeaf>;
pub type SemanticExpr = Expr<SemanticLeaf>;
```

These are the spelled-out names used throughout downstream docs (`19`, `33`, `34`) and downstream-crate APIs. The generic `Expr<L>` form appears in trait bounds and shared algorithmic code (e.g., the optimizer's tree-walks).

### 4.4 Type-enforced forbidden combinations

Per `[14 §3.7](../foundations/14_expressions.md)`, the leaf-set boundary makes several invariants type-level:

- `PhysicalExpr` cannot contain `Field` / `Dimension` / `Measure` / `Metric` / `Key` — those variants do not exist in `PhysicalLeaf`. The static type system, not a runtime check, upholds this.
- `SemanticExpr` cannot contain `Parameter` — `Parameter` is `PhysicalLeaf`-only.
- A `Dimension`-tagged leaf cannot carry a `MeasureAccessor` (or any non-Dimension accessor) — the variant signature `Dimension { name, accessor: Option<DimensionAccessor> }` enforces kind agreement at construction.

There is no `try_into_physical` runtime check, no defensive `panic!` for "Field found in PhysicalExpr". `SemanticLeaf::Column` is type-admissible but context-validated by compile per §4.2's manual-vs-auto rule.

### 4.5 What `35` does NOT re-ratify

- Variant rosters of `PhysicalLeaf` / `SemanticLeaf` — owned by `[14 §3.4](../foundations/14_expressions.md)` / `[§3.5](../foundations/14_expressions.md)`.
- The structural-invariant rules between the two leaf sets (`PhysicalExpr` carries no `Field`, etc.) — owned by `[14 §3.7](../foundations/14_expressions.md)`.
- The retired vocabulary from earlier drafts — `EntityRef` wrapper, outer `Accessor` enum, `Access` structural variant — is **not present** here, per `[14 §3.5](../foundations/14_expressions.md)`'s second-refinement landing. `35`'s implementation tracks `14`'s canonical shape.

## 5. Per-Kind Accessor Enums and `Parameter`

### 5.1 Per-kind accessor enums

Per-entity sugar lets authors write shorthand like `measure("revenue").previous()` or `metric("conv_rate").delta()`. The mechanism is a kind-specific accessor enum carried as an `Option<…>` field on each typed semantic leaf (§4.2). The four enums and their variant catalogs are ratified in `[14 §4.1](../foundations/14_expressions.md)`. `35` carries the implementation:

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

### 5.2 `Parameter` and `ParameterKey`

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

`Parameter` lives only in `PhysicalLeaf` (§4.1); `SemanticLeaf` carries no `Parameter`. The plan-time binding postcondition is per `[14 §5.3](../foundations/14_expressions.md)`: no `Parameter` survives into adapt time. Phase B substitution mechanics are per `[19 §6](../foundations/19_expression_flow.md)`; a `Parameter` reaching an adapter is a hard error owned by the planner (`PlanErrorKind`), not by `35`.

### 5.3 What `35` does NOT own

- The sugar-elimination lowering shape (typed leaf with `accessor: Some(_)` → `Window`-rooted subtree, run to fixpoint) — `[14 §4.2](../foundations/14_expressions.md)` ratifies the target shape; `[19 §3](../foundations/19_expression_flow.md)` ratifies where and how `compile` runs the elimination.
- Plan-time `Parameter` binding mechanics — `[19 §2.1](../foundations/19_expression_flow.md)` / `[34](34_semstrait_planner.md)`.
- The `Request` shape that supplies substitution values — `[34](34_semstrait_planner.md)`.

## 6. Authoring-Surface DSL — `expr_fn`, `std::ops`, `ExprFunctionExt`

Per `[14 §9.2](../foundations/14_expressions.md)` final paragraph, the canonical authoring-surface constructors live in `semstrait-ir::expr::expr_fn`. The Rust DSL mirrors the YAML reserved-tag catalog from `[14 §6.4.1](../foundations/14_expressions.md)` exactly — `dim("region")` in Rust corresponds to `{ dim: region }` in YAML and produces the same `SemanticLeaf::Dimension { name, accessor: None }` shape.

The DSL is **opt-in** ergonomic sugar; every value it produces is also constructible via direct struct / enum literal. Downstream tooling that prefers not to take the `expr_fn` dependency-surface (`semstrait-manifest::compile`'s lowering, `semstrait-planner`'s plan-tree builders) constructs `Expr<L>` values directly.

### 6.1 `expr_fn` free constructors

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
    /// Optional accessor attached via `ExprFunctionExt` methods (§6.3).
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

### 6.2 `std::ops` impls

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

`BitAnd` / `BitOr` carry SQL `AND` / `OR` semantics (not bitwise) because `Expr<L>` does not include bitwise operators in v1. Comparison operators (`==`, `<`, `>`, …) cannot be `std::ops`-overloaded in Rust because the `PartialEq` / `PartialOrd` traits return `bool`, not custom types; these surface via the `ExprFunctionExt` trait (§6.3) as `.eq(other)` / `.lt(other)` / etc., or via the `binary_op` Declarative form per `[14 §6.4](../foundations/14_expressions.md)`.

### 6.3 `ExprFunctionExt` extension trait

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

Per-kind accessor sugar (`.previous()`, `.next()`, `.delta()`, `.percent_change()`, `.first()`, `.last()`, `.lag(n)`, `.lead(n)`) is surfaced as methods on `SemanticExpr` whose visibility is gated by the inner `SemanticLeaf` variant — typically realized via thin per-kind newtype shims returned by `expr_fn::measure` / `expr_fn::dim` / etc. so that `measure("x").previous()` typechecks while `dim("x").previous()` does not. The exact shim discipline is an implementation choice; `35` ratifies the four accessor enums (§5.1) and the kind-agreement contract.

Sugar accessors lower to `Window`-rooted subtrees during compile per `[14 §4.2](../foundations/14_expressions.md)`; the DSL methods produce typed leaves with `accessor: Some(_)`, not `Window` nodes directly. `Window` is intentionally not author-constructible per `[14 §6.4.1](../foundations/14_expressions.md)`.

### 6.4 What `35` does NOT own

- The YAML authoring surface (`ExprSource` enum with `Inline(String)` and `Block(Expr<L>)` variants, parse-site dispatch) — lives in `semstrait-model` per `[14 §9.3](../foundations/14_expressions.md)` and `[32](32_semstrait_model.md)`. The `Block(Expr<L>)` variant carries `Expr<SemanticLeaf>` (semantic sites) or `Expr<PhysicalLeaf>` (physical-mapping sites) directly via the serde derives on `Expr<L>` (`§14.1`); there is no separate `ExprBlock` type.
- Per-site shape gates governing which authoring sites admit which expression shapes (scalar / Boolean / aggregate-admitting) — `[14 §7](../foundations/14_expressions.md)`.
- Bare-identifier resolution rules (semantic site defaults to `field`, physical-mapping site defaults to `col`) — `[14 §6.5](../foundations/14_expressions.md)`.

## 7. `CanonicalFn` and the `FunctionRegistry`

### 7.1 Where the function-catalog architecture lives

Per `[14a §2](../foundations/14a_function_catalog.md)`, the canonical function identity (`CanonicalFn`), the registry shape (`FunctionRegistry`), the specification shape (`FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`), and the extension hook (`RegistryExtension`) are ratified in `14a`. `35` is the **crate** that owns the implementation.

This ownership moved from `semstrait-core` to `semstrait-ir` at the `14` second-refinement landing per `[14 §9.2](../foundations/14_expressions.md)`. Rationale: every consumer of `Expr<L>::FunctionCall { name: CanonicalFn, ... }` needs the registry to resolve `name` to a `FunctionSpec`; placing the registry adjacent to `Expr<L>` (and to the leaf sets that reference it) keeps the dependency direction natural and removes the cross-crate hop downstream consumers previously paid.

### 7.2 Public surface

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

/// Adapter / downstream-crate extension hook. Per `14a §7.1`.
///
/// Not sealed — adapter crates outside the workspace MAY contribute their
/// own overloads at registry build time.
pub trait RegistryExtension {
    fn extend(&self, registry: &mut FunctionRegistryBuilder)
        -> Result<(), CompileError>;
}
```

`CompileError` and `ValidateError` are owned here in `semstrait-ir::error` (§15) — they are raised by the registry callbacks (`ReturnTypeRule::Custom`) and the trait machinery (`Tree::with_new_children`, `Rewriter<N>::f_*`) respectively. The plan-shape `IrErrorKind` (§15.3) is the third enum exposed by this crate, scoped to plan-tree concerns.

### 7.3 What `35` does NOT own

- The semantics of each canonical function (which `DataType`s `add` accepts, what `coalesce` does to nulls) — `[14a §4](../foundations/14a_function_catalog.md)` is authoritative.
- Per-engine canonical → engine-native mapping (`add` → DataFusion's `Add` operator, `coalesce` → Spark's `coalesce`, …) — `registry/functions_mapping.md` plus per-adapter `RegistryExtension` impls in `36`.
- Signature resolution at parse / compile time (which overload a `FunctionCall` resolves to given its argument types) — `[14a §5](../foundations/14a_function_catalog.md)` ratifies the algorithm; `semstrait-manifest::compile` (`[33](33_semstrait_manifest.md)`) executes it.
- Registry sealing protocol (the moment after which no more `extend` calls are accepted) — `[14a §7.2](../foundations/14a_function_catalog.md)`.

## 8. Public Types — `SemanticPlan` Root

### 8.1 Shape

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
    /// Contains only `Severity::Warning` entries (per `30 §5.2`'s
    /// 2-variant Severity); errors abort planning and never reach
    /// `SemanticPlan` per `10 §3.4`. The element type uses a heterogeneous
    /// envelope (boxed `Diagnostic<dyn Diagnose>` or an erased
    /// `PlanDiagnostic` wrapper per `34 §13.2`) so warnings from
    /// `PlanErrorKind` and `OptimizeErrorKind` can co-exist on the
    /// artifact. Adapters MAY append their own warning-severity entries
    /// during `adapt` but are not required to (see `36`).
    pub diagnostics: Vec<PlanDiagnostic>,
}
```

### 8.2 Invariants

A `SemanticPlan` is **well-formed** when:

1. `output_names.len() == root.meta().output_schema.len()` — every output column has a user-visible name.
2. Every `Name` in `output_names` is a valid identifier (see §10.4).
3. `root` and every descendant satisfy the tree invariants of §12.
4. Every `PlanDiagnostic` in `diagnostics` has `Severity::Warning`.

Construction does **not** re-check invariants 1–3 at runtime (planning established them; re-checking is a planner-regression catch, not a caller contract). An optional `SemanticPlan::validate()` method (§13.3) walks the tree and reports violations as `Diagnostic<IrErrorKind>` for debugging.

### 8.3 Serde

`SemanticPlan` derives `Serialize` / `Deserialize` under the crate-level `serde` feature. The wire form is the direct struct shape; no intermediate envelope. `PhysicalExpr` inside child nodes serializes through `35`'s own `Expr<L>` serde (§14). `PlanNode` (`#[non_exhaustive]`) uses serde's `untagged` policy with a discriminator field (`kind: "scan" | "filter" | ...`) so the wire form survives the addition of new variants per I10.

A serialized `SemanticPlan` is a format-stable portable plan artifact: two processes with the same compiled SemanticManifest can exchange a `SemanticPlan` and get identical adapter output. This is what makes the crate a faithful IR.

### 8.4 Construction patterns

Planners build a `SemanticPlan` from the bottom up:

```rust
let scan = PlanNode::Scan(ScanNode {
    meta: NodeMeta::new(binding.output_schema()),
    source: SourceRef::from_binding(&binding.id),
    columns: binding.columns_in_native_order(),
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

### 8.5 Cloning

`SemanticPlan: Clone`. Every child node is `Box`-owned; `Clone` is a deep clone. For cheap structural sharing inside optimizer passes, use `walk_post` with in-place `transform` (§13) rather than successive `clone`s.

## 9. Public Types — `PlanNode` Sum

### 9.1 The sum type

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

### 9.2 `Scan` — leaf source

```rust
/// Reads a resolved source. The only `PlanNode` variant without child
/// nodes. Per `15 §3.5.6 / §10.6` the planner walks the
/// `ResolvedBinding` and emits one `ScanNode` per `ResolvedPhysicalSource`
/// in the binding's `sources` list. Each `ResolvedPhysicalSource` is an
/// engine-level LogicalRelation (one Substrait `ReadRel`, one DataFusion
/// `TableScan`, one Spark `LogicalRelation`, one SQL `FROM` reference);
/// engine-internal mechanics (Hive partition discovery, multi-file
/// consolidation, schema merge) live below the `ScanNode` boundary —
/// see §9.2.1.
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
    /// references only columns in `columns` (enforced by §12.4). Empty
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

### 9.3 `Filter` — predicate

```rust
#[non_exhaustive]
pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Boolean-valued predicate. Operand type must be `Boolean` per
    /// `14 §7` / §12.10. Columns referenced must exist in
    /// `input.meta().output_schema`.
    pub predicate: PhysicalExpr,
}
```

Pass-through schema: `FilterNode.meta.output_schema` equals `input.meta().output_schema` (enforced at construction per §10.8; adapters rely on this).

### 9.4 `Project` — column list

```rust
#[non_exhaustive]
pub struct ProjectNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Ordered list of `(output_name, expression)` pairs. The result
    /// schema's field at ordinal `i` has name `projections[i].0` and
    /// `data_type` follows by inference from `projections[i].1`'s
    /// leaf-level `ExprLeaf::inferred_type` per §12.2. Empty list is
    /// rejected at construction (trivial `Project` collapses to
    /// `input`).
    pub projections: Vec<(Name, PhysicalExpr)>,
}
```

### 9.5 `Agg` — group-by + aggregates

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
    /// Each `AggregateExpr` is an `Expr::Aggregate` kernel (see §10.7)
    /// lifted out of the input `PhysicalExpr` by Phase B per `19 §7`,
    /// whose inner expression references only columns in
    /// `input.meta().output_schema`.
    pub aggregates: Vec<(Name, AggregateExpr)>,
}
```

### 9.6 `Join` — binary composition

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
    /// v1 variant (see §16.1 TD-IR-CROSS-JOIN).
    pub on: Vec<KeyPair>,
}
```

Non-equi-join predicates (range joins, inequality joins) are deferred — a future `JoinNode.residual: Option<PhysicalExpr>` field is MINOR per §16.1.

### 9.7 `Union` — n-ary stack

```rust
#[non_exhaustive]
pub struct UnionNode {
    pub meta: NodeMeta,

    /// Two or more inputs. Every input's `output_schema` must be
    /// structurally compatible (same arity, same element types, same
    /// nullability — per §12.6).
    pub inputs: Vec<PlanNode>,

    /// Whether to apply `DISTINCT` deduplication after the union.
    /// `false` = `UNION ALL` (bag semantics); `true` = `UNION` (set
    /// semantics). Defaults to `false` — every engine natively
    /// supports `UNION ALL` without rewrite; `true` demands a
    /// post-hash-agg pass.
    pub distinct: bool,
}
```

### 9.8 `Sort` — ordering

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

### 9.9 `Fetch` — limit / offset

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

Unsigned integer (`u64`) deliberately rejects negative values at the type boundary; the `Option` shape keeps "no limit / no offset" distinct from "limit 0 / offset 0". This matches Substrait's `FetchRel.count` / `offset` (both `i64`) at the upper half of `u64`'s range; the rare `u64 > i64::MAX` case is rejected at adapter-emit time with `IR_E_3510 FetchValueOutOfRange`.

## 10. Public Types — Shared Primitives

### 10.1 `NodeMeta`

```rust
/// Metadata attached to every `PlanNode`. Per `15 §7` and `17 §3`.
#[non_exhaustive]
pub struct NodeMeta {
    /// Unique identifier for this node in the plan tree. Used by the
    /// optimizer (rule-engine source tracking) and the adapter
    /// (diagnostic correlation). Not stable across planner
    /// invocations — two runs against the same SemanticManifest + Request
    /// MAY produce different `NodeId`s.
    pub node_id: NodeId,

    /// Output schema after this node. `Arc` allows pass-through
    /// nodes (Filter, Sort, Fetch) to share the parent schema without
    /// deep-cloning.
    pub output_schema: std::sync::Arc<Schema>,

    /// Semantic annotations attached by `semstrait-planner`
    /// (additivity role, filter source, kind-ref, etc.). Consumed by
    /// the adapter's Substrait-emission path to round-trip semstrait
    /// context across the Substrait boundary.
    pub annotations: Vec<SemAnnotation>,
}
```

`NodeId` is a newtype over `Uuid::new_v4()` in v1; external consumers should treat it as opaque. `Schema` is a plan-level structural schema (not a SemanticManifest-level `ResolvedDataKind` schema) — `{ fields: Vec<Field> }` where `Field { name: Name, data_type: DataType, nullable: bool }` per `15 §4.2`.

`SemAnnotation` is an additive `#[non_exhaustive]` sum (AggregateRole, FilterSource, Additivity, KindRef, …) ratified in `34`'s planner notes; `35` re-exports the enum for the purpose of serde-roundtrip fidelity and adapter consumption.

### 10.2 `SourceRef`

```rust
/// Opaque reference to a `ResolvedPhysicalSource` in the SemanticManifest.
/// Per `15 §7.1` / `00 §4.1`.
///
/// `SourceRef` is a deliberately opaque handle — adapters resolve it
/// against the SemanticManifest they were handed alongside the `SemanticPlan`.
/// No path, URL, catalog name, or file format leaks into the plan
/// tree. I1 guarantee.
///
/// Newtype-over-stable exception per `30 §4.3`: no `#[non_exhaustive]`
/// on the outer newtype; the inner variant is crate-private.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SourceRef(/* crate-private: (BindingId, u32) */);

impl SourceRef {
    /// Construct from a `(binding_id, source_index)` pair. Called by
    /// `semstrait-planner` at `ScanNode` creation time.
    pub fn new(binding_id: BindingId, source_index: u32) -> Self;

    pub fn binding_id(self) -> BindingId;
    pub fn source_index(self) -> u32;
}
```

### 10.3 `ResolvedColumn`

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

### 10.4 `Name`

```rust
/// Identifier used for output-column names, group-by keys, sort keys,
/// and projection aliases. A plan-level newtype over `String` with a
/// construction boundary that enforces identifier well-formedness:
///
/// - Non-empty.
/// - UTF-8 (guaranteed by `String`).
/// - Not a reserved plan-tree tag (see §10.4.1).
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

### 10.5 `KeyPair`

```rust
/// One join-key pair on a `JoinNode.on`. Per `16 §5.1`.
///
/// Both `left` and `right` are column names resolving against the
/// join's corresponding child's `output_schema`. Column types must
/// match per §12.5 — planner-side reconciliation lives in
/// `19 §3` / `15 §10.5`; a mismatch reaching `35` is reported by
/// `SemanticPlan::validate` as `IrErrorKind::JoinKeyTypeMismatch`.
#[non_exhaustive]
pub struct KeyPair {
    pub left: Name,
    pub right: Name,
}
```

### 10.6 `SortDir` / `NullOrdering`

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

### 10.7 `AggregateExpr`

```rust
/// A single aggregate kernel on `AggNode.aggregates`. Wraps an
/// `Expr::Aggregate` payload with plan-level shape:
///
/// - `aggregation` and `distinct` match `Expr::Aggregate`'s `op` and
///   `distinct` fields per `14 §3.3`.
/// - `input_expr` is the inner expression (e.g. `Column("amount")`
///   for `sum(amount)`; `Literal(1)` for `count(1)`).
/// - `filter` is the optional `FILTER (WHERE ...)` clause Substrait
///   supports natively. Per `14 §3.3`'s second-refinement landing,
///   `Expr::Aggregate` carries a `filter: Option<Box<Self>>` field
///   directly; the `AggregateExpr` carrier hoists that into a separate
///   plan-level slot after the planner's Phase B aggregate-lift per
///   `19 §7`. v1 adapters MUST accept `None`.
///
/// `AggregateExpr` is NOT a `PhysicalExpr` — it is a plan-level
/// *carrier* for an aggregate kernel that the planner's Phase B
/// aggregate-lift pass per `19 §7` extracted out of the input
/// `PhysicalExpr`. After lift, `AggNode.aggregates` carries these
/// kernels and the residual `PhysicalExpr` (typically a `Column`
/// reference to the lifted slot) lives in the parent `ProjectNode`
/// / `FilterNode`. Per the plan-tree invariant in §12.1, no
/// `Expr::Aggregate` node remains inside a `PhysicalExpr` stored on
/// a `FilterNode.predicate` or `ProjectNode.projections[*].1`.
#[non_exhaustive]
pub struct AggregateExpr {
    pub aggregation:   AggregationOp,
    pub input_expr:    PhysicalExpr,
    pub distinct:      bool,
    pub filter:        Option<PhysicalExpr>,
    pub inferred_type: DataType,
}
```

The `inferred_type` field is populated by the planner's Phase B aggregate-lift pass (`[34](34_semstrait_planner.md)`, `[19 §7](../foundations/19_expression_flow.md)`). Adapters MAY read it directly without re-deriving from the `aggregation` + `input_expr`'s inferred type.

### 10.8 Invariants enforced at construction

Each `PlanNode` variant's struct is constructed directly (no hidden builder); the variant's field combination is the contract. Schema invariants (§12) are *not* checked at construction — consumers rely on the planner to produce well-formed trees and rely on `SemanticPlan::validate()` (§13.3) for a debug-only full re-check.

## 11. Public Types — Adapter Artifact Family

The output types produced by `semstrait-adapter::adapt()` (`36`) from a `SemanticPlan`. `35` ratifies the structural shape; `36` owns the emission semantics.

### 11.1 `EngineArtifact`

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

### 11.2 `EnginePlan`

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

### 11.3 `SqlArtifact`

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

### 11.4 `DialectId` + `Dialect`

```rust
/// Stable identifier for a SQL dialect. Per `00 §4.1` and `36`.
///
/// Implemented as a newtype over a `&'static str` with `pub const`
/// identities per built-in adapter; adapters outside the workspace
/// register new dialects via the `Dialect` trait (§11.5).
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

### 11.5 `Dialect` trait

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

`Capability` is a `#[non_exhaustive]` enum ratified in `36` (e.g. `DistinctAggregate`, `RegexpMatch`, `AsOfJoin`, `StructAccess`); `35` re-exports it only for the purposes of adapter compile-time integration. Adapter-side capability roster ownership is `36`'s.

### 11.6 `Capability`

```rust
/// Dialect / adapter capability vocabulary. Ratified by `36`; re-exported
/// here so `SemanticPlan` consumers can interrogate an adapter before
/// calling `adapt`.
#[non_exhaustive]
pub enum Capability {
    DistinctAggregate,
    RegexpMatch,
    RegexpExtract,
    IntervalLiteral,
    AsOfJoin,
    StructAccess,
    // ... full enumeration lives in `36`
}
```

## 12. Tree Invariants

A **well-formed** `SemanticPlan` satisfies every invariant below. The planner (`34`) is the canonical producer; every invariant below is the planner's contract. `SemanticPlan::validate()` (§13.3) is an optional post-hoc walker that reports violations as `Diagnostic<IrErrorKind>`.

### 12.1 Expression-wrapper invariants

- Every predicate-valued expression on a `PlanNode` is a `PhysicalExpr` — never a `SemanticExpr`. This applies to `FilterNode.predicate`, `ScanNode.filters_pushdown[*]`, `ProjectNode.projections[*].1`, `AggregateExpr.input_expr`, `AggregateExpr.filter`. Invariant rationale: per `[14 §3.7](../foundations/14_expressions.md)`, the leaf-set boundary makes this a type-level invariant — `PhysicalExpr = Expr<PhysicalLeaf>` literally cannot contain `Field` / `Dimension` / `Measure` / `Metric` / `Key` because those variants do not exist in `PhysicalLeaf` (§4.1). Semantic-leaf resolution completed at `compile` per `[19 §3](../foundations/19_expression_flow.md)`.
- No `PhysicalExpr` stored on a `FilterNode.predicate`, `ProjectNode.projections[*].1`, `ScanNode.filters_pushdown[*]`, or future `JoinNode` residual carries an `Expr::Aggregate` node — aggregation is lifted into `AggNode.aggregates` as `AggregateExpr` (§10.7) by the planner's Phase B aggregate-lift per `[19 §7](../foundations/19_expression_flow.md)`. The `Expr::Aggregate` structural variant exists at the type level (per `[14 §3.3](../foundations/14_expressions.md)`); the no-aggregate-in-predicate rule is a plan-tree-level invariant enforced by `34`'s lift pass.
- No `PhysicalExpr` reaching a `PlanNode` carries an `Expr::Window` node directly authored — `Window` is compile-emitted only per `[14 §3.3](../foundations/14_expressions.md)`, entering the tree exclusively through sugar-accessor elimination at compile. A `Window` node in a `PhysicalExpr` stored on a `PlanNode` predicate / projection slot is acceptable as long as it came from the canonical sugar-elimination path; planner-side window placement (e.g. wrapping window functions into a future `PlanNode::Window` variant) is post-v1.

### 12.2 Type-resolution invariants

- Every `PhysicalExpr` stored on every `PlanNode` is fully type-resolved: every leaf returns `Some(_)` from `ExprLeaf::inferred_type()`, and every structural node's type follows by canonical inference from its children. Type inference is part of compile (`[19 §3.6](../foundations/19_expression_flow.md)`). A leaf reaching the plan tree with `inferred_type() == None` is `IrErrorKind::UnresolvedType` (reported by `SemanticPlan::validate`).
- Every `AggregateExpr.inferred_type` is populated per the aggregate-typing rules implied by the registered aggregate signatures in `FunctionRegistry` per `[14a §3.3](../foundations/14a_function_catalog.md)`.

### 12.3 Scan-schema invariants

- `ScanNode.columns[*]` references actual columns of the resolved source. The planner populates `columns` from the SemanticManifest's `ResolvedBinding.sources[source_index].columns` — if the SemanticManifest is consistent with the plan, this invariant holds.
- `ScanNode.meta.output_schema.len() == ScanNode.columns.len()`.
- `ScanNode.meta.output_schema.fields[i].name == ScanNode.columns[i].name` for all `i`.

### 12.4 Push-down invariants

- Every `PhysicalExpr` in `ScanNode.filters_pushdown` references only columns in `ScanNode.columns` (enforced by adapter at `36`, optimizer at `34`).
- `filters_pushdown` does not change `meta.output_schema` — it narrows row count, not column shape.

### 12.5 Join invariants

- `JoinNode.on` is non-empty. (Cross-joins deferred per §16.1.)
- For each `KeyPair`, `left` resolves to a column in `left.meta().output_schema` and `right` resolves to a column in `right.meta().output_schema`.
- For each `KeyPair`, `left`'s column `data_type` matches `right`'s (modulo nullability). Type reconciliation is a planner responsibility (`15 §10.5` Cast-wrapping at SemanticManifest compile time per `[19 §3](../foundations/19_expression_flow.md)`); a mismatch reaching `35` is `IrErrorKind::JoinKeyTypeMismatch`.
- `JoinNode.meta.output_schema` = structural concatenation of `left.meta().output_schema` and `right.meta().output_schema`, with nullability widened on the outer side per `join_type` (per SQL semantics).

### 12.6 Union invariants

- `UnionNode.inputs.len() >= 2`.
- All inputs have structurally compatible output schemas: same arity; same `DataType` at each ordinal; same nullability at each ordinal (after upward widening of nullable-to-non-nullable mismatches by the planner).
- `UnionNode.meta.output_schema` = first input's schema with nullability widened across inputs (per SQL semantics).

### 12.7 Agg invariants

- Every `Name` in `AggNode.group_by` resolves to a column in `input.meta().output_schema`.
- Every `(Name, AggregateExpr)` in `AggNode.aggregates` has a unique output `Name`. Duplicate output-name is `IrErrorKind::DuplicateAggName`.
- The inner `input_expr` of each `AggregateExpr` references only columns in `input.meta().output_schema`.
- `AggNode.meta.output_schema` = one column per `group_by` entry (in that order) followed by one column per `aggregates` entry (in that order).

### 12.8 Sort invariants

- Every `Name` in `SortNode.order[*].0` resolves to a column in `input.meta().output_schema`.
- Pass-through schema: `SortNode.meta.output_schema == input.meta().output_schema` (cheap via `Arc` share).

### 12.9 Fetch invariants

- If `FetchNode.limit == Some(0)`, the adapter MAY short-circuit to an empty-relation emission (e.g. `SELECT ... FROM ... WHERE false`); this is an adapter choice, not a plan-tree invariant.
- Pass-through schema: `FetchNode.meta.output_schema == input.meta().output_schema`.

### 12.10 Filter invariants

- The `FilterNode.predicate`'s inferred type is `DataType::Boolean` (derived from leaf-level inference per §12.2). A non-Boolean predicate reaching `35` is `IrErrorKind::FilterPredicateNotBoolean`.
- Pass-through schema: `FilterNode.meta.output_schema == input.meta().output_schema`.

## 13. Visitor / Traversal API

### 13.1 `PlanVisitor`

```rust
/// Tree walker over a `SemanticPlan`. Implementations provide node
/// handlers; `PlanNode::walk_pre` / `walk_post` dispatch.
///
/// Equivalent to `Visitor<PlanNode>` per the universal trait surface in
/// `semstrait-core` (`14 §3.1`); preserved as a named alias for
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

### 13.2 Walk / transform free functions

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

### 13.3 `SemanticPlan::validate`

```rust
impl SemanticPlan {
    /// Full tree walk; re-checks every invariant in §12. Returns the
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

### 13.4 Typical usage patterns

**Count nodes of a variant.** Implement `PlanVisitor` with `Output = ()` and a counter field; let the default descend-children implementation do the work.

**Extract all `Scan`'s sources.** Implement `PlanVisitor` collecting `&SourceRef` from every `PlanNode::Scan(ScanNode { source, .. })`.

**Push-down rewrite.** Implement `transform` with a closure that matches `Filter { input: box Scan { .. }, predicate }` and rebuilds the subtree with the predicate in `filters_pushdown`.

**Schema re-check.** Implement `PlanVisitor<Output = Result<(), Diagnostic<IrErrorKind>>>` that recomputes `output_schema` for each variant and compares to `meta().output_schema`; return the first mismatch.

**Generic expression rewrite reuse.** Because `PlanNode` and `Expr<L>` share the `Tree` trait surface (§3.2), an optimizer rule that, say, constant-folds an `Expr<L>` subtree can use the same `transform` helper to rewrite an entire `PlanNode` subtree — one trait, two scales.

## 14. Serde / Substrait Mapping

### 14.1 Serde

Every public IR type derives `Serialize` / `Deserialize` under the crate-level `serde` feature flag (§16). `SemanticPlan` is the intended portable form: a serialized plan can be round-tripped across processes sharing the same SemanticManifest. Wire-form stability rules:

- Every `#[non_exhaustive]` enum serializes with a `kind` discriminator field (serde-tagged). Adding a variant preserves round-trip of existing variants.
- Every `#[non_exhaustive]` struct serializes with absent-field-tolerant deserialization. Adding a field preserves round-trip of existing values (new field defaults to its `Default::default` or `None`).
- `PhysicalExpr` and `SemanticExpr` serialize through the `Expr<L>` `#[derive(Serialize, Deserialize)]` machinery owned by this crate (§3.3). `PlanNode` and `Expr<L>` are the two `semstrait-ir`-owned `#[non_exhaustive]` enums that require serde-tagged discriminator wire form; everything else uses direct `#[derive(...)]`.

### 14.2 Substrait mapping table

The adapter crate (`36`) owns the bidirectional conversion between `SemanticPlan` and `substrait::proto::Plan`. `35` ratifies the **mapping** so both the crate's tests (round-trip), `36`'s emitter, and `36`'s deserializer agree on which `substrait::proto::Rel` corresponds to which `PlanNode`.

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

`SemAnnotation` on `NodeMeta.annotations` round-trips through Substrait's `AdvancedExtension.optimization` (URN `urn:semstrait:annotations:v1`) per `36 §4`.

The adapter is free to emit Substrait proto plans with extra hints (capacity, parallelism) in `AdvancedExtension.enhancement` slots; those hints are adapter-owned and not round-tripped through `35`.

## 15. Error Types

> **Migration note.** Body sections `§9`–`§13` retain references to legacy `IR_E_*` codes (e.g. `IR_E_3502 UnresolvedType`). Those codes are **retired** per `30 §5`; the public-API surface identifies errors by `IrErrorKind` variant identity. The legacy code prefixes remain in body prose during the migration as cross-reference anchors and will be stripped in a follow-up doc pass. Read `IR_E_NNNN VariantName` in the body as shorthand for `IrErrorKind::VariantName`.
>
> **Naming.** This crate exposes three error enums: the two narrow ir-emitted kinds (`ValidateError`, `CompileError`) and the plan-shape `IrErrorKind`. The first two carry no `Kind` suffix per the scoped cleanup tied to the second-cascade landing (`STATUS.md` item Q); the rest of the workspace's `*ErrorKind` enums (`IrErrorKind` here, `PlanErrorKind`, `AdaptErrorKind`, `ParseErrorKind`, …) keep the suffix until a future global rename pass.

### 15.1 `ValidateError` — raised by trait machinery

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

### 15.2 `CompileError` — raised by `FunctionSpec` machinery

```rust
/// Function-resolution diagnostic raised by `ReturnTypeRule::Custom`
/// callbacks wired into `FunctionSpec` (§7). Per `14a §3.5`.
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

### 15.3 `IrErrorKind`

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

    /// `Name::new` was called with a reserved plan-tree prefix (§10.4.1).
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

`IrErrorKind` is owned by `semstrait-ir`. It is distinct from `ir::CompileError` (§15.2, raised by `FunctionSpec` machinery), `ir::ValidateError` (§15.1, raised by trait machinery), and the downstream `manifest::CompileError` / `PlanErrorKind` / `AdaptErrorKind` — each has a different production site and a different lifecycle. A planner-side failure producing a malformed `SemanticPlan` is a `PlanErrorKind` (`34`); that same malformed plan caught by `SemanticPlan::validate()` on the consumer side becomes an `IrErrorKind`. All four surface as `Diagnostic<K>` envelopes for the appropriate `K` per `30 §5.1`.

### 15.4 Variant identity, not codes

The retired `IR_E_*` numeric range from earlier drafts is gone. Identification is by variant identity per `30 §5.4`; renaming or removing a variant is MAJOR per `30 §2.1`; adding a variant inside `#[non_exhaustive]` is MINOR per `30 §2.2`. The §2 module layout allocates a dedicated `error` module that co-locates `ValidateError`, `CompileError`, and `IrErrorKind` next to their production sites.

### 15.5 Warning posture

`semstrait-ir` itself, being pure data + validation, has no warning-emitting operation in v1. Warnings surfaced by planner or optimizer ride `PlanErrorKind` / `OptimizeErrorKind` envelopes per `34`; adapter warnings ride `AdaptErrorKind` per `36`. If a future v2 walker emits `Severity::Warning` `Diagnostic<IrErrorKind>` (e.g. an "unused PlanNode" advisory), it lands additively under `#[non_exhaustive]` per `30 §2.2`.

## 16. Stability

### 16.1 Stable parts

- **`PlanNode` variant set growth is non-breaking (I10).** Adding a variant (e.g. a future `Distinct`, `Window`, `Unnest`, `TopN`) is MINOR. Consumers that pattern-match exhaustively on `PlanNode` will compile-error by design — the `#[non_exhaustive]` attribute forces them to add a fallback arm.
- **Struct field addition inside a `PlanNode` variant is non-breaking.** Every variant's struct is `#[non_exhaustive]` per §9; adding a new field with a sensible default (`None`, `Vec::new()`, `0`, `false`) is MINOR. Examples: `JoinNode.residual: Option<PhysicalExpr>` for non-equi joins; `ScanNode.order_hint: Option<Vec<(Name, SortDir)>>` for order-preserving scans.
- **`Expr<L>` structural-variant additions are non-breaking** under the `14 §3.3` `#[non_exhaustive]` discipline. Adding e.g. a `Try` / `Filter` / `Match` variant in a future spec rev is MINOR; consumers pattern-matching `Expr<L>` must add a fallback arm.
- **`PhysicalLeaf` / `SemanticLeaf` variant additions are non-breaking** per the same discipline.
- **Per-kind accessor enum variant additions are non-breaking** (each accessor enum is `#[non_exhaustive]`).
- **`ParameterKey` variant additions are non-breaking** — adding new internal parameter keys for future sugar-elimination patterns is MINOR.
- **`FunctionRegistry` content growth is non-breaking** — new entries added via `RegistryExtension::extend` at startup are part of the registry's runtime state, not its type surface.
- **`DialectId` const additions are non-breaking.** Adding a new `pub const` on `DialectId` is MINOR.
- **`SemAnnotation` variant additions are non-breaking** (annotation roster growth is expected as `34` matures).
- **Variant additions to `IrErrorKind` inside `#[non_exhaustive]` are non-breaking** per `30 §2.2`.
- **Substrait mapping table entries are non-breaking** — adding a new `PlanNode` variant with a corresponding Substrait `Rel` kind is MINOR; changing an existing mapping is MAJOR.

### 16.2 Internal parts

- **`NodeMeta.node_id` values** are not stable across planner invocations. Consumers relying on stable identity across runs should derive identity from the plan-tree shape (e.g. a tree-hash visitor), not from `node_id`.
- **`SemanticPlan::validate()`'s error-ordering** is not stable. The first violation reported may shift between releases as `validate` reorders its checks for performance; consumers SHOULD treat any `Diagnostic<IrErrorKind>` as a single bad-plan signal, not a "first problem is X" guarantee.
- **Serde's on-wire shape under `#[non_exhaustive]` enums** follows the serde-tagged convention (§14.1). The exact JSON spelling of a `kind` discriminator is stable across MINOR releases; deserializers MUST be tolerant to unknown variant tags (typically mapping unknowns to a skipped-node error rather than panicking).

### 16.3 Delta with current code

The `crates/semstrait-ir/src/plan/node.rs` definitions exist today as `LogicalPlan` + `PlanNode` per `[TD-IR-RENAME]`. Target state for the plan-tree surface:

- `LogicalPlan` → rename to `SemanticPlan` (matches `00 §4.1` vocabulary).
- Local `JoinType` / `SortDirection` enums → drop in favor of canonical `16 §5.2` `JoinType` re-exported through `semstrait-core` and the `SortDir` + `NullOrdering` types from §10.6.
- Every `pub enum` in `plan/` → add `#[non_exhaustive]`.
- `ScanNode.location` / `ScanNode.format` → drop in favor of the opaque `SourceRef` (§10.2); path + format resolution moves to `36`.
- Add `filters_pushdown: Vec<PhysicalExpr>` to `ScanNode`.

Target state for the absorbed-from-`semstrait-core` surface (per `[14 §9.2](../foundations/14_expressions.md)`'s placement contract):

- Move `Expr` / `SemanticExpr` / `PhysicalExpr` / `ExprSource`-related vocabulary from `semstrait-core` to `semstrait-ir`. New modules: `expr` (with `tree`, `leaves`, `accessor`, `parameter`, `expr_fn` submodules) and `functions`.
- Replace the legacy `Expr` flat-enum + `SemanticExpr` / `PhysicalExpr` wrapper-struct pattern with the parameterized `Expr<L>` + type-alias pattern per `[14 §3.6](../foundations/14_expressions.md)`. Retire the wrapper-struct `inferred_type` / `referenced_columns` fields; per-leaf `ExprLeaf::inferred_type()` plus `ResolvedExprEntry` (per `[19 §3.2](../foundations/19_expression_flow.md)`) take over.
- Replace the retired `SemanticLeaf::EntityRef` / `Access` / outer `Accessor` enum vocabulary with the per-kind typed semantic leaves + `Option<XxxAccessor>` fields per `[14 §3.5](../foundations/14_expressions.md)` / `[§4.1](../foundations/14_expressions.md)`.
- Move `CanonicalFn`, `FunctionRegistry`, `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`, `RegistryExtension`, and `function_registry()` from `semstrait-core::functions` to `semstrait-ir::functions` per `[14a §2](../foundations/14a_function_catalog.md)`.
- Add the authoring-surface DSL (`expr_fn` constructors, `std::ops` impls on `Expr<L>`, `ExprFunctionExt` trait) per §6.

Migration items are tracked in `implementation/40_refactor_plan.md` under `[TD-IR-RENAME]`, `[TD-IR-NONEXHAUSTIVE]`, and `[TD-IR-ABSORB-EXPR]` (new — covers the `semstrait-core` → `semstrait-ir` movement of expression types + function registry).

## 17. Crate Boundaries

### 17.1 What `semstrait-ir` does NOT do

- **No planning.** `semstrait-ir` contains no `fn plan(manifest, request) -> SemanticPlan`. Planning logic (strategy dispatch, per-DataKind expansion, Relationship-graph traversal, constraint checking) all live in `semstrait-planner` per `34`.
- **No compile-time resolution.** `semstrait-ir` contains no `fn resolve(semantic_expr, ctx) -> PhysicalExpr`. The `SemanticExpr::resolve` entry point and the substep algorithm (sugar elimination, reference substitution, fold, reconciliation) live in `semstrait-manifest::compile` per `[19 §3](../foundations/19_expression_flow.md)`.
- **No optimization.** `semstrait-ir` contains no `fn optimize(plan) -> SemanticPlan`. Canonical optimizer passes (constant folding, predicate pushdown, metadata-dimension substitution) live in `semstrait-planner` per `34 §5`.
- **No emission.** `semstrait-ir` contains no `fn adapt(plan) -> EngineArtifact`. Adapter emission (SQL rendering, Substrait proto building, capability checking) lives in `semstrait-adapter` per `36`.
- **No I/O.** No `std::fs`, no `reqwest`, no `tokio`. Every method on every public type is synchronous and pure. I11 guarantee.
- **No engine identity.** No adapter-specific logic inside `PlanNode` variants, `Expr<L>` variants, or `FunctionRegistry`. `Scan` carries `SourceRef` (opaque); `Join` carries `JoinType` (canonical, not engine-specific); `Filter.predicate` is `PhysicalExpr` (canonical, not SQL text); `CanonicalFn` is engine-neutral. I1 / I3 guarantees.
- **No SemanticManifest construction.** `semstrait-ir` consumes `SourceRef`s that reference an external SemanticManifest but never constructs one. SemanticManifest construction is `semstrait-manifest`'s responsibility per `33`.

### 17.2 Dependency posture

```toml
[dependencies]
semstrait-core = { path = "../semstrait-core" }
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
serde   = ["dep:serde", "semstrait-core/serde"]
```

**No runtime-only dependencies.** No `tokio`, `async-trait`, `futures`, `reqwest`, `hyper`, `sqlx`.
**No engine dependencies.** `substrait` (a proto codegen crate) is permitted because `EnginePlan::Substrait(Box<substrait::proto::Plan>)` is structural, not engine-identity. No `datafusion`, no `arrow`, no `duckdb`, no `spark-*`.
**No in-workspace dependencies beyond `semstrait-core`.** CI-enforced manifest audit per `30 §9`.

## 18. Invariants Upheld by the Crate

| Invariant | `semstrait-ir` guarantee |
|---|---|
| **I1** — no raw SQL in canonical layer | `PlanNode` variants carry `PhysicalExpr` for every predicate; `Name` for every column / key identifier; `SourceRef` (opaque) for every source. `Expr<L>` and its leaf sets are typed trees — no `String`-as-SQL field exists on any structural variant or leaf. `SqlArtifact.text` exists, but it is an *adapter output*, not a *plan content*. |
| **I2** — physical types belong to adapters | `SemanticPlan` and `Expr<L>` reference only `DataType` (canonical, re-exported from `semstrait-core`). `EnginePlan::Substrait(Box<substrait::proto::Plan>)` carries engine-specific types, but it is an *adapter output*, not an input to or content of a `SemanticPlan`. |
| **I3** — no engine-identity branching in canonical types | `PlanNode` has zero variants keyed by adapter / dialect. `Expr<L>::FunctionCall` carries `CanonicalFn` (engine-neutral) — engine-specific operators land as registry-extension entries per `[14a §7](../foundations/14a_function_catalog.md)`, not as new `Expr<L>` variants. The only engine-identity value anywhere in `semstrait-ir` is `DialectId`, and it appears only on `SqlArtifact` (adapter output) / `Dialect::ID` (adapter-trait associated constant). |
| **I5** — name resolution at compile time | `SemanticLeaf::Field` / `Dimension` / `Measure` / `Metric` / `Key` carry unresolved names at parse and are resolved at compile per `[19 §3](../foundations/19_expression_flow.md)`. `PhysicalLeaf` carries no semantic names, only binding-resolved `ColumnRef`s and compile-emitted `Parameter`s. The leaf-set boundary makes the "no semantic refs in PhysicalExpr" rule a type-level invariant per `[14 §3.7](../foundations/14_expressions.md)`. |
| **I6** — plan hot path is synchronous | **No `pub async fn` exists on `semstrait-ir`.** Every method on every public type — including `Expr<L>` traversal, `FunctionRegistry` lookup, `PlanNode` walking, and `SemanticPlan::validate` — is synchronous. CI lint + `forbid_async_fn!` macro audit guard the crate. |
| **I7** — strict DAG | `Cargo.toml` lists `semstrait-core` as the only internal workspace dependency. CI check greps for any other `semstrait-*` entry. The expression types + registry absorbed from `semstrait-core` at the `14` second-refinement landing do not change the DAG — `semstrait-core` remains the workspace leaf carrying primitives + trait scaffolding + support enums. |
| **I10** — extensibility | Every `pub enum` and `pub struct` carries `#[non_exhaustive]` except the newtype-over-stable set: `Name`, `SourceRef`, `DialectId`, `NodeId`, `CanonicalFn`. An `integration-test` over `cargo public-api` enforces the rule. |
| **I11** — no downward I/O surprises | No `std::fs`, no `std::net`, no `tokio`, no `reqwest` anywhere in the crate. `substrait`'s `prost` dependency is bytes-encoding only, not I/O. |
| **I12** — first-class diagnostics | `Diagnose` implemented on `IrErrorKind`, `ValidateError`, and `CompileError` per `30 §5.4`; identification is by variant identity. The blanket `Display` and `std::error::Error` impls on `Diagnostic<K>` (per `30 §5.5`) make `Diagnostic<IrErrorKind>` directly usable as a `std::error::Error` value. Registry-side construction-time errors raised by trait / `FunctionSpec` machinery flow through `ir::ValidateError` / `ir::CompileError` (§15.1 / §15.2). |

## 19. Public API Surface Sketch

### 19.1 `expr`

```
pub enum   Expr<L: ExprLeaf>                             // §3.3
pub type   PhysicalExpr = Expr<PhysicalLeaf>             // §4.3
pub type   SemanticExpr = Expr<SemanticLeaf>             // §4.3

pub enum   PhysicalLeaf                                  // §4.1
pub enum   SemanticLeaf                                  // §4.2

pub enum   DimensionAccessor                             // §5.1
pub enum   MeasureAccessor                               // §5.1
pub enum   MetricAccessor                                // §5.1
pub enum   KeyAccessor                                   // §5.1

pub struct Parameter                                     // §5.2
pub enum   ParameterKey                                  // §5.2

pub mod expr_fn {                                        // §6.1
    pub trait FromColumnRef;
    pub fn col<E: FromColumnRef>(name: impl Into<String>) -> E;
    pub fn field(name: impl Into<String>) -> SemanticExpr;
    pub fn dim(name: impl Into<String>) -> SemanticExpr;
    pub fn measure(name: impl Into<String>) -> SemanticExpr;
    pub fn metric(name: impl Into<String>) -> SemanticExpr;
    pub fn key(name: impl Into<String>) -> SemanticExpr;
}

pub trait ExprFunctionExt                                // §6.3
impl      ExprFunctionExt for SemanticExpr
impl      ExprFunctionExt for PhysicalExpr

// std::ops impls per §6.2
impl<L: ExprLeaf> Add | Sub | Mul | Div | Rem | BitAnd | BitOr | Neg | Not for Expr<L>
```

### 19.2 `functions`

```
pub struct CanonicalFn                                   // §7.2
pub struct FunctionRegistry                              // §7.2
pub struct FunctionSpec                                  // §7.2
pub struct FnSignature                                   // §7.2
pub enum   ParamType                                     // §7.2
pub enum   ReturnTypeRule                                // §7.2
pub enum   FunctionCategory                              // §7.2
pub trait  RegistryExtension                             // §7.2
pub fn     function_registry() -> &'static FunctionRegistry;  // §7.2
```

### 19.3 `plan`

```
pub struct SemanticPlan                                  // root; { root, output_names, diagnostics }
pub enum   PlanNode                                      // 8 variants per §9
pub struct ScanNode                                      // §9.2
pub struct FilterNode                                    // §9.3
pub struct ProjectNode                                   // §9.4
pub struct AggNode                                       // §9.5
pub struct JoinNode                                      // §9.6
pub struct UnionNode                                     // §9.7
pub struct SortNode                                      // §9.8
pub struct FetchNode                                     // §9.9
pub struct NodeMeta                                      // §10.1
pub struct NodeId                                        // newtype over Uuid
pub struct Schema                                        // plan-level schema; { fields }
pub struct Field                                         // { name, data_type, nullable }
pub enum   SemAnnotation                                 // #[non_exhaustive]; AggregateRole, FilterSource, ...
```

### 19.4 `plan::traversal`

```
pub trait  PlanVisitor                                   // visit(&PlanNode) -> Self::Output
pub trait  PlanVisitorMut                                // visit_mut(&mut PlanNode) -> Self::Output
```

### 19.5 `primitives`

```
pub struct SourceRef                                     // opaque handle; §10.2
pub struct ResolvedColumn                                // §10.3
pub struct Name                                          // newtype over String; §10.4
pub struct KeyPair                                       // §10.5
pub enum   SortDir                                       // Asc | Desc with NullOrdering; §10.6
pub enum   NullOrdering                                  // First | Last | Unspecified
pub struct AggregateExpr                                 // §10.7
pub use    semstrait_core::{Cardinality, JoinType}       // re-exported from 16 §5 per `authoritative-for`
```

### 19.6 `artifact`

```
pub enum   EngineArtifact                                // Sql | Plan
pub enum   EnginePlan                                    // Substrait
pub struct SqlArtifact                                   // { text, dialect }
pub struct DialectId                                     // newtype; ANSI | DATAFUSION | DUCKDB | SPARK
pub trait  Dialect                                       // ID const + capabilities()
pub enum   Capability                                    // #[non_exhaustive]; roster owned by 36
```

### 19.7 `error`

```
pub enum   ValidateError                                 // raised by Tree::with_new_children + Rewriter<N>::f_*
pub enum   CompileError                                  // raised by ReturnTypeRule::Custom callbacks
pub enum   IrErrorKind                                   // plan-shape diagnostics; 14 variants in v1
impl       semstrait_core::diagnostic::Diagnose for ValidateError
impl       semstrait_core::diagnostic::Diagnose for CompileError
impl       semstrait_core::diagnostic::Diagnose for IrErrorKind
```

### 19.8 Free functions / inherent impl methods at crate root

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

### 19.9 Crate-root re-exports

```rust
// lib.rs
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
    NodeMeta, NodeId, Schema, Field, SemAnnotation,
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

// Re-exports from semstrait-core that `35`-authoritative surfaces rely on:
pub use semstrait_core::{
    DataType, Grain, TypeClass, Schema, SchemaColumn,
    Cardinality, JoinType,
};
```

