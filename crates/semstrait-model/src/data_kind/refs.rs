//! View enums for heterogeneous iteration (`32 §3.6`).
//!
//! Pure declarations live here; trait impls (which match-dispatch over
//! the concrete inner reference) live in [`super::refs_impls`].

use super::dataset::{Dataset, NestedDataset};
use super::grainset::{Grainset, NestedGrainset};
use super::joinset::{Joinset, NestedJoinset};
use super::unionset::{NestedUnionset, Unionset};

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
