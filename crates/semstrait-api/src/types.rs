//! API request/response types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw query request from external clients (JSON/CLI/gRPC).
/// Parsed into ResolvedQueryRequest by RequestParser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawQueryRequest {
    /// Semantic model source (file path for CLI, inline YAML/JSON for REST/gRPC).
    #[serde(default)]
    pub model: Option<String>,
    /// Entity to query: a kind name or a dataset name.
    pub from: String,
    /// Semantic names to select — system classifies into dimensions/measures/metrics.
    /// Use `["*"]` to select all columns from the entity.
    #[serde(default)]
    pub select: Vec<String>,
    /// Named filters from the manifest.
    #[serde(default)]
    pub filters: Vec<String>,
    /// Inline filter expressions (stub — not implemented in v1).
    #[serde(default)]
    pub raw_filters: Vec<RawFilter>,
    pub grain: Option<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub order_by: Vec<RawOrderBy>,
    #[serde(default)]
    pub session: HashMap<String, String>,
    /// Engine to use for plan generation (e.g., "datafusion", "duckdb").
    /// If not set, uses the default engine from the connector.
    #[serde(default)]
    pub engine: Option<String>,
}

/// Convenience alias — same as RawQueryRequest for now.
pub type QueryRequest = RawQueryRequest;

/// A filter in the raw query request (inline expression — stub, not implemented in v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFilter {
    pub field: String,
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
    /// Human-readable plan tree (indented, similar to DataFusion EXPLAIN)
    pub plan_text: String,
}

/// Result of a validation operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}
