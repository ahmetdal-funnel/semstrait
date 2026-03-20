//! RequestParser — converts RawQueryRequest → validated/resolved request.

use crate::error::ParseError;
use crate::types::RawQueryRequest;
use semstrait_manifest::{CompiledKind, CompiledManifest};
use semstrait_planner::request::{
    OrderByClause, ResolvedQueryRequest, SortDirection,
};

/// Parses raw query requests against a compiled manifest.
pub struct RequestParser;

impl RequestParser {
    /// Basic structural validation (no manifest needed).
    pub fn parse(raw: &RawQueryRequest) -> Result<ValidatedRequest, ParseError> {
        if raw.from.is_empty() {
            return Err(ParseError::EntityNotFound("empty entity name".to_string()));
        }

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

        // Resolve entity (kind or dataset).
        let entity = manifest
            .resolve_entity(&raw.from)
            .ok_or_else(|| ParseError::EntityNotFound(raw.from.clone()))?;
        let kind = entity.as_kind();

        // Expand "*" and classify select names.
        let select_names = expand_select(&raw.select, kind);
        let (dimensions, measures) = classify_select(&select_names, kind, &raw.from)?;

        // Resolve named filters against kind-level filters.
        for filter_name in &raw.filters {
            if !kind.filters.iter().any(|f| f.name == *filter_name) {
                return Err(ParseError::FilterNotFound {
                    entity: raw.from.clone(),
                    name: filter_name.clone(),
                });
            }
        }

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
            entity_name: raw.from.clone(),
            dimensions,
            measures,
            filters: vec![], // v1: named filters are applied by the planner via kind-level filters
            grain: None, // v1: grain conversion deferred
            limit: raw.limit,
            order_by,
            domain_hint: None,
            session_variables: raw.session.clone(),
        })
    }
}

/// Expand `["*"]` into all dimension + measure + metric names from the entity.
/// If no `*` is present, returns the select list as-is.
fn expand_select(select: &[String], kind: &CompiledKind) -> Vec<String> {
    if select.len() == 1 && select[0] == "*" {
        let mut names: Vec<String> = Vec::new();
        names.extend(kind.dimensions.keys().cloned());
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
    kind: &CompiledKind,
    entity_name: &str,
) -> Result<(Vec<String>, Vec<String>), ParseError> {
    let mut dimensions = Vec::new();
    let mut measures = Vec::new();

    for name in names {
        if kind.dimensions.contains_key(name) {
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
    pub entity_name: String,
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
            from: "sales".to_string(),
            select: vec!["region".to_string(), "revenue".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(result.is_ok());
        let req = result.unwrap();
        assert_eq!(req.entity_name, "sales");
        assert_eq!(req.select, vec!["region", "revenue"]);
    }

    #[test]
    fn test_parse_star_select() {
        let raw = RawQueryRequest {
            from: "sales".to_string(),
            select: vec!["*".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_empty_from() {
        let raw = RawQueryRequest {
            from: "".to_string(),
            select: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(matches!(result, Err(ParseError::EntityNotFound(_))));
    }

    #[test]
    fn test_parse_empty_select() {
        let raw = RawQueryRequest {
            from: "sales".to_string(),
            ..Default::default()
        };

        let result = RequestParser::parse(&raw);
        assert!(matches!(result, Err(ParseError::Validation(_))));
    }

    #[test]
    fn test_raw_filters_rejected() {
        use crate::types::RawFilter;

        let raw = RawQueryRequest {
            from: "sales".to_string(),
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
