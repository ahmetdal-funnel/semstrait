//! Engine profile trait for compute engine capabilities.
//!
//! `EngineProfile` is the V2 abstraction that replaces direct use of `ConsumerProfile`
//! for capability queries. Concrete per-engine implementations live in `semstrait-adapter`;
//! `ConsumerProfile` implements this trait as a backward-compatible bridge.

use crate::consumer_profile::SemiAdditiveStrategy;

/// Trait describing compute engine capabilities.
///
/// Used by the planner to make strategy decisions (e.g., semi-additive handling,
/// join depth limits) and by adapters to decide what artifact to produce.
///
/// Concrete implementations live in `semstrait-adapter` (per-engine profiles).
/// `ConsumerProfile` implements this trait as a backward-compatible bridge.
pub trait EngineProfile: Send + Sync {
    /// Human-readable engine name (e.g., "datafusion", "duckdb", "trino").
    fn name(&self) -> &str;

    /// Whether the engine natively consumes Substrait plans.
    fn supports_substrait(&self) -> bool;

    /// Whether the engine supports window functions (ROW_NUMBER, etc.).
    fn supports_window_functions(&self) -> bool;

    /// Whether the engine supports FULL OUTER JOIN.
    fn supports_full_outer_join(&self) -> bool;

    /// Whether the engine supports CTEs (WITH clauses).
    fn supports_cte(&self) -> bool;

    /// Whether the engine supports subqueries in FROM/WHERE.
    fn supports_subquery(&self) -> bool;

    /// Whether the engine supports inline views (derived tables).
    fn supports_inline_views(&self) -> bool;

    /// Whether the engine supports FETCH FIRST / LIMIT.
    fn supports_fetch_rel(&self) -> bool;

    /// Maximum join depth before requiring subqueries or CTEs.
    /// None means unlimited.
    fn max_join_depth(&self) -> Option<usize>;
}

/// Determine the strategy for handling semi-additive measures based on engine capabilities.
pub fn semi_additive_strategy(profile: &dyn EngineProfile) -> SemiAdditiveStrategy {
    if profile.supports_window_functions() {
        SemiAdditiveStrategy::WindowFunction
    } else {
        SemiAdditiveStrategy::DoubleAggregate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consumer_profile::ConsumerProfile;
    use std::sync::Arc;

    #[test]
    fn test_consumer_profile_as_trait_object() {
        let profile = ConsumerProfile::default();
        let dyn_profile: &dyn EngineProfile = &profile;

        assert_eq!(dyn_profile.name(), "default");
        assert!(!dyn_profile.supports_substrait());
        assert!(dyn_profile.supports_window_functions());
        assert!(dyn_profile.supports_full_outer_join());
        assert!(dyn_profile.supports_cte());
        assert!(dyn_profile.supports_subquery());
        assert!(dyn_profile.supports_inline_views());
        assert!(dyn_profile.supports_fetch_rel());
        assert_eq!(dyn_profile.max_join_depth(), Some(10));
    }

    #[test]
    fn test_minimal_consumer_profile_as_trait_object() {
        let profile = ConsumerProfile::minimal();
        let dyn_profile: &dyn EngineProfile = &profile;

        assert_eq!(dyn_profile.name(), "default");
        assert!(!dyn_profile.supports_substrait());
        assert!(!dyn_profile.supports_window_functions());
        assert!(!dyn_profile.supports_full_outer_join());
        assert!(!dyn_profile.supports_cte());
        assert!(dyn_profile.supports_subquery());
        assert!(dyn_profile.supports_inline_views());
        assert!(!dyn_profile.supports_fetch_rel());
        assert_eq!(dyn_profile.max_join_depth(), Some(5));
    }

    #[test]
    fn test_semi_additive_strategy_window() {
        let profile = ConsumerProfile::default();
        let strategy = semi_additive_strategy(&profile);
        assert_eq!(strategy, SemiAdditiveStrategy::WindowFunction);
    }

    #[test]
    fn test_semi_additive_strategy_double_aggregate() {
        let profile = ConsumerProfile::minimal();
        let strategy = semi_additive_strategy(&profile);
        assert_eq!(strategy, SemiAdditiveStrategy::DoubleAggregate);
    }

    #[test]
    fn test_trait_object_send_sync() {
        // Compile-time check: dyn EngineProfile must be Send + Sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ConsumerProfile>();

        // Verify we can store in Arc<dyn EngineProfile>
        let profile = ConsumerProfile::default();
        let arc_profile: Arc<dyn EngineProfile> = Arc::new(profile);
        assert_eq!(arc_profile.name(), "default");
        assert!(arc_profile.supports_window_functions());
    }

    #[test]
    fn test_arc_dyn_engine_profile_clone() {
        let profile = ConsumerProfile::default();
        let arc1: Arc<dyn EngineProfile> = Arc::new(profile);
        let arc2 = Arc::clone(&arc1);

        assert_eq!(arc1.name(), arc2.name());
        assert_eq!(arc1.supports_cte(), arc2.supports_cte());
        assert_eq!(arc1.max_join_depth(), arc2.max_join_depth());
    }

    #[test]
    fn test_consumer_profile_field_mapping_consistency() {
        // Verify that the trait impl delegates correctly to struct fields
        let profile = ConsumerProfile {
            supports_window_functions: false,
            supports_full_outer_join: true,
            supports_cte: false,
            supports_fetch_rel: true,
            max_join_depth: Some(3),
            substrait_function_uris: Default::default(),
        };
        let dyn_profile: &dyn EngineProfile = &profile;

        assert_eq!(dyn_profile.supports_window_functions(), profile.supports_window_functions);
        assert_eq!(dyn_profile.supports_full_outer_join(), profile.supports_full_outer_join);
        assert_eq!(dyn_profile.supports_cte(), profile.supports_cte);
        assert_eq!(dyn_profile.supports_fetch_rel(), profile.supports_fetch_rel);
        assert_eq!(dyn_profile.max_join_depth(), profile.max_join_depth);
    }
}
