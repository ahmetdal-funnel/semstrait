//! Sealed trait hierarchy on two axes (`32 §3.4`).
//!
//! - Base: [`DataKind`] (universal name + tag accessors).
//! - Structural axis: [`SimpleDataKind`] (leaf extras) vs
//!   [`ComplexDataKind`] (composer extras + child enumeration).
//! - Behavioral axis: [`PublicDataKind`] (Public-form-only accessors)
//!   vs [`NestedDataKind`] (pure marker).
//!
//! Sealing happens via the parent module's `sealed::Sealed` super-
//! trait, which keeps `impl DataKind for …` confined to the
//! `data_kind` tree.

use super::base::{ComplexExtras, LeafExtras};
use super::refs::NestedDataKindRef;
use super::sealed;
use super::variants::{DataKindForm, DataKindVariant};
use crate::entities::ai::AiContext;
use crate::entities::semantic_interface::SemanticInterface;

pub trait DataKind: sealed::Sealed {
    fn name(&self) -> &str;
    fn variant(&self) -> DataKindVariant;
    fn form(&self) -> DataKindForm;
}

pub trait SimpleDataKind: DataKind {
    fn extras(&self) -> &LeafExtras;
}

pub trait ComplexDataKind: DataKind {
    fn extras(&self) -> &ComplexExtras;
    fn allowed_child_variants(&self) -> &'static [DataKindVariant];
    fn child_count(&self) -> usize;
    fn children_ref(&self) -> Box<dyn Iterator<Item = NestedDataKindRef<'_>> + '_>;
}

pub trait PublicDataKind: DataKind {
    fn description(&self) -> Option<&str>;
    fn ai_context(&self) -> Option<&AiContext>;
    fn semantic_interface(&self) -> &SemanticInterface;
}

/// Pure marker — its contribution is the trait bound itself.
pub trait NestedDataKind: DataKind {}
