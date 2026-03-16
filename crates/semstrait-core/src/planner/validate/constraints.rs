//! Constraint validation for measures and metrics.
//!
//! Checks a query's requested dimensions and aggregation against the
//! `MeasureConstraints` defined in the schema. Also validates that
//! key columns are not used with distributional aggregations (SUM, AVG).

use crate::diagnostics::{codes, Diagnostic, ValidationReport};
use crate::schema::model::{MeasureConstraints, Keys};

/// Dimensions and aggregation context for a query being validated.
pub struct QueryContext<'a> {
    /// Names of dimensions in the query's GROUP BY / scope.
    pub dimensions: &'a [String],
    /// Aggregation function being applied (as uppercase string, e.g. "SUM").
    pub aggregation: Option<&'a str>,
}

/// Check a measure's constraints against a query context.
///
/// Returns a `ValidationReport` — callers decide whether to treat warnings
/// differently from errors.
pub fn check_constraints(
    measure_name: &str,
    constraints: &MeasureConstraints,
    query: &QueryContext<'_>,
) -> ValidationReport {
    let mut report = ValidationReport::new();

    if let Some(dim_constraints) = &constraints.dimensions {
        // one_of: at least one of the listed dimensions must be in query scope
        if let Some(one_of) = &dim_constraints.one_of {
            let found = one_of.iter().any(|d| query.dimensions.contains(d));
            if !found {
                report.push(
                    Diagnostic::error(
                        codes::CONST_E001,
                        format!(
                            "measure '{}': requires at least one of [{}] in query dimensions",
                            measure_name,
                            one_of.join(", ")
                        ),
                    )
                    .with_entity(format!("measures.{}.constraints.dimensions.one_of", measure_name), measure_name),
                );
            }
        }

        // none_of: none of the listed dimensions may be in query scope
        if let Some(none_of) = &dim_constraints.none_of {
            for dim in none_of {
                if query.dimensions.contains(dim) {
                    report.push(
                        Diagnostic::error(
                            codes::CONST_E002,
                            format!(
                                "measure '{}': dimension '{}' is prohibited in query scope",
                                measure_name, dim
                            ),
                        )
                        .with_entity(
                            format!("measures.{}.constraints.dimensions.none_of", measure_name),
                            measure_name,
                        ),
                    );
                }
            }
        }

        // all: all listed dimensions must be in query scope
        if let Some(all) = &dim_constraints.all {
            for dim in all {
                if !query.dimensions.contains(dim) {
                    report.push(
                        Diagnostic::error(
                            codes::CONST_E003,
                            format!(
                                "measure '{}': dimension '{}' is required in query scope",
                                measure_name, dim
                            ),
                        )
                        .with_entity(
                            format!("measures.{}.constraints.dimensions.all", measure_name),
                            measure_name,
                        ),
                    );
                }
            }
        }
    }

    if let Some(agg_str) = query.aggregation {
        if let Some(agg_constraints) = &constraints.aggregations {
            let agg_upper = agg_str.to_uppercase();

            // allowed: aggregation must be in the list
            if let Some(allowed) = &agg_constraints.allowed {
                let allowed_upper: Vec<String> = allowed.iter().map(|a| a.to_uppercase()).collect();
                if !allowed_upper.contains(&agg_upper) {
                    report.push(
                        Diagnostic::error(
                            codes::CONST_E004,
                            format!(
                                "measure '{}': aggregation '{}' is not in allowed list [{}]",
                                measure_name,
                                agg_str,
                                allowed.join(", ")
                            ),
                        )
                        .with_entity(
                            format!("measures.{}.constraints.aggregations.allowed", measure_name),
                            measure_name,
                        ),
                    );
                }
            }

            // prohibited: aggregation must NOT be in the list
            if let Some(prohibited) = &agg_constraints.prohibited {
                let prohibited_upper: Vec<String> =
                    prohibited.iter().map(|a| a.to_uppercase()).collect();
                if prohibited_upper.contains(&agg_upper) {
                    report.push(
                        Diagnostic::error(
                            codes::CONST_E005,
                            format!(
                                "measure '{}': aggregation '{}' is prohibited",
                                measure_name, agg_str
                            ),
                        )
                        .with_entity(
                            format!(
                                "measures.{}.constraints.aggregations.prohibited",
                                measure_name
                            ),
                            measure_name,
                        ),
                    );
                }
            }
        }
    }

    report
}

/// Distributional aggregations that are invalid on key columns.
const KEY_INVALID_AGGS: &[&str] = &["SUM", "AVG", "MEDIAN", "PERCENTILE"];

/// Check that key columns are not aggregated with distributional functions.
pub fn check_key_aggregation(
    measure_name: &str,
    aggregation: &str,
    column_name: &str,
    keys: &Keys,
) -> Result<(), Diagnostic> {
    let is_key = keys
        .primary
        .as_ref()
        .is_some_and(|pk| pk.iter().any(|k| k == column_name));

    if is_key && KEY_INVALID_AGGS.contains(&aggregation.to_uppercase().as_str()) {
        return Err(Diagnostic::error(
            codes::CONST_E006,
            format!(
                "measure '{}': cannot apply {} to key column '{}'",
                measure_name, aggregation, column_name
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::model::{
        AggregationConstraints, DimensionConstraints,
    };

    fn make_constraints(
        one_of: Option<Vec<&str>>,
        none_of: Option<Vec<&str>>,
        all: Option<Vec<&str>>,
        allowed: Option<Vec<&str>>,
        prohibited: Option<Vec<&str>>,
    ) -> MeasureConstraints {
        MeasureConstraints {
            dimensions: Some(DimensionConstraints {
                one_of: one_of.map(|v| v.into_iter().map(String::from).collect()),
                none_of: none_of.map(|v| v.into_iter().map(String::from).collect()),
                all: all.map(|v| v.into_iter().map(String::from).collect()),
            }),
            aggregations: if allowed.is_some() || prohibited.is_some() {
                Some(AggregationConstraints {
                    allowed: allowed.map(|v| v.into_iter().map(String::from).collect()),
                    prohibited: prohibited.map(|v| v.into_iter().map(String::from).collect()),
                })
            } else {
                None
            },
        }
    }

    #[test]
    fn test_one_of_satisfied() {
        let c = make_constraints(Some(vec!["date", "region"]), None, None, None, None);
        let ctx = QueryContext {
            dimensions: &["region".into()],
            aggregation: None,
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(!report.has_errors());
    }

    #[test]
    fn test_one_of_violated() {
        let c = make_constraints(Some(vec!["date", "region"]), None, None, None, None);
        let ctx = QueryContext {
            dimensions: &["category".into()],
            aggregation: None,
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(report.has_errors());
        let err = report.finish().unwrap_err();
        assert!(err.to_string().contains("CONST_E001"));
    }

    #[test]
    fn test_none_of_violated() {
        let c = make_constraints(None, Some(vec!["internal_id"]), None, None, None);
        let ctx = QueryContext {
            dimensions: &["internal_id".into()],
            aggregation: None,
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(report.has_errors());
        let err = report.finish().unwrap_err();
        assert!(err.to_string().contains("CONST_E002"));
    }

    #[test]
    fn test_all_violated() {
        let c = make_constraints(None, None, Some(vec!["date", "region"]), None, None);
        let ctx = QueryContext {
            dimensions: &["date".into()],
            aggregation: None,
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(report.has_errors());
        let err = report.finish().unwrap_err();
        assert!(err.to_string().contains("CONST_E003"));
    }

    #[test]
    fn test_all_satisfied() {
        let c = make_constraints(None, None, Some(vec!["date", "region"]), None, None);
        let ctx = QueryContext {
            dimensions: &["date".into(), "region".into(), "extra".into()],
            aggregation: None,
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(!report.has_errors());
    }

    #[test]
    fn test_aggregation_allowed_ok() {
        let c = make_constraints(None, None, None, Some(vec!["SUM", "COUNT"]), None);
        let ctx = QueryContext {
            dimensions: &[],
            aggregation: Some("SUM"),
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(!report.has_errors());
    }

    #[test]
    fn test_aggregation_allowed_violated() {
        let c = make_constraints(None, None, None, Some(vec!["SUM", "COUNT"]), None);
        let ctx = QueryContext {
            dimensions: &[],
            aggregation: Some("AVG"),
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(report.has_errors());
        let err = report.finish().unwrap_err();
        assert!(err.to_string().contains("CONST_E004"));
    }

    #[test]
    fn test_aggregation_prohibited() {
        let c = make_constraints(None, None, None, None, Some(vec!["AVG"]));
        let ctx = QueryContext {
            dimensions: &[],
            aggregation: Some("avg"),
        };
        let report = check_constraints("rev", &c, &ctx);
        assert!(report.has_errors());
        let err = report.finish().unwrap_err();
        assert!(err.to_string().contains("CONST_E005"));
    }

    #[test]
    fn test_key_aggregation_rejected() {
        let keys = Keys {
            primary: Some(vec!["id".into()]),
            unique: None,
            foreign: None,
        };
        let result = check_key_aggregation("rev", "SUM", "id", &keys);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("CONST_E006"));
    }

    #[test]
    fn test_key_aggregation_count_ok() {
        let keys = Keys {
            primary: Some(vec!["id".into()]),
            unique: None,
            foreign: None,
        };
        let result = check_key_aggregation("rev", "COUNT", "id", &keys);
        assert!(result.is_ok());
    }

    #[test]
    fn test_non_key_aggregation_ok() {
        let keys = Keys {
            primary: Some(vec!["id".into()]),
            unique: None,
            foreign: None,
        };
        let result = check_key_aggregation("rev", "SUM", "amount", &keys);
        assert!(result.is_ok());
    }
}
