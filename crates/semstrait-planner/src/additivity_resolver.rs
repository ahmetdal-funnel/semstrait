//! AdditivityResolver — restructure plans for semi/non-additive measures.
//!
//! Strategy is selected from ConsumerProfile:
//! - WindowFunction: ROW_NUMBER OVER (PARTITION BY non_additive_dim) + filter
//! - DoubleAggregate: sub-query with MAX/LATEST, then re-aggregate

use crate::error::PlannerError;
use crate::kind::PlanFragment;
use crate::request::ResolvedQueryRequest;
use semstrait_core::{ConsumerProfile, SemiAdditiveStrategy};
use semstrait_manifest::CompiledMeasure;
use semstrait_manifest::AdditivityType;

/// Resolves additivity concerns by restructuring the plan fragment.
pub struct AdditivityResolver;

impl AdditivityResolver {
    /// Restructure the plan fragment for semi/non-additive measures.
    ///
    /// For v1, this is largely a pass-through. Full window function and
    /// double-aggregate strategies will be implemented in v2.
    pub fn resolve(
        fragment: PlanFragment,
        measure: &CompiledMeasure,
        _request: &ResolvedQueryRequest,
        profile: &ConsumerProfile,
    ) -> Result<PlanFragment, PlannerError> {
        let additivity = match &measure.additivity {
            Some(a) => a,
            None => return Ok(fragment), // Fully additive, no restructuring needed.
        };

        match &additivity.additivity_type {
            AdditivityType::Full => Ok(fragment),
            AdditivityType::Semi(_semi) => {
                let strategy = profile.semi_additive_strategy();
                match strategy {
                    SemiAdditiveStrategy::WindowFunction => {
                        // v1 stub: would wrap with window function node.
                        tracing::debug!(
                            "semi-additive measure '{}': WindowFunction strategy (v1 pass-through)",
                            measure.name
                        );
                        Ok(fragment)
                    }
                    SemiAdditiveStrategy::DoubleAggregate => {
                        // v1 stub: would wrap with double aggregate.
                        tracing::debug!(
                            "semi-additive measure '{}': DoubleAggregate strategy (v1 pass-through)",
                            measure.name
                        );
                        Ok(fragment)
                    }
                }
            }
            AdditivityType::Non => {
                // Non-additive: v1 pass-through.
                tracing::debug!(
                    "non-additive measure '{}': v1 pass-through",
                    measure.name
                );
                Ok(fragment)
            }
        }
    }
}
