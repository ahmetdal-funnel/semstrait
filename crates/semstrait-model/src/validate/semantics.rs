//! Shared-pool / interface semantics rules — SR-E-1 / 2 / 3 plus the
//! `18 §1.5` inline-vs-root-pool shadowing warning.
//!
//! Single axis: every rule here operates over the four top-level
//! [`SemanticInterface`]s and the three root-pool maps. Inline
//! semantics live on the interfaces; the pool maps are the targets.

use crate::entities::{
    DimensionEntry, MeasureEntry, MetricEntry, MetricRef, SemanticInterface,
};
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};
use std::collections::BTreeSet;

pub(super) fn check_all_semantics(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    check_orphan_pool_semantics(model, diags);
    check_pool_ref_expressions(model, diags);
    check_inline_shadow_root_pool(model, diags);
}

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

fn check_inline_shadow_root_pool(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    // An inline declaration whose name collides with a root-pool entry
    // for the same carrier is warned per `18 §1.5`. Authors who want
    // shadowing should use `ref:` + override instead.
    let scan = |iface: &SemanticInterface, diags: &mut Diagnostics<ValidateErrorKind>| {
        for entry in &iface.dimensions {
            if let DimensionEntry::Inline(d) = entry {
                if model.dimensions.contains_key(d.name.as_str()) {
                    diags.push(Diagnostic::new(ValidateErrorKind::SemanticsShadowRootPool {
                        carrier: "Dimension".to_string(),
                        name: d.name.as_str().to_string(),
                    }));
                }
            }
        }
        for entry in &iface.measures {
            if let MeasureEntry::Inline(m) = entry {
                if model.measures.contains_key(m.name.as_str()) {
                    diags.push(Diagnostic::new(ValidateErrorKind::SemanticsShadowRootPool {
                        carrier: "Measure".to_string(),
                        name: m.name.as_str().to_string(),
                    }));
                }
            }
        }
        for entry in &iface.metrics {
            if let MetricEntry::Inline(m) = entry {
                if model.metrics.contains_key(m.name.as_str()) {
                    diags.push(Diagnostic::new(ValidateErrorKind::SemanticsShadowRootPool {
                        carrier: "Metric".to_string(),
                        name: m.name.as_str().to_string(),
                    }));
                }
            }
        }
    };

    for d in model.datasets.values() {
        scan(&d.semantic_interface, diags);
    }
    for g in model.grainsets.values() {
        scan(&g.semantic_interface, diags);
    }
    for u in model.unionsets.values() {
        scan(&u.semantic_interface, diags);
    }
    for j in model.joinsets.values() {
        scan(&j.semantic_interface, diags);
    }
}
