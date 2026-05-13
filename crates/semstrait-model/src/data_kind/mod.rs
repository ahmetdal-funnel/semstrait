//! DataKind type hierarchy — six layers per `32 §3`:
//!
//! 1. Common-fields struct — [`base::DataKindBase`].
//! 2. Per-variant body — [`dataset::DatasetBody`] /
//!    [`grainset::GrainsetBody`] / etc.
//! 3. Concrete types — `Public` (e.g. [`dataset::Dataset`]) and
//!    `Nested` (e.g. [`dataset::NestedDataset`]) forms.
//! 4. Sealed trait hierarchy ([`traits`]) — [`DataKind`] base trait
//!    plus the structural axis ([`SimpleDataKind`] /
//!    [`ComplexDataKind`]) and behavioral axis ([`PublicDataKind`] /
//!    [`NestedDataKind`]).
//! 5. Per-concrete trait impls ([`impls`] for Sealed / DataKind /
//!    Public / Nested / Simple; [`impls_complex`] for ComplexDataKind).
//! 6. View enums ([`refs`]) + their trait impls ([`refs_impls`]) for
//!    heterogeneous iteration.

pub mod base;
pub mod dataset;
pub mod grainset;
pub mod joinset;
pub mod storage;
pub mod unionset;

mod impls;
mod impls_complex;
mod refs;
mod refs_impls;
mod traits;
mod variants;

pub(crate) mod sealed {
    pub trait Sealed {}
}

pub use base::{ComplexExtras, DataKindBase, ExtrasFlavor, LeafExtras};
pub use dataset::{Dataset, DatasetBody, NestedDataset};
pub use grainset::{Grainset, GrainsetBody, NestedGrainset};
pub use joinset::{Joinset, JoinsetBody, NestedJoinset};
pub use refs::{
    AnyDataKindRef, ComplexDataKindRef, NestedDataKindRef, PublicDataKindRef, SimpleDataKindRef,
};
pub use storage::{CatalogRef, PartitionDef, StorageConfig, StorageFormat};
pub use traits::{ComplexDataKind, DataKind, NestedDataKind, PublicDataKind, SimpleDataKind};
pub use unionset::{NestedUnionset, UnionMode, Unionset, UnionsetBody};
pub use variants::{DataKindForm, DataKindVariant};
