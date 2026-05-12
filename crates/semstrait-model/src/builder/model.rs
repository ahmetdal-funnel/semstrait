//! Root-level [`SemanticModel`] builder — `32 §9.7.6`.
//!
//! Hand-rolled because the canonical storage uses `BTreeMap`s for the
//! data-kind and shared-pool collections (per `32 §7` ordering rules)
//! and a `Vec<Relationship>` for the position-significant relationship
//! list. A `bon` derive on the struct would generate per-field setters
//! that take whole maps; the per-instance inserters here let authors
//! add data kinds and entities one at a time, which matches how the
//! YAML surface is authored.
//!
//! The `.build()` step runs the full [`crate::validate`] pipeline so
//! all SR-* / SR-E-* rules apply uniformly to YAML-loaded and
//! code-built models per `32 §9.7.5`.

use crate::data_kind::{Dataset, Grainset, Joinset, Unionset};
use crate::entities::{AiContext, Dimension, Measure, Metric, Relationship};
use crate::error::build::ModelBuildErrorKind;
use crate::model::SemanticModel;
use crate::validate::validate;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};
use std::collections::BTreeMap;

/// Fluent builder for [`SemanticModel`]. Method names equal
/// [`SemanticModel`] field names per the structural-fidelity rule R1
/// (`32 §9.7.1`).
#[derive(Debug, Clone, Default)]
pub struct SemanticModelBuilder {
    name: Option<String>,
    description: Option<String>,
    ai_context: Option<AiContext>,
    labels: Vec<String>,
    datasets: BTreeMap<String, Dataset>,
    grainsets: BTreeMap<String, Grainset>,
    unionsets: BTreeMap<String, Unionset>,
    joinsets: BTreeMap<String, Joinset>,
    dimensions: BTreeMap<String, Dimension>,
    measures: BTreeMap<String, Measure>,
    metrics: BTreeMap<String, Metric>,
    relationships: Vec<Relationship>,
}

impl SemanticModel {
    /// Entry point for the root-level builder.
    pub fn builder() -> SemanticModelBuilder {
        SemanticModelBuilder::default()
    }
}

impl SemanticModelBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn ai_context(mut self, ai_context: AiContext) -> Self {
        self.ai_context = Some(ai_context);
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
        self.datasets.insert(d.body.base.name.clone(), d);
        self
    }

    pub fn grainset(mut self, g: Grainset) -> Self {
        self.grainsets.insert(g.body.base.name.clone(), g);
        self
    }

    pub fn unionset(mut self, u: Unionset) -> Self {
        self.unionsets.insert(u.body.base.name.clone(), u);
        self
    }

    pub fn joinset(mut self, j: Joinset) -> Self {
        self.joinsets.insert(j.body.base.name.clone(), j);
        self
    }

    pub fn dimension(mut self, d: Dimension) -> Self {
        self.dimensions.insert(d.name.as_str().to_string(), d);
        self
    }

    pub fn measure(mut self, m: Measure) -> Self {
        self.measures.insert(m.name.as_str().to_string(), m);
        self
    }

    pub fn metric(mut self, m: Metric) -> Self {
        self.metrics.insert(m.name.as_str().to_string(), m);
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

    /// Finalize and run [`crate::validate`] on the constructed model.
    /// Errors / warnings are surfaced as
    /// [`ModelBuildErrorKind::Validate`] in the same accumulating
    /// vector returned by the loader (`32 §9.6`).
    pub fn build(
        self,
    ) -> Result<(SemanticModel, Diagnostics<ModelBuildErrorKind>), Diagnostics<ModelBuildErrorKind>>
    {
        let model = self.into_unvalidated();
        match validate(&model) {
            Ok(diags) => {
                let lifted: Diagnostics<ModelBuildErrorKind> = diags
                    .into_iter()
                    .map(map_validate_diagnostic)
                    .collect();
                Ok((model, lifted))
            }
            Err(diags) => {
                let lifted: Diagnostics<ModelBuildErrorKind> = diags
                    .into_iter()
                    .map(map_validate_diagnostic)
                    .collect();
                Err(lifted)
            }
        }
    }

    /// Finalize without running [`crate::validate`]. Intended for
    /// inspector / round-trip tooling that needs a parsed-only model.
    pub fn build_unvalidated(self) -> SemanticModel {
        self.into_unvalidated()
    }

    fn into_unvalidated(self) -> SemanticModel {
        SemanticModel {
            name: self.name.unwrap_or_default(),
            description: self.description,
            ai_context: self.ai_context,
            labels: self.labels,
            datasets: self.datasets,
            grainsets: self.grainsets,
            unionsets: self.unionsets,
            joinsets: self.joinsets,
            dimensions: self.dimensions,
            measures: self.measures,
            metrics: self.metrics,
            relationships: self.relationships,
        }
    }
}

fn map_validate_diagnostic(
    d: Diagnostic<crate::error::validate::ValidateErrorKind>,
) -> Diagnostic<ModelBuildErrorKind> {
    Diagnostic {
        kind: ModelBuildErrorKind::Validate(d.kind),
        severity: d.severity,
        location: d.location,
        notes: d.notes,
    }
}
