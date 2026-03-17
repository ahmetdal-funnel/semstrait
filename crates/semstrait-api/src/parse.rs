//! RequestParser — converts RawQueryRequest → validated/resolved request.

use crate::error::ParseError;
use crate::types::{RawFilter, RawQueryRequest};
use semstrait_manifest::CompiledManifest;
use semstrait_planner::request::{
    FilterOperator, FilterValue, OrderByClause, QueryFilter, ResolvedQueryRequest, SortDirection,
};

/// Parses raw query requests against a compiled manifest.
pub struct RequestParser;

impl RequestParser {
    /// Basic structural validation (no manifest needed).
    pub fn parse(raw: &RawQueryRequest) -> Result<ValidatedRequest, ParseError> {
        if raw.kind.is_empty() {
            return Err(ParseError::KindNotFound("empty kind name".to_string()));
        }

        if raw.dimensions.is_empty() && raw.measures.is_empty() {
            return Err(ParseError::Validation(
                "at least one dimension or measure must be specified".to_string(),
            ));
        }

        Ok(ValidatedRequest {
            kind_name: raw.kind.clone(),
            dimensions: raw.dimensions.clone(),
            measures: raw.measures.clone(),
            grain: raw.grain.clone(),
            limit: raw.limit,
        })
    }

    /// Convert a RawQueryRequest into a fully resolved planner request,
    /// validating names against the compiled manifest.
    pub fn to_resolved(
        raw: &RawQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<ResolvedQueryRequest, ParseError> {
        // Basic validation.
        let _ = Self::parse(raw)?;

        // Validate kind exists.
        let kind = manifest
            .get_kind(&raw.kind)
            .ok_or_else(|| ParseError::KindNotFound(raw.kind.clone()))?;

        // Validate dimensions.
        for dim in &raw.dimensions {
            if !kind.dimensions.contains_key(dim) {
                return Err(ParseError::DimensionNotFound {
                    kind: raw.kind.clone(),
                    name: dim.clone(),
                });
            }
        }

        // Validate measures/metrics.
        for mea in &raw.measures {
            if !kind.measures.contains_key(mea) && !kind.metrics.contains_key(mea) {
                return Err(ParseError::MeasureNotFound {
                    kind: raw.kind.clone(),
                    name: mea.clone(),
                });
            }
        }

        // Convert filters.
        let filters = raw
            .filters
            .iter()
            .map(convert_filter)
            .collect::<Result<Vec<_>, _>>()?;

        // Convert order_by.
        let order_by = raw
            .order_by
            .iter()
            .map(|ob| OrderByClause {
                field: ob.field.clone(),
                direction: if ob.direction.to_lowercase() == "desc" {
                    SortDirection::Descending
                } else {
                    SortDirection::Ascending
                },
            })
            .collect();

        Ok(ResolvedQueryRequest {
            kind_name: raw.kind.clone(),
            dimensions: raw.dimensions.clone(),
            measures: raw.measures.clone(),
            filters,
            grain: None, // v1: grain conversion deferred
            limit: raw.limit,
            order_by,
            domain_hint: None,
            session_variables: raw.session.clone(),
        })
    }
}

/// Convert a raw API filter to a planner QueryFilter.
fn convert_filter(raw: &RawFilter) -> Result<QueryFilter, ParseError> {
    let operator = match raw.operator.to_lowercase().as_str() {
        "eq" | "=" | "==" => FilterOperator::Eq,
        "neq" | "!=" | "<>" => FilterOperator::NotEq,
        "lt" | "<" => FilterOperator::Lt,
        "lte" | "<=" => FilterOperator::LtEq,
        "gt" | ">" => FilterOperator::Gt,
        "gte" | ">=" => FilterOperator::GtEq,
        "in" => FilterOperator::In,
        "not_in" | "notin" => FilterOperator::NotIn,
        "between" => FilterOperator::Between,
        "is_null" | "isnull" => FilterOperator::IsNull,
        "is_not_null" | "isnotnull" => FilterOperator::IsNotNull,
        other => {
            return Err(ParseError::InvalidFilter(format!(
                "unknown operator: {}",
                other
            )))
        }
    };

    let values = convert_filter_value(&raw.value)?;

    Ok(QueryFilter {
        field: raw.dimension.clone(),
        operator,
        values,
    })
}

/// Convert a serde_json::Value to FilterValue(s).
fn convert_filter_value(value: &serde_json::Value) -> Result<Vec<FilterValue>, ParseError> {
    match value {
        serde_json::Value::String(s) => Ok(vec![FilterValue::String(s.clone())]),
        serde_json::Value::Number(n) => {
            let f = n
                .as_f64()
                .ok_or_else(|| ParseError::InvalidFilter("non-numeric number".to_string()))?;
            Ok(vec![FilterValue::Number(f)])
        }
        serde_json::Value::Bool(b) => Ok(vec![FilterValue::Bool(*b)]),
        serde_json::Value::Null => Ok(vec![FilterValue::Null]),
        serde_json::Value::Array(arr) => {
            let mut values = Vec::new();
            for item in arr {
                let mut v = convert_filter_value(item)?;
                values.append(&mut v);
            }
            Ok(values)
        }
        _ => Err(ParseError::InvalidFilter(format!(
            "unsupported filter value: {}",
            value
        ))),
    }
}

/// A validated query request (subset of ResolvedQueryRequest from planner).
#[derive(Debug, Clone)]
pub struct ValidatedRequest {
    pub kind_name: String,
    pub dimensions: Vec<String>,
    pub measures: Vec<String>,
    pub grain: Option<String>,
    pub limit: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_request() {
        let raw = RawQueryRequest {
            kind: "sales".to_string(),
            dimensions: vec!["region".to_string()],
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(result.is_ok());
        let req = result.unwrap();
        assert_eq!(req.kind_name, "sales");
        assert_eq!(req.dimensions, vec!["region"]);
        assert_eq!(req.measures, vec!["revenue"]);
    }

    #[test]
    fn test_parse_empty_kind() {
        let raw = RawQueryRequest {
            kind: "".to_string(),
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(matches!(result, Err(ParseError::KindNotFound(_))));
    }

    #[test]
    fn test_parse_no_dimensions_or_measures() {
        let raw = RawQueryRequest {
            kind: "sales".to_string(),
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(matches!(result, Err(ParseError::Validation(_))));
    }

    #[test]
    fn test_convert_filter_operators() {
        let tests = vec![
            ("eq", FilterOperator::Eq),
            ("=", FilterOperator::Eq),
            ("neq", FilterOperator::NotEq),
            ("!=", FilterOperator::NotEq),
            ("lt", FilterOperator::Lt),
            ("gt", FilterOperator::Gt),
            ("in", FilterOperator::In),
            ("between", FilterOperator::Between),
            ("is_null", FilterOperator::IsNull),
        ];

        for (op_str, expected) in tests {
            let raw = RawFilter {
                dimension: "region".to_string(),
                operator: op_str.to_string(),
                value: serde_json::json!("US"),
            };
            let filter = convert_filter(&raw).unwrap();
            assert_eq!(filter.operator, expected, "operator '{}' failed", op_str);
        }
    }

    #[test]
    fn test_convert_array_filter_value() {
        let values = convert_filter_value(&serde_json::json!(["US", "EU"])).unwrap();
        assert_eq!(values.len(), 2);
    }
}
