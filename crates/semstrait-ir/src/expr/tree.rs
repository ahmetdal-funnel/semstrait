//! Canonical structural expression tree, parameterized over a leaf set
//! `L`. Variant catalog per spec `14 §3.3` / `35 §3.3`.
//!
//! The `Expr<L>` enum is the **single structural-variant carrier** for
//! every layer of the canonical-IR pipeline. The leaf set varies by
//! layer (`PhysicalLeaf` for canonical-IR, `SemanticLeaf` for the
//! authoring form); the structural variants are shared by construction.
//!
//! Per `14 §3.3`'s notes:
//! - `Window` is **compile-emitted only** — author-facing parsers do not
//!   accept window syntax; nodes enter the tree exclusively through
//!   sugar-accessor elimination during compile (`14 §4.2`).
//! - Engine-specific operators do not add `Expr<L>` variants. They land
//!   as `FunctionCall` entries via `FunctionRegistry` extensions per
//!   `[14a §7](../../../../docs/design/foundations/14a_function_catalog.md)`.
//! - `Aggregate`'s `filter` field carries the canonical
//!   `agg(expr) FILTER (WHERE p)` shape; adapter compensation for engines
//!   without native `FILTER` is the adapter's concern, not part of the
//!   canonical IR.
//!
//! ## Structural well-formedness via [`Tree::with_new_children`]
//!
//! Per spec `14 §3.3` plus the construction-boundary error contract from
//! `35 §15.1`, [`Tree::with_new_children`] enforces a small set of
//! structural rules at the rebuild boundary:
//!
//! - Aggregate cannot directly contain another Aggregate
//!   ([`crate::error::ValidateError::AggregateInAggregate`]).
//! - Window cannot directly contain Aggregate or Window in its `args`
//!   ([`crate::error::ValidateError::InvalidWindowChild`]).
//! - Coalesce / InList / Case must be non-empty
//!   ([`crate::error::ValidateError::EmptyCoalesce`] /
//!   [`crate::error::ValidateError::EmptyInList`] /
//!   [`crate::error::ValidateError::EmptyCase`]).
//! - Child count must match the variant's arity
//!   ([`crate::error::ValidateError::ChildCountMismatch`]).
//!
//! These checks fire **only** through [`Tree::with_new_children`]
//! (and therefore through [`Tree::transform`] and any `Rewriter<N>`
//! traversal that uses it). Direct construction via the enum literal
//! does not validate — Phase B (`19`) and the authoring-side parser
//! (`32`) own their own pre-flight checks.

use crate::error::ValidateError;
use crate::expr_kinds::{
    AggregateKind, BinaryOpKind, CastFailure, LikeKind, UnaryOpKind, WindowFn, WindowFrame,
};
use crate::functions::CanonicalFn;
use crate::tree::{ExprLeaf, Tree};
use crate::types::DataType;

/// Canonical structural expression tree, parameterized over leaf set `L`.
/// Variant catalog per spec `14 §3.3`. Every variant is `#[non_exhaustive]`
/// at the enum level per invariant I10.
///
/// Instantiated by the type aliases [`crate::expr::leaves::PhysicalExpr`]
/// (with [`crate::expr::leaves::PhysicalLeaf`]) and
/// [`crate::expr::leaves::SemanticExpr`] (with
/// [`crate::expr::leaves::SemanticLeaf`]).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Expr<L: ExprLeaf> {
    /// Leaf node — wraps a leaf-set value (column ref, literal, semantic
    /// reference, parameter, …).
    Leaf(L),

    /// Binary operator over two operands. Op roster per spec `14 §3.3`.
    BinaryOp {
        op: BinaryOpKind,
        left: Box<Self>,
        right: Box<Self>,
    },

    /// Unary operator over a single operand.
    UnaryOp { op: UnaryOpKind, operand: Box<Self> },

    /// Canonical function call. `name` is a [`CanonicalFn`] resolved
    /// against the sealed [`crate::functions::FunctionRegistry`] per
    /// `14a`.
    FunctionCall { name: CanonicalFn, args: Vec<Self> },

    /// Type cast. `target` is canonical [`DataType`]; `on_failure`
    /// selects the engine-level failure profile per
    /// [`crate::expr_kinds::CastFailure`].
    Cast {
        input: Box<Self>,
        target: DataType,
        on_failure: CastFailure,
    },

    /// CASE expression. `whens` is a non-empty list of `(predicate, body)`
    /// pairs; `else_` is the optional fallthrough body. Empty `whens`
    /// raises [`ValidateError::EmptyCase`] at rebuild.
    Case {
        whens: Vec<(Self, Self)>,
        else_: Option<Box<Self>>,
    },

    /// `value [NOT] IN (list...)`. Empty `list` raises
    /// [`ValidateError::EmptyInList`] at rebuild.
    InList {
        value: Box<Self>,
        list: Vec<Self>,
        negated: bool,
    },

    /// `value [NOT] BETWEEN low AND high`.
    Between {
        value: Box<Self>,
        low: Box<Self>,
        high: Box<Self>,
        negated: bool,
    },

    /// LIKE-family predicate. Variant tag per
    /// [`crate::expr_kinds::LikeKind`].
    Like {
        value: Box<Self>,
        pattern: Box<Self>,
        kind: LikeKind,
    },

    /// `value IS NULL`. The structurally negated form is built via
    /// `UnaryOp { op: Not, operand: IsNull(...) }`.
    IsNull(Box<Self>),

    /// `COALESCE(args...)`. Empty `args` raises
    /// [`ValidateError::EmptyCoalesce`] at rebuild.
    Coalesce(Vec<Self>),

    /// `NULLIF(left, right)`.
    NullIf { left: Box<Self>, right: Box<Self> },

    /// Aggregate function. `filter` carries the canonical
    /// `agg(expr) FILTER (WHERE p)` shape per `14 §3.3`. Cannot directly
    /// contain another `Aggregate` per `14 §3.3` — the rule fires on
    /// rebuild via [`ValidateError::AggregateInAggregate`].
    Aggregate {
        op: AggregateKind,
        args: Vec<Self>,
        distinct: bool,
        filter: Option<Box<Self>>,
    },

    /// Window function. Compile-emitted only via sugar-accessor
    /// elimination per `14 §4.2`. Cannot directly contain `Aggregate` or
    /// `Window` in its `args` — the rule fires on rebuild via
    /// [`ValidateError::InvalidWindowChild`].
    Window {
        function: WindowFn,
        args: Vec<Self>,
        partition_by: Vec<Self>,
        order_by: Vec<Self>,
        frame: Option<WindowFrame>,
    },
}

impl<L: ExprLeaf> Tree for Expr<L> {
    /// Borrowed access to this node's structural children, in deterministic
    /// order. The order is the natural reading order per variant — left
    /// before right, args before filter, args before partition_by before
    /// order_by, etc. — and is the inverse of the order
    /// [`Expr::with_new_children`] consumes.
    ///
    /// **Per-variant child layout:**
    ///
    /// | Variant | Children (in order) |
    /// |---|---|
    /// | `Leaf` | none |
    /// | `BinaryOp` | `left`, `right` |
    /// | `UnaryOp` | `operand` |
    /// | `FunctionCall` | `args...` |
    /// | `Cast` | `input` |
    /// | `Case` | `cond_0, body_0, cond_1, body_1, ...`, then `else_?` |
    /// | `InList` | `value`, `list...` |
    /// | `Between` | `value`, `low`, `high` |
    /// | `Like` | `value`, `pattern` |
    /// | `IsNull` | inner |
    /// | `Coalesce` | `args...` |
    /// | `NullIf` | `left`, `right` |
    /// | `Aggregate` | `args...`, then `filter?` |
    /// | `Window` | `args...`, `partition_by...`, `order_by...` |
    ///
    /// `WindowFrame` is metadata, not a child.
    fn children(&self) -> Vec<&Self> {
        match self {
            Expr::Leaf(_) => Vec::new(),
            Expr::BinaryOp { left, right, .. } => vec![left.as_ref(), right.as_ref()],
            Expr::UnaryOp { operand, .. } => vec![operand.as_ref()],
            Expr::FunctionCall { args, .. } => args.iter().collect(),
            Expr::Cast { input, .. } => vec![input.as_ref()],
            Expr::Case { whens, else_ } => {
                let mut v = Vec::with_capacity(whens.len() * 2 + else_.is_some() as usize);
                for (cond, body) in whens {
                    v.push(cond);
                    v.push(body);
                }
                if let Some(e) = else_ {
                    v.push(e.as_ref());
                }
                v
            }
            Expr::InList { value, list, .. } => {
                let mut v = Vec::with_capacity(1 + list.len());
                v.push(value.as_ref());
                v.extend(list.iter());
                v
            }
            Expr::Between {
                value, low, high, ..
            } => vec![value.as_ref(), low.as_ref(), high.as_ref()],
            Expr::Like { value, pattern, .. } => vec![value.as_ref(), pattern.as_ref()],
            Expr::IsNull(inner) => vec![inner.as_ref()],
            Expr::Coalesce(args) => args.iter().collect(),
            Expr::NullIf { left, right } => vec![left.as_ref(), right.as_ref()],
            Expr::Aggregate { args, filter, .. } => {
                let mut v = Vec::with_capacity(args.len() + filter.is_some() as usize);
                v.extend(args.iter());
                if let Some(f) = filter {
                    v.push(f.as_ref());
                }
                v
            }
            Expr::Window {
                args,
                partition_by,
                order_by,
                ..
            } => {
                let mut v = Vec::with_capacity(args.len() + partition_by.len() + order_by.len());
                v.extend(args.iter());
                v.extend(partition_by.iter());
                v.extend(order_by.iter());
                v
            }
        }
    }

    /// Rebuild this node with a new child list. The order MUST match the
    /// order [`Expr::children`] returns. Returns
    /// [`ValidateError::ChildCountMismatch`] when the arity disagrees and
    /// runs the structural well-formedness checks per the module docs.
    fn with_new_children(self, mut new_children: Vec<Self>) -> Result<Self, ValidateError> {
        let expected = self.children().len();
        if new_children.len() != expected {
            return Err(ValidateError::ChildCountMismatch {
                expected,
                got: new_children.len(),
            });
        }

        // Drain helper — pulls the next `n` from the head of `new_children`.
        // Caller has already validated the total length, so each `take_n`
        // consumes the slot directly via `drain`.
        let mut drain = |n: usize| -> Vec<Self> { new_children.drain(..n).collect() };

        let rebuilt = match self {
            Expr::Leaf(l) => {
                debug_assert!(new_children.is_empty(), "Leaf has no children");
                Expr::Leaf(l)
            }
            Expr::BinaryOp { op, .. } => {
                let mut taken = drain(2);
                let right = taken.pop().expect("arity 2");
                let left = taken.pop().expect("arity 2");
                Expr::BinaryOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }
            }
            Expr::UnaryOp { op, .. } => {
                let mut taken = drain(1);
                let operand = taken.pop().expect("arity 1");
                Expr::UnaryOp {
                    op,
                    operand: Box::new(operand),
                }
            }
            Expr::FunctionCall { name, args } => {
                let new_args = drain(args.len());
                Expr::FunctionCall {
                    name,
                    args: new_args,
                }
            }
            Expr::Cast {
                target, on_failure, ..
            } => {
                let mut taken = drain(1);
                let input = taken.pop().expect("arity 1");
                Expr::Cast {
                    input: Box::new(input),
                    target,
                    on_failure,
                }
            }
            Expr::Case { whens, else_ } => {
                let want = whens.len() * 2 + usize::from(else_.is_some());
                let mut taken = drain(want);
                // Re-pair whens; consume in same order children() emitted.
                let mut new_whens = Vec::with_capacity(whens.len());
                let mut iter = taken.drain(..);
                for _ in 0..whens.len() {
                    let cond = iter.next().expect("paired");
                    let body = iter.next().expect("paired");
                    new_whens.push((cond, body));
                }
                let new_else = if else_.is_some() {
                    Some(Box::new(iter.next().expect("else")))
                } else {
                    None
                };
                Expr::Case {
                    whens: new_whens,
                    else_: new_else,
                }
            }
            Expr::InList { list, negated, .. } => {
                let want = 1 + list.len();
                let mut taken = drain(want);
                let mut iter = taken.drain(..);
                let value = iter.next().expect("value");
                let new_list: Vec<Self> = iter.collect();
                Expr::InList {
                    value: Box::new(value),
                    list: new_list,
                    negated,
                }
            }
            Expr::Between { negated, .. } => {
                let mut taken = drain(3);
                let high = taken.pop().expect("arity 3");
                let low = taken.pop().expect("arity 3");
                let value = taken.pop().expect("arity 3");
                Expr::Between {
                    value: Box::new(value),
                    low: Box::new(low),
                    high: Box::new(high),
                    negated,
                }
            }
            Expr::Like { kind, .. } => {
                let mut taken = drain(2);
                let pattern = taken.pop().expect("arity 2");
                let value = taken.pop().expect("arity 2");
                Expr::Like {
                    value: Box::new(value),
                    pattern: Box::new(pattern),
                    kind,
                }
            }
            Expr::IsNull(_) => {
                let mut taken = drain(1);
                let inner = taken.pop().expect("arity 1");
                Expr::IsNull(Box::new(inner))
            }
            Expr::Coalesce(args) => {
                let new_args = drain(args.len());
                Expr::Coalesce(new_args)
            }
            Expr::NullIf { .. } => {
                let mut taken = drain(2);
                let right = taken.pop().expect("arity 2");
                let left = taken.pop().expect("arity 2");
                Expr::NullIf {
                    left: Box::new(left),
                    right: Box::new(right),
                }
            }
            Expr::Aggregate {
                op,
                args,
                distinct,
                filter,
            } => {
                let want = args.len() + usize::from(filter.is_some());
                let mut taken = drain(want);
                let mut iter = taken.drain(..);
                let new_args: Vec<Self> = (&mut iter).take(args.len()).collect();
                let new_filter = if filter.is_some() {
                    Some(Box::new(iter.next().expect("filter")))
                } else {
                    None
                };
                Expr::Aggregate {
                    op,
                    args: new_args,
                    distinct,
                    filter: new_filter,
                }
            }
            Expr::Window {
                function,
                args,
                partition_by,
                order_by,
                frame,
            } => {
                let want = args.len() + partition_by.len() + order_by.len();
                let mut taken = drain(want);
                let mut iter = taken.drain(..);
                let new_args: Vec<Self> = (&mut iter).take(args.len()).collect();
                let new_partition: Vec<Self> = (&mut iter).take(partition_by.len()).collect();
                let new_order: Vec<Self> = (&mut iter).take(order_by.len()).collect();
                Expr::Window {
                    function,
                    args: new_args,
                    partition_by: new_partition,
                    order_by: new_order,
                    frame,
                }
            }
        };

        check_well_formed(&rebuilt)?;
        Ok(rebuilt)
    }
}

/// Run the structural well-formedness rules per spec `14 §3.3` and the
/// construction-boundary contract per `35 §15.1`. Called by
/// [`Tree::with_new_children`] after the rebuild.
fn check_well_formed<L: ExprLeaf>(expr: &Expr<L>) -> Result<(), ValidateError> {
    match expr {
        // Aggregate cannot directly contain another Aggregate (in args or filter).
        Expr::Aggregate { args, filter, .. } => {
            for a in args {
                if matches!(a, Expr::Aggregate { .. }) {
                    return Err(ValidateError::AggregateInAggregate);
                }
            }
            if let Some(f) = filter {
                if matches!(f.as_ref(), Expr::Aggregate { .. }) {
                    return Err(ValidateError::AggregateInAggregate);
                }
            }
            Ok(())
        }

        // Window cannot directly contain Aggregate or Window (in args).
        Expr::Window { args, .. } => {
            for a in args {
                if matches!(a, Expr::Aggregate { .. } | Expr::Window { .. }) {
                    return Err(ValidateError::InvalidWindowChild);
                }
            }
            Ok(())
        }

        // Coalesce.len() == 0 is illegal.
        Expr::Coalesce(args) => {
            if args.is_empty() {
                Err(ValidateError::EmptyCoalesce)
            } else {
                Ok(())
            }
        }

        // InList.list.len() == 0 is illegal.
        Expr::InList { list, .. } => {
            if list.is_empty() {
                Err(ValidateError::EmptyInList)
            } else {
                Ok(())
            }
        }

        // Case.whens.len() == 0 is illegal.
        Expr::Case { whens, .. } => {
            if whens.is_empty() {
                Err(ValidateError::EmptyCase)
            } else {
                Ok(())
            }
        }

        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr_kinds::{
        AggregateKind, AggregationOp, BinaryOpKind, CastFailure, LikeKind, Literal, UnaryOpKind,
        WindowBound, WindowFn, WindowFrame, WindowFrameKind,
    };
    use crate::functions::CanonicalFn;
    use crate::tree::ExprLeaf;
    use crate::types::DataType;

    /// Minimal leaf set used only for tree-machinery tests. Represents a
    /// [`Literal`] payload — no semantic / column reference vocabulary.
    /// Phase 2b's real leaf sets (`PhysicalLeaf` / `SemanticLeaf`) live in
    /// `expr/leaves.rs`; this stub exists so `tree.rs` tests do not depend
    /// on the leaf module's coverage.
    #[derive(Debug, Clone, PartialEq)]
    enum TestLeaf {
        Lit(Literal),
    }

    impl ExprLeaf for TestLeaf {
        fn inferred_type(&self) -> Option<&DataType> {
            None
        }
    }

    fn lit(i: i64) -> Expr<TestLeaf> {
        Expr::Leaf(TestLeaf::Lit(Literal::Integer {
            value: i,
            width: crate::expr_kinds::IntegerWidth::Long,
        }))
    }

    // ── children() arity per variant ─────────────────────────────────────

    #[test]
    fn leaf_has_no_children() {
        let e = lit(1);
        assert_eq!(e.children().len(), 0);
    }

    #[test]
    fn binary_op_has_two_children() {
        let e = Expr::<TestLeaf>::BinaryOp {
            op: BinaryOpKind::Add,
            left: Box::new(lit(1)),
            right: Box::new(lit(2)),
        };
        assert_eq!(e.children().len(), 2);
    }

    #[test]
    fn unary_op_has_one_child() {
        let e = Expr::<TestLeaf>::UnaryOp {
            op: UnaryOpKind::Negate,
            operand: Box::new(lit(1)),
        };
        assert_eq!(e.children().len(), 1);
    }

    #[test]
    fn function_call_arity_matches_args() {
        let e = Expr::<TestLeaf>::FunctionCall {
            name: CanonicalFn::new("upper").unwrap(),
            args: vec![lit(1), lit(2), lit(3)],
        };
        assert_eq!(e.children().len(), 3);
    }

    #[test]
    fn cast_has_one_child() {
        let e = Expr::<TestLeaf>::Cast {
            input: Box::new(lit(1)),
            target: DataType::String,
            on_failure: CastFailure::Null,
        };
        assert_eq!(e.children().len(), 1);
    }

    #[test]
    fn case_children_flatten_pairs_then_else() {
        let e = Expr::<TestLeaf>::Case {
            whens: vec![(lit(1), lit(2)), (lit(3), lit(4))],
            else_: Some(Box::new(lit(99))),
        };
        // 2 whens × 2 + 1 else = 5
        assert_eq!(e.children().len(), 5);

        let no_else = Expr::<TestLeaf>::Case {
            whens: vec![(lit(1), lit(2))],
            else_: None,
        };
        // 1 when × 2 = 2
        assert_eq!(no_else.children().len(), 2);
    }

    #[test]
    fn in_list_children_value_then_list() {
        let e = Expr::<TestLeaf>::InList {
            value: Box::new(lit(0)),
            list: vec![lit(1), lit(2), lit(3)],
            negated: false,
        };
        assert_eq!(e.children().len(), 4);
    }

    #[test]
    fn between_has_three_children() {
        let e = Expr::<TestLeaf>::Between {
            value: Box::new(lit(5)),
            low: Box::new(lit(1)),
            high: Box::new(lit(10)),
            negated: false,
        };
        assert_eq!(e.children().len(), 3);
    }

    #[test]
    fn like_has_two_children() {
        let e = Expr::<TestLeaf>::Like {
            value: Box::new(lit(1)),
            pattern: Box::new(lit(2)),
            kind: LikeKind::Like,
        };
        assert_eq!(e.children().len(), 2);
    }

    #[test]
    fn is_null_has_one_child() {
        let e = Expr::<TestLeaf>::IsNull(Box::new(lit(1)));
        assert_eq!(e.children().len(), 1);
    }

    #[test]
    fn coalesce_arity_matches_args() {
        let e = Expr::<TestLeaf>::Coalesce(vec![lit(1), lit(2), lit(3)]);
        assert_eq!(e.children().len(), 3);
    }

    #[test]
    fn null_if_has_two_children() {
        let e = Expr::<TestLeaf>::NullIf {
            left: Box::new(lit(1)),
            right: Box::new(lit(2)),
        };
        assert_eq!(e.children().len(), 2);
    }

    #[test]
    fn aggregate_children_args_then_filter() {
        let e = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Sum),
            args: vec![lit(1), lit(2)],
            distinct: false,
            filter: Some(Box::new(lit(99))),
        };
        assert_eq!(e.children().len(), 3);

        let no_filter = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Sum),
            args: vec![lit(1)],
            distinct: false,
            filter: None,
        };
        assert_eq!(no_filter.children().len(), 1);
    }

    #[test]
    fn window_children_args_partition_order() {
        let e = Expr::<TestLeaf>::Window {
            function: WindowFn::Lag,
            args: vec![lit(1), lit(2)],
            partition_by: vec![lit(10)],
            order_by: vec![lit(20), lit(30)],
            frame: Some(WindowFrame {
                kind: WindowFrameKind::Rows,
                start: WindowBound::UnboundedPreceding,
                end: WindowBound::CurrentRow,
            }),
        };
        // 2 args + 1 partition + 2 order = 5 (frame is metadata, not a child)
        assert_eq!(e.children().len(), 5);
    }

    // ── with_new_children round-trip ─────────────────────────────────────

    fn assert_round_trip(e: Expr<TestLeaf>) {
        let original = e.clone();
        let kids: Vec<Expr<TestLeaf>> = e.children().into_iter().cloned().collect();
        let rebuilt = e.with_new_children(kids).expect("identity rebuild");
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn round_trip_leaf() {
        assert_round_trip(lit(42));
    }

    #[test]
    fn round_trip_binary_op() {
        let e = Expr::<TestLeaf>::BinaryOp {
            op: BinaryOpKind::Add,
            left: Box::new(lit(1)),
            right: Box::new(lit(2)),
        };
        assert_round_trip(e);
    }

    #[test]
    fn round_trip_case_with_else() {
        let e = Expr::<TestLeaf>::Case {
            whens: vec![(lit(1), lit(10)), (lit(2), lit(20))],
            else_: Some(Box::new(lit(99))),
        };
        assert_round_trip(e);
    }

    #[test]
    fn round_trip_aggregate_with_filter() {
        let e = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Avg),
            args: vec![lit(1)],
            distinct: true,
            filter: Some(Box::new(lit(2))),
        };
        assert_round_trip(e);
    }

    #[test]
    fn round_trip_window() {
        let e = Expr::<TestLeaf>::Window {
            function: WindowFn::Rank,
            args: vec![lit(1)],
            partition_by: vec![lit(2), lit(3)],
            order_by: vec![lit(4)],
            frame: None,
        };
        assert_round_trip(e);
    }

    // ── ChildCountMismatch on every variant ─────────────────────────────

    fn assert_arity_mismatch(e: Expr<TestLeaf>, supply_n: usize, expected_n: usize) {
        let supplied: Vec<Expr<TestLeaf>> = (0..supply_n as i64).map(lit).collect();
        let result = e.with_new_children(supplied);
        match result {
            Err(ValidateError::ChildCountMismatch { expected, got }) => {
                assert_eq!(expected, expected_n);
                assert_eq!(got, supply_n);
            }
            other => panic!("expected ChildCountMismatch, got {:?}", other),
        }
    }

    #[test]
    fn child_count_mismatch_on_each_variant() {
        // Leaf: expects 0 children, supply 1.
        assert_arity_mismatch(lit(0), 1, 0);

        // BinaryOp: expects 2, supply 1.
        assert_arity_mismatch(
            Expr::<TestLeaf>::BinaryOp {
                op: BinaryOpKind::Add,
                left: Box::new(lit(1)),
                right: Box::new(lit(2)),
            },
            1,
            2,
        );

        // UnaryOp: expects 1, supply 0.
        assert_arity_mismatch(
            Expr::<TestLeaf>::UnaryOp {
                op: UnaryOpKind::Not,
                operand: Box::new(lit(1)),
            },
            0,
            1,
        );

        // FunctionCall: expects 2, supply 3.
        assert_arity_mismatch(
            Expr::<TestLeaf>::FunctionCall {
                name: CanonicalFn::new("f").unwrap(),
                args: vec![lit(1), lit(2)],
            },
            3,
            2,
        );

        // Cast: expects 1, supply 2.
        assert_arity_mismatch(
            Expr::<TestLeaf>::Cast {
                input: Box::new(lit(1)),
                target: DataType::Integer,
                on_failure: CastFailure::Error,
            },
            2,
            1,
        );

        // Case: expects 3 (1 when + else), supply 0.
        assert_arity_mismatch(
            Expr::<TestLeaf>::Case {
                whens: vec![(lit(1), lit(2))],
                else_: Some(Box::new(lit(3))),
            },
            0,
            3,
        );

        // InList: expects 3 (value + 2 list), supply 1.
        assert_arity_mismatch(
            Expr::<TestLeaf>::InList {
                value: Box::new(lit(0)),
                list: vec![lit(1), lit(2)],
                negated: false,
            },
            1,
            3,
        );

        // Between: expects 3, supply 2.
        assert_arity_mismatch(
            Expr::<TestLeaf>::Between {
                value: Box::new(lit(1)),
                low: Box::new(lit(0)),
                high: Box::new(lit(10)),
                negated: false,
            },
            2,
            3,
        );

        // Like: expects 2, supply 1.
        assert_arity_mismatch(
            Expr::<TestLeaf>::Like {
                value: Box::new(lit(1)),
                pattern: Box::new(lit(2)),
                kind: LikeKind::Like,
            },
            1,
            2,
        );

        // IsNull: expects 1, supply 0.
        assert_arity_mismatch(Expr::<TestLeaf>::IsNull(Box::new(lit(1))), 0, 1);

        // Coalesce: expects 2, supply 0.
        assert_arity_mismatch(Expr::<TestLeaf>::Coalesce(vec![lit(1), lit(2)]), 0, 2);

        // NullIf: expects 2, supply 3.
        assert_arity_mismatch(
            Expr::<TestLeaf>::NullIf {
                left: Box::new(lit(1)),
                right: Box::new(lit(2)),
            },
            3,
            2,
        );

        // Aggregate: expects 2 (1 arg + filter), supply 1.
        assert_arity_mismatch(
            Expr::<TestLeaf>::Aggregate {
                op: AggregateKind::Builtin(AggregationOp::Sum),
                args: vec![lit(1)],
                distinct: false,
                filter: Some(Box::new(lit(2))),
            },
            1,
            2,
        );

        // Window: expects 3 (1 arg + 1 partition + 1 order), supply 0.
        assert_arity_mismatch(
            Expr::<TestLeaf>::Window {
                function: WindowFn::Rank,
                args: vec![lit(1)],
                partition_by: vec![lit(2)],
                order_by: vec![lit(3)],
                frame: None,
            },
            0,
            3,
        );
    }

    // ── Structural well-formedness ──────────────────────────────────────

    #[test]
    fn aggregate_in_aggregate_is_rejected_via_args() {
        // Build a valid Aggregate via the enum, then rebuild it with an
        // inner Aggregate as one of its args via `with_new_children`.
        let outer = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Sum),
            args: vec![lit(1)],
            distinct: false,
            filter: None,
        };
        let inner = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Avg),
            args: vec![lit(2)],
            distinct: false,
            filter: None,
        };
        let result = outer.with_new_children(vec![inner]);
        assert!(matches!(result, Err(ValidateError::AggregateInAggregate)));
    }

    #[test]
    fn aggregate_in_aggregate_is_rejected_via_filter() {
        let outer = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Sum),
            args: vec![lit(1)],
            distinct: false,
            filter: Some(Box::new(lit(2))),
        };
        // Re-supply args (lit) + filter (Aggregate). The original child
        // count is 2 (1 arg + 1 filter); the rebuild should detect the
        // structural rule violation.
        let inner = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Count),
            args: vec![lit(99)],
            distinct: false,
            filter: None,
        };
        let result = outer.with_new_children(vec![lit(1), inner]);
        assert!(matches!(result, Err(ValidateError::AggregateInAggregate)));
    }

    #[test]
    fn window_with_aggregate_arg_is_rejected() {
        let win = Expr::<TestLeaf>::Window {
            function: WindowFn::Rank,
            args: vec![lit(1)],
            partition_by: vec![lit(2)],
            order_by: vec![lit(3)],
            frame: None,
        };
        let inner_agg = Expr::<TestLeaf>::Aggregate {
            op: AggregateKind::Builtin(AggregationOp::Sum),
            args: vec![lit(99)],
            distinct: false,
            filter: None,
        };
        // args slot is index 0 of children; partition / order are 1 and 2.
        let result = win.with_new_children(vec![inner_agg, lit(2), lit(3)]);
        assert!(matches!(result, Err(ValidateError::InvalidWindowChild)));
    }

    #[test]
    fn window_with_window_arg_is_rejected() {
        let win = Expr::<TestLeaf>::Window {
            function: WindowFn::Rank,
            args: vec![lit(1)],
            partition_by: vec![lit(2)],
            order_by: vec![lit(3)],
            frame: None,
        };
        let inner_win = Expr::<TestLeaf>::Window {
            function: WindowFn::Lag,
            args: vec![lit(99)],
            partition_by: vec![lit(100)],
            order_by: vec![lit(101)],
            frame: None,
        };
        let result = win.with_new_children(vec![inner_win, lit(2), lit(3)]);
        assert!(matches!(result, Err(ValidateError::InvalidWindowChild)));
    }

    #[test]
    fn empty_coalesce_is_rejected() {
        // A Coalesce with one arg is valid; rebuild it with zero args.
        // Direct empty construction also fails on round-trip but the
        // narrow rule needs `with_new_children` to fire.
        let original = Expr::<TestLeaf>::Coalesce(vec![lit(1)]);
        // Empty children supplied — original arity is 1, so this is also
        // a count mismatch. Rebuild it via a different approach: start
        // from an empty Coalesce by supplying `Coalesce(vec![])` directly
        // (which by Phase 2b construction is allowed to exist) and then
        // re-rebuild it with zero children.
        let _ = original; // silence unused warning above
        let empty = Expr::<TestLeaf>::Coalesce(Vec::new());
        let result = empty.with_new_children(Vec::new());
        assert!(matches!(result, Err(ValidateError::EmptyCoalesce)));
    }

    #[test]
    fn empty_in_list_is_rejected() {
        let expr = Expr::<TestLeaf>::InList {
            value: Box::new(lit(0)),
            list: Vec::new(),
            negated: false,
        };
        // children count is 1 (value only); supply [value].
        let result = expr.with_new_children(vec![lit(0)]);
        assert!(matches!(result, Err(ValidateError::EmptyInList)));
    }

    #[test]
    fn empty_case_is_rejected() {
        let expr = Expr::<TestLeaf>::Case {
            whens: Vec::new(),
            else_: None,
        };
        let result = expr.with_new_children(Vec::new());
        assert!(matches!(result, Err(ValidateError::EmptyCase)));
    }

    #[test]
    fn empty_case_with_else_is_rejected() {
        let expr = Expr::<TestLeaf>::Case {
            whens: Vec::new(),
            else_: Some(Box::new(lit(0))),
        };
        // children count is 1 (else only)
        let result = expr.with_new_children(vec![lit(99)]);
        assert!(matches!(result, Err(ValidateError::EmptyCase)));
    }
}
