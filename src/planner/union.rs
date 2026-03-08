//! Union-based planning: conformed, qualified, and virtual-only queries

use std::collections::HashSet;
use crate::semantic_model::{Dataset, GrainSet, MeasureExpr, MetricExpr, Schema, SemanticModel};
use crate::plan::{
    Aggregate, AggregateExpr, Column, Expr, Join, JoinType,
    Literal, PlanNode, Scan, Project, ProjectExpr, Union,
    VirtualTable, LiteralValue as PlanLiteralValue,
};
use crate::resolver::{resolve_query, collect_required_measure_names};
use crate::selector::{
    SelectedDataset, select_datasets_for_join, select_partial_for_grain_set,
    PartialSelection,
};
use crate::query::QueryRequest;
use super::error::PlanError;
use super::util::{needs_join_for_dimension, ParsedDimensionAttr, get_virtual_attribute_value, get_virtual_attribute_value_with_dataset};
use super::expr::convert_measure_expr;
use super::table::plan_query;
use super::join::plan_same_grain_set_join;

/// Plan a query on conformed dimensions across multiple tableGroups.
///
/// Triggered when all queried dimensions are conformed and there are multiple tableGroups.
/// Special case: virtual-only queries produce a VirtualTable instead.
pub fn plan_conformed_query(
    schema: &Schema,
    model: &SemanticModel,
    request: &QueryRequest,
    dimension_attrs: &[String],
) -> Result<PlanNode, PlanError> {
    let physical_dims: Vec<String> = dimension_attrs.iter()
        .filter(|d| {
            let parts: Vec<&str> = d.split('.').collect();
            if parts.len() != 2 { return true; }
            !model.get_dimension(parts[0])
                .map(|dim| dim.is_virtual())
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let virtual_dims: Vec<String> = dimension_attrs.iter()
        .filter(|d| {
            let parts: Vec<&str> = d.split('.').collect();
            if parts.len() != 2 { return false; }
            model.get_dimension(parts[0])
                .map(|dim| dim.is_virtual())
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let metric_names: Vec<String> = request.metrics.clone().unwrap_or_default();
    let required_measures: Vec<String> = collect_required_measure_names(model, &metric_names);

    if physical_dims.is_empty() && metric_names.is_empty() && !virtual_dims.is_empty() {
        return plan_virtual_only_query(model, &virtual_dims);
    }

    let mut branches: Vec<PlanNode> = Vec::new();
    let grain_sets = model.grain_sets();

    let is_feasible_table = |grain_set: &GrainSet, table: &Dataset| -> bool {
        for dim_attr in &physical_dims {
            let parts: Vec<&str> = dim_attr.split('.').collect();
            if parts.len() != 2 { return false; }
            let (dim_name, attr_name) = (parts[0], parts[1]);
            if let Some(attrs) = table.get_dimension_attributes(dim_name) {
                if !attrs.iter().any(|a| a == attr_name) { return false; }
            } else {
                return false;
            }
        }
        for measure_name in &required_measures {
            if grain_set.get_measure(measure_name).is_none() { return false; }
            if !table.has_measure(measure_name) { return false; }
        }
        true
    };

    for grain_set in &grain_sets {
        // Tier 1: single feasible table
        let feasible_table = grain_set.datasets.iter()
            .find(|t| is_feasible_table(grain_set, t));
        if let Some(table) = feasible_table {
            let selected = SelectedDataset { group: grain_set, dataset: table };
            let resolved = resolve_query(schema, request, &selected, &grain_sets)
                .map_err(|e| PlanError::InvalidQuery(format!(
                    "Query resolution error for tableGroup '{}': {:?}",
                    grain_set.name, e
                )))?;
            let branch = plan_query(&resolved)?;
            branches.push(branch);
            continue;
        }

        // Tier 2: join within this grain set
        let gs_slice = std::slice::from_ref(grain_set);
        if let Ok(multi) = select_datasets_for_join(
            schema, model, gs_slice, dimension_attrs, &required_measures,
        ) {
            if multi.datasets.len() == 1 {
                let selected = SelectedDataset {
                    group: multi.group,
                    dataset: multi.datasets[0].dataset,
                };
                let resolved = resolve_query(schema, request, &selected, &grain_sets)
                    .map_err(|e| PlanError::InvalidQuery(format!(
                        "Query resolution error for tableGroup '{}': {:?}",
                        grain_set.name, e
                    )))?;
                let branch = plan_query(&resolved)?;
                branches.push(branch);
            } else {
                let branch = plan_same_grain_set_join(
                    model, &multi, dimension_attrs, &metric_names,
                )?;
                branches.push(branch);
            }
            continue;
        }

        // Tier 3: partial selection, fill missing with NULL
        if let Some(partial) = select_partial_for_grain_set(
            model, grain_set, dimension_attrs, &required_measures,
        ) {
            let branch = build_partial_union_branch(
                model, &partial, dimension_attrs, &metric_names,
            )?;
            branches.push(branch);
        }
    }

    if branches.is_empty() {
        return Err(PlanError::InvalidQuery(
            "No tableGroup can serve this conformed dimension query".to_string()
        ));
    }
    if branches.len() == 1 {
        return Ok(branches.into_iter().next().unwrap());
    }

    Ok(PlanNode::Union(Union { inputs: branches }))
}

/// Plan a query with dimensions qualified for multiple different tableGroups.
/// Uses the same three-tier policy per grain set as conformed: single table → join → partial + NULL.
pub fn plan_multi_grain_set_query(
    schema: &Schema,
    model: &SemanticModel,
    request: &QueryRequest,
    dimension_attrs: &[String],
    qualified_groups: &HashSet<&str>,
) -> Result<PlanNode, PlanError> {
    let metric_names: Vec<String> = request.metrics.clone().unwrap_or_default();
    let mut branches: Vec<PlanNode> = Vec::new();
    let grain_sets = model.grain_sets();

    let required_measures = collect_required_measure_names(model, &metric_names);
    for grain_set in &grain_sets {
        if !qualified_groups.contains(grain_set.name.as_str()) {
            continue;
        }

        // Tier 1: single feasible table
        let feasible_table = find_feasible_table_for_qualified(
            model, grain_set, dimension_attrs, &required_measures
        );
        if let Some(table) = feasible_table {
            let branch = build_union_branch(
                model, grain_set, table, dimension_attrs, &metric_names,
            )?;
            branches.push(branch);
            continue;
        }

        // Tier 2: join within this grain set
        let gs_slice = std::slice::from_ref(grain_set);
        if let Ok(multi) = select_datasets_for_join(
            schema, model, gs_slice, dimension_attrs, &required_measures,
        ) {
            if multi.datasets.len() == 1 {
                let selected = SelectedDataset {
                    group: multi.group,
                    dataset: multi.datasets[0].dataset,
                };
                let resolved = resolve_query(schema, request, &selected, &grain_sets)
                    .map_err(|e| PlanError::InvalidQuery(format!(
                        "Query resolution error for tableGroup '{}': {:?}",
                        grain_set.name, e
                    )))?;
                let branch = plan_query(&resolved)?;
                branches.push(branch);
            } else {
                let branch = plan_same_grain_set_join(
                    model, &multi, dimension_attrs, &metric_names,
                )?;
                branches.push(branch);
            }
            continue;
        }

        // Tier 3: partial selection, fill missing with NULL
        if let Some(partial) = select_partial_for_grain_set(
            model, grain_set, dimension_attrs, &required_measures,
        ) {
            let branch = build_partial_union_branch(
                model, &partial, dimension_attrs, &metric_names,
            )?;
            branches.push(branch);
        }
    }

    if branches.is_empty() {
        return Err(PlanError::InvalidQuery(
            "No tableGroup can serve this qualified dimension query".to_string()
        ));
    }
    if branches.len() == 1 {
        return Ok(branches.into_iter().next().unwrap());
    }

    Ok(PlanNode::Union(Union { inputs: branches }))
}

/// Plan a query constrained to a single tableGroup (via qualified dimension).
pub fn plan_single_grain_set_query(
    schema: &Schema,
    model: &SemanticModel,
    request: &QueryRequest,
    dimension_attrs: &[String],
    target_group: &str,
) -> Result<PlanNode, PlanError> {
    let grain_sets = model.grain_sets();
    let grain_set = grain_sets.iter()
        .find(|tg| tg.name == target_group)
        .ok_or_else(|| PlanError::InvalidQuery(format!(
            "Grain set '{}' not found in model", target_group
        )))?;

    let metric_names: Vec<String> = request.metrics.clone().unwrap_or_default();
    let required_measures = collect_required_measure_names(model, &metric_names);
    let feasible_table = find_feasible_table_for_qualified(
        model, grain_set, dimension_attrs, &required_measures
    );
    let Some(table) = feasible_table else {
        return Err(PlanError::InvalidQuery(format!(
            "No table in tableGroup '{}' can serve the qualified dimension query",
            target_group
        )));
    };

    let normalized_dims: Vec<String> = dimension_attrs.iter()
        .map(|path| {
            let parts: Vec<&str> = path.split('.').collect();
            if parts.len() >= 3 {
                let path_segments = &parts[0..parts.len() - 2];
                if model.grain_set_under_path(path_segments, target_group) {
                    format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
                } else {
                    path.clone()
                }
            } else {
                path.clone()
            }
        })
        .collect();

    let normalized_request = QueryRequest {
        model: request.model.clone(),
        dimensions: None,
        rows: Some(normalized_dims.iter()
            .filter(|d| request.rows.as_ref().map(|r| r.iter().any(|rd| {
                let parts: Vec<&str> = rd.split('.').collect();
                if parts.len() >= 3 {
                    let path_segments = &parts[0..parts.len() - 2];
                    let normalized = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
                    model.grain_set_under_path(path_segments, target_group) && normalized == **d
                } else {
                    rd == *d
                }
            })).unwrap_or(false))
            .cloned()
            .collect()),
        columns: request.columns.as_ref().map(|cols| {
            cols.iter()
                .map(|c| {
                    let parts: Vec<&str> = c.split('.').collect();
                    if parts.len() >= 3 {
                        let path_segments = &parts[0..parts.len() - 2];
                        if model.grain_set_under_path(path_segments, target_group) {
                            format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
                        } else {
                            c.clone()
                        }
                    } else {
                        c.clone()
                    }
                })
                .collect()
        }),
        metrics: request.metrics.clone(),
        filter: request.filter.clone(),
    };

    let selected = SelectedDataset { group: grain_set, dataset: table };
    let resolved = resolve_query(schema, &normalized_request, &selected, &grain_sets)
        .map_err(|e| PlanError::InvalidQuery(format!(
            "Query resolution error for tableGroup '{}': {:?}",
            target_group, e
        )))?;

    plan_query(&resolved)
}

/// Find a feasible table in a tableGroup for qualified dimension queries.
pub fn find_feasible_table_for_qualified<'a>(
    model: &SemanticModel,
    grain_set: &'a GrainSet,
    dimension_attrs: &[String],
    required_measures: &[String],
) -> Option<&'a Dataset> {
    grain_set.datasets.iter().find(|table| {
        for dim_attr in dimension_attrs {
            let parts: Vec<&str> = dim_attr.split('.').collect();
            if parts.len() >= 3 {
                let path_segments = &parts[0..parts.len() - 2];
                let dim_name = parts[parts.len() - 2];
                let attr_name = parts[parts.len() - 1];
                if !model.grain_set_under_path(path_segments, &grain_set.name) {
                    continue;
                }
                if let Some(attrs) = table.get_dimension_attributes(dim_name) {
                    if !attrs.iter().any(|a| a == attr_name) { return false; }
                } else {
                    return false;
                }
            } else if parts.len() == 2 {
                let (dim_name, attr_name) = (parts[0], parts[1]);
                if model.get_dimension(dim_name).map(|d| d.is_virtual()).unwrap_or(false) {
                    continue;
                }
                if let Some(attrs) = table.get_dimension_attributes(dim_name) {
                    if !attrs.iter().any(|a| a == attr_name) { return false; }
                } else {
                    return false;
                }
            }
        }
        for measure_name in required_measures {
            if grain_set.get_measure(measure_name).is_none() { return false; }
            if !table.has_measure(measure_name) { return false; }
        }
        true
    })
}

/// Build a single branch for a multi-tableGroup UNION query with NULL projection.
fn build_union_branch(
    model: &SemanticModel,
    grain_set: &GrainSet,
    table: &Dataset,
    dimension_attrs: &[String],
    metric_names: &[String],
) -> Result<PlanNode, PlanError> {
    let parsed_attrs: Vec<(String, ParsedDimensionAttr)> = dimension_attrs.iter()
        .map(|attr_path| (attr_path.clone(), ParsedDimensionAttr::parse(attr_path, model)))
        .collect();

    let physical_attrs: Vec<&(String, ParsedDimensionAttr)> = parsed_attrs.iter()
        .filter(|(_, parsed)| {
            !parsed.is_virtual() && parsed.belongs_to_grain_set(model, &grain_set.name)
        })
        .collect();

    let mut unique_dim_attrs: Vec<(String, String)> = Vec::new();
    let mut dim_attr_to_group_idx: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();

    for (_, parsed) in &physical_attrs {
        let key = (parsed.dim_name().to_string(), parsed.attr_name().to_string());
        if !dim_attr_to_group_idx.contains_key(&key) {
            let idx = unique_dim_attrs.len();
            unique_dim_attrs.push(key.clone());
            dim_attr_to_group_idx.insert(key, idx);
        }
    }

    let fact_alias: &str = &table.name;
    let mut columns = Vec::new();
    let mut types = Vec::new();
    let mut joined_dimensions: HashSet<String> = HashSet::new();

    for (dim_name, attr_name) in &unique_dim_attrs {
        if let Some(group_dim) = grain_set.get_dimension(dim_name) {
            if group_dim.is_degenerate() {
                if let Some(attr) = group_dim.get_attribute(attr_name) {
                    columns.push(attr.column_name().to_string());
                    types.push(attr.data_type.to_string());
                }
            }
        }
    }

    let measures_to_aggregate: Vec<(&str, &crate::semantic_model::Measure)> = metric_names.iter()
        .filter_map(|metric_name| {
            model.get_metric(metric_name).and_then(|m| {
                match &m.expr {
                    MetricExpr::MeasureRef(measure_name) => {
                        grain_set.get_measure(measure_name)
                            .map(|measure| (metric_name.as_str(), measure))
                    }
                    MetricExpr::Structured(_) => None,
                }
            })
        })
        .collect();

    for (_, measure) in &measures_to_aggregate {
        if let MeasureExpr::Column(col) = &measure.expr {
            columns.push(col.clone());
            types.push(measure.data_type().to_string());
        }
    }

    let mut plan = PlanNode::Scan(
        Scan::new(&table.name)
            .with_alias(fact_alias)
            .with_columns(columns, types)
    );

    for (dim_name, _) in &unique_dim_attrs {
        if joined_dimensions.contains(dim_name) { continue; }
        if let Some(group_dim) = grain_set.get_dimension(dim_name) {
            if let Some(join_spec) = &group_dim.join {
                if let Some(dimension) = model.get_dimension(dim_name) {
                    if needs_join_for_dimension(table, group_dim, dimension) {
                        let dim_alias = dimension.alias.as_deref().unwrap_or(&dimension.name);
                        let dim_cols: Vec<String> = dimension.attributes.iter()
                            .map(|a| a.column_name().to_string()).collect();
                        let dim_types: Vec<String> = dimension.attributes.iter()
                            .map(|a| a.data_type.to_string()).collect();
                        let dim_table = dimension.table.as_ref()
                            .expect("Non-virtual dimension must have a table");
                        let dim_scan = PlanNode::Scan(
                            Scan::new(dim_table)
                                .with_alias(dim_alias)
                                .with_columns(dim_cols, dim_types)
                        );
                        let left_key = Column::new(fact_alias, &join_spec.left_key);
                        let right_key = Column::new(
                            join_spec.right_alias.as_deref().unwrap_or(dim_alias),
                            &join_spec.right_key,
                        );
                        plan = PlanNode::Join(Join {
                            left: Box::new(plan),
                            right: Box::new(dim_scan),
                            join_type: JoinType::Left,
                            left_key,
                            right_key,
                        });
                        joined_dimensions.insert(dim_name.clone());
                    }
                }
            }
        }
    }

    let group_by: Vec<Column> = unique_dim_attrs.iter()
        .filter_map(|(dim_name, attr_name)| {
            if let Some(group_dim) = grain_set.get_dimension(dim_name) {
                if group_dim.is_degenerate() {
                    if let Some(attr) = group_dim.get_attribute(attr_name) {
                        return Some(Column::new(fact_alias, attr.column_name()));
                    }
                } else if let Some(dimension) = model.get_dimension(dim_name) {
                    if let Some(attr) = dimension.get_attribute(attr_name) {
                        let dim_alias = dimension.alias.as_deref().unwrap_or(&dimension.name);
                        return Some(Column::new(dim_alias, attr.column_name()));
                    }
                }
            }
            None
        })
        .collect();

    let aggregates: Vec<AggregateExpr> = measures_to_aggregate.iter()
        .map(|(metric_name, measure)| {
            AggregateExpr {
                func: measure.aggregation,
                expr: convert_measure_expr(&measure.expr),
                alias: metric_name.to_string(),
            }
        })
        .collect();

    if !group_by.is_empty() || !aggregates.is_empty() {
        plan = PlanNode::Aggregate(Aggregate {
            input: Box::new(plan),
            group_by: group_by.clone(),
            aggregates,
        });
    }

    let mut projections = Vec::new();

    for (attr_path, parsed) in &parsed_attrs {
        let expr = if parsed.is_virtual() {
            let dim_name = parsed.dim_name();
            let attr_name = parsed.attr_name();
            let value = get_virtual_attribute_value(model, grain_set, dim_name, attr_name);
            match value {
                PlanLiteralValue::String(s) => Expr::Literal(Literal::String(s)),
                PlanLiteralValue::Int64(i) => Expr::Literal(Literal::Int(i)),
                PlanLiteralValue::Float64(f) => Expr::Literal(Literal::Float(f)),
                PlanLiteralValue::Bool(b) => Expr::Literal(Literal::Bool(b)),
                PlanLiteralValue::Null => Expr::Literal(Literal::Null("string".to_string())),
                _ => Expr::Literal(Literal::Null("string".to_string())),
            }
        } else if parsed.belongs_to_grain_set(model, &grain_set.name) {
            let key = (parsed.dim_name().to_string(), parsed.attr_name().to_string());
            if let Some(&idx) = dim_attr_to_group_idx.get(&key) {
                let col = group_by.get(idx).cloned()
                    .unwrap_or_else(|| Column::unqualified(attr_path));
                Expr::Column(col)
            } else {
                let data_type = parsed.get_data_type(model);
                Expr::Literal(Literal::Null(data_type))
            }
        } else {
            let data_type = parsed.get_data_type(model);
            Expr::Literal(Literal::Null(data_type))
        };

        projections.push(ProjectExpr {
            expr,
            alias: attr_path.clone(),
        });
    }

    for metric_name in metric_names {
        projections.push(ProjectExpr {
            expr: Expr::Column(Column::unqualified(metric_name)),
            alias: metric_name.clone(),
        });
    }

    Ok(PlanNode::Project(Project {
        input: Box::new(plan),
        expressions: projections,
    }))
}

/// Build a branch for tier 3 (partial selection): same output schema as a full branch
/// but with NULL for missing dimensions and measures.
fn build_partial_union_branch(
    model: &SemanticModel,
    partial: &PartialSelection<'_>,
    dimension_attrs: &[String],
    metric_names: &[String],
) -> Result<PlanNode, PlanError> {
    use std::collections::HashSet;
    let grain_set = partial.group;
    let table = partial.dataset;
    let missing_dims: HashSet<&str> = partial.missing_dimensions.iter().map(String::as_str).collect();
    let present_dims: HashSet<&str> = partial.present_dimensions.iter().map(String::as_str).collect();
    let present_measures_set: HashSet<&str> = partial.present_measures.iter().map(String::as_str).collect();

    let parsed_attrs: Vec<(String, ParsedDimensionAttr)> = dimension_attrs.iter()
        .map(|attr_path| (attr_path.clone(), ParsedDimensionAttr::parse(attr_path, model)))
        .collect();

    let physical_attrs: Vec<&(String, ParsedDimensionAttr)> = parsed_attrs.iter()
        .filter(|(path, parsed)| {
            if parsed.is_virtual() {
                return true;
            }
            parsed.belongs_to_grain_set(model, &grain_set.name) && present_dims.contains(path.as_str())
        })
        .collect();

    let mut unique_dim_attrs: Vec<(String, String)> = Vec::new();
    let mut dim_attr_to_group_idx: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();

    for (_, parsed) in &physical_attrs {
        let key = (parsed.dim_name().to_string(), parsed.attr_name().to_string());
        if !dim_attr_to_group_idx.contains_key(&key) {
            let idx = unique_dim_attrs.len();
            unique_dim_attrs.push(key.clone());
            dim_attr_to_group_idx.insert(key, idx);
        }
    }

    let fact_alias: &str = &table.name;
    let mut columns = Vec::new();
    let mut types = Vec::new();
    let mut joined_dimensions: HashSet<String> = HashSet::new();

    for (dim_name, attr_name) in &unique_dim_attrs {
        if let Some(group_dim) = grain_set.get_dimension(dim_name) {
            if group_dim.is_degenerate() {
                if let Some(attr) = group_dim.get_attribute(attr_name) {
                    columns.push(attr.column_name().to_string());
                    types.push(attr.data_type.to_string());
                }
            }
        }
    }

    let measures_to_aggregate: Vec<(&str, &crate::semantic_model::Measure)> = metric_names.iter()
        .filter_map(|metric_name| {
            model.get_metric(metric_name).and_then(|m| {
                match &m.expr {
                    MetricExpr::MeasureRef(measure_name) => {
                        if !present_measures_set.contains(measure_name.as_str()) {
                            return None;
                        }
                        grain_set.get_measure(measure_name)
                            .map(|measure| (metric_name.as_str(), measure))
                    }
                    MetricExpr::Structured(_) => None,
                }
            })
        })
        .collect();

    for (_, measure) in &measures_to_aggregate {
        if let MeasureExpr::Column(col) = &measure.expr {
            columns.push(col.clone());
            types.push(measure.data_type().to_string());
        }
    }

    let mut plan = PlanNode::Scan(
        Scan::new(&table.name)
            .with_alias(fact_alias)
            .with_columns(columns, types)
    );

    for (dim_name, _) in &unique_dim_attrs {
        if joined_dimensions.contains(dim_name) { continue; }
        if let Some(group_dim) = grain_set.get_dimension(dim_name) {
            if let Some(join_spec) = &group_dim.join {
                if let Some(dimension) = model.get_dimension(dim_name) {
                    if needs_join_for_dimension(table, group_dim, dimension) {
                        let dim_alias = dimension.alias.as_deref().unwrap_or(&dimension.name);
                        let dim_cols: Vec<String> = dimension.attributes.iter()
                            .map(|a| a.column_name().to_string()).collect();
                        let dim_types: Vec<String> = dimension.attributes.iter()
                            .map(|a| a.data_type.to_string()).collect();
                        let dim_table = dimension.table.as_ref()
                            .expect("Non-virtual dimension must have a table");
                        let dim_scan = PlanNode::Scan(
                            Scan::new(dim_table)
                                .with_alias(dim_alias)
                                .with_columns(dim_cols, dim_types)
                        );
                        let left_key = Column::new(fact_alias, &join_spec.left_key);
                        let right_key = Column::new(
                            join_spec.right_alias.as_deref().unwrap_or(dim_alias),
                            &join_spec.right_key,
                        );
                        plan = PlanNode::Join(Join {
                            left: Box::new(plan),
                            right: Box::new(dim_scan),
                            join_type: JoinType::Left,
                            left_key,
                            right_key,
                        });
                        joined_dimensions.insert(dim_name.clone());
                    }
                }
            }
        }
    }

    let group_by: Vec<Column> = unique_dim_attrs.iter()
        .filter_map(|(dim_name, attr_name)| {
            if let Some(group_dim) = grain_set.get_dimension(dim_name) {
                if group_dim.is_degenerate() {
                    if let Some(attr) = group_dim.get_attribute(attr_name) {
                        return Some(Column::new(fact_alias, attr.column_name()));
                    }
                } else if let Some(dimension) = model.get_dimension(dim_name) {
                    if let Some(attr) = dimension.get_attribute(attr_name) {
                        let dim_alias = dimension.alias.as_deref().unwrap_or(&dimension.name);
                        return Some(Column::new(dim_alias, attr.column_name()));
                    }
                }
            }
            None
        })
        .collect();

    let aggregates: Vec<AggregateExpr> = measures_to_aggregate.iter()
        .map(|(metric_name, measure)| {
            AggregateExpr {
                func: measure.aggregation,
                expr: convert_measure_expr(&measure.expr),
                alias: metric_name.to_string(),
            }
        })
        .collect();

    if !group_by.is_empty() || !aggregates.is_empty() {
        plan = PlanNode::Aggregate(Aggregate {
            input: Box::new(plan),
            group_by: group_by.clone(),
            aggregates,
        });
    }

    let mut projections = Vec::new();

    for (attr_path, parsed) in &parsed_attrs {
        let expr = if parsed.is_virtual() {
            let dim_name = parsed.dim_name();
            let attr_name = parsed.attr_name();
            let value = get_virtual_attribute_value(model, grain_set, dim_name, attr_name);
            match value {
                PlanLiteralValue::String(s) => Expr::Literal(Literal::String(s)),
                PlanLiteralValue::Int64(i) => Expr::Literal(Literal::Int(i)),
                PlanLiteralValue::Float64(f) => Expr::Literal(Literal::Float(f)),
                PlanLiteralValue::Bool(b) => Expr::Literal(Literal::Bool(b)),
                PlanLiteralValue::Null => Expr::Literal(Literal::Null("string".to_string())),
                _ => Expr::Literal(Literal::Null("string".to_string())),
            }
        } else if missing_dims.contains(attr_path.as_str()) {
            let data_type = parsed.get_data_type(model);
            Expr::Literal(Literal::Null(data_type))
        } else if parsed.belongs_to_grain_set(model, &grain_set.name) {
            let key = (parsed.dim_name().to_string(), parsed.attr_name().to_string());
            if let Some(&idx) = dim_attr_to_group_idx.get(&key) {
                let col = group_by.get(idx).cloned()
                    .unwrap_or_else(|| Column::unqualified(attr_path));
                Expr::Column(col)
            } else {
                let data_type = parsed.get_data_type(model);
                Expr::Literal(Literal::Null(data_type))
            }
        } else {
            let data_type = parsed.get_data_type(model);
            Expr::Literal(Literal::Null(data_type))
        };

        projections.push(ProjectExpr {
            expr,
            alias: attr_path.clone(),
        });
    }

    for metric_name in metric_names {
        let expr = match model.get_metric(metric_name) {
            Some(m) => match &m.expr {
                MetricExpr::MeasureRef(measure_name) if present_measures_set.contains(measure_name.as_str()) => {
                    Expr::Column(Column::unqualified(metric_name))
                }
                _ => {
                    let data_type = m.data_type().to_string();
                    Expr::Literal(Literal::Null(data_type))
                }
            },
            None => Expr::Literal(Literal::Null("f64".to_string())),
        };
        projections.push(ProjectExpr {
            expr,
            alias: metric_name.clone(),
        });
    }

    Ok(PlanNode::Project(Project {
        input: Box::new(plan),
        expressions: projections,
    }))
}

/// Plan a virtual-only query that doesn't need table scans.
pub fn plan_virtual_only_query(
    model: &SemanticModel,
    virtual_dims: &[String],
) -> Result<PlanNode, PlanError> {
    let attrs: Vec<(&str, &str)> = virtual_dims.iter()
        .filter_map(|d| {
            let parts: Vec<&str> = d.split('.').collect();
            if parts.len() == 2 { Some((parts[0], parts[1])) } else { None }
        })
        .collect();

    if attrs.is_empty() {
        return Err(PlanError::InvalidQuery(
            "No valid virtual dimension attributes in query".to_string()
        ));
    }

    let columns: Vec<String> = virtual_dims.iter().cloned().collect();
    let column_types: Vec<String> = attrs.iter().map(|_| "string".to_string()).collect();

    let mut rows: Vec<Vec<PlanLiteralValue>> = Vec::new();

    for grain_set in model.grain_sets() {
        let first_dataset = grain_set.datasets.first();
        let row: Vec<PlanLiteralValue> = attrs.iter()
            .map(|(dim_name, attr_name)| {
                match first_dataset {
                    Some(ds) => get_virtual_attribute_value_with_dataset(model, &grain_set, Some(ds), dim_name, attr_name),
                    None => get_virtual_attribute_value(model, &grain_set, dim_name, attr_name),
                }
            })
            .collect();
        rows.push(row);
    }

    Ok(PlanNode::VirtualTable(VirtualTable {
        columns,
        column_types,
        rows,
    }))
}
