//! Authoring-surface DSL per spec `14 §6.4.1` / `35 §6`.
//!
//! Provides the six canonical free-function constructors (`col`, `field`,
//! `dim`, `measure`, `metric`, `key`) plus `lit`, `physical_col`, `std::ops`
//! impls on `Expr<L>`, and the [`ExprFunctionExt`] extension trait for
//! comparison / predicate sugar plus best-effort accessor builders.
//!
//! ## Scope decisions for this iteration
//!
//! - The six canonical constructors `col` / `field` / `dim` / `measure` /
//!   `metric` / `key` are **semantic-only** — they return [`SemanticExpr`].
//!   Per spec `14 §6.4.1` the `col` tag is also legal at physical-mapping
//!   sites; this iteration exposes a separate [`physical_col`] helper for
//!   that case. A unified `col` constructor that dispatches to either leaf
//!   set via a sealed trait (per `35 §6.1`) is deferred to a future
//!   iteration — the spec ratifies "both target types are constructible
//!   from a single name surface", and the `physical_col` helper plus
//!   `Expr::Leaf(PhysicalLeaf::Column(...))` direct construction satisfy
//!   that obligation while keeping this iteration's surface small.
//!
//! - `lit` is **semantic-only** for the same reasons. Physical-mapping
//!   authors construct `Expr::Leaf(PhysicalLeaf::Literal(...))` directly.
//!
//! - The accessor builder methods on [`ExprFunctionExt`] (`first` / `last`
//!   / `previous` / `next` / `delta` / `percent_change` / `lag` / `lead`)
//!   are **best-effort** at v1 per spec `35 §6.3` — they pattern-match on
//!   the inner [`crate::expr::leaves::SemanticLeaf`] variant when the
//!   `Expr` root is a leaf, and otherwise return the expression unchanged
//!   plus a `debug_assert` (so dev builds catch misuse). Spec `19 §3.3` is
//!   the source of truth at compile; the DSL is opt-in ergonomic sugar.
//!
//! - Comparison and predicate methods (`eq` / `neq` / `lt` / `lt_eq` /
//!   `gt` / `gt_eq` / `is_null`) are generic over `L: ExprLeaf` and apply
//!   to both `SemanticExpr` and `PhysicalExpr` per spec `35 §6.3`.

use crate::expr::accessor::{DimensionAccessor, KeyAccessor, MeasureAccessor, MetricAccessor};
use crate::expr::leaves::{PhysicalExpr, PhysicalLeaf, SemanticExpr, SemanticLeaf};
use crate::expr::tree::Expr;
use crate::expr_kinds::{BinaryOpKind, ColumnRef, Literal, SemanticsName, UnaryOpKind};
use crate::tree::ExprLeaf;
use std::ops::{Add, BitAnd, BitOr, Div, Mul, Neg, Not, Rem, Sub};

// ── IntoLiteral — primitive → Literal coercion ──────────────────────────

/// Trait converting Rust primitives into [`Literal`]. Used by [`lit`].
/// Implemented for `bool`, `i64`, `i32`, `f64`, `&str`, `String`, and
/// `Literal` itself (identity).
pub trait IntoLiteral {
    /// Convert this value into a [`Literal`].
    fn into_literal(self) -> Literal;
}

impl IntoLiteral for bool {
    fn into_literal(self) -> Literal {
        Literal::Boolean(self)
    }
}

impl IntoLiteral for i64 {
    fn into_literal(self) -> Literal {
        Literal::Integer(self)
    }
}

impl IntoLiteral for i32 {
    fn into_literal(self) -> Literal {
        Literal::Integer(self as i64)
    }
}

impl IntoLiteral for f64 {
    fn into_literal(self) -> Literal {
        Literal::Float(self)
    }
}

impl IntoLiteral for &str {
    fn into_literal(self) -> Literal {
        Literal::String(self.to_owned())
    }
}

impl IntoLiteral for String {
    fn into_literal(self) -> Literal {
        Literal::String(self)
    }
}

impl IntoLiteral for Literal {
    fn into_literal(self) -> Literal {
        self
    }
}

// ── Six canonical semantic-site constructors per `14 §6.4.1` ────────────

/// `col("amount")` — physical column reference at a semantic site.
/// Legal only under `semantic_mapping: auto` per `14 §8`.
pub fn col(name: impl Into<String>) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Column(ColumnRef(name.into())))
}

/// `field("revenue")` — untyped semantic reference. Kind resolved at
/// compile by registry lookup per `19 §3`.
pub fn field(name: impl Into<String>) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Field(SemanticsName(name.into())))
}

/// `dim("region")` — typed Dimension reference.
pub fn dim(name: impl Into<String>) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Dimension {
        name: SemanticsName(name.into()),
        accessor: None,
    })
}

/// `measure("revenue")` — typed Measure reference.
pub fn measure(name: impl Into<String>) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Measure {
        name: SemanticsName(name.into()),
        accessor: None,
    })
}

/// `metric("conv_rate")` — typed Metric reference.
pub fn metric(name: impl Into<String>) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Metric {
        name: SemanticsName(name.into()),
        accessor: None,
    })
}

/// `key("order_id")` — typed Key reference.
pub fn key(name: impl Into<String>) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Key {
        name: SemanticsName(name.into()),
        accessor: None,
    })
}

/// `lit(value)` — typed literal at a semantic site. Accepts any value
/// implementing [`IntoLiteral`].
pub fn lit(value: impl IntoLiteral) -> SemanticExpr {
    Expr::Leaf(SemanticLeaf::Literal(value.into_literal()))
}

// ── Physical-side helpers ───────────────────────────────────────────────

/// Physical column constructor for `PhysicalExpr` sites. Provided as a
/// distinct helper to avoid leaking the dual-target dispatch from
/// `35 §6.1` into v1 — physical-mapping authors that prefer the free-fn
/// shape use this; everyone else writes
/// `Expr::Leaf(PhysicalLeaf::Column(...))` directly.
pub fn physical_col(name: impl Into<String>) -> PhysicalExpr {
    Expr::Leaf(PhysicalLeaf::Column(ColumnRef(name.into())))
}

// ── std::ops impls ──────────────────────────────────────────────────────
//
// Per spec `14 §6.2` / `35 §6.2`. Arithmetic / logical / unary operators
// produce structural `Expr<L>::BinaryOp` / `UnaryOp` values. `BitAnd` /
// `BitOr` carry SQL `AND` / `OR` (Rust does not allow overloading
// `&&` / `||`).

impl<L: ExprLeaf> Add for Expr<L> {
    type Output = Expr<L>;
    fn add(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::Add,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> Sub for Expr<L> {
    type Output = Expr<L>;
    fn sub(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::Subtract,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> Mul for Expr<L> {
    type Output = Expr<L>;
    fn mul(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::Multiply,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> Div for Expr<L> {
    type Output = Expr<L>;
    fn div(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::Divide,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> Rem for Expr<L> {
    type Output = Expr<L>;
    fn rem(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::Mod,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> BitAnd for Expr<L> {
    type Output = Expr<L>;
    fn bitand(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::And,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> BitOr for Expr<L> {
    type Output = Expr<L>;
    fn bitor(self, rhs: Self) -> Self::Output {
        Expr::BinaryOp {
            op: BinaryOpKind::Or,
            left: Box::new(self),
            right: Box::new(rhs),
        }
    }
}

impl<L: ExprLeaf> Neg for Expr<L> {
    type Output = Expr<L>;
    fn neg(self) -> Self::Output {
        Expr::UnaryOp {
            op: UnaryOpKind::Negate,
            operand: Box::new(self),
        }
    }
}

impl<L: ExprLeaf> Not for Expr<L> {
    type Output = Expr<L>;
    fn not(self) -> Self::Output {
        Expr::UnaryOp {
            op: UnaryOpKind::Not,
            operand: Box::new(self),
        }
    }
}

// ── ExprFunctionExt — comparison / predicate / accessor builders ────────

/// Builder-style sugar on `Expr<L>` for operations that `std::ops`
/// cannot directly model. Per spec `14 §9.2` / `35 §6.3`.
///
/// The comparison and predicate methods are generic and apply to any
/// `Expr<L>` (both `SemanticExpr` and `PhysicalExpr`). The accessor
/// builders on [`SemanticExprAccessorExt`] are best-effort and pattern-match
/// on the inner `SemanticLeaf` root.
pub trait ExprFunctionExt: Sized {
    /// `self == other` — produces `Expr::BinaryOp { op: Eq, .. }`.
    fn eq(self, other: Self) -> Self;
    /// `self != other`.
    fn neq(self, other: Self) -> Self;
    /// `self < other`.
    fn lt(self, other: Self) -> Self;
    /// `self <= other`.
    fn lt_eq(self, other: Self) -> Self;
    /// `self > other`.
    fn gt(self, other: Self) -> Self;
    /// `self >= other`.
    fn gt_eq(self, other: Self) -> Self;
    /// `self IS NULL`.
    #[allow(clippy::wrong_self_convention)] // DSL consume-and-wrap matches the rest of this trait.
    fn is_null(self) -> Self;
}

impl<L: ExprLeaf> ExprFunctionExt for Expr<L> {
    fn eq(self, other: Self) -> Self {
        Expr::BinaryOp {
            op: BinaryOpKind::Eq,
            left: Box::new(self),
            right: Box::new(other),
        }
    }
    fn neq(self, other: Self) -> Self {
        Expr::BinaryOp {
            op: BinaryOpKind::NotEq,
            left: Box::new(self),
            right: Box::new(other),
        }
    }
    fn lt(self, other: Self) -> Self {
        Expr::BinaryOp {
            op: BinaryOpKind::Lt,
            left: Box::new(self),
            right: Box::new(other),
        }
    }
    fn lt_eq(self, other: Self) -> Self {
        Expr::BinaryOp {
            op: BinaryOpKind::LtEq,
            left: Box::new(self),
            right: Box::new(other),
        }
    }
    fn gt(self, other: Self) -> Self {
        Expr::BinaryOp {
            op: BinaryOpKind::Gt,
            left: Box::new(self),
            right: Box::new(other),
        }
    }
    fn gt_eq(self, other: Self) -> Self {
        Expr::BinaryOp {
            op: BinaryOpKind::GtEq,
            left: Box::new(self),
            right: Box::new(other),
        }
    }
    fn is_null(self) -> Self {
        Expr::IsNull(Box::new(self))
    }
}

// ── SemanticExpr accessor builders (best-effort) ────────────────────────

/// Best-effort accessor-builder sugar on [`SemanticExpr`]. Per spec
/// `35 §6.3`. The methods pattern-match on the inner [`SemanticLeaf`] and
/// fill the matching accessor slot when the root is the right kind;
/// mismatched roots return the expression unchanged with a `debug_assert`.
///
/// Authors that need rigorous compile-time kind enforcement should
/// construct typed leaves directly via the `Dimension { .. }` literal
/// shape; this trait is the ergonomic layer.
pub trait SemanticExprAccessorExt: Sized {
    /// `dim(name).first()` / `key(name).first()`. No-op when the root is
    /// not a `Dimension` or `Key` leaf.
    fn first(self) -> Self;
    /// `dim(name).last()` / `key(name).last()`. No-op when the root is
    /// not a `Dimension` or `Key` leaf.
    fn last(self) -> Self;
    /// `measure(name).previous()` / `metric(name).previous()`. No-op when
    /// the root is not a `Measure` or `Metric` leaf.
    fn previous(self) -> Self;
    /// `measure(name).next()` / `metric(name).next()`. No-op when the root
    /// is not a `Measure` or `Metric` leaf.
    fn next(self) -> Self;
    /// `measure(name).delta()` / `metric(name).delta()`. No-op when the
    /// root is not a `Measure` or `Metric` leaf.
    fn delta(self) -> Self;
    /// `measure(name).percent_change()` / `metric(name).percent_change()`.
    /// No-op when the root is not a `Measure` or `Metric` leaf.
    fn percent_change(self) -> Self;
    /// `dim/measure/metric/key(name).lag(n)`. No-op when the root is not a
    /// typed semantic leaf.
    fn lag(self, n: u32) -> Self;
    /// `dim/measure/metric/key(name).lead(n)`. No-op when the root is not
    /// a typed semantic leaf.
    fn lead(self, n: u32) -> Self;
}

impl SemanticExprAccessorExt for SemanticExpr {
    fn first(self) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Dimension { name, .. }) => Expr::Leaf(SemanticLeaf::Dimension {
                name,
                accessor: Some(DimensionAccessor::First),
            }),
            Expr::Leaf(SemanticLeaf::Key { name, .. }) => Expr::Leaf(SemanticLeaf::Key {
                name,
                accessor: Some(KeyAccessor::First),
            }),
            other => {
                debug_assert!(false, "first() called on non-Dimension / non-Key root");
                other
            }
        }
    }

    fn last(self) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Dimension { name, .. }) => Expr::Leaf(SemanticLeaf::Dimension {
                name,
                accessor: Some(DimensionAccessor::Last),
            }),
            Expr::Leaf(SemanticLeaf::Key { name, .. }) => Expr::Leaf(SemanticLeaf::Key {
                name,
                accessor: Some(KeyAccessor::Last),
            }),
            other => {
                debug_assert!(false, "last() called on non-Dimension / non-Key root");
                other
            }
        }
    }

    fn previous(self) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Measure { name, .. }) => Expr::Leaf(SemanticLeaf::Measure {
                name,
                accessor: Some(MeasureAccessor::Previous),
            }),
            Expr::Leaf(SemanticLeaf::Metric { name, .. }) => Expr::Leaf(SemanticLeaf::Metric {
                name,
                accessor: Some(MetricAccessor::Previous),
            }),
            other => {
                debug_assert!(false, "previous() called on non-Measure / non-Metric root");
                other
            }
        }
    }

    fn next(self) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Measure { name, .. }) => Expr::Leaf(SemanticLeaf::Measure {
                name,
                accessor: Some(MeasureAccessor::Next),
            }),
            Expr::Leaf(SemanticLeaf::Metric { name, .. }) => Expr::Leaf(SemanticLeaf::Metric {
                name,
                accessor: Some(MetricAccessor::Next),
            }),
            other => {
                debug_assert!(false, "next() called on non-Measure / non-Metric root");
                other
            }
        }
    }

    fn delta(self) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Measure { name, .. }) => Expr::Leaf(SemanticLeaf::Measure {
                name,
                accessor: Some(MeasureAccessor::Delta),
            }),
            Expr::Leaf(SemanticLeaf::Metric { name, .. }) => Expr::Leaf(SemanticLeaf::Metric {
                name,
                accessor: Some(MetricAccessor::Delta),
            }),
            other => {
                debug_assert!(false, "delta() called on non-Measure / non-Metric root");
                other
            }
        }
    }

    fn percent_change(self) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Measure { name, .. }) => Expr::Leaf(SemanticLeaf::Measure {
                name,
                accessor: Some(MeasureAccessor::PercentChange),
            }),
            Expr::Leaf(SemanticLeaf::Metric { name, .. }) => Expr::Leaf(SemanticLeaf::Metric {
                name,
                accessor: Some(MetricAccessor::PercentChange),
            }),
            other => {
                debug_assert!(
                    false,
                    "percent_change() called on non-Measure / non-Metric root"
                );
                other
            }
        }
    }

    fn lag(self, n: u32) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Dimension { name, .. }) => Expr::Leaf(SemanticLeaf::Dimension {
                name,
                accessor: Some(DimensionAccessor::Lag(n)),
            }),
            Expr::Leaf(SemanticLeaf::Measure { name, .. }) => Expr::Leaf(SemanticLeaf::Measure {
                name,
                accessor: Some(MeasureAccessor::Lag(n)),
            }),
            Expr::Leaf(SemanticLeaf::Metric { name, .. }) => Expr::Leaf(SemanticLeaf::Metric {
                name,
                accessor: Some(MetricAccessor::Lag(n)),
            }),
            Expr::Leaf(SemanticLeaf::Key { name, .. }) => Expr::Leaf(SemanticLeaf::Key {
                name,
                accessor: Some(KeyAccessor::Lag(n)),
            }),
            other => {
                debug_assert!(false, "lag() called on non-typed-semantic-leaf root");
                other
            }
        }
    }

    fn lead(self, n: u32) -> Self {
        match self {
            Expr::Leaf(SemanticLeaf::Dimension { name, .. }) => Expr::Leaf(SemanticLeaf::Dimension {
                name,
                accessor: Some(DimensionAccessor::Lead(n)),
            }),
            Expr::Leaf(SemanticLeaf::Measure { name, .. }) => Expr::Leaf(SemanticLeaf::Measure {
                name,
                accessor: Some(MeasureAccessor::Lead(n)),
            }),
            Expr::Leaf(SemanticLeaf::Metric { name, .. }) => Expr::Leaf(SemanticLeaf::Metric {
                name,
                accessor: Some(MetricAccessor::Lead(n)),
            }),
            Expr::Leaf(SemanticLeaf::Key { name, .. }) => Expr::Leaf(SemanticLeaf::Key {
                name,
                accessor: Some(KeyAccessor::Lead(n)),
            }),
            other => {
                debug_assert!(false, "lead() called on non-typed-semantic-leaf root");
                other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Six canonical constructors produce the right leaf shape ─────────

    #[test]
    fn col_produces_semantic_column() {
        let e = col("x");
        match e {
            Expr::Leaf(SemanticLeaf::Column(c)) => assert_eq!(c.0, "x"),
            other => panic!("expected SemanticLeaf::Column, got {:?}", other),
        }
    }

    #[test]
    fn field_produces_field_leaf() {
        let e = field("revenue");
        match e {
            Expr::Leaf(SemanticLeaf::Field(s)) => assert_eq!(s.0, "revenue"),
            other => panic!("expected SemanticLeaf::Field, got {:?}", other),
        }
    }

    #[test]
    fn dim_produces_dimension_leaf_with_no_accessor() {
        let e = dim("region");
        match e {
            Expr::Leaf(SemanticLeaf::Dimension { name, accessor }) => {
                assert_eq!(name.0, "region");
                assert!(accessor.is_none());
            }
            other => panic!("expected SemanticLeaf::Dimension, got {:?}", other),
        }
    }

    #[test]
    fn measure_produces_measure_leaf_with_no_accessor() {
        let e = measure("revenue");
        match e {
            Expr::Leaf(SemanticLeaf::Measure { name, accessor }) => {
                assert_eq!(name.0, "revenue");
                assert!(accessor.is_none());
            }
            other => panic!("expected SemanticLeaf::Measure, got {:?}", other),
        }
    }

    #[test]
    fn metric_produces_metric_leaf_with_no_accessor() {
        let e = metric("conv_rate");
        match e {
            Expr::Leaf(SemanticLeaf::Metric { name, accessor }) => {
                assert_eq!(name.0, "conv_rate");
                assert!(accessor.is_none());
            }
            other => panic!("expected SemanticLeaf::Metric, got {:?}", other),
        }
    }

    #[test]
    fn key_produces_key_leaf_with_no_accessor() {
        let e = key("order_id");
        match e {
            Expr::Leaf(SemanticLeaf::Key { name, accessor }) => {
                assert_eq!(name.0, "order_id");
                assert!(accessor.is_none());
            }
            other => panic!("expected SemanticLeaf::Key, got {:?}", other),
        }
    }

    #[test]
    fn physical_col_produces_physical_column_leaf() {
        let e = physical_col("amount");
        match e {
            Expr::Leaf(PhysicalLeaf::Column(c)) => assert_eq!(c.0, "amount"),
            other => panic!("expected PhysicalLeaf::Column, got {:?}", other),
        }
    }

    // ── lit() coercions ─────────────────────────────────────────────────

    #[test]
    fn lit_integer_produces_integer_literal() {
        match lit(42i64) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(n))) => assert_eq!(n, 42),
            other => panic!("expected Literal::Integer, got {:?}", other),
        }
    }

    #[test]
    fn lit_i32_widens_to_integer() {
        match lit(7i32) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(n))) => assert_eq!(n, 7),
            other => panic!("expected Literal::Integer, got {:?}", other),
        }
    }

    #[test]
    fn lit_f64_produces_float_literal() {
        match lit(2.5f64) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Float(f))) => assert_eq!(f, 2.5),
            other => panic!("expected Literal::Float, got {:?}", other),
        }
    }

    #[test]
    fn lit_str_produces_string_literal() {
        match lit("hello") {
            Expr::Leaf(SemanticLeaf::Literal(Literal::String(s))) => assert_eq!(s, "hello"),
            other => panic!("expected Literal::String, got {:?}", other),
        }
    }

    #[test]
    fn lit_string_produces_string_literal() {
        match lit("hi".to_string()) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::String(s))) => assert_eq!(s, "hi"),
            other => panic!("expected Literal::String, got {:?}", other),
        }
    }

    #[test]
    fn lit_bool_produces_boolean_literal() {
        match lit(true) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Boolean(b))) => assert!(b),
            other => panic!("expected Literal::Boolean, got {:?}", other),
        }
    }

    #[test]
    fn lit_passes_through_existing_literal() {
        match lit(Literal::Integer(99)) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(n))) => assert_eq!(n, 99),
            other => panic!("expected Literal::Integer pass-through, got {:?}", other),
        }
    }

    // ── std::ops impls ──────────────────────────────────────────────────

    #[test]
    fn add_produces_binary_op_add() {
        let e = dim("a") + lit(1i64);
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Add,
                ..
            } => {}
            other => panic!("expected BinaryOp::Add, got {:?}", other),
        }
    }

    #[test]
    fn sub_produces_binary_op_subtract() {
        let e = measure("revenue") - measure("cost");
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Subtract,
                ..
            } => {}
            other => panic!("expected BinaryOp::Subtract, got {:?}", other),
        }
    }

    #[test]
    fn mul_produces_binary_op_multiply() {
        let e = lit(2i64) * lit(3i64);
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Multiply,
                ..
            } => {}
            other => panic!("expected BinaryOp::Multiply, got {:?}", other),
        }
    }

    #[test]
    fn div_produces_binary_op_divide() {
        let e = lit(6i64) / lit(2i64);
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Divide,
                ..
            } => {}
            other => panic!("expected BinaryOp::Divide, got {:?}", other),
        }
    }

    #[test]
    fn rem_produces_binary_op_mod() {
        let e = lit(7i64) % lit(2i64);
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Mod,
                ..
            } => {}
            other => panic!("expected BinaryOp::Mod, got {:?}", other),
        }
    }

    #[test]
    fn bitand_produces_binary_op_and() {
        let e = field("a") & field("b");
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::And,
                ..
            } => {}
            other => panic!("expected BinaryOp::And, got {:?}", other),
        }
    }

    #[test]
    fn bitor_produces_binary_op_or() {
        let e = field("a") | field("b");
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Or,
                ..
            } => {}
            other => panic!("expected BinaryOp::Or, got {:?}", other),
        }
    }

    #[test]
    fn neg_produces_unary_op_negate() {
        let e = -lit(1i64);
        match e {
            Expr::UnaryOp {
                op: UnaryOpKind::Negate,
                ..
            } => {}
            other => panic!("expected UnaryOp::Negate, got {:?}", other),
        }
    }

    #[test]
    fn not_produces_unary_op_not() {
        let e = !lit(true);
        match e {
            Expr::UnaryOp {
                op: UnaryOpKind::Not,
                ..
            } => {}
            other => panic!("expected UnaryOp::Not, got {:?}", other),
        }
    }

    // ── ExprFunctionExt comparison + is_null ────────────────────────────

    #[test]
    fn eq_produces_binary_op_eq() {
        let e = field("x").eq(lit(1i64));
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Eq,
                ..
            } => {}
            other => panic!("expected BinaryOp::Eq, got {:?}", other),
        }
    }

    #[test]
    fn neq_produces_binary_op_not_eq() {
        let e = field("x").neq(lit(1i64));
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::NotEq,
                ..
            } => {}
            other => panic!("expected BinaryOp::NotEq, got {:?}", other),
        }
    }

    #[test]
    fn lt_lt_eq_gt_gt_eq_produce_correct_ops() {
        for (e, expected) in [
            (field("x").lt(lit(1i64)), BinaryOpKind::Lt),
            (field("x").lt_eq(lit(1i64)), BinaryOpKind::LtEq),
            (field("x").gt(lit(1i64)), BinaryOpKind::Gt),
            (field("x").gt_eq(lit(1i64)), BinaryOpKind::GtEq),
        ] {
            match e {
                Expr::BinaryOp { op, .. } => assert_eq!(op, expected),
                other => panic!("expected BinaryOp, got {:?}", other),
            }
        }
    }

    #[test]
    fn is_null_produces_is_null_node() {
        let e = field("x").is_null();
        match e {
            Expr::IsNull(_) => {}
            other => panic!("expected IsNull, got {:?}", other),
        }
    }

    #[test]
    fn is_null_works_on_physical_expr_too() {
        let e: PhysicalExpr = physical_col("amount").is_null();
        match e {
            Expr::IsNull(_) => {}
            other => panic!("expected IsNull on PhysicalExpr, got {:?}", other),
        }
    }

    // ── SemanticExprAccessorExt sugar ───────────────────────────────────

    #[test]
    fn dim_first_attaches_dimension_accessor_first() {
        let e = dim("order_date").first();
        match e {
            Expr::Leaf(SemanticLeaf::Dimension {
                accessor: Some(DimensionAccessor::First),
                ..
            }) => {}
            other => panic!("expected Dimension::First, got {:?}", other),
        }
    }

    #[test]
    fn dim_lag_attaches_dimension_accessor_lag() {
        let e = dim("order_date").lag(2);
        match e {
            Expr::Leaf(SemanticLeaf::Dimension {
                accessor: Some(DimensionAccessor::Lag(2)),
                ..
            }) => {}
            other => panic!("expected Dimension::Lag(2), got {:?}", other),
        }
    }

    #[test]
    fn measure_previous_attaches_measure_accessor_previous() {
        let e = measure("revenue").previous();
        match e {
            Expr::Leaf(SemanticLeaf::Measure {
                accessor: Some(MeasureAccessor::Previous),
                ..
            }) => {}
            other => panic!("expected Measure::Previous, got {:?}", other),
        }
    }

    #[test]
    fn measure_delta_and_percent_change() {
        match measure("revenue").delta() {
            Expr::Leaf(SemanticLeaf::Measure {
                accessor: Some(MeasureAccessor::Delta),
                ..
            }) => {}
            other => panic!("expected Measure::Delta, got {:?}", other),
        }
        match measure("revenue").percent_change() {
            Expr::Leaf(SemanticLeaf::Measure {
                accessor: Some(MeasureAccessor::PercentChange),
                ..
            }) => {}
            other => panic!("expected Measure::PercentChange, got {:?}", other),
        }
    }

    #[test]
    fn metric_previous_attaches_metric_accessor_previous() {
        let e = metric("conv_rate").previous();
        match e {
            Expr::Leaf(SemanticLeaf::Metric {
                accessor: Some(MetricAccessor::Previous),
                ..
            }) => {}
            other => panic!("expected Metric::Previous, got {:?}", other),
        }
    }

    #[test]
    fn key_first_attaches_key_accessor_first() {
        let e = key("order_id").first();
        match e {
            Expr::Leaf(SemanticLeaf::Key {
                accessor: Some(KeyAccessor::First),
                ..
            }) => {}
            other => panic!("expected Key::First, got {:?}", other),
        }
    }

    #[test]
    fn key_lead_attaches_key_accessor_lead() {
        let e = key("order_id").lead(3);
        match e {
            Expr::Leaf(SemanticLeaf::Key {
                accessor: Some(KeyAccessor::Lead(3)),
                ..
            }) => {}
            other => panic!("expected Key::Lead(3), got {:?}", other),
        }
    }

    // ── Composition: arithmetic + comparison + literal mix ──────────────

    #[test]
    fn dim_a_plus_lit_one_compares_lt_lit_ten() {
        let e = (dim("a") + lit(1i64)).lt(lit(10i64));
        match e {
            Expr::BinaryOp {
                op: BinaryOpKind::Lt,
                ..
            } => {}
            other => panic!("expected BinaryOp::Lt, got {:?}", other),
        }
    }
}
