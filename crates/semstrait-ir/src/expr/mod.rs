//! Expression IR — `Expr<L>`, leaf sets, accessors, [`Parameter`], DSL.
//! Per spec `14 §3` / `14 §4` / `14 §5` / `14 §6` and `35 §2`.
//!
//! This module is the home of the **canonical-IR expression tree** and
//! its authoring-form surface:
//!
//! - [`tree`] — `Expr<L>` structural variants per `14 §3.3`. Implements
//!   [`crate::tree::Tree`] with structural well-formedness checks at the
//!   rebuild boundary (Aggregate / Window nesting; non-empty Coalesce /
//!   InList / Case).
//! - [`leaves`] — [`PhysicalLeaf`], [`SemanticLeaf`] and the type aliases
//!   [`PhysicalExpr`] / [`SemanticExpr`]. Per `14 §3.4` / `§3.5` / `§3.6`.
//! - [`accessor`] — per-kind accessor enums ([`DimensionAccessor`],
//!   [`MeasureAccessor`], [`MetricAccessor`], [`KeyAccessor`]) carried as
//!   `Option<…>` fields on the typed [`SemanticLeaf`] variants. Per
//!   `14 §4.1`.
//! - [`parameter`] — [`Parameter`] placeholder + closed [`ParameterKey`]
//!   enum. Per `14 §5`.
//! - [`expr_fn`] — authoring DSL: `col` / `field` / `dim` / `measure` /
//!   `metric` / `key` / `lit` / `physical_col` constructors; `std::ops`
//!   impls on `Expr<L>`; [`expr_fn::ExprFunctionExt`] comparison / predicate
//!   sugar; [`expr_fn::SemanticExprAccessorExt`] best-effort accessor
//!   builders. Per `14 §6.4.1` / `35 §6`.

pub mod accessor;
pub mod expr_fn;
pub mod leaves;
pub mod parameter;
pub mod tree;

pub use accessor::{DimensionAccessor, KeyAccessor, MeasureAccessor, MetricAccessor};
pub use leaves::{PhysicalExpr, PhysicalLeaf, SemanticExpr, SemanticLeaf};
pub use parameter::{Parameter, ParameterKey};
pub use tree::Expr;
