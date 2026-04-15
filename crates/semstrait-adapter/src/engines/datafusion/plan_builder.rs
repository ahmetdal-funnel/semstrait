//! DataFusion-specific plan builder with expression rewriting.
//!
//! Handles function name remaps and structural rewrites for DataFusion's
//! Substrait consumer. See `docs/FUNCTION_CATALOG.md` for the complete mapping.

use std::collections::HashMap;

use semstrait_core::expr::Expr;
use semstrait_ir::rewrite::{CanonicalFn, FunctionRewriter, FunctionTarget};
use semstrait_ir::PlanBuilder;

/// DataFusion plan builder with engine-specific expression rewriting.
///
/// Applies two layers of rewriting in [`rewrite_expr`]:
/// 1. `FunctionCall` nodes via [`FunctionRewriter`] (data-driven HashMap lookup)
/// 2. Dedicated `Expr` variants via pattern matching (typed, structural)
pub struct DataFusionPlanBuilder {
    rewriter: FunctionRewriter,
}

impl DataFusionPlanBuilder {
    pub fn new() -> Self {
        let mut map = HashMap::new();

        // ── Name-remaps ────────────────────────────────────────────────
        map.insert(CanonicalFn::Position, FunctionTarget::Rename("strpos"));

        // ── Structural rewrites ────────────────────────────────────────
        map.insert(CanonicalFn::Mod, FunctionTarget::Rewrite(rewrite_mod));
        map.insert(CanonicalFn::DateAdd, FunctionTarget::Rewrite(rewrite_date_add));
        map.insert(CanonicalFn::DateDiff, FunctionTarget::Rewrite(rewrite_date_diff));

        // All other canonical functions: implicit SameName (passthrough)

        Self {
            rewriter: FunctionRewriter::new(map),
        }
    }
}

impl PlanBuilder for DataFusionPlanBuilder {
    fn rewrite_expr(&self, expr: Expr) -> Expr {
        expr.transform(&|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
            match e {
                // Layer 1: FunctionCall nodes — data-driven rewriter
                Expr::FunctionCall(fc) => Ok(self.rewriter.rewrite_function_call(fc)),

                // Layer 2: Dedicated Expr variants — pattern matching
                //
                // RegexpMatch → regexp_like(expr, pattern)
                //
                // DataFusion's regexp_match() returns List<Utf8>, not Boolean.
                // For boolean predicate semantics, use regexp_like() which returns Boolean.
                // For full_match, anchor the pattern with ^ and $.
                Expr::RegexpMatch(re) => {
                    let pattern = if re.full_match {
                        Expr::function_call("concat", vec![
                            Expr::string("^"),
                            (*re.pattern).clone(),
                            Expr::string("$"),
                        ])
                    } else {
                        (*re.pattern).clone()
                    };
                    Ok(Some(Expr::function_call(
                        "regexp_like",
                        vec![(*re.expr).clone(), pattern],
                    )))
                }

                // RegexpExtract → array_element(regexp_match(expr, pattern), group_idx)
                //
                // DataFusion's regexp_match returns List<Utf8> of capture groups only
                // (full match / group 0 is excluded when capture groups exist).
                // array_element is 1-based, and so is canonical group_idx (1 = first
                // capture group), so group_idx maps directly — no offset needed.
                Expr::RegexpExtract(re) => {
                    let regexp_match = Expr::function_call(
                        "regexp_match",
                        vec![(*re.expr).clone(), (*re.pattern).clone()],
                    );
                    let index = Expr::int(re.group_idx as i64);
                    Ok(Some(Expr::function_call("array_element", vec![regexp_match, index])))
                }

                // All other variants pass through unchanged
                _ => Ok(None),
            }
        })
        .expect("rewrite_expr is infallible")
    }
}

// ── Structural rewrite fn pointers ─────────────────────────────────────

/// `mod(a, b)` → `a - floor(a / b) * b`
///
/// DataFusion supports `%` operator in SQL but not a `mod()` function
/// in Substrait. Decompose into arithmetic.
fn rewrite_mod(args: &[Expr]) -> Expr {
    if args.len() != 2 {
        return passthrough("mod", args);
    }
    let a = args[0].clone();
    let b = args[1].clone();
    // a - floor(a / b) * b
    Expr::subtract(
        a.clone(),
        Expr::multiply(
            Expr::function_call("floor", vec![Expr::divide(a, b.clone())]),
            b,
        ),
    )
}

/// `date_add(date_expr, interval_expr)` → `date_expr + interval_expr`
///
/// DataFusion uses interval arithmetic instead of a `date_add()` function.
fn rewrite_date_add(args: &[Expr]) -> Expr {
    if args.len() != 2 {
        return passthrough("date_add", args);
    }
    Expr::add(args[0].clone(), args[1].clone())
}

/// `date_diff(date1, date2)` → `date2 - date1`
///
/// DataFusion uses date subtraction instead of a `date_diff()` function.
fn rewrite_date_diff(args: &[Expr]) -> Expr {
    if args.len() < 2 {
        return passthrough("date_diff", args);
    }
    Expr::subtract(args[1].clone(), args[0].clone())
}

/// Reconstruct original FunctionCall unchanged (arity mismatch fallback).
fn passthrough(name: &str, args: &[Expr]) -> Expr {
    Expr::function_call(name, args.to_vec())
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::expr::{RegexpExpr, RegexpExtractExpr};

    #[test]
    fn position_renamed_to_strpos() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::function_call("position", vec![Expr::column("name"), Expr::string("x")]);
        let result = builder.rewrite_expr(expr);

        if let Expr::FunctionCall(fc) = &result {
            assert_eq!(fc.name, "strpos");
            assert_eq!(fc.args.len(), 2);
        } else {
            panic!("expected FunctionCall, got {:?}", result);
        }
    }

    #[test]
    fn regexp_match_to_regexp_like_substring() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::RegexpMatch(RegexpExpr {
            expr: Box::new(Expr::column("campaign")),
            pattern: Box::new(Expr::string("^UK_")),
            full_match: false,
        });
        let result = builder.rewrite_expr(expr);

        // Should become: regexp_like(campaign, '^UK_')
        if let Expr::FunctionCall(fc) = &result {
            assert_eq!(fc.name, "regexp_like");
            assert_eq!(fc.args.len(), 2);
            assert_eq!(fc.args[0], Expr::column("campaign"));
            assert_eq!(fc.args[1], Expr::string("^UK_"));
        } else {
            panic!("expected FunctionCall(regexp_like), got {:?}", result);
        }
    }

    #[test]
    fn regexp_match_to_regexp_like_full_match() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::RegexpMatch(RegexpExpr {
            expr: Box::new(Expr::column("email")),
            pattern: Box::new(Expr::string(".*@example\\.com")),
            full_match: true,
        });
        let result = builder.rewrite_expr(expr);

        // Should become: regexp_like(email, concat('^', '.*@example\.com', '$'))
        if let Expr::FunctionCall(fc) = &result {
            assert_eq!(fc.name, "regexp_like");
            assert_eq!(fc.args.len(), 2);
            assert_eq!(fc.args[0], Expr::column("email"));

            // Second arg: concat('^', pattern, '$')
            if let Expr::FunctionCall(concat_fc) = &fc.args[1] {
                assert_eq!(concat_fc.name, "concat");
                assert_eq!(concat_fc.args.len(), 3);
                assert_eq!(concat_fc.args[0], Expr::string("^"));
                assert_eq!(concat_fc.args[1], Expr::string(".*@example\\.com"));
                assert_eq!(concat_fc.args[2], Expr::string("$"));
            } else {
                panic!("expected FunctionCall(concat) for anchored pattern, got {:?}", fc.args[1]);
            }
        } else {
            panic!("expected FunctionCall(regexp_like), got {:?}", result);
        }
    }

    #[test]
    fn regexp_extract_to_array_element_regexp_match() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::RegexpExtract(RegexpExtractExpr {
            expr: Box::new(Expr::column("text")),
            pattern: Box::new(Expr::string(r"(\d+)")),
            group_idx: 1,
        });
        let result = builder.rewrite_expr(expr);

        // Outer: array_element(regexp_match(...), 1)
        if let Expr::FunctionCall(fc) = &result {
            assert_eq!(fc.name, "array_element");
            assert_eq!(fc.args.len(), 2);

            // First arg: regexp_match(expr, pattern) — 2 args, no group_idx
            if let Expr::FunctionCall(inner) = &fc.args[0] {
                assert_eq!(inner.name, "regexp_match");
                assert_eq!(inner.args.len(), 2);
            } else {
                panic!("expected inner FunctionCall(regexp_match), got {:?}", fc.args[0]);
            }

            // Second arg: Expr::int(1) — group_idx maps directly (DF array excludes full match)
            assert_eq!(fc.args[1], Expr::int(1));
        } else {
            panic!("expected FunctionCall(array_element), got {:?}", result);
        }
    }

    #[test]
    fn date_add_to_binary_add() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::function_call(
            "date_add",
            vec![Expr::column("created_at"), Expr::string("1 day")],
        );
        let result = builder.rewrite_expr(expr);
        assert!(matches!(result, Expr::BinaryOp(_)), "expected BinaryOp(Add)");
    }

    #[test]
    fn date_diff_to_binary_subtract() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::function_call(
            "date_diff",
            vec![Expr::column("start"), Expr::column("end")],
        );
        let result = builder.rewrite_expr(expr);
        assert!(matches!(result, Expr::BinaryOp(_)), "expected BinaryOp(Subtract)");
    }

    #[test]
    fn mod_to_arithmetic() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::function_call("mod", vec![Expr::int(10), Expr::int(3)]);
        let result = builder.rewrite_expr(expr);
        // mod(a,b) → a - floor(a/b) * b → BinaryOp(Subtract, ...)
        assert!(matches!(result, Expr::BinaryOp(_)), "expected BinaryOp");
    }

    #[test]
    fn unknown_function_passthrough() {
        let builder = DataFusionPlanBuilder::new();
        let expr = Expr::function_call("my_custom_udf", vec![Expr::int(1)]);
        let result = builder.rewrite_expr(expr);

        if let Expr::FunctionCall(fc) = &result {
            assert_eq!(fc.name, "my_custom_udf");
        } else {
            panic!("expected passthrough");
        }
    }

    #[test]
    fn same_name_functions_passthrough() {
        let builder = DataFusionPlanBuilder::new();
        for name in &["upper", "lower", "concat", "abs", "ceil", "floor", "round"] {
            let expr = Expr::function_call(*name, vec![Expr::column("x")]);
            let result = builder.rewrite_expr(expr);
            if let Expr::FunctionCall(fc) = &result {
                assert_eq!(fc.name, *name, "SameName function should be unchanged");
            } else {
                panic!("expected FunctionCall for {}", name);
            }
        }
    }

    #[test]
    fn nested_rewrite_bottom_up() {
        let builder = DataFusionPlanBuilder::new();
        // position(upper(x), "a") — both upper (passthrough) and position (rename) should work
        let inner = Expr::function_call("upper", vec![Expr::column("x")]);
        let outer = Expr::function_call("position", vec![inner, Expr::string("a")]);
        let result = builder.rewrite_expr(outer);

        if let Expr::FunctionCall(fc) = &result {
            assert_eq!(fc.name, "strpos");
            // Inner upper should still be upper (passthrough)
            if let Expr::FunctionCall(inner_fc) = &fc.args[0] {
                assert_eq!(inner_fc.name, "upper");
            }
        } else {
            panic!("expected FunctionCall");
        }
    }

    #[test]
    fn send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DataFusionPlanBuilder>();
    }
}
