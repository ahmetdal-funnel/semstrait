//! Root-level [`SemanticModel`] builder — `32 §9.7.6`.
//!
//! Backing storage is a `bon`-derived `SemanticModelStorage` that carries
//! every collection as a `Vec<(Location, _)>` so cross-source append
//! happens uniformly (`32 §9.7.5`, D-10). The bon derive owns the struct
//! plus state machinery; the public setters, the cross-source merge
//! semantics for `name` / `description` / `ai_context` (first-wins), and
//! the duplicate-name plus validate pass inside `.build()` are written
//! by hand.

use crate::builder::dedup;
use crate::data_kind::{Dataset, Grainset, Joinset, Unionset};
use crate::entities::{AiContext, Dimension, Measure, Metric, Relationship};
use crate::error::build::ModelBuildErrorKind;
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use crate::validate::validate;
use bon::Builder;
use semstrait_core::diagnostic::{split_by_severity, Diagnostics, Location};
use std::collections::BTreeMap;

/// Internal Vec-backed storage that the bon-derived builder fills in.
/// Materialised into a [`SemanticModel`] (with `Vec → BTreeMap` first-
/// wins dedup) by [`SemanticModelBuilder::build`].
#[derive(Debug, Clone, Builder)]
#[builder(
    builder_type(name = SemanticModelBuilder, vis = "pub"),
    state_mod(name = semantic_model_builder, vis = "pub"),
    start_fn(name = builder, vis = "pub(crate)"),
    finish_fn(name = finalize_storage, vis = "pub(crate)"),
    derive(Debug, Clone),
)]
#[non_exhaustive]
pub(crate) struct SemanticModelStorage {
    #[builder(field)]
    pub(crate) name: String,
    #[builder(field)]
    pub(crate) description: Option<String>,
    #[builder(field)]
    pub(crate) ai_context: Option<AiContext>,
    #[builder(field)]
    pub(crate) labels: Vec<String>,
    #[builder(field)]
    pub(crate) datasets: Vec<(Location, Dataset)>,
    #[builder(field)]
    pub(crate) grainsets: Vec<(Location, Grainset)>,
    #[builder(field)]
    pub(crate) unionsets: Vec<(Location, Unionset)>,
    #[builder(field)]
    pub(crate) joinsets: Vec<(Location, Joinset)>,
    #[builder(field)]
    pub(crate) dimensions: Vec<(Location, Dimension)>,
    #[builder(field)]
    pub(crate) measures: Vec<(Location, Measure)>,
    #[builder(field)]
    pub(crate) metrics: Vec<(Location, Metric)>,
    #[builder(field)]
    pub(crate) relationships: Vec<Relationship>,
}

impl SemanticModel {
    pub fn builder() -> SemanticModelBuilder {
        SemanticModelStorage::builder()
    }
}

/// Sentinel [`Location`] for code-built entries (no parse origin).
fn synthetic_location() -> Location {
    Location::new("<builder>")
}

impl<S: semantic_model_builder::State> SemanticModelBuilder<S> {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        if self.name.is_empty() {
            self.name = name.into();
        }
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        if self.description.is_none() {
            self.description = Some(description.into());
        }
        self
    }

    pub fn ai_context(mut self, ai_context: AiContext) -> Self {
        if self.ai_context.is_none() {
            self.ai_context = Some(ai_context);
        }
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.labels.extend(labels.into_iter().map(Into::into));
        self
    }

    pub fn dataset(mut self, d: Dataset) -> Self {
        self.datasets.push((synthetic_location(), d));
        self
    }

    pub fn grainset(mut self, g: Grainset) -> Self {
        self.grainsets.push((synthetic_location(), g));
        self
    }

    pub fn unionset(mut self, u: Unionset) -> Self {
        self.unionsets.push((synthetic_location(), u));
        self
    }

    pub fn joinset(mut self, j: Joinset) -> Self {
        self.joinsets.push((synthetic_location(), j));
        self
    }

    pub fn dimension(mut self, d: Dimension) -> Self {
        self.dimensions.push((synthetic_location(), d));
        self
    }

    pub fn measure(mut self, m: Measure) -> Self {
        self.measures.push((synthetic_location(), m));
        self
    }

    pub fn metric(mut self, m: Metric) -> Self {
        self.metrics.push((synthetic_location(), m));
        self
    }

    pub fn relationship(mut self, r: Relationship) -> Self {
        self.relationships.push(r);
        self
    }

    pub fn relationships(mut self, items: impl IntoIterator<Item = Relationship>) -> Self {
        self.relationships.extend(items);
        self
    }

    // ── Internal location-aware inserters used by `yaml::root::lower`
    //    and `loader::SemanticModelLoader::build` so parsed entries
    //    carry their YAML pointer / source label through to dup-
    //    detection. ─────────────────────────────────────────────────

    pub(crate) fn dataset_at(mut self, d: Dataset, loc: Location) -> Self {
        self.datasets.push((loc, d));
        self
    }
    pub(crate) fn grainset_at(mut self, g: Grainset, loc: Location) -> Self {
        self.grainsets.push((loc, g));
        self
    }
    pub(crate) fn unionset_at(mut self, u: Unionset, loc: Location) -> Self {
        self.unionsets.push((loc, u));
        self
    }
    pub(crate) fn joinset_at(mut self, j: Joinset, loc: Location) -> Self {
        self.joinsets.push((loc, j));
        self
    }
    pub(crate) fn dimension_at(mut self, d: Dimension, loc: Location) -> Self {
        self.dimensions.push((loc, d));
        self
    }
    pub(crate) fn measure_at(mut self, m: Measure, loc: Location) -> Self {
        self.measures.push((loc, m));
        self
    }
    pub(crate) fn metric_at(mut self, m: Metric, loc: Location) -> Self {
        self.metrics.push((loc, m));
        self
    }
}

/// Read-only accessors used by `parse::check_identifiers` over a
/// builder that has not yet been finalised.
impl<S: semantic_model_builder::State> SemanticModelBuilder<S> {
    #[allow(dead_code)]
    pub(crate) fn name_view(&self) -> &str {
        &self.name
    }
    pub(crate) fn datasets_view(&self) -> &[(Location, Dataset)] {
        &self.datasets
    }
    pub(crate) fn grainsets_view(&self) -> &[(Location, Grainset)] {
        &self.grainsets
    }
    pub(crate) fn unionsets_view(&self) -> &[(Location, Unionset)] {
        &self.unionsets
    }
    pub(crate) fn joinsets_view(&self) -> &[(Location, Joinset)] {
        &self.joinsets
    }
    pub(crate) fn dimensions_view(&self) -> &[(Location, Dimension)] {
        &self.dimensions
    }
    pub(crate) fn measures_view(&self) -> &[(Location, Measure)] {
        &self.measures
    }
    pub(crate) fn metrics_view(&self) -> &[(Location, Metric)] {
        &self.metrics
    }
    pub(crate) fn relationships_view(&self) -> &[Relationship] {
        &self.relationships
    }
}

impl<S: semantic_model_builder::State> SemanticModelBuilder<S> {
    /// Finalise the builder. Runs uniform SR-3 / SR-E-3 dup detection
    /// across the Vec storage, materialises `Vec → BTreeMap` with
    /// first-write-wins, and then runs the full
    /// [`crate::validate`] pipeline. Diagnostics are lifted into
    /// [`ModelBuildErrorKind::Validate`].
    pub fn build(
        self,
    ) -> Result<(SemanticModel, Diagnostics<ModelBuildErrorKind>), Diagnostics<ModelBuildErrorKind>>
    {
        let storage = self.finalize_storage();

        let mut validate_diags: Diagnostics<ValidateErrorKind> = Vec::new();
        dedup::collect_duplicate_data_kinds(&storage, &mut validate_diags);
        dedup::collect_duplicate_shared_semantics(&storage, &mut validate_diags);

        let model = materialize(storage);

        match validate(&model) {
            Ok(extra) => validate_diags.extend(extra),
            Err(extra) => validate_diags.extend(extra),
        }

        let lifted: Diagnostics<ModelBuildErrorKind> = validate_diags
            .into_iter()
            .map(|d| d.map_kind(ModelBuildErrorKind::Validate))
            .collect();

        let (errors, warnings) = split_by_severity(lifted);
        if errors.is_empty() {
            Ok((model, warnings))
        } else {
            let mut combined = errors;
            combined.extend(warnings);
            Err(combined)
        }
    }
}

/// Materialise `Vec`-backed storage into the canonical
/// `BTreeMap`-keyed [`SemanticModel`]. First-write-wins on key
/// collision — the duplicate diagnostic carries every occurrence's
/// [`Location`], so the surviving entry's identity is well-defined.
fn materialize(storage: SemanticModelStorage) -> SemanticModel {
    let mut datasets = BTreeMap::new();
    for (_, d) in storage.datasets {
        datasets.entry(d.body.base.name.clone()).or_insert(d);
    }
    let mut grainsets = BTreeMap::new();
    for (_, g) in storage.grainsets {
        grainsets.entry(g.body.base.name.clone()).or_insert(g);
    }
    let mut unionsets = BTreeMap::new();
    for (_, u) in storage.unionsets {
        unionsets.entry(u.body.base.name.clone()).or_insert(u);
    }
    let mut joinsets = BTreeMap::new();
    for (_, j) in storage.joinsets {
        joinsets.entry(j.body.base.name.clone()).or_insert(j);
    }
    let mut dimensions = BTreeMap::new();
    for (_, d) in storage.dimensions {
        dimensions
            .entry(d.name.as_str().to_string())
            .or_insert(d);
    }
    let mut measures = BTreeMap::new();
    for (_, m) in storage.measures {
        measures.entry(m.name.as_str().to_string()).or_insert(m);
    }
    let mut metrics = BTreeMap::new();
    for (_, m) in storage.metrics {
        metrics.entry(m.name.as_str().to_string()).or_insert(m);
    }
    SemanticModel {
        name: storage.name,
        description: storage.description,
        ai_context: storage.ai_context,
        labels: storage.labels,
        datasets,
        grainsets,
        unionsets,
        joinsets,
        dimensions,
        measures,
        metrics,
        relationships: storage.relationships,
    }
}
