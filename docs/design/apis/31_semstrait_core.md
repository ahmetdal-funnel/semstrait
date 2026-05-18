---
prereqs: [13, 14, 30]
authoritative-for:
  - the `semstrait-core` public-API surface (types, traits, free functions) after the `14` second-refinement cascade — slimmed to primitives + trait scaffolding
  - module layout within `semstrait-core` (top-level `pub mod`s and their split rationale)
  - the universal-traversal trait family: the `Tree` trait, the `Visitor<N>` / `Rewriter<N>` companion traits, and their default-derived helpers (`apply`, `transform`)
  - the `ExprLeaf` trait (per-leaf-set metadata contract consumed by every `Expr<L>` instantiation owned by `semstrait-ir`)
  - the `DataType`-family visibility surface (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`) and the feature-gated serde derivations
  - the structural-variant support enums shared by every leaf set — `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `Literal`
  - the shared identifier carriers `ColumnRef` and `SemanticsName` (consumed by both leaf sets in `semstrait-ir` per `14 §3.4 / §3.5`)
  - the constraint-DSL toolkit types (`MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints`) exposed at `semstrait-core`
  - the cross-cutting diagnostic primitives (`Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose` trait) and the narrow core-emitted kind enums (`ValidateErrorKind`, `CompileErrorKind`) that live in `semstrait-core`
  - feature-flag surface (`serde`, `schemars`, `io`, `io-aws`) and dependency posture — under default features core pulls `tokio`; under `--no-default-features` it retains the original zero-runtime-dep shape
  - mapping of design invariants I1, I2, I5, I6, I7, I10, I11, I12 to concrete crate-level guarantees
refined-by:
  - 14 (the type-architecture contract — `Tree` / `Visitor` / `Rewriter` / `ExprLeaf` trait surfaces, support-enum rosters, leaf-set boundary — `31` carries the implementation)
  - 31b (`semstrait-core::io` — text-blob transport module, amends §1.3 / §2 / §11 / §12 of this doc)
  - 32 (`semstrait-model` owns `ExprSource`, `ExprBlock`, reserved-tag dispatch, `is_reserved_tag`; declares its own `ParseErrorKind` and `ValidateErrorKind` embedding `Core(core::ValidateErrorKind)`; adds `io` wrappers over `31b`)
  - 33 (`semstrait-manifest` declares its own `CompileErrorKind` embedding `Core(core::CompileErrorKind)`; adds `RepositoryErrorKind`; adds `io` wrappers over `31b`)
  - 34 (`semstrait-planner` consumes the sealed `FunctionRegistry` and resolved `PhysicalExpr`s at plan time; declares `PlanErrorKind` / `OptimizeErrorKind`)
  - 35 (`semstrait-ir` owns the canonical-IR expression types `Expr<L>` / `PhysicalLeaf` / `SemanticLeaf` / `PhysicalExpr` / `SemanticExpr`, the per-kind accessor enums, `Parameter`, the authoring-surface DSL constructors, and the `CanonicalFn` / `FunctionRegistry` family — moved from `semstrait-core` per `14 §9.2`)
  - 36 (`semstrait-adapter` contributes `RegistryExtension` impls; declares `AdaptErrorKind`)
  - 38 (`semstrait-api` declares the sum-typed `SemStraitErrorKind` lifting per-stage kinds)
  - 40 (`implementation/40_refactor_plan.md` — current code vs target layout delta is tracked here)
---

# 31. semstrait-core

> **Status:** rebase landing (2026-05-18). Cascade absorption of `[14](../foundations/14_expressions.md)`'s second-refinement landing: `semstrait-core` is now the **trait-scaffolding + shared-primitives crate**. The canonical-IR expression types (`Expr<L>`, leaf sets, type aliases, per-kind accessor enums, `Parameter`, authoring-surface DSL constructors) and the `CanonicalFn` / `FunctionRegistry` family have moved to `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)` and `[35](35_semstrait_ir.md)`. The `ExprSource` YAML carrier, `ExprBlock`, reserved-tag dispatch, and `is_reserved_tag` helper have moved to `semstrait-model` per `[14 §9.3](../foundations/14_expressions.md)` and `[32](32_semstrait_model.md)`. `31`'s post-cascade surface is: the universal-traversal trait family (`Tree` / `Visitor` / `Rewriter` / `ExprLeaf`), the canonical logical-type vocabulary (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`), the structural-variant support enums shared by every leaf set (`BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, `Literal`), the shared identifier carriers (`ColumnRef`, `SemanticsName`), the constraint DSL, the cross-cutting diagnostic primitives, and the `io` transport per `31b`.

## 1. Purpose and Scope

`semstrait-core` is the **trait-scaffolding + primitives crate** every layer above it consumes. It owns the **universal-traversal trait surface** (`Tree` / `Visitor` / `Rewriter` / `ExprLeaf`), the **logical type system** (`DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`), the **structural-variant support enums** shared by every leaf set (`BinaryOpKind`, `UnaryOpKind`, …, `Literal`), and the cross-cutting **diagnostic / error primitives** that flow across stage boundaries. It contains no canonical-IR expression types (those live in `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)`), no function registry (also `semstrait-ir`), no parsing (`semstrait-model`), no planner logic, no adapter logic — just the trait scaffolding and shared vocabulary every consumer agrees on.

### 1.1 What `semstrait-core` OWNS

- The **universal-traversal trait family** (`§3`): the `Tree` trait — implemented by `Expr<L>` and `PlanNode` (both `semstrait-ir`) — and its `Visitor<N>` / `Rewriter<N>` companions. Per `[14 §3.1](../foundations/14_expressions.md)`.
- The **`ExprLeaf` trait** (`§3`): per-leaf-set metadata contract that `PhysicalLeaf` and `SemanticLeaf` (`semstrait-ir`) implement. Per `[14 §3.2](../foundations/14_expressions.md)`.
- The canonical **logical type system** (`§4`): `DataType`, `Grain`, `TypeClass` per `[13](../foundations/13_types_and_grain.md)`, plus `Schema` and `SchemaColumn` per `[15 §3.2](../foundations/15_mapping_and_binding.md)`.
- The **structural-variant support enums** shared by every `Expr<L>` instantiation (`§5`): `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`, `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`, and the `Literal` typed-value carrier. Rosters per `[14 §3.3](../foundations/14_expressions.md)`.
- The shared **identifier carriers** referenced by both leaf sets (`§5`): `ColumnRef` and `SemanticsName`.
- The constraint-DSL toolkit (`§6`): `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints` (per `[11 §8.3](../foundations/11_constraints.md)` / `§8.4`).
- The cross-cutting **diagnostic primitives** per `[30 §5](../foundations/30_stability_diagnostics.md)` (`§7`): `Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose`.
- The narrow core-emitted **kind enums** (`§8`): `ValidateErrorKind` (raised by `Tree::with_new_children` and `Rewriter<N>::f_*`) and `CompileErrorKind` (raised by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec` over in `semstrait-ir`). Each implements `Diagnose`; downstream stages MAY embed via D.ii kind-nesting (`[30 §7.4](../foundations/30_stability_diagnostics.md)`).
- The `io` module per `[31b](31b_semstrait_core_io.md)` (`§2` / `§11` / `§12.1` / `§14.8`): byte-blob transport primitives. Unchanged by the `14` cascade.

### 1.2 What `semstrait-core` does NOT own

- **The canonical-IR expression types.** `Expr<L>`, `PhysicalLeaf`, `SemanticLeaf`, `PhysicalExpr`, `SemanticExpr`, the per-kind accessor enums (`DimensionAccessor`, `MeasureAccessor`, `MetricAccessor`, `KeyAccessor`), and `Parameter` / `ParameterKey` live in `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §3](35_semstrait_ir.md)` / `[§5](35_semstrait_ir.md)`. `31` exposes only the trait scaffolding those types implement and the support enums that flow through their structural variants.
- **The authoring-surface DSL.** The `expr_fn` module (`col` / `field` / `dim` / `measure` / `metric` / `key`), the `std::ops` impls on `SemanticExpr` / `PhysicalExpr`, and the `ExprFunctionExt` extension trait live in `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §6](35_semstrait_ir.md)`.
- **The function-registry family.** `CanonicalFn`, `FunctionRegistry`, `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`, `RegistryExtension`, `function_registry()` live in `semstrait-ir` per `[14 §9.2](../foundations/14_expressions.md)` and `[35 §7](35_semstrait_ir.md)`.
- **The YAML authoring surface.** `ExprSource`, `ExprBlock`, the reserved-tag catalog, parse-site dispatch (`parse_semantic` / `parse_physical`), and `is_reserved_tag(&str) -> bool` live in `semstrait-model` per `[14 §9.3](../foundations/14_expressions.md)` and `[32](32_semstrait_model.md)`.
- **SemanticManifest structure.** `SemanticManifest`, `ResolvedDataKind`, `ResolvedExprTable`, `ResolvedSource`, `ResolvedColumnMapping` live in `semstrait-manifest` per `[33](33_semstrait_manifest.md)`.
- **Planner + plan tree.** `SemanticPlan`, `PlanNode`, plan-time `Request` / `SessionContext`, `PlanErrorKind`, `OptimizeErrorKind` live in `semstrait-ir` (plan types) and `semstrait-planner` (stages) per `[34](34_semstrait_planner.md)` / `[35](35_semstrait_ir.md)`.
- **Engine identity and dialect.** `EngineArtifact`, `EngineAdapter`, `Dialect`, `DialectId`, `EnginePlan`, `SqlArtifact`, `AdaptErrorKind` live in `semstrait-adapter` per `[36](36_semstrait_adapter.md)`.
- **Catalog and filesystem.** `CatalogProvider`, `FileSystem`, `Repository`, `CatalogSnapshot` live in `semstrait-catalog` per `[37](37_semstrait_catalog.md)`.
- **Name resolution, scope chains, shape unification.** Algorithms live in `semstrait-model` (`validate`) and `semstrait-manifest` (`compile`); their kind enums MAY embed `Core(core::ValidateErrorKind)` / `Core(core::CompileErrorKind)` per D.ii.
- **Domain load / dump wrappers.** `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs` live in `semstrait-model::io` (`[32 §10.4](32_semstrait_model.md)`); `load_manifest` / `dump_manifest` live in `semstrait-manifest::io` (`[33](33_semstrait_manifest.md)`). `semstrait-core::io` owns only the text-blob transport; the domain crates own the format.

### 1.3 Design posture — slimmed shared-vocabulary crate

`semstrait-core` is deliberately **minimal**, and the `[14](../foundations/14_expressions.md)` second-refinement cascade has slimmed it further. It exists to keep `DataType`, the universal-traversal trait surface (`Tree` / `Visitor` / `Rewriter` / `ExprLeaf`), the shared support enums, the diagnostic envelope, and the `Diagnose` trait definable without pulling in the IR / model / manifest / planner / adapter crates. If a type is not needed by two or more downstream crates *and* does not belong to a single owning crate further up the DAG, it does not belong here. Engine-identity deps (datafusion, arrow, duckdb, substrait) are rejected outright.

Three structural ejections landed at the cascade: the **canonical-IR types** (`Expr<L>`, both leaf sets, type aliases, per-kind accessor enums, `Parameter`) moved to `semstrait-ir`; the **function registry** family (`CanonicalFn`, `FunctionRegistry`, `FunctionSpec`, `RegistryExtension`, `function_registry()`) moved to `semstrait-ir`; the **YAML authoring surface** (`ExprSource`, `ExprBlock`, `is_reserved_tag`) moved to `semstrait-model`. `31` retains the trait scaffolding those types depend on and the support enums they reference structurally.

The crate remains the **leaf** of the semstrait workspace DAG (I7): zero workspace dependencies; every other crate depends on it directly or transitively.

**I/O amendment (ratified in `[31b](31b_semstrait_core_io.md)`).** The `io` module provides the shared transport vocabulary that every downstream load / dump wrapper composes. Under default features (`io` ON), `semstrait-core` pulls `tokio`. Under `--no-default-features`, the crate retains its original zero-runtime-dep posture. Cloud SDKs (`aws-sdk-s3`, future `gcs` / `azure`) sit behind additional opt-in flags. The "no async, no I/O in core" blanket from earlier drafts is replaced by: "text-blob transport is a first-class core concern; domain-specific wrappers are not." Unchanged by the `14` cascade.

## 2. Module Layout

Top-level `pub mod` structure after the `14` cascade. One module per cohesive concept; no cross-module cycles; no `pub use` re-exports of internal modules outside this table.

```
semstrait-core
├── tree                 // Tree trait + Visitor<N> / Rewriter<N> companions + ExprLeaf trait
├── types                // DataType, Grain, TypeClass, Schema, SchemaColumn
├── expr_kinds           // BinaryOpKind, UnaryOpKind, AggregationOp, LikeKind,
│                        //   CastFailure, WindowFn, WindowFrame, Literal,
│                        //   ColumnRef, SemanticsName
├── constraints          // MeasureConstraints, DimensionConstraints, AggregationConstraints
├── diagnostic           // Diagnostic<K>, Diagnostics<K>, Severity, Location, Span,
│                        //   SourceId, Diagnose trait
├── error                // ValidateErrorKind, CompileErrorKind
│                        //   (narrow, core-emitted only; stages downstream
│                        //    may embed via D.ii kind-nesting)
└── io                   // Source, Sink, Location, IoError + backends::{memory, local, s3}
                         //   (feature "io", default ON; s3 under "io-aws")
                         //   Full spec: 31b
```

Post-cascade roster is **seven modules** (was nine). Departed: `expr` and `functions` (both now in `semstrait-ir`). New: `tree` (lifted from the older `expr::visit` to the top level since it serves both `Expr<L>` and `PlanNode` per `[35 §3.2](35_semstrait_ir.md)` / `[§13](35_semstrait_ir.md)`). The support enums previously nested under `expr::types` migrate to a top-level `expr_kinds` module.

**Split rationale:**

- `tree` vs `expr_kinds` — the trait surface (`Tree`, `Visitor`, `Rewriter`, `ExprLeaf`) is conceptually independent of the support-enum roster: `Tree` works for `PlanNode` (which never references `BinaryOpKind`) just as well as for `Expr<L>`. Isolating the traits limits the I10 blast radius when a support-enum variant lands.
- `expr_kinds` vs `types` — `expr_kinds` carries enums that change at the cadence of expression-shape evolution (a new `BinaryOpKind::ConcatString` lands when SQL `||` gains canonical status); `types` carries types that change at the cadence of canonical-type-system evolution (a new `DataType::List` lands with collection support per `[13 §2.5](../foundations/13_types_and_grain.md)`). Separating lets downstream consumers import one without the other's recompile cost.
- `constraints` — its own module because `MeasureConstraints` binds to Measure and Metric carriers, is referenced by model and planner alike, and carries its own `serde` derivations (`[11 §8.4.3](../foundations/11_constraints.md)`).
- `diagnostic` vs `error` — generic envelope (`Diagnostic<K>`, `Diagnose`) split from the two narrow core-emitted kind enums (`ValidateErrorKind`, `CompileErrorKind`), reinforcing `[30 §5](../foundations/30_stability_diagnostics.md)`'s typed-kind-per-stage discipline.
- `io` — full split rationale and back-end roster live in `[31b §2](31b_semstrait_core_io.md)`.

**Re-exports.** The crate root (`lib.rs`) re-exports a curated surface (§14). Non-root re-exports of internal helpers are forbidden — consumers either import `semstrait_core::Tree` or `semstrait_core::tree::Tree`, never both.

## 3. Public Trait Family — `Tree`, `Visitor`, `Rewriter`, `ExprLeaf`

### 3.1 Where the trait architecture lives

The universal-traversal trait family is ratified by `[14 §3](../foundations/14_expressions.md)`:

- `[14 §3.1](../foundations/14_expressions.md)` ratifies the `Tree` trait and its `Visitor<N>` / `Rewriter<N>` companions, plus the default-derived helpers (`apply`, `transform`).
- `[14 §3.2](../foundations/14_expressions.md)` ratifies the `ExprLeaf` trait.

`31` is the **crate** that holds the implementation. Per `[14 §9.1](../foundations/14_expressions.md)`, this trait family lives in `semstrait-core` because both `Expr<L>` (`semstrait-ir`, per `[14 §9.2](../foundations/14_expressions.md)` / `[35 §3](35_semstrait_ir.md)`) and `PlanNode` (`semstrait-ir`, per `[35 §13](35_semstrait_ir.md)`) implement these traits, so they MUST be definable upstream of either consumer. `31` does not re-ratify the trait surface; the surface is `[14 §3](../foundations/14_expressions.md)`'s contract and any change to it lands in `14` first, then cascades here.

### 3.2 The `Tree` trait

```rust
/// Universal traversal contract for any tree-shaped value. Implemented by
/// `Expr<L>` (`semstrait-ir`, per `35 §3.3`) and by `PlanNode`
/// (`semstrait-ir`, per `35 §13.1`). Stage-agnostic.
///
/// Per `14 §3.1`.
pub trait Tree: Sized {
    /// Borrowed children of this node, in left-to-right structural order.
    /// Leaves return an empty `Vec`.
    fn children(&self) -> Vec<&Self>;

    /// Reconstruct this node with a new child list. The implementation
    /// MUST preserve the node's variant tag and any non-child fields;
    /// only the children list changes.
    ///
    /// Returns `ValidateErrorKind` when the new child list is structurally
    /// invalid for this node (e.g. a `BinaryOp` reconstructed with three
    /// children, an `Aggregate` reconstructed with zero args).
    fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateErrorKind>;
}
```

Two default-derived helpers extend the contract without expanding the trait method count (I10 sensitivity):

```rust
impl<T: Tree> T {
    /// Pre-order read-only walk driven by a `Visitor<Self>`. Short-circuits
    /// on `ControlFlow::Break`.
    pub fn apply<V: Visitor<Self>>(&self, v: &mut V) -> V::Output { /* default body */ }

    /// Bottom-up rewrite. The closure runs on every node in post-order,
    /// producing a new value; structural failures propagate.
    pub fn transform<F>(self, f: F) -> Result<Self, ValidateErrorKind>
    where F: FnMut(Self) -> Result<Self, ValidateErrorKind> { /* default body */ }
}
```

`apply` and `transform` live as `impl<T: Tree>` default methods (not as separate trait methods) so adding a new helper later is non-breaking. Per `[14 §3.1](../foundations/14_expressions.md)`, this surface is intentionally narrow — every algorithmic pass in the pipeline (compile, plan, adapt) composes these primitives rather than adding new trait methods.

### 3.3 The `Visitor<N>` and `Rewriter<N>` companion traits

```rust
use std::ops::ControlFlow;

/// Pre-/post-order analysis hook. Implementations decide the output type;
/// `f_down` / `f_up` may signal early termination via `ControlFlow::Break`.
///
/// Per `14 §3.1`.
pub trait Visitor<N> {
    type Output;
    fn f_down(&mut self, node: &N) -> ControlFlow<Self::Output>;
    fn f_up(&mut self,   node: &N) -> ControlFlow<Self::Output>;
}

/// Pre-/post-order rewrite hook. Owned-mutation form for value-producing
/// passes (constant folding, sugar elimination, …).
///
/// Per `14 §3.1`.
pub trait Rewriter<N> {
    fn f_down(&mut self, node: N) -> Result<N, ValidateErrorKind>;
    fn f_up(&mut self,   node: N) -> Result<N, ValidateErrorKind>;
}
```

The `f_down` / `f_up` shape is compatible with the pattern documented for canonical tree-traversal libraries (DataFusion's `TreeNode`, Calcite's `RexVisitor`); per `[14 §3.1](../foundations/14_expressions.md)`, this shape was chosen deliberately so downstream consumers can port existing visitor logic with minimal rewrites.

Both traits are **non-sealed** — third-party analysis / rewrite passes (e.g. a test-harness counting `Aggregate` nodes, an external optimizer rule plugged into `semstrait-planner`) MUST be able to impl them without a sealed-trait escape hatch.

### 3.4 The `ExprLeaf` trait

```rust
/// Per-leaf-set metadata contract. Implemented by `PhysicalLeaf` and
/// `SemanticLeaf` (`semstrait-ir`, per `35 §4`). Used by `Expr<L>` and
/// every algorithmic pass that walks expression trees.
///
/// Per `14 §3.2`.
pub trait ExprLeaf: Sized + Clone + Debug {
    /// Canonical logical type carried (or inferred) by this leaf. Returns
    /// `None` only when type cannot be determined locally — e.g. an
    /// untyped `Null` literal, or an unresolved `SemanticLeaf::Field`
    /// before compile-time substitution per `19 §3`.
    fn inferred_type(&self) -> Option<&DataType>;
}
```

`ExprLeaf` is intentionally minimal — leaf-set-specific behaviour (semantic-ref resolution per `[19 §3](../foundations/19_expression_flow.md)`, `Parameter` binding at plan time per `[34](34_semstrait_planner.md)`) lives at the site that operates on the leaf, not as a trait method. This keeps the trait surface stable across leaf-set evolution; the only requirement is "you can ask the leaf for its canonical type".

### 3.5 What `31` does NOT own (trait family)

The `Expr<L>` variant catalog (owned by `[14 §3.3](../foundations/14_expressions.md)`, implemented by `[35 §3.3](35_semstrait_ir.md)`), the leaf-set rosters (`[14 §3.4](../foundations/14_expressions.md)` / `[§3.5](../foundations/14_expressions.md)`, implemented by `[35 §4](35_semstrait_ir.md)`), and the `PlanNode` variant catalog (which also implements `Tree` — `[35 §9](35_semstrait_ir.md)`).

## 4. Public Types — `DataType` Family

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

Construction of `Decimal { precision, scale }` with out-of-range values is rejected at `semstrait-model` parse time (`ParseErrorKind::InvalidDecimalParameters`); `semstrait-core` performs no validation at the constructor. `DataType` is referenced by every layer above core — expression leaves and `Cast { target }` in `semstrait-ir`, function signatures (`ParamType::Exact`), plan-tree primitives (`ResolvedColumn`), and adapter renderers.

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

`TypeClass` is exposed but **not** wired into the v1 `ParamType` activation — `[14a §3.3](../foundations/14a_function_catalog.md)` Q6 ratified overload-set polymorphism, not type-class generics, for v1. `TypeClass` exists as vocabulary for future registry evolution (`[TD-REGISTRY-TYPECLASS]`, `[14a §10.1](../foundations/14a_function_catalog.md)`) and for documentation / advisory diagnostics. The reserved `ParamType::TypeClass(TypeClass)` variant lives in `semstrait-ir::functions` per `[35 §7.2](35_semstrait_ir.md)`.

### 4.4 `Schema` and `SchemaColumn` — physical-source schema

```rust
/// The compile-time snapshot of physical columns exposed by a source.
/// Per `15 §3.2`. Referenced by:
/// - `15 §3.1 PhysicalSource::{File, Stream, Table, Snapshot}` for the
///   resolved schema attached to every `PhysicalSource` variant.
/// - `35 §10.1 NodeMeta.output_schema` for the per-`PlanNode` output
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
// ... similarly for Grain, TypeClass, Schema, SchemaColumn, and every type in §5–§7.
```

Feature-gated per §11. Off by default; `semstrait-ir` / `semstrait-model` / `semstrait-manifest` enable it transitively (they require YAML / JSON round-tripping of values carried through these primitives).

## 5. Public Types — Structural-Variant Support Enums

The structural variants of `Expr<L>` (declared in `semstrait-ir` per `[35 §3.3](35_semstrait_ir.md)`) reference a small set of support enums that live here in `semstrait-core` so every consumer of any leaf set shares one vocabulary. The roster and per-enum variant list are ratified by `[14 §3.3](../foundations/14_expressions.md)`'s structural-variant catalog; `31` is the crate that holds the implementation.

### 5.1 `BinaryOpKind` and `UnaryOpKind`

```rust
/// Operator discriminator for `Expr<L>::BinaryOp`. Per `14 §3.3`.
#[non_exhaustive]
pub enum BinaryOpKind {
    Add, Subtract, Multiply, Divide, SafeDivide, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

/// Operator discriminator for `Expr<L>::UnaryOp`. Per `14 §3.3`.
#[non_exhaustive]
pub enum UnaryOpKind {
    Negate,
    Not,
}
```

`BinaryOpKind` covers canonical arithmetic / comparison / logical operators (14 variants); `UnaryOpKind` covers `Negate` / `Not`. Engine-specific operators (bitwise, string concatenation `||`, …) do not add variants — they land as `FunctionCall` entries via `FunctionRegistry` extensions per `[14a §7](../foundations/14a_function_catalog.md)`.

### 5.2 `AggregationOp`

```rust
/// Aggregation function tag carried on `Expr<L>::Aggregate`. Per `14 §3.3`.
/// The enum is CLOSED-for-v1 by convention but `#[non_exhaustive]` per I10
/// so future canonical additions (`StddevPop`, `MedianApprox`, …) land
/// non-breakingly.
///
/// `CountDistinct` is NOT a variant — it is expressed as
/// `Aggregate { op: Count, distinct: true, ... }` per `14 §3.3`.
#[non_exhaustive]
pub enum AggregationOp {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}
```

Renamed from the pre-cascade `Aggregation` to disambiguate from the `Aggregate` structural variant per `[14 §3.3](../foundations/14_expressions.md)`. v1 roster carries five variants.

### 5.3 `LikeKind` and `CastFailure`

```rust
/// `Like` operator variant — case-sensitivity and negation profile.
/// Per `14 §3.3`.
#[non_exhaustive]
pub enum LikeKind {
    Like,
    NotLike,
    ILike,
    NotILike,
}

/// `Cast` failure-mode discriminator. Per `14 §3.3`.
/// Adapters MAY emit different SQL forms per variant (CAST vs TRY_CAST).
#[non_exhaustive]
pub enum CastFailure {
    /// Raise an engine-level error on cast failure.
    Error,
    /// Return `NULL` on cast failure (TRY_CAST semantics).
    Null,
}
```

`LikeKind` consolidates the pre-cascade `Like` / `ILike` / negated variants into one carrier on a single `Expr<L>::Like { kind: LikeKind, … }` structural variant. `CastFailure` makes cast semantics explicit at the IR level rather than leaving the choice to per-engine adapter rendering.

### 5.4 `WindowFn` and `WindowFrame`

```rust
/// Window function identity carried on `Expr<L>::Window`. Per `14 §3.3`.
/// Author-rejected; `Window` nodes are compile-emitted only via sugar-
/// accessor elimination (`14 §4.2`). v1 carries the function set needed
/// by the per-kind accessor lowerings.
#[non_exhaustive]
pub enum WindowFn {
    Lag,
    Lead,
    FirstValue,
    LastValue,
    RowNumber,
    Rank,
    DenseRank,
}

/// Window frame specification carried on `Expr<L>::Window`. Per `14 §3.3`.
/// Frame computation kind + boundaries.
#[non_exhaustive]
pub struct WindowFrame {
    pub kind: WindowFrameKind,
    pub start: WindowBound,
    pub end:   WindowBound,
}

#[non_exhaustive]
pub enum WindowFrameKind { Rows, Range, Groups }

#[non_exhaustive]
pub enum WindowBound {
    UnboundedPreceding,
    Preceding(u64),
    CurrentRow,
    Following(u64),
    UnboundedFollowing,
}
```

Both are reached only through compile-emitted `Expr<L>::Window` nodes; author-facing parsers do not accept window syntax (deferred per `[14 §11](../foundations/14_expressions.md)`'s out-of-scope list and the per-site shape gates in `[14 §7](../foundations/14_expressions.md)`).

### 5.5 `Literal` — typed literal value

```rust
/// Typed literal value carried by `PhysicalLeaf::Literal` and
/// `SemanticLeaf::Literal` (`semstrait-ir`, per `35 §4`). Per `14 §3.3`.
///
/// Renamed from the pre-cascade `LiteralValue` to align with the `14`
/// vocabulary.
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
```

`Literal` is the **single** typed-value carrier shared by both leaf sets — `PhysicalLeaf::Literal(Literal)` and `SemanticLeaf::Literal(Literal)` reach the same enum. Variant list aligns 1:1 with `DataType` (§4.1) plus `Null`.

### 5.6 Shared identifier carriers — `ColumnRef` and `SemanticsName`

```rust
/// Physical column reference. Carried by `PhysicalLeaf::Column(ColumnRef)`
/// and (conditionally per `14 §8`) `SemanticLeaf::Column(ColumnRef)`.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ColumnRef(pub String);

/// Semantic-entity name. Carried by `SemanticLeaf::Field(SemanticsName)`
/// and the per-kind typed semantic leaves (`Dimension`, `Measure`,
/// `Metric`, `Key`) per `14 §3.5`. Resolved at compile per `19 §3`.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SemanticsName(pub String);
```

Both are newtypes over `String` for type-distinction at the leaf-set boundary — code that handles `ColumnRef` cannot accidentally pass a `SemanticsName` (and vice versa). The tuple field is `pub` to save consumers from `.0` indirection. Newtype-over-stable exception per `[30 §4.3](../foundations/30_stability_diagnostics.md)`: no `#[non_exhaustive]` on either type.

## 6. Public Types — Constraint Family

Per `[11 §8.3](../foundations/11_constraints.md)`–`§8.4`. Current type name `MeasureConstraints` is a legacy artifact for the Measure + Metric carriers (`[11 §8.4.3](../foundations/11_constraints.md)`, `[TD-CONSTRAINT-RENAME]`); the three sub-blocks below retain their current names to avoid a breaking rename before the SemanticManifest-schema revision pass.

### 6.1 `MeasureConstraints`

```rust
/// Constraint DSL block attached to Measure and Metric carriers.
/// Per `11 §8.4.1` / `§8.4.2`. Shape-field (`11 §8.8`).
#[non_exhaustive]
pub struct MeasureConstraints {
    pub dimensions: Option<DimensionConstraints>,
    pub aggregations: Option<AggregationConstraints>,
}

impl MeasureConstraints {
    pub fn none() -> Self;
    pub fn is_empty(&self) -> bool;
}
```

### 6.2 `DimensionConstraints`

```rust
/// Three-way set-membership policy over Dimension names. Per `11 §8.3` /
/// `§8.4.1`. All three fields optional; AND-combined.
#[non_exhaustive]
pub struct DimensionConstraints {
    pub one_of: Vec<String>,
    pub none_of: Vec<String>,
    pub all: Vec<String>,
}

impl DimensionConstraints {
    pub fn none() -> Self;
    pub fn is_empty(&self) -> bool;
}
```

### 6.3 `AggregationConstraints`

```rust
/// Two-way whitelist/blacklist policy over UPPERCASE aggregation-name
/// tokens (matching the `AggregationOp` enum names plus `COUNT_DISTINCT`
/// for the `distinct: true` encoding). Per `11 §8.4.1`.
#[non_exhaustive]
pub struct AggregationConstraints {
    pub allowed: Vec<String>,
    pub prohibited: Vec<String>,
}

impl AggregationConstraints {
    pub fn none() -> Self;
    pub fn is_empty(&self) -> bool;
}
```

**Why `Vec<String>` rather than `Vec<AggregationOp>` for `allowed` / `prohibited`.** Per `[11 §8.4.1](../foundations/11_constraints.md)`'s DSL shape: the field accepts UPPERCASE tokens including `COUNT_DISTINCT` (which is not an `AggregationOp` enum variant but an `Expr<L>::Aggregate { op: Count, distinct: true, ... }` encoding). Matching is token-based against a planner-owned normalization; `semstrait-core` exposes the shape, not the matching logic (which lives in `semstrait-planner` per `[11 §8.6](../foundations/11_constraints.md)`).

## 7. Public Types — Diagnostic Primitives

`semstrait-core` provides the **diagnostic envelope** every consumer crate composes around its own per-stage typed-kind enum, plus the `Diagnose` trait those kinds implement. Authoritative sub-shape per `[30 §5](../foundations/30_stability_diagnostics.md)`.

### 7.1 `Severity`

```rust
/// Per `30 §5.2`. Two variants only — `Info` retired into the `tracing`
/// channel (`30 §6`).
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}
```

Severity carries message intent only; control flow (accumulating vs fail-fast) lives in the function signature, not the diagnostic. Per `[30 §5.2](../foundations/30_stability_diagnostics.md)`.

### 7.2 `Location`, `Span`, `SourceId`

```rust
/// Source-level location. Per `30 §5.3`.
#[non_exhaustive]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}

/// Half-open byte-offset span into the source buffer.
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Opaque identifier for the originating source document. Constructors
/// live on the producing crate (`semstrait-model` for YAML files,
/// inline-string for tests, etc.); `semstrait-core` exposes only the
/// shape consumers need.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SourceId(/* crate-private */);

impl SourceId {
    pub const fn unknown() -> Self;
    pub fn as_str(&self) -> &str;
}
```

`Span` retains its `[30 §5.3](../foundations/30_stability_diagnostics.md)` byte-range shape. `SourceId` is opaque — it is NOT `#[non_exhaustive]` because its variant set is private; the crate's public surface exposes only `SourceId::unknown()` and the `Eq` / `Hash` / `Display` traits consumers need.

### 7.3 `Diagnostic<K>` — generic envelope

```rust
/// Generic envelope wrapping a per-stage typed kind. Per `30 §5.1`.
/// Each consumer crate plugs in its own `K: Diagnose`.
#[non_exhaustive]
pub struct Diagnostic<K: Diagnose> {
    pub kind: K,
    pub severity: Severity,
    pub location: Option<Location>,
    pub notes: Vec<String>,
}

/// Type alias for an accumulating-stage diagnostic vector.
pub type Diagnostics<K> = Vec<Diagnostic<K>>;
```

Construction is via per-crate helpers; `semstrait-core` does not expose a `Diagnostic::new` or builder. Each consumer crate's helper sets `severity` from `K::severity_default()` (overridable) and attaches `location` / `notes` as appropriate.

### 7.4 `Diagnose` trait

```rust
/// The trait every per-stage kind implements. Per `30 §5.4`.
pub trait Diagnose {
    /// Human-readable rendering. Powers the `Display` impl on
    /// `Diagnostic<K>`. Must not include line breaks (callers add
    /// their own framing).
    fn message(&self) -> String;

    /// Default severity for this variant. Construction sites may
    /// override; most callers accept the default.
    fn severity_default(&self) -> Severity;

    /// Foreign-error chain for `std::error::Error` interop. Default
    /// `None`. Variants wrapping foreign errors override to return
    /// `Some(&inner)`.
    fn cause(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
```

`Diagnose` is **open** — third-party kind enums (e.g. a downstream plugin's typed error) implement it and slot into the `Diagnostic<K>` envelope.

### 7.5 Blanket impls on `Diagnostic<K>`

```rust
impl<K: Diagnose> std::fmt::Display for Diagnostic<K> { /* delegates to K::message() */ }
impl<K: Diagnose + std::fmt::Debug> std::error::Error for Diagnostic<K> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.cause()
    }
}
```

Both blanket impls live in `semstrait-core::diagnostic` because of the orphan rule — no other crate could provide them. `Diagnose::cause()` is the source of truth for the `std::error::Error` chain.

### 7.6 `Diagnostics<K>` ergonomics

`Diagnostics<K>` is a transparent `Vec<Diagnostic<K>>` alias, so all `Vec` methods apply directly. Per `[30 §5.6](../foundations/30_stability_diagnostics.md)`, fused helpers in `semstrait-api` use a sum-typed kind (`SemStraitErrorKind`) and lift per-stage results via `From<ParseErrorKind>`, `From<ValidateErrorKind>`, etc. — `From` impls live on the fused-kind enum, not on `Diagnostic<K>`.

## 8. Public Types — Core-Emitted Kind Enums

Per `[30 §5](../foundations/30_stability_diagnostics.md)`, each crate owns its own per-stage typed-kind enum implementing `Diagnose`. After the `14` cascade, `semstrait-core` provides two **narrowed** kind enums, scoped strictly to failures core code itself raises:

- `ValidateErrorKind` — emitted by `Tree::with_new_children` reconstructions and the `Rewriter<N>::f_*` paths that compose them. Variants formerly scoped to `SemanticExpr` / `PhysicalExpr` wrapper construction (which retired at the `14` second refinement) have moved to `semstrait-ir` per `[35 §15](35_semstrait_ir.md)` (`IrErrorKind`) or to per-stage kinds in `semstrait-model` / `semstrait-manifest`.
- `CompileErrorKind` — emitted by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec` (`semstrait-ir::functions` per `[35 §7](35_semstrait_ir.md)`). Variants formerly scoped to manifest-level name resolution have moved to `semstrait-manifest::CompileErrorKind` per `[19 §8](../foundations/19_expression_flow.md)`.

### 8.1 `ValidateErrorKind`

```rust
/// Tree-shape reconstruction failures raised by `Tree::with_new_children`
/// and `Rewriter<N>::f_*` paths. Per `14 §3.1`. Implements `Diagnose`
/// (§7.4).
///
/// Per-leaf-set construction invariants formerly carried here
/// (`ColumnInSemanticExpr`, `EntityRefInPhysicalExpr`, …) retired at the
/// `14` second refinement: the leaf-set boundary now makes those
/// invariants type-level rather than runtime-checked (per `14 §3.7`).
/// Tree-walk validation that previously raised those variants now either
/// (a) is type-enforced and removed entirely, or (b) lives in
/// `semstrait-ir::IrErrorKind` (`35 §15`) for runtime structural checks.
#[non_exhaustive]
pub enum ValidateErrorKind {
    /// `Tree::with_new_children` was called with a child count that
    /// does not match the node's variant arity (e.g. `BinaryOp`
    /// reconstructed with three children).
    ChildArityMismatch { expected: usize, got: usize },
}

impl Diagnose for ValidateErrorKind {
    fn message(&self) -> String { /* per-variant rendering */ }
    fn severity_default(&self) -> Severity { Severity::Error }
}
```

`ValidateErrorKind` is intentionally narrow — it carries only the structural-shape failures that any `Tree` implementor can raise generically. Implementation-specific concerns (e.g. "`SemanticLeaf::Column` under manual mapping" per `[14 §8](../foundations/14_expressions.md)`, kind mismatches between authored and registered semantics) live in `manifest::CompileErrorKind` per `[19 §8](../foundations/19_expression_flow.md)`. Per-node well-formedness violations in the plan tree (predicate type not Boolean, schema-arity mismatch in `Union`, …) live in `semstrait-ir::IrErrorKind` per `[35 §15](35_semstrait_ir.md)`.

### 8.2 `CompileErrorKind`

```rust
/// Failures from `ReturnTypeRule::Custom(fn(&[DataType]) -> ...)`
/// callbacks wired into `FunctionSpec` (`semstrait-ir::functions`,
/// per `35 §7`). Per `14a §3.4`. Implements `Diagnose` (§7.4).
#[non_exhaustive]
pub enum CompileErrorKind {
    /// The custom callback could not infer a return type for the
    /// argument types it was given. `reason` is callback-supplied.
    TypeInferenceFailure { reason: String },

    /// A literal value cannot fit into its target type without overflow.
    /// Raised by `Literal` construction paths that perform range checks
    /// (e.g. `Decimal { precision, scale }` against a typed target).
    LiteralOverflow { value: String, target: DataType },

    /// A literal value would lose precision when coerced into its
    /// target type (e.g., narrowing decimal cast).
    LiteralPrecisionLoss { value: String, target: DataType },
}

impl Diagnose for CompileErrorKind {
    fn message(&self) -> String { /* per-variant rendering */ }
    fn severity_default(&self) -> Severity { Severity::Error }
}
```

Variants formerly carried by the pre-cascade `CompileErrorKind` covering name resolution, function resolution, shape unification, manual-mapping rejection, kind mismatches, and physical-type representability move to `semstrait-manifest::CompileErrorKind` per `[19 §8](../foundations/19_expression_flow.md)` and `[33](33_semstrait_manifest.md)`. The core enum retains only the three variants raisable by core-emitted callbacks (`ReturnTypeRule::Custom`) or by `Literal` construction paths inside core.

### 8.3 No code() methods, no IntoDiagnostic

Both enums implement the `Diagnose` trait only. There is no `code() -> &'static str`, no kebab-case identifier, no legacy numeric-code constant. Stable identification is variant identity (renaming a variant is MAJOR per `[30 §2](../foundations/30_stability_diagnostics.md)`; adding one inside `#[non_exhaustive]` is MINOR).

The `IntoDiagnostic` trait of earlier drafts is **retired**. Constructing a `Diagnostic<K>` from a kind is direct: callers (or per-crate helper functions) build `Diagnostic { kind, severity: kind.severity_default(), location: …, notes: vec![] }` at the point of failure.

### 8.4 Display / Error blanket impls

```rust
impl std::fmt::Display for ValidateErrorKind { /* delegates to Diagnose::message */ }
impl std::error::Error for ValidateErrorKind {}
impl std::fmt::Display for CompileErrorKind { /* delegates to Diagnose::message */ }
impl std::error::Error for CompileErrorKind {}
```

These complement the blanket impls on `Diagnostic<K>` (§7.5): callers may use the bare kind directly in `?` chains (without the `Diagnostic<K>` envelope) when they have no location to attach.

## 9. Public Free Functions

After the `14` cascade, `semstrait-core` exposes **no expression-related or function-registry-related free functions**:

- `function_registry()` moves to `semstrait-ir::functions::function_registry()` per `[35 §7.2](35_semstrait_ir.md)` (every consumer of `Expr<L>::FunctionCall { name: CanonicalFn, ... }` needs the registry, and `Expr<L>` lives in `semstrait-ir`).
- `is_reserved_tag(&str) -> bool` moves to `semstrait-model::is_reserved_tag` per `[14 §9.3](../foundations/14_expressions.md)` (the helper is consumed by the Declarative-block parser, which lives in `semstrait-model`; co-locating it with `ExprBlock` keeps the reserved-tag catalog single-sourced).

A `coarseness(g: Grain) -> u8` free-function form was considered and rejected — it is already exposed as `Grain::coarseness(self)` per `[13 §3.2](../foundations/13_types_and_grain.md)`. A `type_class_of(dt: DataType) -> TypeClass` helper was considered and rejected — it would encode classification policy that `[13 §4](../foundations/13_types_and_grain.md)` leaves to authors. Any new free function requires ratification against this section in an amendment.

## 10. Traits Exported

Per `[30](../foundations/30_stability_diagnostics.md)`'s trait-surface rules: every public trait that can be externally implemented SHOULD be evaluated against the sealed-trait pattern. Sealed traits prevent external impls; non-sealed traits are part of the external extension surface.

| Trait | Surface | Externally implementable? | Sealed? | Source |
|---|---|---|---|---|
| `Tree` | `fn children(&self) -> Vec<&Self>`, `fn with_new_children(self, …) -> Result<Self, ValidateErrorKind>` | yes — any tree-shaped value MAY impl `Tree` | no | §3.2 |
| `Visitor<N>` | `type Output`, `fn f_down`, `fn f_up` (returning `ControlFlow<Output>`) | yes — third-party analysis passes | no | §3.3 |
| `Rewriter<N>` | `fn f_down`, `fn f_up` (returning `Result<N, ValidateErrorKind>`) | yes — third-party rewrites | no | §3.3 |
| `ExprLeaf` | `fn inferred_type(&self) -> Option<&DataType>` | yes — `semstrait-ir`'s `PhysicalLeaf` / `SemanticLeaf` impl, plus any future leaf set | no | §3.4 |
| `Diagnose` | `fn message`, `fn severity_default`, `fn cause` | yes — third-party kind enums slot into `Diagnostic<K>` | no | §7.4 |

**Sealed-trait justification, positive cases.** None. All external-facing traits are non-sealed because:

- `Tree` — a future external crate that introduces a new tree shape (e.g. a Substrait-side IR adapter) MUST be able to impl it without a workspace-private escape hatch.
- `Visitor<N>` / `Rewriter<N>` — third-party analysis / rewrite passes MUST be able to impl them without sealing.
- `ExprLeaf` — leaf-set evolution (a future SQL-fragment leaf set, a graph-query leaf set, …) requires open impls.
- `Diagnose` — a third-party error type from e.g. a user-defined plugin MAY define its own kind and slot into `Diagnostic<K>`.

The pre-cascade `ExprVisitor` (single trait method `visit(&mut self, &Expr) -> Self::Output`) and `RegistryExtension` (const-driven adapter-contribution hook) are no longer in core: `ExprVisitor` retired in favor of the generic `Tree` + `Visitor<N>` / `Rewriter<N>` family, and `RegistryExtension` moved to `semstrait-ir::functions::RegistryExtension` per `[35 §7.2](35_semstrait_ir.md)`.

## 11. Feature Flags

v1 has a small, axis-orthogonal flag set. The `io`-family flags were added in the `[31b](31b_semstrait_core_io.md)` ratification. The `serde` flag now gates a smaller surface area than pre-cascade — `Expr<L>` / `PhysicalLeaf` / `SemanticLeaf` / `Parameter` / `CanonicalFn` / `FunctionRegistry` / `FunctionSpec` / `FnSignature` / `ParamType` / `ReturnTypeRule` / `FunctionCategory` / `ExprSource` / `ExprBlock` are no longer in this crate, so their serde derivations live in `semstrait-ir` (`[35 §14](35_semstrait_ir.md)`) and `semstrait-model` respectively.

| Feature | Default | Gates | Reason |
|---|---|---|---|
| `serde` | OFF | `Serialize` / `Deserialize` on every public type in this crate: `DataType`, `Grain`, `TypeClass`, `Schema`, `SchemaColumn`, the `expr_kinds` enum/struct roster, `Literal`, `ColumnRef`, `SemanticsName`, the constraint family, `Diagnostic<K>` (where `K: Serialize`), `Severity`, `Location`, `Span`, `SourceId`, `ValidateErrorKind`, `CompileErrorKind` | keeps the crate's dependency footprint minimal for consumers that only need types; `semstrait-ir` / `semstrait-model` / `semstrait-manifest` enable it transitively |
| `schemars` | OFF | JSON schema derivations on the same types | consumers needing JSON-Schema emission pay a second compile cost; off by default per `[30](../foundations/30_stability_diagnostics.md)` |
| `io` | **ON** | The `io` module — `Source` / `Sink` / `FromIoBytes` / `IntoIoBytes` / `Location` / `IoError` + `backends::memory` + `backends::local`; pulls `tokio`, `bytes`, `object_store` (Local + InMemory features), `dashmap` | ergonomic common case for every downstream crate that wants transport; disable with `default-features = false` for pure-type consumers (see `[31b §9.1](31b_semstrait_core_io.md)`) |
| `io-aws` | OFF | `Location::S3` variant + `backends::s3::{S3Source, S3SourceBuilder}`; enables `object_store/aws` which transitively pulls the AWS config / credential crates | cloud SDK footprint stays opt-in; enabled explicitly by CLI / `semstrait-api` / `semstrait-facade` |

No other I/O features in v1. Future `io-http`, `io-gcs`, `io-azure` land additively behind the same gating pattern.

**Delta with current code.** Moving the pre-cascade `Expr` / canonical-IR family + `FunctionRegistry` to `semstrait-ir` is tracked under `[TD-CORE-EXPR-MIGRATION]` in `[implementation/40_refactor_plan.md](../implementation/40_refactor_plan.md)`. Moving `serde` behind `#[cfg(feature = "serde")]` is tracked under `[TD-CORE-SERDE-GATING]`. The `io` module is a net-new addition per `[31b](31b_semstrait_core_io.md)` / `TD-008`. `arrow-feature` gating is rejected — engine-specific data-plane deps violate I11.

## 12. Dependency Posture

### 12.1 External dependencies

A canonical `Cargo.toml` target after the `[31b](31b_semstrait_core_io.md)` ratification and the `[14](../foundations/14_expressions.md)` cascade:

```toml
[dependencies]
thiserror = "^"                         # error enum derivations

[dependencies.serde]
version = "^"
optional = true
features = ["derive"]

[dependencies.schemars]
version = "^"
optional = true

[dependencies.tokio]                    # I/O runtime; gated by "io"
version = "^"
optional = true
features = ["rt", "fs", "io-util", "macros"]

[dependencies.bytes]                    # zero-copy byte buffers for Source::read_raw / Sink::write_raw
version = "^"
optional = true

[dependencies.dashmap]                  # (region, endpoint) → AmazonS3 client cache for Location dispatch
version = "^"
optional = true

[dependencies.object_store]             # back-end wrapper (Apache Arrow); gated by "io"
version = "^"
optional = true
default-features = false                # enable only what's needed per feature

[features]
default  = ["io"]
serde    = ["dep:serde"]
schemars = ["dep:schemars", "serde"]
io       = ["dep:tokio", "dep:bytes", "dep:dashmap", "dep:object_store"]
io-aws   = ["io", "object_store/aws"]
```

**Runtime dependency posture.** Under default features, `semstrait-core` pulls `tokio`, `bytes`, `dashmap`, and `object_store` (with its `Local` + `InMemory` back-ends compiled; no cloud SDKs unless `io-aws` is enabled). Under `--no-default-features`, the crate retains its historical zero-runtime-dep shape — only `thiserror` remains. Pure-type consumers take the `--no-default-features` path.

**`object_store` as internal detail.** Consumers never see `object_store::ObjectStore`, `object_store::Path`, or any of its error types on a public signature. The one escape hatch is `S3SourceBuilder::with_object_store_builder(object_store::aws::AmazonS3Builder)` — callers opting into advanced S3 configuration implicitly opt into `object_store` evolution. See `[31b §1.4](31b_semstrait_core_io.md)` for the adoption rationale and SR-IO-8 for the encapsulation rule.

**No other runtime deps.** No `async-trait` (stable async-fn-in-trait suffices), no `futures` beyond what `tokio` re-exports, no `reqwest`, no `hyper`, no `sqlx`, no direct `aws-sdk-s3` / `aws-config` (they come in transitively via `object_store/aws`).

**No engine-identity dependencies.** No `datafusion`, no `arrow`, no `spark-*`, no `duckdb`, no `substrait`. These live in `semstrait-adapter` and its per-engine modules. The pre-cascade `nonzero_ext` dep is removed — `Span`'s `usize` field stays plain.

### 12.2 Internal (workspace) dependencies

**Zero.** `semstrait-core` is the root of the workspace DAG per I7. Every other crate depends on `semstrait-core` directly or transitively; `semstrait-core` depends on no workspace crate. Attempting to add a `semstrait-*` dependency to `Cargo.toml` is a compile error in CI.

## 13. Invariants Upheld by the Crate

Concrete crate-level guarantees mapping to `[00 §9](../00_overview.md)` invariants. After the `14` cascade, several invariants narrow their crate-level scope because the types those invariants used to anchor have moved upstream.

| Invariant | `semstrait-core` guarantee |
|---|---|
| **I1** — no raw SQL in canonical layer | `semstrait-core` exposes no string-as-SQL types. `Literal` carries typed values; `ColumnRef` / `SemanticsName` carry identifier-class names, not SQL fragments. The pre-cascade carriers (`Expr` / `ExprSource::Inline(String)`) moved to `semstrait-ir` / `semstrait-model`; every surface here is typed-value / typed-identifier. |
| **I2** — physical types belong to adapters | `DataType` variants are engine-neutral per `[13 §2](../foundations/13_types_and_grain.md)`. No `arrow::*` / `spark::*` / `datafusion::*` types are visible on any public surface. `Schema` and `SchemaColumn` carry `DataType`, not engine-native types. |
| **I5** — name resolution is compile-time | `semstrait-core` declares no semantic-reference types (those live in `semstrait-ir::SemanticLeaf` per `[35 §4.2](35_semstrait_ir.md)`). The identifier carriers `ColumnRef` and `SemanticsName` are pure newtypes — no resolution methods. Resolution is performed by `semstrait-manifest::compile` per `[19 §3](../foundations/19_expression_flow.md)`. After the cascade, the core-level surface is *the absence* of any resolution shape. |
| **I6** — plan hot path is synchronous | No `pub async fn` exists on the plan hot path: `Tree` / `Visitor<N>` / `Rewriter<N>` / `ExprLeaf` / `Diagnose` impls — all synchronous. The only `async fn`s at `semstrait-core` live in `io` (`Source::read`, `Sink::write`, `Location`'s impls) — I/O is explicitly outside the plan hot path. A CI lint enforces: no `async fn` outside `semstrait_core::io::*`. |
| **I7** — strict DAG | `Cargo.toml` contains zero `semstrait-*` entries in `[dependencies]`. A CI check greps the manifest and fails on any workspace-internal entry. |
| **I10** — extensibility | Every `pub enum` and `pub struct` (with the `[30](../foundations/30_stability_diagnostics.md)`-documented newtype-over-stable exception) carries `#[non_exhaustive]`. The exception set: `Span`, `SourceId` (opaque per `[30 §5.3](../foundations/30_stability_diagnostics.md)`), `ColumnRef`, `SemanticsName`, `Schema`, `SchemaColumn` (field-stable shared-vocabulary types). An `integration-test` over `cargo public-api` enforces the `#[non_exhaustive]` rule. |
| **I11** — no downward I/O surprises | Transport primitives (`io::Source`, `io::Sink`, `io::Location`, `io::backends::{memory, local, s3}`) live on `semstrait-core` under the `io` feature flag (ratified in `[31b](31b_semstrait_core_io.md)`). Domain-specific load / dump (`load_model`, `load_manifest`) do not — they live in the crate that owns the typed artifact. `reqwest`, `hyper`, raw `std::net` sockets remain rejected; cloud SDKs (`aws-sdk-s3`) sit behind opt-in `io-aws`. The dependency audit (§12.1) is enforced in CI via `cargo deny`. |
| **I12** — first-class diagnostics | `Diagnostic<K>` and `Diagnose` are the workspace's diagnostic primitives. `Diagnostic<K>` carries `kind: K, severity, location, notes`; the kind decides per-variant rendering and severity defaults via `Diagnose`. No central error-code allocation; stable identification is variant identity. The parallel observability channel (`tracing`) is described in `[30 §6](../foundations/30_stability_diagnostics.md)`; library code never writes to stdout / stderr. `IoError` per `[31b §6](31b_semstrait_core_io.md)` is its own kind enum implementing `Diagnose`. |

## 14. Public API Surface Sketch

One rustdoc-style line per exported item, grouped by module. Doubles as the "test-the-contract" target — an integration test enumerates this list against `cargo public-api` output. After the `14` cascade, the surface is substantially smaller than pre-cascade; the `expr.*` and `functions.*` modules retire entirely, the `tree` module replaces the older `expr::visit`, and `expr_kinds` replaces the older `expr::types`.

### 14.1 `tree`

```
pub trait Tree                                            // fn children, fn with_new_children
pub trait Visitor<N>                                      // type Output; fn f_down, fn f_up (ControlFlow)
pub trait Rewriter<N>                                     // fn f_down, fn f_up (Result<N, ValidateErrorKind>)
pub trait ExprLeaf                                        // fn inferred_type(&self) -> Option<&DataType>
// Default-derived helpers on Tree: pub fn apply, pub fn transform.
```

### 14.2 `types`

```
pub enum   DataType                                       // 14 variants per 13 §2.1
pub enum   Grain                                          // 7 variants per 13 §3.1
pub enum   TypeClass                                      // 7 variants per 13 §4
pub struct Schema                                         // { columns: Vec<SchemaColumn> }
pub struct SchemaColumn                                   // { name, data_type, nullable }
impl Grain { pub fn coarseness(self) -> u8 }
```

### 14.3 `expr_kinds`

```
pub enum   BinaryOpKind                                   // 14 variants per 14 §3.3
pub enum   UnaryOpKind                                    // Negate | Not
pub enum   AggregationOp                                  // Sum | Avg | Count | Min | Max
pub enum   LikeKind                                       // Like | NotLike | ILike | NotILike
pub enum   CastFailure                                    // Error | Null
pub enum   WindowFn                                       // Lag | Lead | FirstValue | LastValue | RowNumber | Rank | DenseRank
pub struct WindowFrame                                    // { kind: WindowFrameKind, start: WindowBound, end: WindowBound }
pub enum   WindowFrameKind                                // Rows | Range | Groups
pub enum   WindowBound                                    // UnboundedPreceding | Preceding(u64) | CurrentRow | Following(u64) | UnboundedFollowing
pub enum   Literal                                        // one variant per DataType + Null
pub struct ColumnRef                                      // newtype over String — physical column reference
pub struct SemanticsName                                  // newtype over String — semantic-entity name
```

### 14.4 `constraints`

```
pub struct MeasureConstraints                             // { dimensions, aggregations }
pub struct DimensionConstraints                           // { one_of, none_of, all }
pub struct AggregationConstraints                         // { allowed, prohibited }
// Each of the three exposes fn none() and fn is_empty(&self) -> bool.
```

### 14.5 `diagnostic`

```
pub struct Diagnostic<K: Diagnose>                        // { kind, severity, location, notes }
pub type   Diagnostics<K> = Vec<Diagnostic<K>>
pub enum   Severity                                       // Error | Warning
pub struct Location                                       // { source: SourceId, span: Span }
pub struct Span                                           // { start: usize, end: usize }
pub struct SourceId                                       // opaque; SourceId::unknown() + as_str()
pub trait  Diagnose                                       // fn message + severity_default + cause
// Blanket: Display / std::error::Error on Diagnostic<K> via Diagnose.
```

### 14.6 `error`

```
pub enum ValidateErrorKind                                // ChildArityMismatch (§8.1)
pub enum CompileErrorKind                                 // TypeInferenceFailure | LiteralOverflow | LiteralPrecisionLoss (§8.2)
// Each kind enum: impl Diagnose, impl Display, impl std::error::Error.
```

### 14.7 Crate-root re-exports (stable convenience surface)

```rust
// lib.rs
pub use crate::tree::{Tree, Visitor, Rewriter, ExprLeaf};
pub use crate::types::{DataType, Grain, TypeClass, Schema, SchemaColumn};
pub use crate::expr_kinds::{
    BinaryOpKind, UnaryOpKind, AggregationOp, LikeKind, CastFailure,
    WindowFn, WindowFrame, WindowFrameKind, WindowBound,
    Literal, ColumnRef, SemanticsName,
};
pub use crate::constraints::{
    MeasureConstraints, DimensionConstraints, AggregationConstraints,
};
pub use crate::diagnostic::{
    Diagnostic, Diagnostics, Severity, Location, Span, SourceId, Diagnose,
};
pub use crate::error::{ValidateErrorKind, CompileErrorKind};
```

### 14.8 `io` (feature `io`, default ON)

Full spec: `[31b](31b_semstrait_core_io.md)`. This section is a re-export sketch only.

```
pub use self::io::{
    Source, Sink,                       // §31b §3–§4 — byte-blob transport traits
    FromIoBytes, IntoIoBytes,           // §31b §5 — byte↔typed conversion traits
    Location,                           // §31b §6 — polymorphic back-end dispatch
    IoError,                            // §31b §7 — #[non_exhaustive] error enum
};

pub mod io::backends::memory  { pub struct InMemory; }
pub mod io::backends::local   { pub struct LocalFile; }
#[cfg(feature = "io-aws")]
pub mod io::backends::s3 {
    pub struct S3Source;
    pub struct S3SourceBuilder;         // §31b §8.3 — custom S3 configuration
}
```

Internally every back-end thin-wraps `object_store` (Apache Arrow project); `object_store` types never appear on a public signature except the one documented escape hatch (`S3SourceBuilder::with_object_store_builder`). See `[31b §1.4](31b_semstrait_core_io.md)` for the adoption rationale and SR-IO-8 for the encapsulation rule.
