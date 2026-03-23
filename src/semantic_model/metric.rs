//! Metric types - derived calculations from measures

use super::types::DataType;
use serde::Deserialize;

/// A metric - calculation combining measures
#[derive(Debug, Deserialize)]
pub struct Metric {
    pub name: String,
    pub label: Option<String>,
    /// Human-readable description for UIs and LLMs
    pub description: Option<String>,
    /// Alternative names (for LLM query understanding)
    pub synonyms: Option<Vec<String>>,
    pub hidden: Option<bool>,
    pub format: Option<String>,
    /// Result data type. Defaults to F64 for metrics.
    #[serde(rename = "type")]
    pub data_type: Option<DataType>,
    /// Expression combining measures
    pub expr: MetricExpr,
}

/// Metric expression - references measures by name
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MetricExpr {
    /// Simple measure reference: "sales"
    MeasureRef(String),
    /// Structured expression
    Structured(MetricExprNode),
}

/// Expression node for metric calculations
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricExprNode {
    /// Reference a measure: { measure: "sales" }
    Measure(String),
    /// Literal number
    Literal(f64),
    /// Addition
    Add(Vec<MetricExprArg>),
    /// Subtraction
    Subtract(Vec<MetricExprArg>),
    /// Multiplication
    Multiply(Vec<MetricExprArg>),
    /// Division
    Divide(Vec<MetricExprArg>),
    /// CASE WHEN expression - for cross-grain-set metrics
    Case(MetricCaseExpr),
}

/// CASE WHEN expression for metrics
/// Used for cross-grain-set metrics that select different measures based on _dataset.path
#[derive(Debug, Clone, Deserialize)]
pub struct MetricCaseExpr {
    /// List of WHEN...THEN branches
    pub when: Vec<MetricCaseWhen>,
    /// Optional ELSE value (defaults to 0)
    #[serde(rename = "else")]
    pub else_value: Option<Box<MetricExprArg>>,
}

/// A single WHEN...THEN branch in a metric CASE expression
#[derive(Debug, Clone, Deserialize)]
pub struct MetricCaseWhen {
    /// The condition to evaluate
    pub condition: MetricCondition,
    /// The measure to use if condition is true
    pub then: MetricExprArg,
}

/// Condition expression for metric CASE WHEN
/// Supports _dataset.path comparisons for cross-grain-set metrics
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricCondition {
    /// Equal: eq: [a, b]
    Eq(Vec<MetricConditionArg>),
    /// Not equal: ne: [a, b]
    Ne(Vec<MetricConditionArg>),
    /// Glob match: match: [_dataset.path, "*.facebookads.*"] — pattern uses * for any run of characters
    Match(Vec<MetricConditionArg>),
}

/// Argument in a metric condition
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MetricConditionArg {
    /// String value (e.g., "_dataset.path" or a path literal like "google_ads")
    String(String),
    /// Literal number
    Number(f64),
}

/// Argument in a metric expression
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MetricExprArg {
    /// Shorthand: measure name as string
    MeasureName(String),
    /// Literal number
    LiteralNumber(f64),
    /// Nested expression
    Node(MetricExprNode),
}

impl Metric {
    /// Get the result data type, defaulting to F64 for metrics
    pub fn data_type(&self) -> DataType {
        self.data_type.clone().unwrap_or(DataType::F64)
    }

    /// Check if this metric is a cross-grain-set metric
    ///
    /// A cross-grain-set metric uses `_dataset.path` in CASE conditions
    /// to select different measures based on the active grain set.
    pub fn is_cross_grain_set(&self) -> bool {
        match &self.expr {
            MetricExpr::Structured(MetricExprNode::Case(case_expr)) => case_expr
                .when
                .iter()
                .any(|w| w.condition.references_grain_set()),
            _ => false,
        }
    }

    /// Extract grain-set-to-measure mappings from a cross-grain-set metric (eq/ne only).
    /// For metrics that use `match` conditions, use the planner's expanded mapping instead.
    ///
    /// Returns a vec of (grain_set_path, measure_name) tuples.
    /// Returns empty vec if not a cross-grain-set metric or if any WHEN uses `match`.
    pub fn grain_set_measures(&self) -> Vec<(String, String)> {
        match &self.expr {
            MetricExpr::Structured(MetricExprNode::Case(case_expr)) => {
                let has_match = case_expr
                    .when
                    .iter()
                    .any(|w| w.condition.grain_set_pattern().is_some());
                if has_match {
                    return vec![];
                }
                case_expr
                    .when
                    .iter()
                    .filter_map(|w| {
                        let grain_set = w.condition.grain_set_value()?;
                        let measure = w.then.measure_name()?;
                        Some((grain_set, measure))
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// CASE WHEN list for cross-grain-set metrics (for expansion in planner).
    pub fn case_when_branches(&self) -> Option<&[MetricCaseWhen]> {
        match &self.expr {
            MetricExpr::Structured(MetricExprNode::Case(case_expr)) => {
                Some(case_expr.when.as_slice())
            }
            _ => None,
        }
    }
}

impl MetricCondition {
    /// Check if this condition references _dataset.path in CASE (cross-grain-set metric).
    pub fn references_grain_set(&self) -> bool {
        match self {
            MetricCondition::Eq(args)
            | MetricCondition::Ne(args)
            | MetricCondition::Match(args) => args
                .iter()
                .any(|arg| matches!(arg, MetricConditionArg::String(s) if s == "_dataset.path")),
        }
    }

    /// Extract the path/grain set value from a condition like eq: [_dataset.path, "google_ads"].
    /// Returns None for Match (use grain_set_pattern and expand against model).
    pub fn grain_set_value(&self) -> Option<String> {
        match self {
            MetricCondition::Eq(args) if args.len() == 2 => {
                let has_path_ref = args
                    .iter()
                    .any(|a| matches!(a, MetricConditionArg::String(s) if s == "_dataset.path"));
                if has_path_ref {
                    args.iter().find_map(|a| match a {
                        MetricConditionArg::String(s) if s != "_dataset.path" => Some(s.clone()),
                        _ => None,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract the glob pattern from a condition like match: [_dataset.path, "*.facebookads.*"].
    pub fn grain_set_pattern(&self) -> Option<String> {
        match self {
            MetricCondition::Match(args) if args.len() == 2 => {
                let has_path_ref = args
                    .iter()
                    .any(|a| matches!(a, MetricConditionArg::String(s) if s == "_dataset.path"));
                if has_path_ref {
                    args.iter().find_map(|a| match a {
                        MetricConditionArg::String(s) if s != "_dataset.path" => Some(s.clone()),
                        _ => None,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl MetricExprArg {
    /// Get the measure name if this is a simple measure reference
    pub fn measure_name(&self) -> Option<String> {
        match self {
            MetricExprArg::MeasureName(name) => Some(name.clone()),
            MetricExprArg::Node(MetricExprNode::Measure(name)) => Some(name.clone()),
            _ => None,
        }
    }
}
