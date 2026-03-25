//! Spark compute connector.
//!
//! Executes SQL queries against a Spark cluster via the Spark Connect gRPC
//! protocol. Requires a running Spark Connect server (Spark 3.4+).
//!
//! **Status:** Structural implementation. The connector compiles and defines
//! the full interface, but actual gRPC communication with Spark Connect
//! requires the `spark-connect-rs` crate or a custom proto client, which is
//! deferred to a follow-up due to dependency alignment constraints.

use crate::payload::{ComputeResult, ConnectorError};
use crate::traits::ComputeConnector;
use semstrait_adapter::{EngineAdapter, SparkAdapter};
use semstrait_ir::PlanArtifact;

/// Spark Connect-based compute connector.
///
/// Submits SQL queries to a Spark cluster via Spark Connect (gRPC).
/// Requires Spark 3.4+ with Connect server enabled.
pub struct SparkConnector {
    endpoint: String,
    session_id: String,
    adapter: SparkAdapter,
}

impl SparkConnector {
    /// Create a new Spark connector.
    ///
    /// `endpoint` should be the Spark Connect gRPC endpoint (e.g., `sc://spark:15002`).
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            adapter: SparkAdapter,
        }
    }

    /// Set a custom session ID.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = session_id.into();
        self
    }

    /// Get the Spark Connect endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

// ── ComputeConnector ────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl ComputeConnector for SparkConnector {
    fn adapter(&self) -> &dyn EngineAdapter {
        &self.adapter
    }

    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError> {
        let _sql = artifact.as_sql().ok_or_else(|| {
            ConnectorError::Execution(
                "Spark connector currently requires SQL artifact".to_string(),
            )
        })?;

        // TODO: Implement Spark Connect gRPC client.
        // This requires either:
        // 1. spark-connect-rs crate (needs prost/tonic version alignment)
        // 2. Custom proto compilation from spark/connect/v1/*.proto
        //
        // For now, return NotImplemented so callers get a clear error.
        Err(ConnectorError::NotImplemented(format!(
            "Spark Connect execution not yet implemented (endpoint: {})",
            self.endpoint
        )))
    }

    async fn health_check(&self) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented(format!(
            "Spark Connect health check not yet implemented (endpoint: {})",
            self.endpoint
        )))
    }

    fn name(&self) -> &str {
        "spark"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let conn = SparkConnector::new("sc://spark:15002")
            .with_session_id("test-session");

        assert_eq!(conn.name(), "spark");
        assert_eq!(conn.endpoint(), "sc://spark:15002");
        assert_eq!(conn.session_id, "test-session");
    }

    #[test]
    fn test_adapter_accessible() {
        let conn = SparkConnector::new("sc://spark:15002");
        let adapter = conn.adapter();
        assert_eq!(adapter.name(), "spark");
    }

    #[tokio::test]
    async fn test_execute_not_implemented() {
        let conn = SparkConnector::new("sc://spark:15002");
        let artifact = PlanArtifact::Sql("SELECT 1".to_string());
        let result = conn.execute(&artifact).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConnectorError::NotImplemented(_)));
    }
}
