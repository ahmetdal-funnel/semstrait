//! DataKind type hierarchy — six layers per `32 §3`:
//!
//! 1. Common-fields struct — [`base::DataKindBase`].
//! 2. Per-variant body — [`dataset::DatasetBody`] / [`grainset::GrainsetBody`] / etc.
//! 3. Concrete types — `Public` (e.g. [`dataset::Dataset`]) and
//!    `Nested` (e.g. [`dataset::NestedDataset`]) forms.
//! 4. Sealed trait hierarchy — [`DataKind`] base trait plus the
//!    structural axis ([`SimpleDataKind`] / [`ComplexDataKind`]) and
//!    behavioral axis ([`PublicDataKind`] / [`NestedDataKind`]).
//! 5. Per-concrete trait impls.
//! 6. View enums — `*Ref<'a>` for heterogeneous iteration.

pub mod base;
pub mod dataset;
pub mod grainset;
pub mod joinset;
pub mod storage;
pub mod unionset;

pub use base::{ComplexExtras, DataKindBase, ExtrasFlavor, LeafExtras};
pub use dataset::{Dataset, DatasetBody, NestedDataset};
pub use grainset::{Grainset, GrainsetBody, NestedGrainset};
pub use joinset::{Joinset, JoinsetBody, NestedJoinset};
pub use storage::{CatalogRef, PartitionDef, StorageConfig, StorageFormat};
pub use unionset::{NestedUnionset, UnionMode, Unionset, UnionsetBody};

use crate::entities::ai::AiContext;
use crate::entities::semantic_interface::SemanticInterface;
use serde::{Deserialize, Serialize};

mod sealed {
    pub trait Sealed {}
}

/// Variant tag — one of the four data-kind families. Carried on the
/// trait base accessor so generic code can dispatch without inspecting
/// concrete types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DataKindVariant {
    Dataset,
    Grainset,
    Unionset,
    Joinset,
}

impl DataKindVariant {
    /// Order used by [`crate::SemanticModel::iter_all`] for the
    /// `(variant-tag, name)` sort key per `32 §7`.
    pub fn ordering_key(self) -> u8 {
        match self {
            Self::Dataset => 0,
            Self::Grainset => 1,
            Self::Unionset => 2,
            Self::Joinset => 3,
        }
    }
}

/// Form tag — Public (top-level) vs Nested (structural shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DataKindForm {
    Public,
    Nested,
}

// ─── Base trait ─────────────────────────────────────────────────────

pub trait DataKind: sealed::Sealed {
    fn name(&self) -> &str;
    fn variant(&self) -> DataKindVariant;
    fn form(&self) -> DataKindForm;
}

// ─── Structural axis ────────────────────────────────────────────────

pub trait SimpleDataKind: DataKind {
    fn extras(&self) -> &LeafExtras;
}

pub trait ComplexDataKind: DataKind {
    fn extras(&self) -> &ComplexExtras;
    fn allowed_child_variants(&self) -> &'static [DataKindVariant];
    fn child_count(&self) -> usize;
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_>;
}

// ─── Behavioral axis ────────────────────────────────────────────────

pub trait PublicDataKind: DataKind {
    fn description(&self) -> Option<&str>;
    fn ai_context(&self) -> Option<&AiContext>;
    fn semantic_interface(&self) -> &SemanticInterface;
}

/// Pure marker — its contribution is the trait bound itself.
pub trait NestedDataKind: DataKind {}

// ─── Per-concrete impls ─────────────────────────────────────────────
// Macros keep the boilerplate manageable across 8 concrete types.

macro_rules! impl_data_kind {
    (
        ty: $ty:ty,
        body: $body:ty,
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
    body: DatasetBody,
    variant: DataKindVariant::Dataset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedDataset,
    body: DatasetBody,
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

// — Grainset / NestedGrainset — complex —
impl_data_kind!(
    ty: Grainset,
    body: GrainsetBody,
    variant: DataKindVariant::Grainset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedGrainset,
    body: GrainsetBody,
    variant: DataKindVariant::Grainset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Grainset);
impl_nested_data_kind!(NestedGrainset);

const GRAINSET_ALLOWED_CHILDREN: &[DataKindVariant] = &[
    DataKindVariant::Dataset,
    DataKindVariant::Unionset,
    DataKindVariant::Joinset,
];

impl ComplexDataKind for Grainset {
    fn extras(&self) -> &ComplexExtras {
        &self.body.base.extras
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        GRAINSET_ALLOWED_CHILDREN
    }
    fn child_count(&self) -> usize {
        grainset_child_count(&self.body)
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        Box::new(grainset_children_ref(&self.body))
    }
}

impl ComplexDataKind for NestedGrainset {
    fn extras(&self) -> &ComplexExtras {
        &self.body.base.extras
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        GRAINSET_ALLOWED_CHILDREN
    }
    fn child_count(&self) -> usize {
        grainset_child_count(&self.body)
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        Box::new(grainset_children_ref(&self.body))
    }
}

fn grainset_child_count(body: &GrainsetBody) -> usize {
    body.datasets.len() + body.unionsets.len() + body.joinsets.len()
}

fn grainset_children_ref(body: &GrainsetBody) -> impl Iterator<Item = NestedDataKindRef<'_>> {
    body.datasets
        .iter()
        .map(NestedDataKindRef::Dataset)
        .chain(body.unionsets.iter().map(NestedDataKindRef::Unionset))
        .chain(body.joinsets.iter().map(NestedDataKindRef::Joinset))
}

// — Unionset / NestedUnionset — complex —
impl_data_kind!(
    ty: Unionset,
    body: UnionsetBody,
    variant: DataKindVariant::Unionset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedUnionset,
    body: UnionsetBody,
    variant: DataKindVariant::Unionset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Unionset);
impl_nested_data_kind!(NestedUnionset);

const UNIONSET_ALLOWED_CHILDREN: &[DataKindVariant] = &[
    DataKindVariant::Dataset,
    DataKindVariant::Grainset,
    DataKindVariant::Joinset,
];

impl ComplexDataKind for Unionset {
    fn extras(&self) -> &ComplexExtras {
        &self.body.base.extras
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        UNIONSET_ALLOWED_CHILDREN
    }
    fn child_count(&self) -> usize {
        unionset_child_count(&self.body)
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        Box::new(unionset_children_ref(&self.body))
    }
}

impl ComplexDataKind for NestedUnionset {
    fn extras(&self) -> &ComplexExtras {
        &self.body.base.extras
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        UNIONSET_ALLOWED_CHILDREN
    }
    fn child_count(&self) -> usize {
        unionset_child_count(&self.body)
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        Box::new(unionset_children_ref(&self.body))
    }
}

fn unionset_child_count(body: &UnionsetBody) -> usize {
    body.datasets.len() + body.grainsets.len() + body.joinsets.len()
}

fn unionset_children_ref(body: &UnionsetBody) -> impl Iterator<Item = NestedDataKindRef<'_>> {
    body.datasets
        .iter()
        .map(NestedDataKindRef::Dataset)
        .chain(body.grainsets.iter().map(NestedDataKindRef::Grainset))
        .chain(body.joinsets.iter().map(NestedDataKindRef::Joinset))
}

// — Joinset / NestedJoinset — complex —
impl_data_kind!(
    ty: Joinset,
    body: JoinsetBody,
    variant: DataKindVariant::Joinset,
    form: DataKindForm::Public
);
impl_data_kind!(
    ty: NestedJoinset,
    body: JoinsetBody,
    variant: DataKindVariant::Joinset,
    form: DataKindForm::Nested
);
impl_public_data_kind!(Joinset);
impl_nested_data_kind!(NestedJoinset);

const JOINSET_ALLOWED_CHILDREN: &[DataKindVariant] = &[
    DataKindVariant::Dataset,
    DataKindVariant::Grainset,
    DataKindVariant::Unionset,
];

impl ComplexDataKind for Joinset {
    fn extras(&self) -> &ComplexExtras {
        &self.body.base.extras
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        JOINSET_ALLOWED_CHILDREN
    }
    fn child_count(&self) -> usize {
        joinset_child_count(&self.body)
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        Box::new(joinset_children_ref(&self.body))
    }
}

impl ComplexDataKind for NestedJoinset {
    fn extras(&self) -> &ComplexExtras {
        &self.body.base.extras
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        JOINSET_ALLOWED_CHILDREN
    }
    fn child_count(&self) -> usize {
        joinset_child_count(&self.body)
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        Box::new(joinset_children_ref(&self.body))
    }
}

fn joinset_child_count(body: &JoinsetBody) -> usize {
    body.datasets.len() + body.grainsets.len() + body.unionsets.len()
}

fn joinset_children_ref(body: &JoinsetBody) -> impl Iterator<Item = NestedDataKindRef<'_>> {
    body.datasets
        .iter()
        .map(NestedDataKindRef::Dataset)
        .chain(body.grainsets.iter().map(NestedDataKindRef::Grainset))
        .chain(body.unionsets.iter().map(NestedDataKindRef::Unionset))
}

// ─── View enums (32 §3.6) ───────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum AnyDataKindRef<'a> {
    Dataset(&'a Dataset),
    NestedDataset(&'a NestedDataset),
    Grainset(&'a Grainset),
    NestedGrainset(&'a NestedGrainset),
    Unionset(&'a Unionset),
    NestedUnionset(&'a NestedUnionset),
    Joinset(&'a Joinset),
    NestedJoinset(&'a NestedJoinset),
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum PublicDataKindRef<'a> {
    Dataset(&'a Dataset),
    Grainset(&'a Grainset),
    Unionset(&'a Unionset),
    Joinset(&'a Joinset),
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum NestedDataKindRef<'a> {
    Dataset(&'a NestedDataset),
    Grainset(&'a NestedGrainset),
    Unionset(&'a NestedUnionset),
    Joinset(&'a NestedJoinset),
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum SimpleDataKindRef<'a> {
    Public(&'a Dataset),
    Nested(&'a NestedDataset),
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ComplexDataKindRef<'a> {
    Grainset(&'a Grainset),
    NestedGrainset(&'a NestedGrainset),
    Unionset(&'a Unionset),
    NestedUnionset(&'a NestedUnionset),
    Joinset(&'a Joinset),
    NestedJoinset(&'a NestedJoinset),
}

// ── View enum trait impls ──

impl<'a> sealed::Sealed for AnyDataKindRef<'a> {}
impl<'a> DataKind for AnyDataKindRef<'a> {
    fn name(&self) -> &str {
        match self {
            AnyDataKindRef::Dataset(v) => v.name(),
            AnyDataKindRef::NestedDataset(v) => v.name(),
            AnyDataKindRef::Grainset(v) => v.name(),
            AnyDataKindRef::NestedGrainset(v) => v.name(),
            AnyDataKindRef::Unionset(v) => v.name(),
            AnyDataKindRef::NestedUnionset(v) => v.name(),
            AnyDataKindRef::Joinset(v) => v.name(),
            AnyDataKindRef::NestedJoinset(v) => v.name(),
        }
    }
    fn variant(&self) -> DataKindVariant {
        match self {
            AnyDataKindRef::Dataset(_) | AnyDataKindRef::NestedDataset(_) => {
                DataKindVariant::Dataset
            }
            AnyDataKindRef::Grainset(_) | AnyDataKindRef::NestedGrainset(_) => {
                DataKindVariant::Grainset
            }
            AnyDataKindRef::Unionset(_) | AnyDataKindRef::NestedUnionset(_) => {
                DataKindVariant::Unionset
            }
            AnyDataKindRef::Joinset(_) | AnyDataKindRef::NestedJoinset(_) => {
                DataKindVariant::Joinset
            }
        }
    }
    fn form(&self) -> DataKindForm {
        match self {
            AnyDataKindRef::Dataset(_)
            | AnyDataKindRef::Grainset(_)
            | AnyDataKindRef::Unionset(_)
            | AnyDataKindRef::Joinset(_) => DataKindForm::Public,
            AnyDataKindRef::NestedDataset(_)
            | AnyDataKindRef::NestedGrainset(_)
            | AnyDataKindRef::NestedUnionset(_)
            | AnyDataKindRef::NestedJoinset(_) => DataKindForm::Nested,
        }
    }
}

impl<'a> sealed::Sealed for PublicDataKindRef<'a> {}
impl<'a> DataKind for PublicDataKindRef<'a> {
    fn name(&self) -> &str {
        match self {
            Self::Dataset(v) => v.name(),
            Self::Grainset(v) => v.name(),
            Self::Unionset(v) => v.name(),
            Self::Joinset(v) => v.name(),
        }
    }
    fn variant(&self) -> DataKindVariant {
        match self {
            Self::Dataset(_) => DataKindVariant::Dataset,
            Self::Grainset(_) => DataKindVariant::Grainset,
            Self::Unionset(_) => DataKindVariant::Unionset,
            Self::Joinset(_) => DataKindVariant::Joinset,
        }
    }
    fn form(&self) -> DataKindForm {
        DataKindForm::Public
    }
}
impl<'a> PublicDataKind for PublicDataKindRef<'a> {
    fn description(&self) -> Option<&str> {
        match self {
            Self::Dataset(v) => v.description(),
            Self::Grainset(v) => v.description(),
            Self::Unionset(v) => v.description(),
            Self::Joinset(v) => v.description(),
        }
    }
    fn ai_context(&self) -> Option<&AiContext> {
        match self {
            Self::Dataset(v) => v.ai_context(),
            Self::Grainset(v) => v.ai_context(),
            Self::Unionset(v) => v.ai_context(),
            Self::Joinset(v) => v.ai_context(),
        }
    }
    fn semantic_interface(&self) -> &SemanticInterface {
        match self {
            Self::Dataset(v) => v.semantic_interface(),
            Self::Grainset(v) => v.semantic_interface(),
            Self::Unionset(v) => v.semantic_interface(),
            Self::Joinset(v) => v.semantic_interface(),
        }
    }
}

impl<'a> sealed::Sealed for NestedDataKindRef<'a> {}
impl<'a> DataKind for NestedDataKindRef<'a> {
    fn name(&self) -> &str {
        match self {
            Self::Dataset(v) => v.name(),
            Self::Grainset(v) => v.name(),
            Self::Unionset(v) => v.name(),
            Self::Joinset(v) => v.name(),
        }
    }
    fn variant(&self) -> DataKindVariant {
        match self {
            Self::Dataset(_) => DataKindVariant::Dataset,
            Self::Grainset(_) => DataKindVariant::Grainset,
            Self::Unionset(_) => DataKindVariant::Unionset,
            Self::Joinset(_) => DataKindVariant::Joinset,
        }
    }
    fn form(&self) -> DataKindForm {
        DataKindForm::Nested
    }
}
impl<'a> NestedDataKind for NestedDataKindRef<'a> {}

impl<'a> sealed::Sealed for SimpleDataKindRef<'a> {}
impl<'a> DataKind for SimpleDataKindRef<'a> {
    fn name(&self) -> &str {
        match self {
            Self::Public(v) => v.name(),
            Self::Nested(v) => v.name(),
        }
    }
    fn variant(&self) -> DataKindVariant {
        DataKindVariant::Dataset
    }
    fn form(&self) -> DataKindForm {
        match self {
            Self::Public(_) => DataKindForm::Public,
            Self::Nested(_) => DataKindForm::Nested,
        }
    }
}
impl<'a> SimpleDataKind for SimpleDataKindRef<'a> {
    fn extras(&self) -> &LeafExtras {
        match self {
            Self::Public(v) => &v.body.base.extras,
            Self::Nested(v) => &v.body.base.extras,
        }
    }
}

impl<'a> sealed::Sealed for ComplexDataKindRef<'a> {}
impl<'a> DataKind for ComplexDataKindRef<'a> {
    fn name(&self) -> &str {
        match self {
            Self::Grainset(v) => v.name(),
            Self::NestedGrainset(v) => v.name(),
            Self::Unionset(v) => v.name(),
            Self::NestedUnionset(v) => v.name(),
            Self::Joinset(v) => v.name(),
            Self::NestedJoinset(v) => v.name(),
        }
    }
    fn variant(&self) -> DataKindVariant {
        match self {
            Self::Grainset(_) | Self::NestedGrainset(_) => DataKindVariant::Grainset,
            Self::Unionset(_) | Self::NestedUnionset(_) => DataKindVariant::Unionset,
            Self::Joinset(_) | Self::NestedJoinset(_) => DataKindVariant::Joinset,
        }
    }
    fn form(&self) -> DataKindForm {
        match self {
            Self::Grainset(_) | Self::Unionset(_) | Self::Joinset(_) => DataKindForm::Public,
            Self::NestedGrainset(_) | Self::NestedUnionset(_) | Self::NestedJoinset(_) => {
                DataKindForm::Nested
            }
        }
    }
}
impl<'a> ComplexDataKind for ComplexDataKindRef<'a> {
    fn extras(&self) -> &ComplexExtras {
        match self {
            Self::Grainset(v) => ComplexDataKind::extras(*v),
            Self::NestedGrainset(v) => ComplexDataKind::extras(*v),
            Self::Unionset(v) => ComplexDataKind::extras(*v),
            Self::NestedUnionset(v) => ComplexDataKind::extras(*v),
            Self::Joinset(v) => ComplexDataKind::extras(*v),
            Self::NestedJoinset(v) => ComplexDataKind::extras(*v),
        }
    }
    fn allowed_child_variants(&self) -> &'static [DataKindVariant] {
        match self {
            Self::Grainset(v) => v.allowed_child_variants(),
            Self::NestedGrainset(v) => v.allowed_child_variants(),
            Self::Unionset(v) => v.allowed_child_variants(),
            Self::NestedUnionset(v) => v.allowed_child_variants(),
            Self::Joinset(v) => v.allowed_child_variants(),
            Self::NestedJoinset(v) => v.allowed_child_variants(),
        }
    }
    fn child_count(&self) -> usize {
        match self {
            Self::Grainset(v) => v.child_count(),
            Self::NestedGrainset(v) => v.child_count(),
            Self::Unionset(v) => v.child_count(),
            Self::NestedUnionset(v) => v.child_count(),
            Self::Joinset(v) => v.child_count(),
            Self::NestedJoinset(v) => v.child_count(),
        }
    }
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_> {
        match self {
            Self::Grainset(v) => v.children_ref(),
            Self::NestedGrainset(v) => v.children_ref(),
            Self::Unionset(v) => v.children_ref(),
            Self::NestedUnionset(v) => v.children_ref(),
            Self::Joinset(v) => v.children_ref(),
            Self::NestedJoinset(v) => v.children_ref(),
        }
    }
}
