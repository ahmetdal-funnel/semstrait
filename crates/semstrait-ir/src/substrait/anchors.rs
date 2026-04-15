//! Shared Substrait function anchor constants and literal builders.
//!
//! Both `ExprConverter` and `SubstraitSerializer` reference the same set of
//! function anchors. Keeping them in one place avoids drift.

use crate::rewrite::CanonicalFn;
use substrait::proto::{
    self,
    expression::{self, literal::LiteralType},
    r#type::{Kind, Nullability},
};

// ── FunctionRegistry ────────────────────────────────────────────────────────

/// An entry in the function registry: anchor + engine-specific name.
#[derive(Debug, Clone)]
pub struct FunctionEntry {
    pub anchor: u32,
    pub name: String,
}

/// Registry of function anchor → name mappings.
///
/// Adapters provide engine-specific registries. The serializer uses the
/// registry to emit `SimpleExtensionDeclaration` entries in the Substrait plan.
#[derive(Debug, Clone)]
pub struct FunctionRegistry {
    entries: Vec<FunctionEntry>,
}

impl FunctionRegistry {
    pub fn new(entries: Vec<FunctionEntry>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &[FunctionEntry] {
        &self.entries
    }

    /// DataFusion-compatible function name mappings.
    ///
    /// Names match what DataFusion's Substrait consumer resolves:
    /// `name_to_op`, `BuiltinExprBuilder`, and registered UDFs.
    pub fn datafusion() -> Self {
        Self::new(vec![
            // Aggregates
            FunctionEntry { anchor: FUNC_SUM, name: "sum".into() },
            FunctionEntry { anchor: FUNC_AVG, name: "avg".into() },
            FunctionEntry { anchor: FUNC_COUNT, name: "count".into() },
            FunctionEntry { anchor: FUNC_COUNT_DISTINCT, name: "count".into() },
            FunctionEntry { anchor: FUNC_MIN, name: "min".into() },
            FunctionEntry { anchor: FUNC_MAX, name: "max".into() },
            // Comparison
            FunctionEntry { anchor: FUNC_EQUAL, name: "equal".into() },
            FunctionEntry { anchor: FUNC_NOT_EQUAL, name: "not_equal".into() },
            FunctionEntry { anchor: FUNC_LT, name: "lt".into() },
            FunctionEntry { anchor: FUNC_LTE, name: "lte".into() },
            FunctionEntry { anchor: FUNC_GT, name: "gt".into() },
            FunctionEntry { anchor: FUNC_GTE, name: "gte".into() },
            // Boolean / misc
            FunctionEntry { anchor: FUNC_AND, name: "and".into() },
            FunctionEntry { anchor: FUNC_OR, name: "or".into() },
            FunctionEntry { anchor: FUNC_NOT, name: "not".into() },
            FunctionEntry { anchor: FUNC_IS_NULL, name: "is_null".into() },
            FunctionEntry { anchor: FUNC_IS_NOT_NULL, name: "is_not_null".into() },
            // IN emitted as native SingularOrList, not a scalar function
            FunctionEntry { anchor: FUNC_BETWEEN, name: "between".into() },
            FunctionEntry { anchor: FUNC_LIKE, name: "like".into() },
            FunctionEntry { anchor: FUNC_COALESCE, name: "coalesce".into() },
            FunctionEntry { anchor: FUNC_NULLIF, name: "nullif".into() },
            FunctionEntry { anchor: FUNC_DATE_TRUNC, name: "date_trunc".into() },
            // String
            FunctionEntry { anchor: FUNC_ILIKE, name: "ilike".into() },
            FunctionEntry { anchor: FUNC_REGEXP_MATCH, name: "regexp_match".into() },
            // regexp_extract: DataFusion has no such UDF; omitted from registry
            // Arithmetic
            FunctionEntry { anchor: FUNC_ADD, name: "add".into() },
            FunctionEntry { anchor: FUNC_SUBTRACT, name: "subtract".into() },
            FunctionEntry { anchor: FUNC_MULTIPLY, name: "multiply".into() },
            FunctionEntry { anchor: FUNC_DIVIDE, name: "divide".into() },
            // ── Canonical functions (CanonicalFn::anchor() is source of truth) ──
            // String
            FunctionEntry { anchor: CanonicalFn::Upper.anchor(), name: "upper".into() },
            FunctionEntry { anchor: CanonicalFn::Lower.anchor(), name: "lower".into() },
            FunctionEntry { anchor: CanonicalFn::Concat.anchor(), name: "concat".into() },
            FunctionEntry { anchor: CanonicalFn::ConcatWs.anchor(), name: "concat_ws".into() },
            FunctionEntry { anchor: CanonicalFn::Length.anchor(), name: "length".into() },
            FunctionEntry { anchor: CanonicalFn::Trim.anchor(), name: "trim".into() },
            FunctionEntry { anchor: CanonicalFn::Ltrim.anchor(), name: "ltrim".into() },
            FunctionEntry { anchor: CanonicalFn::Rtrim.anchor(), name: "rtrim".into() },
            FunctionEntry { anchor: CanonicalFn::Replace.anchor(), name: "replace".into() },
            FunctionEntry { anchor: CanonicalFn::Substring.anchor(), name: "substring".into() },
            FunctionEntry { anchor: CanonicalFn::Left.anchor(), name: "left".into() },
            FunctionEntry { anchor: CanonicalFn::Right.anchor(), name: "right".into() },
            FunctionEntry { anchor: CanonicalFn::Lpad.anchor(), name: "lpad".into() },
            FunctionEntry { anchor: CanonicalFn::Rpad.anchor(), name: "rpad".into() },
            FunctionEntry { anchor: CanonicalFn::SplitPart.anchor(), name: "split_part".into() },
            FunctionEntry { anchor: CanonicalFn::StartsWith.anchor(), name: "starts_with".into() },
            FunctionEntry { anchor: CanonicalFn::EndsWith.anchor(), name: "ends_with".into() },
            FunctionEntry { anchor: CanonicalFn::Initcap.anchor(), name: "initcap".into() },
            FunctionEntry { anchor: CanonicalFn::Reverse.anchor(), name: "reverse".into() },
            FunctionEntry { anchor: CanonicalFn::Repeat.anchor(), name: "repeat".into() },
            FunctionEntry { anchor: CanonicalFn::Position.anchor(), name: "strpos".into() },
            // Math
            FunctionEntry { anchor: CanonicalFn::Abs.anchor(), name: "abs".into() },
            FunctionEntry { anchor: CanonicalFn::Ceil.anchor(), name: "ceil".into() },
            FunctionEntry { anchor: CanonicalFn::Floor.anchor(), name: "floor".into() },
            FunctionEntry { anchor: CanonicalFn::Round.anchor(), name: "round".into() },
            FunctionEntry { anchor: CanonicalFn::Power.anchor(), name: "power".into() },
            FunctionEntry { anchor: CanonicalFn::Sqrt.anchor(), name: "sqrt".into() },
            // Note: mod, date_add, date_diff are structurally rewritten by
            // DataFusionPlanBuilder::rewrite_expr() into arithmetic operations
            // and never appear as FunctionCall nodes in the serialized plan.
            // Date/Time
            FunctionEntry { anchor: CanonicalFn::DatePart.anchor(), name: "date_part".into() },
            FunctionEntry { anchor: CanonicalFn::CurrentDate.anchor(), name: "current_date".into() },
            FunctionEntry { anchor: CanonicalFn::CurrentTimestamp.anchor(), name: "current_timestamp".into() },
            FunctionEntry { anchor: CanonicalFn::ToDate.anchor(), name: "to_date".into() },
            FunctionEntry { anchor: CanonicalFn::ToTimestamp.anchor(), name: "to_timestamp".into() },
            // Conditional
            FunctionEntry { anchor: CanonicalFn::Greatest.anchor(), name: "greatest".into() },
            FunctionEntry { anchor: CanonicalFn::Least.anchor(), name: "least".into() },
            // Pattern
            FunctionEntry { anchor: CanonicalFn::RegexpReplace.anchor(), name: "regexp_replace".into() },
        ])
    }
}

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
pub const FUNC_ILIKE: u32 = 211;
pub const FUNC_REGEXP_MATCH: u32 = 212;
pub const FUNC_REGEXP_EXTRACT: u32 = 213;
pub const FUNC_CAST: u32 = 214;

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
