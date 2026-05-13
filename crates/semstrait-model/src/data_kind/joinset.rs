//! `JoinsetBody`, `Joinset`, `NestedJoinset` — `32 §3.2`, `§3.3`.

use crate::data_kind::base::{ComplexExtras, DataKindBase};
use crate::data_kind::dataset::NestedDataset;
use crate::data_kind::grainset::NestedGrainset;
use crate::data_kind::unionset::NestedUnionset;
use crate::entities::ai::AiContext;
use crate::entities::relationship::Relationship;
use crate::entities::semantic_interface::SemanticInterface;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JoinsetBody {
    #[serde(flatten)]
    pub base: DataKindBase<ComplexExtras>,

    /// Joinset-local relationships. Unified `Relationship` shape per
    /// `18 §2`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<NestedDataset>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grainsets: Vec<NestedGrainset>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unionsets: Vec<NestedUnionset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Joinset {
    #[serde(flatten)]
    pub body: JoinsetBody,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[serde(flatten, default)]
    pub semantic_interface: SemanticInterface,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NestedJoinset {
    #[serde(flatten)]
    pub body: JoinsetBody,
}
