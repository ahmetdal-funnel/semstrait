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
    extract_metadata_value, find_temporal_dimension, grain_to_temporal, partition_dimensions,
    resolve_column_name, resolve_native_grain, KindPlanner, PlanFragment, PlannerContext,
};
use super::shared::{build_scan_node, infer_aggregation};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, AggregateMeasure, Aggregation, Expr, Field, NodeMeta, PlanNode, ProjectNode,
    Schema, UnionNode,
};
use semstrait_manifest::{
    ColumnMappingValue, CompiledKind, CompiledKindDataset, CompiledKindType, LiteralValue,
    TemporalGrain,
};
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
    fn supports(&self, kind_type: &CompiledKindType) -> bool {
        matches!(kind_type, CompiledKindType::Grainset)
    }

    fn resolve(
        &self,
        kind: &CompiledKind,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        if kind.datasets.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: kind.name.clone(),
                reason: "grainset kind has no datasets".to_string(),
            });
        }

        // Determine requested temporal grain.
        let temporal_dim = find_temporal_dimension(kind);
        let request_grain = request.grain.map(grain_to_temporal);

        // Step 1: Prune datasets.
        let eligible = prune_datasets(kind, request, temporal_dim, request_grain)?;

        if eligible.is_empty() {
            return Err(PlannerError::NoCoveringDataset {
                kind: kind.name.clone(),
                reason: "no datasets remain after grain/coverage pruning".to_string(),
            });
        }

        // Separate requested names into actual measures vs metrics.
        let (measure_names, metric_names) = classify_requested_measures(kind, &request.measures);

        // Step 2: Group by grain and assign measures/metrics.
        let assignments = assign_to_grain_groups(
            kind,
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
                a.dataset.extras.column_mapping.contains_key(m)
                    || kind.metrics.contains_key(m)
            });
            if covers_all {
                return build_single_dataset_plan(kind, request, a.dataset, ctx, temporal_dim, request_grain);
            }
        }

        // Step 3: Build UNION ALL plan.
        build_union_plan(kind, request, &assignments, ctx, temporal_dim, request_grain)
    }
}

// ─────────────────────── Step 1: Prune ───────────────────────

/// Prune datasets by grain eligibility and zero-coverage.
fn prune_datasets<'a>(
    kind: &'a CompiledKind,
    request: &ResolvedQueryRequest,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<Vec<&'a CompiledKindDataset>, PlannerError> {
    let (_, regular_dims) = partition_dimensions(&request.dimensions, kind);

    let mut eligible: Vec<&CompiledKindDataset> = Vec::new();

    for ds in &kind.datasets {
        // 1c. Grain eligibility: exclude datasets whose native grain is coarser than requested.
        if let (Some(rg), Some(td_name)) = (request_grain, temporal_dim) {
            if let Some(native) = resolve_native_grain(ds, td_name, kind) {
                if native.coarseness() > rg.coarseness() {
                    continue; // Can't disaggregate.
                }
            }
        }

        // 1d. Zero-coverage: exclude datasets that cover no requested semantics.
        let mapping = &ds.extras.column_mapping;
        let covers_any = regular_dims.iter().any(|d| mapping.contains_key(d))
            || request.measures.iter().any(|m| mapping.contains_key(m));

        if covers_any {
            eligible.push(ds);
        }
    }

    Ok(eligible)
}

// ─────────────────────── Step 2: Assign ──────────────────────

/// A dataset with its assigned measures for a specific grain group.
struct DatasetAssignment<'a> {
    dataset: &'a CompiledKindDataset,
    measures: Vec<String>,
    native_grain: Option<TemporalGrain>,
}

/// Separate requested measure names into actual measures vs metric names.
fn classify_requested_measures(
    kind: &CompiledKind,
    requested: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut measures = Vec::new();
    let mut metrics = Vec::new();
    for name in requested {
        if kind.metrics.contains_key(name) {
            metrics.push(name.clone());
        } else {
            measures.push(name.clone());
        }
    }
    (measures, metrics)
}

/// Group eligible datasets by native grain, assign measures/metrics to cheapest group.
fn assign_to_grain_groups<'a>(
    kind: &CompiledKind,
    eligible: &[&'a CompiledKindDataset],
    temporal_dim: Option<&str>,
    _request_grain: Option<TemporalGrain>,
    measure_names: &[String],
    metric_names: &[String],
) -> Result<Vec<DatasetAssignment<'a>>, PlannerError> {
    // Group datasets by native grain (None = no temporal dimension).
    let mut grain_groups: HashMap<Option<TemporalGrain>, Vec<&'a CompiledKindDataset>> =
        HashMap::new();

    for ds in eligible {
        let native = temporal_dim.and_then(|td| resolve_native_grain(ds, td, kind));
        grain_groups.entry(native).or_default().push(ds);
    }

    // Sort grain groups by coarseness (coarsest first = cheapest).
    let mut sorted_groups: Vec<(Option<TemporalGrain>, Vec<&'a CompiledKindDataset>)> =
        grain_groups.into_iter().collect();
    sorted_groups.sort_by_key(|(g, _)| std::cmp::Reverse(g.map_or(0, |g| g.coarseness())));

    // For each measure, find the cheapest grain group that has it.
    let mut measure_to_group: HashMap<String, Option<TemporalGrain>> = HashMap::new();

    for measure_name in measure_names {
        let mut assigned = false;
        for (grain, datasets) in &sorted_groups {
            if datasets.iter().any(|ds| ds.extras.column_mapping.contains_key(measure_name)) {
                measure_to_group.insert(measure_name.clone(), *grain);
                assigned = true;
                break;
            }
        }
        if !assigned {
            return Err(PlannerError::NoCoveringDataset {
                kind: kind.name.clone(),
                reason: format!(
                    "measure '{}' cannot be provided — no eligible dataset maps it",
                    measure_name
                ),
            });
        }
    }

    // For metrics, find the cheapest group where all constituent measures are available.
    for metric_name in metric_names {
        if let Some(metric) = kind.metrics.get(metric_name) {
            let constituent_measures = extract_metric_constituents(metric);
            let mut assigned = false;
            for (grain, datasets) in &sorted_groups {
                let group_covers_all = constituent_measures.iter().all(|cm| {
                    datasets.iter().any(|ds| ds.extras.column_mapping.contains_key(cm))
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
                    kind: kind.name.clone(),
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

    for (grain, datasets) in &sorted_groups {
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
        for ds in datasets {
            let ds_measures: Vec<String> = group_measures
                .iter()
                .filter(|m| ds.extras.column_mapping.contains_key(m.as_str()))
                .cloned()
                .collect();

            if !ds_measures.is_empty() {
                for m in &ds_measures {
                    assigned_measures.insert(m.clone());
                }
                dataset_assignments.push(DatasetAssignment {
                    dataset: ds,
                    measures: ds_measures,
                    native_grain: *grain,
                });
            }
        }
    }

    Ok(dataset_assignments)
}

/// Extract constituent measure names from a metric's expression.
fn extract_metric_constituents(metric: &semstrait_manifest::CompiledMetric) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_column_refs(&metric.expr, &mut names, &mut seen);
    names
}

// ─────────────────── Step 3: Build Plan ──────────────────────

/// How a dimension is resolved in a UNION branch.
enum DimSource {
    Physical(String),
    MetadataLiteral(Expr),
    NullFill,
}

/// Build the unified output schema for the UNION plan.
fn build_unified_schema(request: &ResolvedQueryRequest) -> Schema {
    let fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), DataType::Utf8))
        .chain(
            request
                .measures
                .iter()
                .map(|name| Field::new(name.clone(), DataType::Float64)),
        )
        .collect();
    Schema::new(fields)
}

/// Build a single-dataset plan with optional grain rollup.
fn build_single_dataset_plan(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    dataset: &CompiledKindDataset,
    ctx: &PlannerContext<'_>,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<PlanFragment, PlannerError> {
    // Determine if grain rollup is needed.
    let needs_rollup = if let (Some(rg), Some(td)) = (request_grain, temporal_dim) {
        if let Some(native) = resolve_native_grain(dataset, td, kind) {
            native.coarseness() < rg.coarseness()
        } else {
            false
        }
    } else {
        false
    };

    // Use the shared single-dataset builder, but with grain rollup if needed.
    if needs_rollup {
        build_single_dataset_with_rollup(kind, request, dataset, ctx, temporal_dim.unwrap(), request_grain.unwrap())
    } else {
        super::shared::build_dataset_plan(kind, request, dataset, ctx, true)
    }
}

/// Build a single-dataset plan with DATE_TRUNC grain rollup.
fn build_single_dataset_with_rollup(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    dataset: &CompiledKindDataset,
    _ctx: &PlannerContext<'_>,
    temporal_dim_name: &str,
    request_grain: TemporalGrain,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &dataset.extras.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    let (metadata_dims, regular_dims) = partition_dimensions(&request.dimensions, kind);

    // Map dimensions, applying DATE_TRUNC for the temporal dimension.
    let mut dim_physical: Vec<(String, DimResolve)> = Vec::new();
    let mut metadata_literals: Vec<(String, Expr)> = Vec::new();

    for dim_name in &regular_dims {
        let mv = mapping.get(dim_name).ok_or_else(|| PlannerError::DimensionNotFound {
            kind: kind.name.clone(),
            dimension: dim_name.clone(),
        })?;
        match mv {
            ColumnMappingValue::Literal(lit) => {
                let expr = match lit {
                    LiteralValue::String(s) => Expr::string(s.clone()),
                };
                metadata_literals.push((dim_name.clone(), expr));
            }
            _ => {
                let phys = resolve_column_name(mv).to_string();
                if dim_name == temporal_dim_name {
                    dim_physical.push((dim_name.clone(), DimResolve::DateTrunc(phys.clone(), request_grain)));
                } else {
                    dim_physical.push((dim_name.clone(), DimResolve::Column(phys.clone())));
                }
                if scan_seen.insert(phys.clone()) {
                    scan_columns.push(phys);
                }
            }
        }
    }

    for (dim_name, meta) in &metadata_dims {
        let value = extract_metadata_value(meta, dataset).unwrap_or_default();
        metadata_literals.push((dim_name.clone(), Expr::string(value)));
    }

    // Lower measures.
    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            let lowered = if let Some(agg) = measure.agg {
                expr_lower::lower_measure_declarative(measure_name, agg, &measure.expr, mapping, &measure.filters)?
            } else {
                expr_lower::lower_measure_with_filters(measure_name, &measure.expr, mapping, &measure.filters)?
            };
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if kind.metrics.contains_key(measure_name) {
            let physical = mapping
                .get(measure_name)
                .map(resolve_column_name)
                .unwrap_or(measure_name.as_str());
            let phys = physical.to_string();
            let lowered = expr_lower::LoweredMeasure {
                aggregates: vec![AggregateMeasure {
                    function: Aggregation::Sum,
                    expr: Expr::column(phys.clone()),
                    distinct: false,
                }],
                post_agg_expr: Expr::column(measure_name.clone()),
            };
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: kind.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Build Scan (multi-source aware).
    let scan = build_scan_node(dataset, &scan_columns);

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
        .map(|(semantic, _)| Field::new(semantic.clone(), DataType::Utf8))
        .collect();
    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        for (j, _) in lowered.aggregates.iter().enumerate() {
            if j == 0 {
                agg_fields.push(Field::new(semantic.clone(), DataType::Float64));
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
        project_fields.push(Field::new(dim_name.clone(), DataType::Utf8));
    }
    for (_, lowered) in &lowered_measures {
        project_exprs.push(lowered.post_agg_expr.clone());
    }
    project_fields.extend(
        lowered_measures
            .iter()
            .map(|(name, _)| Field::new(name.clone(), DataType::Float64)),
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
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    assignments: &[DatasetAssignment<'_>],
    _ctx: &PlannerContext<'_>,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<PlanFragment, PlannerError> {
    let unified_schema = build_unified_schema(request);
    let (metadata_dims, regular_dims) = partition_dimensions(&request.dimensions, kind);

    // All requested measure/metric names for the unified schema.
    let all_measure_names: Vec<&str> = request.measures.iter().map(|s| s.as_str()).collect();

    // Build one branch per dataset assignment.
    let branches: Vec<PlanNode> = assignments
        .iter()
        .map(|a| {
            build_union_branch(
                kind,
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
            function: infer_aggregation(kind, name),
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
    kind: &CompiledKind,
    _request: &ResolvedQueryRequest,
    assignment: &DatasetAssignment<'_>,
    metadata_dims: &[(String, semstrait_manifest::MetadataDimension)],
    regular_dims: &[String],
    all_measure_names: &[&str],
    unified_schema: &Schema,
    temporal_dim: Option<&str>,
    request_grain: Option<TemporalGrain>,
) -> Result<PlanNode, PlannerError> {
    let dataset = assignment.dataset;
    let mapping = &dataset.extras.column_mapping;

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
        if let Some(mv) = mapping.get(dim_name) {
            match mv {
                ColumnMappingValue::Literal(lit) => {
                    let expr = match lit {
                        LiteralValue::String(s) => Expr::string(s.clone()),
                    };
                    dim_sources.push((dim_name.clone(), DimSource::MetadataLiteral(expr)));
                }
                _ => {
                    let phys = resolve_column_name(mv).to_string();
                    dim_sources.push((dim_name.clone(), DimSource::Physical(phys.clone())));
                    if scan_seen.insert(phys.clone()) {
                        scan_columns.push(phys);
                    }
                }
            }
        } else {
            dim_sources.push((dim_name.clone(), DimSource::NullFill));
        }
    }

    // Also resolve metadata dimensions.
    let mut meta_lit_sources: Vec<(String, Expr)> = Vec::new();
    for (dim_name, meta) in metadata_dims {
        let value = extract_metadata_value(meta, dataset).unwrap_or_default();
        meta_lit_sources.push((dim_name.clone(), Expr::string(value)));
    }

    // Lower measures that this branch covers.
    let mut lowered_measures: Vec<(String, Option<expr_lower::LoweredMeasure>)> = Vec::new();
    for measure_name in all_measure_names {
        let ds_has_it = assignment.measures.contains(&measure_name.to_string());
        if ds_has_it {
            if let Some(measure) = kind.measures.get(*measure_name) {
                let lowered = if let Some(agg) = measure.agg {
                    expr_lower::lower_measure_declarative(
                        measure_name, agg, &measure.expr, mapping, &measure.filters,
                    )?
                } else {
                    expr_lower::lower_measure_with_filters(
                        measure_name, &measure.expr, mapping, &measure.filters,
                    )?
                };
                for agg_m in &lowered.aggregates {
                    collect_column_refs(&agg_m.expr, &mut scan_columns, &mut scan_seen);
                }
                lowered_measures.push((measure_name.to_string(), Some(lowered)));
            } else if kind.metrics.contains_key(*measure_name) {
                // Metric: SUM pass-through on constituent column.
                let physical = mapping
                    .get(*measure_name)
                    .map(resolve_column_name)
                    .unwrap_or(measure_name);
                let phys = physical.to_string();
                let lowered = expr_lower::LoweredMeasure {
                    aggregates: vec![AggregateMeasure {
                        function: Aggregation::Sum,
                        expr: Expr::column(phys.clone()),
                        distinct: false,
                    }],
                    post_agg_expr: Expr::column(measure_name.to_string()),
                };
                if scan_seen.insert(phys.clone()) {
                    scan_columns.push(phys);
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
    let scan = build_scan_node(dataset, &scan_columns);

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
            DimSource::Physical(_) => Some(Field::new(semantic.clone(), DataType::Utf8)),
            _ => None,
        })
        .collect();

    let mut agg_idx = 0;
    for (semantic, lowered) in &lowered_measures {
        if let Some(l) = lowered {
            for (j, _) in l.aggregates.iter().enumerate() {
                if j == 0 {
                    agg_fields.push(Field::new(semantic.clone(), DataType::Float64));
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
        ColumnMappingValue, CompiledDimension, CompiledKind, CompiledKindDataset,
        CompiledManifest, CompiledMeasure, DimensionType, KindDatasetExtras,
        TemporalDimension,
    };

    // ── Single-dataset tests ─────────────────────────────────────────

    #[test]
    fn test_single_dataset_basic() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
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
        let manifest = make_metadata_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "source_info"], vec!["revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
        assert!(result.is_ok(), "metadata dim should not block: {:?}", result.err());
    }

    // ── Multi-dataset UNION ALL tests ────────────────────────────────

    #[test]
    fn test_multi_dataset_union_all() {
        let manifest = make_multi_dataset_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["cost", "revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
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
        let manifest = make_multi_dataset_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["cost", "revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let fragment = planner.resolve(kind, &request, &ctx).unwrap();

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
        let manifest = make_temporal_manifest();
        let kind = manifest.get_kind("orders").unwrap();

        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.grain = Some(semstrait_core::Grain::Month);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
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
        let manifest = make_multi_grain_manifest();
        let kind = manifest.get_kind("orders").unwrap();

        // Request day grain — should exclude monthly dataset.
        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.grain = Some(semstrait_core::Grain::Day);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
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
        let manifest = make_multi_grain_manifest();
        let kind = manifest.get_kind("orders").unwrap();

        // Request month grain, revenue — monthly dataset is cheaper.
        let mut request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        request.grain = Some(semstrait_core::Grain::Month);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multi_source_dataset() {
        let manifest = make_multi_source_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
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
        let kind = CompiledKind {
            name: "empty".to_string(),
            description: None,
            dimensions: IndexMap::new(),
            measures: IndexMap::new(),
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Grainset,
            datasets: vec![],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let request = make_test_request("empty", vec![], vec![]);
        let manifest = CompiledManifest {
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
        };
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let planner = GrainsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_err());
    }

    // ── Test helpers ─────────────────────────────────────────────────

    fn make_metadata_manifest() -> CompiledManifest {
        use semstrait_manifest::{MetadataDimension, PathExtraction};

        let mut dimensions = IndexMap::new();
        dimensions.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );
        dimensions.insert(
            "source_info".to_string(),
            CompiledDimension {
                name: "source_info".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Metadata(MetadataDimension {
                    path: Some(PathExtraction { token: 1 }),
                    partition: None,
                }),
            },
        );

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                agg: Some(Aggregation::Sum),
                expr: semstrait_core::Expr::entity_ref("amount"),
                expr_source: "amount".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        let mut column_mapping = std::collections::HashMap::new();
        column_mapping.insert("date".to_string(), ColumnMappingValue::Simple("order_date".to_string()));
        column_mapping.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));

        let dataset = CompiledKindDataset {
            name: "orders_daily".to_string(),
            extras: KindDatasetExtras {
                column_mapping: column_mapping.into(),
                temporal: None,
                storage: None,
                catalog: None,
            },
            resolved_sources: vec![semstrait_manifest::ResolvedSource::path("bucket/shopify/data.parquet")],
        };

        let kind = CompiledKind {
            name: "orders".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Grainset,
            datasets: vec![dataset],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let mut kinds = IndexMap::new();
        kinds.insert("orders".to_string(), kind);

        CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_metadata".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test_metadata_model".to_string(),
            model_description: None,
            data_kinds: IndexMap::new(),
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            catalog_snapshot: None,
        }
    }

    /// Create a manifest with a temporal dimension for grain rollup testing.
    fn make_temporal_manifest() -> CompiledManifest {
        let mut dimensions = IndexMap::new();
        dimensions.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: "date".to_string(),
                dim_type: DimensionType::Temporal(TemporalDimension {
                    grains: vec![TemporalGrain::Day, TemporalGrain::Week, TemporalGrain::Month],
                }),
            },
        );
        dimensions.insert(
            "region".to_string(),
            CompiledDimension {
                name: "region".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                agg: Some(Aggregation::Sum),
                expr: semstrait_core::Expr::entity_ref("amount"),
                expr_source: "amount".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        let mut mapping = std::collections::HashMap::new();
        mapping.insert(
            "date".to_string(),
            ColumnMappingValue::WithGrain {
                column: "order_date".to_string(),
                grain: Some(TemporalGrain::Day),
            },
        );
        mapping.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
        mapping.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));

        let dataset = CompiledKindDataset {
            name: "orders_daily".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping.into(),
                temporal: None,
                storage: None,
                catalog: None,
            },
            resolved_sources: vec![],
        };

        let kind = CompiledKind {
            name: "orders".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Grainset,
            datasets: vec![dataset],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let mut kinds = IndexMap::new();
        kinds.insert("orders".to_string(), kind);

        CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_temporal".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test_temporal_model".to_string(),
            model_description: None,
            data_kinds: IndexMap::new(),
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            catalog_snapshot: None,
        }
    }

    /// Create a manifest with two datasets at different temporal grains.
    fn make_multi_grain_manifest() -> CompiledManifest {
        let mut dimensions = IndexMap::new();
        dimensions.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: "date".to_string(),
                dim_type: DimensionType::Temporal(TemporalDimension {
                    grains: vec![TemporalGrain::Day, TemporalGrain::Month],
                }),
            },
        );
        dimensions.insert(
            "region".to_string(),
            CompiledDimension {
                name: "region".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                agg: Some(Aggregation::Sum),
                expr: semstrait_core::Expr::entity_ref("amount"),
                expr_source: "amount".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );
        measures.insert(
            "unique_customers".to_string(),
            CompiledMeasure {
                name: "unique_customers".to_string(),
                description: None,
                data_type: "float64".to_string(),
                agg: Some(Aggregation::CountDistinct),
                expr: semstrait_core::Expr::entity_ref("customer_id"),
                expr_source: "customer_id".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        // Daily dataset: maps date(day), region, revenue, unique_customers.
        let mut mapping1 = std::collections::HashMap::new();
        mapping1.insert(
            "date".to_string(),
            ColumnMappingValue::WithGrain {
                column: "order_date".to_string(),
                grain: Some(TemporalGrain::Day),
            },
        );
        mapping1.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
        mapping1.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));
        mapping1.insert("unique_customers".to_string(), ColumnMappingValue::Simple("customer_id".to_string()));

        let ds1 = CompiledKindDataset {
            name: "orders_daily".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping1.into(),
                temporal: None,
                storage: None,
                catalog: None,
            },
            resolved_sources: vec![],
        };

        // Monthly dataset: maps date(month), region, revenue (no unique_customers).
        let mut mapping2 = std::collections::HashMap::new();
        mapping2.insert(
            "date".to_string(),
            ColumnMappingValue::WithGrain {
                column: "report_month".to_string(),
                grain: Some(TemporalGrain::Month),
            },
        );
        mapping2.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
        mapping2.insert("revenue".to_string(), ColumnMappingValue::Simple("monthly_revenue".to_string()));

        let ds2 = CompiledKindDataset {
            name: "orders_monthly".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping2.into(),
                temporal: None,
                storage: None,
                catalog: None,
            },
            resolved_sources: vec![],
        };

        let kind = CompiledKind {
            name: "orders".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Grainset,
            datasets: vec![ds1, ds2],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let mut kinds = IndexMap::new();
        kinds.insert("orders".to_string(), kind);

        CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_multi_grain".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test_multi_grain_model".to_string(),
            model_description: None,
            data_kinds: IndexMap::new(),
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            catalog_snapshot: None,
        }
    }

    /// Create a manifest with a dataset that has multiple resolved_sources.
    fn make_multi_source_manifest() -> CompiledManifest {
        let mut dimensions = IndexMap::new();
        dimensions.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                agg: Some(Aggregation::Sum),
                expr: semstrait_core::Expr::entity_ref("amount"),
                expr_source: "amount".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        let mut mapping = std::collections::HashMap::new();
        mapping.insert("date".to_string(), ColumnMappingValue::Simple("order_date".to_string()));
        mapping.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));

        let dataset = CompiledKindDataset {
            name: "orders_daily".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping.into(),
                temporal: None,
                storage: None,
                catalog: None,
            },
            resolved_sources: vec![
                semstrait_manifest::ResolvedSource::path("bucket/account_001/orders.parquet"),
                semstrait_manifest::ResolvedSource::path("bucket/account_002/orders.parquet"),
                semstrait_manifest::ResolvedSource::path("bucket/account_003/orders.parquet"),
            ],
        };

        let kind = CompiledKind {
            name: "orders".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Grainset,
            datasets: vec![dataset],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let mut kinds = IndexMap::new();
        kinds.insert("orders".to_string(), kind);

        CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_multi_source".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test_multi_source_model".to_string(),
            model_description: None,
            data_kinds: IndexMap::new(),
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            catalog_snapshot: None,
        }
    }
}
