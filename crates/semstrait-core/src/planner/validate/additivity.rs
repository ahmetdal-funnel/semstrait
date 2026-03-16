//! Additivity routing for semi-additive and non-additive measures.
//!
//! Determines how a measure should be aggregated based on its additivity
//! declaration and the query's GROUP BY dimensions.
//!
//! - **Full**: no special handling — standard aggregation.
//! - **Semi**: if a non-additive dimension is NOT in GROUP BY, inject a
//!   pre-resolution sub-query that collapses via the resolution strategy.
//! - **Non**: must use source-grain dataset; reject if pre-aggregated.

use crate::diagnostics::Diagnostic;
use crate::schema::model::{Additivity, AdditivityType, ResolutionStrategy};

/// The action the planner should take for a measure's additivity.
#[derive(Debug, Clone, PartialEq)]
pub enum AdditivityAction {
    /// Standard aggregation, no special handling needed.
    Standard,
    /// Needs a pre-resolution sub-query for the given dimensions.
    PreResolve {
        /// Dimensions that require pre-resolution.
        dimensions: Vec<String>,
        /// Strategy to apply (latest → MAX, earliest → MIN, etc.).
        strategy: ResolutionStrategy,
    },
    /// Non-additive: must use source-grain dataset.
    SourceGrainRequired,
}

/// Determine the additivity action for a measure given the query dimensions.
///
/// Returns `Ok(action)` on success, or `Err(Diagnostic)` if the additivity
/// configuration is invalid for the query.
pub fn resolve_additivity(
    _measure_name: &str,
    additivity: Option<&Additivity>,
    query_dimensions: &[String],
) -> Result<AdditivityAction, Diagnostic> {
    let additivity = match additivity {
        Some(a) => a,
        None => return Ok(AdditivityAction::Standard),
    };

    match &additivity.additivity_type {
        AdditivityType::Full => Ok(AdditivityAction::Standard),
        AdditivityType::Semi(semi) => {
            // Check if any non-additive dimension is NOT in the query GROUP BY
            let missing: Vec<String> = semi
                .non_additive_dimensions
                .iter()
                .filter(|d| !query_dimensions.contains(d))
                .cloned()
                .collect();

            if missing.is_empty() {
                // All non-additive dims are in GROUP BY — safe to aggregate normally
                Ok(AdditivityAction::Standard)
            } else {
                Ok(AdditivityAction::PreResolve {
                    dimensions: missing,
                    strategy: semi.resolution_strategy,
                })
            }
        }
        AdditivityType::Non => Ok(AdditivityAction::SourceGrainRequired),
    }
}

/// Return the SQL aggregation function for a resolution strategy.
/// Used when generating the pre-resolution sub-query.
pub fn resolution_strategy_agg(strategy: ResolutionStrategy) -> &'static str {
    match strategy {
        ResolutionStrategy::Latest | ResolutionStrategy::Max => "MAX",
        ResolutionStrategy::Earliest | ResolutionStrategy::Min => "MIN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::model::SemiAdditivity;

    fn semi(dims: &[&str], strategy: ResolutionStrategy) -> Additivity {
        Additivity {
            additivity_type: AdditivityType::Semi(SemiAdditivity {
                non_additive_dimensions: dims.iter().map(|s| s.to_string()).collect(),
                resolution_strategy: strategy,
            }),
        }
    }

    #[test]
    fn test_no_additivity_is_standard() {
        let action = resolve_additivity("rev", None, &[]).unwrap();
        assert_eq!(action, AdditivityAction::Standard);
    }

    #[test]
    fn test_full_is_standard() {
        let a = Additivity {
            additivity_type: AdditivityType::Full,
        };
        let action = resolve_additivity("rev", Some(&a), &[]).unwrap();
        assert_eq!(action, AdditivityAction::Standard);
    }

    #[test]
    fn test_semi_all_dims_present_is_standard() {
        let a = semi(&["account_date"], ResolutionStrategy::Latest);
        let dims = vec!["account_date".to_string(), "region".to_string()];
        let action = resolve_additivity("balance", Some(&a), &dims).unwrap();
        assert_eq!(action, AdditivityAction::Standard);
    }

    #[test]
    fn test_semi_missing_dim_triggers_pre_resolve() {
        let a = semi(&["account_date"], ResolutionStrategy::Latest);
        let dims = vec!["region".to_string()];
        let action = resolve_additivity("balance", Some(&a), &dims).unwrap();
        match action {
            AdditivityAction::PreResolve {
                dimensions,
                strategy,
            } => {
                assert_eq!(dimensions, vec!["account_date"]);
                assert_eq!(strategy, ResolutionStrategy::Latest);
            }
            other => panic!("expected PreResolve, got {:?}", other),
        }
    }

    #[test]
    fn test_non_additive() {
        let a = Additivity {
            additivity_type: AdditivityType::Non,
        };
        let action = resolve_additivity("balance", Some(&a), &[]).unwrap();
        assert_eq!(action, AdditivityAction::SourceGrainRequired);
    }

    #[test]
    fn test_resolution_strategy_agg() {
        assert_eq!(resolution_strategy_agg(ResolutionStrategy::Latest), "MAX");
        assert_eq!(resolution_strategy_agg(ResolutionStrategy::Earliest), "MIN");
        assert_eq!(resolution_strategy_agg(ResolutionStrategy::Max), "MAX");
        assert_eq!(resolution_strategy_agg(ResolutionStrategy::Min), "MIN");
    }
}
