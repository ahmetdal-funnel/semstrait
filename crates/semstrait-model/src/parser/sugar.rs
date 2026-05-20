//! Closed sugar-tag rosters (Phase 8 Pass A).
//!
//! Exposes three lookup helpers — one each for binary operators, unary
//! operators, and registry-backed function calls. Every author-visible
//! tag is in exactly one table; unknown tags surface as
//! [`ParseError::UnknownTag`] at the dispatcher in
//! [`super::block::parse_tagged_mapping`].
//!
//! Roster ratified Phase 8 (`STATUS.md` item Q follow-up):
//!
//! - **BinaryOp** — `add`, `subtract`, `multiply`, `divide`,
//!   `safe_divide`, `mod`, `eq`, `not_eq`, `lt`, `lt_eq`, `gt`,
//!   `gt_eq`, `and`, `or`. Body is `[a, b]` or `{ left, right }`.
//! - **UnaryOp** — `negate`. (`not:` is handled via the negation-fold
//!   in [`super::block`], not as a plain unary.)
//! - **FunctionCall** — full legacy parity (`Option A`):
//!   string / math / temporal / pattern / conditional functions
//!   ratified in `[14a §4](../../../../docs/design/foundations/14a_function_catalog.md)`.
//!
//! No `function_call:` / `binary_op:` / `unary_op:` author-facing
//! escape hatch — tag = name; unknown = error.

use semstrait_ir::{BinaryOpKind, CanonicalFn, UnaryOpKind};

/// Map a sugar tag (e.g. `"add"`, `"eq"`) to its [`BinaryOpKind`] —
/// `None` if the tag is not in the binary roster.
pub fn binary_op_for_tag(tag: &str) -> Option<BinaryOpKind> {
    Some(match tag {
        "add" => BinaryOpKind::Add,
        "subtract" => BinaryOpKind::Subtract,
        "multiply" => BinaryOpKind::Multiply,
        "divide" => BinaryOpKind::Divide,
        "safe_divide" => BinaryOpKind::SafeDivide,
        "mod" => BinaryOpKind::Mod,
        "eq" => BinaryOpKind::Eq,
        "not_eq" => BinaryOpKind::NotEq,
        "lt" => BinaryOpKind::Lt,
        "lt_eq" => BinaryOpKind::LtEq,
        "gt" => BinaryOpKind::Gt,
        "gt_eq" => BinaryOpKind::GtEq,
        "and" => BinaryOpKind::And,
        "or" => BinaryOpKind::Or,
        _ => return None,
    })
}

/// Map a sugar tag to its [`UnaryOpKind`] — `None` if not in the unary
/// roster. Note: `not:` is intentionally excluded; the dispatcher
/// handles it via the negation-fold so `not: { in: ... }` flips
/// `InList::negated` instead of wrapping in `UnaryOp::Not`.
pub fn unary_op_for_tag(tag: &str) -> Option<UnaryOpKind> {
    Some(match tag {
        "negate" => UnaryOpKind::Negate,
        _ => return None,
    })
}

/// Function-call-sugar table entry — author tag plus the canonical
/// function name we emit into [`semstrait_ir::Expr::FunctionCall`].
pub struct FunctionSpec {
    pub tag: &'static str,
    pub canonical: &'static str,
}

/// Closed function-call sugar roster (Option A — full legacy parity,
/// Phase 8 ratification). One row per author-visible function tag.
/// `mod` is intentionally absent — it is a [`BinaryOpKind`] and
/// resolves through [`binary_op_for_tag`].
pub const FUNCTION_TABLE: &[FunctionSpec] = &[
    // String
    FunctionSpec { tag: "upper", canonical: "UPPER" },
    FunctionSpec { tag: "lower", canonical: "LOWER" },
    FunctionSpec { tag: "length", canonical: "LENGTH" },
    FunctionSpec { tag: "substring", canonical: "SUBSTRING" },
    FunctionSpec { tag: "trim", canonical: "TRIM" },
    FunctionSpec { tag: "ltrim", canonical: "LTRIM" },
    FunctionSpec { tag: "rtrim", canonical: "RTRIM" },
    FunctionSpec { tag: "concat", canonical: "CONCAT" },
    FunctionSpec { tag: "concat_ws", canonical: "CONCAT_WS" },
    FunctionSpec { tag: "replace", canonical: "REPLACE" },
    FunctionSpec { tag: "lpad", canonical: "LPAD" },
    FunctionSpec { tag: "rpad", canonical: "RPAD" },
    FunctionSpec { tag: "reverse", canonical: "REVERSE" },
    FunctionSpec { tag: "left", canonical: "LEFT" },
    FunctionSpec { tag: "right", canonical: "RIGHT" },
    FunctionSpec { tag: "position", canonical: "POSITION" },
    FunctionSpec { tag: "split_part", canonical: "SPLIT_PART" },
    FunctionSpec { tag: "starts_with", canonical: "STARTS_WITH" },
    FunctionSpec { tag: "ends_with", canonical: "ENDS_WITH" },
    FunctionSpec { tag: "initcap", canonical: "INITCAP" },
    FunctionSpec { tag: "repeat", canonical: "REPEAT" },
    // Math
    FunctionSpec { tag: "abs", canonical: "ABS" },
    FunctionSpec { tag: "round", canonical: "ROUND" },
    FunctionSpec { tag: "ceil", canonical: "CEIL" },
    FunctionSpec { tag: "floor", canonical: "FLOOR" },
    FunctionSpec { tag: "sqrt", canonical: "SQRT" },
    FunctionSpec { tag: "power", canonical: "POWER" },
    FunctionSpec { tag: "sign", canonical: "SIGN" },
    FunctionSpec { tag: "exp", canonical: "EXP" },
    FunctionSpec { tag: "ln", canonical: "LN" },
    FunctionSpec { tag: "log", canonical: "LOG" },
    FunctionSpec { tag: "log10", canonical: "LOG10" },
    // Temporal
    FunctionSpec { tag: "date_add", canonical: "DATE_ADD" },
    FunctionSpec { tag: "date_sub", canonical: "DATE_SUB" },
    FunctionSpec { tag: "date_diff", canonical: "DATE_DIFF" },
    FunctionSpec { tag: "date_trunc", canonical: "DATE_TRUNC" },
    FunctionSpec { tag: "extract", canonical: "EXTRACT" },
    FunctionSpec { tag: "current_date", canonical: "CURRENT_DATE" },
    FunctionSpec { tag: "current_timestamp", canonical: "CURRENT_TIMESTAMP" },
    FunctionSpec { tag: "to_date", canonical: "TO_DATE" },
    FunctionSpec { tag: "to_timestamp", canonical: "TO_TIMESTAMP" },
    // Pattern
    FunctionSpec { tag: "regexp_match", canonical: "REGEXP_MATCH" },
    FunctionSpec { tag: "regexp_extract", canonical: "REGEXP_EXTRACT" },
    FunctionSpec { tag: "regexp_replace", canonical: "REGEXP_REPLACE" },
    // Conditional
    FunctionSpec { tag: "greatest", canonical: "GREATEST" },
    FunctionSpec { tag: "least", canonical: "LEAST" },
];

/// Lookup helper for [`FUNCTION_TABLE`].
pub fn function_for_tag(tag: &str) -> Option<CanonicalFn> {
    FUNCTION_TABLE
        .iter()
        .find(|spec| spec.tag == tag)
        .map(|spec| CanonicalFn(spec.canonical.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn binary_op_roster_is_closed() {
        assert_eq!(binary_op_for_tag("add"), Some(BinaryOpKind::Add));
        assert_eq!(binary_op_for_tag("eq"), Some(BinaryOpKind::Eq));
        assert_eq!(binary_op_for_tag("lt_eq"), Some(BinaryOpKind::LtEq));
        assert_eq!(binary_op_for_tag("mod"), Some(BinaryOpKind::Mod));
        assert_eq!(binary_op_for_tag("safe_divide"), Some(BinaryOpKind::SafeDivide));
        // Unknown / mistyped → None
        assert_eq!(binary_op_for_tag("plus"), None);
        assert_eq!(binary_op_for_tag("equals"), None);
        assert_eq!(binary_op_for_tag(""), None);
    }

    #[test]
    fn unary_op_roster_excludes_not() {
        assert_eq!(unary_op_for_tag("negate"), Some(UnaryOpKind::Negate));
        // `not:` is the dispatcher's responsibility (negation-fold).
        assert_eq!(unary_op_for_tag("not"), None);
    }

    #[test]
    fn function_table_has_no_duplicates() {
        let tags: HashSet<&str> = FUNCTION_TABLE.iter().map(|s| s.tag).collect();
        assert_eq!(tags.len(), FUNCTION_TABLE.len(), "duplicate tags");
        let canonicals: HashSet<&str> =
            FUNCTION_TABLE.iter().map(|s| s.canonical).collect();
        assert_eq!(
            canonicals.len(),
            FUNCTION_TABLE.len(),
            "duplicate canonical names"
        );
    }

    #[test]
    fn function_lookup_round_trip() {
        let cf = function_for_tag("upper").expect("upper present");
        assert_eq!(cf.0, "UPPER");
        let cf = function_for_tag("regexp_match").expect("regexp_match present");
        assert_eq!(cf.0, "REGEXP_MATCH");
        assert!(function_for_tag("not_a_function").is_none());
    }

    #[test]
    fn function_table_disjoint_with_binary_ops() {
        // `mod` is in BinaryOp; must NOT also be a function.
        assert!(function_for_tag("mod").is_none());
        for spec in FUNCTION_TABLE {
            assert!(
                binary_op_for_tag(spec.tag).is_none(),
                "function tag `{}` collides with binary-op tag",
                spec.tag
            );
        }
    }
}
