//! Temporal-shape rules — SR-E-6/7/8 grain placement.
//!
//! Single axis: temporal invariants. Walks the model with
//! [`super::walk_complex`], visiting each data kind and applying the
//! per-form temporal rule. Grain-on-Complex (SR-E-7) and Grainset-
//! child-missing-grain (SR-E-8) live here alongside the leaf
//! `TemporalLeafMissingGrain` rule.

use crate::data_kind::{AnyDataKindRef, DataKind, DataKindVariant, Dataset, NestedDataset};
use crate::entities::TemporalShape;
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};

pub(super) fn check_temporal(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    super::walk_complex(model, |parent, any| match any {
        AnyDataKindRef::Dataset(d) => check_dataset_temporal(d, diags),
        AnyDataKindRef::NestedDataset(nd) => check_nested_dataset_temporal(parent, nd, diags),
        AnyDataKindRef::Grainset(g) => {
            check_complex_extras_grain(g.name(), &g.body.base.extras.temporal, diags);
        }
        AnyDataKindRef::NestedGrainset(g) => {
            check_complex_extras_grain(g.name(), &g.body.base.extras.temporal, diags);
        }
        AnyDataKindRef::Unionset(u) => {
            check_complex_extras_grain(u.name(), &u.body.base.extras.temporal, diags);
        }
        AnyDataKindRef::NestedUnionset(u) => {
            check_complex_extras_grain(u.name(), &u.body.base.extras.temporal, diags);
        }
        AnyDataKindRef::Joinset(j) => {
            check_complex_extras_grain(j.name(), &j.body.base.extras.temporal, diags);
        }
        AnyDataKindRef::NestedJoinset(j) => {
            check_complex_extras_grain(j.name(), &j.body.base.extras.temporal, diags);
        }
    });
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
    parent: Option<AnyDataKindRef<'_>>,
    nd: &NestedDataset,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    let parent_variant = parent.map(|p| p.variant());
    let parent_name = parent
        .map(|p| p.name().to_string())
        .unwrap_or_default();

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
    if matches!(parent_variant, Some(DataKindVariant::Grainset)) {
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
                    grainset: parent_name,
                    child: nd.body.base.name.clone(),
                },
            ));
        }
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
