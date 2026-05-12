//! Relationship rules — SR-E-5 / SR-E-13 / SR-E-14.
//!
//! Single axis: relationship cardinality, optionality, cross-filter,
//! and endpoint resolution. The orchestrator runs the per-list rules
//! over `model.relationships` and every joinset-local
//! `joinset.body.relationships`, then checks root-level endpoints.

// TODO(P3): the joinset-local relationship walk drops the
// parent-scope context (the previous `scope_parent: Option<String>`
// argument was unused). Re-introduce a scope argument when SR-E-5
// joinset-local endpoint resolution actually fires per-joinset
// diagnostics — at that point `check_root_relationships_endpoints`
// should also fan out into each joinset's locally-known names instead
// of consulting only the root data-kind maps.

use crate::entities::{Cardinality, CrossFilter, Relationship};
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};
use std::collections::BTreeSet;

pub(super) fn check_all_relationships(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    check_relationships(&model.relationships, diags);
    check_root_relationships_endpoints(model, diags);
    for j in model.joinsets.values() {
        check_relationships(&j.body.relationships, diags);
    }
}

fn check_relationships(rels: &[Relationship], diags: &mut Diagnostics<ValidateErrorKind>) {
    for r in rels {
        // SR-E-13 — symmetric cardinalities require explicit `optional` and `cross_filter`.
        if matches!(r.cardinality, Cardinality::OneToOne | Cardinality::ManyToMany) {
            if r.optional.is_none() {
                diags.push(Diagnostic::new(
                    ValidateErrorKind::RelationshipSymmetricCardinalityIncomplete {
                        relationship: r.name.clone(),
                        missing: "optional".to_string(),
                    },
                ));
            }
            if r.cross_filter.is_none() {
                diags.push(Diagnostic::new(
                    ValidateErrorKind::RelationshipSymmetricCardinalityIncomplete {
                        relationship: r.name.clone(),
                        missing: "cross_filter".to_string(),
                    },
                ));
            }
        }

        // SR-E-14 — many-to-many `cross_filter` must be `Both` or `None`.
        if matches!(r.cardinality, Cardinality::ManyToMany) {
            if let Some(cf) = r.cross_filter {
                if matches!(cf, CrossFilter::Left | CrossFilter::Right) {
                    diags.push(Diagnostic::new(
                        ValidateErrorKind::RelationshipManyToManyCrossFilterDirectional {
                            relationship: r.name.clone(),
                        },
                    ));
                }
            }
        }
    }
}

fn check_root_relationships_endpoints(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    let mut known_names: BTreeSet<&str> = BTreeSet::new();
    known_names.extend(model.datasets.keys().map(|k| k.as_str()));
    known_names.extend(model.grainsets.keys().map(|k| k.as_str()));
    known_names.extend(model.unionsets.keys().map(|k| k.as_str()));
    known_names.extend(model.joinsets.keys().map(|k| k.as_str()));

    for r in &model.relationships {
        if !known_names.contains(r.from.as_str()) {
            diags.push(Diagnostic::new(
                ValidateErrorKind::RelationshipDanglingEndpoint {
                    relationship: r.name.clone(),
                    side: "from".to_string(),
                    endpoint: r.from.as_str().to_string(),
                },
            ));
        }
        if !known_names.contains(r.to.as_str()) {
            diags.push(Diagnostic::new(
                ValidateErrorKind::RelationshipDanglingEndpoint {
                    relationship: r.name.clone(),
                    side: "to".to_string(),
                    endpoint: r.to.as_str().to_string(),
                },
            ));
        }
    }
}
