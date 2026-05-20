//! Semstrait Intermediate Representation (IR).
//!
//! Per the second-cascade landing (`STATUS.md` item Q, 2026-05-19), this
//! crate is the **canonical IR home** for the workspace. It owns the
//! traversal trait family, the structural-variant support enums, the
//! identifier carriers, and the narrow ir-emitted error kinds, in
//! addition to the plan-tree types that have always lived here.
//!
//! ## Phase 2a additions
//!
//! - [`tree`] — `Tree` / `Visitor` / `Rewriter` / `ExprLeaf` traits per
//!   spec `14 §3.1` / `§3.2`, `35 §3.2`.
//! - [`expr_kinds`] — `BinaryOpKind`, `UnaryOpKind`, `AggregationOp`,
//!   `LikeKind`, `CastFailure`, `WindowFn`, `WindowFrame`,
//!   `WindowFrameKind`, `WindowBound`, `Literal`, `ColumnRef`,
//!   `SemanticsName`, `CanonicalFn` per spec `14 §3.3` / `35 §3.4`.
//! - [`error`] — adds `ValidateError` / `CompileError` per spec
//!   `35 §15.1` / `§15.2`.
//!
//! ## Phase 2b additions (this iteration)
//!
//! - [`expr`] — the canonical-IR expression subtree per spec `14 §3` /
//!   `§4` / `§5` / `§6` and `35 §2`:
//!   - [`expr::tree`] — `Expr<L>` structural enum + `Tree` impl with
//!     structural well-formedness checks (Aggregate / Window nesting;
//!     non-empty Coalesce / InList / Case).
//!   - [`expr::leaves`] — `PhysicalLeaf`, `SemanticLeaf`, `PhysicalExpr`,
//!     `SemanticExpr`.
//!   - [`expr::accessor`] — `DimensionAccessor`, `MeasureAccessor`,
//!     `MetricAccessor`, `KeyAccessor`.
//!   - [`expr::parameter`] — `Parameter`, `ParameterKey`.
//!   - [`expr::expr_fn`] — authoring DSL constructors, `std::ops` impls,
//!     `ExprFunctionExt` and `SemanticExprAccessorExt` extension traits.
//!
//! ## Legacy modules (pre-spec-cascade)
//!
//! `annotation`, `artifact`, `plan`, `plan_builder`, `rewrite`, `schema`,
//! `substrait` are pre-cascade modules consumed by the `substrait`
//! subsystem. They reference `semstrait_core::expr` items that the
//! second cascade moved out of `semstrait-core`; the `expr/` subtree in
//! Phase 2b will replace those references. See
//! `[docs/design/implementation/40_refactor_plan.md](../../docs/design/implementation/40_refactor_plan.md)`.

pub mod error;
pub mod expr;
pub mod expr_kinds;
pub mod tree;

// Legacy pre-cascade modules — to be replaced by Phase 2b's `expr/`
// subtree. Currently disabled (`cfg(any())`) because Phase 1 removed
// `semstrait_core::expr`, which these modules consume; Phase 2b will
// stand up the replacement `expr/` subtree and either wire these legacy
// modules to it or retire them entirely. The file contents remain on
// disk untouched per Phase-2a scope rules; only the module declarations
// here are gated.
#[cfg(any())]
pub mod annotation;
#[cfg(any())]
pub mod artifact;
#[cfg(any())]
pub mod plan;
#[cfg(any())]
pub mod plan_builder;
#[cfg(any())]
pub mod rewrite;
#[cfg(any())]
pub mod schema;
#[cfg(any())]
pub mod substrait;

// ── Phase 2a re-exports (spec-aligned) ─────────────────────────────────

pub use error::{CompileError, ValidateError};
pub use expr_kinds::{
    AggregationOp, BinaryOpKind, CanonicalFn, CastFailure, ColumnRef, LikeKind, Literal,
    SemanticsName, UnaryOpKind, WindowBound, WindowFn, WindowFrame, WindowFrameKind,
};
pub use tree::{ExprLeaf, Rewriter, Tree, Visitor};

// ── Phase 2b re-exports (expr/ subtree) ────────────────────────────────
//
// The `expr_fn` DSL is intentionally NOT re-exported at the crate root
// (busy namespace; consumers do `use semstrait_ir::expr::expr_fn::*;`).

pub use expr::{
    DimensionAccessor, Expr, KeyAccessor, MeasureAccessor, MetricAccessor, Parameter,
    ParameterKey, PhysicalExpr, PhysicalLeaf, SemanticExpr, SemanticLeaf,
};

// ── Legacy re-exports (pre-spec-cascade) ───────────────────────────────

// Legacy `error` re-exports from the pre-cascade substrait subsystem.
// These remain active because the legacy error types live in this same
// `error` module, which compiles independently of the gated legacy
// modules above.
pub use error::{ConvertError, DeserializeError, SerializeError};

// The remaining legacy re-exports (`AdditivityAnnotation`,
// `AggregateRole`, `FilterSource`, `SemAnnotation`, `PlanArtifact`,
// `AggNode`, `AggregateMeasure`, `FetchNode`, `FilterNode`, `JoinNode`,
// `JoinType`, `LogicalPlan`, `NodeMeta`, `PlanNode`, `PlannerWarning`,
// `ProjectNode`, `ScanNode`, `SortDirection`, `SortKey`, `SortNode`,
// `UnionNode`, `Aggregation`, `BinaryOp`, `DataType`, `Expr`,
// `DefaultPlanBuilder`, `PlanBuilder`, `FunctionRewriter`,
// `FunctionTarget`, `Field`, `Schema`, `ExprConverter`,
// `FunctionRegistry`, `SubstraitSerializer`) are gated together with
// their owning modules above (`#[cfg(any())]`). They restore in Phase 2b
// once the `expr/` subtree is wired in.
