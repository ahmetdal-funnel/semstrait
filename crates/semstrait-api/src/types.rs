//! API request/response types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw query request from external clients (JSON/CLI).
/// Parsed into ResolvedQueryRequest by RequestParser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawQueryRequest {
    pub kind: String,
    #[serde(default)]
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub measures: Vec<String>,
    #[serde(default)]
    pub filters: Vec<RawFilter>,
    pub grain: Option<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub order_by: Vec<RawOrderBy>,
    #[serde(default)]
    pub session: HashMap<String, String>,
}

/// Convenience alias — same as RawQueryRequest for now.
pub type QueryRequest = RawQueryRequest;

/// A filter in the raw query request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFilter {
    pub dimension: String,
    pub operator: String,
    pub value: serde_json::Value,
}

/// An order-by clause in the raw query request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawOrderBy {
    pub field: String,
    #[serde(default = "default_asc")]
    pub direction: String,
}

fn default_asc() -> String {
    "asc".to_string()
}

/// Result of an explain operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainResult {
    /// SQL string (if SQL emitter was used)
    pub sql: Option<String>,
    /// Substrait plan as JSON
    pub substrait_json: Option<String>,
    /// Human-readable plan tree
    pub plan_text: String,
}

/// Result of a validation operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}
