//! Shared plan-building utilities used by grainset, joinset, and unionset planners.

use crate::error::PlannerError;
use crate::decomposer::{self, DecomposedMeasure};
use crate::resolver::{ExprResolver as _, PhysicalResolver};
use super::{extract_metadata_value_binding, extract_metadata_value_source, partition_dimensions_iface, resolve_guards, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    Aggregation, AggregateMeasure, Expr, Field, PlanBuilder, PlanNode,
    Schema,
};
use indexmap::IndexMap;
use semstrait_manifest::{DatasetBinding, CompiledDatasetKind, CompiledInterface, MetadataDimension, ResolvedSource, TemporalGrain};
use std::collections::{HashMap, HashSet};

use super::collect_column_refs;

/// Resolve scan column type from a DatasetBinding's catalog schema.
///
/// Priority: catalog schema (physical truth) → semantic type fallback → DataType::String.
/// The `semantic_types` map provides physical_col → DataType from the kind interface,
/// used when catalog schema is unavailable (e.g., local files without a catalog provider).
pub(crate) fn resolve_scan_type_binding(
    physical_col: &str,
    binding: &DatasetBinding,
    semantic_types: &HashMap<String, DataType>,
) -> DataType {
    // 1. Catalog schema takes priority (physical truth from the source).
    if let Some(schema) = binding
        .resolved_sources
        .first()
        .and_then(|s| s.schema.as_ref())
    {
        if let Some(col) = schema.iter().find(|c| c.name == physical_col) {
            return col.data_type.clone();
        }
    }
    // 2. Semantic type from kind interface (declared in model YAML).
    if let Some(dt) = semantic_types.get(physical_col) {
        return dt.clone();
    }
    // 3. Last resort fallback.
    DataType::String
}

/// Build a map from physical column name → semantic DataType.
///
/// Inverts the column_mapping (semantic → physical) and resolves types
/// from the CompiledInterface. Used as fallback when catalog schema is unavailable.
pub(crate) fn build_semantic_type_map(
    iface: &CompiledInterface,
    physical_mapping: &IndexMap<String, String>,
) -> HashMap<String, DataType> {
    let mut map = HashMap::new();
    for (semantic_name, physical_name) in physical_mapping {
        if let Some(d) = iface.dimensions.get(semantic_name) {
            map.insert(physical_name.clone(), d.data_type.clone());
        } else if let Some(m) = iface.measures.get(semantic_name) {
            map.insert(physical_name.clone(), m.data_type.clone());
        } else if let Some(m) = iface.metrics.get(semantic_name) {
            map.insert(physical_name.clone(), m.data_type.clone());
        }
        // Join keys and other non-semantic columns are skipped —
        // their types are resolved from the join condition or scan fallback.
    }
    map
}

/// Build a scan node for a DatasetBinding (multi-source aware).
pub(crate) fn build_scan_node_binding(
    binding: &DatasetBinding,
    scan_columns: &[String],
    semantic_types: &HashMap<String, DataType>,
    pb: &dyn PlanBuilder,
) -> PlanNode {
    let scan_schema = Schema::new(
        scan_columns
            .iter()
            .map(|c| Field::new(c.clone(), resolve_scan_type_binding(c, binding, semantic_types)))
            .collect(),
    );

    if binding.resolved_sources.len() <= 1 {
        let first_source = binding.resolved_sources.first();
        let table_name = first_source
            .and_then(|s| s.table_fqn.as_deref())
            .or_else(|| first_source.map(|s| s.reference.as_str()))
            .unwrap_or(&binding.dataset_name);
        pb.build_scan(
            scan_schema,
            table_name.to_string(),
            first_source.and_then(|s| s.location.clone()),
            first_source.and_then(|s| s.format),
            scan_columns.to_vec(),
        )
    } else {
        let inputs: Vec<PlanNode> = binding.resolved_sources
            .iter()
            .map(|source| {
                let table_name = source.table_fqn.as_deref()
                    .unwrap_or(&source.reference);
                pb.build_scan(
                    scan_schema.clone(),
                    table_name.to_string(),
                    source.location.clone(),
                    source.format,
                    scan_columns.to_vec(),
                )
            })
            .collect();
        pb.build_union(scan_schema, inputs, false)
    }
}

/// Infer re-aggregation function from a CompiledInterface measure.
///
/// Re-aggregation preserves MIN/MAX (idempotent); everything else re-aggregates as SUM
/// (partial sums, partial counts). This is correct for fully-additive measures.
///
/// For non-additive measures (AVG, COUNT_DISTINCT), re-aggregation is lossy.
/// A warning is emitted; future versions will error or restructure the plan.
pub(crate) fn infer_aggregation_iface(iface: &CompiledInterface, measure_name: &str) -> Aggregation {
    if let Some(measure) = iface.measures.get(measure_name) {
        // Check additivity — warn on non-additive re-aggregation.
        if let Some(ref additivity) = measure.additivity {
            use semstrait_manifest::AdditivityType;
            match additivity {
                AdditivityType::Non => {
                    tracing::warn!(
                        "re-aggregating non-additive measure '{}' (agg: {:?}) — result may be lossy",
                        measure_name, measure.agg,
                    );
                }
                AdditivityType::Semi(_) => {
                    tracing::warn!(
                        "re-aggregating semi-additive measure '{}' — resolution strategy not yet applied",
                        measure_name,
                    );
                }
                AdditivityType::Full => {}
            }
        }
        return match measure.agg {
            Aggregation::Min => Aggregation::Min,
            Aggregation::Max => Aggregation::Max,
            _ => Aggregation::Sum,
        };
    }
    Aggregation::Sum
}

/// Build layered plan for a CompiledDatasetKind (single-dataset fast path).
///
/// Layered architecture: Scan → Rename → Expression → Aggregate → Project.
/// Uses `CompiledInterface` for type resolution and `DatasetBinding` for column mapping.
pub(crate) fn build_dataset_kind_plan(
    dk: &CompiledDatasetKind,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    let iface = &dk.interface;
    let binding = &dk.binding;
    build_layered_plan(iface, binding, request, ctx, true, None)
}

/// Build layered plan for a single DatasetBinding.
///
/// This is the per-binding plan builder used by ALL kind planners (grainset,
/// unionset, joinset). Layered architecture: Scan → Rename → Expression → Aggregate → Project.
///
/// `temporal_rollup`: if `Some((dim_name, grain))`, applies DATE_TRUNC to the
/// temporal dimension in the GROUP BY for grain rollup.
pub(crate) fn build_binding_plan(
    iface: &CompiledInterface,
    binding: &DatasetBinding,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
    handle_metrics: bool,
    temporal_rollup: Option<(&str, TemporalGrain)>,
) -> Result<PlanFragment, PlannerError> {
    build_layered_plan(iface, binding, request, ctx, handle_metrics, temporal_rollup)
}

/// Core layered plan builder: Scan → Rename → Expression → Aggregate → Project.
///
/// After rename, ALL names are semantic. Aggregate GROUP BY and measure expressions
/// reference semantic column names from the rename project output.
///
/// For multi-source bindings (multiple `ResolvedSource`s), builds per-source
/// Scan→Aggregate plans with correct per-source metadata values, UNION ALLs the
/// results, re-aggregates, then applies the final projection.
///
/// `temporal_rollup`: optional `(dim_name, grain)` for DATE_TRUNC in GROUP BY.
fn build_layered_plan(
    iface: &CompiledInterface,
    binding: &DatasetBinding,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
    handle_metrics: bool,
    temporal_rollup: Option<(&str, TemporalGrain)>,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &binding.column_mapping;
    let pb = ctx.plan_builder;

    // ── Partition dimensions ────────────────────────────────────────
    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_dims, computed_dims) = super::split_computed_dims(&regular_dims, iface);

    // ── Collect physical scan columns (for scan node) ──────────────
    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Physical dimension columns.
    let mut dim_physical: Vec<(String, String)> = Vec::new(); // (semantic, physical)
    for dim_name in &physical_dims {
        if mapping.literals.contains_key(dim_name) {
            continue;
        } else if let Some(phys) = mapping.physical.get(dim_name) {
            dim_physical.push((dim_name.clone(), phys.clone()));
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

    // Computed dimension dependency columns (physical columns referenced by computed expressions).
    // Filter out phantom refs: PhysicalResolver passes through unmapped names (metadata/literal dims)
    // which are not physical columns and must not appear in scan_columns.
    {
        let physical_values: HashSet<&str> =
            mapping.physical.values().map(|v| v.as_str()).collect();
        for (_, expr) in &computed_dims {
            let lowered = PhysicalResolver::new(&mapping.physical).resolve_expr(expr)?;
            let mut temp = Vec::new();
            let mut temp_seen = HashSet::new();
            collect_column_refs(&lowered, &mut temp, &mut temp_seen);
            for col in temp {
                if physical_values.contains(col.as_str()) && scan_seen.insert(col.clone()) {
                    scan_columns.push(col);
                }
            }
        }
    }

    // Measure columns (physical) — decompose with PhysicalResolver for scan column collection.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = phys_resolver.resolve_expr(&measure.expr)?;
            collect_column_refs(&lowered, &mut scan_columns, &mut scan_seen);
            for filter in &measure.filters {
                let lowered_filter = phys_resolver.resolve_expr(&filter.expr)?;
                collect_column_refs(&lowered_filter, &mut scan_columns, &mut scan_seen);
            }
        } else if handle_metrics {
            if let Some(metric) = iface.metrics.get(measure_name) {
                let constituents = extract_metric_constituents(metric, iface);
                for cm_name in &constituents {
                    if let Some(cm) = iface.measures.get(cm_name) {
                        let lowered = phys_resolver.resolve_expr(&cm.expr)?;
                        collect_column_refs(&lowered, &mut scan_columns, &mut scan_seen);
                        for filter in &cm.filters {
                            let lowered_filter = phys_resolver.resolve_expr(&filter.expr)?;
                            collect_column_refs(&lowered_filter, &mut scan_columns, &mut scan_seen);
                        }
                    }
                }
            } else {
                return Err(PlannerError::MeasureNotFound {
                    kind: iface.name.clone(),
                    measure: measure_name.clone(),
                });
            }
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: iface.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Entity-level filter columns (physical) — needed in scan for pre-aggregate filtering.
    for filter in &iface.filters {
        let lowered_filter = phys_resolver.resolve_expr(&filter.expr)?;
        collect_column_refs(&lowered_filter, &mut scan_columns, &mut scan_seen);
    }

    // ── Aggregate setup: GROUP BY + measure decomposition ──────────
    let group_by: Vec<Expr> = request
        .dimensions
        .iter()
        .map(|name| {
            if let Some((td_name, grain)) = temporal_rollup {
                if name == td_name {
                    return Expr::date_trunc(grain.into(), Expr::column(name.clone()));
                }
            }
            Expr::column(name.clone())
        })
        .collect();

    let identity_physical: IndexMap<String, String> = IndexMap::new();
    let identity_resolver = PhysicalResolver::new(&identity_physical);
    let mut lowered_measures: Vec<(String, DecomposedMeasure)> = Vec::new();

    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = decomposer::decompose_measure(
                &identity_resolver,
                measure_name,
                measure.agg,
                &measure.expr,
                &measure.filters,
                &measure.data_type,
            )?;
            lowered_measures.push((measure_name.clone(), lowered));
        } else if handle_metrics {
            if let Some(metric) = iface.metrics.get(measure_name) {
                let lowered = decomposer::decompose_metric(
                    measure_name,
                    metric,
                    iface,
                    binding,
                    5,
                )?;
                lowered_measures.push((measure_name.clone(), lowered));
            } else {
                return Err(PlannerError::MeasureNotFound {
                    kind: iface.name.clone(),
                    measure: measure_name.clone(),
                });
            }
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: iface.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), iface.resolve_dim_type(name)))
        .collect();
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

    // ── Final projection setup ──────────────────────────────────────
    let mut project_exprs: Vec<Expr> = Vec::new();
    let mut project_fields: Vec<Field> = Vec::new();

    for dim_name in &request.dimensions {
        project_exprs.push(Expr::column(dim_name.clone()));
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

    // ── Build plan: single-source vs multi-source ─────────────────
    let sem_types = build_semantic_type_map(iface, &mapping.physical);

    // Catalog types: physical column → catalog-reported DataType (for CAST in L2).
    let catalog_types = build_catalog_type_map(binding);

    // ── Pre-resolve entity filters to physical names (for scan-level injection) ──
    let physical_entity_filters: Vec<Expr> = iface
        .filters
        .iter()
        .map(|f| phys_resolver.resolve_expr(&f.expr))
        .collect::<Result<Vec<_>, _>>()?;

    let l4_output = if binding.resolved_sources.len() <= 1 {
        // ── Single-source path ────────────────────────────────────
        let known_values = collect_known_values(binding, &metadata_dims);
        let mut scan = build_scan_node_binding(binding, &scan_columns, &sem_types, pb);

        // Inject entity-level filters right after scan (physical names).
        for predicate in &physical_entity_filters {
            let schema = (*scan.meta().output_schema).clone();
            scan = pb.build_filter(schema, scan, predicate.clone());
        }

        let rename = build_rename_project(
            scan, &dim_physical, &physical_dims, &metadata_dims, &known_values,
            &computed_dims, &request.measures, iface, mapping, &catalog_types, handle_metrics, pb,
        )?;
        let l3 = build_expression_project(&computed_dims, &known_values, rename, iface, pb);
        pb.build_aggregate(agg_schema, l3, group_by, aggregates)
    } else {
        // ── Multi-source: per-source plans, UNION ALL, re-aggregate ──
        let mut source_plans: Vec<PlanNode> = Vec::new();
        let scan_schema = Schema::new(
            scan_columns
                .iter()
                .map(|c| Field::new(c.clone(), resolve_scan_type_binding(c, binding, &sem_types)))
                .collect(),
        );

        for source in &binding.resolved_sources {
            let known_values = collect_known_values_for_source(
                source, &mapping.literals, &metadata_dims,
            );

            // Scan for this specific source.
            let table_name = source.table_fqn.as_deref().unwrap_or(&source.reference);
            let mut scan = pb.build_scan(
                scan_schema.clone(),
                table_name.to_string(),
                source.location.clone(),
                source.format,
                scan_columns.to_vec(),
            );

            // Inject entity-level filters right after scan (physical names).
            for predicate in &physical_entity_filters {
                let schema = (*scan.meta().output_schema).clone();
                scan = pb.build_filter(schema, scan, predicate.clone());
            }

            // Rename with per-source metadata values.
            let rename = build_rename_project(
                scan, &dim_physical, &physical_dims, &metadata_dims, &known_values,
                &computed_dims, &request.measures, iface, mapping, &catalog_types, handle_metrics, pb,
            )?;

            // Expression (computed dims with per-source known_values).
            let l3 = build_expression_project(&computed_dims, &known_values, rename, iface, pb);

            // Aggregate (pre-aggregate per source).
            let agg = pb.build_aggregate(
                agg_schema.clone(), l3, group_by.clone(), aggregates.clone(),
            );

            source_plans.push(agg);
        }

        // UNION ALL of pre-aggregated per-source plans.
        let union = pb.build_union(agg_schema.clone(), source_plans, false);

        // Skip re-aggregation when a metadata dimension in the GROUP BY has
        // distinct values per source — no rows from different sources can merge.
        if has_source_distinguishing_metadata(
            &binding.resolved_sources, &metadata_dims, &request.dimensions,
        ) {
            union
        } else {
            // Re-aggregate the union (merge partial aggregates).
            let reagg_group_by: Vec<Expr> = request
                .dimensions
                .iter()
                .map(|name| Expr::column(name.clone()))
                .collect();

            // Build re-aggregation measures: per-aggregate re-agg function derived from
            // each individual aggregate's original function, NOT the parent measure.
            // SUM/COUNT → SUM (partial sums merge), MIN → MIN, MAX → MAX.
            let num_dims = request.dimensions.len();
            let reagg_aggregates: Vec<AggregateMeasure> = lowered_measures
                .iter()
                .flat_map(|(_, lowered)| &lowered.aggregates)
                .zip(agg_schema.fields[num_dims..].iter())
                .map(|(orig_agg, field)| {
                    let reagg_fn = match orig_agg.function {
                        Aggregation::Min => Aggregation::Min,
                        Aggregation::Max => Aggregation::Max,
                        // SUM, COUNT, COUNT_DISTINCT, AVG partial results all re-aggregate as SUM.
                        _ => Aggregation::Sum,
                    };
                    AggregateMeasure {
                        function: reagg_fn,
                        expr: Expr::column(field.name.clone()),
                        distinct: false,
                        data_type: field.data_type.clone(),
                    }
                })
                .collect();

            let reagg_schema = agg_schema.clone();
            pb.build_aggregate(reagg_schema, union, reagg_group_by, reagg_aggregates)
        }
    };

    // ── Final projection (skipped if identity) ───────────────────
    let l4_schema = &l4_output.meta().output_schema;
    let is_identity = is_identity_projection(&project_exprs, l4_schema);
    let root = if is_identity {
        l4_output
    } else {
        pb.build_project(project_schema.clone(), l4_output, project_exprs)
    };

    let output_schema = if is_identity {
        root.meta().output_schema.as_ref().clone()
    } else {
        project_schema
    };

    Ok(PlanFragment {
        root,
        output_schema,
        pending_filters: Vec::new(),
    })
}

/// Build rename project: maps physical → semantic column names.
///
/// Shared by both single-source and multi-source paths. The `known_values`
/// parameter provides per-source metadata dimension values.
///
/// `catalog_types`: physical column name → catalog-reported DataType.
/// When a physical column's catalog type differs from the semantic type,
/// a CAST is emitted to ensure type safety.
fn build_rename_project(
    scan: PlanNode,
    dim_physical: &[(String, String)],
    physical_dims: &[String],
    metadata_dims: &[(String, MetadataDimension)],
    known_values: &HashMap<String, String>,
    computed_dims: &[(String, semstrait_core::Expr)],
    measure_names: &[String],
    iface: &CompiledInterface,
    mapping: &semstrait_manifest::ResolvedColumnMapping,
    catalog_types: &HashMap<String, DataType>,
    handle_metrics: bool,
    pb: &dyn PlanBuilder,
) -> Result<PlanNode, PlannerError> {
    let mut rename_exprs: Vec<Expr> = Vec::new();
    let mut rename_fields: Vec<Field> = Vec::new();

    // Helper: Column(physical) with CAST when catalog type differs from semantic type.
    let maybe_cast = |physical: &str, semantic_type: &DataType| -> Expr {
        if let Some(catalog_type) = catalog_types.get(physical) {
            if catalog_type != semantic_type {
                return Expr::cast(Expr::column(physical.to_string()), semantic_type.clone());
            }
        }
        Expr::column(physical.to_string())
    };

    // Physical dimensions: semantic := Column(physical), with optional CAST.
    for (semantic, physical) in dim_physical {
        let semantic_type = iface.resolve_dim_type(semantic);
        rename_exprs.push(maybe_cast(physical, &semantic_type));
        rename_fields.push(Field::new(semantic.clone(), semantic_type));
    }

    // Literal dimensions: semantic := Literal(value).
    for dim_name in physical_dims {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            rename_exprs.push(Expr::string(lit_val.clone()));
            rename_fields.push(Field::new(dim_name.clone(), iface.resolve_dim_type(dim_name)));
        }
    }

    // Metadata dimensions: semantic := Literal(extracted_value).
    for (dim_name, _) in metadata_dims {
        let value = known_values.get(dim_name).cloned().unwrap_or_default();
        rename_exprs.push(Expr::string(value));
        rename_fields.push(Field::new(dim_name.clone(), iface.resolve_dim_type(dim_name)));
    }

    // Computed dim dependencies: include physical columns that computed expressions reference.
    for (_, expr) in computed_dims {
        let mut sem_refs: Vec<String> = Vec::new();
        let mut sem_refs_seen: HashSet<String> = HashSet::new();
        collect_column_refs(expr, &mut sem_refs, &mut sem_refs_seen);
        for sem_ref in &sem_refs {
            if !rename_fields.iter().any(|f| f.name == *sem_ref) {
                if let Some(phys) = mapping.physical.get(sem_ref) {
                    let sem_type = resolve_semantic_type(sem_ref, iface);
                    rename_exprs.push(maybe_cast(phys, &sem_type));
                    rename_fields.push(Field::new(sem_ref.clone(), sem_type));
                }
            }
        }
    }

    // Measure source columns: map entity refs to their physical columns, with optional CAST.
    // Helper closure to add a physical→semantic column mapping to the rename project.
    let mut add_physical_ref = |sem_ref: &str| {
        if !rename_fields.iter().any(|f| f.name == sem_ref) {
            if let Some(phys) = mapping.physical.get(sem_ref) {
                let sem_type = resolve_semantic_type(sem_ref, iface);
                rename_exprs.push(maybe_cast(phys, &sem_type));
                rename_fields.push(Field::new(sem_ref.to_string(), sem_type));
            }
        }
    };

    for measure_name in measure_names {
        let expr = if let Some(m) = iface.measures.get(measure_name) {
            Some(&m.expr)
        } else if handle_metrics {
            None
        } else {
            None
        };

        if let Some(expr) = expr {
            let mut sem_refs: Vec<String> = Vec::new();
            let mut sem_refs_seen: HashSet<String> = HashSet::new();
            collect_semantic_refs(expr, &mut sem_refs, &mut sem_refs_seen);
            for sem_ref in &sem_refs {
                add_physical_ref(sem_ref);
            }
            if let Some(m) = iface.measures.get(measure_name) {
                for filter in &m.filters {
                    let mut filter_refs: Vec<String> = Vec::new();
                    let mut filter_seen: HashSet<String> = HashSet::new();
                    collect_semantic_refs(&filter.expr, &mut filter_refs, &mut filter_seen);
                    for sem_ref in &filter_refs {
                        add_physical_ref(sem_ref);
                    }
                }
            }
        } else if handle_metrics {
            if let Some(metric) = iface.metrics.get(measure_name) {
                let constituents = extract_metric_constituents(metric, iface);
                for cm_name in &constituents {
                    if let Some(cm) = iface.measures.get(cm_name) {
                        let mut sem_refs: Vec<String> = Vec::new();
                        let mut sem_refs_seen: HashSet<String> = HashSet::new();
                        collect_semantic_refs(&cm.expr, &mut sem_refs, &mut sem_refs_seen);
                        for sem_ref in &sem_refs {
                            add_physical_ref(sem_ref);
                        }
                    }
                }
            }
        }
    }

    let rename_schema = Schema::new(rename_fields);
    let scan_schema = scan.meta().output_schema.as_ref();
    if is_rename_identity(&rename_exprs, &rename_schema.fields, scan_schema) {
        Ok(scan)
    } else {
        Ok(pb.build_project(rename_schema, scan, rename_exprs))
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers: layered plan construction (rename, expression, known values)
// ═══════════════════════════════════════════════════════════════════

/// Pre-compute compile-time-known dimension values for a binding.
///
/// Collects values from:
/// - Metadata dimensions (extracted from source paths/partitions)
/// - Literal dimensions (from column_mapping.literals)
///
/// These are the "known_values" used for SR-10 static pushdown in
/// computed expression simplification.
pub(crate) fn collect_known_values(
    binding: &DatasetBinding,
    metadata_dims: &[(String, MetadataDimension)],
) -> HashMap<String, String> {
    let mut known = HashMap::new();

    // Metadata dimensions.
    for (name, meta) in metadata_dims {
        let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
        known.insert(name.clone(), value);
    }

    // Literal dimensions.
    for (name, value) in &binding.column_mapping.literals {
        known.insert(name.clone(), value.clone());
    }

    known
}

/// Pre-compute compile-time-known dimension values for a single resolved source.
///
/// Per-source variant of `collect_known_values` — extracts metadata from the
/// specific source rather than defaulting to `binding.resolved_sources.first()`.
pub(crate) fn collect_known_values_for_source(
    source: &ResolvedSource,
    literals: &HashMap<String, String>,
    metadata_dims: &[(String, MetadataDimension)],
) -> HashMap<String, String> {
    let mut known = HashMap::new();

    for (name, meta) in metadata_dims {
        let value = extract_metadata_value_source(meta, source).unwrap_or_default();
        known.insert(name.clone(), value);
    }

    for (name, value) in literals {
        known.insert(name.clone(), value.clone());
    }

    known
}

/// Check whether any metadata dimension in the GROUP BY has distinct values
/// across all resolved sources, making re-aggregation after UNION ALL a no-op.
///
/// When a metadata dimension like `funnel_account_id` produces unique values
/// per source, no two sources can produce rows that share the same GROUP BY key,
/// so the re-aggregation merges nothing and can be skipped.
fn has_source_distinguishing_metadata(
    sources: &[ResolvedSource],
    metadata_dims: &[(String, MetadataDimension)],
    group_by_dims: &[String],
) -> bool {
    if sources.len() <= 1 {
        return false; // single source: re-agg is already skipped by the caller
    }
    for (dim_name, meta) in metadata_dims {
        if !group_by_dims.contains(dim_name) {
            continue; // not in GROUP BY — can't distinguish
        }
        let values: Vec<Option<String>> = sources
            .iter()
            .map(|s| extract_metadata_value_source(meta, s))
            .collect();
        // All values must be Some and all must be distinct.
        if values.iter().all(|v| v.is_some()) {
            let unique: HashSet<&str> = values.iter().filter_map(|v| v.as_deref()).collect();
            if unique.len() == sources.len() {
                return true;
            }
        }
    }
    false
}

/// Build a map from physical column name → catalog-reported DataType.
///
/// Uses the first resolved source's schema (all sources in the same binding
/// share the same physical schema). Returns an empty map when no catalog
/// schema is available.
fn build_catalog_type_map(binding: &DatasetBinding) -> HashMap<String, DataType> {
    let mut map = HashMap::new();
    if let Some(schema) = binding
        .resolved_sources
        .first()
        .and_then(|s| s.schema.as_ref())
    {
        for col in schema {
            map.insert(col.name.clone(), col.data_type.clone());
        }
    }
    map
}

/// Check if a projection is an identity (every expr is Column(name) matching the input schema fields).
///
/// Returns `true` when L5 can be skipped because it would produce the same output as L4.
fn is_identity_projection(exprs: &[Expr], input_schema: &Schema) -> bool {
    if exprs.len() != input_schema.fields.len() {
        return false;
    }
    exprs
        .iter()
        .zip(input_schema.fields.iter())
        .all(|(expr, field)| matches!(expr, Expr::Column(col) if col.name == field.name))
}

/// Check if L2 rename projection is an identity transformation.
///
/// Stronger than `is_identity_projection`: also verifies that output field names
/// match input field names (no physical→semantic renaming). Returns `true` when
/// every expression is `Column(col)` where `col.name` equals both the scan field
/// name and the rename output field name at that position.
fn is_rename_identity(
    rename_exprs: &[Expr],
    rename_fields: &[Field],
    scan_schema: &Schema,
) -> bool {
    if rename_exprs.len() != scan_schema.fields.len()
        || rename_fields.len() != scan_schema.fields.len()
    {
        return false;
    }
    rename_exprs
        .iter()
        .zip(rename_fields.iter())
        .zip(scan_schema.fields.iter())
        .all(|((expr, out_field), in_field)| {
            matches!(expr, Expr::Column(col) if col.name == in_field.name)
                && out_field.name == in_field.name
        })
}

/// Resolve DataType for a semantic name from CompiledInterface.
///
/// Checks dimensions, then measures, then metrics. Falls back to String.
pub(crate) fn resolve_semantic_type(name: &str, iface: &CompiledInterface) -> DataType {
    if let Some(d) = iface.dimensions.get(name) {
        return d.data_type.clone();
    }
    if let Some(m) = iface.measures.get(name) {
        return m.data_type.clone();
    }
    if let Some(m) = iface.metrics.get(name) {
        return m.data_type.clone();
    }
    DataType::String
}

/// Collect semantic entity/column references from an expression tree.
///
/// Collects both `Column(name)` and `EntityRef(name)` — used to determine
/// which semantic names a measure expression depends on (for rename project).
pub(crate) fn collect_semantic_refs(
    expr: &Expr,
    refs: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    match expr {
        Expr::Column(col) => {
            if seen.insert(col.name.clone()) {
                refs.push(col.name.clone());
            }
        }
        Expr::EntityRef(er) => {
            if seen.insert(er.name.clone()) {
                refs.push(er.name.clone());
            }
        }
        Expr::BinaryOp(bin) => {
            collect_semantic_refs(&bin.left, refs, seen);
            collect_semantic_refs(&bin.right, refs, seen);
        }
        Expr::Case(case) => {
            for wc in &case.when_then {
                collect_semantic_refs(&wc.condition, refs, seen);
                collect_semantic_refs(&wc.result, refs, seen);
            }
            if let Some(e) = &case.else_expr {
                collect_semantic_refs(e, refs, seen);
            }
        }
        Expr::FunctionCall(fc) => {
            for arg in &fc.args {
                collect_semantic_refs(arg, refs, seen);
            }
        }
        Expr::Aggregate(agg) => collect_semantic_refs(&agg.expr, refs, seen),
        Expr::Negate(u) | Expr::Not(u) | Expr::IsNull(u) | Expr::IsNotNull(u) => {
            collect_semantic_refs(&u.expr, refs, seen);
        }
        Expr::InList(il) => {
            collect_semantic_refs(&il.expr, refs, seen);
            for item in &il.list {
                collect_semantic_refs(item, refs, seen);
            }
        }
        Expr::Guard(g) => {
            collect_semantic_refs(&g.condition, refs, seen);
            collect_semantic_refs(&g.expr, refs, seen);
        }
        Expr::Cast(c) => collect_semantic_refs(&c.expr, refs, seen),
        Expr::DateTrunc(dt) => collect_semantic_refs(&dt.expr, refs, seen),
        Expr::Coalesce(co) => {
            for e in &co.exprs {
                collect_semantic_refs(e, refs, seen);
            }
        }
        Expr::NullIf(ni) => {
            collect_semantic_refs(&ni.expr, refs, seen);
            collect_semantic_refs(&ni.null_expr, refs, seen);
        }
        Expr::Between(bt) => {
            collect_semantic_refs(&bt.expr, refs, seen);
            collect_semantic_refs(&bt.low, refs, seen);
            collect_semantic_refs(&bt.high, refs, seen);
        }
        Expr::Like(lk) => {
            collect_semantic_refs(&lk.expr, refs, seen);
            collect_semantic_refs(&lk.pattern, refs, seen);
        }
        Expr::ILike(lk) => {
            collect_semantic_refs(&lk.expr, refs, seen);
            collect_semantic_refs(&lk.pattern, refs, seen);
        }
        Expr::RegexpMatch(re) => {
            collect_semantic_refs(&re.expr, refs, seen);
            collect_semantic_refs(&re.pattern, refs, seen);
        }
        Expr::RegexpExtract(re) => {
            collect_semantic_refs(&re.expr, refs, seen);
            collect_semantic_refs(&re.pattern, refs, seen);
        }
        Expr::Literal(_) => {}
    }
}

/// Build expression ProjectNode for computed dimensions with SR-10 simplification.
///
/// For each computed dim: `resolve_guards → substitute(known_values) → simplify`.
/// Passes through all existing columns from input.
/// Returns the input unchanged if no computed dims (skip this layer).
pub(crate) fn build_expression_project(
    computed_dims: &[(String, semstrait_core::Expr)],
    known_values: &HashMap<String, String>,
    input: PlanNode,
    iface: &CompiledInterface,
    pb: &dyn PlanBuilder,
) -> PlanNode {
    if computed_dims.is_empty() {
        return input;
    }

    let input_schema = input.meta().output_schema.clone();

    // Passthrough all existing columns.
    let mut project_exprs: Vec<Expr> = input_schema
        .fields
        .iter()
        .map(|f| Expr::column(f.name.clone()))
        .collect();
    let mut project_fields: Vec<Field> = input_schema.fields.clone();

    // Add computed dimension expressions with SR-10 simplification.
    for (dim_name, expr) in computed_dims {
        let guard_resolved = resolve_guards(expr);
        let substituted = crate::simplify::substitute(&guard_resolved, known_values);
        let simplified = crate::simplify::simplify(&substituted);

        project_exprs.push(simplified);
        project_fields.push(Field::new(
            dim_name.clone(),
            iface.resolve_dim_type(dim_name),
        ));
    }

    let schema = Schema::new(project_fields);
    pb.build_project(schema, input, project_exprs)
}

/// Validate that all UNION branches produce the same types.
///
/// Errors on type mismatch rather than falling back to a default type.
/// This ensures type consistency is enforced at plan time.
pub(crate) fn validate_union_types(branches: &[PlanNode]) -> Result<(), PlannerError> {
    if branches.len() <= 1 {
        return Ok(());
    }
    let expected = &branches[0].meta().output_schema.fields;
    for (i, branch) in branches[1..].iter().enumerate() {
        let actual = &branch.meta().output_schema.fields;
        if actual.len() != expected.len() {
            return Err(PlannerError::Internal(format!(
                "UNION branch {}: field count mismatch ({} vs {})",
                i + 1,
                actual.len(),
                expected.len()
            )));
        }
        for (exp, act) in expected.iter().zip(actual.iter()) {
            if exp.data_type != act.data_type {
                return Err(PlannerError::Internal(format!(
                    "UNION branch {}, column '{}': type mismatch ({:?} vs {:?})",
                    i + 1,
                    exp.name,
                    exp.data_type,
                    act.data_type
                )));
            }
        }
    }
    Ok(())
}

/// Recursively expands nested metric references to their underlying measures.
/// For example, `roi = profit / cost` where `profit = revenue - cost` returns
/// `["revenue", "cost"]` — the actual physical measures, not intermediate metrics.
pub(crate) fn extract_metric_constituents(
    metric: &semstrait_manifest::CompiledMetric,
    iface: &CompiledInterface,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_leaf_measures(&metric.expr, iface, &mut names, &mut seen);
    names
}

/// Collect transitive leaf measure names from an expression tree.
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
        if seen.insert(format!("__metric__{}", name)) {
            collect_leaf_measures(&sub_metric.expr, iface, out, seen);
        }
    } else if seen.insert(name.to_string()) {
        out.push(name.to_string());
    }
}

/// Parameters for building a single UNION branch.
///
/// Callers determine which measures are covered; any measure not in
/// `covered_measures` is null-filled in the final projection.
pub(crate) struct UnionBranchParams<'a> {
    /// Measures/metrics covered by this binding (aggregated normally).
    pub covered_measures: Vec<String>,
    /// Optional temporal rollup: (dim_name, grain) for DATE_TRUNC in GROUP BY.
    pub temporal_rollup: Option<(&'a str, TemporalGrain)>,
}

/// Build a single UNION branch for one dataset binding.
///
/// Layered architecture: Scan → Rename → Expression → Aggregate → Project.
/// The final projection outputs the unified schema, using NULL for unmapped
/// dimensions and uncovered measures.
///
/// For multi-source bindings (multiple resolved sources), builds per-source
/// plans with correct per-source metadata values, UNION ALLs, and re-aggregates
/// before the final null-fill projection.
pub(crate) fn build_union_branch(
    iface: &CompiledInterface,
    request: &ResolvedQueryRequest,
    binding: &DatasetBinding,
    params: &UnionBranchParams<'_>,
    unified_schema: &Schema,
    ctx: &PlannerContext<'_>,
) -> Result<PlanNode, PlannerError> {
    let mapping = &binding.column_mapping;
    let pb = ctx.plan_builder;

    // ── Partition dimensions ──────────────────────────────────────
    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_dims_all, computed_dims) = super::split_computed_dims(&regular_dims, iface);

    // Determine which computed dims are resolvable (all deps available in this binding).
    let known_values_binding = collect_known_values(binding, &metadata_dims);
    let mut resolvable_computed: Vec<(String, semstrait_core::Expr)> = Vec::new();
    let mut null_computed: HashSet<String> = HashSet::new();
    for (name, expr) in &computed_dims {
        let mut refs = Vec::new();
        let mut seen = HashSet::new();
        collect_column_refs(expr, &mut refs, &mut seen);
        let all_available = refs.iter().all(|r| {
            mapping.physical.contains_key(r)
                || mapping.literals.contains_key(r)
                || known_values_binding.contains_key(r)
        });
        if all_available {
            resolvable_computed.push((name.clone(), expr.clone()));
        } else {
            null_computed.insert(name.clone());
        }
    }

    // Track null-filled physical dims (not mapped in this binding).
    let mut null_physical: HashSet<String> = HashSet::new();

    // ── Collect physical scan columns ─────────────────────────────
    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    let mut dim_physical: Vec<(String, String)> = Vec::new();
    for dim_name in &physical_dims_all {
        if mapping.literals.contains_key(dim_name) {
            continue;
        } else if let Some(phys) = mapping.physical.get(dim_name) {
            dim_physical.push((dim_name.clone(), phys.clone()));
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys.clone());
            }
        } else {
            null_physical.insert(dim_name.clone());
        }
    }

    // Resolvable computed dim deps (with phantom ref filtering).
    {
        let physical_values: HashSet<&str> =
            mapping.physical.values().map(|v| v.as_str()).collect();
        for (_, expr) in &resolvable_computed {
            let lowered = PhysicalResolver::new(&mapping.physical).resolve_expr(expr)?;
            let mut temp = Vec::new();
            let mut temp_seen = HashSet::new();
            collect_column_refs(&lowered, &mut temp, &mut temp_seen);
            for col in temp {
                if physical_values.contains(col.as_str()) && scan_seen.insert(col.clone()) {
                    scan_columns.push(col);
                }
            }
        }
    }

    // Covered measure scan columns.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    for measure_name in &params.covered_measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = phys_resolver.resolve_expr(&measure.expr)?;
            collect_column_refs(&lowered, &mut scan_columns, &mut scan_seen);
            for filter in &measure.filters {
                let lowered_f = phys_resolver.resolve_expr(&filter.expr)?;
                collect_column_refs(&lowered_f, &mut scan_columns, &mut scan_seen);
            }
        } else if let Some(metric) = iface.metrics.get(measure_name) {
            let constituents = extract_metric_constituents(metric, iface);
            for cm_name in &constituents {
                if let Some(cm) = iface.measures.get(cm_name) {
                    let lowered = phys_resolver.resolve_expr(&cm.expr)?;
                    collect_column_refs(&lowered, &mut scan_columns, &mut scan_seen);
                    for filter in &cm.filters {
                        let lowered_f = phys_resolver.resolve_expr(&filter.expr)?;
                        collect_column_refs(&lowered_f, &mut scan_columns, &mut scan_seen);
                    }
                }
            }
        }
    }

    // ── Aggregate setup ──────────────────────────────────────────
    // GROUP BY: non-null dimensions, with optional DATE_TRUNC for grain rollup.
    let mut group_by: Vec<Expr> = Vec::new();
    for dim_name in &request.dimensions {
        if null_physical.contains(dim_name) || null_computed.contains(dim_name) {
            continue;
        }
        if let Some((td_name, grain)) = params.temporal_rollup {
            if dim_name == td_name {
                group_by.push(Expr::date_trunc(grain.into(), Expr::column(dim_name.clone())));
                continue;
            }
        }
        group_by.push(Expr::column(dim_name.clone()));
    }

    // Decompose measures in request order (covered get real decomposition, uncovered get None).
    let identity_physical: IndexMap<String, String> = IndexMap::new();
    let identity_resolver = PhysicalResolver::new(&identity_physical);
    let covered_set: HashSet<&str> = params.covered_measures.iter().map(|s| s.as_str()).collect();
    let mut lowered_measures: Vec<(String, Option<DecomposedMeasure>)> = Vec::new();

    for measure_name in &request.measures {
        if covered_set.contains(measure_name.as_str()) {
            if let Some(measure) = iface.measures.get(measure_name) {
                let lowered = decomposer::decompose_measure(
                    &identity_resolver,
                    measure_name,
                    measure.agg,
                    &measure.expr,
                    &measure.filters,
                    &measure.data_type,
                )?;
                lowered_measures.push((measure_name.clone(), Some(lowered)));
            } else if let Some(metric) = iface.metrics.get(measure_name) {
                let lowered = decomposer::decompose_metric(
                    measure_name, metric, iface, binding, 4,
                )?;
                lowered_measures.push((measure_name.clone(), Some(lowered)));
            } else {
                lowered_measures.push((measure_name.clone(), None));
            }
        } else {
            lowered_measures.push((measure_name.clone(), None));
        }
    }

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .filter_map(|(_, lowered)| lowered.as_ref())
        .flat_map(|l| l.aggregates.clone())
        .collect();

    // Aggregate schema: non-null dimension fields + covered measure fields.
    let mut agg_fields: Vec<Field> = group_by
        .iter()
        .filter_map(|e| match e {
            Expr::Column(c) => Some(Field::new(c.name.clone(), iface.resolve_dim_type(&c.name))),
            Expr::DateTrunc(dt) => {
                if let Expr::Column(c) = dt.expr.as_ref() {
                    Some(Field::new(c.name.clone(), iface.resolve_dim_type(&c.name)))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
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

    // ── Build plan: single-source vs multi-source ────────────────
    let sem_types = build_semantic_type_map(iface, &mapping.physical);
    let catalog_types = build_catalog_type_map(binding);
    let covered_names: Vec<String> = params.covered_measures.clone();

    let agg_output = if binding.resolved_sources.len() <= 1 {
        // ── Single-source path ────────────────────────────────────
        let known_values = collect_known_values(binding, &metadata_dims);
        let scan = build_scan_node_binding(binding, &scan_columns, &sem_types, pb);
        let rename = build_rename_project(
            scan, &dim_physical, &physical_dims_all, &metadata_dims, &known_values,
            &resolvable_computed, &covered_names, iface, mapping, &catalog_types, true, pb,
        )?;
        let expr_out = build_expression_project(&resolvable_computed, &known_values, rename, iface, pb);
        pb.build_aggregate(agg_schema, expr_out, group_by, aggregates)
    } else {
        // ── Multi-source: per-source plans, UNION ALL, re-aggregate ──
        let scan_schema = Schema::new(
            scan_columns
                .iter()
                .map(|c| Field::new(c.clone(), resolve_scan_type_binding(c, binding, &sem_types)))
                .collect(),
        );

        let mut source_plans: Vec<PlanNode> = Vec::new();
        for source in &binding.resolved_sources {
            let known_values = collect_known_values_for_source(
                source, &mapping.literals, &metadata_dims,
            );

            let table_name = source.table_fqn.as_deref().unwrap_or(&source.reference);
            let scan = pb.build_scan(
                scan_schema.clone(),
                table_name.to_string(),
                source.location.clone(),
                source.format,
                scan_columns.to_vec(),
            );

            let rename = build_rename_project(
                scan, &dim_physical, &physical_dims_all, &metadata_dims, &known_values,
                &resolvable_computed, &covered_names, iface, mapping, &catalog_types, true, pb,
            )?;

            let expr_out = build_expression_project(&resolvable_computed, &known_values, rename, iface, pb);

            let agg = pb.build_aggregate(
                agg_schema.clone(), expr_out, group_by.clone(), aggregates.clone(),
            );

            source_plans.push(agg);
        }

        let union = pb.build_union(agg_schema.clone(), source_plans, false);

        // Non-null dims that participate in GROUP BY.
        let active_dims: Vec<String> = request
            .dimensions
            .iter()
            .filter(|d| !null_physical.contains(*d) && !null_computed.contains(*d))
            .cloned()
            .collect();

        // Skip re-aggregation when a metadata dimension in the GROUP BY has
        // distinct values per source — no rows from different sources can merge.
        if has_source_distinguishing_metadata(
            &binding.resolved_sources, &metadata_dims, &active_dims,
        ) {
            union
        } else {
            // Re-aggregate: merge partial aggregates from different sources.
            let reagg_group_by: Vec<Expr> = active_dims
                .iter()
                .map(|name| Expr::column(name.clone()))
                .collect();

            let num_dims = reagg_group_by.len();
            let reagg_aggregates: Vec<AggregateMeasure> = lowered_measures
                .iter()
                .filter_map(|(_, lowered)| lowered.as_ref())
                .flat_map(|l| &l.aggregates)
                .zip(agg_schema.fields[num_dims..].iter())
                .map(|(orig_agg, field)| {
                    let reagg_fn = match orig_agg.function {
                        Aggregation::Min => Aggregation::Min,
                        Aggregation::Max => Aggregation::Max,
                        _ => Aggregation::Sum,
                    };
                    AggregateMeasure {
                        function: reagg_fn,
                        expr: Expr::column(field.name.clone()),
                        distinct: false,
                        data_type: field.data_type.clone(),
                    }
                })
                .collect();

            pb.build_aggregate(agg_schema.clone(), union, reagg_group_by, reagg_aggregates)
        }
    };

    // ── Final projection (unified schema with null-fill) ──────────
    let mut project_exprs: Vec<Expr> = Vec::new();

    for dim_name in &request.dimensions {
        if null_physical.contains(dim_name) || null_computed.contains(dim_name) {
            project_exprs.push(Expr::null());
        } else {
            project_exprs.push(Expr::column(dim_name.clone()));
        }
    }
    for (_, lowered) in &lowered_measures {
        project_exprs.push(
            lowered
                .as_ref()
                .map_or(Expr::null(), |l| l.post_agg_expr.clone()),
        );
    }

    Ok(pb.build_project(unified_schema.clone(), agg_output, project_exprs))
}
