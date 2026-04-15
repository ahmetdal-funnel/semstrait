//! Expression constant-folding and substitution for SR-10 static pushdown.
//!
//! Two public functions:
//! - `substitute`: replace Column references with literal values for compile-time-known dimensions
//! - `simplify`: constant-fold an expression tree (literal comparisons, dead CASE branch pruning, etc.)

use semstrait_core::expr::{BinaryOp, Literal};
use semstrait_core::Expr;
use std::collections::HashMap;

/// Replace `Column(name)` with `Literal(String(value))` for all names in `known_values`.
///
/// Used to inject compile-time-known dimension values (metadata dims, literal mappings)
/// into computed expressions before simplification.
pub(crate) fn substitute(expr: &Expr, known_values: &HashMap<String, String>) -> Expr {
    expr.transform(&|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
        match e {
            Expr::Column(col) => {
                if let Some(value) = known_values.get(&col.name) {
                    Ok(Some(Expr::string(value.clone())))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    })
    .expect("substitution is infallible")
}

/// Constant-fold an expression tree.
///
/// Applies simplification rules bottom-up until fixed point (max 10 iterations).
/// Rules:
/// - Literal equality: `Lit("a") == Lit("a")` → `Bool(true)`
/// - Literal inequality: `Lit("a") == Lit("b")` → `Bool(false)`
/// - Literal IN list: all-literal → `Bool(true/false)`
/// - NOT Bool: `NOT true` → `Bool(false)`
/// - AND/OR short-circuit: `true AND x` → `x`, `false OR x` → `x`, etc.
/// - CASE true branch: first true condition → result
/// - CASE false prune: skip branches with false conditions
/// - CASE all-false + else: collapse to else_expr or NULL
pub(crate) fn simplify(expr: &Expr) -> Expr {
    let mut current = expr.clone();
    for _ in 0..10 {
        let next = simplify_once(&current);
        if next == current {
            break;
        }
        current = next;
    }
    current
}

/// Single bottom-up simplification pass.
fn simplify_once(expr: &Expr) -> Expr {
    expr.transform(&|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
        Ok(simplify_node(e))
    })
    .expect("simplification is infallible")
}

/// Try to simplify a single node. Returns `Some(simplified)` or `None` (keep as-is).
fn simplify_node(expr: &Expr) -> Option<Expr> {
    match expr {
        // ── Binary comparison of literals ────────────────────────────
        Expr::BinaryOp(bin) => simplify_binary(bin),

        // ── NOT on boolean literal ──────────────────────────────────
        Expr::Not(u) => match u.expr.as_ref() {
            Expr::Literal(Literal::Boolean { value }) => Some(Expr::boolean(!value)),
            _ => None,
        },

        // ── IN list with all literals ───────────────────────────────
        Expr::InList(il) => simplify_in_list(il),

        // ── CASE with evaluable conditions ──────────────────────────
        Expr::Case(case) => simplify_case(case),

        _ => None,
    }
}

/// Simplify binary operations on literals.
fn simplify_binary(bin: &semstrait_core::expr::BinaryExpr) -> Option<Expr> {
    match (&*bin.left, bin.op, &*bin.right) {
        // Literal == Literal
        (Expr::Literal(l), BinaryOp::Eq, Expr::Literal(r)) => Some(Expr::boolean(l == r)),

        // Literal != Literal
        (Expr::Literal(l), BinaryOp::NotEq, Expr::Literal(r)) => Some(Expr::boolean(l != r)),

        // AND short-circuit
        (Expr::Literal(Literal::Boolean { value: true }), BinaryOp::And, rhs) => {
            Some(rhs.clone())
        }
        (lhs, BinaryOp::And, Expr::Literal(Literal::Boolean { value: true })) => {
            Some(lhs.clone())
        }
        (Expr::Literal(Literal::Boolean { value: false }), BinaryOp::And, _) => {
            Some(Expr::boolean(false))
        }
        (_, BinaryOp::And, Expr::Literal(Literal::Boolean { value: false })) => {
            Some(Expr::boolean(false))
        }

        // OR short-circuit
        (Expr::Literal(Literal::Boolean { value: true }), BinaryOp::Or, _) => {
            Some(Expr::boolean(true))
        }
        (_, BinaryOp::Or, Expr::Literal(Literal::Boolean { value: true })) => {
            Some(Expr::boolean(true))
        }
        (Expr::Literal(Literal::Boolean { value: false }), BinaryOp::Or, rhs) => {
            Some(rhs.clone())
        }
        (lhs, BinaryOp::Or, Expr::Literal(Literal::Boolean { value: false })) => {
            Some(lhs.clone())
        }

        _ => None,
    }
}

/// Simplify IN list when expr and all list items are literals.
fn simplify_in_list(il: &semstrait_core::expr::InListExpr) -> Option<Expr> {
    // Only simplify when the tested expression is a literal.
    let needle = match il.expr.as_ref() {
        Expr::Literal(lit) => lit,
        _ => return None,
    };

    // All list items must be literals.
    let haystack: Vec<&Literal> = il
        .list
        .iter()
        .map(|e| match e {
            Expr::Literal(lit) => Some(lit),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;

    let found = haystack.contains(&needle);
    let result = if il.negated { !found } else { found };
    Some(Expr::boolean(result))
}

/// Simplify CASE expression by evaluating boolean conditions.
fn simplify_case(case: &semstrait_core::expr::CaseExpr) -> Option<Expr> {
    let mut remaining = Vec::new();
    let mut changed = false;

    for wc in &case.when_then {
        match &wc.condition {
            // True condition → this branch always fires.
            Expr::Literal(Literal::Boolean { value: true }) => {
                // Return the result directly (all prior branches were false/pruned).
                return Some(wc.result.clone());
            }
            // False condition → prune this branch entirely.
            Expr::Literal(Literal::Boolean { value: false }) => {
                changed = true;
                // Skip this branch.
            }
            // Non-literal condition → keep.
            _ => {
                remaining.push(wc.clone());
            }
        }
    }

    if !changed {
        return None;
    }

    // All branches pruned → collapse to else or NULL.
    if remaining.is_empty() {
        return Some(
            case.else_expr
                .as_ref()
                .map(|e| *e.clone())
                .unwrap_or_else(Expr::null),
        );
    }

    // Some branches remain.
    Some(Expr::case(
        remaining,
        case.else_expr.as_ref().map(|e| *e.clone()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::expr::WhenClause;

    // ── substitute tests ────────────────────────────────────────────

    #[test]
    fn test_substitute_replaces_known_column() {
        let expr = Expr::column("dataset_name");
        let known = HashMap::from([("dataset_name".to_string(), "adwords".to_string())]);
        let result = substitute(&expr, &known);
        assert_eq!(result, Expr::string("adwords"));
    }

    #[test]
    fn test_substitute_leaves_unknown_column() {
        let expr = Expr::column("campaign");
        let known = HashMap::from([("dataset_name".to_string(), "adwords".to_string())]);
        let result = substitute(&expr, &known);
        assert_eq!(result, Expr::column("campaign"));
    }

    #[test]
    fn test_substitute_nested() {
        let expr = Expr::eq(Expr::column("dataset_name"), Expr::string("klaviyo"));
        let known = HashMap::from([("dataset_name".to_string(), "adwords".to_string())]);
        let result = substitute(&expr, &known);
        assert_eq!(result, Expr::eq(Expr::string("adwords"), Expr::string("klaviyo")));
    }

    // ── literal equality ────────────────────────────────────────────

    #[test]
    fn test_simplify_literal_eq_true() {
        let expr = Expr::eq(Expr::string("a"), Expr::string("a"));
        assert_eq!(simplify(&expr), Expr::boolean(true));
    }

    #[test]
    fn test_simplify_literal_eq_false() {
        let expr = Expr::eq(Expr::string("a"), Expr::string("b"));
        assert_eq!(simplify(&expr), Expr::boolean(false));
    }

    #[test]
    fn test_simplify_literal_neq() {
        let expr = Expr::ne(Expr::string("a"), Expr::string("b"));
        assert_eq!(simplify(&expr), Expr::boolean(true));
    }

    // ── NOT ─────────────────────────────────────────────────────────

    #[test]
    fn test_simplify_not_true() {
        let expr = Expr::not(Expr::boolean(true));
        assert_eq!(simplify(&expr), Expr::boolean(false));
    }

    #[test]
    fn test_simplify_not_false() {
        let expr = Expr::not(Expr::boolean(false));
        assert_eq!(simplify(&expr), Expr::boolean(true));
    }

    // ── AND / OR short-circuit ──────────────────────────────────────

    #[test]
    fn test_simplify_and_true_x() {
        let expr = Expr::binary(Expr::boolean(true), BinaryOp::And, Expr::column("x"));
        assert_eq!(simplify(&expr), Expr::column("x"));
    }

    #[test]
    fn test_simplify_and_false_x() {
        let expr = Expr::binary(Expr::boolean(false), BinaryOp::And, Expr::column("x"));
        assert_eq!(simplify(&expr), Expr::boolean(false));
    }

    #[test]
    fn test_simplify_or_true_x() {
        let expr = Expr::binary(Expr::boolean(true), BinaryOp::Or, Expr::column("x"));
        assert_eq!(simplify(&expr), Expr::boolean(true));
    }

    #[test]
    fn test_simplify_or_false_x() {
        let expr = Expr::binary(Expr::boolean(false), BinaryOp::Or, Expr::column("x"));
        assert_eq!(simplify(&expr), Expr::column("x"));
    }

    // ── IN list ─────────────────────────────────────────────────────

    #[test]
    fn test_simplify_in_list_found() {
        let expr = Expr::in_list(
            Expr::string("a"),
            vec![Expr::string("a"), Expr::string("b")],
        );
        assert_eq!(simplify(&expr), Expr::boolean(true));
    }

    #[test]
    fn test_simplify_in_list_not_found() {
        let expr = Expr::in_list(
            Expr::string("c"),
            vec![Expr::string("a"), Expr::string("b")],
        );
        assert_eq!(simplify(&expr), Expr::boolean(false));
    }

    #[test]
    fn test_simplify_not_in_list() {
        let expr = Expr::not_in_list(
            Expr::string("c"),
            vec![Expr::string("a"), Expr::string("b")],
        );
        assert_eq!(simplify(&expr), Expr::boolean(true));
    }

    #[test]
    fn test_simplify_in_list_with_non_literal_kept() {
        // If any list item is not a literal, don't simplify.
        let expr = Expr::in_list(
            Expr::string("a"),
            vec![Expr::column("x"), Expr::string("b")],
        );
        assert_eq!(simplify(&expr), expr);
    }

    // ── CASE pruning ────────────────────────────────────────────────

    #[test]
    fn test_simplify_case_true_branch() {
        let expr = Expr::case(
            vec![
                WhenClause::new(Expr::boolean(true), Expr::string("hit")),
                WhenClause::new(Expr::column("x"), Expr::string("miss")),
            ],
            Some(Expr::string("default")),
        );
        assert_eq!(simplify(&expr), Expr::string("hit"));
    }

    #[test]
    fn test_simplify_case_false_prune() {
        let expr = Expr::case(
            vec![
                WhenClause::new(Expr::boolean(false), Expr::string("dead")),
                WhenClause::new(Expr::column("x"), Expr::string("live")),
            ],
            Some(Expr::string("default")),
        );
        let expected = Expr::case(
            vec![WhenClause::new(Expr::column("x"), Expr::string("live"))],
            Some(Expr::string("default")),
        );
        assert_eq!(simplify(&expr), expected);
    }

    #[test]
    fn test_simplify_case_all_false_to_else() {
        let expr = Expr::case(
            vec![
                WhenClause::new(Expr::boolean(false), Expr::string("a")),
                WhenClause::new(Expr::boolean(false), Expr::string("b")),
            ],
            Some(Expr::string("fallback")),
        );
        assert_eq!(simplify(&expr), Expr::string("fallback"));
    }

    #[test]
    fn test_simplify_case_all_false_no_else() {
        let expr = Expr::case(
            vec![WhenClause::new(Expr::boolean(false), Expr::string("a"))],
            None,
        );
        assert_eq!(simplify(&expr), Expr::null());
    }

    // ── Composite: substitute + simplify (SR-10 scenario) ───────────

    #[test]
    fn test_sr10_substitute_then_simplify_case() {
        // CASE WHEN dataset_name IN ('adwords', 'facebookads') THEN 'Paid Search'
        //      WHEN dataset_name = 'klaviyo' THEN 'Email'
        //      ELSE NULL END
        // With known: dataset_name = 'adwords'
        let expr = Expr::case(
            vec![
                WhenClause::new(
                    Expr::in_list(
                        Expr::column("dataset_name"),
                        vec![Expr::string("adwords"), Expr::string("facebookads")],
                    ),
                    Expr::string("Paid Search"),
                ),
                WhenClause::new(
                    Expr::eq(Expr::column("dataset_name"), Expr::string("klaviyo")),
                    Expr::string("Email"),
                ),
            ],
            None,
        );

        let known = HashMap::from([("dataset_name".to_string(), "adwords".to_string())]);
        let substituted = substitute(&expr, &known);
        let simplified = simplify(&substituted);

        // The first branch condition becomes `'adwords' IN ('adwords', 'facebookads')` → true
        // → collapses to 'Paid Search'
        assert_eq!(simplified, Expr::string("Paid Search"));
    }

    #[test]
    fn test_sr10_substitute_then_simplify_nested_case() {
        // Nested: CASE WHEN dataset_name = 'adwords' THEN
        //           CASE WHEN campaign LIKE 'UK%' THEN 'GB' ELSE 'US' END
        //         WHEN dataset_name = 'klaviyo' THEN 'Email'
        //         ELSE NULL END
        // With known: dataset_name = 'klaviyo'
        let inner_case = Expr::case(
            vec![WhenClause::new(
                Expr::Like(semstrait_core::expr::LikeExpr {
                    expr: Box::new(Expr::column("campaign")),
                    pattern: Box::new(Expr::string("UK%")),
                }),
                Expr::string("GB"),
            )],
            Some(Expr::string("US")),
        );

        let expr = Expr::case(
            vec![
                WhenClause::new(
                    Expr::eq(Expr::column("dataset_name"), Expr::string("adwords")),
                    inner_case,
                ),
                WhenClause::new(
                    Expr::eq(Expr::column("dataset_name"), Expr::string("klaviyo")),
                    Expr::string("Email"),
                ),
            ],
            None,
        );

        let known = HashMap::from([("dataset_name".to_string(), "klaviyo".to_string())]);
        let substituted = substitute(&expr, &known);
        let simplified = simplify(&substituted);

        // First branch: 'klaviyo' = 'adwords' → false → pruned
        // Second branch: 'klaviyo' = 'klaviyo' → true → 'Email'
        assert_eq!(simplified, Expr::string("Email"));
    }

    #[test]
    fn test_simplify_idempotent() {
        // Already simplified expression stays the same.
        let expr = Expr::column("x");
        assert_eq!(simplify(&expr), expr);
    }

    #[test]
    fn test_simplify_multi_pass_needed() {
        // After substituting and simplifying a CASE condition to true,
        // the next iteration collapses the CASE itself.
        // Build: CASE WHEN (true AND true) THEN 'x' ELSE 'y' END
        // Pass 1: (true AND true) → true
        // Pass 2: CASE WHEN true THEN 'x' → 'x'
        let expr = Expr::case(
            vec![WhenClause::new(
                Expr::binary(Expr::boolean(true), BinaryOp::And, Expr::boolean(true)),
                Expr::string("x"),
            )],
            Some(Expr::string("y")),
        );
        assert_eq!(simplify(&expr), Expr::string("x"));
    }
}
