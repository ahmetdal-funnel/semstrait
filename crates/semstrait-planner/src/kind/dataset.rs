//! DatasetPlanner — kind planner for Dataset kinds (single-dataset fast path).
//!
//! Builds Scan → Aggregate → Project for a single dataset with its binding.

use crate::error::PlannerError;
use crate::request::ResolvedQueryRequest;
use super::{KindPlanner, PlanFragment, PlannerContext, PrunedView};
use super::plan_builder;
use semstrait_manifest::acceleration::CompiledDataKind;

/// Kind planner for the Dataset variant — the simplest kind.
pub struct DatasetPlanner;

impl KindPlanner for DatasetPlanner {
    fn supports(&self, data_kind: &CompiledDataKind) -> bool {
        matches!(data_kind, CompiledDataKind::Dataset(_))
    }

    fn resolve(
        &self,
        pruned: &PrunedView<'_>,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        match pruned.data_kind() {
            CompiledDataKind::Dataset(dk) => {
                plan_builder::build_dataset_kind_plan(dk, request, ctx)
            }
            _ => Err(PlannerError::UnsupportedKindType(
                format!("DatasetPlanner cannot handle {:?}", pruned.data_kind().name()),
            )),
        }
    }
}
