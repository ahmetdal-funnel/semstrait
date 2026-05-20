//! Structural-variant support enums and identifier carriers per spec
//! `14 §3.3` and `35 §3.4`.
//!
//! Per the second-cascade placement (`STATUS.md` item Q, 2026-05-19),
//! these types live in `semstrait-ir`, not `semstrait-core`. They are the
//! shared vocabulary every `Expr<L>` instantiation references — operator
//! discriminators, the typed-literal carrier, and the newtype identifier
//! carriers.
//!
//! Variant rosters here track `14 §3.3` and `35 §3.4` verbatim. Every
//! public enum carries `#[non_exhaustive]` per invariant I10 (additive
//! variant growth must not break exhaustively-matching consumers).
//!
//! ## Note on `CanonicalFn` placement
//!
//! `CanonicalFn` lives in this module pending `FunctionRegistry` stand-up.
//! Spec `35 §2` allocates a dedicated `functions/` module that will own
//! `CanonicalFn` once `[14a §2](../../docs/design/foundations/14a_function_catalog.md)`
//! lands; the type relocates at that point. This iteration places it here
//! because `Expr<L>::FunctionCall` (Phase 2b) carries it and we are not
//! building the registry yet.

// ── Operator discriminators ────────────────────────────────────────────

/// Binary-operator discriminator carried by `Expr<L>::BinaryOp`. Roster
/// per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOpKind {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    /// `SafeDivide` returns NULL on division-by-zero (vs `Divide`'s
    /// engine-defined error). Adapter compensation is per-engine.
    SafeDivide,
    Mod,

    // Comparison
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    // Logical
    And,
    Or,
}

/// Unary-operator discriminator carried by `Expr<L>::UnaryOp`. Roster
/// per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOpKind {
    Negate,
    Not,
}

/// Aggregation-operation tag carried by `Expr<L>::Aggregate`. Roster
/// per spec `14 §3.3`. `CountDistinct` is encoded as
/// `Aggregate { op: Count, distinct: true, .. }`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregationOp {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

/// `LIKE`-operator variant tag — case-sensitivity and negation profile.
/// Carried by `Expr<L>::Like`. Roster per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LikeKind {
    Like,
    NotLike,
    ILike,
    NotILike,
}

/// Cast failure-mode discriminator carried by `Expr<L>::Cast`. Adapters
/// MAY emit different SQL forms per variant (`CAST` vs `TRY_CAST`).
/// Roster per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastFailure {
    /// Raise an engine-level error on cast failure.
    Error,
    /// Return NULL on cast failure (`TRY_CAST` semantics).
    Null,
}

// ── Window-function support ────────────────────────────────────────────

/// Window-function identity carried by `Expr<L>::Window`. `Window` nodes
/// are compile-emitted only via sugar-accessor elimination per `14 §4.2`.
/// Roster per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowFn {
    Lag,
    Lead,
    FirstValue,
    LastValue,
    RowNumber,
    Rank,
    DenseRank,
}

/// Window frame specification carried by `Expr<L>::Window`. Per spec
/// `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowFrame {
    pub kind: WindowFrameKind,
    pub start: WindowBound,
    pub end: WindowBound,
}

/// Window frame mode. Per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowFrameKind {
    Rows,
    Range,
    Groups,
}

/// Window-frame boundary. Per spec `14 §3.3`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowBound {
    UnboundedPreceding,
    Preceding(u64),
    CurrentRow,
    Following(u64),
    UnboundedFollowing,
}

// ── Typed literal carrier ──────────────────────────────────────────────

/// Typed literal value — single carrier shared by `PhysicalLeaf::Literal`
/// and `SemanticLeaf::Literal`. Variant list aligns 1:1 with
/// [`semstrait_core::DataType`] plus `Null`. Per spec `14 §3.3`,
/// `35 §3.4`.
///
/// `Float(f64)` makes this enum non-`Eq` / non-`Hash` (NaN inequality);
/// downstream code that needs hash-comparable literal keys MUST normalize
/// or work over a stable string spelling.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Decimal {
        value: String,
        precision: u8,
        scale: i8,
    },
    String(String),
    Date(String),
    Time {
        value: String,
        precision: u8,
    },
    Timestamp {
        value: String,
        precision: u8,
    },
    Interval(String),
    Binary(Vec<u8>),
    Null,
}

// ── Identifier carriers ────────────────────────────────────────────────

/// Physical column reference. Newtype-over-stable per `30 §4.3` —
/// `.0` access is intentional; no `#[non_exhaustive]`.
///
/// Carried by `PhysicalLeaf::Column` and (under auto-mapping)
/// `SemanticLeaf::Column`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnRef(pub String);

/// Declared semantic-entity name. Newtype-over-stable per `30 §4.3` —
/// `.0` access is intentional; no `#[non_exhaustive]`.
///
/// Carried by `SemanticLeaf::Field` / `Dimension` / `Measure` / `Metric`
/// / `Key`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemanticsName(pub String);

/// Canonical function identity per `[14a §2](../../docs/design/foundations/14a_function_catalog.md)`.
/// Newtype-over-stable per `30 §4.3` — `.0` access is intentional; no
/// `#[non_exhaustive]`.
///
/// **Placement note.** Lives here pending `FunctionRegistry` stand-up;
/// relocates to the `functions` module when `14a §2` lands. See module
/// docs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalFn(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn binary_op_kind_equality_and_hash() {
        let a: HashSet<BinaryOpKind> = [BinaryOpKind::Add, BinaryOpKind::Eq].into_iter().collect();
        assert!(a.contains(&BinaryOpKind::Add));
        assert!(a.contains(&BinaryOpKind::Eq));
        assert!(!a.contains(&BinaryOpKind::Subtract));
        assert_eq!(BinaryOpKind::And, BinaryOpKind::And);
        assert_ne!(BinaryOpKind::And, BinaryOpKind::Or);
    }

    #[test]
    fn unary_op_kind_equality_and_hash() {
        let a: HashSet<UnaryOpKind> = [UnaryOpKind::Negate, UnaryOpKind::Not].into_iter().collect();
        assert_eq!(a.len(), 2);
        assert_ne!(UnaryOpKind::Negate, UnaryOpKind::Not);
    }

    #[test]
    fn aggregation_op_equality_and_hash() {
        let a: HashSet<AggregationOp> = [AggregationOp::Sum, AggregationOp::Count]
            .into_iter()
            .collect();
        assert!(a.contains(&AggregationOp::Sum));
        assert!(!a.contains(&AggregationOp::Avg));
    }

    #[test]
    fn like_kind_equality_and_hash() {
        let a: HashSet<LikeKind> = [LikeKind::Like, LikeKind::ILike].into_iter().collect();
        assert!(a.contains(&LikeKind::Like));
        assert!(a.contains(&LikeKind::ILike));
        assert!(!a.contains(&LikeKind::NotLike));
    }

    #[test]
    fn cast_failure_equality_and_hash() {
        let a: HashSet<CastFailure> = [CastFailure::Error, CastFailure::Null].into_iter().collect();
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn window_fn_equality_and_hash() {
        let a: HashSet<WindowFn> = [WindowFn::Lag, WindowFn::Lead, WindowFn::RowNumber]
            .into_iter()
            .collect();
        assert_eq!(a.len(), 3);
        assert_ne!(WindowFn::Rank, WindowFn::DenseRank);
    }

    #[test]
    fn window_bound_equality_and_hash() {
        let a: HashSet<WindowBound> = [
            WindowBound::UnboundedPreceding,
            WindowBound::Preceding(5),
            WindowBound::CurrentRow,
        ]
        .into_iter()
        .collect();
        assert!(a.contains(&WindowBound::Preceding(5)));
        assert!(!a.contains(&WindowBound::Preceding(7)));
    }

    #[test]
    fn window_frame_kind_equality_and_hash() {
        let a: HashSet<WindowFrameKind> =
            [WindowFrameKind::Rows, WindowFrameKind::Range].into_iter().collect();
        assert_eq!(a.len(), 2);
        assert_ne!(WindowFrameKind::Rows, WindowFrameKind::Groups);
    }

    #[test]
    fn window_frame_struct_equality_and_hash() {
        let f = WindowFrame {
            kind: WindowFrameKind::Rows,
            start: WindowBound::UnboundedPreceding,
            end: WindowBound::CurrentRow,
        };
        let g = f.clone();
        assert_eq!(f, g);
        let mut set = HashSet::new();
        set.insert(f);
        assert!(set.contains(&g));
    }

    #[test]
    fn literal_round_trip_via_debug() {
        // Each variant produces a Debug rendering and round-trips via
        // Clone + PartialEq.
        let cases = vec![
            Literal::Boolean(true),
            Literal::Integer(-42),
            Literal::Float(std::f64::consts::PI),
            Literal::Decimal {
                value: "1.23".to_string(),
                precision: 4,
                scale: 2,
            },
            Literal::String("hello".to_string()),
            Literal::Date("2026-05-19".to_string()),
            Literal::Time {
                value: "12:34:56".to_string(),
                precision: 0,
            },
            Literal::Timestamp {
                value: "2026-05-19T12:34:56".to_string(),
                precision: 6,
            },
            Literal::Interval("P1D".to_string()),
            Literal::Binary(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            Literal::Null,
        ];
        for lit in cases {
            let cloned = lit.clone();
            assert_eq!(lit, cloned);
            // Debug rendering must be non-empty for every variant.
            let dbg = format!("{:?}", lit);
            assert!(!dbg.is_empty(), "Debug empty for {:?}", lit);
        }
    }

    #[test]
    fn literal_float_nan_is_not_equal_to_itself() {
        // Sanity check that we did not derive Eq on Literal — f64 NaN
        // inequality is the architectural reason.
        let nan = Literal::Float(f64::NAN);
        assert_ne!(nan, nan.clone());
    }

    #[test]
    fn column_ref_equality_and_hash() {
        let a = ColumnRef("amount".to_string());
        let b = ColumnRef("amount".to_string());
        assert_eq!(a, b);
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        // Direct .0 access is part of the newtype-over-stable contract.
        let c = ColumnRef("other".to_string());
        assert_eq!(c.0, "other");
    }

    #[test]
    fn semantics_name_equality_and_hash() {
        let a = SemanticsName("revenue".to_string());
        let b = SemanticsName("revenue".to_string());
        assert_eq!(a, b);
        let c = SemanticsName("cost".to_string());
        assert_ne!(a, c);
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn canonical_fn_equality_and_hash() {
        let a = CanonicalFn("coalesce".to_string());
        let b = CanonicalFn("coalesce".to_string());
        assert_eq!(a, b);
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        let c = CanonicalFn("substring".to_string());
        assert!(!set.contains(&c));
    }
}
