//! Shared Substrait function anchor constants and literal builders.
//!
//! Both `ExprConverter` and `SubstraitSerializer` reference the same set of
//! function anchors. Keeping them in one place avoids drift.

use substrait::proto::{
    self,
    expression::{self, literal::LiteralType},
    r#type::{Kind, Nullability},
};

// ── Extension URI anchors ───────────────────────────────────────────────────

pub const URI_AGGREGATE: u32 = 1;
pub const URI_COMPARISON: u32 = 2;
pub const URI_BOOLEAN: u32 = 3;
pub const URI_ARITHMETIC: u32 = 4;

// ── Aggregate function anchors ──────────────────────────────────────────────

pub const FUNC_SUM: u32 = 1;
pub const FUNC_AVG: u32 = 2;
pub const FUNC_COUNT: u32 = 3;
pub const FUNC_COUNT_DISTINCT: u32 = 4;
pub const FUNC_MIN: u32 = 5;
pub const FUNC_MAX: u32 = 6;

// ── Comparison function anchors ─────────────────────────────────────────────

pub const FUNC_EQUAL: u32 = 100;
pub const FUNC_NOT_EQUAL: u32 = 101;
pub const FUNC_LT: u32 = 102;
pub const FUNC_LTE: u32 = 103;
pub const FUNC_GT: u32 = 104;
pub const FUNC_GTE: u32 = 105;

// ── Boolean / misc function anchors ─────────────────────────────────────────

pub const FUNC_AND: u32 = 200;
pub const FUNC_OR: u32 = 201;
pub const FUNC_IS_NULL: u32 = 202;
pub const FUNC_IS_NOT_NULL: u32 = 203;
pub const FUNC_COALESCE: u32 = 204;
pub const FUNC_NOT: u32 = 205;
pub const FUNC_IN: u32 = 206;
pub const FUNC_BETWEEN: u32 = 207;
pub const FUNC_LIKE: u32 = 208;
pub const FUNC_NULLIF: u32 = 209;
pub const FUNC_DATE_TRUNC: u32 = 210;

// ── Arithmetic function anchors ─────────────────────────────────────────────

pub const FUNC_ADD: u32 = 300;
pub const FUNC_SUBTRACT: u32 = 301;
pub const FUNC_MULTIPLY: u32 = 302;
pub const FUNC_DIVIDE: u32 = 303;

// ── Literal builders ────────────────────────────────────────────────────────
//
// Pure construction helpers — no schema context needed.

pub fn literal_i64(value: i64) -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(LiteralType::I64(value)),
        })),
    }
}

pub fn literal_f64(value: f64) -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(LiteralType::Fp64(value)),
        })),
    }
}

pub fn literal_string(value: &str) -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(LiteralType::String(value.to_string())),
        })),
    }
}

pub fn literal_bool(value: bool) -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(LiteralType::Boolean(value)),
        })),
    }
}

pub fn literal_null() -> proto::Expression {
    proto::Expression {
        rex_type: Some(expression::RexType::Literal(proto::expression::Literal {
            nullable: true,
            type_variation_reference: 0,
            literal_type: Some(LiteralType::Null(proto::Type {
                kind: Some(Kind::Bool(proto::r#type::Boolean {
                    type_variation_reference: 0,
                    nullability: Nullability::Nullable as i32,
                })),
            })),
        })),
    }
}
