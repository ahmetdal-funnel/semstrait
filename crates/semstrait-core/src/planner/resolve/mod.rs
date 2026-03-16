//! Kind type resolution — dispatches to grainset, unionset, or joinset algorithms.
//!
//! This is the central entry point for resolving a query against a kind definition.
//! It delegates to the specific algorithm based on `KindType`.
//!
//! Each kind type implements the [`KindResolver`] trait, which enforces a shared
//! contract: given a Kind and QueryRequest, produce a typed resolution plan.

pub mod grainset;
pub mod joinset;
pub mod unionset;

use crate::diagnostics::CompileError;
use crate::schema::model::{Kind, KindType};

// =============================================================================
// Trait
// =============================================================================

/// Trait for kind-specific resolution algorithms.
///
/// Each kind type (grainset, unionset, joinset) implements this
/// to convert a semantic query request into an algorithm-specific plan.
pub trait KindResolver {
    /// The plan type produced by this resolver.
    type Plan: std::fmt::Debug;

    /// Resolve a query against a kind definition.
    fn resolve(kind: &Kind, request: &QueryRequest) -> Result<Self::Plan, CompileError>;
}

// =============================================================================
// Implementations
// =============================================================================

/// Grainset resolver — routes to the optimal dataset(s) by grain.
pub struct GrainsetResolver;

impl KindResolver for GrainsetResolver {
    type Plan = grainset::GrainsetPlan;

    fn resolve(kind: &Kind, request: &QueryRequest) -> Result<Self::Plan, CompileError> {
        grainset::resolve(kind, request)
    }
}

/// Unionset resolver — combines datasets via UNION ALL.
pub struct UnionsetResolver;

impl KindResolver for UnionsetResolver {
    type Plan = unionset::UnionsetPlan;

    fn resolve(kind: &Kind, request: &QueryRequest) -> Result<Self::Plan, CompileError> {
        unionset::resolve(kind, request)
    }
}

/// Joinset resolver — builds a join tree from relationship graph.
///
/// Note: joinset resolution requires additional `JoinsetConfig` from the kind type.
/// The trait provides a convenience entry point; for full control use
/// [`joinset::resolve()`] directly.
#[allow(dead_code)]
pub struct JoinsetResolver;

impl KindResolver for JoinsetResolver {
    type Plan = joinset::JoinsetPlan;

    fn resolve(kind: &Kind, request: &QueryRequest) -> Result<Self::Plan, CompileError> {
        match &kind.kind_type {
            KindType::Joinset(config) => joinset::resolve(kind, config, request),
            _ => Err(CompileError::single(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::codes::PLAN_E001,
                format!("kind '{}' is not a joinset", kind.name),
            ))),
        }
    }
}

// =============================================================================
// Shared types
// =============================================================================

/// A request to resolve a query against a kind.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct QueryRequest {
    /// Requested dimension names.
    pub dimensions: Vec<String>,
    /// Requested measure names.
    pub measures: Vec<String>,
    /// Requested metric names.
    pub metrics: Vec<String>,
    /// Optional domain filter.
    pub domain: Option<String>,
    /// Aggregation function override (if applicable).
    pub aggregation: Option<String>,
}

/// Result of resolving a kind — an intermediate representation
/// that the compiler turns into a `PlanNode`.
#[derive(Debug)]
pub enum ResolvedKind {
    Grainset(grainset::GrainsetPlan),
    Unionset(unionset::UnionsetPlan),
    Joinset(joinset::JoinsetPlan),
}

// =============================================================================
// Dispatch
// =============================================================================

/// Resolve a query against a kind definition.
///
/// Dispatches to the appropriate [`KindResolver`] implementation based on `KindType`.
pub fn resolve_kind(
    kind: &Kind,
    request: &QueryRequest,
) -> Result<ResolvedKind, CompileError> {
    match &kind.kind_type {
        KindType::Grainset => {
            let plan = GrainsetResolver::resolve(kind, request)?;
            Ok(ResolvedKind::Grainset(plan))
        }
        KindType::Unionset => {
            let plan = UnionsetResolver::resolve(kind, request)?;
            Ok(ResolvedKind::Unionset(plan))
        }
        KindType::Joinset(config) => {
            let plan = joinset::resolve(kind, config, request)?;
            Ok(ResolvedKind::Joinset(plan))
        }
    }
}
