//! YAML-facing `semantic_model:` root.
//!
//! Authors write per-variant data kinds and shared-pool entries as
//! YAML **arrays** (`32 §1`). The canonical in-memory model uses
//! `BTreeMap<String, _>` per `32 §2`, so this intermediate form lives
//! between the YAML decoder and the canonical types: it carries
//! `Vec<_>` fields and converts to the canonical form via
//! [`YamlRoot::lower`], emitting duplicate-name diagnostics in the
//! process.

use crate::data_kind::{Dataset, Grainset, Joinset, Unionset};
use crate::entities::{AiContext, Dimension, Measure, Metric, Relationship};
use crate::error::parse::ParseErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics, Location};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Outer wrapper — the YAML must have exactly one `semantic_model:`
/// root key (SR-1). `deny_unknown_fields` rejects every other top-
/// level key with [`ParseErrorKind::UnknownTopLevelBlock`] surfaced by
/// the caller.
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
    /// Convert to the canonical [`SemanticModel`]. Duplicate names
    /// across the four data-kind plurals raise SR-3
    /// (`DuplicateDataKindName`); duplicates inside a shared pool
    /// raise the pool's specific kind.
    pub(crate) fn lower(self, source: &str) -> (SemanticModel, Diagnostics<ParseErrorKind>) {
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

        let mut diags: Diagnostics<ParseErrorKind> = Vec::new();

        // ── Cross-variant (SR-3) duplicate detection ──
        // Tracks every name + the variant it lives under so a
        // grainset/dataset name collision raises one diagnostic per
        // collision, not per duplicate-on-the-same-side.
        let mut data_kind_index: BTreeMap<String, Vec<(&'static str, Location)>> = BTreeMap::new();

        let datasets =
            collect_data_kinds(datasets, "datasets", source, &mut data_kind_index, &mut diags);
        let grainsets = collect_data_kinds(
            grainsets,
            "grainsets",
            source,
            &mut data_kind_index,
            &mut diags,
        );
        let unionsets = collect_data_kinds(
            unionsets,
            "unionsets",
            source,
            &mut data_kind_index,
            &mut diags,
        );
        let joinsets = collect_data_kinds(
            joinsets,
            "joinsets",
            source,
            &mut data_kind_index,
            &mut diags,
        );

        // Emit one diagnostic per cross-variant collision.
        for (name, occurrences) in data_kind_index {
            if occurrences.len() > 1 {
                let occurrence_locations: Vec<Location> =
                    occurrences.iter().map(|(_, loc)| loc.clone()).collect();
                diags.push(Diagnostic::new(ParseErrorKind::DuplicateDataKindName {
                    name,
                    occurrences: occurrence_locations,
                }));
            }
        }

        let dimensions = collect_pool(dimensions, |d| d.name.as_str().to_string(), "dimensions", source, &mut diags);
        let measures = collect_pool(measures, |m| m.name.as_str().to_string(), "measures", source, &mut diags);
        let metrics = collect_pool(metrics, |m| m.name.as_str().to_string(), "metrics", source, &mut diags);

        let model = SemanticModel {
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
        };

        (model, diags)
    }
}

trait HasName {
    fn name(&self) -> &str;
}

impl HasName for Dataset {
    fn name(&self) -> &str {
        &self.body.base.name
    }
}
impl HasName for Grainset {
    fn name(&self) -> &str {
        &self.body.base.name
    }
}
impl HasName for Unionset {
    fn name(&self) -> &str {
        &self.body.base.name
    }
}
impl HasName for Joinset {
    fn name(&self) -> &str {
        &self.body.base.name
    }
}

fn collect_data_kinds<T: HasName>(
    items: Vec<T>,
    plural_tag: &'static str,
    source: &str,
    cross_index: &mut BTreeMap<String, Vec<(&'static str, Location)>>,
    _diags: &mut Diagnostics<ParseErrorKind>,
) -> BTreeMap<String, T> {
    let mut out = BTreeMap::new();
    for (i, item) in items.into_iter().enumerate() {
        let name = item.name().to_string();
        let loc = Location::new(source.to_string()).with_path(format!("/{}/{}", plural_tag, i));
        cross_index.entry(name.clone()).or_default().push((plural_tag, loc));
        out.insert(name, item);
    }
    out
}

fn collect_pool<T, F>(
    items: Vec<T>,
    name_of: F,
    carrier: &'static str,
    source: &str,
    diags: &mut Diagnostics<ParseErrorKind>,
) -> BTreeMap<String, T>
where
    F: Fn(&T) -> String,
{
    let mut out: BTreeMap<String, T> = BTreeMap::new();
    let mut occurrences: BTreeMap<String, Vec<Location>> = BTreeMap::new();
    for (i, item) in items.into_iter().enumerate() {
        let name = name_of(&item);
        let loc = Location::new(source.to_string()).with_path(format!("/{}/{}", carrier, i));
        occurrences.entry(name.clone()).or_default().push(loc);
        out.insert(name, item);
    }
    for (name, locs) in occurrences {
        if locs.len() > 1 {
            diags.push(Diagnostic::new(
                ParseErrorKind::DuplicateSharedSemanticsName {
                    carrier: carrier.to_string(),
                    name,
                    occurrences: locs,
                },
            ));
        }
    }
    out
}
