//! `UnionsetBody`, `Unionset`, `NestedUnionset`, `UnionMode` —
//! `32 §3.2`, `§3.3`; `23 §4.1` for the `UnionMode` roster.

use crate::data_kind::base::{ComplexExtras, DataKindBase};
use crate::data_kind::dataset::NestedDataset;
use crate::data_kind::grainset::NestedGrainset;
use crate::data_kind::joinset::NestedJoinset;
use crate::entities::ai::AiContext;
use crate::entities::semantic_interface::SemanticInterface;
use serde::{Deserialize, Serialize};

/// Union mode — `All` is the default per `23 §4.1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UnionMode {
    #[default]
    All,
    Unique,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UnionsetBody {
    #[serde(flatten)]
    pub base: DataKindBase<ComplexExtras>,

    /// Always required per `23 §4.1`. Default `All` allows the YAML
    /// author to omit `mode:` for the common case.
    #[serde(default)]
    pub mode: UnionMode,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<NestedDataset>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grainsets: Vec<NestedGrainset>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joinsets: Vec<NestedJoinset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Unionset {
    #[serde(flatten)]
    pub body: UnionsetBody,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[serde(flatten, default)]
    pub semantic_interface: SemanticInterface,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NestedUnionset {
    #[serde(flatten)]
    pub body: UnionsetBody,
}
