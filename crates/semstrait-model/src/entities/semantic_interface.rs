//! `SemanticInterface` — the per-Public-DataKind block that lists the
//! data kind's Dimensions / Measures / Metrics / Filters / Keys.
//! Authored directly on every Public form (never on `Nested*`).

use crate::entities::dimension::DimensionEntry;
use crate::entities::filter::DataKindFilter;
use crate::entities::keys::{ForeignKeyDecl, KeyDecl, Keys};
use crate::entities::measure::MeasureEntry;
use crate::entities::metric::MetricEntry;
use bon::Builder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(
    start_fn = builder,
    finish_fn = build,
    state_mod(name = semantic_interface_builder, vis = "pub(crate)"),
)]
pub struct SemanticInterface {
    /// Dimensions exposed by this DataKind. Each entry is either an
    /// inline declaration or a `ref` against the root pool (`18 §1.2`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(field)]
    pub dimensions: Vec<DimensionEntry>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(field)]
    pub measures: Vec<MeasureEntry>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(field)]
    pub metrics: Vec<MetricEntry>,

    /// DataKind-level filters. `AggregationFilter`s are authored on
    /// individual Measures / Metrics, not here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(field)]
    pub filters: Vec<DataKindFilter>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(field)]
    pub keys: Option<Keys>,
}

impl SemanticInterface {
    pub fn is_empty(&self) -> bool {
        self.dimensions.is_empty()
            && self.measures.is_empty()
            && self.metrics.is_empty()
            && self.filters.is_empty()
            && self.keys.is_none()
    }
}

// Inherent `with_*` setters — single source of truth for the
// SemanticInterface flattener logic that Public-form DataKind builder
// facades (`32 §9.7.8.5`) delegate to. Singular `with_X(item)` pushes
// onto the collection; plural `with_Xs(items)` replaces. `with_keys(k)`
// replaces the whole `keys` slot; the per-key shortcuts auto-create
// `keys` via `Option::get_or_insert_with(Default::default)`.
impl SemanticInterface {
    pub fn with_dimension(mut self, e: DimensionEntry) -> Self {
        self.dimensions.push(e);
        self
    }
    pub fn with_dimensions(mut self, items: impl IntoIterator<Item = DimensionEntry>) -> Self {
        self.dimensions = items.into_iter().collect();
        self
    }
    pub fn with_measure(mut self, e: MeasureEntry) -> Self {
        self.measures.push(e);
        self
    }
    pub fn with_measures(mut self, items: impl IntoIterator<Item = MeasureEntry>) -> Self {
        self.measures = items.into_iter().collect();
        self
    }
    pub fn with_metric(mut self, e: MetricEntry) -> Self {
        self.metrics.push(e);
        self
    }
    pub fn with_metrics(mut self, items: impl IntoIterator<Item = MetricEntry>) -> Self {
        self.metrics = items.into_iter().collect();
        self
    }
    pub fn with_filter(mut self, f: DataKindFilter) -> Self {
        self.filters.push(f);
        self
    }
    pub fn with_filters(mut self, items: impl IntoIterator<Item = DataKindFilter>) -> Self {
        self.filters = items.into_iter().collect();
        self
    }
    pub fn with_keys(mut self, k: Keys) -> Self {
        self.keys = Some(k);
        self
    }
    pub fn with_primary_key(mut self, k: KeyDecl) -> Self {
        self.keys.get_or_insert_with(Default::default).primary = Some(k);
        self
    }
    pub fn with_unique_key(mut self, k: KeyDecl) -> Self {
        self.keys.get_or_insert_with(Default::default).unique.push(k);
        self
    }
    pub fn with_foreign_key(mut self, k: ForeignKeyDecl) -> Self {
        self.keys.get_or_insert_with(Default::default).foreign.push(k);
        self
    }
}

// `SemanticInterfaceBuilder` facade — delegates to the inherent
// `SemanticInterface::with_*` methods above by taking ownership of the
// Vec/Option-backed builder fields, calling the carrier method, and
// writing back. This keeps the per-collection mutation logic in one
// place; the builder shell here only owns the bon typestate plumbing.
impl<S: semantic_interface_builder::State> SemanticInterfaceBuilder<S> {
    pub fn dimensions(mut self, items: impl IntoIterator<Item = DimensionEntry>) -> Self {
        self.dimensions = items.into_iter().collect();
        self
    }
    pub fn measures(mut self, items: impl IntoIterator<Item = MeasureEntry>) -> Self {
        self.measures = items.into_iter().collect();
        self
    }
    pub fn metrics(mut self, items: impl IntoIterator<Item = MetricEntry>) -> Self {
        self.metrics = items.into_iter().collect();
        self
    }
    pub fn filters(mut self, items: impl IntoIterator<Item = DataKindFilter>) -> Self {
        self.filters = items.into_iter().collect();
        self
    }
    pub fn keys(mut self, k: Keys) -> Self {
        self.keys = Some(k);
        self
    }
    pub fn dimension(mut self, e: DimensionEntry) -> Self {
        self.dimensions.push(e);
        self
    }
    pub fn measure(mut self, e: MeasureEntry) -> Self {
        self.measures.push(e);
        self
    }
    pub fn metric(mut self, e: MetricEntry) -> Self {
        self.metrics.push(e);
        self
    }
    pub fn filter(mut self, f: DataKindFilter) -> Self {
        self.filters.push(f);
        self
    }
    pub fn primary_key(mut self, k: KeyDecl) -> Self {
        self.keys.get_or_insert_with(Default::default).primary = Some(k);
        self
    }
    pub fn unique_key(mut self, k: KeyDecl) -> Self {
        self.keys.get_or_insert_with(Default::default).unique.push(k);
        self
    }
    pub fn foreign_key(mut self, k: ForeignKeyDecl) -> Self {
        self.keys.get_or_insert_with(Default::default).foreign.push(k);
        self
    }
}
