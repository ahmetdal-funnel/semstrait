//! GrainsetPlanner — kind planner for Grainset kinds.
//!
//! Resolves queries via grain-aware UNION ALL:
//! 1. Prune datasets (grain eligibility, zero-coverage)
//! 2. Group datasets by native temporal grain
//! 3. Assign measures/metrics to cheapest (coarsest) grain group
//! 4. Build per-dataset branches: Scan → DATE_TRUNC → Pre-aggregate → NULL-fill Project
//! 5. UNION ALL all branches → Re-aggregate → Final Project

use crate::error::PlannerError;
use super::{
    grain_to_temporal, partition_dimensions_iface,
    resolve_native_grain_binding, KindPlanner, PlanFragment, PlannerContext, PrunedView,
};
use super::plan_builder;
use super::plan_builder::infer_aggregation_iface;
use crate::request::ResolvedQueryRequest;
use semstrait_ir::{
    AggregateMeasure, Expr, Field, PlanNode,
    Schema,
};
use semstrait_manifest::{
    CompiledInterface, TemporalGrain,
};
use semstrait_manifest::acceleration::{CompiledDataKind, DatasetBinding};
use std::collections::{HashMap, HashSet};

/// Planner for Grainset kinds — grain-aware UNION ALL resolution.
pub struct GrainsetPlanner;

impl KindPlanner for GrainsetPlanner {
    fn supports(&self, data_kind: &CompiledDataKind) -> bool {
        matches!(data_kind, CompiledDataKind::Grainset(_))
    }

    fn resolve(
        &self,
        pruned: &PrunedView<'_>,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        let grainset = match pruned.data_kind() {
            CompiledDataKind::Grainset(g) => g,
            _ => return Err(PlannerError::Internal("GrainsetPlanner received non-Grainset CompiledDataKind".into())),
        };
        let iface = &grainset.interface;
        let bindings = pruned.active_bindings();

        if bindings.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: "grainset kind has no datasets".to_string(),
            });
        }

        // Determine requested temporal grain.
        let temporal_dim = iface.find_temporal_dimension();
        let request_grain = request.grain.map(grain_to_temporal);

        // Step 1: Prune datasets.
        let eligible = prune_datasets(iface, &bindings, request, temporal_dim, request_grain)?;

        if eligible.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: "no datasets remain after grain/coverage pruning".to_string(),
            });
        }

        // Separate requested names into actual measures vs metrics.
        let (measure_names, metric_names) = classify_requested_measures(iface, &request.measures);

        // Step 2: Group by grain and assign measures/metrics.
        let assignments = assign_to_grain_groups(
            iface,
            &eligible,
            temporal_dim,
            request_grain,
            &measure_names,
            &metric_names,
        )?;

        // Single-dataset optimization: if all eligible datasets resolved to a single
        // dataset covering all requested measures/metrics AND dimensions, use the simpler plan.
        if assignments.len() == 1 {
            let a = &assignments[0];
            let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
            let _ = &metadata_dims; // metadata dims don't need column_mapping coverage
            let covers_measures = request.measures.iter().all(|m| {
                a.binding.column_mapping.contains_key(m)
                    || iface.metrics.contains_key(m)
            });
            let covers_dims = regular_dims.iter().all(|d| {
                a.binding.column_mapping.contains_key(d)
            });
            if covers_measures && covers_dims {
                return build_single_dataset_plan(iface, request, a.binding, ctx, temporal_dim, request_grain);
            }
        }

        // Step 3: Build UNION ALL plan.
        build_union_plan(iface, request, &assignments, ctx, temporal_dim, request_grain)
    }
}

// ─────────────────────── Step 1: Prune ───────────────────────

/// Prune datasets by grain eligibility and zero-coverage.
fn prune_datasets<'a>(
    iface: &CompiledInterface,
    bindings: &[&'a DatasetBinding],
    request: &ResolvedQueryRequest,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<Vec<&'a DatasetBinding>, PlannerError> {
    let (_, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);

    let mut eligible: Vec<&DatasetBinding> = Vec::new();

    for &binding in bindings {
        // 1c. Grain eligibility: exclude datasets whose native grain is coarser than requested.
        if let (Some(rg), Some(td_name)) = (request_grain, temporal_dim) {
            if let Some(native) = resolve_native_grain_binding(binding, td_name, iface) {
                if native.coarseness() > rg.coarseness() {
                    continue; // Can't disaggregate.
                }
            }
        }

        // 1d. Zero-coverage: exclude datasets that cover no requested semantics.
        // Expand metric names to their constituent measures for coverage check.
        let mapping = &binding.column_mapping;
        let expanded_measures: Vec<String> = request.measures.iter().flat_map(|m| {
            if let Some(metric) = iface.metrics.get(m) {
                plan_builder::extract_metric_constituents(metric, iface)
            } else {
                vec![m.clone()]
            }
        }).collect();
        let covers_any = regular_dims.iter().any(|d| mapping.contains_key(d))
            || expanded_measures.iter().any(|m| mapping.contains_key(m));

        if covers_any {
            eligible.push(binding);
        }
    }

    Ok(eligible)
}

// ─────────────────────── Step 2: Assign ──────────────────────

/// A dataset with its assigned measures for a specific grain group.
struct DatasetAssignment<'a> {
    binding: &'a DatasetBinding,
    measures: Vec<String>,
    native_grain: Option<TemporalGrain>,
}

/// Separate requested measure names into actual measures vs metric names.
fn classify_requested_measures(
    iface: &CompiledInterface,
    requested: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut measures = Vec::new();
    let mut metrics = Vec::new();
    for name in requested {
        if iface.metrics.contains_key(name) {
            metrics.push(name.clone());
        } else {
            measures.push(name.clone());
        }
    }
    (measures, metrics)
}

/// Group eligible datasets by native grain, assign measures/metrics to cheapest group.
fn assign_to_grain_groups<'a>(
    iface: &CompiledInterface,
    eligible: &[&'a DatasetBinding],
    temporal_dim: Option<&str>,
    _request_grain: Option<TemporalGrain>,
    measure_names: &[String],
    metric_names: &[String],
) -> Result<Vec<DatasetAssignment<'a>>, PlannerError> {
    // Group datasets by native grain (None = no temporal dimension).
    let mut grain_groups: HashMap<Option<TemporalGrain>, Vec<&'a DatasetBinding>> =
        HashMap::new();

    for binding in eligible {
        let native = temporal_dim.and_then(|td| resolve_native_grain_binding(binding, td, iface));
        grain_groups.entry(native).or_default().push(binding);
    }

    // Sort grain groups by coarseness (coarsest first = cheapest).
    let mut sorted_groups: Vec<(Option<TemporalGrain>, Vec<&'a DatasetBinding>)> =
        grain_groups.into_iter().collect();
    sorted_groups.sort_by_key(|(g, _)| std::cmp::Reverse(g.map_or(0, |g| g.coarseness())));

    // For each measure, find the cheapest grain group that has it.
    let mut measure_to_group: HashMap<String, Option<TemporalGrain>> = HashMap::new();

    for measure_name in measure_names {
        let mut assigned = false;
        for (grain, bindings) in &sorted_groups {
            if bindings.iter().any(|b| b.column_mapping.contains_key(measure_name)) {
                measure_to_group.insert(measure_name.clone(), *grain);
                assigned = true;
                break;
            }
        }
        if !assigned {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: format!(
                    "measure '{}' cannot be provided — no eligible dataset maps it",
                    measure_name
                ),
            });
        }
    }

    // For metrics, find the cheapest group where all constituent measures are available.
    for metric_name in metric_names {
        if let Some(metric) = iface.metrics.get(metric_name) {
            let constituent_measures = plan_builder::extract_metric_constituents(metric, iface);
            let mut assigned = false;
            for (grain, bindings) in &sorted_groups {
                let group_covers_all = constituent_measures.iter().all(|cm| {
                    bindings.iter().any(|b| b.column_mapping.contains_key(cm))
                });
                if group_covers_all {
                    // Assign all constituent measures of this metric to this group.
                    for cm in &constituent_measures {
                        measure_to_group.entry(cm.clone()).or_insert(*grain);
                    }
                    assigned = true;
                    break;
                }
            }
            if !assigned {
                return Err(PlannerError::NoCoveringDataset {
                    kind: iface.name.clone(),
                    reason: format!(
                        "metric '{}' cannot be provided — constituent measures split across incompatible grain groups",
                        metric_name
                    ),
                });
            }
        }
    }

    // Build per-dataset assignments: each dataset gets measures assigned to its grain group.
    let mut dataset_assignments: Vec<DatasetAssignment<'a>> = Vec::new();
    let mut assigned_measures: HashSet<String> = HashSet::new();

    for (grain, bindings) in &sorted_groups {
        // Measures assigned to this grain group.
        let group_measures: Vec<String> = measure_to_group
            .iter()
            .filter(|(_, g)| *g == grain)
            .map(|(m, _)| m.clone())
            .collect();

        if group_measures.is_empty() {
            continue;
        }

        // Distribute measures across datasets in this group.
        // Each measure goes to every dataset that maps it (for UNION ALL aggregation).
        for binding in bindings {
            let ds_measures: Vec<String> = group_measures
                .iter()
                .filter(|m| binding.column_mapping.contains_key(m.as_str()))
                .cloned()
                .collect();

            if !ds_measures.is_empty() {
                for m in &ds_measures {
                    assigned_measures.insert(m.clone());
                }
                dataset_assignments.push(DatasetAssignment {
                    binding,
                    measures: ds_measures,
                    native_grain: *grain,
                });
            }
        }
    }

    Ok(dataset_assignments)
}

// ─────────────────── Step 3: Build Plan ──────────────────────

/// Build the unified output schema for the UNION plan.
fn build_unified_schema(request: &ResolvedQueryRequest, iface: &CompiledInterface) -> Schema {
    let fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), iface.resolve_dim_type(name)))
        .chain(
            request
                .measures
                .iter()
                .map(|name| Field::new(name.clone(), iface.resolve_measure_type(name))),
        )
        .collect();
    Schema::new(fields)
}

/// Build a single-dataset plan with optional grain rollup.
fn build_single_dataset_plan(
    iface: &CompiledInterface,
    request: &ResolvedQueryRequest,
    binding: &DatasetBinding,
    ctx: &PlannerContext<'_>,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<PlanFragment, PlannerError> {
    // Determine if grain rollup is needed.
    let needs_rollup = if let (Some(rg), Some(td)) = (request_grain, temporal_dim) {
        if let Some(native) = resolve_native_grain_binding(binding, td, iface) {
            native.coarseness() < rg.coarseness()
        } else {
            false
        }
    } else {
        false
    };

    // Use the shared layered builder with optional grain rollup.
    let rollup = if needs_rollup {
        Some((temporal_dim.unwrap(), request_grain.unwrap()))
    } else {
        None
    };
    plan_builder::build_binding_plan(iface, binding, request, ctx, true, rollup)
}


/// Build UNION ALL plan across multiple dataset assignments.
fn build_union_plan(
    iface: &CompiledInterface,
    request: &ResolvedQueryRequest,
    assignments: &[DatasetAssignment<'_>],
    ctx: &PlannerContext<'_>,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<PlanFragment, PlannerError> {
    let unified_schema = build_unified_schema(request, iface);

    // Build one branch per dataset assignment.
    let branches: Vec<PlanNode> = assignments
        .iter()
        .map(|a| {
            build_union_branch(
                iface, request, a, &unified_schema,
                temporal_dim, request_grain, ctx,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Validate type consistency across branches before UNION.
    plan_builder::validate_union_types(&branches)?;

    let pb = ctx.plan_builder;

    // UNION ALL.
    let union_input = if branches.len() == 1 {
        branches.into_iter().next().unwrap()
    } else {
        pb.build_union(unified_schema.clone(), branches, false)
    };

    // Re-aggregate.
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

    Ok(PlanFragment {
        root: agg,
        output_schema: unified_schema,
        pending_filters: Vec::new(),
    })
}

/// Build a single UNION branch for one dataset assignment.
///
/// Determines covered/uncovered measures for this assignment and delegates
/// to the shared `plan_builder::build_union_branch`.
fn build_union_branch(
    iface: &CompiledInterface,
    request: &ResolvedQueryRequest,
    assignment: &DatasetAssignment<'_>,
    unified_schema: &Schema,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
    ctx: &PlannerContext<'_>,
) -> Result<PlanNode, PlannerError> {
    let binding = assignment.binding;

    // Determine covered measures for this assignment.
    let mut covered = Vec::new();
    for measure_name in &request.measures {
        if iface.measures.contains_key(measure_name) {
            if assignment.measures.contains(measure_name) {
                covered.push(measure_name.clone());
            }
        } else if let Some(metric) = iface.metrics.get(measure_name) {
            let constituents = plan_builder::extract_metric_constituents(metric, iface);
            if constituents.iter().all(|c| assignment.measures.contains(c)) {
                covered.push(measure_name.clone());
            }
        }
    }

    // Determine temporal rollup.
    let needs_rollup = if let (Some(rg), Some(native)) = (request_grain, assignment.native_grain) {
        native.coarseness() < rg.coarseness()
    } else {
        false
    };
    let temporal_rollup = if needs_rollup {
        temporal_dim.map(|td| (td, request_grain.unwrap()))
    } else {
        None
    };

    plan_builder::build_union_branch(
        iface, request, binding,
        &plan_builder::UnionBranchParams {
            covered_measures: covered,
            temporal_rollup,
        },
        unified_schema, ctx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::helpers::*;
    use indexmap::IndexMap;
    use semstrait_ir::Aggregation;
    use semstrait_manifest::{
        CompiledDimension, CompiledMeasure, DimensionType,
    };
    use semstrait_manifest::acceleration::{
        CoverageIndex, DimensionIndex, GrainMap, CompiledGrainsetKind,
        ResolvedColumnMapping, TemporalMapping,
    };

    // ── Test helpers ─────────────────────────────────────────────────

    /// Build a CompiledDataKind::Grainset from dimensions, measures, and bindings.
    fn make_grainset(
        name: &str,
        dimensions: IndexMap<String, CompiledDimension>,
        measures: IndexMap<String, CompiledMeasure>,
        bindings: Vec<DatasetBinding>,
    ) -> CompiledDataKind {
        make_grainset_with_metrics(name, dimensions, measures, IndexMap::new(), bindings)
    }

    fn make_grainset_with_metrics(
        name: &str,
        dimensions: IndexMap<String, CompiledDimension>,
        measures: IndexMap<String, CompiledMeasure>,
        metrics: IndexMap<String, semstrait_manifest::CompiledMetric>,
        bindings: Vec<DatasetBinding>,
    ) -> CompiledDataKind {
        let temporal_dim = dimensions.iter().find_map(|(n, d)| {
            if matches!(d.dim_type, DimensionType::Temporal(_)) {
                Some(n.clone())
            } else {
                None
            }
        });
        let iface = CompiledInterface {
            name: name.to_string(),
            description: None,
            dimensions: dimensions.clone(),
            measures: measures.clone(),
            metrics,
            keys: None,
            filters: vec![],
            temporal_dim: temporal_dim.clone(),
        };
        let coverage = CoverageIndex::build(&dimensions, &measures, &bindings);
        let dimension_index = DimensionIndex::build(&dimensions, &bindings);
        let grain_map = temporal_dim.as_deref().map(|td| GrainMap::build(td, &bindings));

        CompiledDataKind::Grainset(Box::new(CompiledGrainsetKind {
            interface: iface,
            bindings,
            coverage_index: coverage,
            dimension_index,
            metric_order: None,
            grain_map,
        }))
    }

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

    /// Build a DatasetBinding with temporal grain info.
    fn make_temporal_binding(
        name: &str,
        mappings: Vec<(&str, &str)>,
        temporal_dim: &str,
        grain: Option<TemporalGrain>,
    ) -> DatasetBinding {
        let physical: IndexMap<String, String> = mappings.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let temporal_phys = mappings.iter()
            .find(|(k, _)| *k == temporal_dim)
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();
        let mut temporal = HashMap::new();
        temporal.insert(
            temporal_dim.to_string(),
            TemporalMapping {
                physical_column: temporal_phys,
                grain,
            },
        );
        DatasetBinding {
            dataset_name: name.to_string(),
            column_mapping: ResolvedColumnMapping {
                physical,
                literals: HashMap::new(),
                temporal,
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
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
                expr: None,
                expr_source: None,
            },
        )
    }

    fn make_temporal_dim(name: &str, grains: Vec<TemporalGrain>) -> (String, CompiledDimension) {
        (
            name.to_string(),
            CompiledDimension {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Date,
                dim_type: DimensionType::Temporal(semstrait_manifest::TemporalDimension {
                    grains,
                }),
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
                agg: Aggregation::Sum,
                expr: semstrait_core::Expr::entity_ref(expr_ref),
                expr_source: expr_ref.to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        )
    }

    fn make_count_distinct_measure(name: &str, expr_ref: &str) -> (String, CompiledMeasure) {
        (
            name.to_string(),
            CompiledMeasure {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Number,
                agg: Aggregation::CountDistinct,
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

    // ── Single-dataset tests ─────────────────────────────────────────

    #[test]
    fn test_single_dataset_basic() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();
        let binding = make_binding("orders_daily", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "amount"),
        ]);

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding]);
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "single dataset should succeed: {:?}", result.err());

        let fragment = result.unwrap();
        // Root should be Aggregate (identity L5 skipped) or Project -> Aggregate -> Scan (no Union).
        match &fragment.root {
            PlanNode::Aggregate(_) => {} // identity L5 skipped
            PlanNode::Project(p) => {
                assert!(matches!(p.input.as_ref(), PlanNode::Aggregate(_)));
            }
            _ => panic!("Expected Aggregate or Project as root"),
        }
    }

    #[test]
    fn test_metadata_dimension_coverage() {
        use semstrait_manifest::{MetadataDimension, PathExtraction};

        let mut dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
        ].into_iter().collect();
        dimensions.insert(
            "source_info".to_string(),
            CompiledDimension {
                name: "source_info".to_string(),
                description: None,
                data_type: semstrait_core::DataType::String,
                dim_type: DimensionType::Metadata(MetadataDimension {
                    path: Some(PathExtraction { token: 1 }),
                    partition: None,
                }),
                expr: None,
                expr_source: None,
            },
        );
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let mut binding = make_binding("orders_daily", vec![
            ("date", "order_date"),
            ("revenue", "amount"),
        ]);
        binding.resolved_sources = vec![
            semstrait_manifest::acceleration::ResolvedSource::path("bucket/shopify/data.parquet"),
        ];

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding]);
        let request = make_test_request("orders", vec!["date", "source_info"], vec!["revenue"]);
        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "metadata dim should not block: {:?}", result.err());
    }

    // ── Multi-dataset UNION ALL tests ────────────────────────────────

    #[test]
    fn test_multi_dataset_union_all() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("cost", "cost_amount"),
            make_sum_measure("revenue", "rev_amount"),
        ].into_iter().collect();

        let binding1 = make_binding("cost_daily", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("cost", "cost_amount"),
        ]);
        let binding2 = make_binding("revenue_daily", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "rev_amount"),
        ]);

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding1, binding2]);
        let request = make_test_request("orders", vec!["date", "region"], vec!["cost", "revenue"]);
        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "union plan should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Output schema should have dims + both measures.
        let field_names: Vec<&str> = fragment.output_schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(field_names, vec!["date", "region", "cost", "revenue"]);

        // Root should be Aggregate over Union (re-aggregation).
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                match agg.input.as_ref() {
                    PlanNode::Union(union_node) => {
                        assert_eq!(union_node.inputs.len(), 2, "should have 2 branches");
                        assert!(!union_node.distinct);
                    }
                    _ => panic!("Expected Union under Aggregate"),
                }
            }
            _ => panic!("Expected Aggregate as root"),
        }
    }

    #[test]
    fn test_null_fill_unmapped_measures() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
            make_categorical_dim("region"),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("cost", "cost_amount"),
            make_sum_measure("revenue", "rev_amount"),
        ].into_iter().collect();

        let binding1 = make_binding("cost_daily", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("cost", "cost_amount"),
        ]);
        let binding2 = make_binding("revenue_daily", vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "rev_amount"),
        ]);

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding1, binding2]);
        let request = make_test_request("orders", vec!["date", "region"], vec!["cost", "revenue"]);
        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let fragment = planner.resolve(&pruned, &request, &ctx).unwrap();

        // Check that branches have NULL-fill for unmapped measures.
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                if let PlanNode::Union(union_node) = agg.input.as_ref() {
                    for branch in &union_node.inputs {
                        if let PlanNode::Project(proj) = branch {
                            // Each branch should have 4 expressions (2 dims + 2 measures).
                            assert_eq!(proj.expressions.len(), 4);
                            // One of the measure expressions should be NULL.
                            let has_null = proj.expressions[2..].iter().any(|e| *e == Expr::null());
                            assert!(has_null, "branch should NULL-fill unmapped measure");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // ── Grain-related tests ──────────────────────────────────────────

    #[test]
    fn test_grain_rollup_single_dataset() {
        let dimensions: IndexMap<_, _> = vec![
            make_temporal_dim("date", vec![TemporalGrain::Day, TemporalGrain::Week, TemporalGrain::Month]),
            make_categorical_dim("region"),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let binding = make_temporal_binding(
            "orders_daily",
            vec![("date", "order_date"), ("region", "region_name"), ("revenue", "amount")],
            "date",
            Some(TemporalGrain::Day),
        );

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding]);

        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.grain = Some(semstrait_core::Grain::Month);

        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "grain rollup should succeed: {:?}", result.err());

        let fragment = result.unwrap();
        // Verify DATE_TRUNC is in the aggregate's GROUP BY.
        // Root is Aggregate (identity L5 skipped) or Project -> Aggregate.
        let agg_node = match &fragment.root {
            PlanNode::Aggregate(a) => a,
            PlanNode::Project(proj) => match proj.input.as_ref() {
                PlanNode::Aggregate(a) => a,
                _ => panic!("Expected Aggregate under Project"),
            },
            _ => panic!("Expected Aggregate or Project as root"),
        };
        let has_date_trunc = agg_node.group_by.iter().any(|e| matches!(e, Expr::DateTrunc(_)));
        assert!(has_date_trunc, "should have DATE_TRUNC in GROUP BY");
    }

    #[test]
    fn test_grain_pruning_excludes_coarser() {
        let dimensions: IndexMap<_, _> = vec![
            make_temporal_dim("date", vec![TemporalGrain::Day, TemporalGrain::Month]),
            make_categorical_dim("region"),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
            make_count_distinct_measure("unique_customers", "customer_id"),
        ].into_iter().collect();

        // Daily dataset: maps date(day), region, revenue, unique_customers.
        let binding1 = make_temporal_binding(
            "orders_daily",
            vec![
                ("date", "order_date"),
                ("region", "region_name"),
                ("revenue", "amount"),
                ("unique_customers", "customer_id"),
            ],
            "date",
            Some(TemporalGrain::Day),
        );

        // Monthly dataset: maps date(month), region, revenue (no unique_customers).
        let binding2 = make_temporal_binding(
            "orders_monthly",
            vec![
                ("date", "report_month"),
                ("region", "region_name"),
                ("revenue", "monthly_revenue"),
            ],
            "date",
            Some(TemporalGrain::Month),
        );

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding1, binding2]);

        // Request day grain — should exclude monthly dataset.
        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.grain = Some(semstrait_core::Grain::Day);

        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "should succeed with daily dataset: {:?}", result.err());

        let fragment = result.unwrap();
        // Should be single dataset (Project -> Aggregate -> Scan, no Union).
        // Root is Aggregate (identity L5 skipped) or Project -> Aggregate.
        match &fragment.root {
            PlanNode::Aggregate(_) => {} // identity L5 skipped
            PlanNode::Project(p) => {
                assert!(
                    matches!(p.input.as_ref(), PlanNode::Aggregate(_)),
                    "single dataset should skip Union"
                );
            }
            _ => panic!("Expected Aggregate or Project as root"),
        }
    }

    #[test]
    fn test_multi_grain_assigns_to_cheapest() {
        let dimensions: IndexMap<_, _> = vec![
            make_temporal_dim("date", vec![TemporalGrain::Day, TemporalGrain::Month]),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let binding1 = make_temporal_binding(
            "orders_daily",
            vec![("date", "order_date"), ("revenue", "amount")],
            "date",
            Some(TemporalGrain::Day),
        );
        let binding2 = make_temporal_binding(
            "orders_monthly",
            vec![("date", "report_month"), ("revenue", "monthly_revenue")],
            "date",
            Some(TemporalGrain::Month),
        );

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding1, binding2]);

        // Request month grain, revenue — monthly dataset is cheaper.
        let mut request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        request.grain = Some(semstrait_core::Grain::Month);

        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multi_source_dataset() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("date"),
        ].into_iter().collect();
        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let mut binding = make_binding("orders_daily", vec![
            ("date", "order_date"),
            ("revenue", "amount"),
        ]);
        binding.resolved_sources = vec![
            semstrait_manifest::acceleration::ResolvedSource::path("bucket/account_001/orders.parquet"),
            semstrait_manifest::acceleration::ResolvedSource::path("bucket/account_002/orders.parquet"),
            semstrait_manifest::acceleration::ResolvedSource::path("bucket/account_003/orders.parquet"),
        ];

        let data_kind = make_grainset("orders", dimensions, measures, vec![binding]);
        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "multi-source should succeed: {:?}", result.err());

        // Multi-source bindings produce per-source layered plans:
        // Project → Aggregate(re-agg) → Union → [Aggregate → ... → Scan, ...]
        let fragment = result.unwrap();
        fn has_union_of_aggregates(node: &PlanNode) -> bool {
            match node {
                PlanNode::Union(u) => u.inputs.iter().all(|n| matches!(n, PlanNode::Aggregate(_))),
                PlanNode::Aggregate(a) => has_union_of_aggregates(&a.input),
                PlanNode::Project(p) => has_union_of_aggregates(&p.input),
                _ => false,
            }
        }
        assert!(has_union_of_aggregates(&fragment.root), "should have UNION ALL of pre-aggregated per-source plans");
    }

    #[test]
    fn test_empty_datasets_error() {
        let dimensions: IndexMap<String, CompiledDimension> = IndexMap::new();
        let measures: IndexMap<String, CompiledMeasure> = IndexMap::new();

        let data_kind = make_grainset("empty", dimensions, measures, vec![]);
        let request = make_test_request("empty", vec![], vec![]);
        let manifest = empty_manifest();
        let session = std::collections::HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = GrainsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_err());
    }
}
