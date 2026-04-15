//! SimplePlanner — kind planner for Simple kinds (single-dataset fast path).
//!
//! Builds Scan → Aggregate → Project for a single dataset with its binding.

use crate::error::PlannerError;
use crate::request::ResolvedQueryRequest;
use super::{DataKindPlanner, PlanFragment, PlannerContext, PrunedView};
use super::plan_layers;
use semstrait_manifest::acceleration::CompiledDataKind;

/// Kind planner for the Simple variant — the simplest kind.
pub struct SimplePlanner;

impl DataKindPlanner for SimplePlanner {
    fn supports(&self, data_kind: &CompiledDataKind) -> bool {
        matches!(data_kind, CompiledDataKind::Simple(_))
    }

    fn resolve(
        &self,
        pruned: &PrunedView<'_>,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        match pruned.data_kind() {
            CompiledDataKind::Simple(dk) => {
                plan_layers::build_dataset_kind_plan(dk, request, ctx)
            }
            _ => Err(PlannerError::UnsupportedKindType(
                format!("SimplePlanner cannot handle {:?}", pruned.data_kind().name()),
            )),
        }
    }
}
