//! `SemanticModel` root type — `32 §2`.

use crate::data_kind::{
    AnyDataKindRef, ComplexDataKindRef, DataKindVariant, Dataset, Grainset, Joinset,
    PublicDataKindRef, SimpleDataKindRef, Unionset,
};
use crate::entities::{AiContext, Dimension, Measure, Metric, Relationship};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Root in-memory representation of a `semstrait` model. Every field
/// is `pub` so consumers can destructure without getter boilerplate
/// per `32 §2`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SemanticModel {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,

    // ── Data kinds (per-variant typed maps) ──
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub datasets: BTreeMap<String, Dataset>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub grainsets: BTreeMap<String, Grainset>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unionsets: BTreeMap<String, Unionset>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub joinsets: BTreeMap<String, Joinset>,

    // ── Shared Semantics pools ──
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dimensions: BTreeMap<String, Dimension>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measures: BTreeMap<String, Measure>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<String, Metric>,

    // ── Cross-entity relationships (position-significant) ──
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
}

impl SemanticModel {
    /// Total count of top-level public data kinds across all four
    /// per-variant maps.
    pub fn public_data_kind_count(&self) -> usize {
        self.datasets.len() + self.grainsets.len() + self.unionsets.len() + self.joinsets.len()
    }

    pub fn is_empty_model(&self) -> bool {
        self.public_data_kind_count() == 0
    }

    /// Iterate every top-level public data kind in `(variant-tag, name)`
    /// order per `32 §7`. Variants order: Dataset, Grainset, Unionset,
    /// Joinset; names alphabetically within each variant.
    pub fn iter_all(&self) -> impl Iterator<Item = AnyDataKindRef<'_>> {
        let datasets = self.datasets.values().map(AnyDataKindRef::Dataset);
        let grainsets = self.grainsets.values().map(AnyDataKindRef::Grainset);
        let unionsets = self.unionsets.values().map(AnyDataKindRef::Unionset);
        let joinsets = self.joinsets.values().map(AnyDataKindRef::Joinset);
        datasets.chain(grainsets).chain(unionsets).chain(joinsets)
    }

    /// Iterate every top-level public data kind via [`PublicDataKindRef`].
    pub fn iter_public(&self) -> impl Iterator<Item = PublicDataKindRef<'_>> {
        let datasets = self.datasets.values().map(PublicDataKindRef::Dataset);
        let grainsets = self.grainsets.values().map(PublicDataKindRef::Grainset);
        let unionsets = self.unionsets.values().map(PublicDataKindRef::Unionset);
        let joinsets = self.joinsets.values().map(PublicDataKindRef::Joinset);
        datasets.chain(grainsets).chain(unionsets).chain(joinsets)
    }

    /// Iterate top-level Datasets (the only simple variant) via
    /// [`SimpleDataKindRef::Public`].
    pub fn iter_simple(&self) -> impl Iterator<Item = SimpleDataKindRef<'_>> {
        self.datasets.values().map(SimpleDataKindRef::Public)
    }

    /// Iterate top-level complex data kinds via [`ComplexDataKindRef`].
    pub fn iter_complex(&self) -> impl Iterator<Item = ComplexDataKindRef<'_>> {
        let grainsets = self.grainsets.values().map(ComplexDataKindRef::Grainset);
        let unionsets = self.unionsets.values().map(ComplexDataKindRef::Unionset);
        let joinsets = self.joinsets.values().map(ComplexDataKindRef::Joinset);
        grainsets.chain(unionsets).chain(joinsets)
    }

    /// Look up a top-level public data kind by name, returning the
    /// matching view. Returns `None` if the name is missing from all
    /// four per-variant maps. Per SR-3 the four maps are globally
    /// disjoint, so at most one match exists.
    pub fn find_public(&self, name: &str) -> Option<PublicDataKindRef<'_>> {
        if let Some(d) = self.datasets.get(name) {
            return Some(PublicDataKindRef::Dataset(d));
        }
        if let Some(g) = self.grainsets.get(name) {
            return Some(PublicDataKindRef::Grainset(g));
        }
        if let Some(u) = self.unionsets.get(name) {
            return Some(PublicDataKindRef::Unionset(u));
        }
        if let Some(j) = self.joinsets.get(name) {
            return Some(PublicDataKindRef::Joinset(j));
        }
        None
    }

    /// Returns the variant tag where `name` lives, if any.
    pub fn variant_of(&self, name: &str) -> Option<DataKindVariant> {
        self.find_public(name).map(|r| match r {
            PublicDataKindRef::Dataset(_) => DataKindVariant::Dataset,
            PublicDataKindRef::Grainset(_) => DataKindVariant::Grainset,
            PublicDataKindRef::Unionset(_) => DataKindVariant::Unionset,
            PublicDataKindRef::Joinset(_) => DataKindVariant::Joinset,
        })
    }
}
