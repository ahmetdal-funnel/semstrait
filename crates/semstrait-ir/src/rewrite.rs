//! Expression rewriting types for engine-specific function mapping.
//!
//! Engines that differ from the canonical function set declare their mappings
//! via [`FunctionRewriter`], which is used by [`PlanBuilder::rewrite_expr()`]
//! during plan construction.
//!
//! Only `FunctionCall` nodes are handled here. Dedicated `Expr` variants
//! (Like, ILike, RegexpMatch, RegexpExtract, DateTrunc, Cast, etc.) are
//! handled by pattern matching in the engine's `PlanBuilder::rewrite_expr()`.

use semstrait_core::expr::{Expr, FunctionCallExpr};
use std::collections::HashMap;

// ── CanonicalFn ────────────────────────────────────────────────────────────

/// Identifies a canonical function for data-driven rewriting lookup.
///
/// Each variant corresponds to a `FunctionCall` name in the canonical IR.
/// Dedicated `Expr` variants are NOT represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalFn {
    // ── String (21) ────────────────────────────────────────────────────
    Upper,
    Lower,
    Concat,
    ConcatWs,
    Length,
    Trim,
    Ltrim,
    Rtrim,
    Replace,
    Substring,
    Left,
    Right,
    Lpad,
    Rpad,
    SplitPart,
    StartsWith,
    EndsWith,
    Initcap,
    Reverse,
    Repeat,
    Position,

    // ── Math (7) ───────────────────────────────────────────────────────
    Abs,
    Ceil,
    Floor,
    Round,
    Power,
    Sqrt,
    Mod,

    // ── Date/Time (7) ──────────────────────────────────────────────────
    DatePart,
    CurrentDate,
    CurrentTimestamp,
    DateAdd,
    DateDiff,
    ToDate,
    ToTimestamp,

    // ── Conditional (2) ────────────────────────────────────────────────
    Greatest,
    Least,

    // ── Pattern (1) ────────────────────────────────────────────────────
    RegexpReplace,
}

impl CanonicalFn {
    /// Resolve a function name to a canonical function (case-insensitive).
    ///
    /// Returns `None` for unrecognized names — they pass through unchanged.
    /// Handles common aliases (substr/substring, pow/power, etc.).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            // String
            "upper" | "ucase" => Some(Self::Upper),
            "lower" | "lcase" => Some(Self::Lower),
            "concat" => Some(Self::Concat),
            "concat_ws" => Some(Self::ConcatWs),
            "length" | "char_length" | "len" => Some(Self::Length),
            "trim" => Some(Self::Trim),
            "ltrim" => Some(Self::Ltrim),
            "rtrim" => Some(Self::Rtrim),
            "replace" => Some(Self::Replace),
            "substring" | "substr" => Some(Self::Substring),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "lpad" => Some(Self::Lpad),
            "rpad" => Some(Self::Rpad),
            "split_part" => Some(Self::SplitPart),
            "starts_with" | "startswith" | "prefix" => Some(Self::StartsWith),
            "ends_with" | "endswith" | "suffix" => Some(Self::EndsWith),
            "initcap" => Some(Self::Initcap),
            "reverse" => Some(Self::Reverse),
            "repeat" => Some(Self::Repeat),
            "position" | "strpos" | "locate" => Some(Self::Position),
            // Math
            "abs" => Some(Self::Abs),
            "ceil" | "ceiling" => Some(Self::Ceil),
            "floor" => Some(Self::Floor),
            "round" => Some(Self::Round),
            "power" | "pow" => Some(Self::Power),
            "sqrt" => Some(Self::Sqrt),
            "mod" | "pmod" => Some(Self::Mod),
            // Date/Time
            "date_part" | "extract" => Some(Self::DatePart),
            "current_date" | "curdate" => Some(Self::CurrentDate),
            "current_timestamp" | "now" => Some(Self::CurrentTimestamp),
            "date_add" | "dateadd" => Some(Self::DateAdd),
            "date_diff" | "datediff" => Some(Self::DateDiff),
            "to_date" => Some(Self::ToDate),
            "to_timestamp" => Some(Self::ToTimestamp),
            // Conditional
            "greatest" => Some(Self::Greatest),
            "least" => Some(Self::Least),
            // Pattern
            "regexp_replace" => Some(Self::RegexpReplace),
            _ => None,
        }
    }

    /// Canonical lowercase name for this function.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Concat => "concat",
            Self::ConcatWs => "concat_ws",
            Self::Length => "length",
            Self::Trim => "trim",
            Self::Ltrim => "ltrim",
            Self::Rtrim => "rtrim",
            Self::Replace => "replace",
            Self::Substring => "substring",
            Self::Left => "left",
            Self::Right => "right",
            Self::Lpad => "lpad",
            Self::Rpad => "rpad",
            Self::SplitPart => "split_part",
            Self::StartsWith => "starts_with",
            Self::EndsWith => "ends_with",
            Self::Initcap => "initcap",
            Self::Reverse => "reverse",
            Self::Repeat => "repeat",
            Self::Position => "position",
            Self::Abs => "abs",
            Self::Ceil => "ceil",
            Self::Floor => "floor",
            Self::Round => "round",
            Self::Power => "power",
            Self::Sqrt => "sqrt",
            Self::Mod => "mod",
            Self::DatePart => "date_part",
            Self::CurrentDate => "current_date",
            Self::CurrentTimestamp => "current_timestamp",
            Self::DateAdd => "date_add",
            Self::DateDiff => "date_diff",
            Self::ToDate => "to_date",
            Self::ToTimestamp => "to_timestamp",
            Self::Greatest => "greatest",
            Self::Least => "least",
            Self::RegexpReplace => "regexp_replace",
        }
    }

    /// Substrait function anchor for this canonical function.
    ///
    /// Single source of truth for anchor values. Ranges:
    /// - String: 400–420
    /// - Date/Time: 500–507
    /// - Math: 600–606
    /// - Conditional/Pattern: 700–702
    pub fn anchor(&self) -> u32 {
        match self {
            // String 400–420
            Self::Upper => 400,
            Self::Lower => 401,
            Self::Concat => 402,
            Self::ConcatWs => 403,
            Self::Length => 404,
            Self::Trim => 405,
            Self::Ltrim => 406,
            Self::Rtrim => 407,
            Self::Replace => 408,
            Self::Substring => 409,
            Self::Left => 410,
            Self::Right => 411,
            Self::Lpad => 412,
            Self::Rpad => 413,
            Self::SplitPart => 414,
            Self::StartsWith => 415,
            Self::EndsWith => 416,
            Self::Initcap => 417,
            Self::Reverse => 418,
            Self::Repeat => 419,
            Self::Position => 420,
            // Date/Time 500–507
            Self::DatePart => 500,
            Self::CurrentDate => 501,
            Self::CurrentTimestamp => 502,
            Self::DateAdd => 503,
            Self::DateDiff => 504,
            Self::ToDate => 505,
            Self::ToTimestamp => 506,
            // Math 600–606
            Self::Abs => 600,
            Self::Ceil => 601,
            Self::Floor => 602,
            Self::Round => 603,
            Self::Power => 604,
            Self::Sqrt => 605,
            Self::Mod => 606,
            // Conditional/Pattern 700–702
            Self::Greatest => 700,
            Self::Least => 701,
            Self::RegexpReplace => 702,
        }
    }

    /// All canonical function variants (for iteration/testing).
    pub fn all() -> &'static [CanonicalFn] {
        &[
            Self::Upper, Self::Lower, Self::Concat, Self::ConcatWs, Self::Length,
            Self::Trim, Self::Ltrim, Self::Rtrim, Self::Replace, Self::Substring,
            Self::Left, Self::Right, Self::Lpad, Self::Rpad, Self::SplitPart,
            Self::StartsWith, Self::EndsWith, Self::Initcap, Self::Reverse,
            Self::Repeat, Self::Position,
            Self::Abs, Self::Ceil, Self::Floor, Self::Round, Self::Power,
            Self::Sqrt, Self::Mod,
            Self::DatePart, Self::CurrentDate, Self::CurrentTimestamp,
            Self::DateAdd, Self::DateDiff, Self::ToDate, Self::ToTimestamp,
            Self::Greatest, Self::Least,
            Self::RegexpReplace,
        ]
    }
}

// ── FunctionTarget ─────────────────────────────────────────────────────────

/// What to do with a canonical function during expression rewriting.
#[derive(Clone)]
pub enum FunctionTarget {
    /// Keep the same function name (passthrough).
    SameName,

    /// Rename to a different function name, preserving args, arity, and distinct flag.
    Rename(&'static str),

    /// Structural rewrite: replace the entire `FunctionCall` node with a new `Expr` tree.
    ///
    /// The fn pointer receives the original args (borrowed from the already-rewritten
    /// children via `Expr::transform()`). Uses a fn pointer for `Copy + Send + Sync`.
    Rewrite(fn(&[Expr]) -> Expr),

    /// Engine does not support this function.
    /// V1: passthrough (infallible `rewrite_expr`). Future: error or warning.
    Unsupported,
}

impl std::fmt::Debug for FunctionTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SameName => write!(f, "SameName"),
            Self::Rename(name) => write!(f, "Rename({name:?})"),
            Self::Rewrite(_) => write!(f, "Rewrite(fn)"),
            Self::Unsupported => write!(f, "Unsupported"),
        }
    }
}

// ── FunctionRewriter ───────────────────────────────────────────────────────

/// Data-driven function rewriter for `FunctionCall` nodes.
///
/// Maps [`CanonicalFn`] to [`FunctionTarget`]. Used by engine-specific
/// [`PlanBuilder::rewrite_expr()`] implementations.
///
/// Functions not in the map pass through unchanged (implicit `SameName`).
pub struct FunctionRewriter {
    map: HashMap<CanonicalFn, FunctionTarget>,
}

impl FunctionRewriter {
    pub fn new(map: HashMap<CanonicalFn, FunctionTarget>) -> Self {
        Self { map }
    }

    /// Attempt to rewrite a `FunctionCall` expression.
    ///
    /// Returns `Some(replacement)` if the function was rewritten, or `None`
    /// if it should be kept unchanged. This fits `Expr::transform()`'s
    /// `Ok(None)` = keep convention.
    pub fn rewrite_function_call(&self, fc: &FunctionCallExpr) -> Option<Expr> {
        let canonical = CanonicalFn::from_name(&fc.name)?;
        let target = self.map.get(&canonical)?;
        match target {
            FunctionTarget::SameName => None,
            FunctionTarget::Rename(new_name) => Some(Expr::FunctionCall(FunctionCallExpr {
                name: (*new_name).to_string(),
                args: fc.args.clone(),
                distinct: fc.distinct,
            })),
            FunctionTarget::Rewrite(f) => Some(f(&fc.args)),
            FunctionTarget::Unsupported => None,
        }
    }
}

impl std::fmt::Debug for FunctionRewriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionRewriter")
            .field("entries", &self.map.len())
            .finish()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_fn_from_name_round_trip() {
        for cf in CanonicalFn::all() {
            let resolved = CanonicalFn::from_name(cf.canonical_name());
            assert_eq!(resolved, Some(*cf), "round-trip failed for {:?}", cf);
        }
    }

    #[test]
    fn canonical_fn_aliases() {
        assert_eq!(CanonicalFn::from_name("UPPER"), Some(CanonicalFn::Upper));
        assert_eq!(CanonicalFn::from_name("ucase"), Some(CanonicalFn::Upper));
        assert_eq!(CanonicalFn::from_name("substr"), Some(CanonicalFn::Substring));
        assert_eq!(CanonicalFn::from_name("pow"), Some(CanonicalFn::Power));
        assert_eq!(CanonicalFn::from_name("ceiling"), Some(CanonicalFn::Ceil));
        assert_eq!(CanonicalFn::from_name("now"), Some(CanonicalFn::CurrentTimestamp));
        assert_eq!(CanonicalFn::from_name("strpos"), Some(CanonicalFn::Position));
        assert_eq!(CanonicalFn::from_name("datediff"), Some(CanonicalFn::DateDiff));
    }

    #[test]
    fn canonical_fn_unknown_returns_none() {
        assert_eq!(CanonicalFn::from_name("unknown_func"), None);
        assert_eq!(CanonicalFn::from_name("my_udf"), None);
    }

    #[test]
    fn anchor_values_unique() {
        let mut seen = std::collections::HashSet::new();
        for cf in CanonicalFn::all() {
            assert!(seen.insert(cf.anchor()), "duplicate anchor for {:?}", cf);
        }
    }

    #[test]
    fn rewriter_rename() {
        let mut map = HashMap::new();
        map.insert(CanonicalFn::Position, FunctionTarget::Rename("strpos"));

        let rewriter = FunctionRewriter::new(map);
        let fc = FunctionCallExpr {
            name: "position".to_string(),
            args: vec![Expr::column("name"), Expr::string("x")],
            distinct: false,
        };

        let result = rewriter.rewrite_function_call(&fc);
        assert!(result.is_some());
        if let Some(Expr::FunctionCall(rewritten)) = result {
            assert_eq!(rewritten.name, "strpos");
            assert_eq!(rewritten.args.len(), 2);
            assert_eq!(rewritten.distinct, false);
        } else {
            panic!("expected FunctionCall");
        }
    }

    #[test]
    fn rewriter_structural() {
        fn double_first(args: &[Expr]) -> Expr {
            Expr::add(args[0].clone(), args[0].clone())
        }

        let mut map = HashMap::new();
        map.insert(CanonicalFn::Abs, FunctionTarget::Rewrite(double_first));

        let rewriter = FunctionRewriter::new(map);
        let fc = FunctionCallExpr {
            name: "abs".to_string(),
            args: vec![Expr::int(5)],
            distinct: false,
        };

        let result = rewriter.rewrite_function_call(&fc);
        assert!(result.is_some());
        assert!(matches!(result, Some(Expr::BinaryOp(_))));
    }

    #[test]
    fn rewriter_unmapped_passthrough() {
        let rewriter = FunctionRewriter::new(HashMap::new());
        let fc = FunctionCallExpr {
            name: "upper".to_string(),
            args: vec![Expr::column("name")],
            distinct: false,
        };
        assert!(rewriter.rewrite_function_call(&fc).is_none());
    }

    #[test]
    fn rewriter_unknown_function_passthrough() {
        let rewriter = FunctionRewriter::new(HashMap::new());
        let fc = FunctionCallExpr {
            name: "my_custom_udf".to_string(),
            args: vec![],
            distinct: false,
        };
        assert!(rewriter.rewrite_function_call(&fc).is_none());
    }

    #[test]
    fn rewriter_same_name_passthrough() {
        let mut map = HashMap::new();
        map.insert(CanonicalFn::Upper, FunctionTarget::SameName);

        let rewriter = FunctionRewriter::new(map);
        let fc = FunctionCallExpr {
            name: "upper".to_string(),
            args: vec![Expr::column("name")],
            distinct: false,
        };
        assert!(rewriter.rewrite_function_call(&fc).is_none());
    }

    #[test]
    fn rewriter_preserves_distinct_on_rename() {
        let mut map = HashMap::new();
        map.insert(CanonicalFn::Concat, FunctionTarget::Rename("string_concat"));

        let rewriter = FunctionRewriter::new(map);
        let fc = FunctionCallExpr {
            name: "concat".to_string(),
            args: vec![Expr::column("a"), Expr::column("b")],
            distinct: true,
        };

        if let Some(Expr::FunctionCall(rewritten)) = rewriter.rewrite_function_call(&fc) {
            assert_eq!(rewritten.name, "string_concat");
            assert!(rewritten.distinct, "distinct flag must be preserved");
        } else {
            panic!("expected renamed FunctionCall");
        }
    }

    #[test]
    fn send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FunctionRewriter>();
        assert_send_sync::<FunctionTarget>();
        assert_send_sync::<CanonicalFn>();
    }
}
