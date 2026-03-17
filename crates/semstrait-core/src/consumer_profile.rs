//! Consumer profile for compute engine capabilities.
//!
//! Lives in core to break the circular dependency between planner and connectors.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// ConsumerProfile describes the capabilities of a compute engine.
/// Connectors produce it via ComputeAdapter::consumer_profile().
/// The planner reads it to make strategy decisions (e.g., semi-additive handling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerProfile {
    /// Whether the engine supports window functions.
    pub supports_window_functions: bool,

    /// Whether the engine supports FULL OUTER JOIN.
    pub supports_full_outer_join: bool,

    /// Whether the engine supports CTEs (WITH clauses).
    pub supports_cte: bool,

    /// Whether the engine supports Substrait FetchRel (LIMIT/OFFSET).
    pub supports_fetch_rel: bool,

    /// Maximum join depth before requiring subqueries or CTEs.
    pub max_join_depth: Option<usize>,

    /// Set of Substrait function URIs supported by the engine.
    pub substrait_function_uris: HashSet<String>,
}

impl ConsumerProfile {
    /// Create a default profile with common capabilities.
    pub fn default_sql() -> Self {
        ConsumerProfile {
            supports_window_functions: true,
            supports_full_outer_join: true,
            supports_cte: true,
            supports_fetch_rel: true,
            max_join_depth: Some(10),
            substrait_function_uris: HashSet::new(),
        }
    }

    /// Create a minimal profile with limited capabilities.
    pub fn minimal() -> Self {
        ConsumerProfile {
            supports_window_functions: false,
            supports_full_outer_join: false,
            supports_cte: false,
            supports_fetch_rel: false,
            max_join_depth: Some(5),
            substrait_function_uris: HashSet::new(),
        }
    }

    /// Determine the strategy for handling semi-additive measures.
    pub fn semi_additive_strategy(&self) -> SemiAdditiveStrategy {
        if self.supports_window_functions {
            SemiAdditiveStrategy::WindowFunction
        } else {
            SemiAdditiveStrategy::DoubleAggregate
        }
    }
}

impl Default for ConsumerProfile {
    fn default() -> Self {
        Self::default_sql()
    }
}

/// Strategy for handling semi-additive measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemiAdditiveStrategy {
    /// Use window functions (ROW_NUMBER) to select the latest value per grain.
    WindowFunction,

    /// Use double aggregation: first select max date per grain, then join back.
    DoubleAggregate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profile() {
        let profile = ConsumerProfile::default();

        assert!(profile.supports_window_functions);
        assert!(profile.supports_full_outer_join);
        assert!(profile.supports_cte);
        assert!(profile.supports_fetch_rel);
        assert_eq!(profile.max_join_depth, Some(10));
    }

    #[test]
    fn test_minimal_profile() {
        let profile = ConsumerProfile::minimal();

        assert!(!profile.supports_window_functions);
        assert!(!profile.supports_full_outer_join);
        assert!(!profile.supports_cte);
        assert!(!profile.supports_fetch_rel);
        assert_eq!(profile.max_join_depth, Some(5));
    }

    #[test]
    fn test_semi_additive_strategy() {
        let profile = ConsumerProfile::default();
        assert_eq!(
            profile.semi_additive_strategy(),
            SemiAdditiveStrategy::WindowFunction
        );

        let profile = ConsumerProfile::minimal();
        assert_eq!(
            profile.semi_additive_strategy(),
            SemiAdditiveStrategy::DoubleAggregate
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let profile = ConsumerProfile::default();

        let json = serde_json::to_string(&profile).unwrap();
        let parsed: ConsumerProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(profile.supports_window_functions, parsed.supports_window_functions);
        assert_eq!(profile.supports_cte, parsed.supports_cte);
    }
}
