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
            .get_kind(&request.entity_name)
            .ok_or_else(|| PlannerError::KindNotFound(request.entity_name.clone()))?;

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
                    let agg_func = Self::extract_agg_function(&measure.expr_source);
                    Self::check_aggregation_constraints(
                        measure_name,
                        constraints,
                        agg_func,
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
    ///
    /// The aggregation function is extracted from the measure's expr_source
    /// (e.g., "SUM(amount)" → "SUM"). This is checked against the allowed/prohibited
    /// lists in the measure's constraints.
    fn check_aggregation_constraints(
        entity_name: &str,
        constraints: &semstrait_manifest::MeasureConstraints,
        agg_function: Option<&str>,
    ) -> Result<(), PlannerError> {
        if let Some(ref agg_constraints) = constraints.aggregations {
            if let Some(func) = agg_function {
                // allowed: only these functions are permitted.
                if let Some(ref allowed) = agg_constraints.allowed {
                    if !allowed.is_empty()
                        && !allowed.iter().any(|a| a.eq_ignore_ascii_case(func))
                    {
                        return Err(PlannerError::ConstraintViolation {
                            entity: entity_name.to_string(),
                            message: format!(
                                "aggregation constraint violated: '{}' is not in allowed list [{}]",
                                func,
                                allowed.join(", ")
                            ),
                        });
                    }
                }

                // prohibited: these functions are not allowed.
                if let Some(ref prohibited) = agg_constraints.prohibited {
                    if prohibited.iter().any(|p| p.eq_ignore_ascii_case(func)) {
                        return Err(PlannerError::ConstraintViolation {
                            entity: entity_name.to_string(),
                            message: format!(
                                "aggregation constraint violated: '{}' is prohibited",
                                func,
                            ),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Extract the aggregation function name from an expression source string.
    /// E.g., "SUM(amount)" → Some("SUM"), "COUNT_DISTINCT(id)" → Some("COUNT_DISTINCT").
    fn extract_agg_function(expr_source: &str) -> Option<&str> {
        let trimmed = expr_source.trim();
        if let Some(paren_pos) = trimmed.find('(') {
            let func = trimmed[..paren_pos].trim();
            if !func.is_empty() {
                return Some(func);
            }
        }
        None
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
    fn test_agg_allowed_satisfied() {
        let manifest = make_test_manifest_with_constraints(
            None,
            Some(semstrait_manifest::AggregationConstraints {
                allowed: Some(vec!["SUM".to_string(), "AVG".to_string()]),
                prohibited: None,
            }),
        );

        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        // revenue's expr_source is "SUM(amount)", SUM is in allowed list.
        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_ok(), "SUM should be allowed");
    }

    #[test]
    fn test_agg_allowed_violated() {
        let manifest = make_test_manifest_with_constraints(
            None,
            Some(semstrait_manifest::AggregationConstraints {
                allowed: Some(vec!["AVG".to_string(), "COUNT".to_string()]),
                prohibited: None,
            }),
        );

        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        // revenue's expr_source is "SUM(amount)", SUM is NOT in allowed list.
        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_err(), "SUM should not be in allowed list");
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::ConstraintViolation { .. }
        ));
    }

    #[test]
    fn test_agg_prohibited_violated() {
        let manifest = make_test_manifest_with_constraints(
            None,
            Some(semstrait_manifest::AggregationConstraints {
                allowed: None,
                prohibited: Some(vec!["SUM".to_string()]),
            }),
        );

        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        // revenue's expr_source is "SUM(amount)", SUM is prohibited.
        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_err(), "SUM should be prohibited");
    }

    #[test]
    fn test_agg_prohibited_satisfied() {
        let manifest = make_test_manifest_with_constraints(
            None,
            Some(semstrait_manifest::AggregationConstraints {
                allowed: None,
                prohibited: Some(vec!["AVG".to_string()]),
            }),
        );

        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        // revenue's expr_source is "SUM(amount)", only AVG is prohibited.
        let result = ConstraintEvaluator::check(&request, &manifest);
        assert!(result.is_ok(), "SUM should not be prohibited when only AVG is");
    }

    #[test]
    fn test_extract_agg_function() {
        assert_eq!(ConstraintEvaluator::extract_agg_function("SUM(amount)"), Some("SUM"));
        assert_eq!(ConstraintEvaluator::extract_agg_function("COUNT_DISTINCT(id)"), Some("COUNT_DISTINCT"));
        assert_eq!(ConstraintEvaluator::extract_agg_function("AVG(price)"), Some("AVG"));
        assert_eq!(ConstraintEvaluator::extract_agg_function("plain_column"), None);
        assert_eq!(ConstraintEvaluator::extract_agg_function(""), None);
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
