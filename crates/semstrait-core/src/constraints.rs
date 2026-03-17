//! Constraint types for measures and dimensions.
//!
//! These are the runtime types for the `constraints:` YAML field.
//! Evaluated at step 0 (pre-resolution). This is NOT `requires`.

use serde::{Deserialize, Serialize};

/// MeasureConstraints — the runtime type for the `constraints:` YAML field.
/// Evaluated at step 0 (pre-resolution).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasureConstraints {
    /// Constraints on which dimensions can be used with this measure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<DimensionConstraints>,

    /// Constraints on which aggregations can be used with this measure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregations: Option<AggregationConstraints>,
}

/// Constraints on which dimensions can be used with a measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionConstraints {
    /// At least one of these dimensions must be present in the query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub one_of: Vec<String>,

    /// None of these dimensions can be present in the query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub none_of: Vec<String>,

    /// All of these dimensions must be present in the query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<String>,
}

/// Constraints on which aggregations can be used with a measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationConstraints {
    /// Only these aggregations are allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<String>,

    /// These aggregations are prohibited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prohibited: Vec<String>,
}

impl MeasureConstraints {
    /// Create empty constraints (no restrictions).
    pub fn none() -> Self {
        MeasureConstraints {
            dimensions: None,
            aggregations: None,
        }
    }

    /// Check if there are any constraints.
    pub fn is_empty(&self) -> bool {
        self.dimensions.is_none() && self.aggregations.is_none()
    }
}

impl DimensionConstraints {
    /// Create empty dimension constraints.
    pub fn none() -> Self {
        DimensionConstraints {
            one_of: Vec::new(),
            none_of: Vec::new(),
            all: Vec::new(),
        }
    }

    /// Check if there are any dimension constraints.
    pub fn is_empty(&self) -> bool {
        self.one_of.is_empty() && self.none_of.is_empty() && self.all.is_empty()
    }
}

impl AggregationConstraints {
    /// Create empty aggregation constraints.
    pub fn none() -> Self {
        AggregationConstraints {
            allowed: Vec::new(),
            prohibited: Vec::new(),
        }
    }

    /// Check if there are any aggregation constraints.
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty() && self.prohibited.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measure_constraints_none() {
        let constraints = MeasureConstraints::none();
        assert!(constraints.is_empty());
        assert!(constraints.dimensions.is_none());
        assert!(constraints.aggregations.is_none());
    }

    #[test]
    fn test_dimension_constraints() {
        let constraints = DimensionConstraints {
            one_of: vec!["date".to_string()],
            none_of: vec!["user_id".to_string()],
            all: vec!["region".to_string()],
        };

        assert!(!constraints.is_empty());
        assert_eq!(constraints.one_of.len(), 1);
        assert_eq!(constraints.none_of.len(), 1);
        assert_eq!(constraints.all.len(), 1);
    }

    #[test]
    fn test_aggregation_constraints() {
        let constraints = AggregationConstraints {
            allowed: vec!["sum".to_string(), "avg".to_string()],
            prohibited: vec!["count_distinct".to_string()],
        };

        assert!(!constraints.is_empty());
        assert_eq!(constraints.allowed.len(), 2);
        assert_eq!(constraints.prohibited.len(), 1);
    }

    #[test]
    fn test_serde_roundtrip() {
        let constraints = MeasureConstraints {
            dimensions: Some(DimensionConstraints {
                one_of: vec!["date".to_string()],
                none_of: vec![],
                all: vec!["region".to_string()],
            }),
            aggregations: Some(AggregationConstraints {
                allowed: vec!["sum".to_string()],
                prohibited: vec![],
            }),
        };

        let json = serde_json::to_string(&constraints).unwrap();
        let parsed: MeasureConstraints = serde_json::from_str(&json).unwrap();

        assert_eq!(constraints, parsed);
    }

    #[test]
    fn test_serde_skip_empty() {
        let constraints = MeasureConstraints {
            dimensions: Some(DimensionConstraints {
                one_of: vec!["date".to_string()],
                none_of: vec![],
                all: vec![],
            }),
            aggregations: None,
        };

        let json = serde_json::to_string(&constraints).unwrap();

        // Should not serialize empty vecs
        assert!(!json.contains("none_of"));
        assert!(!json.contains("all"));
        assert!(!json.contains("aggregations"));
    }
}
