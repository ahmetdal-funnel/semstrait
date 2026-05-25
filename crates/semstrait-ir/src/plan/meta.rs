//! `NodeMeta`, `NodeId`, and `SemAnnotation` — plan-tree per-node metadata.
//! Per spec `35 §11.1`.
//!
//! Owns:
//! - [`NodeId`] — opaque per-process node identifier. Newtype-over-stable
//!   per `30 §4.3`. v1 = `u128` to keep `semstrait-ir` free of the `uuid`
//!   dependency (Q-PLAN-15, 2026-05-25). Generation is the planner's
//!   concern — IR carries the value only.
//! - [`NodeMeta`] — output schema + node id + annotations carried by
//!   every [`crate::plan::PlanNode`] variant. Per `35 §11.1`.
//! - [`SemAnnotation`] — TRACE-class plan annotations. PLAN-class
//!   variants land additively when `34` ratifies `AggregateRole` /
//!   `FilterSource` / `AdditivityAnnotation`.
//! - [`BoundaryPosition`] — Entry / Exit marker for `StrategyBoundary`.
//!
//! `Schema` is an owned `Arc<Schema>` so pass-through nodes (Filter,
//! Sort, Fetch) share the parent schema without deep-cloning per
//! `35 §11.1`.

use std::sync::Arc;

use crate::expr_kinds::SemanticsName;
use crate::types::Schema;

// ── NodeId ──────────────────────────────────────────────────────────────

/// Per-process opaque node identifier. Per spec `35 §11.1` and Q-IR-002.
///
/// **Scoping (Q-PLAN-15, 2026-05-25).** v1 = newtype over `u128`. The
/// spec mentions `Uuid::new_v4()` as the canonical generator, but that
/// generation is the planner's concern — `semstrait-ir` is pure data
/// per I7 and stays free of the `uuid` crate dependency. A future v2
/// MAY re-export a generator helper conditional on a `uuid` feature
/// flag (additive, MINOR per `30 §2.2`).
///
/// **Identity is per-`SemanticPlan` only.** Two structurally-equal
/// plans MAY have different `NodeId`s; a `transform` that rebuilds the
/// same shape MAY produce a fresh `NodeId`; and comparison across
/// processes / serialize-rehydrate cycles is undefined. Consumers
/// requiring cross-run diff MUST compare structurally.
///
/// Newtype-over-stable per `30 §4.3` (no `#[non_exhaustive]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u128);

impl NodeId {
    /// Construct a `NodeId` from a raw `u128`. Generation is the
    /// caller's concern (typically `Uuid::new_v4().as_u128()` from
    /// the planner crate).
    pub const fn from_raw(value: u128) -> Self {
        Self(value)
    }

    /// Borrow the inner `u128`. Adapters / debug tooling MAY format
    /// this as hex / UUID; `35` makes no commitment to the encoding.
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

// ── NodeMeta ────────────────────────────────────────────────────────────

/// Per-node metadata carried by every [`crate::plan::PlanNode`] variant.
/// Per spec `35 §11.1`.
///
/// `output_schema` is eager (Q-PLAN-15, 2026-05-25): the planner stores
/// the resolved schema at construction; `validate()` re-derives and
/// compares as a regression catch. `Arc<Schema>` enables pass-through
/// sharing per `35 §11.1`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct NodeMeta {
    pub node_id: NodeId,
    pub output_schema: Arc<Schema>,
    pub annotations: Vec<SemAnnotation>,
}

impl NodeMeta {
    /// Construct a `NodeMeta` with no annotations. Annotations are added
    /// by the planner per `35 §11.1`.
    pub fn new(node_id: NodeId, output_schema: Arc<Schema>) -> Self {
        Self {
            node_id,
            output_schema,
            annotations: Vec::new(),
        }
    }

    /// Construct a `NodeMeta` with a pre-supplied annotation list.
    pub fn with_annotations(
        node_id: NodeId,
        output_schema: Arc<Schema>,
        annotations: Vec<SemAnnotation>,
    ) -> Self {
        Self {
            node_id,
            output_schema,
            annotations,
        }
    }
}

// ── SemAnnotation ───────────────────────────────────────────────────────

/// Per-node annotation. Per spec `35 §11.1.1`.
///
/// Variants split into two classes per S5 (`35 §1.5`):
///
/// - **PLAN class** — read by `34` / `36` as part of the IR-to-consumer
///   contract (advisory hints, never dispatch). Renaming or removing
///   a PLAN variant is MAJOR per `30 §2.1`.
/// - **TRACE class** — descriptive only; never read by `34` / `36`.
///   Adding, renaming, or removing a TRACE variant is MINOR (additive
///   growth) since no consumer reads them.
///
/// **v1 scope (Q-PLAN-15, 2026-05-25).** Only TRACE-class variants
/// land in v1. PLAN-class variants (`AggregateRole`, `FilterSource`,
/// `AdditivityAnnotation`) land additively when `34` ratifies their
/// support enums. Adding variants is MINOR per `30 §2.2` thanks to
/// `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum SemAnnotation {
    /// DataKind that contributed this subtree. Per `35 §11.1.1`.
    /// **TRACE** — for tools, debuggers, SQL pretty-printers.
    DataKindRef(SemanticsName),

    /// Boundary marker emitted by a planner strategy when one
    /// DataKind's strategy expansion stops contributing nodes and
    /// another begins. Per `35 §11.1.1`.
    /// **TRACE** — for tools that visualize plan-tree composition.
    StrategyBoundary {
        type_: SemanticsName,
        position: BoundaryPosition,
    },
}

/// Position of a `StrategyBoundary` annotation relative to the
/// strategy's emitted subtree. Per spec `35 §11.1.1`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryPosition {
    /// First node a strategy emitted; conceptually "begin".
    Entry,
    /// Last node a strategy emitted; conceptually "end".
    Exit,
}

// ── AnnotationClass ─────────────────────────────────────────────────────

/// Classification of a [`SemAnnotation`] variant. Per spec `35 §11.1.1`.
///
/// Two classes split annotations by their consumer contract:
///
/// - [`AnnotationClass::Trace`] — descriptive only; `34` and `36` MUST
///   NOT read TRACE annotations. Adding, renaming, or removing a TRACE
///   variant is MINOR (additive growth) since no consumer reads them.
///
/// - [`AnnotationClass::Plan`] — advisory hints read by `34` / `36`
///   (never dispatch — see S5 / `35 §1.5`). Renaming or removing a
///   PLAN variant is MAJOR.
///
/// **v1 scope.** Only TRACE-class variants exist today
/// ([`SemAnnotation::DataKindRef`], [`SemAnnotation::StrategyBoundary`]).
/// PLAN-class variants land additively when `34` ratifies their
/// support enums; the [`AnnotationClass::Plan`] discriminator is
/// reserved here so consumers can pattern-match exhaustively without
/// breakage when those variants land.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnnotationClass {
    /// Descriptive provenance; never read by `34` / `36`.
    Trace,
    /// Advisory hint read by `34` / `36`. Reserved for PLAN-class
    /// variants per `35 §11.1.1`; no v1 [`SemAnnotation`] variant
    /// classifies as `Plan`.
    Plan,
}

impl SemAnnotation {
    /// Classify this annotation per `35 §11.1.1`.
    ///
    /// All v1 variants ([`Self::DataKindRef`], [`Self::StrategyBoundary`])
    /// classify as [`AnnotationClass::Trace`]. PLAN-class variants land
    /// additively when `34` ratifies their support enums; consumers
    /// SHOULD match on the returned class rather than inspecting
    /// variants directly.
    pub fn class(&self) -> AnnotationClass {
        match self {
            Self::DataKindRef(_) | Self::StrategyBoundary { .. } => AnnotationClass::Trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DataType, SchemaColumn};
    use crate::expr_kinds::SemanticsName;

    fn schema_one_int() -> Arc<Schema> {
        Arc::new(Schema {
            columns: vec![SchemaColumn {
                name: "x".into(),
                data_type: DataType::Integer,
                nullable: false,
            }],
        })
    }

    // ── NodeId ───────────────────────────────────────────────────────

    #[test]
    fn node_id_round_trips_through_raw() {
        let id = NodeId::from_raw(0xdead_beef_dead_beef_dead_beef_dead_beef);
        assert_eq!(id.as_u128(), 0xdead_beef_dead_beef_dead_beef_dead_beef);
    }

    #[test]
    fn node_id_equality_is_value_equality() {
        let a = NodeId::from_raw(42);
        let b = NodeId::from_raw(42);
        let c = NodeId::from_raw(43);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn node_id_zero_is_admissible() {
        // No "null" sentinel — generation is the planner's contract,
        // and an all-zero u128 is a valid value at this layer.
        let _ = NodeId::from_raw(0);
    }

    #[test]
    fn node_id_is_copy() {
        // Compile-time constraint: NodeId must be Copy so PlanNode
        // accessors can return it by value.
        fn assert_copy<T: Copy>() {}
        assert_copy::<NodeId>();
    }

    // ── NodeMeta ─────────────────────────────────────────────────────

    #[test]
    fn node_meta_new_has_empty_annotations() {
        let m = NodeMeta::new(NodeId::from_raw(1), schema_one_int());
        assert!(m.annotations.is_empty());
        assert_eq!(m.node_id, NodeId::from_raw(1));
    }

    #[test]
    fn node_meta_with_annotations_preserves_order() {
        let anns = vec![
            SemAnnotation::DataKindRef(SemanticsName("orders".to_string())),
            SemAnnotation::DataKindRef(SemanticsName("invoices".to_string())),
        ];
        let m = NodeMeta::with_annotations(NodeId::from_raw(7), schema_one_int(), anns.clone());
        assert_eq!(m.annotations, anns);
    }

    #[test]
    fn node_meta_share_schema_via_arc() {
        let schema = schema_one_int();
        let m1 = NodeMeta::new(NodeId::from_raw(1), Arc::clone(&schema));
        let m2 = NodeMeta::new(NodeId::from_raw(2), Arc::clone(&schema));
        // Same `Arc<Schema>` underneath — pass-through schema sharing
        // per `35 §11.1`.
        assert!(Arc::ptr_eq(&m1.output_schema, &m2.output_schema));
    }

    #[test]
    fn node_meta_clone_preserves_fields() {
        let m1 = NodeMeta::with_annotations(
            NodeId::from_raw(99),
            schema_one_int(),
            vec![SemAnnotation::DataKindRef(SemanticsName("x".to_string()))],
        );
        let m2 = m1.clone();
        assert_eq!(m1, m2);
    }

    // ── SemAnnotation ────────────────────────────────────────────────

    #[test]
    fn sem_annotation_data_kind_ref_carries_name() {
        let ann = SemAnnotation::DataKindRef(SemanticsName("orders".to_string()));
        match ann {
            SemAnnotation::DataKindRef(n) => assert_eq!(n.0, "orders"),
            _ => panic!("expected DataKindRef"),
        }
    }

    #[test]
    fn sem_annotation_strategy_boundary_carries_position() {
        let ann = SemAnnotation::StrategyBoundary {
            type_: SemanticsName("revenue".to_string()),
            position: BoundaryPosition::Entry,
        };
        match ann {
            SemAnnotation::StrategyBoundary { position, .. } => {
                assert_eq!(position, BoundaryPosition::Entry);
            }
            _ => panic!("expected StrategyBoundary"),
        }
    }

    #[test]
    fn sem_annotation_variants_distinguish() {
        let a = SemAnnotation::DataKindRef(SemanticsName("x".to_string()));
        let b = SemAnnotation::StrategyBoundary {
            type_: SemanticsName("x".to_string()),
            position: BoundaryPosition::Exit,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn boundary_position_equality_and_copy() {
        assert_eq!(BoundaryPosition::Entry, BoundaryPosition::Entry);
        assert_ne!(BoundaryPosition::Entry, BoundaryPosition::Exit);
        // Copy bound for ergonomic by-value passing.
        fn assert_copy<T: Copy>() {}
        assert_copy::<BoundaryPosition>();
    }

    // ── AnnotationClass / SemAnnotation::class ───────────────────────

    #[test]
    fn data_kind_ref_classifies_as_trace() {
        let ann = SemAnnotation::DataKindRef(SemanticsName("orders".to_string()));
        assert_eq!(ann.class(), AnnotationClass::Trace);
    }

    #[test]
    fn strategy_boundary_classifies_as_trace() {
        let ann = SemAnnotation::StrategyBoundary {
            type_: SemanticsName("revenue".to_string()),
            position: BoundaryPosition::Entry,
        };
        assert_eq!(ann.class(), AnnotationClass::Trace);
    }

    #[test]
    fn all_v1_variants_classify_as_trace() {
        // Invariant: spec `35 §11.1.1` v1 scope — only TRACE-class
        // variants exist. If a PLAN-class variant is added without
        // updating `class()`, this test will fail to compile due to
        // a non-exhaustive match (after the variant lands).
        for ann in [
            SemAnnotation::DataKindRef(SemanticsName("a".to_string())),
            SemAnnotation::StrategyBoundary {
                type_: SemanticsName("b".to_string()),
                position: BoundaryPosition::Exit,
            },
        ] {
            assert_eq!(ann.class(), AnnotationClass::Trace);
        }
    }

    #[test]
    fn annotation_class_is_copy_eq_hash() {
        // Reserved for use as hash-map keys in tools that bucket nodes
        // by annotation class.
        fn assert_bounds<T: Copy + Eq + std::hash::Hash>() {}
        assert_bounds::<AnnotationClass>();
    }

    #[test]
    fn annotation_class_trace_and_plan_distinct() {
        assert_ne!(AnnotationClass::Trace, AnnotationClass::Plan);
    }
}
