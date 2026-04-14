//! RequestParser — converts RawQueryRequest → validated/resolved request.

use crate::error::ParseError;
use crate::types::RawQueryRequest;
use semstrait_manifest::{CompiledManifest, CompiledInterface};
use semstrait_planner::request::{
    OrderByClause, ResolvedQueryRequest, SortDirection,
};

/// Parses raw query requests against a compiled manifest.
pub struct RequestParser;

impl RequestParser {
    /// Basic structural validation (no manifest needed).
    pub fn parse(raw: &RawQueryRequest) -> Result<ValidatedRequest, ParseError> {
        if raw.select.is_empty() {
            return Err(ParseError::Validation(
                "select must contain at least one column name or \"*\"".to_string(),
            ));
        }

        // Reject inline raw filters in v1.
        if !raw.raw_filters.is_empty() {
            return Err(ParseError::RawFiltersNotImplemented);
        }

        Ok(ValidatedRequest {
            entity_name: raw.from.clone(),
            select: raw.select.clone(),
            filters: raw.filters.clone(),
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

        // Convert order_by (entity-independent).
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

        // If `from` is None, pass through to planner for ad-hoc resolution.
        // Select names are passed as-is — the planner classifies them.
        let Some(ref from) = raw.from else {
            return Ok(ResolvedQueryRequest {
                entity_name: String::new(),
                dimensions: raw.select.clone(), // planner will reclassify
                measures: vec![],
                filters: vec![],
                grain: None,
                limit: raw.limit,
                order_by,
                session_variables: raw.session.clone(),
            });
        };

        // Resolve entity via CompiledDataKind.
        let data_kind = manifest
            .resolve(from)
            .ok_or_else(|| ParseError::EntityNotFound(from.clone()))?;
        let kind = data_kind.interface();

        // Expand "*" and classify select names.
        let select_names = expand_select(&raw.select, kind);
        let (dimensions, measures) = classify_select(&select_names, kind, from)?;

        // Resolve named filters against kind-level filters.
        for filter_name in &raw.filters {
            if !kind.filters.iter().any(|f| f.name == *filter_name) {
                return Err(ParseError::FilterNotFound {
                    entity: from.clone(),
                    name: filter_name.clone(),
                });
            }
        }

        Ok(ResolvedQueryRequest {
            entity_name: from.clone(),
            dimensions,
            measures,
            filters: vec![],
            grain: None,
            limit: raw.limit,
            order_by,
            session_variables: raw.session.clone(),
        })
    }
}

/// Expand `["*"]` into all dimension + measure + metric + key names from the entity.
/// If no `*` is present, returns the select list as-is.
fn expand_select(select: &[String], kind: &CompiledInterface) -> Vec<String> {
    if select.len() == 1 && select[0] == "*" {
        let mut names: Vec<String> = Vec::new();
        names.extend(kind.dimensions.keys().cloned());
        // Include key columns not already in dimensions.
        if let Some(ref keys) = kind.keys {
            for key_col in keys.all_column_names() {
                if !kind.dimensions.contains_key(&key_col) {
                    names.push(key_col);
                }
            }
        }
        names.extend(kind.measures.keys().cloned());
        names.extend(kind.metrics.keys().cloned());
        names
    } else {
        select.to_vec()
    }
}

/// Classify select names into dimensions and measures/metrics.
/// Returns `(dimensions, measures)` where measures includes both measures and metrics.
fn classify_select(
    names: &[String],
    kind: &CompiledInterface,
    entity_name: &str,
) -> Result<(Vec<String>, Vec<String>), ParseError> {
    let mut dimensions = Vec::new();
    let mut measures = Vec::new();

    // Collect key column names for classification.
    let key_names: std::collections::HashSet<String> = kind
        .keys
        .as_ref()
        .map(|k| k.all_column_names().into_iter().collect())
        .unwrap_or_default();

    for name in names {
        if kind.dimensions.contains_key(name) {
            dimensions.push(name.clone());
        } else if key_names.contains(name) {
            // Keys contribute to GROUP BY — classify as dimension.
            dimensions.push(name.clone());
        } else if kind.measures.contains_key(name) || kind.metrics.contains_key(name) {
            measures.push(name.clone());
        } else {
            return Err(ParseError::UnknownSelectName {
                entity: entity_name.to_string(),
                name: name.clone(),
            });
        }
    }

    Ok((dimensions, measures))
}

/// A validated query request (structural validation only, no manifest).
#[derive(Debug, Clone)]
pub struct ValidatedRequest {
    pub entity_name: Option<String>,
    pub select: Vec<String>,
    pub filters: Vec<String>,
    pub grain: Option<String>,
    pub limit: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_request() {
        let raw = RawQueryRequest {
            from: Some("sales".to_string()),
            select: vec!["region".to_string(), "revenue".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(result.is_ok());
        let req = result.unwrap();
        assert_eq!(req.entity_name, Some("sales".to_string()));
        assert_eq!(req.select, vec!["region", "revenue"]);
    }

    #[test]
    fn test_parse_star_select() {
        let raw = RawQueryRequest {
            from: Some("sales".to_string()),
            select: vec!["*".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_no_from() {
        let raw = RawQueryRequest {
            from: None,
            select: vec!["revenue".to_string()],
            ..Default::default()
        };

        // None passes structural validation — entity resolution happens in planner.
        let result = RequestParser::parse(&raw);
        assert!(result.is_ok());
        let req = result.unwrap();
        assert_eq!(req.entity_name, None);
    }

    #[test]
    fn test_parse_empty_select() {
        let raw = RawQueryRequest {
            from: Some("sales".to_string()),
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(matches!(result, Err(ParseError::Validation(_))));
    }

    #[test]
    fn test_raw_filters_rejected() {
        use crate::types::RawFilter;

        let raw = RawQueryRequest {
            from: Some("sales".to_string()),
            select: vec!["revenue".to_string()],
            raw_filters: vec![RawFilter {
                field: "region".to_string(),
                operator: "=".to_string(),
                value: serde_json::json!("US"),
            }],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(matches!(result, Err(ParseError::RawFiltersNotImplemented)));
    }
}
