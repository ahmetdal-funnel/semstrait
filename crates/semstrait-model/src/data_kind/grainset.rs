//! `GrainsetBody`, `Grainset`, `NestedGrainset` — `32 §3.2`, `§3.3`.

use crate::data_kind::base::{ComplexExtras, DataKindBase};
use crate::data_kind::dataset::NestedDataset;
use crate::data_kind::joinset::NestedJoinset;
use crate::data_kind::unionset::NestedUnionset;
use crate::entities::ai::AiContext;
use crate::entities::semantic_interface::SemanticInterface;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GrainsetBody {
    #[serde(flatten)]
    pub base: DataKindBase<ComplexExtras>,

    /// Nested datasets the grainset rolls up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<NestedDataset>,

    /// Nested unionsets — per `26 §1`'s allowed-children matrix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unionsets: Vec<NestedUnionset>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joinsets: Vec<NestedJoinset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Grainset {
    #[serde(flatten)]
    pub body: GrainsetBody,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[serde(flatten, default)]
    pub semantic_interface: SemanticInterface,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NestedGrainset {
    #[serde(flatten)]
    pub body: GrainsetBody,
}
