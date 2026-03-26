//! GrainsetPlanner — kind planner for Grainset kinds.
//!
//! Resolves queries via grain-aware UNION ALL:
//! 1. Prune datasets (grain eligibility, zero-coverage)
//! 2. Group datasets by native temporal grain
//! 3. Assign measures/metrics to cheapest (coarsest) grain group
//! 4. Build per-dataset branches: Scan → DATE_TRUNC → Pre-aggregate → NULL-fill Project
//! 5. UNION ALL all branches → Re-aggregate → Final Project

use crate::error::PlannerError;
use crate::expr_lower;
use super::{
    extract_metadata_value_binding, grain_to_temporal, partition_dimensions_iface,
    resolve_native_grain_binding, KindPlanner, PlanFragment, PlannerContext,
};
use super::shared;
use super::shared::{build_scan_node_binding, infer_aggregation_iface};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, AggregateMeasure, Expr, Field, NodeMeta, PlanNode, ProjectNode,
    Schema, UnionNode,
};
use semstrait_manifest::{
    KindInterface, TemporalGrain,
};
use semstrait_manifest::acceleration::{DataKind, DatasetBinding};
use std::collections::{HashMap, HashSet};

/// Recursively collect all column references from an Expr tree.
pub fn collect_column_refs(expr: &Expr, columns: &mut Vec<String>, seen: &mut HashSet<String>) {
    match expr {
        Expr::Column(col) => {
            if seen.insert(col.name.clone()) {
                columns.push(col.name.clone());
            }
        }
        Expr::BinaryOp(bin) => {
            collect_column_refs(&bin.left, columns, seen);
            collect_column_refs(&bin.right, columns, seen);
        }
        Expr::Case(case) => {
            for wc in &case.when_then {
                collect_column_refs(&wc.condition, columns, seen);
                collect_column_refs(&wc.result, columns, seen);
            }
            if let Some(e) = &case.else_expr {
                collect_column_refs(e, columns, seen);
            }
        }
        Expr::FunctionCall(fc) => {
            for arg in &fc.args {
                collect_column_refs(arg, columns, seen);
            }
        }
        Expr::Aggregate(agg) => {
            collect_column_refs(&agg.expr, columns, seen);
        }
        Expr::Negate(u) | Expr::Not(u) | Expr::IsNull(u) | Expr::IsNotNull(u) => {
            collect_column_refs(&u.expr, columns, seen);
        }
        Expr::InList(il) => {
            collect_column_refs(&il.expr, columns, seen);
            for item in &il.list {
                collect_column_refs(item, columns, seen);
            }
        }
        Expr::Between(bt) => {
            collect_column_refs(&bt.expr, columns, seen);
            collect_column_refs(&bt.low, columns, seen);
            collect_column_refs(&bt.high, columns, seen);
        }
        Expr::Like(lk) => {
            collect_column_refs(&lk.expr, columns, seen);
            collect_column_refs(&lk.pattern, columns, seen);
        }
        Expr::Coalesce(co) => {
            for e in &co.exprs {
                collect_column_refs(e, columns, seen);
            }
        }
        Expr::NullIf(ni) => {
            collect_column_refs(&ni.expr, columns, seen);
            collect_column_refs(&ni.null_expr, columns, seen);
        }
        Expr::DateTrunc(dt) => {
            collect_column_refs(&dt.expr, columns, seen);
        }
        Expr::Guard(g) => {
            collect_column_refs(&g.condition, columns, seen);
            collect_column_refs(&g.expr, columns, seen);
        }
        Expr::Literal(_) | Expr::EntityRef(_) => {}
    }
}

/// Planner for Grainset kinds — grain-aware UNION ALL resolution.
pub struct GrainsetPlanner;

impl KindPlanner for GrainsetPlanner {
    fn supports(&self, data_kind: &DataKind) -> bool {
        matches!(data_kind, DataKind::Grainset(_))
    }

    fn resolve(
        &self,
        data_kind: &DataKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        let grainset = match data_kind {
            DataKind::Grainset(g) => g,
            _ => return Err(PlannerError::Internal("GrainsetPlanner received non-Grainset DataKind".into())),
        };
        let iface = &grainset.interface;
        let bindings = &grainset.bindings;

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
        let eligible = prune_datasets(iface, bindings, request, temporal_dim, request_grain)?;

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
        // dataset covering all requested measures/metrics, use the simpler plan.
        if assignments.len() == 1 {
            let a = &assignments[0];
            let covers_all = request.measures.iter().all(|m| {
                a.binding.column_mapping.contains_key(m)
                    || iface.metrics.contains_key(m)
            });
            if covers_all {
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
    iface: &KindInterface,
    bindings: &'a [DatasetBinding],
    request: &ResolvedQueryRequest,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<Vec<&'a DatasetBinding>, PlannerError> {
    let (_, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);

    let mut eligible: Vec<&DatasetBinding> = Vec::new();

    for binding in bindings {
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
                extract_metric_constituents(metric)
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
    iface: &KindInterface,
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
    iface: &KindInterface,
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
            let constituent_measures = extract_metric_constituents(metric);
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

/// Extract constituent measure names from a metric's expression.
///
/// Walks the pre-lowering expr tree, collecting both `Column` and `EntityRef`
/// leaf names. Uses `&str` borrows from the expr tree for dedup — the tree
/// outlives this call so no clones are needed for the seen-set.
pub fn extract_metric_constituents(metric: &semstrait_manifest::CompiledMetric) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_leaf_refs(&metric.expr, &mut names, &mut seen);
    names
}

/// Collect leaf entity/column names from an expression tree (borrow-based dedup).
///
/// Unlike `collect_column_refs` (which serves the post-lowering scan-column path
/// and only handles `Expr::Column`), this handles both `Column` and `EntityRef` —
/// the two forms that appear in pre-lowering metric expressions.
fn collect_leaf_refs<'a>(expr: &'a Expr, out: &mut Vec<String>, seen: &mut HashSet<&'a str>) {
    match expr {
        Expr::Column(col) => {
            if seen.insert(&col.name) {
                out.push(col.name.clone());
            }
        }
        Expr::EntityRef(er) => {
            if seen.insert(&er.name) {
                out.push(er.name.clone());
            }
        }
        Expr::BinaryOp(bin) => {
            collect_leaf_refs(&bin.left, out, seen);
            collect_leaf_refs(&bin.right, out, seen);
        }
        Expr::Case(case) => {
            for wc in &case.when_then {
                collect_leaf_refs(&wc.condition, out, seen);
                collect_leaf_refs(&wc.result, out, seen);
            }
            if let Some(e) = &case.else_expr {
                collect_leaf_refs(e, out, seen);
            }
        }
        _ => {}
    }
}

// ─────────────────── Step 3: Build Plan ──────────────────────

/// How a dimension is resolved in a UNION branch.
enum DimSource {
    Physical(String),
    MetadataLiteral(Expr),
    NullFill,
}

/// Build the unified output schema for the UNION plan.
fn build_unified_schema(request: &ResolvedQueryRequest, iface: &KindInterface) -> Schema {
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
    iface: &KindInterface,
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
        shared::build_binding_plan(iface, binding, request, ctx, true)
    }
}

/// Build a single-dataset plan with DATE_TRUNC grain rollup.
fn build_single_dataset_with_rollup(
    iface: &KindInterface,
    request: &ResolvedQueryRequest,
    binding: &DatasetBinding,
    _ctx: &PlannerContext<'_>,
    temporal_dim_name: &str,
    request_grain: TemporalGrain,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &binding.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);

    // Map dimensions, applying DATE_TRUNC for the temporal dimension.
    let mut dim_physical: Vec<(String, DimResolve)> = Vec::new();
    let mut metadata_literals: Vec<(String, Expr)> = Vec::new();

    for dim_name in &regular_dims {
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

    for (dim_name, meta) in &metadata_dims {
        let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
        metadata_literals.push((dim_name.clone(), Expr::string(value)));
    }

    // Lower measures.
    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = if let Some(agg) = measure.agg {
                expr_lower::lower_measure_declarative_physical(measure_name, agg, &measure.expr, &mapping.physical, &measure.filters)?
            } else {
                expr_lower::lower_measure_with_filters_physical(measure_name, &measure.expr, &mapping.physical, &measure.filters)?
            };
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if iface.metrics.contains_key(measure_name) {
            let metric = &iface.metrics[measure_name];
            let lowered = expr_lower::lower_metric_iface(measure_name, metric, iface, binding, 4)?;
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
    let scan = build_scan_node_binding(binding, &scan_columns);

    // Build Aggregate with DATE_TRUNC in GROUP BY.
    let group_by: Vec<Expr> = dim_physical
        .iter()
        .map(|(_, resolve)| match resolve {
            DimResolve::Column(phys) => Expr::column(phys.clone()),
            DimResolve::DateTrunc(phys, grain) => {
                Expr::date_trunc((*grain).into(), Expr::column(phys.clone()))
            }
        })
        .collect();

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = dim_physical
        .iter()
        .map(|(semantic, _)| Field::new(semantic.clone(), iface.resolve_dim_type(semantic)))
        .collect();
    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        for (j, _) in lowered.aggregates.iter().enumerate() {
            if j == 0 {
                agg_fields.push(Field::new(semantic.clone(), iface.resolve_measure_type(semantic)));
            } else {
                agg_fields.push(Field::new(format!("__agg_{}", agg_idx), DataType::Float64));
            }
            agg_idx += 1;
        }
    }
    let agg_schema = Schema::new(agg_fields);

    let agg = PlanNode::Aggregate(AggNode {
        meta: NodeMeta::new(agg_schema),
        input: Box::new(scan),
        group_by,
        aggregates,
    });

    // Build Project.
    let mut project_exprs: Vec<Expr> = Vec::new();
    let mut project_fields: Vec<Field> = Vec::new();

    for dim_name in &request.dimensions {
        if let Some((_, lit_expr)) = metadata_literals.iter().find(|(n, _)| n == dim_name) {
            project_exprs.push(lit_expr.clone());
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

    let project = PlanNode::Project(ProjectNode {
        meta: NodeMeta::new(project_schema.clone()),
        input: Box::new(agg),
        expressions: project_exprs,
    });

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
    iface: &KindInterface,
    request: &ResolvedQueryRequest,
    assignments: &[DatasetAssignment<'_>],
    _ctx: &PlannerContext<'_>,
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
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Validate type consistency across branches before UNION.
    shared::validate_union_types(&branches)?;

    // UNION ALL.
    let union_input = if branches.len() == 1 {
        branches.into_iter().next().unwrap()
    } else {
        PlanNode::Union(UnionNode {
            meta: NodeMeta::new(unified_schema.clone()),
            inputs: branches,
            distinct: false,
        })
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
        })
        .collect();

    let agg = PlanNode::Aggregate(AggNode {
        meta: NodeMeta::new(unified_schema.clone()),
        input: Box::new(union_input),
        group_by,
        aggregates,
    });

    Ok(PlanFragment {
        root: agg,
        output_schema: unified_schema,
        pending_filters: Vec::new(),
    })
}

/// Build a single UNION branch for one dataset assignment.
#[allow(clippy::too_many_arguments)]
fn build_union_branch(
    iface: &KindInterface,
    _request: &ResolvedQueryRequest,
    assignment: &DatasetAssignment<'_>,
    metadata_dims: &[(String, semstrait_manifest::MetadataDimension)],
    regular_dims: &[String],
    all_measure_names: &[&str],
    unified_schema: &Schema,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
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

    // Resolve regular dimensions.
    let mut dim_sources: Vec<(String, DimSource)> = Vec::new();
    for dim_name in regular_dims {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            dim_sources.push((dim_name.clone(), DimSource::MetadataLiteral(Expr::string(lit_val.clone()))));
        } else if let Some(phys) = mapping.physical.get(dim_name) {
            dim_sources.push((dim_name.clone(), DimSource::Physical(phys.clone())));
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys.clone());
            }
        } else {
            dim_sources.push((dim_name.clone(), DimSource::NullFill));
        }
    }

    // Also resolve metadata dimensions.
    let mut meta_lit_sources: Vec<(String, Expr)> = Vec::new();
    for (dim_name, meta) in metadata_dims {
        let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
        meta_lit_sources.push((dim_name.clone(), Expr::string(value)));
    }

    // Lower measures that this branch covers.
    let mut lowered_measures: Vec<(String, Option<expr_lower::LoweredMeasure>)> = Vec::new();
    for measure_name in all_measure_names {
        if let Some(measure) = iface.measures.get(*measure_name) {
            // Direct measure: check if this dataset was assigned it.
            let ds_has_it = assignment.measures.contains(&measure_name.to_string());
            if ds_has_it {
                let lowered = if let Some(agg) = measure.agg {
                    expr_lower::lower_measure_declarative_physical(
                        measure_name, agg, &measure.expr, &mapping.physical, &measure.filters,
                    )?
                } else {
                    expr_lower::lower_measure_with_filters_physical(
                        measure_name, &measure.expr, &mapping.physical, &measure.filters,
                    )?
                };
                for agg_m in &lowered.aggregates {
                    collect_column_refs(&agg_m.expr, &mut scan_columns, &mut scan_seen);
                }
                lowered_measures.push((measure_name.to_string(), Some(lowered)));
            } else {
                lowered_measures.push((measure_name.to_string(), None));
            }
        } else if let Some(metric) = iface.metrics.get(*measure_name) {
            // Metric: check if all constituent measures are assigned to this dataset.
            let constituents = extract_metric_constituents(metric);
            let ds_has_all = constituents.iter().all(|c| assignment.measures.contains(c));
            if ds_has_all {
                let lowered = expr_lower::lower_metric_iface(measure_name, metric, iface, binding, 4)?;
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
    let scan = build_scan_node_binding(binding, &scan_columns);

    // Build Aggregate node.
    let group_by: Vec<Expr> = dim_sources
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

    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        if let Some(l) = lowered {
            for (j, _) in l.aggregates.iter().enumerate() {
                if j == 0 {
                    agg_fields.push(Field::new(semantic.clone(), iface.resolve_measure_type(semantic)));
                } else {
                    agg_fields.push(Field::new(format!("__agg_{}", agg_idx), DataType::Float64));
                }
                agg_idx += 1;
            }
        }
    }
    let agg_schema = Schema::new(agg_fields);

    let agg = PlanNode::Aggregate(AggNode {
        meta: NodeMeta::new(agg_schema),
        input: Box::new(scan),
        group_by,
        aggregates,
    });

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

    let project = PlanNode::Project(ProjectNode {
        meta: NodeMeta::new(unified_schema.clone()),
        input: Box::new(agg),
        expressions: project_exprs,
    });

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use indexmap::IndexMap;
    use semstrait_ir::Aggregation;
    use semstrait_manifest::{
        CompiledDimension, CompiledMeasure, DimensionType,
    };
    use semstrait_manifest::acceleration::{
        CoverageIndex, DimensionIndex, GrainMap, GrainsetKind,
        ResolvedColumnMapping, TemporalMapping,
    };

    // ── Test helpers ─────────────────────────────────────────────────

    /// Build a DataKind::Grainset from dimensions, measures, and bindings.
    fn make_grainset(
        name: &str,
        dimensions: IndexMap<String, CompiledDimension>,
        measures: IndexMap<String, CompiledMeasure>,
        bindings: Vec<DatasetBinding>,
    ) -> DataKind {
        make_grainset_with_metrics(name, dimensions, measures, IndexMap::new(), bindings)
    }

    fn make_grainset_with_metrics(
        name: &str,
        dimensions: IndexMap<String, CompiledDimension>,
        measures: IndexMap<String, CompiledMeasure>,
        metrics: IndexMap<String, semstrait_manifest::CompiledMetric>,
        bindings: Vec<DatasetBinding>,
    ) -> DataKind {
        let temporal_dim = dimensions.iter().find_map(|(n, d)| {
            if matches!(d.dim_type, DimensionType::Temporal(_)) {
                Some(n.clone())
            } else {
                None
            }
        });
        let iface = KindInterface {
            name: name.to_string(),
            description: None,
            dimensions: dimensions.clone(),
            measures: measures.clone(),
            metrics,
            keys: None,
            filters: vec![],
            domain: None,
            temporal_dim: temporal_dim.clone(),
        };
        let coverage = CoverageIndex::build(&dimensions, &measures, &bindings);
        let dimension_index = DimensionIndex::build(&dimensions, &bindings);
        let grain_map = temporal_dim.as_deref().map(|td| GrainMap::build(td, &bindings));

        DataKind::Grainset(Box::new(GrainsetKind {
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
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        )
    }

    fn make_temporal_dim(name: &str, grains: Vec<TemporalGrain>) -> (String, CompiledDimension) {
        (
            name.to_string(),
            CompiledDimension {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Date32,
                dim_type: DimensionType::Temporal(semstrait_manifest::TemporalDimension {
                    grains,
                }),
            },
        )
    }

    fn make_sum_measure(name: &str, expr_ref: &str) -> (String, CompiledMeasure) {
        (
            name.to_string(),
            CompiledMeasure {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Float64,
                agg: Some(Aggregation::Sum),
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
                data_type: semstrait_core::DataType::Float64,
                agg: Some(Aggregation::CountDistinct),
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
            datasets: IndexMap::new(),
            kinds: IndexMap::new(),
            relationships: vec![],
            model_name: "test".to_string(),
            model_description: None,
            data_kinds: IndexMap::new(),
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Metadata(MetadataDimension {
                    path: Some(PathExtraction { token: 1 }),
                    partition: None,
                }),
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let fragment = planner.resolve(&data_kind, &request, &ctx).unwrap();

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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
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
        let profile = semstrait_core::ConsumerProfile::default();
        let session = std::collections::HashMap::new();
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &profile,
            catalog: None,
            session: &session,
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&data_kind, &request, &ctx);
        assert!(result.is_err());
    }
}
