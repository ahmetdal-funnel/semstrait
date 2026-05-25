//! Plan-level primitives. Spec `35 §11`, ratified by `15` / `16 §5`.
//!
//! Owns:
//! - [`Name`] — newtype-over-stable identifier per `35 §11.4`. Validated
//!   non-empty at construction; reserved-prefix policy is spec-only per
//!   `35 §1.6 R2` (no runtime enforcement at this layer).
//! - [`SourceRef`] — opaque handle into the SemanticManifest per
//!   `35 §11.2`. Inner shape is crate-private; consumers compare for
//!   equality only. String-opaque per Q-PLAN-03.
//! - [`ResolvedColumn`] — single-column descriptor produced by `Scan`
//!   per `35 §11.3` / `15 §4.2`.
//! - [`KeyPair`] — equi-join key pair carried on `JoinNode.on` per
//!   `35 §11.5` / `16 §5.1`.
//! - [`SortDir`] / [`NullOrdering`] — sort-direction + null-ordering
//!   bundle per `35 §11.6`.
//! - [`AggregateExpr`] — plan-level aggregate kernel per `35 §11.7` /
//!   `19 §7`. `input_expr` is the single lifted argument (Q-IR-NEW-003
//!   lift contract); n-ary `Expr::Aggregate.args` is the upstream
//!   pre-lift form.
//! - [`JoinType`] — equi-join kind per `35 §11` / `16 §5.2`. IR-local
//!   per Q-PLAN-01 override.
//! - [`Cardinality`] — relationship-cardinality annotation per
//!   `35 §11` / `16 §5.1`. IR-local per Q-PLAN-01 override.

use serde::{Deserialize, Serialize};

use crate::error::ValidateError;
use crate::expr::PhysicalExpr;
use crate::types::DataType;

// ── Name ────────────────────────────────────────────────────────────────

/// Plan-tree identifier: output column names, group-by keys, sort keys,
/// projection aliases. Newtype-over-stable per `30 §4.3` — `.0` access
/// is intentionally **not** offered; consumers go through [`Self::as_str`]
/// or [`Self::into_string`].
///
/// Validation at construction:
/// - **Non-empty.** Empty `Name` is a hard structural error.
/// - **UTF-8.** Guaranteed by `String`.
/// - **Reserved-prefix policy is spec-only** (`__semstrait_`, `__plan_`,
///   `__agg_`). No runtime check at this layer per Q-PLAN-04 — the
///   planner / manifest layer enforces, with diagnostics scoped to its
///   own error vocabulary.
///
/// `Name` is **not** normalized: `Name::new("Amount")` and
/// `Name::new("amount")` are distinct values, matching `11 §5`'s
/// case-preserving-but-case-sensitive rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Name(String);

impl Name {
    /// Validates the identifier and constructs. Empty input is rejected
    /// with [`ValidateError::EmptyName`].
    pub fn new(s: impl Into<String>) -> Result<Self, ValidateError> {
        let s = s.into();
        if s.is_empty() {
            return Err(ValidateError::EmptyName);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

// ── SourceRef ───────────────────────────────────────────────────────────

/// Opaque handle to a source in the SemanticManifest. Constructed by the
/// planner; resolved by the adapter against the SemanticManifest it was
/// handed alongside the `SemanticPlan`. No path, URL, catalog name, or
/// file format leaks into the plan tree (I1).
///
/// String-opaque per Q-PLAN-03: the inner spelling is crate-private;
/// consumers compare for equality only. Newtype-over-stable per
/// `30 §4.3`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRef(String);

impl SourceRef {
    /// Construct a `SourceRef` from an opaque handle string. The string
    /// is treated as a comparison key only — no parsing, no validation.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the opaque handle. Adapters use this when looking the
    /// `SourceRef` up against the SemanticManifest; the encoding is
    /// out-of-scope for `35`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ── ResolvedColumn ──────────────────────────────────────────────────────

/// One column projected by a [`crate::plan::ScanNode`]. Per `35 §11.3` /
/// `15 §4.2`.
///
/// `ordinal` is the column's position in the underlying source's native
/// schema order (Parquet footer / Iceberg field order / CSV header /
/// catalog table-column order). Adapters consume this when emitting
/// stable column references.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolvedColumn {
    pub name: Name,
    pub data_type: DataType,
    pub nullable: bool,
    pub ordinal: u32,
}

// ── KeyPair ─────────────────────────────────────────────────────────────

/// One join-key pair on `JoinNode.on`. Per `35 §11.5` / `16 §5.1`. Both
/// `left` and `right` are column [`Name`]s resolving against the join's
/// corresponding child schema.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyPair {
    pub left: Name,
    pub right: Name,
}

// ── SortDir / NullOrdering ─────────────────────────────────────────────

/// Sort direction + null-ordering bundle. Per `35 §11.6`. Matches
/// Substrait's `SortField.direction` carrier shape (one field encodes
/// both axes).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    Asc { nulls: NullOrdering },
    Desc { nulls: NullOrdering },
}

/// Where to place `NULL` values in a sort. Per `35 §11.6`.
///
/// `Unspecified` lets the adapter choose its engine's default
/// (typically `ASC NULLS LAST` / `DESC NULLS FIRST`). It carries zero
/// semstrait-side constraint.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NullOrdering {
    First,
    Last,
    Unspecified,
}

impl SortDir {
    pub const ASC_NULLS_FIRST: SortDir = SortDir::Asc { nulls: NullOrdering::First };
    pub const ASC_NULLS_LAST: SortDir = SortDir::Asc { nulls: NullOrdering::Last };
    pub const DESC_NULLS_FIRST: SortDir = SortDir::Desc { nulls: NullOrdering::First };
    pub const DESC_NULLS_LAST: SortDir = SortDir::Desc { nulls: NullOrdering::Last };
}

// ── JoinType ────────────────────────────────────────────────────────────

/// Equi-join kind. Per `35 §11` / `16 §5.2`. IR-local per Q-PLAN-01
/// override (model-side parsing parses to its own enum and lowers to
/// this one at compile, mirroring the `Expr` propagation pattern).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

// ── Cardinality ─────────────────────────────────────────────────────────

/// Relationship cardinality annotation on `JoinNode.cardinality`. Per
/// `35 §11` / `16 §5.1`. Adapters MAY use this for optimization hints
/// (e.g. `OneToOne` → redundant-join elimination).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

// ── AggregateExpr ───────────────────────────────────────────────────────

/// Plan-level aggregate kernel carried on `AggNode.aggregates`. Per
/// `35 §11.7` / `19 §7`.
///
/// **Lift contract (Q-IR-NEW-003).** Phase B planning lifts an
/// `Expr::Aggregate { op, args, distinct, filter }` out of the input
/// `PhysicalExpr` into this structure. The N-ary `args` of the
/// upstream form collapses to a single `input_expr` for v1: every v1
/// canonical aggregate (`SUM` / `AVG` / `COUNT` / `MIN` / `MAX`) is
/// 1-ary. Future N-ary aggregates land additively (e.g. a future
/// `args_tail: Vec<PhysicalExpr>` field is MINOR per `30 §2.2`).
///
/// `inferred_type` is populated by Phase B so adapters MAY read the
/// aggregate's resolved output type without re-running inference.
/// `filter` carries the canonical `agg(expr) FILTER (WHERE p)` shape;
/// adapter compensation for engines without native `FILTER` is the
/// adapter's concern, not the IR's.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateExpr {
    pub aggregation: crate::expr_kinds::AggregationOp,
    pub input_expr: PhysicalExpr,
    pub distinct: bool,
    pub filter: Option<PhysicalExpr>,
    pub inferred_type: DataType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::PhysicalLeaf;
    use crate::expr_kinds::{AggregationOp, ColumnRef, Literal};
    use crate::Expr;
    use std::collections::HashSet;

    // ── Name ─────────────────────────────────────────────────────────

    #[test]
    fn name_round_trips_through_as_str_and_into_string() {
        let n = Name::new("amount").unwrap();
        assert_eq!(n.as_str(), "amount");
        assert_eq!(n.clone().into_string(), "amount");
    }

    #[test]
    fn name_rejects_empty() {
        let r = Name::new("");
        assert!(matches!(r, Err(ValidateError::EmptyName)));
    }

    #[test]
    fn name_is_case_sensitive() {
        let a = Name::new("Amount").unwrap();
        let b = Name::new("amount").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn name_admits_reserved_prefix_at_this_layer() {
        // Q-PLAN-04: reserved-prefix policy is spec-only at this layer;
        // the manifest / planner layer enforces with its own diagnostic.
        for raw in ["__semstrait_x", "__plan_x", "__agg_x"] {
            assert!(
                Name::new(raw).is_ok(),
                "Name::new must admit reserved-prefix `{raw}` at the IR layer"
            );
        }
    }

    #[test]
    fn name_admits_unicode_and_punctuation_at_this_layer() {
        // Identifier grammar is enforced at construction sites that have
        // it (e.g. CanonicalFn::new); plan-level Name is a String wrapper
        // with non-emptiness as the only invariant.
        for raw in ["x.y", "amount $", "ψ", "1col"] {
            assert!(Name::new(raw).is_ok(), "Name::new must admit `{raw}`");
        }
    }

    #[test]
    fn name_serde_json_roundtrip() {
        let n = Name::new("revenue").unwrap();
        let json = serde_json::to_string(&n).unwrap();
        let back: Name = serde_json::from_str(&json).unwrap();
        assert_eq!(n, back);
    }

    #[test]
    fn name_hash_is_deterministic() {
        let a = Name::new("x").unwrap();
        let b = Name::new("x").unwrap();
        let mut set: HashSet<Name> = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    // ── SourceRef ────────────────────────────────────────────────────

    #[test]
    fn source_ref_equality_is_string_equality() {
        let a = SourceRef::new("manifest://orders");
        let b = SourceRef::new("manifest://orders");
        assert_eq!(a, b);
        let c = SourceRef::new("manifest://invoices");
        assert_ne!(a, c);
    }

    #[test]
    fn source_ref_is_opaque_via_as_str_only() {
        let s = SourceRef::new("sem:foo");
        assert_eq!(s.as_str(), "sem:foo");
    }

    #[test]
    fn source_ref_serde_json_roundtrip() {
        let s = SourceRef::new("urn:semstrait:source:42");
        let json = serde_json::to_string(&s).unwrap();
        let back: SourceRef = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn source_ref_hash_is_deterministic() {
        let mut set: HashSet<SourceRef> = HashSet::new();
        set.insert(SourceRef::new("a"));
        assert!(set.contains(&SourceRef::new("a")));
    }

    // ── ResolvedColumn ───────────────────────────────────────────────

    #[test]
    fn resolved_column_construction_and_equality() {
        let a = ResolvedColumn {
            name: Name::new("amount").unwrap(),
            data_type: DataType::Decimal { precision: 10, scale: 2 },
            nullable: false,
            ordinal: 3,
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = ResolvedColumn { ordinal: 4, ..a.clone() };
        assert_ne!(a, c, "ordinal participates in equality");
    }

    #[test]
    fn resolved_column_serde_json_roundtrip() {
        let c = ResolvedColumn {
            name: Name::new("ts").unwrap(),
            data_type: DataType::Timestamp { precision: 6 },
            nullable: true,
            ordinal: 0,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: ResolvedColumn = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    // ── KeyPair ──────────────────────────────────────────────────────

    #[test]
    fn key_pair_equality_and_clone() {
        let a = KeyPair {
            left: Name::new("order_id").unwrap(),
            right: Name::new("order_id").unwrap(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = KeyPair {
            left: Name::new("order_id").unwrap(),
            right: Name::new("ord_id").unwrap(),
        };
        assert_ne!(a, c);
    }

    #[test]
    fn key_pair_serde_json_roundtrip() {
        let kp = KeyPair {
            left: Name::new("a").unwrap(),
            right: Name::new("b").unwrap(),
        };
        let json = serde_json::to_string(&kp).unwrap();
        let back: KeyPair = serde_json::from_str(&json).unwrap();
        assert_eq!(kp, back);
    }

    // ── SortDir / NullOrdering ───────────────────────────────────────

    #[test]
    fn sort_dir_constants_match_struct_form() {
        assert_eq!(
            SortDir::ASC_NULLS_FIRST,
            SortDir::Asc { nulls: NullOrdering::First }
        );
        assert_eq!(
            SortDir::DESC_NULLS_LAST,
            SortDir::Desc { nulls: NullOrdering::Last }
        );
    }

    #[test]
    fn sort_dir_distinguishes_dir_and_null_ordering() {
        assert_ne!(SortDir::ASC_NULLS_FIRST, SortDir::ASC_NULLS_LAST);
        assert_ne!(SortDir::ASC_NULLS_FIRST, SortDir::DESC_NULLS_FIRST);
    }

    #[test]
    fn sort_dir_serde_json_roundtrip() {
        let roster = [
            SortDir::ASC_NULLS_FIRST,
            SortDir::ASC_NULLS_LAST,
            SortDir::DESC_NULLS_FIRST,
            SortDir::DESC_NULLS_LAST,
            SortDir::Asc { nulls: NullOrdering::Unspecified },
            SortDir::Desc { nulls: NullOrdering::Unspecified },
        ];
        for s in roster {
            let json = serde_json::to_string(&s).unwrap();
            let back: SortDir = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn null_ordering_serde_json_roundtrip() {
        for o in [NullOrdering::First, NullOrdering::Last, NullOrdering::Unspecified] {
            let json = serde_json::to_string(&o).unwrap();
            let back: NullOrdering = serde_json::from_str(&json).unwrap();
            assert_eq!(o, back);
        }
    }

    // ── JoinType / Cardinality ──────────────────────────────────────

    #[test]
    fn join_type_serde_json_roundtrip() {
        for j in [JoinType::Inner, JoinType::Left, JoinType::Right, JoinType::Full] {
            let json = serde_json::to_string(&j).unwrap();
            let back: JoinType = serde_json::from_str(&json).unwrap();
            assert_eq!(j, back);
        }
    }

    #[test]
    fn cardinality_serde_json_roundtrip() {
        for c in [
            Cardinality::OneToOne,
            Cardinality::OneToMany,
            Cardinality::ManyToOne,
            Cardinality::ManyToMany,
        ] {
            let json = serde_json::to_string(&c).unwrap();
            let back: Cardinality = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    // ── AggregateExpr ────────────────────────────────────────────────

    fn col_leaf(name: &str) -> PhysicalExpr {
        Expr::Leaf(PhysicalLeaf::Column(ColumnRef(name.to_string())))
    }

    #[test]
    fn aggregate_expr_construction_and_clone() {
        let a = AggregateExpr {
            aggregation: AggregationOp::Sum,
            input_expr: col_leaf("amount"),
            distinct: false,
            filter: None,
            inferred_type: DataType::Decimal { precision: 18, scale: 2 },
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn aggregate_expr_distinguishes_distinct_and_filter() {
        let base = AggregateExpr {
            aggregation: AggregationOp::Count,
            input_expr: col_leaf("user_id"),
            distinct: false,
            filter: None,
            inferred_type: DataType::Long,
        };
        let with_distinct = AggregateExpr { distinct: true, ..base.clone() };
        assert_ne!(base, with_distinct);

        let with_filter = AggregateExpr {
            filter: Some(Expr::Leaf(PhysicalLeaf::Literal(Literal::Boolean(true)))),
            ..base.clone()
        };
        assert_ne!(base, with_filter);
    }

    #[test]
    fn aggregate_expr_input_is_single_per_lift_contract() {
        // Q-IR-NEW-003: AggregateExpr.input_expr is one PhysicalExpr,
        // not a Vec. Compile-time enforcement — this test simply
        // documents the structural choice via construction.
        let _ = AggregateExpr {
            aggregation: AggregationOp::Avg,
            input_expr: col_leaf("price"),
            distinct: false,
            filter: None,
            inferred_type: DataType::Double,
        };
    }
}
