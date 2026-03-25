//! Consumer profile for compute engine capabilities.
//!
//! Lives in core to break the circular dependency between planner and connectors.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::engine_profile::EngineProfile;

/// ConsumerProfile describes the capabilities of a compute engine.
/// Implements `EngineProfile` and is used as the default profile by the planner.
/// The `profile_from_adapter()` function bridges `EngineAdapter` flags into this struct.
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

impl EngineProfile for ConsumerProfile {
    fn name(&self) -> &str {
        "default"
    }

    fn supports_substrait(&self) -> bool {
        false
    }

    fn supports_window_functions(&self) -> bool {
        self.supports_window_functions
    }

    fn supports_full_outer_join(&self) -> bool {
        self.supports_full_outer_join
    }

    fn supports_cte(&self) -> bool {
        self.supports_cte
    }

    fn supports_subquery(&self) -> bool {
        true
    }

    fn supports_inline_views(&self) -> bool {
        true
    }

    fn supports_fetch_rel(&self) -> bool {
        self.supports_fetch_rel
    }

    fn max_join_depth(&self) -> Option<usize> {
        self.max_join_depth
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
    fn test_default_as_engine_profile() {
        use crate::engine_profile::EngineProfile;

        let profile = ConsumerProfile::default();
        let ep: &dyn EngineProfile = &profile;

        assert_eq!(ep.name(), "default");
        assert!(!ep.supports_substrait());
        assert!(ep.supports_window_functions());
        assert!(ep.supports_full_outer_join());
        assert!(ep.supports_cte());
        assert!(ep.supports_subquery());
        assert!(ep.supports_inline_views());
        assert!(ep.supports_fetch_rel());
        assert_eq!(ep.max_join_depth(), Some(10));
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
