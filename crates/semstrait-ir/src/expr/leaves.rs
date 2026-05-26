//! Canonical-IR and semantic leaf sets, plus the type aliases that name
//! the two layered `Expr<L>` instantiations. Per spec `14 §3.4` /
//! `14 §3.5` / `14 §3.6` / `35 §4`.
//!
//! - [`PhysicalLeaf`] — canonical-IR leaf set. Carries the leaves that
//!   adapters render: physical column references, typed literals, and
//!   the compile-emitted, plan-bound parameter placeholder
//!   ([`crate::expr::parameter::Parameter`]).
//! - [`SemanticLeaf`] — authoring-form leaf set with per-kind typed
//!   leaves (`Field` / `Dimension` / `Measure` / `Metric` / `Key`). Each
//!   typed leaf optionally carries a kind-specific sugar accessor
//!   (`14 §4.1`).
//! - [`PhysicalExpr`] — `Expr<PhysicalLeaf>` alias.
//! - [`SemanticExpr`] — `Expr<SemanticLeaf>` alias.
//!
//! ## Notes on `inferred_type`
//!
//! Per spec `14 §3.2`, `ExprLeaf::inferred_type()` is permitted to return
//! `None` when the type cannot be determined locally. v1 returns:
//!
//! - `PhysicalLeaf::Column` → `None` — type is resolved against the
//!   binding's schema during compile, not local to the leaf.
//! - `PhysicalLeaf::Literal` → `None` — the literal carrier owns no
//!   `&'static DataType` slot, so we cannot return a borrow into a
//!   long-lived value. Local type derivation is deferred to Phase 4 / 5;
//!   downstream stages compute literal types from context. This matches
//!   `14 §3.2`'s "returning None is acceptable when type cannot be
//!   determined locally".
//! - `PhysicalLeaf::Parameter` → `Some(&parameter.data_type)` — the
//!   parameter struct carries its declared `data_type` field by value.
//! - `SemanticLeaf::Literal` → `None` (same rationale as physical).
//! - All other `SemanticLeaf` variants → `None` — semantic leaves
//!   resolve their types at compile time per `19 §3`.
//!
//! Phase 4/5 may add a richer locally-derived type API; this iteration
//! keeps the leaf surface conservative.

use crate::expr::accessor::{DimensionAccessor, KeyAccessor, MeasureAccessor, MetricAccessor};
use crate::expr::parameter::Parameter;
use crate::expr::tree::Expr;
use crate::expr_kinds::{ColumnRef, Literal, SemanticsName};
use crate::tree::ExprLeaf;
use crate::types::DataType;

/// Canonical-IR leaf set per spec `14 §3.4` / `35 §4.1`.
///
/// Carries exactly what the planner and adapters need:
///
/// - Physical column references (binding-resolved per `15`).
/// - Typed literals.
/// - Compile-emitted, plan-bound parameter placeholders. Replaced with a
///   concrete value during Phase B planning per `14 §5.3` / `19 §6`.
///
/// Invariants per `14 §3.4`:
///
/// - No `Field` / `Dimension` / `Measure` / `Metric` / `Key` — semantic
///   references are eliminated during compile per `19 §3`.
/// - No sugar accessors — typed-leaf-with-accessor leaves are eliminated
///   during compile (lowered to `Window`-rooted subtrees per `14 §4.2`).
/// - `Parameter` leaves are the only non-resolved state the canonical IR
///   carries; they MUST be substituted before adapt time
///   (`14 §5.3` postcondition).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicalLeaf {
    /// Physical column reference (binding-resolved).
    Column(ColumnRef),

    /// Typed literal value.
    Literal(Literal),

    /// Compile-emitted, plan-bound parameter placeholder.
    Parameter(Parameter),
}

impl ExprLeaf for PhysicalLeaf {
    fn inferred_type(&self) -> Option<&DataType> {
        match self {
            // Resolved against schema during compile / plan; not local.
            Self::Column(_) => None,
            // See module-doc note — local literal-type derivation is
            // deferred so we always return None here in v1.
            Self::Literal(_) => None,
            // Parameter carries its own declared type by value.
            Self::Parameter(p) => Some(&p.data_type),
        }
    }
}

/// Per-kind typed leaf set per spec `14 §3.5` / `35 §4.2`.
///
/// Each variant tag encodes the entity kind; the optional `accessor`
/// field carries kind-specific sugar (`14 §4.1`). Compile substitutes
/// per the algorithm in `19 §3`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticLeaf {
    /// Typed literal value.
    Literal(Literal),

    /// Physical column reference. Type-admissible inside [`SemanticExpr`];
    /// LEGAL only under `semantic_mapping: auto` per `14 §8`'s
    /// compile-time rejection rule.
    Column(ColumnRef),

    /// Untyped semantic reference. Kind resolved at compile by registry
    /// lookup per `19 §3`.
    Field(SemanticsName),

    /// Typed Dimension reference, optionally with sugar accessor.
    Dimension {
        name: SemanticsName,
        accessor: Option<DimensionAccessor>,
    },

    /// Typed Measure reference, optionally with sugar accessor.
    Measure {
        name: SemanticsName,
        accessor: Option<MeasureAccessor>,
    },

    /// Typed Metric reference, optionally with sugar accessor.
    Metric {
        name: SemanticsName,
        accessor: Option<MetricAccessor>,
    },

    /// Typed Key reference, optionally with sugar accessor.
    Key {
        name: SemanticsName,
        accessor: Option<KeyAccessor>,
    },
}

impl ExprLeaf for SemanticLeaf {
    fn inferred_type(&self) -> Option<&DataType> {
        // v1: every semantic leaf returns None. Literal handling matches
        // PhysicalLeaf — see module doc. Field / Dimension / Measure /
        // Metric / Key resolve their types at compile time per `19 §3`.
        None
    }
}

/// Canonical-IR expression. Per spec `14 §3.6` / `35 §4.3`.
pub type PhysicalExpr = Expr<PhysicalLeaf>;

/// Authoring-form expression. Per spec `14 §3.6` / `35 §4.3`.
pub type SemanticExpr = Expr<SemanticLeaf>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parameter::ParameterKey;

    // ── PhysicalLeaf::inferred_type ─────────────────────────────────────

    #[test]
    fn physical_leaf_inferred_type_parameter_returns_some() {
        let p = Parameter {
            key: ParameterKey::RequestTemporalAxis,
            data_type: DataType::Date,
        };
        let leaf = PhysicalLeaf::Parameter(p);
        assert_eq!(leaf.inferred_type(), Some(&DataType::Date));
    }

    #[test]
    fn physical_leaf_inferred_type_column_returns_none() {
        let leaf = PhysicalLeaf::Column(ColumnRef("a".into()));
        assert!(leaf.inferred_type().is_none());
    }

    #[test]
    fn physical_leaf_inferred_type_literal_returns_none_in_v1() {
        use crate::expr_kinds::IntegerWidth;
        let leaf = PhysicalLeaf::Literal(Literal::Integer {
            value: 7,
            width: IntegerWidth::Long,
        });
        assert!(leaf.inferred_type().is_none());
    }

    // ── SemanticLeaf shape ──────────────────────────────────────────────

    #[test]
    fn semantic_leaf_dimension_with_accessor() {
        let a = SemanticLeaf::Dimension {
            name: SemanticsName("region".into()),
            accessor: Some(DimensionAccessor::First),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = SemanticLeaf::Dimension {
            name: SemanticsName("region".into()),
            accessor: None,
        };
        assert_ne!(a, c);
        let d = SemanticLeaf::Dimension {
            name: SemanticsName("region".into()),
            accessor: Some(DimensionAccessor::Last),
        };
        assert_ne!(a, d);
    }

    #[test]
    fn semantic_leaf_measure_with_accessor() {
        let a = SemanticLeaf::Measure {
            name: SemanticsName("revenue".into()),
            accessor: Some(MeasureAccessor::Previous),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = SemanticLeaf::Measure {
            name: SemanticsName("revenue".into()),
            accessor: Some(MeasureAccessor::Delta),
        };
        assert_ne!(a, c);
    }

    #[test]
    fn semantic_leaf_metric_with_accessor() {
        let a = SemanticLeaf::Metric {
            name: SemanticsName("conv_rate".into()),
            accessor: Some(MetricAccessor::PercentChange),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn semantic_leaf_key_with_accessor() {
        let a = SemanticLeaf::Key {
            name: SemanticsName("order_id".into()),
            accessor: Some(KeyAccessor::Lag(2)),
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(
            a,
            SemanticLeaf::Key {
                name: SemanticsName("order_id".into()),
                accessor: Some(KeyAccessor::Lead(2)),
            }
        );
    }

    // ── SemanticLeaf::inferred_type ─────────────────────────────────────

    #[test]
    fn semantic_leaf_inferred_type_returns_none_for_every_variant_in_v1() {
        let leaves = [
            SemanticLeaf::Literal(Literal::Integer {
                value: 1,
                width: crate::expr_kinds::IntegerWidth::Long,
            }),
            SemanticLeaf::Column(ColumnRef("c".into())),
            SemanticLeaf::Field(SemanticsName("f".into())),
            SemanticLeaf::Dimension {
                name: SemanticsName("d".into()),
                accessor: None,
            },
            SemanticLeaf::Measure {
                name: SemanticsName("m".into()),
                accessor: None,
            },
            SemanticLeaf::Metric {
                name: SemanticsName("mm".into()),
                accessor: None,
            },
            SemanticLeaf::Key {
                name: SemanticsName("k".into()),
                accessor: None,
            },
        ];
        for l in leaves {
            assert!(l.inferred_type().is_none(), "expected None for {:?}", l);
        }
    }

    // ── Type aliases compile ────────────────────────────────────────────

    #[test]
    fn physical_expr_alias_works() {
        let _: PhysicalExpr = Expr::Leaf(PhysicalLeaf::Column(ColumnRef("a".into())));
    }

    #[test]
    fn semantic_expr_alias_works() {
        let _: SemanticExpr = Expr::Leaf(SemanticLeaf::Field(SemanticsName("x".into())));
    }
}
