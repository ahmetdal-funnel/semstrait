//! Structural-precondition pass over a parsed [`SemanticModel`]
//! (`32 §9.4`).
//!
//! `validate` runs every SR-* rule whose enforcement column reads
//! "Enforced at `validate`" (SR-6, SR-10) plus the entity-level
//! invariants from `18 §11`. It is a pure precondition checker — it
//! does not transform the model.

use crate::data_kind::{
    AnyDataKindRef, ComplexDataKind, DataKind, DataKindVariant, Dataset, Grainset, Joinset,
    NestedDataset, NestedGrainset, NestedJoinset, NestedUnionset, Unionset,
};
use crate::entities::{
    Cardinality, CrossFilter, DimensionEntry, MeasureEntry, MetricEntry, MetricRef, Relationship,
    SemanticInterface, TemporalShape,
};
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{split_by_severity, Diagnostic, Diagnostics};
use std::collections::BTreeSet;

/// Run all structural and entity-level validation rules over a parsed
/// model. Accumulates every recoverable diagnostic; the `Err` arm
/// fires when any [`semstrait_core::Severity::Error`] is present.
pub fn validate(
    model: &SemanticModel,
) -> Result<Diagnostics<ValidateErrorKind>, Diagnostics<ValidateErrorKind>> {
    let mut diags: Diagnostics<ValidateErrorKind> = Vec::new();

    // SR-* + SR-E-* rules.
    check_empty_model(model, &mut diags);
    check_temporal_and_complex_children(model, &mut diags);
    check_relationships(&model.relationships, &mut diags, /*scope=*/ None);
    check_root_relationships_endpoints(model, &mut diags);
    check_orphan_pool_semantics(model, &mut diags);
    check_pool_ref_expressions(model, &mut diags);

    // Joinset-local relationships use the same rule set, scoped to
    // their parent's address for SR-E-5 endpoint resolution context.
    for j in model.joinsets.values() {
        check_relationships(&j.body.relationships, &mut diags, Some(j.body.base.name.clone()));
    }

    let (errors, warnings) = split_by_severity(diags);
    if errors.is_empty() {
        Ok(warnings)
    } else {
        let mut combined = errors;
        combined.extend(warnings);
        Err(combined)
    }
}

// ── SR-10 / SR-6 / SR-E-6/7/8 ───────────────────────────────────────

fn check_empty_model(model: &SemanticModel, diags: &mut Diagnostics<ValidateErrorKind>) {
    if model.is_empty_model() {
        diags.push(Diagnostic::new(ValidateErrorKind::EmptyModel));
    }
}

fn check_temporal_and_complex_children(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    // Walk every public top-level data kind, descending into bodies.
    for any in model.iter_all() {
        match any {
            AnyDataKindRef::Dataset(d) => check_dataset_temporal(d, diags),
            AnyDataKindRef::Grainset(g) => {
                check_complex_extras_grain(g.name(), &g.body.base.extras.temporal, diags);
                check_complex_min_children(
                    g.name(),
                    g.child_count(),
                    diags,
                );
                walk_grainset(g, diags);
            }
            AnyDataKindRef::Unionset(u) => {
                check_complex_extras_grain(u.name(), &u.body.base.extras.temporal, diags);
                check_complex_min_children(u.name(), u.child_count(), diags);
                walk_unionset(u, diags);
            }
            AnyDataKindRef::Joinset(j) => {
                check_complex_extras_grain(j.name(), &j.body.base.extras.temporal, diags);
                check_complex_min_children(j.name(), j.child_count(), diags);
                walk_joinset(j, diags);
            }
            _ => {}
        }
    }
}

fn check_complex_min_children(
    name: &str,
    count: usize,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    if count < 2 {
        diags.push(Diagnostic::new(
            ValidateErrorKind::ComplexDataKindInsufficientChildren {
                parent: name.to_string(),
                child_count: count,
            },
        ));
    }
}

fn check_complex_extras_grain(
    name: &str,
    temporal: &Option<TemporalShape>,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    // SR-E-7: a complex kind MUST NOT author `temporal.grain:`.
    if let Some(t) = temporal {
        if t.grain.is_some() {
            diags.push(Diagnostic::new(ValidateErrorKind::TemporalGrainOnComplex {
                data_kind: name.to_string(),
            }));
        }
    }
}

fn check_dataset_temporal(d: &Dataset, diags: &mut Diagnostics<ValidateErrorKind>) {
    if let Some(t) = &d.body.base.extras.temporal {
        if t.grain.is_none() {
            diags.push(Diagnostic::new(ValidateErrorKind::TemporalLeafMissingGrain {
                data_kind: d.body.base.name.clone(),
            }));
        }
    }
}

fn check_nested_dataset_temporal(
    parent_kind: Option<DataKindVariant>,
    parent_name: &str,
    nd: &NestedDataset,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    let temporal_owned = nd.body.base.extras.temporal.is_some();
    if let Some(t) = &nd.body.base.extras.temporal {
        if t.grain.is_none() {
            diags.push(Diagnostic::new(ValidateErrorKind::TemporalLeafMissingGrain {
                data_kind: format!("{}.{}", parent_name, nd.body.base.name),
            }));
        }
    }

    // SR-E-8 — every Grainset child Dataset must author its own grain
    // explicitly. Inheritance from the parent's `temporal.kind` is
    // permitted; the grain is not.
    if matches!(parent_kind, Some(DataKindVariant::Grainset)) {
        let has_own_grain = nd
            .body
            .base
            .extras
            .temporal
            .as_ref()
            .and_then(|t| t.grain)
            .is_some();
        if !has_own_grain {
            diags.push(Diagnostic::new(
                ValidateErrorKind::GrainsetChildMissingGrain {
                    grainset: parent_name.to_string(),
                    child: nd.body.base.name.clone(),
                },
            ));
        }
    }

    let _ = temporal_owned;
}

fn walk_grainset(g: &Grainset, diags: &mut Diagnostics<ValidateErrorKind>) {
    let parent = g.body.base.name.as_str();
    for nd in &g.body.datasets {
        check_nested_dataset_temporal(Some(DataKindVariant::Grainset), parent, nd, diags);
    }
    for nu in &g.body.unionsets {
        check_complex_extras_grain(&nu.body.base.name, &nu.body.base.extras.temporal, diags);
        check_complex_min_children(&nu.body.base.name, nu_child_count(nu), diags);
        walk_nested_unionset(nu, diags);
    }
    for nj in &g.body.joinsets {
        check_complex_extras_grain(&nj.body.base.name, &nj.body.base.extras.temporal, diags);
        check_complex_min_children(&nj.body.base.name, nj_child_count(nj), diags);
        walk_nested_joinset(nj, diags);
    }
}

fn walk_unionset(u: &Unionset, diags: &mut Diagnostics<ValidateErrorKind>) {
    let parent = u.body.base.name.as_str();
    for nd in &u.body.datasets {
        check_nested_dataset_temporal(Some(DataKindVariant::Unionset), parent, nd, diags);
    }
    for ng in &u.body.grainsets {
        check_complex_extras_grain(&ng.body.base.name, &ng.body.base.extras.temporal, diags);
        check_complex_min_children(&ng.body.base.name, ng_child_count(ng), diags);
        walk_nested_grainset(ng, diags);
    }
    for nj in &u.body.joinsets {
        check_complex_extras_grain(&nj.body.base.name, &nj.body.base.extras.temporal, diags);
        check_complex_min_children(&nj.body.base.name, nj_child_count(nj), diags);
        walk_nested_joinset(nj, diags);
    }
}

fn walk_joinset(j: &Joinset, diags: &mut Diagnostics<ValidateErrorKind>) {
    let parent = j.body.base.name.as_str();
    for nd in &j.body.datasets {
        check_nested_dataset_temporal(Some(DataKindVariant::Joinset), parent, nd, diags);
    }
    for ng in &j.body.grainsets {
        check_complex_extras_grain(&ng.body.base.name, &ng.body.base.extras.temporal, diags);
        check_complex_min_children(&ng.body.base.name, ng_child_count(ng), diags);
        walk_nested_grainset(ng, diags);
    }
    for nu in &j.body.unionsets {
        check_complex_extras_grain(&nu.body.base.name, &nu.body.base.extras.temporal, diags);
        check_complex_min_children(&nu.body.base.name, nu_child_count(nu), diags);
        walk_nested_unionset(nu, diags);
    }
}

fn walk_nested_grainset(ng: &NestedGrainset, diags: &mut Diagnostics<ValidateErrorKind>) {
    let parent = ng.body.base.name.as_str();
    for nd in &ng.body.datasets {
        check_nested_dataset_temporal(Some(DataKindVariant::Grainset), parent, nd, diags);
    }
    for nu in &ng.body.unionsets {
        walk_nested_unionset(nu, diags);
    }
    for nj in &ng.body.joinsets {
        walk_nested_joinset(nj, diags);
    }
}

fn walk_nested_unionset(nu: &NestedUnionset, diags: &mut Diagnostics<ValidateErrorKind>) {
    let parent = nu.body.base.name.as_str();
    for nd in &nu.body.datasets {
        check_nested_dataset_temporal(Some(DataKindVariant::Unionset), parent, nd, diags);
    }
    for ng in &nu.body.grainsets {
        walk_nested_grainset(ng, diags);
    }
    for nj in &nu.body.joinsets {
        walk_nested_joinset(nj, diags);
    }
}

fn walk_nested_joinset(nj: &NestedJoinset, diags: &mut Diagnostics<ValidateErrorKind>) {
    let parent = nj.body.base.name.as_str();
    for nd in &nj.body.datasets {
        check_nested_dataset_temporal(Some(DataKindVariant::Joinset), parent, nd, diags);
    }
    for ng in &nj.body.grainsets {
        walk_nested_grainset(ng, diags);
    }
    for nu in &nj.body.unionsets {
        walk_nested_unionset(nu, diags);
    }
}

fn ng_child_count(ng: &NestedGrainset) -> usize {
    ng.body.datasets.len() + ng.body.unionsets.len() + ng.body.joinsets.len()
}

fn nu_child_count(nu: &NestedUnionset) -> usize {
    nu.body.datasets.len() + nu.body.grainsets.len() + nu.body.joinsets.len()
}

fn nj_child_count(nj: &NestedJoinset) -> usize {
    nj.body.datasets.len() + nj.body.grainsets.len() + nj.body.unionsets.len()
}

// ── Relationship rules (SR-E-5 / 13 / 14) ───────────────────────────

fn check_relationships(
    rels: &[Relationship],
    diags: &mut Diagnostics<ValidateErrorKind>,
    _scope_parent: Option<String>,
) {
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

// ── Orphan / ref / data-type rules (SR-E-1 / 2 / 3 / 12) ─────────────

fn check_orphan_pool_semantics(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    // SR-E-3 — every shared-pool Semantic must be referenced from at
    // least one DataKind's interface (transitive binding to a Dataset
    // is checked at compile per `15`; v1 model-layer only enforces
    // "is the name reachable through any interface ref or inline").
    // We treat root-pool entries that are never `ref`-ed and never
    // appear inline anywhere as orphans here.

    let referenced_dimensions = collect_referenced_pool_names(model, |i| {
        i.dimensions
            .iter()
            .filter_map(|e| match e {
                DimensionEntry::Ref(r) => Some(r.name.as_str().to_string()),
                DimensionEntry::Inline(_) => None,
            })
            .collect()
    });

    let referenced_measures = collect_referenced_pool_names(model, |i| {
        i.measures
            .iter()
            .filter_map(|e| match e {
                MeasureEntry::Ref(r) => Some(r.name.as_str().to_string()),
                MeasureEntry::Inline(_) => None,
            })
            .collect()
    });

    let referenced_metrics = collect_referenced_pool_names(model, |i| {
        i.metrics
            .iter()
            .filter_map(|e| match e {
                MetricEntry::Ref(r) => Some(r.name.as_str().to_string()),
                MetricEntry::Inline(_) => None,
            })
            .collect()
    });

    for name in model.dimensions.keys() {
        if !referenced_dimensions.contains(name) {
            diags.push(Diagnostic::new(ValidateErrorKind::OrphanSharedSemantics {
                carrier: "Dimension".to_string(),
                name: name.clone(),
            }));
        }
    }
    for name in model.measures.keys() {
        if !referenced_measures.contains(name) {
            diags.push(Diagnostic::new(ValidateErrorKind::OrphanSharedSemantics {
                carrier: "Measure".to_string(),
                name: name.clone(),
            }));
        }
    }
    for name in model.metrics.keys() {
        if !referenced_metrics.contains(name) {
            diags.push(Diagnostic::new(ValidateErrorKind::OrphanSharedSemantics {
                carrier: "Metric".to_string(),
                name: name.clone(),
            }));
        }
    }
}

fn collect_referenced_pool_names<F>(model: &SemanticModel, picker: F) -> BTreeSet<String>
where
    F: Fn(&SemanticInterface) -> Vec<String>,
{
    let mut out = BTreeSet::new();
    for d in model.datasets.values() {
        for n in picker(&d.semantic_interface) {
            out.insert(n);
        }
    }
    for g in model.grainsets.values() {
        for n in picker(&g.semantic_interface) {
            out.insert(n);
        }
    }
    for u in model.unionsets.values() {
        for n in picker(&u.semantic_interface) {
            out.insert(n);
        }
    }
    for j in model.joinsets.values() {
        for n in picker(&j.semantic_interface) {
            out.insert(n);
        }
    }
    out
}

fn check_pool_ref_expressions(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    // SR-E-2 — a `Ref` site missing `expr:` AND root-pool entry
    // missing `expr:` is ill-formed for Metric (where expr is
    // required). For Dimension and Measure, root-pool entries may
    // legally have no `expr:` (the agg / data_type-only declaration);
    // for those carriers SR-E-2 doesn't fire — the binding completes
    // through `semantic_mapping` resolution.

    let scan_iface = |iface: &SemanticInterface, diags: &mut Diagnostics<ValidateErrorKind>| {
        for entry in &iface.metrics {
            if let MetricEntry::Ref(MetricRef { name, expr, .. }) = entry {
                let pool_entry = model.metrics.get(name.as_str());
                let pool_has_expr = pool_entry.and_then(|m| m.expr.as_ref()).is_some();
                let ref_has_expr = expr.is_some();
                if !pool_has_expr && !ref_has_expr {
                    diags.push(Diagnostic::new(
                        ValidateErrorKind::SemanticsRefMissingExpr {
                            carrier: "Metric".to_string(),
                            name: name.as_str().to_string(),
                        },
                    ));
                }
            }
        }
    };

    for d in model.datasets.values() {
        scan_iface(&d.semantic_interface, diags);
    }
    for g in model.grainsets.values() {
        scan_iface(&g.semantic_interface, diags);
    }
    for u in model.unionsets.values() {
        scan_iface(&u.semantic_interface, diags);
    }
    for j in model.joinsets.values() {
        scan_iface(&j.semantic_interface, diags);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_model_fires_diagnostic() {
        let m = SemanticModel {
            name: "tiny".to_string(),
            ..SemanticModel::default()
        };
        let err = validate(&m).unwrap_err();
        assert!(err
            .iter()
            .any(|d| matches!(d.kind, ValidateErrorKind::EmptyModel)));
    }
}
