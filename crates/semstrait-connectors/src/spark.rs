//! Spark compute connector.
//!
//! Executes SQL queries against a Spark cluster via the Spark Connect gRPC
//! protocol. Requires a running Spark Connect server (Spark 3.4+).
//!
//! **Status:** Structural implementation. The connector compiles and defines
//! the full interface, but actual gRPC communication with Spark Connect
//! requires the `spark-connect-rs` crate or a custom proto client, which is
//! deferred to a follow-up due to dependency alignment constraints.

use crate::payload::{
    ComputePayload, ComputeRequest, ComputeResult, ConnectorError, EmitError,
    PayloadKind,
};
use crate::traits::{ComputeAdapter, ComputeConnector, ComputeEmitter};
use semstrait_core::ConsumerProfile;
use semstrait_sql::TargetDialect;

/// Spark Connect-based compute connector.
///
/// Submits SQL queries to a Spark cluster via Spark Connect (gRPC).
/// Requires Spark 3.4+ with Connect server enabled.
pub struct SparkConnector {
    endpoint: String,
    session_id: String,
    profile: ConsumerProfile,
}

impl SparkConnector {
    /// Create a new Spark connector.
    ///
    /// `endpoint` should be the Spark Connect gRPC endpoint (e.g., `sc://spark:15002`).
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            profile: ConsumerProfile::default(),
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

// ── ComputeEmitter ──────────────────────────────────────────────────────────

impl ComputeEmitter for SparkConnector {
    fn emit_sql(&self, sql: &str) -> Result<ComputePayload, EmitError> {
        Ok(ComputePayload::Sql(sql.to_string()))
    }

    fn emit_substrait(&self, _plan_bytes: &[u8]) -> Result<ComputePayload, EmitError> {
        Err(EmitError::UnsupportedNode(
            "Spark connector does not support Substrait payloads".to_string(),
        ))
    }

    fn supported_payloads(&self) -> &[PayloadKind] {
        &[PayloadKind::Sql]
    }
}

// ── ComputeAdapter ──────────────────────────────────────────────────────────

impl ComputeAdapter for SparkConnector {
    fn consumer_profile(&self) -> &ConsumerProfile {
        &self.profile
    }

}

// ── ComputeConnector ────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl ComputeConnector for SparkConnector {
    async fn execute(&self, request: ComputeRequest) -> Result<ComputeResult, ConnectorError> {
        let _sql = match request.payload {
            ComputePayload::Sql(sql) => sql,
            _ => {
                return Err(ConnectorError::Internal(
                    "SparkConnector only supports SQL payloads".to_string(),
                ))
            }
        };

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

    fn preferred_dialect(&self) -> TargetDialect {
        TargetDialect::Spark
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
    fn test_preferred_dialect() {
        let conn = SparkConnector::new("sc://spark:15002");
        assert_eq!(conn.preferred_dialect(), TargetDialect::Spark);
    }

    #[test]
    fn test_supported_payloads() {
        let conn = SparkConnector::new("sc://spark:15002");
        assert_eq!(conn.supported_payloads(), &[PayloadKind::Sql]);
    }

    #[test]
    fn test_emit_sql() {
        let conn = SparkConnector::new("sc://spark:15002");
        let payload = conn.emit_sql("SELECT 1").unwrap();
        assert!(matches!(payload, ComputePayload::Sql(_)));
    }

    #[test]
    fn test_emit_substrait_not_supported() {
        let conn = SparkConnector::new("sc://spark:15002");
        assert!(conn.emit_substrait(&[]).is_err());
    }

    #[test]
    fn test_adapt_sql() {
        let conn = SparkConnector::new("sc://spark:15002");
        let payload = ComputePayload::Sql("SELECT 1".to_string());
        assert!(conn.adapt(payload).is_ok());
    }

    #[test]
    fn test_adapt_substrait_rejected() {
        let conn = SparkConnector::new("sc://spark:15002");
        let payload = ComputePayload::SubstraitPlan(vec![]);
        assert!(conn.adapt(payload).is_err());
    }

    #[tokio::test]
    async fn test_execute_not_implemented() {
        let conn = SparkConnector::new("sc://spark:15002");
        let payload = ComputePayload::Sql("SELECT 1".to_string());
        let request = conn.adapt(payload).unwrap();
        let result = conn.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConnectorError::NotImplemented(_)));
    }
}
