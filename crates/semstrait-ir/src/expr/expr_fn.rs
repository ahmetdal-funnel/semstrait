//! Authoring-surface DSL per spec `14 §6.4.1` / `35 §6`.
//!
//! Six canonical semantic constructors (`col`, `field`, `dim`, `measure`,
//! `metric`, `key`), `lit` literal coercion, `physical_col`, `std::ops`
//! impls on `Expr<L>`, and the [`ExprFunctionExt`] trait for comparison /
//! predicate sugar (`eq`, `neq`, `lt`, `lt_eq`, `gt`, `gt_eq`, `is_null`)
//! generic over `L: ExprLeaf`. Authors that need accessors construct typed
//! leaves directly via the `Dimension { accessor: Some(..) }` literal
//! shape per `14 §4.1`.

use crate::expr::leaves::{PhysicalExpr, PhysicalLeaf, SemanticExpr, SemanticLeaf};
use crate::expr::tree::Expr;
use crate::expr_kinds::{
    BinaryOpKind, ColumnRef, FloatWidth, IntegerWidth, Literal, SemanticsName, UnaryOpKind,
};
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
        Literal::Integer {
            value: self,
            width: IntegerWidth::Long,
        }
    }
}

impl IntoLiteral for i32 {
    fn into_literal(self) -> Literal {
        Literal::Integer {
            value: self as i64,
            width: IntegerWidth::Int,
        }
    }
}

impl IntoLiteral for f64 {
    fn into_literal(self) -> Literal {
        Literal::Float {
            value: self,
            width: FloatWidth::Double,
        }
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

// ── ExprFunctionExt — comparison / predicate sugar ──────────────────────

/// Builder-style sugar on `Expr<L>` for comparison / predicate operations
/// that `std::ops` cannot directly model. Per spec `14 §9.2` / `35 §6.3`.
/// Generic over `L: ExprLeaf` — applies to both `SemanticExpr` and
/// `PhysicalExpr`.
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
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer { value, width })) => {
                assert_eq!(value, 42);
                assert_eq!(width, IntegerWidth::Long);
            }
            other => panic!("expected Literal::Integer, got {:?}", other),
        }
    }

    #[test]
    fn lit_i32_widens_to_integer() {
        match lit(7i32) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer { value, width })) => {
                assert_eq!(value, 7);
                assert_eq!(width, IntegerWidth::Int);
            }
            other => panic!("expected Literal::Integer, got {:?}", other),
        }
    }

    #[test]
    fn lit_f64_produces_float_literal() {
        match lit(2.5f64) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Float { value, width })) => {
                assert_eq!(value, 2.5);
                assert_eq!(width, FloatWidth::Double);
            }
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
        let input = Literal::Integer {
            value: 99,
            width: IntegerWidth::Long,
        };
        match lit(input) {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer { value, width })) => {
                assert_eq!(value, 99);
                assert_eq!(width, IntegerWidth::Long);
            }
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
