//! Universal-traversal trait family per spec `14 §3.1` / `§3.2` and crate-home
//! placement per `35 §3.2`.
//!
//! This module owns the four traits that every IR tree (`Expr<L>` and
//! `PlanNode`) implements:
//!
//! - [`Tree`] — the structural traversal contract. Every implementor exposes
//!   `children` and `with_new_children`. Default-provided helpers `apply` /
//!   `transform` compose those primitives into pre-order visitor walks and
//!   bottom-up rewrites.
//! - [`Visitor`] — read-only walk over a `&Tree`. The `f_down` / `f_up`
//!   shape is canonical for `ControlFlow`-driven early termination.
//! - [`Rewriter`] — fallible per-node rewrite over an owned `Tree`.
//! - [`ExprLeaf`] — per-leaf-set metadata contract. Implemented by both
//!   `PhysicalLeaf` and `SemanticLeaf` once those types land in `expr/`
//!   (Phase 2b). Stable surface here is `inferred_type()`.
//!
//! These traits are stage-agnostic: `Tree`'s default `apply` works for an
//! `Expr<L>` tree just as it does for a `PlanNode` tree. The single trait
//! surface is what lets one generic algorithm operate on both shapes — see
//! `35 §3.2` for the rationale.
//!
//! Per the second-cascade placement (`STATUS.md` item Q, 2026-05-19), this
//! module lives in `semstrait-ir`, not `semstrait-core`.

use crate::error::ValidateError;
use semstrait_core::DataType;
use std::ops::ControlFlow;

/// Universal traversal contract. Implemented by `Expr<L>` and `PlanNode`.
/// Stage-agnostic. Per spec `14 §3.1`, `35 §3.2`.
///
/// Implementors MUST satisfy two structural invariants:
///
/// 1. `children()` returns borrows into `&self`'s structural children in a
///    deterministic order.
/// 2. `with_new_children(self, new_children)` rebuilds an equivalent node
///    when `new_children.len() == self.children().len()` and child kinds
///    match the variant's expected slots; otherwise returns
///    [`ValidateError`].
///
/// The default-provided `apply` and `transform` methods compose those two
/// primitives. `apply` performs a pre-order read-only walk via [`Visitor`];
/// `transform` performs a bottom-up rewrite via the supplied closure.
///
/// `transform` requires `Self: Clone` because the default body extracts
/// children via `children()` (which returns `&Self`) and clones them before
/// recursing. An opt-in non-cloning rewrite path may be added later; for v1
/// the cost of one clone per node is acceptable (per `m04-zero-cost` —
/// monomorphization keeps this cost local to the traversal call site).
pub trait Tree: Sized {
    /// Borrowed access to this node's structural children, in a
    /// deterministic order.
    fn children(&self) -> Vec<&Self>;

    /// Rebuild this node with a new child list. Returns
    /// [`ValidateError::ChildCountMismatch`] when `new_children.len()` does
    /// not match the variant's expected arity. Implementors may also raise
    /// other [`ValidateError`] variants for variant-specific structural
    /// invariants (e.g. `Aggregate` cannot directly contain another
    /// `Aggregate`).
    fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateError>;

    /// Pre-order read-only walk. Calls `v.f_down(self)`, then recurses into
    /// each child in `children()` order, then calls `v.f_up(self)`.
    /// Short-circuits on `ControlFlow::Break`.
    fn apply<V: Visitor<Self>>(&self, v: &mut V) -> ControlFlow<V::Output> {
        v.f_down(self)?;
        for child in self.children() {
            child.apply(v)?;
        }
        v.f_up(self)
    }

    /// Bottom-up rewrite. Recurses into each child via `transform(f)`,
    /// rebuilds `self` via `with_new_children`, then calls `f(rebuilt)`.
    ///
    /// Requires `Self: Clone` because children are accessed via the
    /// borrowing `children()` API and must be moved out for further
    /// rewriting. Implementors that cannot afford a per-node clone may
    /// override `transform` with a move-aware variant.
    fn transform<F>(self, f: &mut F) -> Result<Self, ValidateError>
    where
        F: FnMut(Self) -> Result<Self, ValidateError>,
        Self: Clone,
    {
        let new_children: Vec<Self> = self
            .children()
            .into_iter()
            .cloned()
            .map(|child| child.transform(f))
            .collect::<Result<Vec<_>, _>>()?;
        let rebuilt = self.with_new_children(new_children)?;
        f(rebuilt)
    }
}

/// Read-only walk callback. Invoked by [`Tree::apply`] on entry (`f_down`)
/// and exit (`f_up`) of each node. Returning `ControlFlow::Break(out)`
/// short-circuits the walk and yields `out`. Per spec `14 §3.1`.
pub trait Visitor<N> {
    /// Per-walk output type. `()` for traversals that only mutate visitor
    /// state; a richer type for early-termination walks (e.g. "first
    /// matching node found").
    type Output;

    /// Called before recursing into a node's children.
    fn f_down(&mut self, node: &N) -> ControlFlow<Self::Output>;

    /// Called after recursing into a node's children.
    fn f_up(&mut self, node: &N) -> ControlFlow<Self::Output>;
}

/// Owned-rewrite callback. Per spec `14 §3.1`.
///
/// Distinct from the `FnMut(N) -> Result<N, ValidateError>` closure that
/// [`Tree::transform`] accepts: a `Rewriter` exposes both the down-pass
/// and up-pass hooks for callers that need to inspect a node both before
/// and after its children have been rewritten.
pub trait Rewriter<N> {
    /// Called before recursing into the node's children, on the moved
    /// node. Returning `Err` aborts the rewrite.
    fn f_down(&mut self, node: N) -> Result<N, ValidateError>;

    /// Called after recursing into the node's children, on the rebuilt
    /// node. Returning `Err` aborts the rewrite.
    fn f_up(&mut self, node: N) -> Result<N, ValidateError>;
}

/// Per-leaf-set metadata contract. Implemented by `PhysicalLeaf` and
/// `SemanticLeaf` once they land in `expr/leaves` (Phase 2b). Per spec
/// `14 §3.2`, `35 §3.2`.
///
/// The single method `inferred_type` returns the leaf's canonical
/// [`DataType`] when locally determinable. It returns `None` for leaves
/// whose type is not yet resolved (e.g. an unresolved `SemanticLeaf::Field`
/// before compile-time substitution, or an untyped `Literal::Null`).
///
/// `ExprLeaf` is intentionally minimal — leaf-set–specific behaviour
/// (semantic-ref resolution per `19 §3`, `Parameter` binding at plan time)
/// lives at the site that operates on the leaf, not as a trait method.
pub trait ExprLeaf: Sized + Clone + std::fmt::Debug + PartialEq {
    /// Canonical logical type carried (or inferred) by this leaf. Returns
    /// `None` only when the type cannot be determined locally.
    fn inferred_type(&self) -> Option<&DataType>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ValidateError;

    /// Minimal `Tree` implementor for trait-machinery tests. A `MockTree`
    /// is a tagged node with a `Vec<MockTree>` of children and an `i32`
    /// payload. `children()` returns borrows; `with_new_children` enforces
    /// a `ChildCountMismatch` when arities disagree.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockTree {
        tag: i32,
        children: Vec<MockTree>,
    }

    impl MockTree {
        fn leaf(tag: i32) -> Self {
            Self {
                tag,
                children: Vec::new(),
            }
        }

        fn parent(tag: i32, children: Vec<Self>) -> Self {
            Self { tag, children }
        }
    }

    impl Tree for MockTree {
        fn children(&self) -> Vec<&Self> {
            self.children.iter().collect()
        }

        fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateError> {
            if new_children.len() != self.children.len() {
                return Err(ValidateError::ChildCountMismatch {
                    expected: self.children.len(),
                    got: new_children.len(),
                });
            }
            Ok(Self {
                tag: self.tag,
                children: new_children,
            })
        }
    }

    /// Records pre-order tag visits in `f_down`. `f_up` is a no-op.
    struct TagCollector {
        tags: Vec<i32>,
    }

    impl Visitor<MockTree> for TagCollector {
        type Output = ();

        fn f_down(&mut self, node: &MockTree) -> ControlFlow<Self::Output> {
            self.tags.push(node.tag);
            ControlFlow::Continue(())
        }

        fn f_up(&mut self, _node: &MockTree) -> ControlFlow<Self::Output> {
            ControlFlow::Continue(())
        }
    }

    /// Counts every node visited (in either f_down or f_up).
    struct NodeCounter {
        seen: usize,
    }

    impl Visitor<MockTree> for NodeCounter {
        type Output = ();

        fn f_down(&mut self, _node: &MockTree) -> ControlFlow<Self::Output> {
            self.seen += 1;
            ControlFlow::Continue(())
        }

        fn f_up(&mut self, _node: &MockTree) -> ControlFlow<Self::Output> {
            ControlFlow::Continue(())
        }
    }

    fn sample_tree() -> MockTree {
        // Shape:
        //   1
        //   ├── 2
        //   │   └── 4
        //   └── 3
        MockTree::parent(
            1,
            vec![
                MockTree::parent(2, vec![MockTree::leaf(4)]),
                MockTree::leaf(3),
            ],
        )
    }

    #[test]
    fn apply_visits_pre_order_parent_before_children() {
        let tree = sample_tree();
        let mut collector = TagCollector { tags: Vec::new() };
        let _ = tree.apply(&mut collector);
        // Pre-order: 1, then descend into first child subtree (2, 4), then second (3).
        assert_eq!(collector.tags, vec![1, 2, 4, 3]);
    }

    #[test]
    fn apply_counts_every_node_via_visitor() {
        let tree = sample_tree();
        let mut counter = NodeCounter { seen: 0 };
        let _ = tree.apply(&mut counter);
        assert_eq!(counter.seen, 4);
    }

    #[test]
    fn transform_identity_preserves_tree() {
        let tree = sample_tree();
        let original = tree.clone();
        let mut identity = |t: MockTree| -> Result<MockTree, ValidateError> { Ok(t) };
        let rebuilt = tree.transform(&mut identity).expect("identity rewrite");
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn transform_increments_each_tag() {
        let tree = sample_tree();
        let mut bump = |t: MockTree| -> Result<MockTree, ValidateError> {
            Ok(MockTree {
                tag: t.tag + 10,
                children: t.children,
            })
        };
        let rebuilt = tree.transform(&mut bump).expect("bump rewrite");
        // After bottom-up bump: leaf 4 -> 14, then parent 2 -> 12 (rebuilt over 14),
        // then leaf 3 -> 13, then root 1 -> 11.
        let expected = MockTree::parent(
            11,
            vec![
                MockTree::parent(12, vec![MockTree::leaf(14)]),
                MockTree::leaf(13),
            ],
        );
        assert_eq!(rebuilt, expected);
    }

    #[test]
    fn transform_propagates_callback_error() {
        let tree = sample_tree();
        let mut explode = |t: MockTree| -> Result<MockTree, ValidateError> {
            if t.tag == 4 {
                Err(ValidateError::ChildCountMismatch {
                    expected: 0,
                    got: 99,
                })
            } else {
                Ok(t)
            }
        };
        let result = tree.transform(&mut explode);
        match result {
            Err(ValidateError::ChildCountMismatch { expected, got }) => {
                assert_eq!(expected, 0);
                assert_eq!(got, 99);
            }
            other => panic!("expected ChildCountMismatch, got {:?}", other),
        }
    }

    /// `with_new_children` rejects a mismatched child count with a
    /// dedicated `ValidateError` variant, exercising the construction-
    /// boundary error contract.
    #[test]
    fn with_new_children_rejects_arity_mismatch() {
        let parent = MockTree::parent(1, vec![MockTree::leaf(2), MockTree::leaf(3)]);
        let result = parent.with_new_children(vec![MockTree::leaf(99)]);
        match result {
            Err(ValidateError::ChildCountMismatch { expected, got }) => {
                assert_eq!(expected, 2);
                assert_eq!(got, 1);
            }
            other => panic!("expected ChildCountMismatch, got {:?}", other),
        }
    }
}
