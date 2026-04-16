//! JoinsetPlanner — kind planner for Joinset kinds.
//!
//! Uses BFS from an anchor dataset to construct a join chain.
//! The anchor is the dataset that covers the most requested fields.
//! Other datasets are joined in order of the defined relationships.

use crate::error::PlannerError;
use crate::decomposer::{self, DecomposedMeasure};
use crate::resolver::PhysicalResolver;
use super::collect_column_refs;
use super::plan_layers;
use super::plan_layers::{build_scan_node_binding, build_semantic_type_map, collect_known_values, resolve_semantic_type, collect_semantic_refs, build_expression_project};
use super::{partition_dimensions_iface, split_computed_dims, DataKindPlanner, PlanFragment, PlannerContext, PrunedView};
use crate::request::ResolvedQueryRequest;
use indexmap::IndexMap;
use semstrait_ir::{
    AggregateMeasure, Expr, Field, JoinType as IrJoinType,
    PlanBuilder, PlanNode, Schema,
};
use semstrait_manifest::{
    CompiledManifest, CompiledRelationship, JoinType as ModelJoinType, CompiledInterface,
};
use semstrait_manifest::acceleration::{AdjacencyIndex, CompiledDataKind, DatasetBinding, CompiledJoinsetKind};
use std::collections::{HashSet, VecDeque};

/// Planner for Joinset kinds — BFS-based join chain construction.
pub struct JoinsetPlanner;

impl DataKindPlanner for JoinsetPlanner {
    fn supports(&self, data_kind: &CompiledDataKind) -> bool {
        matches!(data_kind, CompiledDataKind::Joinset(_))
    }

    fn resolve(
        &self,
        pruned: &PrunedView<'_>,
        request: &ResolvedQueryRequest,
        ctx: &PlannerContext<'_>,
    ) -> Result<PlanFragment, PlannerError> {
        let joinset = match pruned.data_kind() {
            CompiledDataKind::Joinset(j) => j,
            _ => return Err(PlannerError::Internal("JoinsetPlanner received non-Joinset CompiledDataKind".into())),
        };
        let iface = &joinset.interface;
        let active_count = pruned.active_count();

        if active_count == 0 {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: "joinset kind has no datasets".to_string(),
            });
        }

        if joinset.relationships.is_empty() && active_count > 1 {
            return Err(PlannerError::NoCoveringDataset {
                kind: iface.name.clone(),
                reason: "joinset kind has multiple datasets but no relationships".to_string(),
            });
        }

        // If single active dataset, delegate to simple scan-aggregate-project.
        let active_bindings = pruned.active_bindings();
        if active_count == 1 {
            return plan_layers::build_binding_plan(iface, active_bindings[0], request, ctx, false, None);
        }

        // Find the anchor dataset (covers most requested fields) among active bindings.
        let anchor_idx = find_anchor_index(iface, &joinset.bindings, pruned, request)?;

        // BFS from anchor through relationships, visiting only active bindings.
        let join_order = bfs_join_order(anchor_idx, &joinset.adjacency_index, pruned);

        // Build the join tree.
        build_join_plan(joinset, request, anchor_idx, &join_order, ctx)
    }
}

/// Map model JoinType to IR JoinType.
pub(crate) fn map_join_type(jt: &ModelJoinType) -> IrJoinType {
    match jt {
        ModelJoinType::Inner => IrJoinType::Inner,
        ModelJoinType::Left => IrJoinType::Left,
        ModelJoinType::Right => IrJoinType::Right,
        ModelJoinType::Full => IrJoinType::Full,
    }
}

/// Find the anchor dataset index — the one covering the most requested fields.
/// Metadata dimensions are excluded — they don't need column mapping coverage.
/// Only considers active bindings from the pruned view.
fn find_anchor_index(
    iface: &CompiledInterface,
    bindings: &[DatasetBinding],
    pruned: &PrunedView<'_>,
    request: &ResolvedQueryRequest,
) -> Result<usize, PlannerError> {
    let (_, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let needed: Vec<&str> = regular_dims
        .iter()
        .chain(request.measures.iter())
        .map(|s| s.as_str())
        .collect();

    bindings
        .iter()
        .enumerate()
        .filter(|(i, _)| pruned.is_active(*i))
        .max_by_key(|(_, binding)| {
            needed
                .iter()
                .filter(|name| binding.column_mapping.contains_key(name))
                .count()
        })
        .map(|(i, _)| i)
        .ok_or_else(|| PlannerError::NoCoveringDataset {
            kind: iface.name.clone(),
            reason: "no datasets available for anchor selection".to_string(),
        })
}

/// A step in the join order: the binding index to join and the relationship to use.
struct JoinStep {
    binding_idx: usize,
    relationship_idx: usize,
    /// Whether this step joins via the reverse edge (to -> from direction).
    reversed: bool,
}

/// BFS from the anchor dataset through the adjacency index to determine join order.
/// Only visits active bindings from the pruned view.
fn bfs_join_order(
    anchor_idx: usize,
    adjacency: &AdjacencyIndex,
    pruned: &PrunedView<'_>,
) -> Vec<JoinStep> {
    let mut visited: HashSet<usize> = HashSet::new();
    visited.insert(anchor_idx);

    let mut queue: VecDeque<usize> = VecDeque::new();
    queue.push_back(anchor_idx);

    let mut steps: Vec<JoinStep> = Vec::new();

    while let Some(current) = queue.pop_front() {
        // Forward edges (from -> to).
        for &(neighbor_idx, rel_idx) in &adjacency.forward[current] {
            if !visited.contains(&neighbor_idx) && pruned.is_active(neighbor_idx) {
                visited.insert(neighbor_idx);
                steps.push(JoinStep {
                    binding_idx: neighbor_idx,
                    relationship_idx: rel_idx,
                    reversed: false,
                });
                queue.push_back(neighbor_idx);
            }
        }
        // Reverse edges (to -> from).
        for &(neighbor_idx, rel_idx) in &adjacency.reverse[current] {
            if !visited.contains(&neighbor_idx) && pruned.is_active(neighbor_idx) {
                visited.insert(neighbor_idx);
                steps.push(JoinStep {
                    binding_idx: neighbor_idx,
                    relationship_idx: rel_idx,
                    reversed: true,
                });
                queue.push_back(neighbor_idx);
            }
        }
    }

    steps
}

/// Build a scan node for a dataset binding in a joinset context.
///
/// Includes dimension columns, measure columns, and join key columns
/// from all relationships that reference this binding.
fn build_scan(
    binding: &DatasetBinding,
    iface: &CompiledInterface,
    relationships: &[CompiledRelationship],
    request: &ResolvedQueryRequest,
    pb: &dyn PlanBuilder,
) -> Result<PlanNode, PlannerError> {
    let mapping = &binding.column_mapping;
    let mut scan_columns: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Partition dimensions and add physical columns to scan.
    let (_, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_dims, computed_dims) = split_computed_dims(&regular_dims, iface);

    for dim_name in &physical_dims {
        if mapping.literals.contains_key(dim_name) {
            continue;
        }
        if let Some(phys) = mapping.physical.get(dim_name) {
            if seen.insert(phys.clone()) {
                scan_columns.push(phys.clone());
            }
        }
    }

    // Add columns referenced by computed dimensions.
    for (_, expr) in &computed_dims {
        let mut sem_refs: Vec<String> = Vec::new();
        let mut sem_seen_refs: HashSet<String> = HashSet::new();
        collect_column_refs(expr, &mut sem_refs, &mut sem_seen_refs);
        for sem_ref in &sem_refs {
            if let Some(phys) = mapping.physical.get(sem_ref.as_str()) {
                if seen.insert(phys.clone()) {
                    scan_columns.push(phys.clone());
                }
            }
        }
    }

    // Add measure columns this binding covers.
    let phys_resolver = PhysicalResolver::new(&mapping.physical);
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            if mapping.contains_key(measure_name) {
                let lowered = decomposer::decompose_measure(
                    &phys_resolver,
                    measure_name,
                    measure.agg,
                    &measure.expr,
                    &measure.filters,
                    &measure.data_type,
                )?;
                for agg in &lowered.aggregates {
                    collect_column_refs(&agg.expr, &mut scan_columns, &mut seen);
                }
            }
        } else if iface.metrics.contains_key(measure_name) {
            // Metric — collect constituent measure columns from this binding.
            let lowered = decomposer::decompose_metric(
                measure_name,
                iface.metrics.get(measure_name).unwrap(),
                iface,
                &phys_resolver,
                5,
            )?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut seen);
            }
        }
    }

    // Add join key columns from relationships that reference this binding.
    for rel in relationships {
        if rel.from == binding.dataset_name {
            for col_pair in &rel.columns {
                let phys = mapping
                    .physical
                    .get(&col_pair.from)
                    .cloned()
                    .unwrap_or_else(|| col_pair.from.clone());
                if seen.insert(phys.clone()) {
                    scan_columns.push(phys);
                }
            }
        }
        if rel.to == binding.dataset_name {
            for col_pair in &rel.columns {
                let phys = mapping
                    .physical
                    .get(&col_pair.to)
                    .cloned()
                    .unwrap_or_else(|| col_pair.to.clone());
                if seen.insert(phys.clone()) {
                    scan_columns.push(phys);
                }
            }
        }
    }

    let sem_types = build_semantic_type_map(iface, &mapping.physical);
    Ok(build_scan_node_binding(binding, &scan_columns, &sem_types, pb))
}

/// Build a join condition from a relationship's column pairs.
fn build_join_condition(
    rel: &CompiledRelationship,
    left_binding: &DatasetBinding,
    right_binding: &DatasetBinding,
) -> Expr {
    let left_mapping = &left_binding.column_mapping;
    let right_mapping = &right_binding.column_mapping;

    let conditions: Vec<Expr> = rel
        .columns
        .iter()
        .map(|col_pair| {
            let left_col = left_mapping
                .physical
                .get(&col_pair.from)
                .cloned()
                .unwrap_or_else(|| col_pair.from.clone());
            let right_col = right_mapping
                .physical
                .get(&col_pair.to)
                .cloned()
                .unwrap_or_else(|| col_pair.to.clone());

            Expr::eq(Expr::column(left_col), Expr::column(right_col))
        })
        .collect();

    // AND together multiple join conditions. reduce() handles single-element case.
    conditions
        .into_iter()
        .reduce(Expr::and)
        .unwrap_or_else(|| Expr::boolean(true)) // Fallback: no columns -> trivial join (shouldn't happen)
}

/// Build the full join plan: Join → Rename → Expression → Aggregate → Final Projection.
///
/// The join tree operates on physical column names (join conditions reference physical cols).
/// After the join, Rename maps physical → semantic, then Expression/Aggregate/Projection follow.
fn build_join_plan(
    joinset: &CompiledJoinsetKind,
    request: &ResolvedQueryRequest,
    anchor_idx: usize,
    join_order: &[JoinStep],
    ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    let iface = &joinset.interface;
    let bindings = &joinset.bindings;
    let relationships = &joinset.relationships;
    let anchor = &bindings[anchor_idx];
    let pb = ctx.plan_builder;

    // ── Build join tree (physical columns) ─────────────────────────
    let mut current_plan = build_scan(anchor, iface, relationships, request, pb)?;
    let mut joined_indices: Vec<usize> = vec![anchor_idx];

    for step in join_order {
        let right_binding = &bindings[step.binding_idx];
        let right_scan = build_scan(right_binding, iface, relationships, request, pb)?;
        joined_indices.push(step.binding_idx);

        let rel = &relationships[step.relationship_idx];
        let (left_binding, right_binding_for_cond) = if step.reversed {
            (right_binding, &bindings[joinset.adjacency_index.dataset_index[&rel.to]])
        } else {
            (&bindings[joinset.adjacency_index.dataset_index[&rel.from]], right_binding)
        };

        let condition = build_join_condition(rel, left_binding, right_binding_for_cond);
        let join_type = map_join_type(&rel.join_type);

        let mut join_fields: Vec<Field> = current_plan.meta().output_schema.fields.clone();
        join_fields.extend(right_scan.meta().output_schema.fields.iter().cloned());
        let join_schema = Schema::new(join_fields);

        current_plan = pb.build_join(join_schema, current_plan, right_scan, join_type, condition);
    }

    // ── Partition dimensions ────────────────────────────────────────
    let (metadata_dims, regular_dims) = partition_dimensions_iface(&request.dimensions, iface);
    let (physical_dims, computed_dims) = split_computed_dims(&regular_dims, iface);
    let known_values = collect_known_values(anchor, &metadata_dims);

    // Helper: find the physical column for a semantic name across joined bindings.
    let mapping_for_field = |field_name: &str| -> Option<String> {
        for &idx in &joined_indices {
            if let Some(phys) = bindings[idx].column_mapping.physical.get(field_name) {
                return Some(phys.clone());
            }
        }
        None
    };

    // ── Rename: physical → semantic ─────────────────────────────────
    let mut rename_exprs: Vec<Expr> = Vec::new();
    let mut rename_fields: Vec<Field> = Vec::new();

    // Physical dimensions.
    for dim_name in &physical_dims {
        // Check literal mappings across joined bindings first.
        let mut found_literal = false;
        for &idx in &joined_indices {
            if let Some(lit_val) = bindings[idx].column_mapping.literals.get(dim_name) {
                rename_exprs.push(Expr::string(lit_val.clone()));
                rename_fields.push(Field::new(dim_name.clone(), iface.resolve_dim_type(dim_name)));
                found_literal = true;
                break;
            }
        }
        if found_literal { continue; }

        if let Some(phys) = mapping_for_field(dim_name) {
            rename_exprs.push(Expr::column(phys));
            rename_fields.push(Field::new(dim_name.clone(), iface.resolve_dim_type(dim_name)));
        } else {
            return Err(PlannerError::DimensionNotFound {
                kind: iface.name.clone(),
                dimension: dim_name.clone(),
            });
        }
    }

    // Metadata dimensions.
    for (dim_name, _) in &metadata_dims {
        let value = known_values.get(dim_name).cloned().unwrap_or_default();
        rename_exprs.push(Expr::string(value));
        rename_fields.push(Field::new(dim_name.clone(), iface.resolve_dim_type(dim_name)));
    }

    // Computed dim dep columns.
    for (_, expr) in &computed_dims {
        let mut sem_refs: Vec<String> = Vec::new();
        let mut sem_refs_seen: HashSet<String> = HashSet::new();
        collect_column_refs(expr, &mut sem_refs, &mut sem_refs_seen);
        for sem_ref in &sem_refs {
            if !rename_fields.iter().any(|f| f.name == *sem_ref) {
                if let Some(phys) = mapping_for_field(sem_ref) {
                    rename_exprs.push(Expr::column(phys));
                    rename_fields.push(Field::new(
                        sem_ref.clone(),
                        resolve_semantic_type(sem_ref, iface),
                    ));
                }
            }
        }
    }

    // Measure source columns.
    for measure_name in &request.measures {
        if let Some(measure) = iface.measures.get(measure_name) {
            let mut refs = Vec::new();
            let mut seen = HashSet::new();
            collect_semantic_refs(&measure.expr, &mut refs, &mut seen);
            for sem_ref in &refs {
                if !rename_fields.iter().any(|f| f.name == *sem_ref) {
                    if let Some(phys) = mapping_for_field(sem_ref) {
                        rename_exprs.push(Expr::column(phys));
                        rename_fields.push(Field::new(
                            sem_ref.clone(),
                            resolve_semantic_type(sem_ref, iface),
                        ));
                    }
                }
            }
            for filter in &measure.filters {
                let mut f_refs = Vec::new();
                let mut f_seen = HashSet::new();
                collect_semantic_refs(&filter.expr, &mut f_refs, &mut f_seen);
                for sem_ref in &f_refs {
                    if !rename_fields.iter().any(|f| f.name == *sem_ref) {
                        if let Some(phys) = mapping_for_field(sem_ref) {
                            rename_exprs.push(Expr::column(phys));
                            rename_fields.push(Field::new(
                                sem_ref.clone(),
                                resolve_semantic_type(sem_ref, iface),
                            ));
                        }
                    }
                }
            }
        } else if let Some(metric) = iface.metrics.get(measure_name) {
            let constituents = super::plan_layers::extract_metric_constituents(metric, iface);
            for cm_name in &constituents {
                if let Some(cm) = iface.measures.get(cm_name) {
                    let mut refs = Vec::new();
                    let mut seen = HashSet::new();
                    collect_semantic_refs(&cm.expr, &mut refs, &mut seen);
                    for sem_ref in &refs {
                        if !rename_fields.iter().any(|f| f.name == *sem_ref) {
                            if let Some(phys) = mapping_for_field(sem_ref) {
                                rename_exprs.push(Expr::column(phys));
                                rename_fields.push(Field::new(
                                    sem_ref.clone(),
                                    resolve_semantic_type(sem_ref, iface),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    let rename_schema = Schema::new(rename_fields);
    let rename = pb.build_project(rename_schema, current_plan, rename_exprs);

    // ── Expression: computed dims (SR-10) ───────────────────────────
    let expr_proj = build_expression_project(&computed_dims, &known_values, rename, iface, pb);

    // ── Aggregate: semantic GROUP BY ──────────────────────────────
    let group_by: Vec<Expr> = request
        .dimensions
        .iter()
        .map(|name| Expr::column(name.clone()))
        .collect();

    // Decompose measures with identity resolver (semantic names after Rename).
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
        } else if let Some(metric) = iface.metrics.get(measure_name) {
            let lowered = decomposer::decompose_metric(
                measure_name, metric, iface, &identity_resolver, 5,
            )?;
            lowered_measures.push((measure_name.clone(), lowered));
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: iface.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    // Deduplicate aggregates: metrics and direct measures may share
    // constituents, producing identical aggregates in the semantic domain.
    let mut aggregates: Vec<AggregateMeasure> = Vec::new();
    let mut agg_seen: HashSet<String> = HashSet::new();
    for (_, lowered) in &lowered_measures {
        for agg_m in &lowered.aggregates {
            let dedup_key = format!("{:?}|{:?}|{}", agg_m.function, agg_m.expr, agg_m.distinct);
            if agg_seen.insert(dedup_key) {
                aggregates.push(agg_m.clone());
            }
        }
    }

    // Aggregate schema: all dimensions + deduplicated measure outputs.
    let mut agg_fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), iface.resolve_dim_type(name)))
        .collect();
    let mut named_aggs: HashSet<String> = HashSet::new();
    for agg_m in &aggregates {
        let field_name = match &agg_m.expr {
            Expr::Column(col) if !named_aggs.contains(&col.name) => {
                named_aggs.insert(col.name.clone());
                col.name.clone()
            }
            _ => format!("__agg_{}", agg_fields.len()),
        };
        let data_type = if let Some(m) = iface.measures.get(&field_name) {
            m.data_type.clone()
        } else {
            agg_m.data_type.clone()
        };
        agg_fields.push(Field::new(field_name, data_type));
    }
    let agg_schema = Schema::new(agg_fields);
    let agg = pb.build_aggregate(agg_schema, expr_proj, group_by, aggregates);

    // ── Final Projection ───────────────────────────────────────────
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
    let project = pb.build_project(project_schema.clone(), agg, project_exprs);

    Ok(PlanFragment { root: project })
}

/// Get the relationship at a given index from the manifest's top-level relationships.
#[allow(dead_code)] // Used during ad-hoc join plan construction
pub fn get_relationship(
    manifest: &CompiledManifest,
    index: usize,
) -> Option<&CompiledRelationship> {
    manifest.relationships.get(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::PlannerContext;
    use crate::tests::helpers::*;
    use indexmap::IndexMap;
    use semstrait_ir::Aggregation;
    use semstrait_manifest::{
        Cardinality, CompiledDimension, CompiledMeasure, CompiledRelationship, JoinAssociativity, JoinColumnPair, JoinType as ModelJoinType,
    };
    use semstrait_manifest::acceleration::{
        AdjacencyIndex, CoverageIndex, DatasetBinding, DimensionIndex,
        CompiledJoinsetKind, ResolvedColumnMapping,
    };
    use std::collections::HashMap;

    // ── Test helpers ─────────────────────────────────────────────────

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
                agg: Aggregation::Sum,
                expr: semstrait_core::Expr::entity_ref(expr_ref),
                expr_source: format!("SUM({})", expr_ref),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        )
    }

    fn make_legacy_measure(name: &str, expr_str: &str) -> (String, CompiledMeasure) {
        (
            name.to_string(),
            CompiledMeasure {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Number,
                agg: Aggregation::Sum,
                expr: semstrait_core::Expr::entity_ref(expr_str),
                expr_source: expr_str.to_string(),
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

    fn make_joinset(
        name: &str,
        dimensions: IndexMap<String, CompiledDimension>,
        measures: IndexMap<String, CompiledMeasure>,
        bindings: Vec<DatasetBinding>,
        relationships: Vec<CompiledRelationship>,
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
        let adjacency_index = AdjacencyIndex::build(&bindings, &relationships);

        CompiledDataKind::Joinset(Box::new(CompiledJoinsetKind {
            interface: iface,
            associativity: JoinAssociativity::Left,
            bindings,
            relationships,
            coverage_index: coverage,
            dimension_index,
            metric_order: None,
            adjacency_index,
        }))
    }

    // ── Tests ────────────────────────────────────────────────────────

    #[test]
    fn test_joinset_two_datasets() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("order_date"),
            make_categorical_dim("customer_name"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_legacy_measure("revenue", "SUM(amount)"),
        ].into_iter().collect();

        let ds_orders = make_binding("orders", vec![
            ("order_date", "created_at"),
            ("revenue", "amount"),
            ("customer_id", "customer_id"),
        ]);
        let ds_customers = make_binding("customers", vec![
            ("customer_name", "name"),
            ("id", "id"),
        ]);

        let relationship = CompiledRelationship {
            name: "orders_customers".to_string(),
            from: "orders".to_string(),
            to: "customers".to_string(),
            join_type: ModelJoinType::Left,
            columns: vec![JoinColumnPair {
                from: "customer_id".to_string(),
                to: "id".to_string(),
            }],
            cardinality: Cardinality::ManyToOne,
        };

        let data_kind = make_joinset(
            "order_details",
            dimensions,
            measures,
            vec![ds_orders, ds_customers],
            vec![relationship],
        );

        let request = make_test_request("order_details", vec!["order_date", "customer_name"], vec!["revenue"]);
        let manifest = empty_manifest();
        let session = HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = JoinsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "joinset resolve should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Root: Project -> Aggregate -> Project (Rename) -> Join -> (Scan, Scan).
        match &fragment.root {
            PlanNode::Project(proj) => {
                match proj.input.as_ref() {
                    PlanNode::Aggregate(agg) => {
                        // Rename project sits between Aggregate and Join.
                        let rename = match agg.input.as_ref() {
                            PlanNode::Project(p) => p,
                            _ => panic!("expected Project (Rename) under Aggregate"),
                        };
                        match rename.input.as_ref() {
                            PlanNode::Join(join) => {
                                assert!(matches!(*join.left, PlanNode::Scan(_)));
                                assert!(matches!(*join.right, PlanNode::Scan(_)));
                                assert_eq!(join.join_type, IrJoinType::Left);
                            }
                            _ => panic!("expected Join under Rename"),
                        }
                    }
                    _ => panic!("expected Aggregate node under Project"),
                }
            }
            _ => panic!("expected Project node as root"),
        }

        // Output schema should have 3 fields.
        assert_eq!(fragment.root.meta().output_schema.fields.len(), 3);
    }

    #[test]
    fn test_joinset_single_dataset() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("order_date"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_legacy_measure("revenue", "SUM(amount)"),
        ].into_iter().collect();

        let ds_orders = make_binding("orders", vec![
            ("order_date", "created_at"),
            ("revenue", "amount"),
        ]);

        let data_kind = make_joinset(
            "order_details",
            dimensions,
            measures,
            vec![ds_orders],
            vec![],
        );

        let request = make_test_request("order_details", vec!["order_date"], vec!["revenue"]);
        let manifest = empty_manifest();
        let session = HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = JoinsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok());

        let fragment = result.unwrap();
        // Single dataset -> Aggregate (Final Projection skipped) or Project -> Aggregate -> Project (Rename) -> Scan.
        let agg_node = match &fragment.root {
            PlanNode::Aggregate(a) => a, // identity Final Projection skipped
            PlanNode::Project(proj) => match proj.input.as_ref() {
                PlanNode::Aggregate(a) => a,
                _ => panic!("expected Aggregate under Project"),
            },
            _ => panic!("expected Aggregate or Project as root"),
        };
        assert!(
            matches!(agg_node.input.as_ref(), PlanNode::Project(_)),
            "Aggregate input should be Project (Rename), got {:?}",
            std::mem::discriminant(agg_node.input.as_ref())
        );
    }

    #[test]
    fn test_joinset_bfs_order() {
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("order_date"),
            make_categorical_dim("customer_name"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let ds_orders = make_binding("orders", vec![
            ("order_date", "created_at"),
            ("revenue", "amount"),
            ("customer_id", "customer_id"),
        ]);
        let ds_customers = make_binding("customers", vec![
            ("customer_name", "name"),
            ("id", "id"),
        ]);

        let relationship = CompiledRelationship {
            name: "orders_customers".to_string(),
            from: "orders".to_string(),
            to: "customers".to_string(),
            join_type: ModelJoinType::Left,
            columns: vec![JoinColumnPair {
                from: "customer_id".to_string(),
                to: "id".to_string(),
            }],
            cardinality: Cardinality::ManyToOne,
        };

        let bindings = vec![ds_orders, ds_customers];
        let relationships = vec![relationship];

        let request = make_test_request("order_details", vec!["order_date"], vec!["revenue"]);
        let data_kind = make_joinset("order_details", dimensions, measures, bindings.clone(), relationships.clone());
        let pruned = super::PrunedView::all(&data_kind);
        let iface = data_kind.interface();

        let anchor_idx = find_anchor_index(iface, &bindings, &pruned, &request).unwrap();
        assert_eq!(anchor_idx, 0); // orders covers order_date + revenue
        assert_eq!(bindings[anchor_idx].dataset_name, "orders");

        let adjacency = AdjacencyIndex::build(&bindings, &relationships);
        let steps = bfs_join_order(anchor_idx, &adjacency, &pruned);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].binding_idx, 1); // customers
        assert!(!steps[0].reversed);
    }

    #[test]
    fn test_joinset_no_relationships_error() {
        let dimensions: IndexMap<String, CompiledDimension> = IndexMap::new();
        let measures: IndexMap<String, CompiledMeasure> = IndexMap::new();

        let ds_a = make_binding("a", vec![]);
        let ds_b = make_binding("b", vec![]);

        let data_kind = make_joinset("broken", dimensions, measures, vec![ds_a, ds_b], vec![]);
        let request = make_test_request("broken", vec![], vec![]);
        let manifest = empty_manifest();
        let session = HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = JoinsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_joinset_multi_hop() {
        // Three datasets: orders -> customers -> regions.
        let dimensions: IndexMap<_, _> = vec![
            make_categorical_dim("order_date"),
            make_categorical_dim("region_name"),
        ].into_iter().collect();

        let measures: IndexMap<_, _> = vec![
            make_sum_measure("revenue", "amount"),
        ].into_iter().collect();

        let ds_orders = make_binding("orders", vec![
            ("order_date", "date"),
            ("revenue", "amount"),
            ("customer_id", "cust_id"),
        ]);
        let ds_customers = make_binding("customers", vec![
            ("id", "id"),
            ("region_id", "region_id"),
        ]);
        let ds_regions = make_binding("regions", vec![
            ("id", "id"),
            ("region_name", "name"),
        ]);

        let rel1 = CompiledRelationship {
            name: "orders_customers".to_string(),
            from: "orders".to_string(),
            to: "customers".to_string(),
            join_type: ModelJoinType::Left,
            columns: vec![JoinColumnPair { from: "customer_id".to_string(), to: "id".to_string() }],
            cardinality: Cardinality::ManyToOne,
        };
        let rel2 = CompiledRelationship {
            name: "customers_regions".to_string(),
            from: "customers".to_string(),
            to: "regions".to_string(),
            join_type: ModelJoinType::Left,
            columns: vec![JoinColumnPair { from: "region_id".to_string(), to: "id".to_string() }],
            cardinality: Cardinality::ManyToOne,
        };

        let data_kind = make_joinset(
            "order_region",
            dimensions,
            measures,
            vec![ds_orders, ds_customers, ds_regions],
            vec![rel1, rel2],
        );

        let request = make_test_request("order_region", vec!["order_date", "region_name"], vec!["revenue"]);
        let manifest = empty_manifest();
        let session = HashMap::new();
        let plan_builder = semstrait_ir::DefaultPlanBuilder;
        let ctx = PlannerContext {
            manifest: &manifest,
            catalog: None,
            session: &session,
            plan_builder: &plan_builder,
        };

        let planner = JoinsetPlanner;
        let pruned = super::PrunedView::all(&data_kind);
        let result = planner.resolve(&pruned, &request, &ctx);
        assert!(result.is_ok(), "multi-hop join should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Root: Project -> Aggregate -> Project (Rename) -> Join -> (Join -> (Scan, Scan), Scan)
        match &fragment.root {
            PlanNode::Project(proj) => {
                match proj.input.as_ref() {
                    PlanNode::Aggregate(agg) => {
                        let rename = match agg.input.as_ref() {
                            PlanNode::Project(p) => p,
                            _ => panic!("expected Project (Rename) under Aggregate"),
                        };
                        match rename.input.as_ref() {
                            PlanNode::Join(outer_join) => {
                                assert!(
                                    matches!(*outer_join.left, PlanNode::Join(_)),
                                    "left of outer join should be inner join"
                                );
                                assert!(matches!(*outer_join.right, PlanNode::Scan(_)));
                            }
                            _ => panic!("expected Join under Rename"),
                        }
                    }
                    _ => panic!("expected Aggregate under Project"),
                }
            }
            _ => panic!("expected Project as root"),
        }
    }

}
