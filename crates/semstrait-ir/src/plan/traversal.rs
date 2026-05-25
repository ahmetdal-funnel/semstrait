//! `Tree` impl for [`PlanNode`]. Spec `35 §14`.
//!
//! Plugs `PlanNode` into the universal-traversal trait family in
//! [`crate::tree`]. Once `Tree` is implemented, the default-provided
//! `apply` / `transform` walkers come for free — see the `tree` module
//! doc for the contract.
//!
//! ## Child slot layout
//!
//! Per spec `35 §10`, each variant exposes a deterministic child order:
//!
//! | Variant   | Children                              | Arity |
//! |-----------|---------------------------------------|-------|
//! | `Scan`    | (no children)                         | 0     |
//! | `Values`  | (no children)                         | 0     |
//! | `Filter`  | `[input]`                             | 1     |
//! | `Project` | `[input]`                             | 1     |
//! | `Agg`     | `[input]`                             | 1     |
//! | `Sort`    | `[input]`                             | 1     |
//! | `Fetch`   | `[input]`                             | 1     |
//! | `Join`    | `[left, right]`                       | 2     |
//! | `Union`   | `inputs[..]`                          | n ≥ 0 |
//!
//! `with_new_children` raises [`ValidateError::ChildCountMismatch`] on
//! arity mismatch. Every variant preserves its non-tree state (Source,
//! predicate `PhysicalExpr`, JoinType, Cardinality, KeyPair list, …);
//! only the structural children are swapped.

use crate::error::ValidateError;
use crate::plan::node::PlanNode;
use crate::tree::Tree;

impl Tree for PlanNode {
    fn children(&self) -> Vec<&Self> {
        // Delegate to the inherent method on PlanNode (`§10.1`'s
        // `children` accessor). Same contract: deterministic order,
        // borrowed children.
        PlanNode::children(self)
    }

    fn with_new_children(self, new_children: Vec<Self>) -> Result<Self, ValidateError> {
        let expected = PlanNode::children(&self).len();
        let got = new_children.len();

        match self {
            Self::Scan(_) | Self::Values(_) => {
                if got != 0 {
                    return Err(ValidateError::ChildCountMismatch { expected: 0, got });
                }
                Ok(self)
            }
            Self::Filter(mut n) => {
                let [child] = take_one(new_children, expected)?;
                n.input = Box::new(child);
                Ok(Self::Filter(n))
            }
            Self::Project(mut n) => {
                let [child] = take_one(new_children, expected)?;
                n.input = Box::new(child);
                Ok(Self::Project(n))
            }
            Self::Agg(mut n) => {
                let [child] = take_one(new_children, expected)?;
                n.input = Box::new(child);
                Ok(Self::Agg(n))
            }
            Self::Sort(mut n) => {
                let [child] = take_one(new_children, expected)?;
                n.input = Box::new(child);
                Ok(Self::Sort(n))
            }
            Self::Fetch(mut n) => {
                let [child] = take_one(new_children, expected)?;
                n.input = Box::new(child);
                Ok(Self::Fetch(n))
            }
            Self::Join(mut n) => {
                let [left, right] = take_two(new_children, expected)?;
                n.left = Box::new(left);
                n.right = Box::new(right);
                Ok(Self::Join(n))
            }
            Self::Union(mut n) => {
                // `Union` is variadic — accept any arity. Caller is
                // responsible for keeping `inputs.len() >= 2` for
                // semantic well-formedness; structural traversal does
                // not enforce that here (`SemanticPlan::validate` does).
                n.inputs = new_children;
                Ok(Self::Union(n))
            }
        }
    }
}

fn take_one(mut new_children: Vec<PlanNode>, expected: usize) -> Result<[PlanNode; 1], ValidateError> {
    if new_children.len() != 1 {
        return Err(ValidateError::ChildCountMismatch {
            expected,
            got: new_children.len(),
        });
    }
    Ok([new_children.remove(0)])
}

fn take_two(mut new_children: Vec<PlanNode>, expected: usize) -> Result<[PlanNode; 2], ValidateError> {
    if new_children.len() != 2 {
        return Err(ValidateError::ChildCountMismatch {
            expected,
            got: new_children.len(),
        });
    }
    let right = new_children.remove(1);
    let left = new_children.remove(0);
    Ok([left, right])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::PhysicalLeaf;
    use crate::expr_kinds::{ColumnRef, Literal};
    use crate::plan::meta::{NodeId, NodeMeta};
    use crate::plan::node::{
        AggNode, FetchNode, FilterNode, JoinNode, ProjectNode, ScanNode, SortNode, UnionNode,
        ValuesNode,
    };
    use crate::primitives::{
        AggregateExpr, Cardinality, JoinType, KeyPair, Name, ResolvedColumn, SortDir, SourceRef,
    };
    use crate::tree::Visitor;
    use crate::types::{DataType, Schema, SchemaColumn};
    use crate::Expr;
    use crate::expr::PhysicalExpr;
    use crate::expr_kinds::AggregationOp;
    use std::ops::ControlFlow;
    use std::sync::Arc;

    // ── Helpers ──────────────────────────────────────────────────────

    fn schema(cols: &[(&str, DataType, bool)]) -> Arc<Schema> {
        Arc::new(Schema {
            columns: cols
                .iter()
                .map(|(n, d, nullable)| SchemaColumn {
                    name: n.to_string(),
                    data_type: d.clone(),
                    nullable: *nullable,
                })
                .collect(),
        })
    }

    fn meta_for(id: u128, s: Arc<Schema>) -> NodeMeta {
        NodeMeta::new(NodeId::from_raw(id), s)
    }

    fn col_leaf(n: &str) -> PhysicalExpr {
        Expr::Leaf(PhysicalLeaf::Column(ColumnRef(n.to_string())))
    }

    fn scan(id: u128) -> PlanNode {
        let s = schema(&[("x", DataType::Integer, false)]);
        PlanNode::Scan(ScanNode::new(
            meta_for(id, s),
            SourceRef::new("t"),
            vec![ResolvedColumn {
                name: Name::new("x").unwrap(),
                data_type: DataType::Integer,
                nullable: false,
                ordinal: 0,
            }],
        ))
    }

    fn filter_over(id: u128, input: PlanNode) -> PlanNode {
        let s = schema(&[("x", DataType::Integer, false)]);
        PlanNode::Filter(FilterNode::new(
            meta_for(id, s),
            input,
            Expr::Leaf(PhysicalLeaf::Literal(Literal::Boolean(true))),
        ))
    }

    fn join_over(id: u128, left: PlanNode, right: PlanNode) -> PlanNode {
        let s = schema(&[
            ("x", DataType::Integer, false),
            ("y", DataType::Integer, false),
        ]);
        PlanNode::Join(JoinNode::new(
            meta_for(id, s),
            left,
            right,
            JoinType::Inner,
            Cardinality::OneToOne,
            vec![KeyPair {
                left: Name::new("x").unwrap(),
                right: Name::new("y").unwrap(),
            }],
        ))
    }

    // ── Tree::children — round-trip with inherent method ────────────

    #[test]
    fn tree_children_matches_inherent_children() {
        let n = filter_over(2, scan(1));
        let inherent = PlanNode::children(&n);
        let trait_kids = <PlanNode as Tree>::children(&n);
        assert_eq!(inherent.len(), trait_kids.len());
    }

    // ── Tree::with_new_children — per-variant arity tests ──────────

    #[test]
    fn with_new_children_scan_rejects_any_children() {
        let n = scan(1);
        let err = n.with_new_children(vec![scan(99)]).unwrap_err();
        assert!(matches!(
            err,
            ValidateError::ChildCountMismatch { expected: 0, got: 1 }
        ));
    }

    #[test]
    fn with_new_children_values_rejects_any_children() {
        let s = Schema {
            columns: vec![SchemaColumn {
                name: "x".into(),
                data_type: DataType::Integer,
                nullable: false,
            }],
        };
        let n = PlanNode::Values(ValuesNode::new(
            meta_for(1, Arc::new(s.clone())),
            Vec::new(),
            s,
        ));
        let err = n.with_new_children(vec![scan(99)]).unwrap_err();
        assert!(matches!(
            err,
            ValidateError::ChildCountMismatch { expected: 0, got: 1 }
        ));
    }

    #[test]
    fn with_new_children_filter_swaps_input() {
        let original = filter_over(2, scan(1));
        let new_input = scan(99);
        let rebuilt = original.with_new_children(vec![new_input]).unwrap();
        // The new input's id propagates through.
        assert_eq!(
            rebuilt.children()[0].meta().node_id,
            NodeId::from_raw(99)
        );
    }

    #[test]
    fn with_new_children_filter_rejects_zero() {
        let original = filter_over(2, scan(1));
        let err = original.with_new_children(Vec::new()).unwrap_err();
        assert!(matches!(
            err,
            ValidateError::ChildCountMismatch { expected: 1, got: 0 }
        ));
    }

    #[test]
    fn with_new_children_filter_rejects_two() {
        let original = filter_over(2, scan(1));
        let err = original.with_new_children(vec![scan(98), scan(99)]).unwrap_err();
        assert!(matches!(
            err,
            ValidateError::ChildCountMismatch { expected: 1, got: 2 }
        ));
    }

    #[test]
    fn with_new_children_project_swaps_input() {
        let s = schema(&[("y", DataType::Integer, false)]);
        let original = PlanNode::Project(ProjectNode::new(
            meta_for(2, s),
            scan(1),
            vec![(Name::new("y").unwrap(), col_leaf("x"))],
        ));
        let rebuilt = original.with_new_children(vec![scan(99)]).unwrap();
        match rebuilt {
            PlanNode::Project(p) => {
                assert_eq!(p.input.meta().node_id, NodeId::from_raw(99));
                assert_eq!(p.projections.len(), 1, "projections must persist through rebuild");
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn with_new_children_agg_swaps_input() {
        let s = schema(&[("total", DataType::Long, false)]);
        let agg_expr = AggregateExpr {
            aggregation: AggregationOp::Sum,
            input_expr: col_leaf("amount"),
            distinct: false,
            filter: None,
            inferred_type: DataType::Long,
        };
        let original = PlanNode::Agg(AggNode::new(
            meta_for(2, s),
            scan(1),
            Vec::new(),
            vec![(Name::new("total").unwrap(), agg_expr)],
        ));
        let rebuilt = original.with_new_children(vec![scan(99)]).unwrap();
        match rebuilt {
            PlanNode::Agg(a) => {
                assert_eq!(a.input.meta().node_id, NodeId::from_raw(99));
                assert_eq!(a.aggregates.len(), 1);
            }
            _ => panic!("expected Agg"),
        }
    }

    #[test]
    fn with_new_children_join_swaps_left_and_right() {
        let original = join_over(3, scan(1), scan(2));
        let rebuilt = original.with_new_children(vec![scan(98), scan(99)]).unwrap();
        match rebuilt {
            PlanNode::Join(j) => {
                assert_eq!(j.left.meta().node_id, NodeId::from_raw(98));
                assert_eq!(j.right.meta().node_id, NodeId::from_raw(99));
                // Non-tree state survives the rebuild.
                assert_eq!(j.join_type, JoinType::Inner);
                assert_eq!(j.cardinality, Cardinality::OneToOne);
                assert_eq!(j.on.len(), 1);
            }
            _ => panic!("expected Join"),
        }
    }

    #[test]
    fn with_new_children_join_rejects_one() {
        let original = join_over(3, scan(1), scan(2));
        let err = original.with_new_children(vec![scan(99)]).unwrap_err();
        assert!(matches!(
            err,
            ValidateError::ChildCountMismatch { expected: 2, got: 1 }
        ));
    }

    #[test]
    fn with_new_children_join_rejects_three() {
        let original = join_over(3, scan(1), scan(2));
        let err = original
            .with_new_children(vec![scan(97), scan(98), scan(99)])
            .unwrap_err();
        assert!(matches!(
            err,
            ValidateError::ChildCountMismatch { expected: 2, got: 3 }
        ));
    }

    #[test]
    fn with_new_children_union_admits_variadic_arity() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let original = PlanNode::Union(UnionNode::new(
            meta_for(99, s),
            vec![scan(1), scan(2), scan(3)],
        ));
        // Union is variadic — go from 3 to 5 inputs, no mismatch.
        let rebuilt = original
            .with_new_children(vec![scan(10), scan(11), scan(12), scan(13), scan(14)])
            .unwrap();
        match rebuilt {
            PlanNode::Union(u) => {
                assert_eq!(u.inputs.len(), 5);
                assert!(!u.distinct, "non-tree state preserved");
            }
            _ => panic!("expected Union"),
        }
    }

    #[test]
    fn with_new_children_sort_swaps_input() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let original = PlanNode::Sort(SortNode::new(
            meta_for(2, s),
            scan(1),
            vec![(Name::new("x").unwrap(), SortDir::ASC_NULLS_LAST)],
        ));
        let rebuilt = original.with_new_children(vec![scan(99)]).unwrap();
        match rebuilt {
            PlanNode::Sort(srt) => {
                assert_eq!(srt.input.meta().node_id, NodeId::from_raw(99));
                assert_eq!(srt.order.len(), 1, "order keys persist through rebuild");
            }
            _ => panic!("expected Sort"),
        }
    }

    #[test]
    fn with_new_children_fetch_swaps_input() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let original = PlanNode::Fetch(FetchNode::new(
            meta_for(2, s),
            scan(1),
            Some(10),
            Some(5),
        ));
        let rebuilt = original.with_new_children(vec![scan(99)]).unwrap();
        match rebuilt {
            PlanNode::Fetch(f) => {
                assert_eq!(f.input.meta().node_id, NodeId::from_raw(99));
                assert_eq!(f.limit, Some(10));
                assert_eq!(f.offset, Some(5));
            }
            _ => panic!("expected Fetch"),
        }
    }

    // ── apply (default) — pre-order walk ────────────────────────────

    struct IdCollector {
        ids: Vec<u128>,
    }

    impl Visitor<PlanNode> for IdCollector {
        type Output = ();
        fn f_down(&mut self, n: &PlanNode) -> ControlFlow<()> {
            self.ids.push(n.meta().node_id.as_u128());
            ControlFlow::Continue(())
        }
        fn f_up(&mut self, _n: &PlanNode) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn apply_walks_pre_order_root_then_children() {
        // join(3) → [filter(2) → scan(1), scan(4)]
        let tree = join_over(3, filter_over(2, scan(1)), scan(4));
        let mut c = IdCollector { ids: Vec::new() };
        let _ = tree.apply(&mut c);
        assert_eq!(c.ids, vec![3, 2, 1, 4]);
    }

    #[test]
    fn apply_descends_into_union_inputs_in_order() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let tree = PlanNode::Union(UnionNode::new(
            meta_for(99, s),
            vec![scan(1), scan(2), scan(3)],
        ));
        let mut c = IdCollector { ids: Vec::new() };
        let _ = tree.apply(&mut c);
        assert_eq!(c.ids, vec![99, 1, 2, 3]);
    }

    // ── transform (default) — bottom-up rewrite ─────────────────────

    #[test]
    fn transform_identity_preserves_tree() {
        let tree = filter_over(2, scan(1));
        let original = tree.clone();
        let rebuilt = tree
            .transform(&mut |n| Ok::<_, ValidateError>(n))
            .unwrap();
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn transform_propagates_callback_error() {
        let tree = filter_over(2, scan(1));
        let result = tree.transform(&mut |n| {
            // Reject specifically the leaf scan.
            if n.children().is_empty() {
                Err(ValidateError::ChildCountMismatch { expected: 0, got: 99 })
            } else {
                Ok(n)
            }
        });
        assert!(matches!(
            result,
            Err(ValidateError::ChildCountMismatch { .. })
        ));
    }

    #[test]
    fn transform_visits_every_node_bottom_up() {
        // Bottom-up: leaf is rewritten first, then each ancestor with
        // its rewritten children. We count visits by mutating a counter
        // captured from the closure environment.
        let tree = join_over(3, filter_over(2, scan(1)), scan(4));
        let mut count = 0usize;
        let _ = tree
            .transform(&mut |n| {
                count += 1;
                Ok::<_, ValidateError>(n)
            })
            .unwrap();
        assert_eq!(count, 4, "every node visited exactly once");
    }
}
