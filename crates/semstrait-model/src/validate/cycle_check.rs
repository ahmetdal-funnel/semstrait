//! Reference-graph cycle detection over the root-pool semantic entities.
//!
//! Per `19 §3.5`'s algorithm, but lifted to validate-time: cycles in the
//! reference graph between declared semantic entities are detectable
//! without bindings or physical-source schemas — the graph is a function
//! of authored `expr:` blocks alone.
//!
//! Algorithm: for each carrier (Dimension / Measure / Metric), build a
//! per-carrier reference graph from `name → set-of-referenced-names`,
//! then walk each strongly-connected component via iterative DFS. Any SCC
//! of size > 1 (or any self-loop) is a cycle and surfaces as
//! [`ValidateErrorKind::CyclicSemanticsReference`].
//!
//! We use plain DFS with three-color marking (white/grey/black) rather
//! than full Tarjan SCC — the algorithm simpler, finds the same cycles,
//! and only requires `O(V + E)` time on an entity graph that v1 caps in
//! the low thousands. The first cycle reached is reported per carrier
//! with the lex-smallest member rotated to the front for deterministic
//! diagnostics (`00 §9` I4).

use crate::entities::{Dimension, Measure, Metric};
use crate::error::validate::ValidateErrorKind;
use crate::expr_source::ExprSource;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};
use semstrait_ir::tree::Visitor;
use semstrait_ir::{Expr, SemanticLeaf};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

pub(super) fn check_all_cycles(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    check_carrier_cycles(
        "Dimension",
        &model.dimensions,
        |d: &Dimension| d.expr.as_ref(),
        diags,
    );
    check_carrier_cycles(
        "Measure",
        &model.measures,
        |m: &Measure| m.expr.as_ref(),
        diags,
    );
    check_carrier_cycles(
        "Metric",
        &model.metrics,
        |m: &Metric| m.expr.as_ref(),
        diags,
    );
}

fn check_carrier_cycles<T>(
    carrier: &'static str,
    pool: &BTreeMap<String, T>,
    get_expr: impl Fn(&T) -> Option<&ExprSource<SemanticLeaf>>,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    // Build adjacency: name → referenced names found in its `expr:` block.
    let graph: BTreeMap<&str, BTreeSet<String>> = pool
        .iter()
        .map(|(name, entity)| {
            let refs = match get_expr(entity) {
                Some(ExprSource::Block(expr)) => collect_references(expr),
                _ => BTreeSet::new(),
            };
            (name.as_str(), refs)
        })
        .collect();

    let mut visited: BTreeSet<String> = BTreeSet::new();
    for start in graph.keys() {
        if visited.contains(*start) {
            continue;
        }
        if let Some(cycle) = find_cycle_from(start, &graph, &mut visited) {
            diags.push(Diagnostic::new(ValidateErrorKind::CyclicSemanticsReference {
                carrier: carrier.to_owned(),
                cycle,
            }));
        }
    }
}

/// Iterative DFS with three-color marking. Returns the cycle (rotated to
/// start at the lex-smallest member) on first detection, else `None`.
fn find_cycle_from(
    start: &str,
    graph: &BTreeMap<&str, BTreeSet<String>>,
    visited: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    // grey = on the current DFS path; black = fully explored (in `visited`).
    let mut grey: BTreeSet<String> = BTreeSet::new();
    let mut path: Vec<String> = Vec::new();
    // Stack frames: (node, child_iter_index)
    let mut stack: Vec<(String, usize)> = vec![(start.to_owned(), 0)];
    grey.insert(start.to_owned());
    path.push(start.to_owned());

    while let Some((node, idx)) = stack.last().cloned() {
        let neighbors: Vec<String> = graph
            .get(node.as_str())
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();

        if idx >= neighbors.len() {
            stack.pop();
            grey.remove(&node);
            visited.insert(node.clone());
            path.pop();
            continue;
        }

        stack.last_mut().unwrap().1 += 1;
        let neighbor = &neighbors[idx];

        if grey.contains(neighbor) {
            let cycle_start = path.iter().position(|n| n == neighbor).unwrap();
            let cycle: Vec<String> = path[cycle_start..].to_vec();
            return Some(rotate_to_smallest(cycle));
        }
        if visited.contains(neighbor) {
            continue;
        }
        if graph.contains_key(neighbor.as_str()) {
            grey.insert(neighbor.clone());
            path.push(neighbor.clone());
            stack.push((neighbor.clone(), 0));
        }
    }
    None
}

/// Rotate a cycle so the lex-smallest member is first (deterministic
/// diagnostics per I4).
fn rotate_to_smallest(cycle: Vec<String>) -> Vec<String> {
    let min_idx = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, name)| (*name).clone())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut out: Vec<String> = cycle[min_idx..].to_vec();
    out.extend_from_slice(&cycle[..min_idx]);
    out
}

/// Walk an `Expr<SemanticLeaf>` and collect every referenced semantics
/// name (Field / Dimension / Measure / Metric / Key).
fn collect_references(expr: &Expr<SemanticLeaf>) -> BTreeSet<String> {
    use semstrait_ir::Tree;
    let mut collector = NameCollector { names: BTreeSet::new() };
    let _ = expr.apply(&mut collector);
    collector.names
}

struct NameCollector {
    names: BTreeSet<String>,
}

impl Visitor<Expr<SemanticLeaf>> for NameCollector {
    type Output = ();

    fn f_down(&mut self, node: &Expr<SemanticLeaf>) -> ControlFlow<()> {
        if let Expr::Leaf(leaf) = node {
            match leaf {
                SemanticLeaf::Field(n)
                | SemanticLeaf::Dimension { name: n, .. }
                | SemanticLeaf::Measure { name: n, .. }
                | SemanticLeaf::Metric { name: n, .. }
                | SemanticLeaf::Key { name: n, .. } => {
                    self.names.insert(n.0.clone());
                }
                SemanticLeaf::Literal(_) | SemanticLeaf::Column(_) => {}
                _ => {} // non_exhaustive — future leaf variants ignored here
            }
        }
        ControlFlow::Continue(())
    }

    fn f_up(&mut self, _node: &Expr<SemanticLeaf>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_ir::expr_kinds::SemanticsName;

    fn field_leaf(name: &str) -> Expr<SemanticLeaf> {
        Expr::Leaf(SemanticLeaf::Field(SemanticsName(name.to_owned())))
    }

    fn measure_leaf(name: &str) -> Expr<SemanticLeaf> {
        Expr::Leaf(SemanticLeaf::Measure {
            name: SemanticsName(name.to_owned()),
            accessor: None,
        })
    }

    #[test]
    fn collect_references_finds_all_kinds() {
        // BinaryOp(field("a"), Dimension("b"))
        let expr = Expr::BinaryOp {
            op: semstrait_ir::BinaryOpKind::Add,
            left: Box::new(field_leaf("a")),
            right: Box::new(Expr::Leaf(SemanticLeaf::Dimension {
                name: SemanticsName("b".into()),
                accessor: None,
            })),
        };
        let refs = collect_references(&expr);
        assert!(refs.contains("a"));
        assert!(refs.contains("b"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn collect_references_skips_literal_and_column() {
        let expr = Expr::BinaryOp {
            op: semstrait_ir::BinaryOpKind::Add,
            left: Box::new(Expr::Leaf(SemanticLeaf::Literal(
                semstrait_ir::Literal::Integer(1),
            ))),
            right: Box::new(Expr::Leaf(SemanticLeaf::Column(
                semstrait_ir::ColumnRef("c".into()),
            ))),
        };
        let refs = collect_references(&expr);
        assert!(refs.is_empty());
    }

    #[test]
    fn rotate_to_smallest_picks_lex_min() {
        let rotated = rotate_to_smallest(vec!["c".into(), "a".into(), "b".into()]);
        assert_eq!(rotated, vec!["a".to_string(), "b".into(), "c".into()]);
    }

    #[test]
    fn no_cycle_in_acyclic_graph() {
        let mut graph: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        graph.insert("a", ["b".to_owned()].into_iter().collect());
        graph.insert("b", ["c".to_owned()].into_iter().collect());
        graph.insert("c", BTreeSet::new());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        assert!(find_cycle_from("a", &graph, &mut visited).is_none());
    }

    #[test]
    fn detects_self_loop() {
        let mut graph: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        graph.insert("a", ["a".to_owned()].into_iter().collect());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let cycle = find_cycle_from("a", &graph, &mut visited).expect("should detect self-loop");
        assert_eq!(cycle, vec!["a".to_string()]);
    }

    #[test]
    fn detects_two_node_cycle() {
        let mut graph: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        graph.insert("a", ["b".to_owned()].into_iter().collect());
        graph.insert("b", ["a".to_owned()].into_iter().collect());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let cycle = find_cycle_from("a", &graph, &mut visited).expect("should detect 2-cycle");
        assert_eq!(cycle, vec!["a".to_string(), "b".into()]);
    }

    #[test]
    fn detects_three_node_cycle_rotated_to_smallest() {
        // c → a → b → c
        let mut graph: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        graph.insert("c", ["a".to_owned()].into_iter().collect());
        graph.insert("a", ["b".to_owned()].into_iter().collect());
        graph.insert("b", ["c".to_owned()].into_iter().collect());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let cycle = find_cycle_from("c", &graph, &mut visited).expect("should detect 3-cycle");
        assert_eq!(cycle, vec!["a".to_string(), "b".into(), "c".into()]);
    }

    #[test]
    fn ignores_cross_carrier_refs() {
        // graph only contains "a" — neighbor "b" is from another carrier
        let mut graph: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        graph.insert("a", ["b".to_owned()].into_iter().collect());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        assert!(find_cycle_from("a", &graph, &mut visited).is_none());
    }

    #[test]
    fn check_all_cycles_finds_metric_cycle() {
        use crate::entities::Metric;
        use crate::expr_source::ExprSource;
        use semstrait_core::DataType;

        // Build expr: metric "b" references metric "a"; metric "a" references metric "b"
        let mut model = SemanticModel {
            name: "m".to_owned(),
            ..Default::default()
        };
        model.metrics.insert(
            "a".to_owned(),
            Metric::builder("a")
                .data_type(DataType::Integer)
                .agg(crate::entities::AggregationType::Sum)
                .expr(ExprSource::Block(measure_leaf("b")))
                .build(),
        );
        model.metrics.insert(
            "b".to_owned(),
            Metric::builder("b")
                .data_type(DataType::Integer)
                .agg(crate::entities::AggregationType::Sum)
                .expr(ExprSource::Block(measure_leaf("a")))
                .build(),
        );

        let mut diags: Diagnostics<ValidateErrorKind> = Vec::new();
        check_all_cycles(&model, &mut diags);

        let cycle_diag = diags.iter().find_map(|d| match &d.kind {
            ValidateErrorKind::CyclicSemanticsReference { carrier, cycle } => {
                Some((carrier.clone(), cycle.clone()))
            }
            _ => None,
        });
        let (carrier, cycle) = cycle_diag.expect("expected one CyclicSemanticsReference");
        assert_eq!(carrier, "Metric");
        assert_eq!(cycle, vec!["a".to_string(), "b".into()]);
    }

    #[test]
    fn check_all_cycles_skips_acyclic_model() {
        use crate::entities::Metric;
        use crate::expr_source::ExprSource;
        use semstrait_core::DataType;

        // a → b (no cycle)
        let mut model = SemanticModel {
            name: "m".to_owned(),
            ..Default::default()
        };
        model.metrics.insert(
            "a".to_owned(),
            Metric::builder("a")
                .data_type(DataType::Integer)
                .agg(crate::entities::AggregationType::Sum)
                .expr(ExprSource::Block(measure_leaf("b")))
                .build(),
        );
        model.metrics.insert(
            "b".to_owned(),
            Metric::builder("b")
                .data_type(DataType::Integer)
                .agg(crate::entities::AggregationType::Sum)
                .build(),
        );

        let mut diags: Diagnostics<ValidateErrorKind> = Vec::new();
        check_all_cycles(&model, &mut diags);
        assert!(!diags
            .iter()
            .any(|d| matches!(&d.kind, ValidateErrorKind::CyclicSemanticsReference { .. })));
    }
}
