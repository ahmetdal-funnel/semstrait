//! GrainsetPlanner — kind planner for Grainset kinds.
//!
//! Resolves queries via grain-aware UNION ALL:
//! 1. Prune datasets (grain eligibility, zero-coverage)
//! 2. Group datasets by native temporal grain
//! 3. Assign measures/metrics to cheapest (coarsest) grain group
//! 4. Build per-dataset branches: Scan → DATE_TRUNC → Pre-aggregate → NULL-fill Project
//! 5. UNION ALL all branches → Re-aggregate → Final Project

use crate::error::PlannerError;
use crate::decomposer::{self, DecomposedMeasure};
use crate::resolver::PhysicalResolver;
use super::{
    extract_metadata_value_binding, grain_to_temporal, partition_dimensions_iface,
    resolve_guards, resolve_native_grain_binding, split_computed_dims, KindPlanner, PlanFragment, PlannerContext, PrunedView,
};
use super::plan_builder;
use super::plan_builder::{build_scan_node_binding, build_semantic_type_map, infer_aggregation_iface};
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
use super::collect_column_refs;

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
                extract_metric_constituents(metric, iface)
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
            let constituent_measures = extract_metric_constituents(metric, iface);
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

/// Extract transitive leaf measure names from a metric's expression.
///
/// Recursively expands nested metric references to their underlying measures.
/// For example, `roi = profit / cost` where `profit = revenue - cost` returns
/// `["revenue", "cost"]` — the actual physical measures, not intermediate metrics.
pub fn extract_metric_constituents(
    metric: &semstrait_manifest::CompiledMetric,
    iface: &CompiledInterface,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_leaf_measures(&metric.expr, iface, &mut names, &mut seen);
    names
}

/// Collect transitive leaf measure names from an expression tree.
///
/// When a leaf reference is itself a metric in the interface, recursively
/// expands it to its underlying measures. This ensures coverage checks
/// operate on actual physical measures, consistent with how
/// `lower_metric_iface()` decomposes metrics at lowering time.
fn collect_leaf_measures(
    expr: &Expr,
    iface: &CompiledInterface,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    match expr {
        Expr::Column(col) => {
            resolve_leaf_or_expand(&col.name, iface, out, seen);
        }
        Expr::EntityRef(er) => {
            resolve_leaf_or_expand(&er.name, iface, out, seen);
        }
        Expr::BinaryOp(bin) => {
            collect_leaf_measures(&bin.left, iface, out, seen);
            collect_leaf_measures(&bin.right, iface, out, seen);
        }
        Expr::Case(case) => {
            for wc in &case.when_then {
                collect_leaf_measures(&wc.condition, iface, out, seen);
                collect_leaf_measures(&wc.result, iface, out, seen);
            }
            if let Some(e) = &case.else_expr {
                collect_leaf_measures(e, iface, out, seen);
            }
        }
        _ => {}
    }
}

/// If `name` is a nested metric, recursively expand; otherwise keep as leaf measure.
fn resolve_leaf_or_expand(
    name: &str,
    iface: &CompiledInterface,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if let Some(sub_metric) = iface.metrics.get(name) {
        // Nested metric — recursively expand to leaf measures.
        // The seen-set prevents infinite recursion on circular references
        // (which should be caught at compile time, but defensive here).
        if seen.insert(format!("__metric__{}", name)) {
            collect_leaf_measures(&sub_metric.expr, iface, out, seen);
        }
    } else if seen.insert(name.to_string()) {
        // Leaf measure (or unknown ref) — include as constituent.
        out.push(name.to_string());
    }
}

// ─────────────────── Step 3: Build Plan ──────────────────────

/// How a dimension is resolved in a UNION branch.
enum DimSource {
    Physical(String),
    MetadataLiteral(Expr),
    Computed(Expr),
    NullFill,
}

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

    // Use the shared single-dataset builder, but with grain rollup if needed.
    if needs_rollup {
        build_single_dataset_with_rollup(iface, request, binding, ctx, temporal_dim.unwrap(), request_grain.unwrap())
    } else {
        plan_builder::build_binding_plan(iface, binding, request, ctx, true)
    }
}

/// Build a single-dataset plan with DATE_TRUNC grain rollup.
fn build_single_dataset_with_rollup(
    iface: &CompiledInterface,
    request: &ResolvedQueryRequest,
    binding: &DatasetBinding,
    ctx: &PlannerContext<'_>,
    temporal_dim_name: &str,
    request_grain: TemporalGrain,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &binding.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_only, computed_dims) = split_computed_dims(&regular_dims, iface);

    // Map physical dimensions, applying DATE_TRUNC for the temporal dimension.
    let mut dim_physical: Vec<(String, DimResolve)> = Vec::new();
    let mut metadata_literals: Vec<(String, Expr)> = Vec::new();

    for dim_name in &physical_only {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            metadata_literals.push((dim_name.clone(), Expr::string(lit_val.clone())));
        } else if let Some(phys) = mapping.physical.get(dim_name) {
            if dim_name == temporal_dim_name {
                dim_physical.push((dim_name.clone(), DimResolve::DateTrunc(phys.clone(), request_grain)));
            } else {
                dim_physical.push((dim_name.clone(), DimResolve::Column(phys.clone())));
            }
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys.clone());
            }
        } else {
            return Err(PlannerError::DimensionNotFound {
                kind: iface.name.clone(),
                dimension: dim_name.clone(),
            });
        }
    }

    // Pre-compute metadata dimension values so computed expressions can reference them.
    let metadata_values: HashMap<String, String> = metadata_dims
        .iter()
        .map(|(name, meta)| {
            let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
            (name.clone(), value)
        })
        .collect();

    // Handle computed dimensions: collect referenced columns for scan and GROUP BY.
    let mut lowered_computed: Vec<(String, semstrait_core::Expr)> = Vec::new();
    let mut extra_group_by_cols: Vec<(String, String)> = Vec::new(); // (semantic, physical)

    for (dim_name, expr) in &computed_dims {
        let mut sem_refs: Vec<String> = Vec::new();
        let mut sem_seen_refs: HashSet<String> = HashSet::new();
        collect_column_refs(expr, &mut sem_refs, &mut sem_seen_refs);

        // Inline literal and metadata values, then collect physical scan columns.
        let resolved = resolve_guards(expr).transform(
            &|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
                if let Expr::Column(col) = e {
                    if let Some(lit_val) = mapping.literals.get(&col.name) {
                        return Ok(Some(Expr::string(lit_val.clone())));
                    }
                    if let Some(meta_val) = metadata_values.get(&col.name) {
                        return Ok(Some(Expr::string(meta_val.clone())));
                    }
                }
                Ok(None)
            },
        ).expect("literal inlining is infallible");

        for sem_ref in &sem_refs {
            if let Some(phys) = mapping.physical.get(sem_ref) {
                if scan_seen.insert(phys.clone()) {
                    scan_columns.push(phys.clone());
                }
                // Add to GROUP BY if not already a physical dim.
                if !dim_physical.iter().any(|(s, _)| s == sem_ref) {
                    extra_group_by_cols.push((sem_ref.clone(), phys.clone()));
                }
            }
        }
        lowered_computed.push((dim_name.clone(), resolved));
    }

    for (dim_name, _meta) in &metadata_dims {
        metadata_literals.push((dim_name.clone(), Expr::string(metadata_values.get(dim_name).cloned().unwrap_or_default())));
    }

    // Lower measures.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    let mut lowered_measures: Vec<(String, DecomposedMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = decomposer::decompose_measure(
                &phys_resolver, measure_name, measure.agg, &measure.expr, &measure.filters, &measure.data_type,
            )?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if iface.metrics.contains_key(measure_name) {
            let metric = &iface.metrics[measure_name];
            let lowered = decomposer::decompose_metric(measure_name, metric, iface, binding, 4)?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: iface.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Build Scan (multi-source aware).
    let pb = ctx.plan_builder;
    let sem_types = build_semantic_type_map(iface, &binding.column_mapping.physical);
    let scan = build_scan_node_binding(binding, &scan_columns, &sem_types, pb);

    // Build Aggregate with DATE_TRUNC in GROUP BY + extra columns for computed dims.
    let mut group_by: Vec<Expr> = dim_physical
        .iter()
        .map(|(_, resolve)| match resolve {
            DimResolve::Column(phys) => Expr::column(phys.clone()),
            DimResolve::DateTrunc(phys, grain) => {
                Expr::date_trunc((*grain).into(), Expr::column(phys.clone()))
            }
        })
        .collect();
    for (_, phys) in &extra_group_by_cols {
        group_by.push(Expr::column(phys.clone()));
    }

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = dim_physical
        .iter()
        .map(|(semantic, _)| Field::new(semantic.clone(), iface.resolve_dim_type(semantic)))
        .collect();
    for (semantic, _) in &extra_group_by_cols {
        agg_fields.push(Field::new(semantic.clone(), iface.resolve_dim_type(semantic)));
    }
    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        for (j, agg_m) in lowered.aggregates.iter().enumerate() {
            if j == 0 {
                agg_fields.push(Field::new(semantic.clone(), iface.resolve_measure_type(semantic)));
            } else {
                agg_fields.push(Field::new(format!("__agg_{}", agg_idx), agg_m.data_type.clone()));
            }
            agg_idx += 1;
        }
    }
    let agg_schema = Schema::new(agg_fields);

    let agg = pb.build_aggregate(agg_schema, scan, group_by, aggregates);

    // Build Project.
    let mut project_exprs: Vec<Expr> = Vec::new();
    let mut project_fields: Vec<Field> = Vec::new();

    for dim_name in &request.dimensions {
        if let Some((_, lit_expr)) = metadata_literals.iter().find(|(n, _)| n == dim_name) {
            project_exprs.push(lit_expr.clone());
        } else if let Some((_, comp_expr)) = lowered_computed.iter().find(|(n, _)| n == dim_name) {
            project_exprs.push(comp_expr.clone());
        } else {
            project_exprs.push(Expr::column(dim_name.clone()));
        }
        project_fields.push(Field::new(dim_name.clone(), iface.resolve_dim_type(dim_name)));
    }
    for (_, lowered) in &lowered_measures {
        project_exprs.push(lowered.post_agg_expr.clone());
    }
    project_fields.extend(
        lowered_measures
            .iter()
            .map(|(name, _)| Field::new(name.clone(), iface.resolve_measure_type(name))),
    );
    let project_schema = Schema::new(project_fields);

    let project = pb.build_project(project_schema.clone(), agg, project_exprs);

    Ok(PlanFragment {
        root: project,
        output_schema: project_schema,
        pending_filters: Vec::new(),
    })
}

/// How a dimension is resolved for GROUP BY.
enum DimResolve {
    Column(String),
    DateTrunc(String, TemporalGrain),
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
    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);

    // All requested measure/metric names for the unified schema.
    let all_measure_names: Vec<&str> = request.measures.iter().map(|s| s.as_str()).collect();

    // Build one branch per dataset assignment.
    let branches: Vec<PlanNode> = assignments
        .iter()
        .map(|a| {
            build_union_branch(
                iface,
                request,
                a,
                &metadata_dims,
                &regular_dims,
                &all_measure_names,
                &unified_schema,
                temporal_dim,
                request_grain,
                ctx,
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
#[allow(clippy::too_many_arguments)]
fn build_union_branch(
    iface: &CompiledInterface,
    _request: &ResolvedQueryRequest,
    assignment: &DatasetAssignment<'_>,
    metadata_dims: &[(String, semstrait_manifest::MetadataDimension)],
    regular_dims: &[String],
    all_measure_names: &[&str],
    unified_schema: &Schema,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
    ctx: &PlannerContext<'_>,
) -> Result<PlanNode, PlannerError> {
    let binding = assignment.binding;
    let mapping = &binding.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Determine if this branch needs DATE_TRUNC.
    let needs_rollup = if let (Some(rg), Some(native)) = (request_grain, assignment.native_grain) {
        native.coarseness() < rg.coarseness()
    } else {
        false
    };

    // Split regular dims into physical and computed.
    let (physical_regular, computed_dims) = split_computed_dims(regular_dims, iface);

    let computed_map: HashMap<&str, &semstrait_core::Expr> = computed_dims
        .iter()
        .map(|(name, expr)| (name.as_str(), expr))
        .collect();

    // Pre-compute metadata dimension values so computed expressions can reference them.
    let metadata_values: HashMap<String, String> = metadata_dims
        .iter()
        .map(|(name, meta)| {
            let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
            (name.clone(), value)
        })
        .collect();

    // Track extra columns needed by computed expressions for GROUP BY.
    let mut extra_group_by: Vec<(String, String)> = Vec::new(); // (semantic, physical)

    // Resolve regular dimensions.
    let mut dim_sources: Vec<(String, DimSource)> = Vec::new();
    for dim_name in regular_dims {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            dim_sources.push((dim_name.clone(), DimSource::MetadataLiteral(Expr::string(lit_val.clone()))));
        } else if let Some(expr) = computed_map.get(dim_name.as_str()) {
            // Computed dimension: check if referenced columns are available
            // (as physical columns, literal values, or metadata dimension values).
            let mut sem_refs: Vec<String> = Vec::new();
            let mut sem_seen_refs: HashSet<String> = HashSet::new();
            collect_column_refs(expr, &mut sem_refs, &mut sem_seen_refs);
            let all_mapped = sem_refs.iter().all(|r| {
                mapping.physical.contains_key(r)
                    || mapping.literals.contains_key(r)
                    || metadata_values.contains_key(r)
            });
            if all_mapped {
                let resolved = resolve_guards(expr).transform(
                    &|e: &Expr| -> Result<Option<Expr>, std::convert::Infallible> {
                        if let Expr::Column(col) = e {
                            if let Some(lit_val) = mapping.literals.get(&col.name) {
                                return Ok(Some(Expr::string(lit_val.clone())));
                            }
                            if let Some(meta_val) = metadata_values.get(&col.name) {
                                return Ok(Some(Expr::string(meta_val.clone())));
                            }
                        }
                        Ok(None)
                    },
                ).expect("literal inlining is infallible");
                for sem_ref in &sem_refs {
                    if let Some(phys) = mapping.physical.get(sem_ref) {
                        if scan_seen.insert(phys.clone()) {
                            scan_columns.push(phys.clone());
                        }
                        if !physical_regular.contains(sem_ref) {
                            extra_group_by.push((sem_ref.clone(), phys.clone()));
                        }
                    }
                }
                dim_sources.push((dim_name.clone(), DimSource::Computed(resolved)));
            } else {
                dim_sources.push((dim_name.clone(), DimSource::NullFill));
            }
        } else if let Some(phys) = mapping.physical.get(dim_name) {
            dim_sources.push((dim_name.clone(), DimSource::Physical(phys.clone())));
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys.clone());
            }
        } else {
            dim_sources.push((dim_name.clone(), DimSource::NullFill));
        }
    }

    // Also resolve metadata dimensions (reuse pre-computed values).
    let mut meta_lit_sources: Vec<(String, Expr)> = Vec::new();
    for (dim_name, _meta) in metadata_dims {
        meta_lit_sources.push((dim_name.clone(), Expr::string(metadata_values.get(dim_name).cloned().unwrap_or_default())));
    }

    // Lower measures that this branch covers.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    let mut lowered_measures: Vec<(String, Option<DecomposedMeasure>)> = Vec::new();
    for measure_name in all_measure_names {
        if let Some(measure) = iface.measures.get(*measure_name) {
            // Direct measure: check if this dataset was assigned it.
            let ds_has_it = assignment.measures.contains(&measure_name.to_string());
            if ds_has_it {
                let lowered = decomposer::decompose_measure(
                    &phys_resolver, measure_name, measure.agg, &measure.expr, &measure.filters, &measure.data_type,
                )?;
                for agg_m in &lowered.aggregates {
                    collect_column_refs(&agg_m.expr, &mut scan_columns, &mut scan_seen);
                }
                lowered_measures.push((measure_name.to_string(), Some(lowered)));
            } else {
                lowered_measures.push((measure_name.to_string(), None));
            }
        } else if let Some(metric) = iface.metrics.get(*measure_name) {
            // Metric: check if all transitive constituent measures are assigned to this dataset.
            let constituents = extract_metric_constituents(metric, iface);
            let ds_has_all = constituents.iter().all(|c| assignment.measures.contains(c));
            if ds_has_all {
                let lowered = decomposer::decompose_metric(measure_name, metric, iface, binding, 4)?;
                for agg_m in &lowered.aggregates {
                    collect_column_refs(&agg_m.expr, &mut scan_columns, &mut scan_seen);
                }
                lowered_measures.push((measure_name.to_string(), Some(lowered)));
            } else {
                lowered_measures.push((measure_name.to_string(), None));
            }
        } else {
            lowered_measures.push((measure_name.to_string(), None));
        }
    }

    // Build Scan (multi-source aware).
    let pb = ctx.plan_builder;
    let sem_types = build_semantic_type_map(iface, &mapping.physical);
    let scan = build_scan_node_binding(binding, &scan_columns, &sem_types, pb);

    // Build Aggregate node (physical dims + extra columns for computed dim refs).
    let mut group_by: Vec<Expr> = dim_sources
        .iter()
        .filter_map(|(name, src)| match src {
            DimSource::Physical(p) => {
                if needs_rollup && temporal_dim == Some(name.as_str()) {
                    Some(Expr::date_trunc(
                        request_grain.unwrap().into(),
                        Expr::column(p.clone()),
                    ))
                } else {
                    Some(Expr::column(p.clone()))
                }
            }
            _ => None,
        })
        .collect();
    for (_, phys) in &extra_group_by {
        group_by.push(Expr::column(phys.clone()));
    }

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .filter_map(|(_, lowered)| lowered.as_ref())
        .flat_map(|l| l.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = dim_sources
        .iter()
        .filter_map(|(semantic, src)| match src {
            DimSource::Physical(_) => Some(Field::new(semantic.clone(), iface.resolve_dim_type(semantic))),
            _ => None,
        })
        .collect();
    for (semantic, _) in &extra_group_by {
        agg_fields.push(Field::new(semantic.clone(), iface.resolve_dim_type(semantic)));
    }

    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        if let Some(l) = lowered {
            for (j, agg_m) in l.aggregates.iter().enumerate() {
                if j == 0 {
                    agg_fields.push(Field::new(semantic.clone(), iface.resolve_measure_type(semantic)));
                } else {
                    agg_fields.push(Field::new(format!("__agg_{}", agg_idx), agg_m.data_type.clone()));
                }
                agg_idx += 1;
            }
        }
    }
    let agg_schema = Schema::new(agg_fields);

    let agg = pb.build_aggregate(agg_schema, scan, group_by, aggregates);

    // Build Project — outputs unified schema, NULL-filling unmapped fields.
    let mut project_exprs: Vec<Expr> = Vec::new();

    // Dimensions: in request order.
    for dim_name in &_request.dimensions {
        if let Some((_, lit)) = meta_lit_sources.iter().find(|(n, _)| n == dim_name) {
            project_exprs.push(lit.clone());
        } else if let Some((_, src)) = dim_sources.iter().find(|(n, _)| n == dim_name) {
            match src {
                DimSource::Physical(_) => project_exprs.push(Expr::column(dim_name.clone())),
                DimSource::MetadataLiteral(lit) => project_exprs.push(lit.clone()),
                DimSource::Computed(expr) => project_exprs.push(expr.clone()),
                DimSource::NullFill => project_exprs.push(Expr::null()),
            }
        } else {
            project_exprs.push(Expr::null());
        }
    }

    // Measures: NULL-fill for unmapped.
    for (_, lowered) in &lowered_measures {
        project_exprs.push(
            lowered.as_ref().map_or(Expr::null(), |l| l.post_agg_expr.clone()),
        );
    }

    let project = pb.build_project(unified_schema.clone(), agg, project_exprs);

    Ok(project)
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
        // Root should be Project -> Aggregate -> Scan (no Union).
        match &fragment.root {
            PlanNode::Project(p) => {
                assert!(matches!(p.input.as_ref(), PlanNode::Aggregate(_)));
            }
            _ => panic!("Expected Project as root"),
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
        match &fragment.root {
            PlanNode::Project(proj) => {
                if let PlanNode::Aggregate(agg) = proj.input.as_ref() {
                    let has_date_trunc = agg.group_by.iter().any(|e| matches!(e, Expr::DateTrunc(_)));
                    assert!(has_date_trunc, "should have DATE_TRUNC in GROUP BY");
                }
            }
            _ => panic!("Expected Project as root"),
        }
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
        match &fragment.root {
            PlanNode::Project(p) => {
                assert!(
                    matches!(p.input.as_ref(), PlanNode::Aggregate(_)),
                    "single dataset should skip Union"
                );
            }
            _ => panic!("Expected Project as root"),
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

        // The scan layer should have a Union of scan nodes.
        let fragment = result.unwrap();
        fn has_union_scan(node: &PlanNode) -> bool {
            match node {
                PlanNode::Union(u) => u.inputs.iter().all(|n| matches!(n, PlanNode::Scan(_))),
                PlanNode::Aggregate(a) => has_union_scan(&a.input),
                PlanNode::Project(p) => has_union_scan(&p.input),
                _ => false,
            }
        }
        assert!(has_union_scan(&fragment.root), "should have intra-dataset UNION ALL of scans");
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
