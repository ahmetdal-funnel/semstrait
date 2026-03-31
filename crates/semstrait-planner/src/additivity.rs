//! AdditivityResolver — restructure plans for semi/non-additive measures.
//!
//! v1 stub: all branches pass through unchanged.
//! Full window function and double-aggregate strategies will be implemented in v2.

use crate::error::PlannerError;
use crate::kind::PlanFragment;
use crate::request::ResolvedQueryRequest;
use semstrait_manifest::CompiledMeasure;
use semstrait_manifest::AdditivityType;

/// Resolves additivity concerns by restructuring the plan fragment.
pub struct AdditivityResolver;

impl AdditivityResolver {
    /// Restructure the plan fragment for semi/non-additive measures.
    ///
    /// For v1, this is a pass-through. Full window function and
    /// double-aggregate strategies will be implemented in v2.
    pub fn resolve(
        fragment: PlanFragment,
        measure: &CompiledMeasure,
        _request: &ResolvedQueryRequest,
    ) -> Result<PlanFragment, PlannerError> {
        let additivity = match &measure.additivity {
            Some(a) => a,
            None => return Ok(fragment), // Fully additive, no restructuring needed.
        };

        match additivity {
            AdditivityType::Full => Ok(fragment),
            AdditivityType::Semi(_semi) => {
                // v1 stub: would wrap with window function or double aggregate node.
                tracing::debug!(
                    "semi-additive measure '{}': v1 pass-through",
                    measure.name
                );
                Ok(fragment)
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
