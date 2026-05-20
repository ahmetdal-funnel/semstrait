//! Per-kind accessor enums per spec `14 §4.1` / `35 §5.1`.
//!
//! Each accessor enum is carried as `Option<…>` on the matching typed
//! [`crate::expr::leaves::SemanticLeaf`] variant. Kind agreement is
//! type-enforced at construction — a `Dimension` leaf cannot carry a
//! `MeasureAccessor`, etc. (`14 §3.7` / `14 §4.1`).
//!
//! Two structural pairings:
//! - [`MetricAccessor`] mirrors [`MeasureAccessor`] 1:1.
//! - [`KeyAccessor`] mirrors [`DimensionAccessor`] 1:1.
//!
//! The variants here are the v1 roster from `14 §4.1` verbatim. Every enum
//! is `#[non_exhaustive]` per invariant I10 — adding a sugar accessor is
//! additive and must not break exhaustively-matching consumers.

/// Sugar accessors for [`crate::expr::leaves::SemanticLeaf::Dimension`].
/// Roster per spec `14 §4.1`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DimensionAccessor {
    /// First value within the partition's window per `14 §4.2`.
    First,
    /// Last value within the partition's window per `14 §4.2`.
    Last,
    /// Lag by `n` rows along the temporal axis.
    Lag(u32),
    /// Lead by `n` rows along the temporal axis.
    Lead(u32),
}

/// Sugar accessors for [`crate::expr::leaves::SemanticLeaf::Measure`].
/// Roster per spec `14 §4.1`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeasureAccessor {
    /// Previous-period value (Lag(1) sugar).
    Previous,
    /// Next-period value (Lead(1) sugar).
    Next,
    /// Lag by `n` periods along the temporal axis.
    Lag(u32),
    /// Lead by `n` periods along the temporal axis.
    Lead(u32),
    /// Per-period delta — `value - value.previous()` lowered shape.
    Delta,
    /// Per-period percent change — `(value - value.previous()) / value.previous()`.
    PercentChange,
}

/// Sugar accessors for [`crate::expr::leaves::SemanticLeaf::Metric`].
/// Roster per spec `14 §4.1` — mirrors [`MeasureAccessor`] 1:1 (a Metric
/// is a per-group already-aggregated value at access time, structurally
/// identical to a Measure at the output projection stage).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricAccessor {
    /// Previous-period value.
    Previous,
    /// Next-period value.
    Next,
    /// Lag by `n` periods along the temporal axis.
    Lag(u32),
    /// Lead by `n` periods along the temporal axis.
    Lead(u32),
    /// Per-period delta.
    Delta,
    /// Per-period percent change.
    PercentChange,
}

/// Sugar accessors for [`crate::expr::leaves::SemanticLeaf::Key`].
/// Roster per spec `14 §4.1` — mirrors [`DimensionAccessor`] 1:1.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAccessor {
    /// First value within the partition's window per `14 §4.2`.
    First,
    /// Last value within the partition's window per `14 §4.2`.
    Last,
    /// Lag by `n` rows along the temporal axis.
    Lag(u32),
    /// Lead by `n` rows along the temporal axis.
    Lead(u32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn dimension_accessor_equality_and_hash() {
        let set: HashSet<DimensionAccessor> = [
            DimensionAccessor::First,
            DimensionAccessor::Last,
            DimensionAccessor::Lag(3),
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 3);
        assert!(set.contains(&DimensionAccessor::First));
        assert!(set.contains(&DimensionAccessor::Lag(3)));
        assert!(!set.contains(&DimensionAccessor::Lag(5)));
        assert_ne!(DimensionAccessor::Lag(3), DimensionAccessor::Lead(3));
    }

    #[test]
    fn measure_accessor_equality_and_hash() {
        let set: HashSet<MeasureAccessor> = [
            MeasureAccessor::Previous,
            MeasureAccessor::Next,
            MeasureAccessor::Delta,
            MeasureAccessor::PercentChange,
            MeasureAccessor::Lag(2),
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 5);
        assert_ne!(MeasureAccessor::Previous, MeasureAccessor::Next);
        assert_ne!(MeasureAccessor::Lag(2), MeasureAccessor::Lead(2));
        assert_ne!(MeasureAccessor::Delta, MeasureAccessor::PercentChange);
    }

    #[test]
    fn metric_accessor_equality_and_hash() {
        let set: HashSet<MetricAccessor> = [
            MetricAccessor::Previous,
            MetricAccessor::Delta,
            MetricAccessor::Lead(7),
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 3);
        assert!(!set.contains(&MetricAccessor::PercentChange));
        assert_ne!(MetricAccessor::Lag(7), MetricAccessor::Lead(7));
    }

    #[test]
    fn key_accessor_equality_and_hash() {
        let set: HashSet<KeyAccessor> = [
            KeyAccessor::First,
            KeyAccessor::Last,
            KeyAccessor::Lag(1),
            KeyAccessor::Lead(1),
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 4);
        assert!(set.contains(&KeyAccessor::First));
        assert_ne!(KeyAccessor::Lag(1), KeyAccessor::Lead(1));
    }

    #[test]
    fn accessors_are_copy() {
        // `Copy` is asserted by the derive; this test ensures we did not
        // accidentally hold state that breaks `Copy` (e.g. by switching
        // `u32` to `String` in a future edit).
        fn assert_copy<T: Copy>() {}
        assert_copy::<DimensionAccessor>();
        assert_copy::<MeasureAccessor>();
        assert_copy::<MetricAccessor>();
        assert_copy::<KeyAccessor>();
    }
}
