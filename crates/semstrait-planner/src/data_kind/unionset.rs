//! UnionsetPlanner — kind planner for Unionset kinds.
//!
//! Builds UNION ALL branches with NULL-fill for unmapped columns.
//! Each dataset becomes a branch: Scan -> Aggregate -> Project (with NULLs for missing cols).
//! Branches are combined with UNION ALL, then re-aggregated.

use std::sync::Arc;
use crate::error::PlannerError;
use super::plan_layers;
use super::plan_layers::infer_aggregation_iface;
use super::{DataKindPlanner, PlanFragment, PlannerContext, PrunedView};
use crate::request::ResolvedQueryRequest;
use semstrait_ir::{
    AggregateMeasure, Expr, PlanNode,
};
use semstrait_manifest::{CompiledInterface, UnionMode};
use semstrait_manifest::acceleration::{CompiledDataKind, DatasetBinding};

/// Planner for Unionset kinds — UNION ALL across multiple datasets.
pub struct UnionsetPlanner;

impl DataKindPlanner for UnionsetPlanner {
    fn supports(&self, data_kind: &CompiledDataKind) -> bool {
        matches!(data_kind, CompiledDataKind::Unionset(_))
    }

    fn resolve(
        &self,
        pruned: &PrunedView<'_>,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        let unionset = match pruned.data_kind() {
            CompiledDataKind::Unionset(u) => u,
            _ => return Err(PlannerError::Internal("UnionsetPlanner received non-Unionset CompiledDataKind".into())),
        };
        let iface = &unionset.interface;
        let bindings = pruned.active_bindings();

        if bindings.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: "unionset kind has no datasets".to_string(),
            });
        }

        // Extract union mode from the unionset kind.
        let distinct = unionset.mode == UnionMode::Unique;

        // Build one branch per dataset binding.
        let branches: Vec<PlanNode> = bindings
            .iter()
            .map(|binding| build_union_branch(iface, request, binding, ctx))
            .collect::<Result<Vec<_>, _>>()?;

        // Validate type consistency across branches before UNION.
        plan_layers::validate_union_types(&branches)?;

        let pb = ctx.plan_builder;

        // If only one branch, skip the UNION node.
        let union_input = if branches.len() == 1 {
            // Safe: we just verified len == 1.
            branches.into_iter().next().expect("checked len == 1")
        } else {
            // All branches share the same output schema (the unified schema).
            let schema = Arc::clone(&branches[0].meta().output_schema);
            pb.build_union((*schema).clone(), branches, distinct)
        };

        // Re-aggregate across all branches.
        let unified_schema = plan_layers::build_unified_schema(request, iface);

        let group_by: Vec<Expr> = request
            .dimensions
            .iter()
            .map(|name| Expr::column(name.clone()))
            .collect();

        let aggregates: Vec<AggregateMeasure> = request
            .measures
            .iter()
            .map(|name| AggregateMeasure {
                function: infer_aggregation_iface(iface, name),
                expr: Expr::column(name.clone()),
                distinct: false,
                data_type: iface.resolve_measure_type(name),
            })
            .collect();

        let agg = pb.build_aggregate(unified_schema.clone(), union_input, group_by, aggregates);

        Ok(PlanFragment { root: agg })
    }
}

/// Build a single UNION branch for one dataset binding.
///
/// Computes covered/uncovered measures, then delegates to the shared
/// `plan_layers::build_union_branch` for Scan → Rename → Expression → Aggregate → Project.
fn build_union_branch(
    iface: &CompiledInterface,
    request: &ResolvedQueryRequest,
    binding: &DatasetBinding,
    ctx: &PlannerContext<'_>,
) -> Result<PlanNode, PlannerError> {
    let mapping = &binding.column_mapping;

    // Determine which measures/metrics are covered by this binding.
    let mut covered_measures: Vec<String> = Vec::new();

    for measure_name in &request.measures {
        if iface.measures.contains_key(measure_name) {
            if mapping.contains_key(measure_name) {
                covered_measures.push(measure_name.clone());
            }
        } else if let Some(metric) = iface.metrics.get(measure_name) {
            let constituents = plan_layers::extract_metric_constituents(metric, iface);
            if constituents.iter().all(|c| mapping.contains_key(c)) {
                covered_measures.push(measure_name.clone());
            }
        }
    }

    let unified_schema = plan_layers::build_unified_schema(request, iface);

    let params = plan_layers::UnionBranchParams {
        covered_measures,
        temporal_rollup: None,
    };

    plan_layers::build_union_branch(iface, request, binding, &params, &unified_schema, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::PlannerContext;
    use crate::tests::helpers::*;
    use indexmap::IndexMap;
    use semstrait_manifest::{
        CompiledDimension, CompiledMeasure,
    };
    use semstrait_manifest::acceleration::{
        CoverageIndex, DimensionIndex, ResolvedColumnMapping, CompiledUnionsetKind,
    };
    use std::collections::HashMap;

    // ── Test helpers ─────────────────────────────────────────────────

    /// Build a simple DatasetBinding with physical-only mappings.
    fn make_binding(name: &str, mappings: Vec<(&str, &str)>) -> DatasetBinding {
        let physical: IndexMap<String, String> = mappings.into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        DatasetBinding {
            dataset_name: name.to_string(),
            column_mapping: ResolvedColumnMapping {
                physical,
                literals: HashMap::new(),
                temporal: HashMap::new(),
                anchored: HashMap::new(),
            },
            resolved_sources: vec![],
        }
    }

    fn make_categorical_dim(name: &str) -> (String, CompiledDimension) {
        (
            name.to_string(),
            CompiledDimension {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::String,
                dim_type: semstrait_manifest::DimensionType::Categorical(
                    semstrait_manifest::CategoricalDimension { enum_values: None },
                ),
                expr: None,
                expr_source: None,
            },
        )
    }

    fn make_sum_measure(name: &str, expr_ref: &str) -> (String, CompiledMeasure) {
        (
            name.to_string(),
            CompiledMeasure {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Number,
                agg: semstrait_core::Aggregation::Sum,
                expr: semstrait_core::Expr::entity_ref(expr_ref),
                expr_source: expr_ref.to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        )
    }

    fn empty_manifest() -> semstrait_manifest::CompiledManifest {
        semstrait_manifest::CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test".to_string(),
            relationships: vec![],
            model_name: "test".to_string(),
            model_description: None,
            entities: IndexMap::new(),
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            semantic_graph: semstrait_manifest::SemanticGraph::default(),
            catalog_snapshot: None,
        }
    }

    /// Build a CompiledDataKind::Unionset from dimensions, measures, and bindings.
    fn make_unionset(
        name: &str,
        dimensions: IndexMap<String, CompiledDimension>,
        measures: IndexMap<String, CompiledMeasure>,
        bindings: Vec<DatasetBinding>,
        mode: UnionMode,
    ) -> CompiledDataKind {
        let iface = CompiledInterface {
            name: name.to_string(),
            description: None,
            dimensions: dimensions.clone(),
            measures: measures.clone(),
            metrics: IndexMap::new(),
            keys: None,
            filters: vec![],
            temporal_dim: None,
        };
        let coverage = CoverageIndex::build(&dimensions, &measures, &bindings);
        let dimension_index = DimensionIndex::build(&dimensions, &bindings);

        CompiledDataKind::Unionset(Box::new(CompiledUnionsetKind {
            interface: iface,
            mode,
            bindings,
            coverage_index: coverage,
            dimension_index,
            metric_order: None,
        }))
    }

    /// Create a Unionset kind with two datasets, each covering all columns.
    fn make_unionset_data_kind() -> CompiledDataKind {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        // Dataset 1: covers date + region + revenue
        let ds1 = make_binding("orders_us", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "amount"),
        ]);

        // Dataset 2: covers date + region + revenue (different physical names)
        let ds2 = make_binding("orders_eu", vec![
            ("date", "sale_date"),
            ("region", "region"),
            ("revenue", "total"),
        ]);

        make_unionset("all_orders", dimensions, measures, vec![ds1, ds2], UnionMode::All)
    }

    #[test]
    fn test_unionset_two_datasets() {
        let data_kind = make_unionset_data_kind();
        let request = make_test_request("all_orders", vec!["date", "region"], vec!["revenue"]);

        let manifest = empty_manifest();
        let session = HashMap::new();
        let default_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &default_builder,
        };

        let planner = UnionsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "unionset resolve should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Root should be Aggregate (re-aggregation over UNION).
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                // Input should be Union node.
                match agg.input.as_ref() {
                    PlanNode::Union(union_node) => {
                        assert_eq!(union_node.inputs.len(), 2, "should have 2 UNION branches");
                        // Each branch should be a Project node.
                        for branch in &union_node.inputs {
                            assert!(
                                matches!(branch, PlanNode::Project(_)),
                                "each branch should be a Project"
                            );
                        }
                    }
                    _ => panic!("expected Union node under Aggregate"),
                }
            }
            _ => panic!("expected Aggregate node as root"),
        }

        // Check output schema.
        assert_eq!(fragment.root.meta().output_schema.fields.len(), 3); // date, region, revenue
    }

    #[test]
    fn test_unionset_single_dataset() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let ds1 = make_binding("orders_us", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "amount"),
        ]);

        let data_kind = make_unionset("all_orders", dimensions, measures, vec![ds1], UnionMode::All);
        let request = make_test_request("all_orders", vec!["date"], vec!["revenue"]);

        let manifest = empty_manifest();
        let session = HashMap::new();
        let default_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &default_builder,
        };

        let planner = UnionsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok());

        let fragment = result.unwrap();
        // With single dataset, no Union node — just Aggregate over Project.
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                assert!(
                    matches!(agg.input.as_ref(), PlanNode::Project(_)),
                    "single dataset should skip Union node"
                );
            }
            _ => panic!("expected Aggregate as root"),
        }
    }

    #[test]
    fn test_unionset_null_fill() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        // Dataset 1: covers date + revenue but NOT region.
        let ds1 = make_binding("partial_orders", vec![
            ("date", "order_date"),
            ("revenue", "amount"),
        ]);

        let data_kind = make_unionset("partial", dimensions, measures, vec![ds1], UnionMode::All);
        let request = make_test_request("partial", vec!["date", "region"], vec!["revenue"]);

        let manifest = empty_manifest();
        let session = HashMap::new();
        let default_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &default_builder,
        };

        let planner = UnionsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "should succeed with NULL-fill for missing region");

        let fragment = result.unwrap();
        // Verify the branch's Project has a NULL expression for the unmapped "region".
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                match agg.input.as_ref() {
                    PlanNode::Project(proj) => {
                        // expressions[1] should be NULL (region is unmapped).
                        assert_eq!(proj.expressions[1], Expr::null());
                    }
                    _ => panic!("expected Project under Aggregate"),
                }
            }
            _ => panic!("expected Aggregate as root"),
        }
    }

    #[test]
    fn test_unionset_empty_datasets() {
        let dimensions: IndexMap<String, CompiledDimension> = IndexMap::new();
        let measures: IndexMap<String, CompiledMeasure> = IndexMap::new();

        let data_kind = make_unionset("empty", dimensions, measures, vec![], UnionMode::All);
        let request = make_test_request("empty", vec![], vec![]);

        let manifest = empty_manifest();
        let session = HashMap::new();
        let default_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &default_builder,
        };

        let planner = UnionsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::NoCoveringDataset { .. }
        ));
    }

    #[test]
    fn test_unionset_distinct_mode() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let ds1 = make_binding("orders_us", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "amount"),
        ]);
        let ds2 = make_binding("orders_eu", vec![
            ("date", "sale_date"),
            ("region", "region"),
            ("revenue", "total"),
        ]);

        let data_kind = make_unionset("all_orders", dimensions, measures, vec![ds1, ds2], UnionMode::Unique);
        let request = make_test_request("all_orders", vec!["date", "region"], vec!["revenue"]);

        let manifest = empty_manifest();
        let session = HashMap::new();
        let default_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &default_builder,
        };

        let planner = UnionsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "unionset distinct should succeed");

        let fragment = result.unwrap();
        // Root should be Aggregate over Union with distinct=true.
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                match agg.input.as_ref() {
                    PlanNode::Union(union_node) => {
                        assert!(union_node.distinct, "union node should be distinct");
                        assert_eq!(union_node.inputs.len(), 2);
                    }
                    _ => panic!("expected Union node under Aggregate"),
                }
            }
            _ => panic!("expected Aggregate as root"),
        }
    }
}
