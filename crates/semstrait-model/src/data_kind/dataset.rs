//! `DatasetBody`, `Dataset`, `NestedDataset` — `32 §3.2`, `§3.3`.

use crate::data_kind::base::{DataKindBase, LeafExtras};
use crate::entities::ai::AiContext;
use crate::entities::semantic_interface::SemanticInterface;
use serde::{Deserialize, Serialize};

/// Per-variant body — degenerate for the leaf (no child arrays).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DatasetBody {
    #[serde(flatten)]
    pub base: DataKindBase<LeafExtras>,
}

/// Public-form `Dataset` — body plus the three Public-only fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Dataset {
    #[serde(flatten)]
    pub body: DatasetBody,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[serde(flatten, default)]
    pub semantic_interface: SemanticInterface,
}

/// Nested-form `Dataset` — body only; no Public-only fields per
/// SR-2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NestedDataset {
    #[serde(flatten)]
    pub body: DatasetBody,
}
