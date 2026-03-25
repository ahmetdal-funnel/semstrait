//! Simplified compute connector trait.
//!
//! Connectors receive a `PlanArtifact` and execute it against a compute engine.
//! The adapter (which produces artifacts) is accessible via `adapter()`.

use crate::payload::{ComputeResult, ConnectorError};
use semstrait_adapter::EngineAdapter;
use semstrait_ir::PlanArtifact;

/// The main async execution interface.
///
/// Each connector:
/// 1. Holds a reference to its `EngineAdapter` (for profile access)
/// 2. Accepts a `PlanArtifact` for execution
/// 3. Optionally executes against a compute engine
#[async_trait::async_trait]
pub trait ComputeConnector: Send + Sync {
    /// The adapter that produces artifacts for this engine.
    fn adapter(&self) -> &dyn EngineAdapter;

    /// Execute a plan artifact against the compute engine.
    async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError>;

    /// Health check — verify the engine is reachable.
    async fn health_check(&self) -> Result<(), ConnectorError>;

    /// Human-readable connector name.
    fn name(&self) -> &str;
}
