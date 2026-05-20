//! Structural-precondition pass over a parsed [`SemanticModel`]
//! (`32 §9.4`).
//!
//! `validate` runs every SR-* rule whose enforcement column reads
//! "Enforced at `validate`" (SR-6, SR-10) plus the entity-level
//! invariants from `18 §11`. It is a pure precondition checker — it
//! does not transform the model.
//!
//! The per-axis check modules each own one diagnostic family:
//!
//! - [`structure`] — composition shape (`EmptyModel`,
//!   `ComplexDataKindInsufficientChildren`).
//! - [`temporal`]  — temporal-shape invariants (`TemporalLeafMissingGrain`,
//!   `TemporalGrainOnComplex`, `GrainsetChildMissingGrain`).
//! - [`relationship`] — relationship rules (`RelationshipDanglingEndpoint`,
//!   `RelationshipSymmetricCardinalityIncomplete`,
//!   `RelationshipManyToManyCrossFilterDirectional`).
//! - [`semantics`] — shared-pool / interface semantics
//!   (`OrphanSharedSemantics`, `SemanticsRefMissingExpr`,
//!   `SemanticsShadowRootPool`).

mod cycle_check;
mod relationship;
mod semantics;
mod structure;
mod temporal;

use crate::data_kind::{AnyDataKindRef, GrainsetBody, JoinsetBody, UnionsetBody};
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{split_by_severity, Diagnostics};

/// Run all structural and entity-level validation rules over a parsed
/// model. Accumulates every recoverable diagnostic; the `Err` arm
/// fires when any [`semstrait_core::Severity::Error`] is present.
pub fn validate(
    model: &SemanticModel,
) -> Result<Diagnostics<ValidateErrorKind>, Diagnostics<ValidateErrorKind>> {
    let mut diags: Diagnostics<ValidateErrorKind> = Vec::new();

    structure::check_structure(model, &mut diags);
    temporal::check_temporal(model, &mut diags);
    relationship::check_all_relationships(model, &mut diags);
    semantics::check_all_semantics(model, &mut diags);
    cycle_check::check_all_cycles(model, &mut diags);

    let (errors, warnings) = split_by_severity(diags);
    if errors.is_empty() {
        Ok(warnings)
    } else {
        let mut combined = errors;
        combined.extend(warnings);
        Err(combined)
    }
}

/// Visit every data kind in `model` — top-level public kinds plus their
/// transitive nested descendants — yielding each one to `visitor` as an
/// `AnyDataKindRef` together with its immediate parent (`None` for
/// top-level entries).
///
/// Used by the per-axis structural + temporal passes that need to apply
/// the same rule at every depth.
pub(super) fn walk_complex<'a, F>(model: &'a SemanticModel, mut visitor: F)
where
    F: FnMut(Option<AnyDataKindRef<'a>>, AnyDataKindRef<'a>),
{
    for any in model.iter_all() {
        visitor(None, any);
        descend(any, &mut visitor);
    }
}

fn descend<'a, F>(parent: AnyDataKindRef<'a>, visitor: &mut F)
where
    F: FnMut(Option<AnyDataKindRef<'a>>, AnyDataKindRef<'a>),
{
    let children: Vec<AnyDataKindRef<'a>> = match parent {
        AnyDataKindRef::Grainset(g) => grainset_children(&g.body),
        AnyDataKindRef::NestedGrainset(g) => grainset_children(&g.body),
        AnyDataKindRef::Unionset(u) => unionset_children(&u.body),
        AnyDataKindRef::NestedUnionset(u) => unionset_children(&u.body),
        AnyDataKindRef::Joinset(j) => joinset_children(&j.body),
        AnyDataKindRef::NestedJoinset(j) => joinset_children(&j.body),
        _ => return,
    };
    for child in children {
        visitor(Some(parent), child);
        descend(child, visitor);
    }
}

fn grainset_children(body: &GrainsetBody) -> Vec<AnyDataKindRef<'_>> {
    body.datasets
        .iter()
        .map(AnyDataKindRef::NestedDataset)
        .chain(body.unionsets.iter().map(AnyDataKindRef::NestedUnionset))
        .chain(body.joinsets.iter().map(AnyDataKindRef::NestedJoinset))
        .collect()
}

fn unionset_children(body: &UnionsetBody) -> Vec<AnyDataKindRef<'_>> {
    body.datasets
        .iter()
        .map(AnyDataKindRef::NestedDataset)
        .chain(body.grainsets.iter().map(AnyDataKindRef::NestedGrainset))
        .chain(body.joinsets.iter().map(AnyDataKindRef::NestedJoinset))
        .collect()
}

fn joinset_children(body: &JoinsetBody) -> Vec<AnyDataKindRef<'_>> {
    body.datasets
        .iter()
        .map(AnyDataKindRef::NestedDataset)
        .chain(body.grainsets.iter().map(AnyDataKindRef::NestedGrainset))
        .chain(body.unionsets.iter().map(AnyDataKindRef::NestedUnionset))
        .collect()
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
