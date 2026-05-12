//! `PhysicalExpr` — bound-physical expression carrier consumed by
//! `SemanticMappingValue::Expr` per `18 §10` and `14 §3`.
//!
//! At the model author surface, the explicit `expr:` form under
//! `semantic_mapping:` carries a structured expression block. The
//! authoring layer represents this as a thin newtype around the
//! declarative [`crate::expr_block::ExprBlock`] tree — the same shape
//! that `ExprSource::Declarative(_)` carries elsewhere. Compile / Bind
//! lower this to the canonical [`semstrait_core::Expr`] AST when
//! resolving against a `PhysicalSource` schema.
//!
//! This newtype boundary is deliberate: it documents that an entry
//! authored under `semantic_mapping:` (post-binding) must reference
//! physical column names, while an `ExprSource` on a Dimension /
//! Measure / Metric (pre-binding) names Semantic identifiers that
//! `compile` resolves later (`11 §7` / `14 §3`).

use crate::expr_block::ExprBlock;
use serde::{Deserialize, Serialize};

/// Physical-layer expression authored under `semantic_mapping: { expr: ... }`.
///
/// Wraps an [`ExprBlock`] tree; the wrapper carries no extra fields.
/// Physical resolution (column-name binding, type-checking) runs at
/// compile, never inside `semstrait-model`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhysicalExpr {
    pub block: ExprBlock,
}

impl PhysicalExpr {
    pub fn new(block: ExprBlock) -> Self {
        Self { block }
    }
}

impl From<ExprBlock> for PhysicalExpr {
    fn from(block: ExprBlock) -> Self {
        Self { block }
    }
}
