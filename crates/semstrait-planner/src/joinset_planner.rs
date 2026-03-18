//! JoinsetPlanner — kind planner for Joinset kinds.
//!
//! Uses BFS from an anchor dataset to construct a join chain.
//! The anchor is the dataset that covers the most requested fields.
//! Other datasets are joined in order of the defined relationships.

use crate::error::PlannerError;
use crate::expr_lower;
use crate::grainset_planner::collect_column_refs;
use crate::kind_planner::{resolve_column_name, KindPlanner, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, AggregateMeasure, DslExpr, Field, JoinNode, JoinType as IrJoinType, NodeMeta,
    PlanNode, ProjectNode, ScanNode, Schema,
};
use semstrait_manifest::{
    CompiledKind, CompiledKindDataset, CompiledKindType, CompiledRelationship,
    JoinType as ModelJoinType,
};
use std::collections::{HashMap, HashSet, VecDeque};

/// Planner for Joinset kinds — BFS-based join chain construction.
pub struct JoinsetPlanner;

impl KindPlanner for JoinsetPlanner {
    fn supports(&self, kind_type: &CompiledKindType) -> bool {
        matches!(kind_type, CompiledKindType::Joinset { .. })
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
                reason: "joinset kind has no datasets".to_string(),
            });
        }

        if kind.relationships.is_empty() && kind.datasets.len() > 1 {
            return Err(PlannerError::NoCoveringDataset {
                kind: kind.name.clone(),
                reason: "joinset kind has multiple datasets but no relationships".to_string(),
            });
        }

        // If single dataset, delegate to simple scan-aggregate-project (like grainset).
        if kind.datasets.len() == 1 {
            return build_single_dataset_plan(kind, request, &kind.datasets[0], ctx);
        }

        // Find the anchor dataset (covers most requested fields).
        let anchor = find_anchor_dataset(kind, request)?;

        // BFS from anchor through relationships to build join order.
        let join_order = bfs_join_order(anchor, &kind.datasets, &kind.relationships);

        // Build the join tree.
        build_join_plan(kind, request, &join_order, ctx)
    }
}

/// Map model JoinType to IR JoinType.
fn map_join_type(jt: &ModelJoinType) -> IrJoinType {
    match jt {
        ModelJoinType::Inner => IrJoinType::Inner,
        ModelJoinType::Left => IrJoinType::Left,
        ModelJoinType::Right => IrJoinType::Right,
        ModelJoinType::Full => IrJoinType::Full,
    }
}

/// Find the anchor dataset — the one covering the most requested fields.
fn find_anchor_dataset<'a>(
    kind: &'a CompiledKind,
    request: &ResolvedQueryRequest,
) -> Result<&'a CompiledKindDataset, PlannerError> {
    let needed: Vec<&str> = request
        .dimensions
        .iter()
        .chain(request.measures.iter())
        .map(|s| s.as_str())
        .collect();

    kind.datasets
        .iter()
        .max_by_key(|ds| {
            needed
                .iter()
                .filter(|name| ds.extras.column_mapping.contains_key(**name))
                .count()
        })
        .ok_or_else(|| PlannerError::NoCoveringDataset {
            kind: kind.name.clone(),
            reason: "no datasets available for anchor selection".to_string(),
        })
}

/// A step in the join order: the dataset to join and the relationship to use.
struct JoinStep<'a> {
    dataset: &'a CompiledKindDataset,
    relationship: &'a CompiledRelationship,
    /// Whether this step joins "from" side to existing tree (true) or "to" side (false).
    reversed: bool,
}

/// BFS from the anchor dataset through relationships to determine join order.
fn bfs_join_order<'a>(
    anchor: &'a CompiledKindDataset,
    datasets: &'a [CompiledKindDataset],
    relationships: &'a [CompiledRelationship],
) -> Vec<JoinStep<'a>> {
    let ds_map: HashMap<&str, &CompiledKindDataset> =
        datasets.iter().map(|ds| (ds.name.as_str(), ds)).collect();

    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(&anchor.name);

    let mut queue: VecDeque<&str> = VecDeque::new();
    queue.push_back(&anchor.name);

    let mut steps: Vec<JoinStep<'a>> = Vec::new();

    while let Some(current) = queue.pop_front() {
        for rel in relationships {
            if rel.from == current && !visited.contains(rel.to.as_str()) {
                visited.insert(&rel.to);
                if let Some(ds) = ds_map.get(rel.to.as_str()) {
                    steps.push(JoinStep {
                        dataset: ds,
                        relationship: rel,
                        reversed: false,
                    });
                    queue.push_back(&rel.to);
                }
            } else if rel.to == current && !visited.contains(rel.from.as_str()) {
                visited.insert(&rel.from);
                if let Some(ds) = ds_map.get(rel.from.as_str()) {
                    steps.push(JoinStep {
                        dataset: ds,
                        relationship: rel,
                        reversed: true,
                    });
                    queue.push_back(&rel.from);
                }
            }
        }
    }

    steps
}

/// Build a scan node for a dataset.
fn build_scan(
    dataset: &CompiledKindDataset,
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    ctx: &PlannerContext<'_>,
) -> Result<PlanNode, PlannerError> {
    let mapping = &dataset.extras.column_mapping;
    let mut scan_columns: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Add dimension columns this dataset covers.
    for dim_name in &request.dimensions {
        if let Some(mv) = mapping.get(dim_name) {
            let phys = resolve_column_name(mv).to_string();
            if seen.insert(phys.clone()) {
                scan_columns.push(phys);
            }
        }
    }

    // Add measure columns this dataset covers.
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            if mapping.contains_key(measure_name) {
                let lowered = expr_lower::lower_measure_with_filters(
                    measure_name,
                    &measure.expr,
                    mapping,
                    &measure.filters,
                )?;
                for agg in &lowered.aggregates {
                    collect_column_refs(&agg.expr, &mut scan_columns, &mut seen);
                }
            }
        }
    }

    // Add join key columns from relationships.
    for rel in &kind.relationships {
        if rel.from == dataset.name {
            for col_pair in &rel.columns {
                let phys = mapping
                    .get(&col_pair.from)
                    .map(|mv| resolve_column_name(mv).to_string())
                    .unwrap_or_else(|| col_pair.from.clone());
                if seen.insert(phys.clone()) {
                    scan_columns.push(phys);
                }
            }
        }
        if rel.to == dataset.name {
            for col_pair in &rel.columns {
                let phys = mapping
                    .get(&col_pair.to)
                    .map(|mv| resolve_column_name(mv).to_string())
                    .unwrap_or_else(|| col_pair.to.clone());
                if seen.insert(phys.clone()) {
                    scan_columns.push(phys);
                }
            }
        }
    }

    let table_name = if let Some(ds) = ctx.manifest.get_dataset(&dataset.name) {
        ds.name.clone()
    } else {
        dataset.name.clone()
    };

    let scan_schema = Schema::new(
        scan_columns
            .iter()
            .map(|c| Field::new(c.clone(), DataType::Utf8))
            .collect(),
    );

    Ok(PlanNode::Scan(ScanNode {
        meta: NodeMeta::new(scan_schema),
        table_name,
        projection: scan_columns,
    }))
}

/// Build a join condition from a relationship's column pairs.
fn build_join_condition(
    rel: &CompiledRelationship,
    left_ds: &str,
    right_ds: &str,
    datasets: &[CompiledKindDataset],
) -> DslExpr {
    let left_mapping = datasets
        .iter()
        .find(|ds| ds.name == left_ds)
        .map(|ds| &ds.extras.column_mapping);
    let right_mapping = datasets
        .iter()
        .find(|ds| ds.name == right_ds)
        .map(|ds| &ds.extras.column_mapping);

    let conditions: Vec<DslExpr> = rel
        .columns
        .iter()
        .map(|col_pair| {
            let left_col = left_mapping
                .and_then(|m| m.get(&col_pair.from))
                .map(|mv| resolve_column_name(mv).to_string())
                .unwrap_or_else(|| col_pair.from.clone());
            let right_col = right_mapping
                .and_then(|m| m.get(&col_pair.to))
                .map(|mv| resolve_column_name(mv).to_string())
                .unwrap_or_else(|| col_pair.to.clone());

            DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column {
                    name: left_col,
                    qualifier: None,
                }),
                op: semstrait_ir::BinaryOp::Eq,
                right: Box::new(DslExpr::Column {
                    name: right_col,
                    qualifier: None,
                }),
            }
        })
        .collect();

    // AND together multiple join conditions. reduce() handles single-element case.
    conditions
        .into_iter()
        .reduce(|acc, cond| DslExpr::BinaryOp {
            left: Box::new(acc),
            op: semstrait_ir::BinaryOp::And,
            right: Box::new(cond),
        })
        .unwrap_or(DslExpr::Bool(true)) // Fallback: no columns → trivial join (shouldn't happen)
}

/// Build the full join plan: join tree -> aggregate -> project.
fn build_join_plan(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    join_order: &[JoinStep<'_>],
    ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    let anchor = find_anchor_dataset(kind, request)?;

    // Build the anchor scan.
    let mut current_plan = build_scan(anchor, kind, request, ctx)?;

    // Track all datasets in the join tree for schema building.
    let mut joined_datasets: Vec<&str> = vec![&anchor.name];

    // Join each step.
    for step in join_order {
        let right_scan = build_scan(step.dataset, kind, request, ctx)?;
        joined_datasets.push(&step.dataset.name);

        let (left_ds, right_ds) = if step.reversed {
            (&step.dataset.name, &step.relationship.from)
        } else {
            (&step.relationship.from, &step.relationship.to)
        };

        // Build the join condition using actual dataset names for column lookup.
        let condition = build_join_condition(
            step.relationship,
            left_ds.as_str(),
            right_ds.as_str(),
            &kind.datasets,
        );

        let join_type = map_join_type(&step.relationship.join_type);

        // Compute join output schema (left fields + right fields).
        let mut join_fields: Vec<Field> = current_plan.meta().output_schema.fields.clone();
        join_fields.extend(right_scan.meta().output_schema.fields.iter().cloned());
        let join_schema = Schema::new(join_fields);

        current_plan = PlanNode::Join(JoinNode {
            meta: NodeMeta::new(join_schema),
            left: Box::new(current_plan),
            right: Box::new(right_scan),
            join_type,
            condition,
        });
    }

    // Now build Aggregate -> Project over the joined result.
    let mapping_for_field = |field_name: &str| -> Option<String> {
        for ds_name in &joined_datasets {
            if let Some(ds) = kind.datasets.iter().find(|d| d.name == *ds_name) {
                if let Some(mv) = ds.extras.column_mapping.get(field_name) {
                    return Some(resolve_column_name(mv).to_string());
                }
            }
        }
        None
    };

    // Build group_by for dimensions.
    let group_by: Vec<DslExpr> = request
        .dimensions
        .iter()
        .filter_map(|dim| {
            mapping_for_field(dim).map(|phys| DslExpr::Column {
                name: phys,
                qualifier: None,
            })
        })
        .collect();

    // Lower measures.
    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            // Find which dataset provides this measure.
            let ds_mapping = joined_datasets
                .iter()
                .find_map(|ds_name| {
                    kind.datasets
                        .iter()
                        .find(|d| d.name == *ds_name)
                        .filter(|d| d.extras.column_mapping.contains_key(measure_name))
                        .map(|d| &d.extras.column_mapping)
                });

            if let Some(mapping) = ds_mapping {
                let lowered = expr_lower::lower_measure_with_filters(
                    measure_name,
                    &measure.expr,
                    mapping,
                    &measure.filters,
                )?;
                lowered_measures.push((measure_name.clone(), lowered));
            } else {
                return Err(PlannerError::MeasureNotFound {
                    kind: kind.name.clone(),
                    measure: measure_name.clone(),
                });
            }
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: kind.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .flat_map(|(_, lowered)| lowered.aggregates.clone())
        .collect();

    // Aggregate schema.
    let mut agg_fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), DataType::Utf8))
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
        input: Box::new(current_plan),
        group_by,
        aggregates,
    });

    // Project node — maps to semantic names.
    let mut project_exprs: Vec<DslExpr> = request
        .dimensions
        .iter()
        .map(|name| DslExpr::Column {
            name: name.clone(),
            qualifier: None,
        })
        .collect();
    for (_, lowered) in &lowered_measures {
        project_exprs.push(lowered.post_agg_expr.clone());
    }

    let project_fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), DataType::Utf8))
        .chain(
            lowered_measures
                .iter()
                .map(|(name, _)| Field::new(name.clone(), DataType::Float64)),
        )
        .collect();
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

/// Build plan for a single-dataset joinset (degenerates to scan-aggregate-project).
fn build_single_dataset_plan(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    dataset: &CompiledKindDataset,
    ctx: &PlannerContext<'_>,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &dataset.extras.column_mapping;

    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    let mut dim_physical: Vec<(String, String)> = Vec::new();
    for dim_name in &request.dimensions {
        let physical = mapping
            .get(dim_name)
            .map(resolve_column_name)
            .ok_or_else(|| PlannerError::DimensionNotFound {
                kind: kind.name.clone(),
                dimension: dim_name.clone(),
            })?;
        let phys = physical.to_string();
        dim_physical.push((dim_name.clone(), phys.clone()));
        if scan_seen.insert(phys.clone()) {
            scan_columns.push(phys);
        }
    }

    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            let lowered = expr_lower::lower_measure_with_filters(
                measure_name,
                &measure.expr,
                mapping,
                &measure.filters,
            )?;
            for agg in &lowered.aggregates {
                collect_column_refs(&agg.expr, &mut scan_columns, &mut scan_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else {
            return Err(PlannerError::MeasureNotFound {
                kind: kind.name.clone(),
                measure: measure_name.clone(),
            });
        }
    }

    let table_name = if let Some(ds) = ctx.manifest.get_dataset(&dataset.name) {
        &ds.name
    } else {
        &dataset.name
    };

    let scan_schema = Schema::new(
        scan_columns
            .iter()
            .map(|c| Field::new(c.clone(), DataType::Utf8))
            .collect(),
    );
    let scan = PlanNode::Scan(ScanNode {
        meta: NodeMeta::new(scan_schema),
        table_name: table_name.to_string(),
        projection: scan_columns,
    });

    let group_by: Vec<DslExpr> = dim_physical
        .iter()
        .map(|(_, physical)| DslExpr::Column {
            name: physical.clone(),
            qualifier: None,
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

    let mut project_exprs: Vec<DslExpr> = request
        .dimensions
        .iter()
        .map(|name| DslExpr::Column {
            name: name.clone(),
            qualifier: None,
        })
        .collect();
    for (_, lowered) in &lowered_measures {
        project_exprs.push(lowered.post_agg_expr.clone());
    }

    let project_fields: Vec<Field> = request
        .dimensions
        .iter()
        .map(|name| Field::new(name.clone(), DataType::Utf8))
        .chain(
            lowered_measures
                .iter()
                .map(|(name, _)| Field::new(name.clone(), DataType::Float64)),
        )
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind_planner::PlannerContext;
    use crate::test_helpers::*;
    use indexmap::IndexMap;
    use semstrait_manifest::{
        Cardinality, CompiledKind, CompiledKindDataset, CompiledKindType, CompiledMeasure,
        CompiledRelationship, ColumnMappingValue, JoinAssociativity, JoinColumnPair,
        JoinType as ModelJoinType, KindDatasetExtras,
    };
    use std::collections::HashMap;

    fn make_joinset_manifest() -> semstrait_manifest::CompiledManifest {
        let mut dimensions = IndexMap::new();
        for name in &["order_date", "customer_name"] {
            dimensions.insert(
                name.to_string(),
                semstrait_manifest::CompiledDimension {
                    name: name.to_string(),
                    description: None,
                    data_type: "string".to_string(),
                    dim_type: semstrait_manifest::DimensionType::Categorical(
                        semstrait_manifest::CategoricalDimension { enum_values: None },
                    ),
                },
            );
        }

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                expr: semstrait_core::DslExpr::entity_ref("SUM(amount)"),
                expr_source: "SUM(amount)".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        // Dataset 1: orders — has order_date and revenue.
        let mut mapping1 = HashMap::new();
        mapping1.insert(
            "order_date".to_string(),
            ColumnMappingValue::Simple("created_at".to_string()),
        );
        mapping1.insert(
            "revenue".to_string(),
            ColumnMappingValue::Simple("amount".to_string()),
        );
        mapping1.insert(
            "customer_id".to_string(),
            ColumnMappingValue::Simple("customer_id".to_string()),
        );

        let ds_orders = CompiledKindDataset {
            name: "orders".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping1,
                temporal: None,
                storage: None,
                catalog: None,
            },
        };

        // Dataset 2: customers — has customer_name.
        let mut mapping2 = HashMap::new();
        mapping2.insert(
            "customer_name".to_string(),
            ColumnMappingValue::Simple("name".to_string()),
        );
        mapping2.insert(
            "id".to_string(),
            ColumnMappingValue::Simple("id".to_string()),
        );

        let ds_customers = CompiledKindDataset {
            name: "customers".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping2,
                temporal: None,
                storage: None,
                catalog: None,
            },
        };

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

        let kind = CompiledKind {
            name: "order_details".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Joinset {
                associativity: JoinAssociativity::Left,
            },
            datasets: vec![ds_orders, ds_customers],
            relationships: vec![relationship],
            domain: None,
        };

        let mut kinds = IndexMap::new();
        kinds.insert("order_details".to_string(), kind);

        semstrait_manifest::CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test_model".to_string(),
            model_description: None,
        }
    }

    #[test]
    fn test_joinset_two_datasets() {
        let manifest = make_joinset_manifest();
        let kind = manifest.get_kind("order_details").unwrap();
        let request =
            make_test_request("order_details", vec!["order_date", "customer_name"], vec!["revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = JoinsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
        assert!(result.is_ok(), "joinset resolve should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Root should be Project -> Aggregate -> Join -> (Scan, Scan).
        match &fragment.root {
            PlanNode::Project(proj) => {
                match proj.input.as_ref() {
                    PlanNode::Aggregate(agg) => {
                        match agg.input.as_ref() {
                            PlanNode::Join(join) => {
                                assert!(matches!(*join.left, PlanNode::Scan(_)));
                                assert!(matches!(*join.right, PlanNode::Scan(_)));
                                assert_eq!(join.join_type, IrJoinType::Left);
                            }
                            _ => panic!("expected Join node under Aggregate"),
                        }
                    }
                    _ => panic!("expected Aggregate node under Project"),
                }
            }
            _ => panic!("expected Project node as root"),
        }

        // Output schema should have 3 fields.
        assert_eq!(fragment.output_schema.fields.len(), 3);
    }

    #[test]
    fn test_joinset_single_dataset() {
        let manifest = make_joinset_manifest();
        let mut kind = manifest.get_kind("order_details").unwrap().clone();
        kind.datasets.truncate(1);
        kind.relationships.clear();

        let request = make_test_request("order_details", vec!["order_date"], vec!["revenue"]);
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = JoinsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_ok());

        let fragment = result.unwrap();
        // Single dataset → Project -> Aggregate -> Scan (no Join).
        match &fragment.root {
            PlanNode::Project(proj) => {
                match proj.input.as_ref() {
                    PlanNode::Aggregate(agg) => {
                        assert!(matches!(agg.input.as_ref(), PlanNode::Scan(_)));
                    }
                    _ => panic!("expected Aggregate under Project"),
                }
            }
            _ => panic!("expected Project as root"),
        }
    }

    #[test]
    fn test_joinset_bfs_order() {
        let manifest = make_joinset_manifest();
        let kind = manifest.get_kind("order_details").unwrap();
        let request = make_test_request("order_details", vec!["order_date"], vec!["revenue"]);

        let anchor = find_anchor_dataset(kind, &request).unwrap();
        assert_eq!(anchor.name, "orders"); // orders covers order_date + revenue

        let steps = bfs_join_order(anchor, &kind.datasets, &kind.relationships);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].dataset.name, "customers");
        assert!(!steps[0].reversed);
    }

    #[test]
    fn test_joinset_no_relationships_error() {
        let kind = CompiledKind {
            name: "broken".to_string(),
            description: None,
            dimensions: IndexMap::new(),
            measures: IndexMap::new(),
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Joinset {
                associativity: JoinAssociativity::Left,
            },
            datasets: vec![
                CompiledKindDataset {
                    name: "a".to_string(),
                    extras: KindDatasetExtras {
                        column_mapping: HashMap::new(),
                        temporal: None,
                        storage: None,
                        catalog: None,
                    },
                },
                CompiledKindDataset {
                    name: "b".to_string(),
                    extras: KindDatasetExtras {
                        column_mapping: HashMap::new(),
                        temporal: None,
                        storage: None,
                        catalog: None,
                    },
                },
            ],
            relationships: vec![],
            domain: None,
        };

        let request = make_test_request("broken", vec![], vec![]);
        let manifest = semstrait_manifest::CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test".to_string(),
            datasets: IndexMap::new(),
            kinds: IndexMap::new(),
            relationships: vec![],
            model_name: "test".to_string(),
            model_description: None,
        };
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = JoinsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_joinset_multi_hop() {
        // Three datasets: orders -> customers -> regions.
        let mut dimensions = IndexMap::new();
        for name in &["order_date", "region_name"] {
            dimensions.insert(
                name.to_string(),
                semstrait_manifest::CompiledDimension {
                    name: name.to_string(),
                    description: None,
                    data_type: "string".to_string(),
                    dim_type: semstrait_manifest::DimensionType::Categorical(
                        semstrait_manifest::CategoricalDimension { enum_values: None },
                    ),
                },
            );
        }

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                expr: semstrait_core::DslExpr::entity_ref("SUM(amount)"),
                expr_source: "SUM(amount)".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        let mut map_orders = HashMap::new();
        map_orders.insert("order_date".to_string(), ColumnMappingValue::Simple("date".to_string()));
        map_orders.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));
        map_orders.insert("customer_id".to_string(), ColumnMappingValue::Simple("cust_id".to_string()));

        let mut map_customers = HashMap::new();
        map_customers.insert("id".to_string(), ColumnMappingValue::Simple("id".to_string()));
        map_customers.insert("region_id".to_string(), ColumnMappingValue::Simple("region_id".to_string()));

        let mut map_regions = HashMap::new();
        map_regions.insert("id".to_string(), ColumnMappingValue::Simple("id".to_string()));
        map_regions.insert("region_name".to_string(), ColumnMappingValue::Simple("name".to_string()));

        let ds_orders = CompiledKindDataset {
            name: "orders".to_string(),
            extras: KindDatasetExtras { column_mapping: map_orders, temporal: None, storage: None, catalog: None },
        };
        let ds_customers = CompiledKindDataset {
            name: "customers".to_string(),
            extras: KindDatasetExtras { column_mapping: map_customers, temporal: None, storage: None, catalog: None },
        };
        let ds_regions = CompiledKindDataset {
            name: "regions".to_string(),
            extras: KindDatasetExtras { column_mapping: map_regions, temporal: None, storage: None, catalog: None },
        };

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

        let kind = CompiledKind {
            name: "order_region".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Joinset { associativity: JoinAssociativity::Left },
            datasets: vec![ds_orders, ds_customers, ds_regions],
            relationships: vec![rel1, rel2],
            domain: None,
        };

        let request = make_test_request("order_region", vec!["order_date", "region_name"], vec!["revenue"]);

        let mut kinds = IndexMap::new();
        kinds.insert("order_region".to_string(), kind.clone());
        let manifest = semstrait_manifest::CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test".to_string(),
            model_description: None,
        };

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = JoinsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_ok(), "multi-hop join should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Root: Project -> Aggregate -> Join -> (Join -> (Scan, Scan), Scan)
        match &fragment.root {
            PlanNode::Project(proj) => {
                match proj.input.as_ref() {
                    PlanNode::Aggregate(agg) => {
                        match agg.input.as_ref() {
                            PlanNode::Join(outer_join) => {
                                // Left side should be another join.
                                assert!(
                                    matches!(*outer_join.left, PlanNode::Join(_)),
                                    "left of outer join should be inner join"
                                );
                                // Right side should be a scan.
                                assert!(matches!(*outer_join.right, PlanNode::Scan(_)));
                            }
                            _ => panic!("expected Join under Aggregate"),
                        }
                    }
                    _ => panic!("expected Aggregate under Project"),
                }
            }
            _ => panic!("expected Project as root"),
        }
    }
}
