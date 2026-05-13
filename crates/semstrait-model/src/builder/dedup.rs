//! Duplicate-name detection over the Vec-backed
//! [`crate::builder::model::SemanticModelStorage`] (`32 §9.7.5`, D-10).
//!
//! Two rules are enforced uniformly across single-file and cross-file
//! merges:
//!
//! - SR-3   — duplicate data-kind names across the four plural
//!   variants. Emits [`ValidateErrorKind::DuplicateDataKindName`] once
//!   per colliding name with every occurrence's [`Location`].
//! - SR-E-3 — duplicate names inside any shared Semantics pool
//!   (`dimensions:` / `measures:` / `metrics:`). Emits
//!   [`ValidateErrorKind::DuplicateSharedSemanticsName`] per carrier.

use crate::builder::model::SemanticModelStorage;
use crate::error::validate::ValidateErrorKind;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics, Location};
use std::collections::BTreeMap;

pub(super) fn collect_duplicate_data_kinds(
    storage: &SemanticModelStorage,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    let mut index: BTreeMap<String, Vec<Location>> = BTreeMap::new();
    for (loc, d) in &storage.datasets {
        index
            .entry(d.body.base.name.clone())
            .or_default()
            .push(loc.clone());
    }
    for (loc, g) in &storage.grainsets {
        index
            .entry(g.body.base.name.clone())
            .or_default()
            .push(loc.clone());
    }
    for (loc, u) in &storage.unionsets {
        index
            .entry(u.body.base.name.clone())
            .or_default()
            .push(loc.clone());
    }
    for (loc, j) in &storage.joinsets {
        index
            .entry(j.body.base.name.clone())
            .or_default()
            .push(loc.clone());
    }
    for (name, occurrences) in index {
        if occurrences.len() > 1 {
            diags.push(Diagnostic::new(ValidateErrorKind::DuplicateDataKindName {
                name,
                occurrences,
            }));
        }
    }
}

pub(super) fn collect_duplicate_shared_semantics(
    storage: &SemanticModelStorage,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    collect_pool(
        storage.dimensions.iter().map(|(l, d)| (d.name.as_str(), l)),
        "dimensions",
        diags,
    );
    collect_pool(
        storage.measures.iter().map(|(l, m)| (m.name.as_str(), l)),
        "measures",
        diags,
    );
    collect_pool(
        storage.metrics.iter().map(|(l, m)| (m.name.as_str(), l)),
        "metrics",
        diags,
    );
}

fn collect_pool<'a>(
    entries: impl Iterator<Item = (&'a str, &'a Location)>,
    carrier: &'static str,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    let mut index: BTreeMap<String, Vec<Location>> = BTreeMap::new();
    for (name, loc) in entries {
        index.entry(name.to_string()).or_default().push(loc.clone());
    }
    for (name, occurrences) in index {
        if occurrences.len() > 1 {
            diags.push(Diagnostic::new(
                ValidateErrorKind::DuplicateSharedSemanticsName {
                    carrier: carrier.to_string(),
                    name,
                    occurrences,
                },
            ));
        }
    }
}
