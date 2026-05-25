//! [`SemanticPlan`] wrapper + structural [`SemanticPlan::validate`] walker.
//! Per spec `35 §9` / `§14.3`.
//!
//! ## What `validate()` checks
//!
//! Structural-only post-order walk. The 3-variant `IrErrorKind` set
//! ratified in P14 routes the spec §13 invariants to two production
//! sites:
//!
//! - [`IrErrorKind::StructuralViolation`] — shape rules with a stable
//!   `kind` discriminator. Variants currently produced:
//!   - `output_names_arity` (§9.2 invariant 1)
//!   - `project_empty` (§10.4 trivial-project rule)
//!   - `union_arity` (§13.6 — `inputs.len() >= 2`)
//!   - `union_schema_mismatch` (§13.6 — structural compatibility)
//!   - `join_empty_keys` (§13.5 — `on` non-empty)
//!   - `agg_duplicate_name` (§13.7 — unique output names)
//!   - `pass_through_schema` (§13.8 / §13.9 / §13.10)
//! - [`IrErrorKind::DanglingReference`] — column-name lookup against
//!   the corresponding child schema:
//!   - `agg.group_by` (§13.7)
//!   - `sort.order` (§13.8)
//!   - `join.on.{left,right}` (§13.5)
//!
//! ## What `validate()` does NOT check
//!
//! - Type-resolution checks (§13.1, §13.2) — deferred to the manifest
//!   layer per Override 3 (Q-PLAN-14).
//! - Predicate-Boolean type (§13.10's `FilterPredicateNotBoolean`) —
//!   same scoping; the manifest layer carries this diagnostic.
//! - Pushdown column resolution (§13.4) — same scoping.
//! - `Expr<L>`-level rules (no `Expr::Aggregate` in predicate slots,
//!   etc.) — those are `ValidateError` raised at construction by
//!   `Tree::with_new_children`, not `validate()`'s concern.
//!
//! First-violation-wins semantics. Ordering is documented as unstable
//! per spec §17.1 — consumers SHOULD treat any returned `Diagnostic` as
//! a "plan is bad" signal, not a "first problem is X" guarantee.

use semstrait_common::diagnostic::Diagnostic;

use crate::error::IrErrorKind;
use crate::plan::node::PlanNode;
use crate::primitives::Name;
use crate::types::{Schema, SchemaColumn};

// ── SemanticPlan ────────────────────────────────────────────────────────

/// The canonical, engine-agnostic query plan tree. Output of the planner
/// (`34`), input of every adapter (`36`). Per spec `35 §9`.
///
/// **v1 scoping (Q-PLAN-15, 2026-05-25).** The full §9 shape includes
/// a `diagnostics: Vec<PlanDiagnostic>` field for warning-severity
/// planner output. `PlanDiagnostic` belongs to the planner's vocabulary
/// (`34 §13.2`); it is not in v1 ir's scope. Adding the field is MINOR
/// per `30 §2.2` thanks to `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPlan {
    /// The root of the plan tree. Never empty; planning produces at
    /// least a `Scan` leaf.
    pub root: PlanNode,
    /// Output column names in the order they appear in `root`'s output
    /// schema. Length equals `root.meta().output_schema.len()`.
    pub output_names: Vec<Name>,
}

impl SemanticPlan {
    /// Construct a `SemanticPlan` from a root and the output-name list.
    /// Construction does NOT re-check invariants — see [`Self::validate`].
    pub fn new(root: PlanNode, output_names: Vec<Name>) -> Self {
        Self { root, output_names }
    }

    /// Full tree walk; re-checks the structural invariants in spec §13.
    /// Returns the first violation as a `Diagnostic<IrErrorKind>`;
    /// `Ok(())` on well-formedness.
    ///
    /// First-violation-wins is the spec contract (§17.1) — consumers
    /// MUST treat the returned diagnostic as a plan-is-bad signal,
    /// never a "first problem is X" guarantee.
    pub fn validate(&self) -> Result<(), Diagnostic<IrErrorKind>> {
        // §9.2 invariant 1 — output_names arity matches root schema.
        let root_cols = self.root.meta().output_schema.columns.len();
        if self.output_names.len() != root_cols {
            return Err(diag(IrErrorKind::StructuralViolation {
                kind: "output_names_arity",
                reason: format!(
                    "output_names.len() = {}, root output_schema columns = {}",
                    self.output_names.len(),
                    root_cols
                ),
            }));
        }
        // §13.* — recurse into the tree.
        validate_node(&self.root)
    }
}

// ── Recursive validator ────────────────────────────────────────────────

fn validate_node(node: &PlanNode) -> Result<(), Diagnostic<IrErrorKind>> {
    // Post-order: children first, then this node's own invariants.
    for child in node.children() {
        validate_node(child)?;
    }
    match node {
        PlanNode::Scan(_) | PlanNode::Values(_) => Ok(()),
        PlanNode::Filter(f) => check_pass_through("filter", &f.input, &node.meta().output_schema),
        PlanNode::Project(p) => {
            if p.projections.is_empty() {
                return Err(diag(IrErrorKind::StructuralViolation {
                    kind: "project_empty",
                    reason: "ProjectNode.projections is empty".to_string(),
                }));
            }
            Ok(())
        }
        PlanNode::Agg(a) => {
            let input_schema = &a.input.meta().output_schema;
            for name in &a.group_by {
                if !schema_has(input_schema, name) {
                    return Err(diag(IrErrorKind::DanglingReference {
                        node_kind: "agg",
                        name: name.clone(),
                        available: schema_names(input_schema),
                    }));
                }
            }
            // Unique aggregate output names.
            for i in 0..a.aggregates.len() {
                for j in (i + 1)..a.aggregates.len() {
                    if a.aggregates[i].0 == a.aggregates[j].0 {
                        return Err(diag(IrErrorKind::StructuralViolation {
                            kind: "agg_duplicate_name",
                            reason: format!(
                                "duplicate aggregate output name `{}` at indices {} and {}",
                                a.aggregates[i].0.as_str(),
                                i,
                                j
                            ),
                        }));
                    }
                }
            }
            Ok(())
        }
        PlanNode::Join(j) => {
            if j.on.is_empty() {
                return Err(diag(IrErrorKind::StructuralViolation {
                    kind: "join_empty_keys",
                    reason: "JoinNode.on is empty (cross-join unsupported in v1)".to_string(),
                }));
            }
            let left_schema = &j.left.meta().output_schema;
            let right_schema = &j.right.meta().output_schema;
            for kp in &j.on {
                if !schema_has(left_schema, &kp.left) {
                    return Err(diag(IrErrorKind::DanglingReference {
                        node_kind: "join",
                        name: kp.left.clone(),
                        available: schema_names(left_schema),
                    }));
                }
                if !schema_has(right_schema, &kp.right) {
                    return Err(diag(IrErrorKind::DanglingReference {
                        node_kind: "join",
                        name: kp.right.clone(),
                        available: schema_names(right_schema),
                    }));
                }
            }
            Ok(())
        }
        PlanNode::Union(u) => {
            if u.inputs.len() < 2 {
                return Err(diag(IrErrorKind::StructuralViolation {
                    kind: "union_arity",
                    reason: format!("UnionNode.inputs.len() = {}, expected >= 2", u.inputs.len()),
                }));
            }
            // Structural schema compatibility — same arity + same DataType
            // at each ordinal. Nullability widening is the planner's job;
            // we only check arity + types here.
            let head = &u.inputs[0].meta().output_schema;
            for (idx, input) in u.inputs.iter().enumerate().skip(1) {
                let here = &input.meta().output_schema;
                if !structurally_compatible(head, here) {
                    return Err(diag(IrErrorKind::StructuralViolation {
                        kind: "union_schema_mismatch",
                        reason: format!(
                            "input[0] arity/types disagree with input[{}]",
                            idx
                        ),
                    }));
                }
            }
            Ok(())
        }
        PlanNode::Sort(s) => {
            let input_schema = &s.input.meta().output_schema;
            for (name, _dir) in &s.order {
                if !schema_has(input_schema, name) {
                    return Err(diag(IrErrorKind::DanglingReference {
                        node_kind: "sort",
                        name: name.clone(),
                        available: schema_names(input_schema),
                    }));
                }
            }
            check_pass_through("sort", &s.input, &node.meta().output_schema)
        }
        PlanNode::Fetch(f) => check_pass_through("fetch", &f.input, &node.meta().output_schema),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn diag(kind: IrErrorKind) -> Diagnostic<IrErrorKind> {
    Diagnostic::new(kind)
}

/// `Filter` / `Sort` / `Fetch` must share output schema with their input.
/// Compared by structural equality on `Schema`, not by `Arc` identity —
/// the planner is encouraged to share via `Arc`, but a deep-equal schema
/// produced by hand is also valid.
fn check_pass_through(
    node_kind: &'static str,
    input: &PlanNode,
    here: &Schema,
) -> Result<(), Diagnostic<IrErrorKind>> {
    let upstream = &input.meta().output_schema;
    if **upstream == *here {
        return Ok(());
    }
    Err(diag(IrErrorKind::StructuralViolation {
        kind: "pass_through_schema",
        reason: format!(
            "{} must share output_schema with input (input cols: {}, this cols: {})",
            node_kind,
            upstream.columns.len(),
            here.columns.len()
        ),
    }))
}

fn structurally_compatible(a: &Schema, b: &Schema) -> bool {
    if a.columns.len() != b.columns.len() {
        return false;
    }
    a.columns
        .iter()
        .zip(b.columns.iter())
        .all(|(ca, cb): (&SchemaColumn, &SchemaColumn)| ca.data_type == cb.data_type)
}

fn schema_has(schema: &Schema, name: &Name) -> bool {
    schema.columns.iter().any(|c| c.name == name.as_str())
}

fn schema_names(schema: &Schema) -> Vec<Name> {
    schema
        .columns
        .iter()
        .filter_map(|c| Name::new(c.name.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{PhysicalExpr, PhysicalLeaf};
    use crate::expr_kinds::{AggregationOp, ColumnRef, Literal};
    use crate::plan::meta::{NodeId, NodeMeta};
    use crate::plan::node::{
        AggNode, FetchNode, FilterNode, JoinNode, ProjectNode, ScanNode, SortNode, UnionNode,
        ValuesNode,
    };
    use crate::primitives::{
        AggregateExpr, Cardinality, JoinType, KeyPair, ResolvedColumn, SortDir, SourceRef,
    };
    use crate::types::{DataType, SchemaColumn};
    use crate::Expr;
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

    fn scan_xy() -> PlanNode {
        let s = schema(&[
            ("x", DataType::Integer, false),
            ("y", DataType::Integer, false),
        ]);
        PlanNode::Scan(ScanNode::new(
            meta_for(1, s),
            SourceRef::new("t"),
            vec![
                ResolvedColumn {
                    name: Name::new("x").unwrap(),
                    data_type: DataType::Integer,
                    nullable: false,
                    ordinal: 0,
                },
                ResolvedColumn {
                    name: Name::new("y").unwrap(),
                    data_type: DataType::Integer,
                    nullable: false,
                    ordinal: 1,
                },
            ],
        ))
    }

    fn scan_with(id: u128, cols: &[(&str, DataType, bool)]) -> PlanNode {
        let s = schema(cols);
        let resolved = cols
            .iter()
            .enumerate()
            .map(|(i, (n, d, nullable))| ResolvedColumn {
                name: Name::new(*n).unwrap(),
                data_type: d.clone(),
                nullable: *nullable,
                ordinal: i as u32,
            })
            .collect();
        PlanNode::Scan(ScanNode::new(meta_for(id, s), SourceRef::new("t"), resolved))
    }

    fn ok_plan(root: PlanNode, names: &[&str]) -> SemanticPlan {
        SemanticPlan::new(
            root,
            names.iter().map(|n| Name::new(*n).unwrap()).collect(),
        )
    }

    // ── Output-names arity ──────────────────────────────────────────

    #[test]
    fn validate_rejects_output_names_arity_mismatch() {
        let plan = ok_plan(scan_xy(), &["x"]); // root has 2 cols, only 1 name
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "output_names_arity");
            }
            other => panic!("expected output_names_arity, got {:?}", other),
        }
    }

    #[test]
    fn validate_accepts_well_formed_scan() {
        let plan = ok_plan(scan_xy(), &["x", "y"]);
        plan.validate().expect("well-formed scan plan");
    }

    // ── Filter (pass-through) ──────────────────────────────────────

    #[test]
    fn validate_filter_with_shared_schema_is_ok() {
        let inner = scan_xy();
        let s = inner.meta().output_schema.clone();
        let filter = PlanNode::Filter(FilterNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            inner,
            Expr::Leaf(PhysicalLeaf::Literal(Literal::Boolean(true))),
        ));
        let plan = ok_plan(filter, &["x", "y"]);
        plan.validate().expect("filter with shared schema");
    }

    #[test]
    fn validate_filter_rejects_divergent_schema() {
        let inner = scan_xy();
        // Use a different schema for the Filter's meta.
        let mismatched = schema(&[("z", DataType::Integer, false)]);
        let filter = PlanNode::Filter(FilterNode::new(
            NodeMeta::new(NodeId::from_raw(2), mismatched),
            inner,
            Expr::Leaf(PhysicalLeaf::Literal(Literal::Boolean(true))),
        ));
        let plan = ok_plan(filter, &["z"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "pass_through_schema");
            }
            other => panic!("expected pass_through_schema, got {:?}", other),
        }
    }

    // ── Project (non-empty) ────────────────────────────────────────

    #[test]
    fn validate_rejects_empty_project() {
        let proj = PlanNode::Project(ProjectNode::new(
            NodeMeta::new(NodeId::from_raw(2), Arc::new(Schema { columns: vec![] })),
            scan_xy(),
            Vec::new(),
        ));
        let plan = ok_plan(proj, &[]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "project_empty");
            }
            other => panic!("expected project_empty, got {:?}", other),
        }
    }

    #[test]
    fn validate_accepts_well_formed_project() {
        let s = schema(&[("y", DataType::Integer, false)]);
        let proj = PlanNode::Project(ProjectNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            scan_xy(),
            vec![(Name::new("y").unwrap(), col_leaf("x"))],
        ));
        let plan = ok_plan(proj, &["y"]);
        plan.validate().expect("non-empty project");
    }

    // ── Agg (group_by + duplicate names) ───────────────────────────

    #[test]
    fn validate_agg_rejects_dangling_group_by() {
        let s = schema(&[("zzz", DataType::Long, false)]);
        let agg = PlanNode::Agg(AggNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            scan_xy(),
            vec![Name::new("zzz").unwrap()], // not in scan_xy()
            Vec::new(),
        ));
        let plan = ok_plan(agg, &["zzz"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::DanglingReference {
                node_kind, name, ..
            } => {
                assert_eq!(node_kind, "agg");
                assert_eq!(name.as_str(), "zzz");
            }
            other => panic!("expected DanglingReference[agg], got {:?}", other),
        }
    }

    #[test]
    fn validate_agg_rejects_duplicate_aggregate_output_name() {
        let s = schema(&[("total", DataType::Long, false)]);
        let agg_expr = || AggregateExpr {
            aggregation: AggregationOp::Sum,
            input_expr: col_leaf("x"),
            distinct: false,
            filter: None,
            inferred_type: DataType::Long,
        };
        let agg = PlanNode::Agg(AggNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            scan_xy(),
            Vec::new(),
            vec![
                (Name::new("total").unwrap(), agg_expr()),
                (Name::new("total").unwrap(), agg_expr()),
            ],
        ));
        let plan = ok_plan(agg, &["total"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "agg_duplicate_name");
            }
            other => panic!("expected agg_duplicate_name, got {:?}", other),
        }
    }

    // ── Join (empty keys, dangling refs) ──────────────────────────

    #[test]
    fn validate_join_rejects_empty_keys() {
        let s = schema(&[
            ("x", DataType::Integer, false),
            ("y", DataType::Integer, false),
        ]);
        let join = PlanNode::Join(JoinNode::new(
            NodeMeta::new(NodeId::from_raw(3), s),
            scan_with(10, &[("x", DataType::Integer, false)]),
            scan_with(11, &[("y", DataType::Integer, false)]),
            JoinType::Inner,
            Cardinality::OneToOne,
            Vec::new(),
        ));
        let plan = ok_plan(join, &["x", "y"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "join_empty_keys");
            }
            other => panic!("expected join_empty_keys, got {:?}", other),
        }
    }

    #[test]
    fn validate_join_rejects_dangling_left_key() {
        let s = schema(&[
            ("x", DataType::Integer, false),
            ("y", DataType::Integer, false),
        ]);
        let join = PlanNode::Join(JoinNode::new(
            NodeMeta::new(NodeId::from_raw(3), s),
            scan_with(10, &[("x", DataType::Integer, false)]),
            scan_with(11, &[("y", DataType::Integer, false)]),
            JoinType::Inner,
            Cardinality::OneToOne,
            vec![KeyPair {
                left: Name::new("missing").unwrap(),
                right: Name::new("y").unwrap(),
            }],
        ));
        let plan = ok_plan(join, &["x", "y"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::DanglingReference { node_kind, name, .. } => {
                assert_eq!(node_kind, "join");
                assert_eq!(name.as_str(), "missing");
            }
            other => panic!("expected DanglingReference[join], got {:?}", other),
        }
    }

    #[test]
    fn validate_join_well_formed() {
        let s = schema(&[
            ("x", DataType::Integer, false),
            ("y", DataType::Integer, false),
        ]);
        let join = PlanNode::Join(JoinNode::new(
            NodeMeta::new(NodeId::from_raw(3), s),
            scan_with(10, &[("x", DataType::Integer, false)]),
            scan_with(11, &[("y", DataType::Integer, false)]),
            JoinType::Inner,
            Cardinality::OneToOne,
            vec![KeyPair {
                left: Name::new("x").unwrap(),
                right: Name::new("y").unwrap(),
            }],
        ));
        let plan = ok_plan(join, &["x", "y"]);
        plan.validate().expect("well-formed join");
    }

    // ── Union (arity, schema compat) ──────────────────────────────

    #[test]
    fn validate_union_rejects_arity_one() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let u = PlanNode::Union(UnionNode::new(
            NodeMeta::new(NodeId::from_raw(99), s),
            vec![scan_with(10, &[("x", DataType::Integer, false)])],
        ));
        let plan = ok_plan(u, &["x"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "union_arity");
            }
            other => panic!("expected union_arity, got {:?}", other),
        }
    }

    #[test]
    fn validate_union_rejects_schema_mismatch() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let u = PlanNode::Union(UnionNode::new(
            NodeMeta::new(NodeId::from_raw(99), s),
            vec![
                scan_with(10, &[("x", DataType::Integer, false)]),
                scan_with(11, &[("x", DataType::String, false)]), // type mismatch
            ],
        ));
        let plan = ok_plan(u, &["x"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "union_schema_mismatch");
            }
            other => panic!("expected union_schema_mismatch, got {:?}", other),
        }
    }

    #[test]
    fn validate_union_well_formed() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let u = PlanNode::Union(UnionNode::new(
            NodeMeta::new(NodeId::from_raw(99), s),
            vec![
                scan_with(10, &[("x", DataType::Integer, false)]),
                scan_with(11, &[("x", DataType::Integer, false)]),
                scan_with(12, &[("x", DataType::Integer, false)]),
            ],
        ));
        let plan = ok_plan(u, &["x"]);
        plan.validate().expect("well-formed union");
    }

    // ── Sort (dangling ref + pass-through) ────────────────────────

    #[test]
    fn validate_sort_rejects_dangling_key() {
        let inner = scan_xy();
        let s = inner.meta().output_schema.clone();
        let sort = PlanNode::Sort(SortNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            inner,
            vec![(Name::new("zzz").unwrap(), SortDir::ASC_NULLS_LAST)],
        ));
        let plan = ok_plan(sort, &["x", "y"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::DanglingReference { node_kind, name, .. } => {
                assert_eq!(node_kind, "sort");
                assert_eq!(name.as_str(), "zzz");
            }
            other => panic!("expected DanglingReference[sort], got {:?}", other),
        }
    }

    #[test]
    fn validate_sort_well_formed() {
        let inner = scan_xy();
        let s = inner.meta().output_schema.clone();
        let sort = PlanNode::Sort(SortNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            inner,
            vec![(Name::new("x").unwrap(), SortDir::ASC_NULLS_LAST)],
        ));
        let plan = ok_plan(sort, &["x", "y"]);
        plan.validate().expect("well-formed sort");
    }

    // ── Fetch (pass-through) ──────────────────────────────────────

    #[test]
    fn validate_fetch_well_formed() {
        let inner = scan_xy();
        let s = inner.meta().output_schema.clone();
        let fetch = PlanNode::Fetch(FetchNode::new(
            NodeMeta::new(NodeId::from_raw(2), s),
            inner,
            Some(10),
            Some(0),
        ));
        let plan = ok_plan(fetch, &["x", "y"]);
        plan.validate().expect("well-formed fetch");
    }

    #[test]
    fn validate_fetch_rejects_divergent_schema() {
        let inner = scan_xy();
        let mismatched = schema(&[("nope", DataType::Integer, false)]);
        let fetch = PlanNode::Fetch(FetchNode::new(
            NodeMeta::new(NodeId::from_raw(2), mismatched),
            inner,
            Some(10),
            None,
        ));
        let plan = ok_plan(fetch, &["nope"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(kind, "pass_through_schema");
            }
            other => panic!("expected pass_through_schema, got {:?}", other),
        }
    }

    // ── Values (no recursion needed; trivial well-formedness) ────

    #[test]
    fn validate_values_node_is_leaf_well_formed() {
        let s = Schema {
            columns: vec![SchemaColumn {
                name: "x".into(),
                data_type: DataType::Integer,
                nullable: false,
            }],
        };
        let node = PlanNode::Values(ValuesNode::new(
            meta_for(1, Arc::new(s.clone())),
            vec![vec![Expr::Leaf(PhysicalLeaf::Literal(Literal::Integer(1)))]],
            s,
        ));
        let plan = ok_plan(node, &["x"]);
        plan.validate().expect("values is a well-formed leaf");
    }

    // ── Post-order: child errors surface before parent errors ────

    #[test]
    fn validate_post_order_surfaces_child_violation_first() {
        // Build: filter(div_schema) over an empty Project — the Project
        // violation must surface before the parent filter's own pass-
        // through check.
        let s = schema(&[("x", DataType::Integer, false)]);
        let bad_proj = PlanNode::Project(ProjectNode::new(
            NodeMeta::new(NodeId::from_raw(2), Arc::new(Schema { columns: vec![] })),
            scan_with(1, &[("x", DataType::Integer, false)]),
            Vec::new(), // empty — project_empty
        ));
        let filter = PlanNode::Filter(FilterNode::new(
            NodeMeta::new(NodeId::from_raw(3), s),
            bad_proj,
            Expr::Leaf(PhysicalLeaf::Literal(Literal::Boolean(true))),
        ));
        let plan = ok_plan(filter, &["x"]);
        let err = plan.validate().unwrap_err();
        match err.kind {
            IrErrorKind::StructuralViolation { kind, .. } => {
                assert_eq!(
                    kind, "project_empty",
                    "child violation must surface before parent"
                );
            }
            other => panic!("expected project_empty, got {:?}", other),
        }
    }
}
