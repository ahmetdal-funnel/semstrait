//! Compile-emitted, plan-bound parameter placeholder per spec `14 §5` /
//! `35 §5.2`.
//!
//! [`Parameter`] is the only non-resolved leaf the canonical IR carries.
//! It enters the tree exclusively via sugar-accessor elimination during
//! compile (see `14 §4.2`) — author-facing parsers do not accept
//! `Parameter` syntax. The planner substitutes every `Parameter` against
//! the `Request` during plan construction; the postcondition is that
//! **no `Parameter` survives into adapt time** (`14 §5.3`). A `Parameter`
//! reaching an adapter is a hard error owned by the planner
//! (`PlanErrorKind`), not by `35`.
//!
//! [`ParameterKey`] is a closed set of typed keys — adding a member is
//! additive per I10 and is not author-extensible. v1 carries exactly the
//! two keys needed by Family-B-sugar elimination (`14 §4.2`); future keys
//! land via `#[non_exhaustive]` additions.

use semstrait_core::DataType;

/// Plan-bound parameter placeholder. Substituted by the planner during
/// Phase B per `19 §6` / `34`. Per spec `14 §5.1`, `35 §5.2`.
///
/// Carries a typed key (not a stringly identifier) plus a mandatory
/// `data_type` at compile-emit time so downstream stages can reason about
/// the placeholder's eventual concrete shape without re-deriving it.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// Typed key from the closed [`ParameterKey`] set.
    pub key: ParameterKey,
    /// Canonical logical type of the eventual concrete value, attached at
    /// emit time.
    pub data_type: DataType,
}

/// Closed set of typed parameter keys. Internal to the canonical pipeline —
/// not author-extensible. v1 carries the two keys needed by sugar-accessor
/// elimination (`14 §4.2`); future keys land additively per I10.
/// Per spec `14 §5.2`, `35 §5.2`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParameterKey {
    /// All declared request dimensions except the temporal axis. Used as
    /// the `partition_by` slot of the lowered `Window` when sugar
    /// accessors apply per-group temporal sugar (`14 §4.2`).
    RequestDimensionsMinusTemporal,
    /// The request's declared temporal axis. Used as the `order_by` slot
    /// of the lowered `Window` (`14 §4.2`).
    RequestTemporalAxis,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parameter_key_equality_and_hash() {
        let set: HashSet<ParameterKey> = [
            ParameterKey::RequestDimensionsMinusTemporal,
            ParameterKey::RequestTemporalAxis,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&ParameterKey::RequestDimensionsMinusTemporal));
        assert!(set.contains(&ParameterKey::RequestTemporalAxis));
        assert_ne!(
            ParameterKey::RequestDimensionsMinusTemporal,
            ParameterKey::RequestTemporalAxis
        );
    }

    #[test]
    fn parameter_key_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<ParameterKey>();
    }

    #[test]
    fn parameter_equality_and_clone() {
        let a = Parameter {
            key: ParameterKey::RequestDimensionsMinusTemporal,
            data_type: DataType::Integer,
        };
        let b = a.clone();
        assert_eq!(a, b);

        let c = Parameter {
            key: ParameterKey::RequestTemporalAxis,
            data_type: DataType::Integer,
        };
        assert_ne!(a, c);

        let d = Parameter {
            key: ParameterKey::RequestDimensionsMinusTemporal,
            data_type: DataType::String,
        };
        assert_ne!(a, d);
    }

    #[test]
    fn parameter_carries_data_type_for_each_canonical_kind() {
        // Sanity sweep — `data_type` admits any canonical DataType.
        for dt in [
            DataType::Integer,
            DataType::Number,
            DataType::String,
            DataType::Boolean,
            DataType::Date,
            DataType::Binary,
        ] {
            let p = Parameter {
                key: ParameterKey::RequestTemporalAxis,
                data_type: dt.clone(),
            };
            assert_eq!(p.data_type, dt);
        }
    }
}
