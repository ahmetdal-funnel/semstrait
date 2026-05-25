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
//! ## Phase 2c additions (this iteration)
//!
//! - [`functions`] — canonical function catalog per spec `35 §8` /
//!   `14a §2`–`§7`. Owns [`functions::CanonicalFn`], the sealed
//!   [`functions::FunctionRegistry`] singleton, and the v1 47-entry
//!   built-in catalog (12 string + 11 math + 14 temporal + 2 logical +
//!   8 aggregate).

pub mod artifact;
pub mod error;
pub mod expr;
pub mod expr_kinds;
pub mod functions;
pub mod plan;
pub mod primitives;
pub mod tree;
pub mod types;

// ── Phase 2a re-exports (spec-aligned) ─────────────────────────────────

pub use error::{CompileError, IrErrorKind, ValidateError};
pub use expr_kinds::{
    AggregationOp, BinaryOpKind, CastFailure, ColumnRef, LikeKind, Literal, SemanticsName,
    UnaryOpKind, WindowBound, WindowFn, WindowFrame, WindowFrameKind,
};
pub use tree::{ExprLeaf, Rewriter, Tree, Visitor};

// ── Phase 2c re-exports (functions/ catalog) ───────────────────────────
//
// `CanonicalFn` lives here per `35 §8.2` / `14a §2`. The sealed registry
// singleton is reached via `function_registry()`.

pub use functions::{
    function_registry, Additivity, CanonicalFn, DimensionAxis, FnSignature, FunctionCategory,
    FunctionRegistry, FunctionSpec, ParamType, RegistryExtension, ReturnTypeRule,
};

// ── Phase 2b re-exports (expr/ subtree) ────────────────────────────────
//
// The `expr_fn` DSL is intentionally NOT re-exported at the crate root
// (busy namespace; consumers do `use semstrait_ir::expr::expr_fn::*;`).

pub use expr::{
    DimensionAccessor, Expr, KeyAccessor, MeasureAccessor, MetricAccessor, Parameter,
    ParameterKey, PhysicalExpr, PhysicalLeaf, SemanticExpr, SemanticLeaf,
};
pub use types::{DataType, Grain, Schema, SchemaColumn, TypeClass};

// ── Phase 2d re-exports (plan-tree primitives) ─────────────────────────
//
// Plan-level identifiers and structural-variant carriers per `35 §11`.
// Live alongside `types::DataType` rather than under a `plan::` namespace
// because they are reused across the IR (e.g. `AggregateExpr` is referenced
// from `expr::*` extension contexts) and the future plan-tree node types.

pub use primitives::{
    AggregateExpr, Cardinality, JoinType, KeyPair, Name, NullOrdering, ResolvedColumn, SortDir,
    SourceRef,
};

// ── Phase 2d re-exports (plan/ subtree) ────────────────────────────────
//
// Plan-tree node sum + per-variant payload structs + per-node metadata
// per `35 §10` / `§11.1`. Traversal helpers (P16) and the
// `SemanticPlan` wrapper (P17) re-export at landing.

pub use plan::{
    AggNode, AnnotationClass, BoundaryPosition, FetchNode, FilterNode, JoinNode, NodeId,
    NodeMeta, PlanNode, ProjectNode, ScanNode, SemAnnotation, SemanticPlan, SortNode, UnionNode,
    ValuesNode,
};

// ── Phase 2e re-exports (artifact/ family) ─────────────────────────────
//
// Adapter-consumable artifacts per `35 §12`. `35` ratifies the
// structural shape; `36` (`semstrait-adapter`) owns the emission
// semantics. `DialectId` is the only engine-identity vocabulary in IR
// per S7 — appears only on `SqlArtifact.dialect` and `Dialect::ID`.

pub use artifact::{
    Capability, Dialect, DialectId, EngineArtifact, EnginePlan, SqlArtifact,
};
