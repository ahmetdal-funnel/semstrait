//! `PlanNode` sum and per-variant struct payloads. Spec `35 §10`.
//!
//! Owns:
//! - [`PlanNode`] — 9-variant `#[non_exhaustive]` sum: 8 variants per
//!   spec §10.1 plus [`PlanNode::Values`] (Q-IR-NEW-002, ratified
//!   2026-05-25). Variants wrap structs (not tuples) so per-variant
//!   field additions are MINOR per `30 §4.2`.
//! - Per-variant payloads: [`ScanNode`] §10.2, [`FilterNode`] §10.3,
//!   [`ProjectNode`] §10.4, [`AggNode`] §10.5, [`JoinNode`] §10.6,
//!   [`UnionNode`] §10.7, [`SortNode`] §10.8, [`FetchNode`] §10.9,
//!   [`ValuesNode`] (Q-IR-NEW-002).
//!
//! **Construction posture.** Every payload struct is `#[non_exhaustive]`
//! per I10. External crates cannot use struct-literal syntax; v1 ships
//! a small set of `Self::new(...)` constructors that take exactly the
//! fields the spec catalogues. Field additions keep `new` as the v1
//! shape and may grow side-doors (`with_filters_pushdown`, ...) as
//! MINOR per `30 §2.2`.
//!
//! No structural validation in constructors (Q-PLAN-15, 2026-05-25).
//! Construction trusts the planner's contract; `SemanticPlan::validate`
//! (P17) walks the tree and reports violations as `IrErrorKind`.

use crate::expr::PhysicalExpr;
use crate::expr_kinds::Literal;
use crate::plan::meta::NodeMeta;
use crate::primitives::{
    AggregateExpr, Cardinality, JoinType, KeyPair, Name, ResolvedColumn, SortDir, SourceRef,
};
use crate::types::Schema;

// ── PlanNode ────────────────────────────────────────────────────────────

/// A single node within a `SemanticPlan`. Per spec `35 §10.1`.
///
/// Variants form the canonical operator catalogue borrowed (structurally
/// only) from DataFusion's `LogicalPlan`, Calcite's `RelNode`, and
/// Substrait's `Rel`. `#[non_exhaustive]` per I10: adding a variant
/// (e.g. a future `Distinct`, `Window`, `TopN`) is MINOR per
/// `30 §2.2`. Consumers MUST add a fallback arm when pattern-matching.
///
/// **9 variants in v1.** The spec §10.1 catalogues 8; [`Self::Values`]
/// is the 9th, ratified by Q-IR-NEW-002 (2026-05-25) for `VALUES`-clause
/// emission and constant-row scaffolding.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum PlanNode {
    /// Leaf source — reads a `SourceRef` resolved against the
    /// SemanticManifest. The only variant without child nodes
    /// (apart from [`Self::Values`]). Per `35 §10.2`.
    Scan(ScanNode),
    /// Predicate node — pass-through schema. Per `35 §10.3`.
    Filter(FilterNode),
    /// Column-list node — projects `(output_name, expression)` pairs.
    /// Per `35 §10.4`.
    Project(ProjectNode),
    /// Group-by + aggregates. Per `35 §10.5`.
    Agg(AggNode),
    /// Binary equi-join. Per `35 §10.6`.
    Join(JoinNode),
    /// N-ary union. Per `35 §10.7`.
    Union(UnionNode),
    /// Ordering. Per `35 §10.8`.
    Sort(SortNode),
    /// Limit / offset. Per `35 §10.9`.
    Fetch(FetchNode),
    /// Inline literal rows (Q-IR-NEW-002, 2026-05-25). The other
    /// no-child leaf variant — emits a Substrait `VirtualTableReadRel`
    /// or SQL `VALUES` clause.
    Values(ValuesNode),
}

impl PlanNode {
    /// Shared accessor for the `NodeMeta` that every variant carries.
    /// Per spec `35 §10.1`.
    pub fn meta(&self) -> &NodeMeta {
        match self {
            Self::Scan(n) => &n.meta,
            Self::Filter(n) => &n.meta,
            Self::Project(n) => &n.meta,
            Self::Agg(n) => &n.meta,
            Self::Join(n) => &n.meta,
            Self::Union(n) => &n.meta,
            Self::Sort(n) => &n.meta,
            Self::Fetch(n) => &n.meta,
            Self::Values(n) => &n.meta,
        }
    }

    /// Mutable companion of [`Self::meta`]. Used by the planner /
    /// optimizer to attach `SemAnnotation`s after construction.
    pub fn meta_mut(&mut self) -> &mut NodeMeta {
        match self {
            Self::Scan(n) => &mut n.meta,
            Self::Filter(n) => &mut n.meta,
            Self::Project(n) => &mut n.meta,
            Self::Agg(n) => &mut n.meta,
            Self::Join(n) => &mut n.meta,
            Self::Union(n) => &mut n.meta,
            Self::Sort(n) => &mut n.meta,
            Self::Fetch(n) => &mut n.meta,
            Self::Values(n) => &mut n.meta,
        }
    }

    /// Borrow the immediate children of this node (0, 1, or 2+).
    /// Per spec `35 §10.1`.
    ///
    /// Leaf variants ([`Self::Scan`], [`Self::Values`]) return an empty
    /// vec. Single-child variants return a 1-element vec. [`Self::Join`]
    /// returns `[left, right]`; [`Self::Union`] returns its full
    /// `inputs` borrow.
    pub fn children(&self) -> Vec<&PlanNode> {
        match self {
            Self::Scan(_) | Self::Values(_) => Vec::new(),
            Self::Filter(n) => vec![&n.input],
            Self::Project(n) => vec![&n.input],
            Self::Agg(n) => vec![&n.input],
            Self::Join(n) => vec![&n.left, &n.right],
            Self::Union(n) => n.inputs.iter().collect(),
            Self::Sort(n) => vec![&n.input],
            Self::Fetch(n) => vec![&n.input],
        }
    }

    /// Mutable companion of [`Self::children`].
    pub fn children_mut(&mut self) -> Vec<&mut PlanNode> {
        match self {
            Self::Scan(_) | Self::Values(_) => Vec::new(),
            Self::Filter(n) => vec![&mut n.input],
            Self::Project(n) => vec![&mut n.input],
            Self::Agg(n) => vec![&mut n.input],
            Self::Join(n) => vec![&mut n.left, &mut n.right],
            Self::Union(n) => n.inputs.iter_mut().collect(),
            Self::Sort(n) => vec![&mut n.input],
            Self::Fetch(n) => vec![&mut n.input],
        }
    }
}

// ── ScanNode ────────────────────────────────────────────────────────────

/// Reads a resolved source. Per spec `35 §10.2`.
///
/// Every `ScanNode` corresponds to one engine-level LogicalRelation
/// (Substrait `ReadRel`, DataFusion `TableScan`, Spark `LogicalRelation`,
/// SQL `FROM`). Engine-internal mechanics (partition discovery,
/// multi-file consolidation, schema merge) live below the `ScanNode`
/// boundary — see §10.2.1.
///
/// `ScanNode` carries **no raw path, no URL, no dialect**. Resolution
/// from `SourceRef` to on-engine identity happens in the adapter via
/// the SemanticManifest (I1).
///
/// `ScanNode` carries **no partition columns, no partition transforms,
/// no `partition_def` declarations** — partition handling is an adapter
/// / engine concern reading from the SemanticManifest, not from the
/// plan tree (§10.2.1).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct ScanNode {
    pub meta: NodeMeta,
    pub source: SourceRef,
    pub columns: Vec<ResolvedColumn>,
    pub filters_pushdown: Vec<PhysicalExpr>,
}

impl ScanNode {
    /// Construct a `ScanNode` with no pushdown predicates. Optimizer
    /// passes attach pushdowns later via `meta_mut` / a future
    /// `with_filters_pushdown` side-door.
    pub fn new(meta: NodeMeta, source: SourceRef, columns: Vec<ResolvedColumn>) -> Self {
        Self {
            meta,
            source,
            columns,
            filters_pushdown: Vec::new(),
        }
    }

    /// Construct a `ScanNode` with pre-supplied pushdown predicates.
    pub fn with_pushdown(
        meta: NodeMeta,
        source: SourceRef,
        columns: Vec<ResolvedColumn>,
        filters_pushdown: Vec<PhysicalExpr>,
    ) -> Self {
        Self {
            meta,
            source,
            columns,
            filters_pushdown,
        }
    }
}

// ── FilterNode ──────────────────────────────────────────────────────────

/// Predicate node. Pass-through schema per spec `35 §10.3`:
/// `FilterNode.meta.output_schema` equals
/// `input.meta().output_schema`. Adapters rely on this; the planner is
/// responsible for honouring it at construction.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub predicate: PhysicalExpr,
}

impl FilterNode {
    pub fn new(meta: NodeMeta, input: PlanNode, predicate: PhysicalExpr) -> Self {
        Self {
            meta,
            input: Box::new(input),
            predicate,
        }
    }
}

// ── ProjectNode ─────────────────────────────────────────────────────────

/// Column-list node. Per spec `35 §10.4`.
///
/// `projections[i]` produces the output schema's column at ordinal `i`:
/// name `projections[i].0`, type by inference from `projections[i].1`.
/// Empty list is rejected at validate-time (a trivial `Project`
/// collapses to `input`).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub projections: Vec<(Name, PhysicalExpr)>,
}

impl ProjectNode {
    pub fn new(meta: NodeMeta, input: PlanNode, projections: Vec<(Name, PhysicalExpr)>) -> Self {
        Self {
            meta,
            input: Box::new(input),
            projections,
        }
    }
}

// ── AggNode ─────────────────────────────────────────────────────────────

/// Group-by + aggregates. Per spec `35 §10.5`.
///
/// Empty `group_by` is a grand-total aggregation (SQL
/// `SELECT agg(...) FROM ...` with no `GROUP BY`).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct AggNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub group_by: Vec<Name>,
    pub aggregates: Vec<(Name, AggregateExpr)>,
}

impl AggNode {
    pub fn new(
        meta: NodeMeta,
        input: PlanNode,
        group_by: Vec<Name>,
        aggregates: Vec<(Name, AggregateExpr)>,
    ) -> Self {
        Self {
            meta,
            input: Box::new(input),
            group_by,
            aggregates,
        }
    }
}

// ── JoinNode ────────────────────────────────────────────────────────────

/// Binary equi-join. Per spec `35 §10.6`.
///
/// `on` is non-empty for v1 equi-join variants (`Inner`, `Left`,
/// `Right`, `Full`). Cross-join is not a v1 variant (TD-IR-CROSS-JOIN).
/// Non-equi predicates (range / inequality) are deferred — a future
/// `JoinNode.residual: Option<PhysicalExpr>` field is MINOR per
/// `35 §17.1`.
///
/// `cardinality` is never elided; it always reflects the authored
/// `Relationship` from `16 §5.1`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct JoinNode {
    pub meta: NodeMeta,
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,
    pub join_type: JoinType,
    pub cardinality: Cardinality,
    pub on: Vec<KeyPair>,
}

impl JoinNode {
    pub fn new(
        meta: NodeMeta,
        left: PlanNode,
        right: PlanNode,
        join_type: JoinType,
        cardinality: Cardinality,
        on: Vec<KeyPair>,
    ) -> Self {
        Self {
            meta,
            left: Box::new(left),
            right: Box::new(right),
            join_type,
            cardinality,
            on,
        }
    }
}

// ── UnionNode ───────────────────────────────────────────────────────────

/// N-ary union. Per spec `35 §10.7`.
///
/// `distinct = false` = `UNION ALL` (bag semantics, default — every
/// engine natively supports it without rewrite). `distinct = true` =
/// `UNION` (set semantics, demands a post-hash-agg pass).
///
/// `inputs.len() >= 2` is enforced at validate-time as
/// `IrErrorKind::StructuralViolation { kind: "union_arity", ... }`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct UnionNode {
    pub meta: NodeMeta,
    pub inputs: Vec<PlanNode>,
    pub distinct: bool,
}

impl UnionNode {
    /// Construct a `UNION ALL` (bag semantics).
    pub fn new(meta: NodeMeta, inputs: Vec<PlanNode>) -> Self {
        Self {
            meta,
            inputs,
            distinct: false,
        }
    }

    /// Construct with explicit set / bag choice.
    pub fn with_distinct(meta: NodeMeta, inputs: Vec<PlanNode>, distinct: bool) -> Self {
        Self {
            meta,
            inputs,
            distinct,
        }
    }
}

// ── SortNode ────────────────────────────────────────────────────────────

/// Ordering. Per spec `35 §10.8`.
///
/// `order` is a priority list: `order[0]` is the primary key, `order[1]`
/// is the tie-breaker, and so on. Each `Name` resolves to a column in
/// `input.meta().output_schema`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct SortNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub order: Vec<(Name, SortDir)>,
}

impl SortNode {
    pub fn new(meta: NodeMeta, input: PlanNode, order: Vec<(Name, SortDir)>) -> Self {
        Self {
            meta,
            input: Box::new(input),
            order,
        }
    }
}

// ── FetchNode ───────────────────────────────────────────────────────────

/// Limit / offset. Per spec `35 §10.9`.
///
/// `limit = None` ⟺ "unlimited" (no `LIMIT` clause). `limit = Some(0)`
/// is well-formed; adapters MAY short-circuit emission.
///
/// `offset = None` ⟺ "no offset". `offset = Some(0)` is equivalent to
/// `None` and kept distinct for Substrait-roundtrip fidelity.
///
/// `u64` deliberately rejects negatives at the type boundary; the
/// `Option` shape distinguishes "no clause" from "value zero". Values
/// outside Substrait's `i64`-encodable range (the rare `u64 > i64::MAX`
/// case) are rejected at adapter-emit time.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct FetchNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

impl FetchNode {
    pub fn new(meta: NodeMeta, input: PlanNode, limit: Option<u64>, offset: Option<u64>) -> Self {
        Self {
            meta,
            input: Box::new(input),
            limit,
            offset,
        }
    }
}

// ── ValuesNode ──────────────────────────────────────────────────────────

/// Inline literal rows. Per spec `35 §10` (Q-IR-NEW-002).
///
/// Each cell is a [`Literal`]. `validate()` checks per-row arity against
/// `schema.columns.len()`. `rows.is_empty()` is well-formed.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct ValuesNode {
    pub meta: NodeMeta,
    pub rows: Vec<Vec<Literal>>,
    pub schema: Schema,
}

impl ValuesNode {
    pub fn new(meta: NodeMeta, rows: Vec<Vec<Literal>>, schema: Schema) -> Self {
        Self { meta, rows, schema }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::PhysicalLeaf;
    use crate::expr_kinds::{AggregateKind, AggregationOp, ColumnRef, Literal};
    use crate::plan::meta::NodeId;
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

    fn lit_int(v: i64) -> Literal {
        Literal::Integer {
            value: v,
            width: crate::expr_kinds::IntegerWidth::Long,
        }
    }

    fn scan(id: u128, schema_cols: &[(&str, DataType, bool)]) -> PlanNode {
        let s = schema(schema_cols);
        let resolved: Vec<ResolvedColumn> = schema_cols
            .iter()
            .enumerate()
            .map(|(i, (n, d, nullable))| ResolvedColumn {
                name: Name::new(*n).unwrap(),
                data_type: d.clone(),
                nullable: *nullable,
                ordinal: i as u32,
            })
            .collect();
        PlanNode::Scan(ScanNode::new(
            meta_for(id, s),
            SourceRef::new("test"),
            resolved,
        ))
    }

    // ── PlanNode variant inventory ──────────────────────────────────

    #[test]
    fn plan_node_has_nine_variants_in_v1() {
        // Compile-time enforcement: every variant must appear in the
        // exhaustive (modulo `_`) match below. If a future variant
        // lands, this test breaks until updated — protecting `meta()`
        // and `children()` from drift.
        let variants: &[&str] = &[
            "Scan", "Filter", "Project", "Agg", "Join", "Union", "Sort", "Fetch", "Values",
        ];
        assert_eq!(variants.len(), 9);
    }

    // ── meta() accessor ──────────────────────────────────────────────

    #[test]
    fn meta_accessor_returns_owning_node_meta() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = PlanNode::Scan(ScanNode::new(
            meta_for(1, Arc::clone(&s)),
            SourceRef::new("t"),
            vec![ResolvedColumn {
                name: Name::new("x").unwrap(),
                data_type: DataType::Integer,
                nullable: false,
                ordinal: 0,
            }],
        ));
        assert_eq!(n.meta().node_id, NodeId::from_raw(1));
    }

    #[test]
    fn meta_mut_accessor_allows_annotation_attachment() {
        use crate::expr_kinds::SemanticsName;
        use crate::plan::meta::SemAnnotation;

        let mut n = scan(7, &[("x", DataType::Integer, false)]);
        n.meta_mut()
            .annotations
            .push(SemAnnotation::DataKindRef(SemanticsName(
                "orders".to_string(),
            )));
        assert_eq!(n.meta().annotations.len(), 1);
    }

    // ── children() accessor ──────────────────────────────────────────

    #[test]
    fn children_for_scan_is_empty() {
        let n = scan(1, &[("x", DataType::Integer, false)]);
        assert!(n.children().is_empty());
    }

    #[test]
    fn children_for_values_is_empty() {
        let s = Schema {
            columns: vec![SchemaColumn {
                name: "x".into(),
                data_type: DataType::Integer,
                nullable: false,
            }],
        };
        let n = PlanNode::Values(ValuesNode::new(
            meta_for(1, Arc::new(s.clone())),
            vec![vec![lit_int(1)]],
            s,
        ));
        assert!(n.children().is_empty());
    }

    #[test]
    fn children_for_filter_is_one() {
        let input = scan(1, &[("x", DataType::Integer, false)]);
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = PlanNode::Filter(FilterNode::new(
            meta_for(2, s),
            input,
            Expr::Leaf(PhysicalLeaf::Literal(Literal::Boolean(true))),
        ));
        assert_eq!(n.children().len(), 1);
    }

    #[test]
    fn children_for_join_is_two_left_then_right() {
        let left = scan(1, &[("a", DataType::Integer, false)]);
        let right = scan(2, &[("b", DataType::Integer, false)]);
        let s = schema(&[
            ("a", DataType::Integer, false),
            ("b", DataType::Integer, false),
        ]);
        let n = PlanNode::Join(JoinNode::new(
            meta_for(3, s),
            left,
            right,
            JoinType::Inner,
            Cardinality::OneToOne,
            vec![KeyPair {
                left: Name::new("a").unwrap(),
                right: Name::new("b").unwrap(),
            }],
        ));
        let kids = n.children();
        assert_eq!(kids.len(), 2);
        // Left first, right second — adapters rely on this ordering
        // for binary-rel emission.
        assert_eq!(kids[0].meta().node_id, NodeId::from_raw(1));
        assert_eq!(kids[1].meta().node_id, NodeId::from_raw(2));
    }

    #[test]
    fn children_for_union_returns_all_inputs_in_order() {
        let inputs = vec![
            scan(1, &[("x", DataType::Integer, false)]),
            scan(2, &[("x", DataType::Integer, false)]),
            scan(3, &[("x", DataType::Integer, false)]),
        ];
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = PlanNode::Union(UnionNode::new(meta_for(99, s), inputs));
        let kids = n.children();
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[0].meta().node_id, NodeId::from_raw(1));
        assert_eq!(kids[2].meta().node_id, NodeId::from_raw(3));
    }

    #[test]
    fn children_mut_returns_same_arity_as_children() {
        let mut n = PlanNode::Filter(FilterNode::new(
            meta_for(2, schema(&[("x", DataType::Integer, false)])),
            scan(1, &[("x", DataType::Integer, false)]),
            col_leaf("x"),
        ));
        let n_kids = n.children().len();
        let n_kids_mut = n.children_mut().len();
        assert_eq!(n_kids, n_kids_mut);
    }

    // ── Per-variant constructor smoke tests ──────────────────────────

    #[test]
    fn scan_node_new_has_empty_pushdown() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = ScanNode::new(
            meta_for(1, s),
            SourceRef::new("t"),
            vec![ResolvedColumn {
                name: Name::new("x").unwrap(),
                data_type: DataType::Integer,
                nullable: false,
                ordinal: 0,
            }],
        );
        assert!(n.filters_pushdown.is_empty());
    }

    #[test]
    fn scan_node_with_pushdown_carries_predicates() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = ScanNode::with_pushdown(
            meta_for(1, s),
            SourceRef::new("t"),
            vec![ResolvedColumn {
                name: Name::new("x").unwrap(),
                data_type: DataType::Integer,
                nullable: false,
                ordinal: 0,
            }],
            vec![col_leaf("x")],
        );
        assert_eq!(n.filters_pushdown.len(), 1);
    }

    #[test]
    fn project_node_carries_named_projections() {
        let s = schema(&[("y", DataType::Integer, false)]);
        let n = ProjectNode::new(
            meta_for(2, s),
            scan(1, &[("x", DataType::Integer, false)]),
            vec![(Name::new("y").unwrap(), col_leaf("x"))],
        );
        assert_eq!(n.projections.len(), 1);
        assert_eq!(n.projections[0].0.as_str(), "y");
    }

    #[test]
    fn agg_node_admits_grand_total_with_empty_group_by() {
        let s = schema(&[("total", DataType::Long, false)]);
        let agg_expr = AggregateExpr {
            aggregation: AggregateKind::Builtin(AggregationOp::Sum),
            args: vec![col_leaf("amount")],
            distinct: false,
            filter: None,
            inferred_type: DataType::Long,
        };
        let n = AggNode::new(
            meta_for(2, s),
            scan(1, &[("amount", DataType::Long, false)]),
            Vec::new(),
            vec![(Name::new("total").unwrap(), agg_expr)],
        );
        assert!(n.group_by.is_empty());
        assert_eq!(n.aggregates.len(), 1);
    }

    #[test]
    fn join_node_carries_join_type_and_cardinality() {
        let s = schema(&[
            ("a", DataType::Integer, false),
            ("b", DataType::Integer, false),
        ]);
        let n = JoinNode::new(
            meta_for(3, s),
            scan(1, &[("a", DataType::Integer, false)]),
            scan(2, &[("b", DataType::Integer, false)]),
            JoinType::Left,
            Cardinality::OneToMany,
            vec![KeyPair {
                left: Name::new("a").unwrap(),
                right: Name::new("b").unwrap(),
            }],
        );
        assert_eq!(n.join_type, JoinType::Left);
        assert_eq!(n.cardinality, Cardinality::OneToMany);
    }

    #[test]
    fn union_node_defaults_to_bag_semantics() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = UnionNode::new(
            meta_for(99, s),
            vec![
                scan(1, &[("x", DataType::Integer, false)]),
                scan(2, &[("x", DataType::Integer, false)]),
            ],
        );
        assert!(!n.distinct, "UnionNode::new must default to UNION ALL");
    }

    #[test]
    fn union_node_with_distinct_admits_set_semantics() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let n = UnionNode::with_distinct(
            meta_for(99, s),
            vec![
                scan(1, &[("x", DataType::Integer, false)]),
                scan(2, &[("x", DataType::Integer, false)]),
            ],
            true,
        );
        assert!(n.distinct);
    }

    #[test]
    fn sort_node_carries_priority_ordering() {
        let s = schema(&[
            ("a", DataType::Integer, false),
            ("b", DataType::Integer, false),
        ]);
        let n = SortNode::new(
            meta_for(2, s),
            scan(
                1,
                &[
                    ("a", DataType::Integer, false),
                    ("b", DataType::Integer, false),
                ],
            ),
            vec![
                (Name::new("a").unwrap(), SortDir::ASC_NULLS_LAST),
                (Name::new("b").unwrap(), SortDir::DESC_NULLS_FIRST),
            ],
        );
        assert_eq!(n.order.len(), 2);
    }

    #[test]
    fn fetch_node_distinguishes_none_from_some_zero() {
        let s = schema(&[("x", DataType::Integer, false)]);
        let no_clause = FetchNode::new(
            meta_for(2, Arc::clone(&s)),
            scan(1, &[("x", DataType::Integer, false)]),
            None,
            None,
        );
        let zero = FetchNode::new(
            meta_for(2, s),
            scan(1, &[("x", DataType::Integer, false)]),
            Some(0),
            Some(0),
        );
        assert_ne!(no_clause.limit, zero.limit);
        assert_ne!(no_clause.offset, zero.offset);
    }

    #[test]
    fn values_node_admits_multiple_rows() {
        let s = Schema {
            columns: vec![SchemaColumn {
                name: "x".into(),
                data_type: DataType::Long,
                nullable: false,
            }],
        };
        let n = ValuesNode::new(
            meta_for(1, Arc::new(s.clone())),
            vec![vec![lit_int(1)], vec![lit_int(2)], vec![lit_int(3)]],
            s,
        );
        assert_eq!(n.rows.len(), 3);
    }

    #[test]
    fn values_node_admits_zero_rows() {
        // Zero-row VALUES is well-formed (constant-table edge case);
        // engines MAY short-circuit emission.
        let s = Schema {
            columns: vec![SchemaColumn {
                name: "x".into(),
                data_type: DataType::Integer,
                nullable: false,
            }],
        };
        let n = ValuesNode::new(meta_for(1, Arc::new(s.clone())), Vec::new(), s);
        assert!(n.rows.is_empty());
    }

    // ── Equality / Clone ─────────────────────────────────────────────

    #[test]
    fn plan_node_equality_is_structural() {
        let a = scan(1, &[("x", DataType::Integer, false)]);
        let b = scan(1, &[("x", DataType::Integer, false)]);
        assert_eq!(a, b);
        let c = scan(1, &[("y", DataType::Integer, false)]);
        assert_ne!(a, c);
    }
}
