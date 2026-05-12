---
prereqs: [13, 14, 14a, 30]
authoritative-for:
  - the `semstrait-core` public-API surface (types, traits, free functions)
  - module layout within `semstrait-core` (top-level `pub mod`s and their split rationale)
  - the `Expr`-family newtype wrappers exposed (`SemanticExpr`, `PhysicalExpr`) and their construction boundaries
  - the `DataType`-family visibility surface (what is `pub`, what is crate-private, feature-gated serde)
  - the `FunctionRegistry` surface: `function_registry()` accessor, `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`, `CanonicalFn` newtype, `RegistryExtension` trait
  - the constraint-DSL toolkit types (`MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints`) exposed at `semstrait-core`
  - the cross-cutting diagnostic primitives (`Diagnostic<K>`, `Diagnostics<K>`, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose` trait) and the narrow core-emitted kind enums (`ValidateErrorKind`, `CompileErrorKind`) that live in `semstrait-core`
  - reserved-tag helpers (`is_reserved_tag`) and other pure-function utilities
  - feature-flag surface (`serde`, `schemars`, `io`, `io-aws`) and dependency posture — under default features core pulls `tokio`; under `--no-default-features` it retains the original zero-runtime-dep shape
  - mapping of design invariants I1, I2, I5, I6, I7, I10, I11, I12 to concrete crate-level guarantees
refined-by:
  - 31b (`semstrait-core::io` — text-blob transport module, amends §1.3 / §2 / §11 / §12 of this doc)
  - 32 (`semstrait-model` declares `ParseErrorKind` and its own `ValidateErrorKind` embedding `Core(core::ValidateErrorKind)`; adds `io` wrappers over `31b`)
  - 33 (`semstrait-manifest` declares its own `CompileErrorKind` embedding `Core(core::CompileErrorKind)`; adds `RepositoryErrorKind`; adds `io` wrappers over `31b`)
  - 34 (`semstrait-planner` consumes the sealed `FunctionRegistry` and resolved `PhysicalExpr`s at plan time; declares `PlanErrorKind` / `OptimizeErrorKind`)
  - 36 (`semstrait-adapter` contributes `RegistryExtension` impls; declares `AdaptErrorKind`)
  - 38 (`semstrait-api` declares the sum-typed `SemStraitErrorKind` lifting per-stage kinds)
  - 40 (`implementation/40_refactor_plan.md` — current code vs target layout delta is tracked here)
---

# 31. semstrait-core

> **Status:** ratified. `31` nails down the public surface of `semstrait-core` — the canonical-layer vocabulary crate — against `13` (types and grain), `14` (expressions), `14a` (function catalog), and `30` (stability / diagnostics policy). All types exposed here are already ratified upstream; `31` adds no new vocabulary, only crate-level visibility, module placement, and I6 / I11 / I12 guarantees.

## 1. Purpose and Scope

`semstrait-core` is the **shared-types crate** every layer above it consumes. It owns the **canonical expression AST**, the **logical type system**, the **function-catalog shape**, and the cross-cutting **diagnostic / error primitives** that flow across stage boundaries. It contains no I/O, no async, no parsing, no planner logic, no adapter logic — just the vocabulary every consumer agrees on.

### 1.1 What `semstrait-core` OWNS

- The canonical `Expr` AST and its wrapper newtypes `SemanticExpr` / `PhysicalExpr` (per `14 §2` / `§3`).
- The `ExprSource` YAML representation enum (per `14 §4`) — structural shape only; per-site dispatch lives in `semstrait-model`.
- Supporting AST types: `BinaryOpKind`, `Aggregation`, `LiteralValue`, `WhenClause` (per `14 §3.2`).
- The canonical `DataType` enum and the `Grain` enum (per `13 §2` / `§3`). `TypeClass` type-grouping vocabulary (per `13 §4`).
- The `FunctionRegistry` (per `14a §2`), the `FunctionSpec` / `FnSignature` / `ParamType` / `ReturnTypeRule` / `FunctionCategory` types (per `14a §3`), the `CanonicalFn` newtype (per `00 §4.1` / `14a §2`), and the `RegistryExtension` trait (per `14a §7.1`).
- The constraint-DSL toolkit: `MeasureConstraints`, `DimensionConstraints`, `AggregationConstraints` (per `11 §8.3` / `§8.4`).
- The cross-cutting diagnostic primitives ratified in `30 §5`: `Diagnostic<K>` generic envelope, `Diagnostics<K>` alias, `Severity`, `Location`, `Span`, `SourceId`, `Diagnose` trait. Placement here avoids upward deps from every stage into a diagnostic-owning leaf.
- The narrow core-emitted kind enums: `ValidateErrorKind` (raised by `SemanticExpr` / `PhysicalExpr` constructors) and `CompileErrorKind` (raised by `ReturnTypeRule::Custom` callbacks). Each implements `Diagnose`. Stages downstream MAY embed these via D.ii kind-nesting (`30 §7.4`).

### 1.2 What `semstrait-core` does NOT own

- **Model parse + YAML grammar.** The `ExprSource` → `SemanticExpr` / `PhysicalExpr` **dispatch** (per `14 §4.2`), the reserved-tag parser, the `ExprBlock` rule tables, all live in `semstrait-model`. `semstrait-core` exposes only the structural types those parsers produce.
- **SemanticManifest structure.** The `SemanticManifest`, `ResolvedDataKind`, `ResolvedExprTable`, `ResolvedSource`, `ResolvedColumnMapping` all live in `semstrait-manifest` (per `33`). `semstrait-core` exposes only the `PhysicalExpr` type that `ResolvedExprTable` stores.
- **Planner + optimizer.** `SemanticPlan`, `PlanNode`, `PlannerError`, plan-time `Request` / `SessionContext` all live in `semstrait-ir` (plan types) and `semstrait-planner` (stages) per `34` / `35`.
- **Engine identity and dialect.** `EngineArtifact`, `EngineAdapter`, `Dialect`, `DialectId`, `EnginePlan`, `SqlArtifact`, `AdaptError` all live in `semstrait-adapter` (per `36`).
- **Catalog and filesystem.** `CatalogProvider`, `FileSystem`, `Repository`, `CatalogSnapshot` all live in `semstrait-catalog` (per `37`).
- **Name resolution, scope chains, shape unification.** The algorithms live in `semstrait-model` (`validate`) and `semstrait-manifest` (`compile`); their kind enums (`model::ValidateErrorKind`, `manifest::CompileErrorKind`) live in those crates and MAY embed `Core(core::ValidateErrorKind)` / `Core(core::CompileErrorKind)` per D.ii.
- **Domain load / dump wrappers.** `load_model` / `dump_model` / `load_catalogs` / `dump_catalogs` live in `semstrait-model::io` (`32 §10.4`). `load_manifest` / `dump_manifest` live in `semstrait-manifest::io` (`33`). `semstrait-core::io` owns only the text-blob transport (`Source`, `Sink`, `Location`, `IoError`, back-ends); the domain crates own the format.

### 1.3 Design posture — minimum-viable shared crate

`semstrait-core` is deliberately **minimal**. It exists to solve one problem: keep `DataType`, `Expr`, `Diagnostic<K>`, the `Diagnose` trait, and `FunctionRegistry` definable without pulling in the model / manifest / planner / adapter crates. If a type is not needed by two or more downstream crates, it does not belong here. Engine-identity deps (datafusion, arrow, duckdb, substrait) are rejected outright.

The crate is the **leaf** of the semstrait workspace DAG (I7): it depends on nothing in the workspace and every other crate depends on it.

**I/O amendment (ratified in `31b`).** The `io` module provides the shared transport vocabulary that every downstream load / dump wrapper composes. Under default features (`io` ON), `semstrait-core` pulls `tokio`. Under `--no-default-features`, the crate retains its original zero-runtime-dep posture. Cloud SDKs (`aws-sdk-s3`, future `gcs` / `azure`) sit behind additional opt-in flags. The "no async, no I/O in core" blanket from earlier drafts is replaced by: "text-blob transport is a first-class core concern; domain-specific wrappers are not."

## 2. Module Layout

Top-level `pub mod` structure. One module per cohesive concept; no cross-module cycles; no `pub use` re-exports of internal modules outside this table.

```
semstrait-core
├── expr                 // Expr, SemanticExpr, PhysicalExpr, ExprSource
│   ├── types            // BinaryOpKind, Aggregation, LiteralValue, WhenClause
│   └── visit            // ExprVisitor, walk / transform helpers
├── types                // DataType, TypeClass, Grain
├── functions            // FunctionRegistry, FunctionSpec, FnSignature, ParamType,
│                        //   ReturnTypeRule, FunctionCategory, CanonicalFn,
│                        //   RegistryExtension, function_registry()
├── constraints          // MeasureConstraints, DimensionConstraints,
│                        //   AggregationConstraints
├── diagnostic           // Diagnostic<K>, Diagnostics<K>, Severity, Location,
│                        //   Span, SourceId, Diagnose trait
├── error                // ValidateErrorKind, CompileErrorKind
│                        //   (narrow, core-emitted only; stages downstream
│                        //    may embed via D.ii kind-nesting)
└── io                   // Source, Sink, Location, IoError + backends::{memory, local, s3}
                         //   (feature "io", default ON; s3 under "io-aws")
                         //   Full spec: 31b
```

The `io` module is the transport layer that every crate above composes into its own typed load / dump wrappers (`32 §10.4` for model + catalogs, `33` for manifest). Domain-specific functions (`load_model`, `load_manifest`, …) are NOT in core — they live in the crate that owns the corresponding type. Core owns the transport; consumers own the format. Full surface lives in `31b`.

**Split rationale:**

- `expr` vs `types` — `Expr` references `DataType` at leaves (`Literal`, `Cast`), so `types` has no dependency on `expr` and `expr` depends on `types`. Keeping them split lets a downstream that only needs `DataType` (e.g. a raw schema crate) skip `expr` compilation.
- `expr::types` vs `expr` — the small support enums (`BinaryOpKind`, `Aggregation`, `LiteralValue`) change at a different cadence than `Expr` itself and are referenced by consumers who never materialize an `Expr` (e.g. the constraint DSL references `Aggregation` via token-string comparison). Nesting as `expr::types` keeps them close to `Expr` without inflating `types` with variant-specific support.
- `expr::visit` — visitor traits are a separate concern from `Expr` itself (I10 non-exhaustive interaction with trait method counts); isolating them limits the blast radius when a new `Expr` variant lands.
- `functions` — alphabetically separate from `expr` because the registry is accessed directly by parse-site code, plan-site code, and adapters without going through an `Expr` value. The registry is the **single source of truth** for function identity (`14a §2.2` / `§2.3`).
- `constraints` — called out as its own module because `MeasureConstraints` binds to Measure and Metric carriers, is referenced by model and planner alike, and carries its own `serde` derivations. Placement in `semstrait-core` matches current code (`11 §8.4.3`).
- `diagnostic` vs `error` — `diagnostic` exposes the generic envelope (`Diagnostic<K>`, `Diagnose` trait) and shared primitives (`Severity`, `Location`); `error` exposes the two narrow core-emitted kind enums (`ValidateErrorKind`, `CompileErrorKind`). Keeping them split reinforces `30 §5`'s typed-kind-per-stage discipline: the envelope is generic, the kind is per-stage. Per `30 §5` the `IntoDiagnostic` trait of earlier drafts is retired; construction is direct via `Diagnose` impls plus a per-crate helper.

**Re-exports.** The crate root (`lib.rs`) re-exports a curated surface. Non-root re-exports are forbidden — consumers either import `semstrait_core::Expr` or `semstrait_core::expr::Expr`, never both. The full re-export list is in §14.

## 3. Public Types — `Expr` Family

### 3.1 Inner AST — `Expr`

```rust
/// The canonical low-level expression AST used across the entire pipeline.
/// Not a direct field type outside the expression module — fields use the
/// wrapper types `SemanticExpr` or `PhysicalExpr` below. See `14 §3.2`.
#[non_exhaustive]
pub enum Expr {
    Column       { name: String },
    Literal      { value: LiteralValue },
    EntityRef    { name: String },
    BinaryOp     { op: BinaryOpKind, left: Box<Expr>, right: Box<Expr> },
    Negate       { expr: Box<Expr> },
    Not          { expr: Box<Expr> },
    Case         { when: Vec<WhenClause>, else_expr: Option<Box<Expr>> },
    Cast         { expr: Box<Expr>, target: DataType },
    InList       { expr: Box<Expr>, list: Vec<Expr>, negated: bool },
    Between      { expr: Box<Expr>, low: Box<Expr>, high: Box<Expr>, negated: bool },
    IsNull       { expr: Box<Expr> },
    IsNotNull    { expr: Box<Expr> },
    Like         { expr: Box<Expr>, pattern: Box<Expr>, negated: bool },
    ILike        { expr: Box<Expr>, pattern: Box<Expr>, negated: bool },
    RegexpMatch  { expr: Box<Expr>, pattern: Box<Expr>, negated: bool },
    RegexpExtract{ expr: Box<Expr>, pattern: Box<Expr>, group: Box<Expr> },
    Coalesce     { args: Vec<Expr> },
    NullIf       { left: Box<Expr>, right: Box<Expr> },
    DateTrunc    { expr: Box<Expr>, grain: Grain },
    Aggregate    { aggregation: Aggregation, expr: Box<Expr>, distinct: bool },
    FunctionCall { name: String, args: Vec<Expr> },
}
```

20 variants total (`14 §3.2`). `#[non_exhaustive]` per I10. The inner AST carries no invariants beyond shape well-formedness; wrapper-level context invariants live on `SemanticExpr` / `PhysicalExpr`.

**Traversal.** `Expr` exposes pre-order `walk`, post-order `transform`, and an `Iterator`-style `children()` method per `14 §3.4`:

```rust
impl Expr {
    pub fn walk<V: ExprVisitor>(&self, visitor: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<Expr, ValidateErrorKind>
    where F: FnMut(Expr) -> Result<Expr, ValidateErrorKind>;
    pub fn children(&self) -> impl Iterator<Item = &Expr>;
}
```

**Construction-site error shape.** Core constructors that fail in-place return the bare kind (`ValidateErrorKind` or `CompileErrorKind`), not the full `Diagnostic<K>` envelope. The constructor knows what went wrong (the kind) but not where the call was made (the location). Callers in `semstrait-model` / `semstrait-manifest` wrap the kind into a `Diagnostic<…>` at the call site, attaching the source location they have. Per `30 §5`'s typed-kind discipline, this is direct construction — there is no `IntoDiagnostic` trait in the new design.

### 3.2 `SemanticExpr` — semantic-layer wrapper

```rust
/// Newtype wrapper over `Expr` for semantic-layer composition. Invariants:
/// - `Expr::Column` forbidden
/// - `Expr::EntityRef` allowed
/// - `Expr::Aggregate` allowed at any depth except nested-in-aggregate
/// Per `14 §2.2`.
///
/// `#[non_exhaustive]` is NOT applied — this is a newtype over a stable
/// inner type, per `30`'s "newtype-over-stable" exception.
pub struct SemanticExpr(Expr);

impl SemanticExpr {
    /// Construction boundary: validates invariants, then wraps.
    pub fn new(expr: Expr) -> Result<Self, ValidateErrorKind>;
    pub fn as_expr(&self) -> &Expr;
    pub fn into_expr(self) -> Expr;
    pub fn walk<V: ExprVisitor>(&self, v: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<Self, ValidateErrorKind>
    where F: FnMut(Expr) -> Result<Expr, ValidateErrorKind>;
}
```

Invariants checked at `new()` / `transform()` boundary, never implicitly. Any rewrite that would introduce an `Expr::Column` leaf raises `ValidateErrorKind::ColumnInSemanticExpr` per `14 §7.2`.

### 3.3 `PhysicalExpr` — binding-layer wrapper

```rust
/// Newtype wrapper over `Expr` for the binding layer. Invariants:
/// - `Expr::Column` allowed
/// - `Expr::EntityRef` forbidden
/// - `Expr::Aggregate` forbidden
/// Per `14 §2.3`. Carries compile-enriched fields populated by `14b`.
pub struct PhysicalExpr {
    expr: Expr,
    /// Populated by compile; None at authoring sites.
    pub inferred_type: Option<DataType>,
    /// Populated by compile; set of Column names referenced in the expr tree.
    pub referenced_columns: Vec<String>,
}

impl PhysicalExpr {
    /// Parse-site construction (pre-compile).
    pub fn new_authored(expr: Expr) -> Result<Self, ValidateErrorKind>;

    /// Compile-time construction from a fully-resolved form.
    pub fn new_resolved(
        expr: Expr,
        inferred_type: DataType,
        referenced_columns: Vec<String>,
    ) -> Result<Self, ValidateErrorKind>;

    pub fn as_expr(&self) -> &Expr;
    pub fn into_expr(self) -> Expr;
    pub fn walk<V: ExprVisitor>(&self, v: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<Self, ValidateErrorKind>
    where F: FnMut(Expr) -> Result<Expr, ValidateErrorKind>;
}
```

Fields are `pub` read/write because `14 §2.3` specifies them as compile-enrichment targets; `new_resolved` is the sealed construction path for `ResolvedExprTable` entries (`14b`). Newtype-over-stable exception applies per `30`.

### 3.4 `ExprSource` — YAML representation enum

```rust
/// The YAML representation of an expression. Dispatched by `semstrait-model`
/// at each parse site to either `SemanticExpr` or `PhysicalExpr`.
/// Per `14 §4`.
#[non_exhaustive]
pub enum ExprSource {
    /// Constrained SQL-like DSL string. Per `14 §4.3`.
    Inline(String),
    /// Structured YAML tree. Per `14 §4.4`.
    Block(ExprBlock),
}

/// Declarative YAML block form. Each block is a single-key tag; the key is
/// either a reserved AST tag (§4.4.1 of `14`) or a function-registry name.
/// Shape enumeration lives in `14 §4.4`.
#[non_exhaustive]
pub enum ExprBlock {
    // One variant per §4.4.1 reserved tag + a FunctionCall catch-all for
    // registry-dispatched tags. Exhaustive enumeration tracked in
    // `14 §4.4.1`'s table; this struct reproduces that table 1:1 in Rust form.
    // Full variant list is §4.4.1 of `14`; structural parity is a doctest.
}
```

Parse-site dispatch methods live on `semstrait-model` (`ExprSource::parse_semantic` / `::parse_physical`), not here. `semstrait-core` owns only the structural definition.

### 3.5 Supporting AST types — `expr::types`

```rust
/// Binary operator discriminator. Per `14 §3.2`.
#[non_exhaustive]
pub enum BinaryOpKind {
    Add, Subtract, Multiply, Divide, SafeDivide, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

/// Closed aggregate enum. Per `14 §3.2` / `§5.4`. The enum is CLOSED —
/// `CountDistinct` is NOT a variant; it is expressed as
/// `Aggregate { aggregation: Count, distinct: true }` per `14 §3.2` note.
///
/// Non-exhaustive per I10: even closed-for-today enums reserve non-breaking
/// future extension (e.g. should `StddevPop` ever be adopted into the core
/// set, not adding it via `FunctionCall` with `FunctionCategory::Aggregate`).
#[non_exhaustive]
pub enum Aggregation {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

/// Typed literal per `14 §5.1`.
#[non_exhaustive]
pub enum LiteralValue {
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

/// CASE WHEN clause per `14 §3.2`.
pub struct WhenClause {
    pub condition: Expr,
    pub result: Expr,
}
```

`BinaryOpKind` lists 14 variants per `14 §3.2`. `Aggregation` lists 5 canonical variants (**not 6** — `CountDistinct` is encoded via the `distinct: bool` flag on `Expr::Aggregate`, per `14 §3.2` catalog row and `14 §3.3` design notes). See §15 and `/docs/design/questions/open/31_questions.md#q1-canonical-aggregation-variant-count` for the naming-discrepancy rationale.

### 3.6 Visitor trait — `expr::visit`

```rust
/// Pre-order / post-order traversal driver. Implementations provide node
/// handlers; `Expr::walk` / `Expr::transform` dispatch.
pub trait ExprVisitor {
    type Output;
    fn visit(&mut self, expr: &Expr) -> Self::Output;
}
```

Sealed-trait pattern applies: external impls may provide analysis visitors; they SHOULD NOT rely on specific variant ordering or variant counts, since `Expr` is `#[non_exhaustive]`. Default implementation of `visit` (when `Output = ()`) SHOULD descend children; a provided `walk_children` helper makes this one-liner-safe.

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

Construction of `Decimal { precision, scale }` with out-of-range values is rejected at `semstrait-model` parse time (`ParseError::InvalidDecimalParameters`); `semstrait-core` performs no validation at the constructor.

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

`TypeClass` is exposed but **not** used as a public `FnSignature.args` element in v1 — Q6 at `14a §3.3` ratified overload-set polymorphism, not type-class generics. `TypeClass` exists as vocabulary for future registry evolution (`[TD-REGISTRY-TYPECLASS]`, `14a §10.1`) and for documentation / advisory diagnostics.

### 4.4 Nullability

Nullability is **NOT** exposed as a separate type in `semstrait-core`. Per `13 §2` and `14a §3.4` Q7 ratification, the canonical `DataType` is nullable-by-default; per-engine nullability tightening lives in `registry/types_mapping.md`, not in a `Nullability` enum on this crate. A `Nullability` type was considered and rejected at `14a §3.4`; any future change goes through that doc's ratification loop.

### 4.5 Serialization feature flag

```rust
#[cfg(feature = "serde")]
impl serde::Serialize for DataType { /* ... */ }
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for DataType { /* ... */ }
// ... similarly for Grain, TypeClass, Expr, SemanticExpr, PhysicalExpr,
// ExprSource, ExprBlock, LiteralValue, BinaryOpKind, Aggregation,
// WhenClause, and all types in §5–§7
```

Feature-gated per §11. Off by default; `semstrait-model` / `semstrait-manifest` enable it transitively (they require YAML / JSON round-tripping).

## 5. Public Types — `FunctionRegistry` Family

### 5.1 `CanonicalFn` — stable function identifier

```rust
/// Stable identifier for a canonical function. Per `00 §4.1` / `14a §2`.
/// Implemented as a newtype over `&'static str` with `pub const` identities;
/// NOT a closed enum, enabling unbounded catalog growth without API
/// breakage. Construction is CRATE-PRIVATE to `semstrait-core`.
///
/// Newtype-over-stable exception: `#[non_exhaustive]` does NOT apply.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CanonicalFn(&'static str);

impl CanonicalFn {
    pub const fn name(self) -> &'static str { self.0 }

    // Canonical identities — one `pub const` per entry in 14a §4.2–§4.6.
    pub const UPPER:    CanonicalFn = CanonicalFn("upper");
    pub const LOWER:    CanonicalFn = CanonicalFn("lower");
    pub const LENGTH:   CanonicalFn = CanonicalFn("length");
    pub const ABS:      CanonicalFn = CanonicalFn("abs");
    pub const ROUND:    CanonicalFn = CanonicalFn("round");
    // ... full list tracked against `14a §4.2–§4.6` Round-2 population.
    // Adding constants is non-breaking; adapters match on constant equality.
}
```

The only public constructor is the `pub const` associated-constant form. There is no `CanonicalFn::from_str` — adapters that need to match a registered name against a canonical identity compare against a `pub const` by equality. This keeps the **single source of truth** at the `FunctionRegistry` (per `14a §2.2`); `CanonicalFn` is a matching-ergonomics wrapper, not a parallel registry.

### 5.2 `FunctionRegistry`

```rust
/// Authoritative catalog of canonical functions. Per `14a §2`.
/// Built at process startup from core entries (§4.2–§4.6 of `14a`) +
/// all linked `RegistryExtension` impls. Sealed and immutable post-init.
pub struct FunctionRegistry {
    /* crate-private: flat HashMap<&'static str, FunctionSpec> per 14a §2.2 */
}

impl FunctionRegistry {
    /// O(1) name-keyed lookup. Returns None for an unknown name per
    /// `14a §2.3`. Case-sensitive exact match; no aliases in v1.
    pub fn lookup(&self, name: &str) -> Option<&FunctionSpec>;

    /// Iterate all registered specs. Consumed by documentation tools and
    /// adapter audit passes; not part of the hot compile/plan path.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, &FunctionSpec)>;
}
```

No mutation API. No per-invocation configurability in v1 (`[TD-REGISTRY-MULTI-CONFIG]`). The process-global accessor is in §9.

### 5.3 `FunctionSpec`

```rust
/// Per-entry catalog record. Per `14a §3.1` Q4.
#[non_exhaustive]
pub struct FunctionSpec {
    pub canonical_name: &'static str,
    pub category: FunctionCategory,
    /// Non-empty list of overloads. First-exact-match wins per
    /// `14a §3.3` Q6.
    pub signatures: &'static [FnSignature],
    pub description: &'static str,
}
```

`signatures` is `&'static [FnSignature]` (not `NonEmpty<FnSignature>`) for const-friendliness at registration — invariant "signatures is non-empty" is checked at registry seal time and produces a panic on violation. The `NonEmpty` shape expressed in `14a §3.1`'s prose collapses to "slice + panic-on-empty at seal" for the const-compatible surface.

### 5.4 `FnSignature`

```rust
/// A single overload shape. Per `14a §3.5` Q8.
#[non_exhaustive]
pub struct FnSignature {
    pub args: &'static [ParamType],
    pub variadic: Option<ParamType>,
    pub return_type: ReturnTypeRule,
}
```

Trailing-variadic-only. Optional args are expressed as multiple overloads differing in arity (§3.5 Q8). Mid-signature variadic is `[TD-REGISTRY-MID-VARIADIC]`.

### 5.5 `ParamType`

```rust
/// Per-argument admissibility vocabulary. In v1, strictly concrete
/// canonical `DataType` values — per `14a §3.3` Q6's overload-set policy
/// (no TypeClass generics). The extra variant is preserved for future
/// TypeClass-based generics (`[TD-REGISTRY-TYPECLASS]`).
#[non_exhaustive]
pub enum ParamType {
    /// Exact canonical `DataType`.
    Exact(DataType),
    /// Reserved for future `[TD-REGISTRY-TYPECLASS]` — NOT authoring-legal
    /// in v1; registry-seal rejects entries that use this variant with
    /// `AdapterFunctionCollision::InvalidParamType`.
    TypeClass(TypeClass),
}
```

The `TypeClass` variant is **exposed but not activated** in v1 per the Q6 ratification. Exposing it now keeps the enum stable under I10 when `[TD-REGISTRY-TYPECLASS]` lands; activation is an adapter-facing contract change, not an enum-variant addition.

### 5.6 `ReturnTypeRule`

```rust
/// Return-type derivation rule. Per `14a §3.4` Q7 minimal-4 set.
#[non_exhaustive]
pub enum ReturnTypeRule {
    Fixed(DataType),
    SameAs(usize),
    /// Common-supertype promotion of the listed arg indices per `14 §5.4`.
    Promoted(&'static [usize]),
    /// Arbitrary rule — `cast(x, T) -> T`, width-dependent decimal rules.
    /// Returns the bare `CompileErrorKind`; callers wrap into a
    /// `Diagnostic<CompileErrorKind>` with the call-site location.
    Custom(fn(&[DataType]) -> Result<DataType, CompileErrorKind>),
}
```

### 5.7 `FunctionCategory`

```rust
/// Flat category per `14a §3.2` Q5.
/// No scalar sub-categorization in v1 (`[TD-REGISTRY-SUBCATEGORY]`).
#[non_exhaustive]
pub enum FunctionCategory {
    Scalar,
    Aggregate,
    // Window — deferred per `14` TD-EXPR-WINDOW.
}
```

### 5.8 `RegistryExtension`

```rust
/// Adapter-contributed registry entries. Per `14a §7.1` Q15.
/// Implemented on a zero-sized marker type in an adapter crate; the
/// `function_registry()` initializer folds `FUNCTIONS` into the flat map
/// at program startup.
pub trait RegistryExtension {
    const ADAPTER_ID: &'static str;
    const FUNCTIONS: &'static [FunctionSpec];
}
```

Sealed-trait pattern deliberately NOT applied — adapter crates outside the workspace (e.g. a third-party `semstrait-adapter-clickhouse`) must be able to `impl RegistryExtension` without a sealed-trait escape hatch. Collision handling at seal time lives in §5.2's prose and `14a §7.2`.

## 6. Public Types — Constraint Family

Per `11 §8.3`–`§8.4`. Current type name `MeasureConstraints` is a legacy artifact for the Measure + Metric carriers (`11 §8.4.3`, `[TD-CONSTRAINT-RENAME]`); the three sub-blocks below retain their current names to avoid a breaking rename before the SemanticManifest-schema revision pass.

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
    /// Empty — no restrictions.
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
/// tokens (matching the `Aggregation` enum names plus `COUNT_DISTINCT`
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

**Why `Vec<String>` rather than `Vec<Aggregation>` for `allowed` / `prohibited`.** Per `11 §8.4.1`'s DSL shape: the field accepts UPPERCASE tokens including `COUNT_DISTINCT` (which is not a `Aggregation` enum variant but an `Aggregation::Count { distinct: true }` encoding). Matching is token-based against a planner-owned normalization; `semstrait-core` exposes the shape, not the matching logic (which lives in `semstrait-planner` per `11 §8.6`).

## 7. Public Types — Diagnostic Primitives

`semstrait-core` provides the **diagnostic envelope** every consumer crate composes around its own per-stage typed-kind enum, plus the `Diagnose` trait those kinds implement. Authoritative sub-shape per `30 §5`.

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

Severity carries message intent only; control flow (accumulating vs fail-fast) lives in the function signature, not the diagnostic. Per `30 §5.2`.

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

`Span` retains its `30 §5.3` byte-range shape (renamed from `ByteSpan` in earlier drafts to align with `30 §5.3`'s vocabulary). `SourceId` is opaque — it is NOT `#[non_exhaustive]` because its variant set is private; the crate's public surface exposes only `SourceId::unknown()` and the `Eq` / `Hash` / `Display` traits consumers need.

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

Both blanket impls live in `semstrait-core::diagnostic` because of the orphan rule — no other crate could provide them. `Diagnose::cause()` is the source of truth for the `std::error::Error` chain; the kind variant decides what to chain (typed foreign-error wrapping per `30 §5.4`).

### 7.6 `Diagnostics<K>` ergonomics

`Diagnostics<K>` is a transparent `Vec<Diagnostic<K>>` alias, so all `Vec` methods apply directly. Common patterns at the consumer level:

```rust
// Partition into errors and warnings:
let (errors, warnings): (Vec<_>, Vec<_>) = diagnostics
    .into_iter()
    .partition(|d| d.severity == Severity::Error);

// Convert kind via From for cross-stage lifting:
let api_diags: Diagnostics<SemStraitErrorKind> = parse_diags
    .into_iter()
    .map(|d| Diagnostic { kind: d.kind.into(), ..d })
    .collect();
```

Per `30 §5.6`, fused helpers in `semstrait-api` use a sum-typed kind (`SemStraitErrorKind`) and lift per-stage results via `From<ParseErrorKind>`, `From<ValidateErrorKind>`, etc. — `From` impls live on the fused-kind enum, not on `Diagnostic<K>`.

## 8. Public Types — Core-Emitted Kind Enums

Per `30 §5`, each crate owns its own per-stage typed-kind enum implementing `Diagnose`. `semstrait-core` provides two **narrow** kind enums, scoped strictly to failures core code itself raises:

- `ValidateErrorKind` — emitted at `SemanticExpr::new`, `PhysicalExpr::new_authored`, and `PhysicalExpr::new_resolved` constructors.
- `CompileErrorKind` — emitted by `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec`.

The model-level / manifest-level / planner-level / adapter-level kinds (`ParseErrorKind`, `model::ValidateErrorKind`, `manifest::CompileErrorKind`, `PlanErrorKind`, `AdaptErrorKind`, …) live in their owning crates and MAY embed the core kinds via D.ii cross-stage nesting (`30 §7.4`).

### 8.2 `ValidateErrorKind`

```rust
/// Wrapper-construction failures raised by `SemanticExpr::new`,
/// `PhysicalExpr::new_authored`, `PhysicalExpr::new_resolved`. Per
/// `14 §7.2`. Implements `Diagnose` (`§7.4`).
#[non_exhaustive]
pub enum ValidateErrorKind {
    /// `SemanticExpr` rejected a tree containing `Expr::Column`.
    ColumnInSemanticExpr   { column: String },

    /// `PhysicalExpr` rejected a tree containing `Expr::EntityRef`.
    EntityRefInPhysicalExpr { name: String },

    /// `PhysicalExpr` rejected a tree containing `Expr::Aggregate`.
    AggregateInPhysicalExpr,

    /// `Expr::Aggregate` was nested inside another `Expr::Aggregate`.
    NestedAggregate { outer: String, inner: String },

    /// A reserved identifier appeared in author position
    /// (e.g. a `Column` named `column`, an `EntityRef` named `entity_ref`).
    ReservedIdentifier { name: String, kind: ReservedKind },

    /// `Expr::Case` `WhenClause::condition` failed the boolean check
    /// (the condition's inferred type is not `Boolean`).
    CaseConditionNotBoolean,
}

#[non_exhaustive]
pub enum ReservedKind {
    Column,
    EntityRef,
    FunctionCall,
    BlockTag,
}

impl Diagnose for ValidateErrorKind {
    fn message(&self) -> String { /* per-variant rendering */ }
    fn severity_default(&self) -> Severity { Severity::Error }
    // No cause() override — these variants do not wrap foreign errors.
}
```

Variants that previously lived in this enum but are NOT core-emitted (`DimensionTypeMalformed`, `InvalidGrainValue`, `MetadataDimensionMalformed`, `BucketsOverlap`, `ShapeMalformed`, `ShapeFieldConflict`) move to `semstrait-model`'s own `ValidateErrorKind` per Pass 2.

### 8.3 `CompileErrorKind`

```rust
/// Failures from `ReturnTypeRule::Custom(fn(&[DataType]) -> ...)`
/// callbacks wired into `FunctionSpec`. Per `14a §3.4` and `14 §7.3`.
/// Implements `Diagnose` (§7.4).
#[non_exhaustive]
pub enum CompileErrorKind {
    /// The custom callback could not infer a return type for the
    /// argument types it was given. `reason` is callback-supplied.
    TypeInferenceFailure { reason: String },

    /// Computed-column type-inference produced a type that does not
    /// match the user-declared type.
    ComputedTypeUnifyConflict { declared: DataType, inferred: DataType },

    /// A literal value cannot fit into its target type without overflow.
    LiteralOverflow { value: String, target: DataType },

    /// A literal value would lose precision when coerced into its
    /// target type (e.g., narrowing decimal cast).
    LiteralPrecisionLoss { value: String, target: DataType },

    /// The engine-side physical type cannot be represented in the
    /// canonical `DataType` lattice.
    UnrepresentablePhysicalType { engine_type: String },
}

impl Diagnose for CompileErrorKind {
    fn message(&self) -> String { /* per-variant rendering */ }
    fn severity_default(&self) -> Severity { Severity::Error }
}
```

Variants that previously lived under the broader `CompileError` enum but are NOT core-emitted (name resolution: `UnresolvedEntityRef`, `UnreachableSemanticsReference`, `CircularSemanticsReference`, `UnresolvedColumn`, `UnresolvedCrossKindReference`; function resolution: `UnknownFunction`, `FunctionArityMismatch`, `NoMatchingSignature`, `ReservedTagCollision`, `AdapterFunctionShadowsCore`, `AdapterFunctionCollision`; shape unification: `SemanticShapeConflict`, `GrainAxisMismatch`) move to `semstrait-manifest`'s own `CompileErrorKind` per Pass 2 and `33`'s ratification.

### 8.4 No code() methods, no IntoDiagnostic

Both enums implement the `Diagnose` trait only. There is no `code() -> &'static str`, no kebab-case identifier, no legacy numeric-code constant. Stable identification is variant identity (renaming a variant is MAJOR per `30 §2`; adding one inside `#[non_exhaustive]` is MINOR).

The `IntoDiagnostic` trait of earlier drafts is **retired**. Constructing a `Diagnostic<K>` from a kind is direct: callers (or per-crate helper functions) build `Diagnostic { kind, severity: kind.severity_default(), location: …, notes: vec![] }` at the point of failure.

### 8.5 Display / Error blanket impls

```rust
impl std::fmt::Display for ValidateErrorKind { /* delegates to Diagnose::message */ }
impl std::error::Error for ValidateErrorKind {}
impl std::fmt::Display for CompileErrorKind { /* delegates to Diagnose::message */ }
impl std::error::Error for CompileErrorKind {}
```

These complement the blanket impls on `Diagnostic<K>` (§7.5): callers may use the bare kind directly in `?` chains (without the `Diagnostic<K>` envelope) when they have no location to attach.

## 9. Public Free Functions

The crate's free-function surface is deliberately tiny. If a piece of logic does not fit into one of the types above, it SHOULD live in a downstream crate.

### 9.1 `function_registry()`

```rust
/// Returns the process-global, sealed function registry. Initialized
/// lazily via `OnceLock` on first call; subsequent calls return the same
/// `&'static`. Per `14a §2.1` Q1.
///
/// Extensions contributed via `RegistryExtension` impls are folded in at
/// initialization; collisions panic at registry-seal time (registry-build
/// failures are bugs, not recoverable conditions, per `14a §7.2`). The
/// panic messages reference the colliding `CanonicalFn::name()` and
/// adapter `ADAPTER_ID`s. Post-init the registry is immutable; there is
/// no `rebuild` or `with_extensions` surface in v1 (`[TD-REGISTRY-MULTI-CONFIG]`).
pub fn function_registry() -> &'static FunctionRegistry;
```

### 9.2 `is_reserved_tag`

```rust
/// Returns true when `tag` is one of the 21 reserved AST tags from
/// `14 §4.4.1`. Consulted by the Declarative-block parser in
/// `semstrait-model` and by the registry-seal collision check in
/// `function_registry()`.
pub fn is_reserved_tag(tag: &str) -> bool;
```

**The 21 reserved tags** (complete enumeration per `14 §4.4.1`):
`column`, `literal`, `entity_ref`, `binary_op`, `negate`, `not`, `case`, `cast`, `in_list`, `between`, `is_null`, `is_not_null`, `like`, `ilike`, `regexp_match`, `regexp_extract`, `coalesce`, `nullif`, `date_trunc`, `aggregate`, `function_call`.

### 9.3 No other pure-function utilities

A `coarseness(g: Grain) -> u8` free-function form was considered and rejected — it is already exposed as `Grain::coarseness(self)` per `13 §3.2`. A `type_class_of(dt: DataType) -> TypeClass` helper was considered and rejected — it would encode classification policy that `13 §4` leaves to authors. Any new free function requires ratification against this section in an amendment.

## 10. Traits Exported

Per `30`'s trait-surface rules: every public trait that can be externally implemented SHOULD be evaluated against the sealed-trait pattern. Sealed traits prevent external impls; non-sealed traits are part of the external extension surface.

| Trait | Surface | Externally implementable? | Sealed? | Source |
|---|---|---|---|---|
| `ExprVisitor` | `fn visit(&mut self, expr: &Expr) -> Self::Output` | yes — analysis / audit visitors | no | §3.6 |
| `RegistryExtension` | `const ADAPTER_ID`, `const FUNCTIONS` | yes — adapter crates MUST impl | no | §5.8 |
| `Diagnose` | `fn message`, `fn severity_default`, `fn cause` | yes — third-party kind enums slot into `Diagnostic<K>` | no | §7.4 |

**Sealed-trait justification, positive cases.** None. All external-facing traits are non-sealed because:

- `ExprVisitor` — third-party analysis passes (e.g. a test-harness visitor counting `Aggregate` nodes) MUST be able to impl without a sealed escape hatch.
- `RegistryExtension` — third-party adapter crates MUST be able to impl without a workspace-private escape hatch (this is the primary extensibility point).
- `Diagnose` — a third-party error type from e.g. a user-defined plugin MAY define its own kind and slot into `Diagnostic<K>`.

## 11. Feature Flags

v1 has a small, axis-orthogonal flag set. The `io`-family flags were added in the `31b` ratification; they amend the original "no runtime-only deps" posture (see §12).

| Feature | Default | Gates | Reason |
|---|---|---|---|
| `serde` | OFF | `Serialize` / `Deserialize` on every public type (`Expr`, `SemanticExpr`, `PhysicalExpr`, `ExprSource`, `ExprBlock`, all support enums, `DataType`, `Grain`, `TypeClass`, `*Constraints`, `Diagnostic<K>` (where `K: Serialize`), `Severity`, `Location`, `Span`, `SourceId`, `CanonicalFn`, `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`, `FunctionCategory`, `ValidateErrorKind`, `CompileErrorKind`) | keeps the crate's dependency footprint minimal for consumers that only need the types (e.g. test harnesses that manipulate `Expr` in memory); `semstrait-model` / `semstrait-manifest` enable it transitively |
| `schemars` | OFF | JSON schema derivations on the same types | consumers needing JSON-Schema emission pay a second compile cost; off by default per `30` |
| `io` | **ON** | The `io` module — `Source` / `Sink` / `FromIoBytes` / `IntoIoBytes` / `Location` / `IoError` + `backends::memory` + `backends::local`; pulls `tokio`, `bytes`, `object_store` (Local + InMemory features), `dashmap` | ergonomic common case for every downstream crate that wants transport; disable with `default-features = false` for pure-type consumers (see `31b §9.1`) |
| `io-aws` | OFF | `Location::S3` variant + `backends::s3::{S3Source, S3SourceBuilder}`; enables `object_store/aws` which transitively pulls the AWS config / credential crates | cloud SDK footprint stays opt-in; enabled explicitly by CLI / `semstrait-api` / `semstrait-facade` |

No other I/O features in v1. Future `io-http`, `io-gcs`, `io-azure` land additively behind the same gating pattern (all three are already supported by `object_store` — the work per feature is ~30 LOC of trait delegation plus a feature flag).

**Delta with current code.** Today `semstrait-core`'s `Expr` / `DataType` / constraint types derive `Serialize` / `Deserialize` unconditionally, and there is no `io` module. Moving `serde` behind `#[cfg(feature = "serde")]` is tracked under `[TD-CORE-SERDE-GATING]` in `implementation/40_refactor_plan.md`. The `io` module is a net-new addition; the existing `crates/semstrait-manifest/src/io.rs` is folded into `semstrait-core::io::backends::{local, s3}` via `object_store` wrapping per the `31b` ratification and `TD-008` migration.

No other feature flags in v1. `arrow-feature` gating is explicitly rejected — engine-specific data-plane deps violate I11 and fragment the API.

## 12. Dependency Posture

### 12.1 External dependencies

A canonical `Cargo.toml` target after the `31b` ratification:

```toml
[dependencies]
thiserror = "^"                         # error enum derivations
nonzero_ext = "^"                       # NonZero usize for Span tightening (optional)

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

**Runtime dependency posture.** Under default features, `semstrait-core` pulls `tokio`, `bytes`, `dashmap`, and `object_store` (with its `Local` + `InMemory` back-ends compiled; no cloud SDKs unless `io-aws` is enabled). Under `--no-default-features`, the crate retains its historical zero-runtime-dep shape — only `thiserror` (and the optional `nonzero_ext`) remain. Pure-type consumers take the `--no-default-features` path.

**`object_store` as internal detail.** Consumers never see `object_store::ObjectStore`, `object_store::Path`, or any of its error types on a public signature. The one escape hatch is `S3SourceBuilder::with_object_store_builder(object_store::aws::AmazonS3Builder)` — callers opting into advanced S3 configuration implicitly opt into `object_store` evolution. See `31b §1.4` for the adoption rationale and SR-IO-8 for the encapsulation rule.

**No other runtime deps.** No `async-trait` (stable async-fn-in-trait suffices), no `futures` beyond what `tokio` re-exports, no `reqwest`, no `hyper`, no `sqlx`, no direct `aws-sdk-s3` / `aws-config` (they come in transitively via `object_store/aws`).

**No engine-identity dependencies.** No `datafusion`, no `arrow`, no `spark-*`, no `duckdb`, no `substrait`. These live in `semstrait-adapter` and its per-engine modules.

### 12.2 Internal (workspace) dependencies

**Zero.** `semstrait-core` is the root of the workspace DAG per I7. Every other crate depends on `semstrait-core` directly or transitively; `semstrait-core` depends on no workspace crate. Attempting to add a `semstrait-*` dependency to `Cargo.toml` is a compile error in CI.

## 13. Invariants Upheld by the Crate

Concrete crate-level guarantees mapping to `00 §9` invariants:

| Invariant | `semstrait-core` guarantee |
|---|---|
| **I1** — no raw SQL in canonical layer | `Expr` is a typed AST; no `String`-as-SQL fields. Every `Column.name`, `EntityRef.name`, `FunctionCall.name` is an identifier, not SQL text. `ExprSource::Inline(String)` is the sole string-form input and it is deliberately a YAML-surface type, not a SemanticManifest-layer type — consumers convert it into `Expr` before it crosses any stage boundary. |
| **I2** — physical types belong to adapters | `DataType` variants are engine-neutral per `13 §2`. No `arrow::*` / `spark::*` / `datafusion::*` types are visible on any public surface. |
| **I5** — name resolution is compile-time | `SemanticExpr` and `PhysicalExpr` are wrapper-only; they expose no `resolve` method. `EntityRef.name` remains a `String` at the `semstrait-core` layer — resolution is performed by `semstrait-manifest::compile` and stored in the SemanticManifest's `ResolvedExprTable`. No runtime resolver trait is exported here. |
| **I6** — plan hot path is synchronous | No `pub async fn` exists on the plan hot path: `SemanticExpr`, `PhysicalExpr`, `FunctionRegistry`, `Diagnose` impls, validate / resolve / plan surfaces. The only `async fn`s at `semstrait-core` live in `io` (`Source::read`, `Sink::write`, `Location`'s impls) — I/O is explicitly outside the plan hot path. A CI lint enforces: no `async fn` outside `semstrait_core::io::*`. |
| **I7** — strict DAG | `Cargo.toml` contains zero `semstrait-*` entries in `[dependencies]`. A CI check greps the manifest and fails on any workspace-internal entry. |
| **I10** — extensibility | Every `pub enum` and `pub struct` (with the `30`-documented newtype-over-stable exception) carries `#[non_exhaustive]`. The exception set: `CanonicalFn`, `SemanticExpr`, `PhysicalExpr`, `Span`, `WhenClause`, `SourceId` (opaque per `30 §5.3`). An `integration-test` over `cargo public-api` enforces the `#[non_exhaustive]` rule. |
| **I11** — no downward I/O surprises | Transport primitives (`io::Source`, `io::Sink`, `io::Location`, `io::backends::{memory, local, s3}`) live on `semstrait-core` under the `io` feature flag (ratified in `31b`). Domain-specific load / dump (`load_model`, `load_manifest`) do not — they live in the crate that owns the typed artifact. `reqwest`, `hyper`, raw `std::net` sockets remain rejected; cloud SDKs (`aws-sdk-s3`) sit behind opt-in `io-aws`. The dependency audit (§12.1) is enforced in CI via `cargo deny`. |
| **I12** — first-class diagnostics | `Diagnostic<K>` and `Diagnose` are the workspace's diagnostic primitives. `Diagnostic<K>` carries `kind: K, severity, location, notes`; the kind decides per-variant rendering and severity defaults via `Diagnose`. No central error-code allocation; stable identification is variant identity. The parallel observability channel (`tracing`) is described in `30 §6`; library code never writes to stdout / stderr. `IoError` per `31b §6` is its own kind enum implementing `Diagnose`. |

## 14. Public API Surface Sketch

One rustdoc-style line per exported item, grouped by module. Doubles as the "test-the-contract" target — an integration test enumerates this list against `cargo public-api` output.

### 14.1 `expr`

```
pub use self::types::{BinaryOpKind, Aggregation, LiteralValue, WhenClause}
pub use self::visit::ExprVisitor
pub enum  Expr                                          // canonical AST, 20 variants
pub struct SemanticExpr                                 // newtype wrapper; EntityRef-ok, Column-forbidden
pub struct PhysicalExpr                                 // newtype wrapper; Column-ok, EntityRef/Aggregate-forbidden
pub enum  ExprSource                                    // YAML surface: Inline | Block
pub enum  ExprBlock                                     // Declarative-block variants per 14 §4.4.1
```

### 14.2 `expr::types`

```
pub enum  BinaryOpKind                                  // 14 variants per 14 §3.2
pub enum  Aggregation                                   // 5 variants: Sum | Avg | Count | Min | Max
pub enum  LiteralValue                                  // one variant per DataType + Null
pub struct WhenClause                                   // { condition: Expr, result: Expr }
```

### 14.3 `expr::visit`

```
pub trait ExprVisitor                                   // fn visit(&mut self, &Expr) -> Self::Output
```

### 14.4 `types`

```
pub enum  DataType                                      // 14 variants per 13 §2.1
pub enum  Grain                                         // 7 variants per 13 §3.1
pub enum  TypeClass                                     // 7 variants per 13 §4
impl Grain { pub fn coarseness(self) -> u8 }
```

### 14.5 `functions`

```
pub struct CanonicalFn                                  // newtype over &'static str; pub consts for catalog entries
pub struct FunctionRegistry                             // sealed, &'static; lookup + entries
pub struct FunctionSpec                                 // per-entry record
pub struct FnSignature                                  // overload shape with trailing variadic
pub enum   ParamType                                    // Exact(DataType) | TypeClass(TypeClass) [reserved]
pub enum   ReturnTypeRule                               // Fixed | SameAs | Promoted | Custom
pub enum   FunctionCategory                             // Scalar | Aggregate
pub trait  RegistryExtension                            // ADAPTER_ID + FUNCTIONS
pub fn     function_registry() -> &'static FunctionRegistry
```

### 14.6 `constraints`

```
pub struct MeasureConstraints                           // { dimensions, aggregations }
pub struct DimensionConstraints                         // { one_of, none_of, all }
pub struct AggregationConstraints                       // { allowed, prohibited }
// Each of the three exposes fn none() and fn is_empty(&self) -> bool.
```

### 14.7 `diagnostic`

```
pub struct Diagnostic<K: Diagnose>                      // { kind, severity, location, notes }
pub type   Diagnostics<K> = Vec<Diagnostic<K>>
pub enum   Severity                                     // Error | Warning
pub struct Location                                     // { source: SourceId, span: Span }
pub struct Span                                         // { start: usize, end: usize }
pub struct SourceId                                     // opaque; SourceId::unknown() + as_str()
pub trait  Diagnose                                     // fn message + severity_default + cause
// Blanket: Display / std::error::Error on Diagnostic<K> via Diagnose.
```

### 14.8 `error`

```
pub enum ValidateErrorKind                              // wrapper-construction failures (§8.2)
pub enum CompileErrorKind                               // ReturnTypeRule::Custom failures (§8.3)
pub enum ReservedKind                                   // Column | EntityRef | FunctionCall | BlockTag (§8.2)
// Each kind enum: impl Diagnose, impl Display, impl std::error::Error.
```

### 14.9 Free functions at crate root

```
pub fn is_reserved_tag(tag: &str) -> bool
pub fn function_registry() -> &'static FunctionRegistry   // re-export from `functions`
```

### 14.10 Crate-root re-exports (stable convenience surface)

```rust
// lib.rs
pub use crate::expr::{
    Expr, SemanticExpr, PhysicalExpr, ExprSource, ExprBlock,
    types::{BinaryOpKind, Aggregation, LiteralValue, WhenClause},
    visit::ExprVisitor,
};
pub use crate::types::{DataType, Grain, TypeClass};
pub use crate::functions::{
    CanonicalFn, FunctionRegistry, FunctionSpec, FnSignature, ParamType,
    ReturnTypeRule, FunctionCategory, RegistryExtension, function_registry,
};
pub use crate::constraints::{
    MeasureConstraints, DimensionConstraints, AggregationConstraints,
};
pub use crate::diagnostic::{
    Diagnostic, Diagnostics, Severity, Location, Span, SourceId, Diagnose,
};
pub use crate::error::{ValidateErrorKind, CompileErrorKind, ReservedKind};
pub use crate::is_reserved_tag;
```

### 14.11 `io` (feature `io`, default ON)

Full spec: `31b`. This section is a re-export sketch only.

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

Internally every back-end thin-wraps `object_store` (Apache Arrow project); `object_store` types never appear on a public signature except the one documented escape hatch (`S3SourceBuilder::with_object_store_builder`). See `31b §1.4` for the adoption rationale and SR-IO-8 for the encapsulation rule.

