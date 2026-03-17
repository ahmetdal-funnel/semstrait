//! ConstraintEvaluator — pre-resolution validity gate (step 0).
//!
//! Checks all measure and metric constraints against the query scope
//! before any dataset routing or plan construction begins.
//!
//! "query scope" = request.dimensions + dimensions referenced in request.filters

use crate::error::PlannerError;
use crate::request::ResolvedQueryRequest;
use semstrait_manifest::CompiledManifest;
use std::collections::HashSet;

/// Evaluates measure/metric constraints against the query scope.
pub struct ConstraintEvaluator;

impl ConstraintEvaluator {
    /// Check all constraints for the requested measures/metrics.
    ///
    /// Returns `Ok(())` if all constraints are satisfied, or
    /// `Err(PlannerError::ConstraintViolation)` on the first violation.
    pub fn check(
        request: &ResolvedQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<(), PlannerError> {
        let kind = manifest
            .get_kind(&request.kind_name)
            .ok_or_else(|| PlannerError::KindNotFound(request.kind_name.clone()))?;

        // Build the query scope: all dimensions in GROUP BY + filter dimensions.
        let mut scope: HashSet<&str> = HashSet::new();
        for dim in &request.dimensions {
            scope.insert(dim.as_str());
        }
        for filter in &request.filters {
            // If the filter field is a known dimension, add it to scope.
            if kind.dimensions.contains_key(&filter.field) {
                scope.insert(filter.field.as_str());
            }
        }

        // Check constraints on each requested measure.
        for measure_name in &request.measures {
            // Check measures first.
            if let Some(measure) = kind.measures.get(measure_name) {
                if let Some(ref constraints) = measure.constraints {
                    Self::check_dimension_constraints(
                        measure_name,
                        constraints,
                        &scope,
                    )?;
                    Self::check_aggregation_constraints(
                        measure_name,
                        constraints,
                    )?;
                }
            }
            // Then check metrics.
            else if let Some(metric) = kind.metrics.get(measure_name) {
                if let Some(ref constraints) = metric.constraints {
                    Self::check_dimension_constraints(
                        measure_name,
                        constraints,
                        &scope,
                    )?;
                }
            }
            // If neither, the planner will catch this later.
        }

        Ok(())
    }

    /// Check dimension constraints (one_of, none_of, all).
    fn check_dimension_constraints(
        entity_name: &str,
        constraints: &semstrait_manifest::MeasureConstraints,
        scope: &HashSet<&str>,
    ) -> Result<(), PlannerError> {
        if let Some(ref dim_constraints) = constraints.dimensions {
            // one_of: at least one of these must be in scope.
            if let Some(ref one_of) = dim_constraints.one_of {
                if !one_of.is_empty() && !one_of.iter().any(|d| scope.contains(d.as_str())) {
                    return Err(PlannerError::ConstraintViolation {
                        entity: entity_name.to_string(),
                        message: format!(
                            "one_of constraint violated: query must include at least one of [{}]",
                            one_of.join(", ")
                        ),
                    });
                }
            }

            // none_of: none of these may be in scope.
            if let Some(ref none_of) = dim_constraints.none_of {
                for dim in none_of {
                    if scope.contains(dim.as_str()) {
                        return Err(PlannerError::ConstraintViolation {
                            entity: entity_name.to_string(),
                            message: format!(
                                "none_of constraint violated: dimension '{}' must not be in query scope",
                                dim
                            ),
                        });
                    }
                }
            }

            // all: all of these must be in scope.
            if let Some(ref all) = dim_constraints.all {
                for dim in all {
                    if !scope.contains(dim.as_str()) {
                        return Err(PlannerError::ConstraintViolation {
                            entity: entity_name.to_string(),
                            message: format!(
                                "all constraint violated: dimension '{}' must be in query scope",
                                dim
                            ),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Check aggregation constraints (allowed, prohibited).
    fn check_aggregation_constraints(
        entity_name: &str,
        constraints: &semstrait_manifest::MeasureConstraints,
    ) -> Result<(), PlannerError> {
        if let Some(ref _agg_constraints) = constraints.aggregations {
            // v1: aggregation constraint checking is a stub.
            // Full implementation requires knowing the requested aggregation function,
            // which is resolved from the measure definition.
            let _ = entity_name;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[test]
    fn test_one_of_satisfied() {
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: Some(vec!["date".to_string(), "region".to_string()]),
                none_of: None,
                all: None,
            }),
            None,
        );

        let request = make_test_request(
            "orders",
            vec!["date"],
            vec!["revenue"],
        );

        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_ok(), "one_of should be satisfied when 'date' is in dimensions");
    }

    #[test]
    fn test_one_of_violated() {
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: Some(vec!["date".to_string(), "region".to_string()]),
                none_of: None,
                all: None,
            }),
            None,
        );

        let request = make_test_request(
            "orders",
            vec!["customer"],
            vec!["revenue"],
        );

        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_err(), "one_of should fail when neither date nor region is in scope");
        let err = result.unwrap_err();
        assert!(
            matches!(err, PlannerError::ConstraintViolation { .. }),
            "should be a ConstraintViolation"
        );
    }

    #[test]
    fn test_none_of_violated() {
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: None,
                none_of: Some(vec!["user_id".to_string()]),
                all: None,
            }),
            None,
        );

        let request = make_test_request(
            "orders",
            vec!["user_id"],
            vec!["revenue"],
        );

        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_err(), "none_of should fail when user_id is in scope");
    }

    #[test]
    fn test_all_satisfied() {
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: None,
                none_of: None,
                all: Some(vec!["date".to_string(), "region".to_string()]),
            }),
            None,
        );

        let request = make_test_request(
            "orders",
            vec!["date", "region", "customer"],
            vec!["revenue"],
        );

        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_ok(), "all constraint should be satisfied");
    }

    #[test]
    fn test_all_violated() {
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: None,
                none_of: None,
                all: Some(vec!["date".to_string(), "region".to_string()]),
            }),
            None,
        );

        let request = make_test_request(
            "orders",
            vec!["date"],
            vec!["revenue"],
        );

        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_err(), "all constraint should fail when region is missing");
    }

    #[test]
    fn test_no_constraints() {
        let manifest = make_test_manifest();
        let request = make_test_request(
            "orders",
            vec!["date"],
            vec!["revenue"],
        );

        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_ok(), "should pass when no constraints exist");
    }
}
