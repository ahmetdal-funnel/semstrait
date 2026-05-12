//! Per-concrete trait impls excluding [`ComplexDataKind`]
//! (which lives in [`super::impls_complex`]).
//!
//! Hosts the three boilerplate-shrinking macros plus their 8
//! invocations across the four `Public` / four `Nested` concrete
//! types, and the two `SimpleDataKind` impls for the dataset leaf.

use super::base::LeafExtras;
use super::dataset::{Dataset, NestedDataset};
use super::grainset::{Grainset, NestedGrainset};
use super::joinset::{Joinset, NestedJoinset};
use super::sealed;
use super::traits::{DataKind, NestedDataKind, PublicDataKind, SimpleDataKind};
use super::unionset::{NestedUnionset, Unionset};
use super::variants::{DataKindForm, DataKindVariant};
use crate::entities::ai::AiContext;
use crate::entities::semantic_interface::SemanticInterface;

// The `body:` documentation arm from the original macro is dropped
// here — it was never expanded into emitted code and only served as a
// reading aid. Per-variant body identity is still discoverable through
// the concrete `$ty`'s definition site.
macro_rules! impl_data_kind {
    (
        ty: $ty:ty,
        variant: $variant:expr,
        form: $form:expr
    ) => {
        impl sealed::Sealed for $ty {}
        impl DataKind for $ty {
            fn name(&self) -> &str {
                &self.body.base.name
            }
            fn variant(&self) -> DataKindVariant {
                $variant
            }
            fn form(&self) -> DataKindForm {
                $form
            }
        }
    };
}

macro_rules! impl_public_data_kind {
    ($ty:ty) => {
        impl PublicDataKind for $ty {
            fn description(&self) -> Option<&str> {
                self.description.as_deref()
            }
            fn ai_context(&self) -> Option<&AiContext> {
                self.ai_context.as_ref()
            }
            fn semantic_interface(&self) -> &SemanticInterface {
                &self.semantic_interface
            }
        }
    };
}

macro_rules! impl_nested_data_kind {
    ($ty:ty) => {
        impl NestedDataKind for $ty {}
    };
}

// — Dataset / NestedDataset — leaf/simple —
impl_data_kind!(
    ty: Dataset,
    variant: DataKindVariant::Dataset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedDataset,
    variant: DataKindVariant::Dataset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Dataset);
impl_nested_data_kind!(NestedDataset);

impl SimpleDataKind for Dataset {
    fn extras(&self) -> &LeafExtras {
        &self.body.base.extras
    }
}

impl SimpleDataKind for NestedDataset {
    fn extras(&self) -> &LeafExtras {
        &self.body.base.extras
    }
}

// — Grainset / NestedGrainset — complex (ComplexDataKind impls in impls_complex.rs) —
impl_data_kind!(
    ty: Grainset,
    variant: DataKindVariant::Grainset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedGrainset,
    variant: DataKindVariant::Grainset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Grainset);
impl_nested_data_kind!(NestedGrainset);

// — Unionset / NestedUnionset — complex —
impl_data_kind!(
    ty: Unionset,
    variant: DataKindVariant::Unionset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedUnionset,
    variant: DataKindVariant::Unionset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Unionset);
impl_nested_data_kind!(NestedUnionset);

// — Joinset / NestedJoinset — complex —
impl_data_kind!(
    ty: Joinset,
    variant: DataKindVariant::Joinset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedJoinset,
    variant: DataKindVariant::Joinset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Joinset);
impl_nested_data_kind!(NestedJoinset);
