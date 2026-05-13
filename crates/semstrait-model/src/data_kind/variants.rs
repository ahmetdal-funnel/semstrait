//! Variant + form tags carried by every concrete DataKind type
//! (`32 §3.4`).

use serde::{Deserialize, Serialize};

/// Variant tag — one of the four data-kind families. Carried on the
/// trait base accessor so generic code can dispatch without inspecting
/// concrete types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DataKindVariant {
    Dataset,
    Grainset,
    Unionset,
    Joinset,
}

impl DataKindVariant {
    /// Order used by [`crate::SemanticModel::iter_all`] for the
    /// `(variant-tag, name)` sort key per `32 §7`.
    pub fn ordering_key(self) -> u8 {
        match self {
            Self::Dataset => 0,
            Self::Grainset => 1,
            Self::Unionset => 2,
            Self::Joinset => 3,
        }
    }
}

/// Form tag — Public (top-level) vs Nested (structural shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DataKindForm {
    Public,
    Nested,
}
