//! Dataset selector module
//!
//! Selects optimal dataset(s) from a model to serve a query.
//!
//! Supports two selection modes:
//! - Single dataset: When one dataset can satisfy all query requirements
//! - Multi-dataset JOIN: When measures span multiple datasets in the same grain set

mod error;
mod select;

pub use error::SelectError;
pub use select::{
    select_datasets,
    select_datasets_for_join,
    select_partial_for_grain_set,
    SelectedDataset,
    MultiDatasetSelection,
    DatasetWithMeasures,
    PartialSelection,
};
