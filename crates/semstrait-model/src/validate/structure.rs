//! Structural-shape rules — SR-10 + EmptyModel.
//!
//! Single axis: composition shape. Walks the model with
//! [`super::walk_complex`] and emits one diagnostic per offending
//! Complex data kind plus the top-level `EmptyModel` check.

use crate::data_kind::{AnyDataKindRef, ComplexDataKind, DataKind};
use crate::error::validate::ValidateErrorKind;
use crate::model::SemanticModel;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics};

pub(super) fn check_structure(
    model: &SemanticModel,
    diags: &mut Diagnostics<ValidateErrorKind>,
) {
    check_empty_model(model, diags);
    super::walk_complex(model, |_parent, any| match any {
        AnyDataKindRef::Grainset(g) => check_complex_min_children(g.name(), g.child_count(), diags),
        AnyDataKindRef::NestedGrainset(g) => {
            check_complex_min_children(g.name(), g.child_count(), diags)
        }
        AnyDataKindRef::Unionset(u) => check_complex_min_children(u.name(), u.child_count(), diags),
        AnyDataKindRef::NestedUnionset(u) => {
            check_complex_min_children(u.name(), u.child_count(), diags)
        }
        AnyDataKindRef::Joinset(j) => check_complex_min_children(j.name(), j.child_count(), diags),
        AnyDataKindRef::NestedJoinset(j) => {
            check_complex_min_children(j.name(), j.child_count(), diags)
        }
        _ => {}
    });
}

fn check_empty_model(model: &SemanticModel, diags: &mut Diagnostics<ValidateErrorKind>) {
    if model.is_empty_model() {
        diags.push(Diagnostic::new(ValidateErrorKind::EmptyModel));
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
