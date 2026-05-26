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
//! `CanonicalFn` lives in [`crate::functions`] per `35 §8.2` / `14a §2`.

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

/// Closed-five aggregation tag. Roster per spec `14 §3.3`. Carried as
/// `AggregateKind::Builtin(...)`. `CountDistinct` is encoded as
/// `Aggregate { op: Builtin(Count), distinct: true, .. }`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregationOp {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

/// Aggregation operator carrier — closed-five builtins or registry
/// extensions. Per `14 §3.3` and `14a §2`. Builtin variants use the
/// closed-five [`AggregationOp`]; extension variants reference a
/// [`crate::functions::CanonicalFn`] resolved against the registry.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AggregateKind {
    Builtin(AggregationOp),
    Extension(crate::functions::CanonicalFn),
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

/// Width discriminator for [`Literal::Integer`]. Per `35 §13.5`,
/// integer literals carry their declared width so the type round-trips
/// without re-inference.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegerWidth {
    Int,
    Long,
}

/// Width discriminator for [`Literal::Float`]. Per `35 §13.5`, floating
/// literals carry their declared width so the type round-trips without
/// re-inference.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloatWidth {
    Float,
    Double,
}

/// Typed literal value — single carrier shared by `PhysicalLeaf::Literal`
/// and `SemanticLeaf::Literal`. Variant list aligns 1:1 with
/// [`crate::types::DataType`]. Per spec `14 §3.3`, `35 §3.4`, `35 §13.5`.
///
/// `Float { width, .. }` makes this enum non-`Eq` / non-`Hash` (NaN
/// inequality); downstream code that needs hash-comparable literal keys
/// MUST normalize or work over a stable string spelling.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Boolean(bool),
    Integer {
        value: i64,
        width: IntegerWidth,
    },
    Float {
        value: f64,
        width: FloatWidth,
    },
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
    Null {
        data_type: crate::types::DataType,
    },
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

// `CanonicalFn` lives in [`crate::functions`] per `14a §2` / `35 §8.2`.
// See [`crate::functions::CanonicalFn`].

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn literal_round_trip_via_debug() {
        // Each variant produces a Debug rendering and round-trips via
        // Clone + PartialEq.
        let cases = vec![
            Literal::Boolean(true),
            Literal::Integer {
                value: -42,
                width: IntegerWidth::Long,
            },
            Literal::Float {
                value: std::f64::consts::PI,
                width: FloatWidth::Double,
            },
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
            Literal::Null {
                data_type: crate::types::DataType::Integer,
            },
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
        let nan = Literal::Float {
            value: f64::NAN,
            width: FloatWidth::Double,
        };
        assert_ne!(nan, nan.clone());
    }

    #[test]
    fn integer_width_serde_round_trip() {
        for w in [IntegerWidth::Int, IntegerWidth::Long] {
            let s = serde_json::to_string(&w).unwrap();
            let back: IntegerWidth = serde_json::from_str(&s).unwrap();
            assert_eq!(w, back);
        }
        // Snake-case spelling is observable on the wire.
        assert_eq!(
            serde_json::to_string(&IntegerWidth::Int).unwrap(),
            "\"int\""
        );
        assert_eq!(
            serde_json::to_string(&IntegerWidth::Long).unwrap(),
            "\"long\""
        );
    }

    #[test]
    fn float_width_serde_round_trip() {
        for w in [FloatWidth::Float, FloatWidth::Double] {
            let s = serde_json::to_string(&w).unwrap();
            let back: FloatWidth = serde_json::from_str(&s).unwrap();
            assert_eq!(w, back);
        }
        assert_eq!(
            serde_json::to_string(&FloatWidth::Float).unwrap(),
            "\"float\""
        );
        assert_eq!(
            serde_json::to_string(&FloatWidth::Double).unwrap(),
            "\"double\""
        );
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

    // `canonical_fn_*` test coverage moved to
    // `crate::functions::canonical_fn::tests` per `35 §8.2`.
}
