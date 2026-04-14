//! Shared plan-building utilities used by grainset, joinset, and unionset planners.

use crate::error::PlannerError;
use crate::decomposer::{self, DecomposedMeasure};
use crate::resolver::{ExprResolver as _, PhysicalResolver};
use super::{extract_metadata_value_binding, partition_dimensions_iface, resolve_guards, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    Aggregation, AggregateMeasure, Expr, Field, PlanBuilder, PlanNode,
    Schema,
};
use indexmap::IndexMap;
use semstrait_manifest::{DatasetBinding, CompiledDatasetKind, CompiledInterface};
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

/// Build Scan → Aggregate → Project plan for a CompiledDatasetKind (single-dataset fast path).
///
/// Uses `CompiledInterface` for type resolution and `DatasetBinding` for column mapping.
pub(crate) fn build_dataset_kind_plan(
    dk: &CompiledDatasetKind,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    let iface = &dk.interface;
    let binding = &dk.binding;
    let mapping = &binding.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Partition dimensions into metadata, computed, and regular (physical).
    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_dims, computed_dims) = super::split_computed_dims(&regular_dims, iface);

    let mut dim_physical: Vec<(String, String)> = Vec::new();
    let mut metadata_literals: Vec<(String, Expr)> = Vec::new();

    for dim_name in &physical_dims {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            metadata_literals.push((dim_name.clone(), Expr::string(lit_val.clone())));
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

    // Pre-compute metadata dimension values so computed expressions can reference them.
    let metadata_values: std::collections::HashMap<String, String> = metadata_dims
        .iter()
        .map(|(name, meta)| {
            let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
            (name.clone(), value)
        })
        .collect();

    // Lower computed dimension expressions to collect physical scan columns,
    // but store the semantic expression (with Guards resolved and literals/metadata inlined).
    let mut lowered_computed: Vec<(String, Expr)> = Vec::new();
    let mut extra_group_by_cols: Vec<(String, String)> = Vec::new();
    for (dim_name, expr) in &computed_dims {
        let lowered = PhysicalResolver::new(&mapping.physical).resolve_expr(expr)?;
        collect_column_refs(&lowered, &mut scan_columns, &mut scan_seen);
        // Resolve Guards and inline literal + metadata values.
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
        // Collect semantic column refs that need to survive aggregation.
        let mut sem_refs: Vec<String> = Vec::new();
        let mut sem_seen: HashSet<String> = HashSet::new();
        collect_column_refs(expr, &mut sem_refs, &mut sem_seen);
        for sem_ref in &sem_refs {
            if !dim_physical.iter().any(|(s, _)| s == sem_ref)
                && !extra_group_by_cols.iter().any(|(s, _)| s == sem_ref)
            {
                if let Some(phys) = mapping.physical.get(sem_ref.as_str()) {
                    extra_group_by_cols.push((sem_ref.clone(), phys.clone()));
                }
            }
        }
        lowered_computed.push((dim_name.clone(), resolved));
    }

    // Extract metadata dimension values as literals.
    for (dim_name, _meta) in &metadata_dims {
        metadata_literals.push((dim_name.clone(), Expr::string(metadata_values.get(dim_name).cloned().unwrap_or_default())));
    }

    // Lower measures via physical mapping.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    let mut lowered_measures: Vec<(String, DecomposedMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = decomposer::decompose_measure(
                &phys_resolver,
                measure_name,
                measure.agg,
                &measure.expr,
                &measure.filters,
                &measure.data_type,
            )?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if let Some(metric) = iface.metrics.get(measure_name) {
            let lowered = decomposer::decompose_metric(
                measure_name,
                metric,
                iface,
                binding,
                5, // max decomposition depth
            )?;
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

    // Build Scan node.
    let pb = ctx.plan_builder;
    let sem_types = build_semantic_type_map(iface, &mapping.physical);
    let scan = build_scan_node_binding(binding, &scan_columns, &sem_types, pb);

    // Build Aggregate node — include extra GROUP BY columns for computed dim refs.
    let group_by: Vec<Expr> = dim_physical
        .iter()
        .chain(extra_group_by_cols.iter())
        .map(|(_, physical)| Expr::column(physical.clone()))
        .collect();

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = dim_physical
        .iter()
        .chain(extra_group_by_cols.iter())
        .map(|(semantic, _)| Field::new(semantic.clone(), iface.resolve_dim_type(semantic)))
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

    let agg = pb.build_aggregate(agg_schema, scan, group_by, aggregates);

    // Build Project node.
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

/// Build Scan → Aggregate → Project plan for a single DatasetBinding.
///
/// This is the per-binding plan builder used by ALL kind planners (grainset,
/// unionset, joinset). It takes `&CompiledInterface` for type resolution and
/// `&DatasetBinding` for column mapping, replacing the v1 `build_dataset_plan`.
pub(crate) fn build_binding_plan(
    iface: &CompiledInterface,
    binding: &DatasetBinding,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
    handle_metrics: bool,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &binding.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Partition dimensions into metadata, computed, and regular (physical).
    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_dims, computed_dims) = super::split_computed_dims(&regular_dims, iface);

    let mut dim_physical: Vec<(String, String)> = Vec::new();
    let mut metadata_literals: Vec<(String, Expr)> = Vec::new();

    for dim_name in &physical_dims {
        if let Some(lit_val) = mapping.literals.get(dim_name) {
            metadata_literals.push((dim_name.clone(), Expr::string(lit_val.clone())));
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

    // Pre-compute metadata dimension values so computed expressions can reference them.
    let metadata_values: std::collections::HashMap<String, String> = metadata_dims
        .iter()
        .map(|(name, meta)| {
            let value = extract_metadata_value_binding(meta, binding).unwrap_or_default();
            (name.clone(), value)
        })
        .collect();

    // Lower computed dimension expressions to collect physical scan columns,
    // but store the semantic expression (with Guards resolved and literals/metadata inlined).
    let mut lowered_computed: Vec<(String, Expr)> = Vec::new();
    let mut extra_group_by_cols: Vec<(String, String)> = Vec::new();
    for (dim_name, expr) in &computed_dims {
        let lowered = PhysicalResolver::new(&mapping.physical).resolve_expr(expr)?;
        collect_column_refs(&lowered, &mut scan_columns, &mut scan_seen);
        // Resolve Guards and inline literal + metadata values.
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
        // Collect semantic column refs that need to survive aggregation.
        let mut sem_refs: Vec<String> = Vec::new();
        let mut sem_seen: HashSet<String> = HashSet::new();
        collect_column_refs(expr, &mut sem_refs, &mut sem_seen);
        for sem_ref in &sem_refs {
            if !dim_physical.iter().any(|(s, _)| s == sem_ref)
                && !extra_group_by_cols.iter().any(|(s, _)| s == sem_ref)
            {
                if let Some(phys) = mapping.physical.get(sem_ref.as_str()) {
                    extra_group_by_cols.push((sem_ref.clone(), phys.clone()));
                }
            }
        }
        lowered_computed.push((dim_name.clone(), resolved));
    }

    // Extract metadata dimension values as literals.
    for (dim_name, _meta) in &metadata_dims {
        metadata_literals.push((dim_name.clone(), Expr::string(metadata_values.get(dim_name).cloned().unwrap_or_default())));
    }

    // Lower measures via physical mapping.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    let mut lowered_measures: Vec<(String, DecomposedMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let lowered = decomposer::decompose_measure(
                &phys_resolver,
                measure_name,
                measure.agg,
                &measure.expr,
                &measure.filters,
                &measure.data_type,
            )?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if handle_metrics {
            if let Some(metric) = iface.metrics.get(measure_name) {
                // Decompose metric into constituent measure aggregates.
                let lowered = decomposer::decompose_metric(
                    measure_name,
                    metric,
                    iface,
                    binding,
                    5, // max decomposition depth
                )?;
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
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: iface.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Build Scan node.
    let pb = ctx.plan_builder;
    let sem_types = build_semantic_type_map(iface, &mapping.physical);
    let scan = build_scan_node_binding(binding, &scan_columns, &sem_types, pb);

    // Build Aggregate node — include extra GROUP BY columns for computed dim refs.
    let group_by: Vec<Expr> = dim_physical
        .iter()
        .chain(extra_group_by_cols.iter())
        .map(|(_, physical)| Expr::column(physical.clone()))
        .collect();

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    let mut agg_fields: Vec<Field> = dim_physical
        .iter()
        .chain(extra_group_by_cols.iter())
        .map(|(semantic, _)| Field::new(semantic.clone(), iface.resolve_dim_type(semantic)))
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

    let agg = pb.build_aggregate(agg_schema, scan, group_by, aggregates);

    // Build Project node.
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
