//! Core compute traits for the connector pipeline.
//!
//! Three traits form the compute pipeline:
//! - `ComputeEmitter` — converts LogicalPlan to a ComputePayload
//! - `ComputeAdapter` — provides ConsumerProfile and adapts payloads
//! - `ComputeConnector` — executes compute requests asynchronously

use crate::payload::{
    AdaptError, ComputePayload, ComputeRequest, ComputeResult, ConnectorError,
    EmitError, PayloadKind,
};
use semstrait_core::ConsumerProfile;
use semstrait_sql::TargetDialect;

/// Converts a LogicalPlan into a compute-ready payload.
///
/// Each engine may support different payload kinds (SQL, Substrait, Native).
/// The emitter selects the best payload format for the target engine.
pub trait ComputeEmitter: Send + Sync {
    /// Emit a compute payload from a logical plan.
    ///
    /// V1: This takes the plan as opaque bytes (Substrait) or SQL string.
    /// V2: Will take `&LogicalPlan` from semstrait-ir.
    fn emit_sql(&self, sql: &str) -> Result<ComputePayload, EmitError>;

    /// Emit from Substrait bytes.
    fn emit_substrait(&self, plan_bytes: &[u8]) -> Result<ComputePayload, EmitError>;

    /// Which payload kinds this emitter supports.
    fn supported_payloads(&self) -> &[PayloadKind];
}

/// Provides engine capabilities and adapts payloads for execution.
///
/// `ComputeAdapter` is a supertrait of `ComputeConnector`.
/// It exposes the `ConsumerProfile` (read by the planner for strategy decisions)
/// and converts payloads into executable requests.
pub trait ComputeAdapter: Send + Sync {
    /// The engine's capability profile.
    ///
    /// Used by `SemanticPlanner` to select strategies (e.g., window functions
    /// vs double-aggregate for semi-additive measures).
    fn consumer_profile(&self) -> &ConsumerProfile;

    /// Adapt a payload into an executable request.
    fn adapt(&self, payload: ComputePayload) -> Result<ComputeRequest, AdaptError>;
}

/// The main async execution interface.
///
/// Every engine implementation satisfies this trait.
/// `ComputeConnector` extends `ComputeAdapter` — every connector can also
/// report its capabilities and adapt payloads.
#[async_trait::async_trait]
pub trait ComputeConnector: ComputeAdapter + Send + Sync {
    /// Execute a compute request and return results.
    async fn execute(&self, request: ComputeRequest) -> Result<ComputeResult, ConnectorError>;

    /// Check if the engine is reachable and healthy.
    async fn health_check(&self) -> Result<(), ConnectorError>;

    /// Human-readable name of this connector.
    fn name(&self) -> &str;

    /// The SQL dialect preferred by this engine.
    ///
    /// Used by the engine to select the appropriate SQL emitter. When the
    /// `polyglot` feature is enabled, `PolyglotEmitter` transpiles ANSI SQL
    /// to this dialect. Defaults to `Ansi` (no transpilation).
    fn preferred_dialect(&self) -> TargetDialect {
        TargetDialect::Ansi
    }
}
