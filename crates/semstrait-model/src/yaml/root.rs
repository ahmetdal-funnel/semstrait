//! YAML-facing `semantic_model:` root.
//!
//! Authors write per-variant data kinds and shared-pool entries as
//! YAML **arrays** (`32 §1`). The canonical in-memory model uses
//! `BTreeMap<String, _>` per `32 §2`; the conversion happens at
//! [`crate::builder::SemanticModelBuilder::build`] time so cross-file
//! merges accumulate before the SR-3 / SR-E-3 dup check fires
//! (`32 §9.7.5`, D-10). [`YamlRoot::lower`] is therefore a pure
//! append: each entry is pushed onto the builder's Vec storage with
//! its YAML-pointer-bearing [`Location`].

use crate::builder::SemanticModelBuilder;
use crate::data_kind::{Dataset, Grainset, Joinset, Unionset};
use crate::entities::{AiContext, Dimension, Measure, Metric, Relationship};
use crate::model::SemanticModel;
use semstrait_core::diagnostic::Location;
use serde::Deserialize;

/// Outer wrapper — the YAML must have exactly one `semantic_model:`
/// root key (SR-1). `deny_unknown_fields` rejects every other top-
/// level key, which the caller surfaces as
/// [`crate::error::parse::ParseErrorKind::UnknownField`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct YamlRoot {
    pub(crate) semantic_model: YamlSemanticModel,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct YamlSemanticModel {
    pub(crate) name: String,

    #[serde(default)]
    pub(crate) description: Option<String>,

    #[serde(default)]
    pub(crate) ai_context: Option<AiContext>,

    #[serde(default)]
    pub(crate) labels: Vec<String>,

    #[serde(default)]
    pub(crate) datasets: Vec<Dataset>,

    #[serde(default)]
    pub(crate) grainsets: Vec<Grainset>,

    #[serde(default)]
    pub(crate) unionsets: Vec<Unionset>,

    #[serde(default)]
    pub(crate) joinsets: Vec<Joinset>,

    #[serde(default)]
    pub(crate) dimensions: Vec<Dimension>,

    #[serde(default)]
    pub(crate) measures: Vec<Measure>,

    #[serde(default)]
    pub(crate) metrics: Vec<Metric>,

    #[serde(default)]
    pub(crate) relationships: Vec<Relationship>,
}

impl YamlRoot {
    /// Append this root into a fresh [`SemanticModelBuilder`].
    pub(crate) fn lower(self, source: &str) -> SemanticModelBuilder {
        self.lower_into(source, SemanticModel::builder())
    }

    /// Append this root's contents into `builder`. Position-significant
    /// fields (`relationships:`) push to the back. Singleton fields
    /// (`name:`, `description:`, `ai_context:`) follow first-wins per
    /// the legacy `merge_models` semantics — the builder's custom
    /// setters guarantee that.
    pub(crate) fn lower_into(
        self,
        source: &str,
        mut builder: SemanticModelBuilder,
    ) -> SemanticModelBuilder {
        let YamlRoot {
            semantic_model:
                YamlSemanticModel {
                    name,
                    description,
                    ai_context,
                    labels,
                    datasets,
                    grainsets,
                    unionsets,
                    joinsets,
                    dimensions,
                    measures,
                    metrics,
                    relationships,
                },
        } = self;

        builder = builder.name(name);
        if let Some(d) = description {
            builder = builder.description(d);
        }
        if let Some(a) = ai_context {
            builder = builder.ai_context(a);
        }
        for label in labels {
            builder = builder.label(label);
        }

        for (i, d) in datasets.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/datasets/{}", i));
            builder = builder.dataset_at(d, loc);
        }
        for (i, g) in grainsets.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/grainsets/{}", i));
            builder = builder.grainset_at(g, loc);
        }
        for (i, u) in unionsets.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/unionsets/{}", i));
            builder = builder.unionset_at(u, loc);
        }
        for (i, j) in joinsets.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/joinsets/{}", i));
            builder = builder.joinset_at(j, loc);
        }

        for (i, d) in dimensions.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/dimensions/{}", i));
            builder = builder.dimension_at(d, loc);
        }
        for (i, m) in measures.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/measures/{}", i));
            builder = builder.measure_at(m, loc);
        }
        for (i, m) in metrics.into_iter().enumerate() {
            let loc = Location::new(source).with_path(format!("/metrics/{}", i));
            builder = builder.metric_at(m, loc);
        }

        for r in relationships {
            builder = builder.relationship(r);
        }

        builder
    }
}
