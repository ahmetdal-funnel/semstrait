//! Top-level query planning entry point
//!
//! `plan_semantic_query` is the main router that classifies queries and
//! dispatches to the appropriate planner.

use std::collections::HashSet;
use crate::semantic_model::{Schema, SemanticModel, Metric};
use crate::plan::PlanNode;
use crate::resolver::{resolve_query, collect_required_measure_names};
use crate::selector::{select_datasets, select_datasets_for_join, SelectedDataset};
use crate::query::QueryRequest;
use super::error::PlanError;
use super::table::plan_query;
use super::cross::{plan_cross_grain_set_query, plan_multi_cross_grain_set_query};
use super::union::{plan_conformed_query, plan_multi_grain_set_query, plan_single_grain_set_query};
use super::join::plan_same_grain_set_join;

/// Plan a semantic query, automatically handling all query types.
///
/// This is the main entry point for query planning. It:
/// 1. Analyzes the requested metrics to detect cross-grain-set metrics
/// 2. Routes to the appropriate planner based on query characteristics
pub fn plan_semantic_query(
    schema: &Schema,
    model: &SemanticModel,
    request: &QueryRequest,
) -> Result<PlanNode, PlanError> {
    let mut dimension_attrs: Vec<String> = Vec::new();
    if let Some(ref rows) = request.rows {
        dimension_attrs.extend(rows.clone());
    }
    if let Some(ref cols) = request.columns {
        dimension_attrs.extend(cols.clone());
    }

    let cross_dataset_metrics: Vec<&Metric> = request.metrics
        .as_ref()
        .map(|names| {
            names.iter()
                .filter_map(|name| model.get_metric(name))
                .filter(|m| m.is_cross_grain_set())
                .collect()
        })
        .unwrap_or_default();

    let qualified_groups: HashSet<String> = dimension_attrs.iter()
        .flat_map(|path| {
            let parts: Vec<&str> = path.split('.').collect();
            if parts.len() >= 3 {
                let path_segments = &parts[0..parts.len() - 2];
                model.grain_sets_under_path(path_segments).into_iter().map(|gs| gs.name).collect::<Vec<_>>()
            } else {
                vec![]
            }
        })
        .collect();

    let is_conformed = model.is_conformed_query(&dimension_attrs);

    let query_kind = classify_query(&cross_dataset_metrics, &qualified_groups);

    match query_kind {
        QueryKind::CrossSingle(metric) => {
            plan_cross_grain_set_query(schema, model, metric, &dimension_attrs)
        }
        QueryKind::CrossMulti(metrics) => {
            plan_multi_cross_grain_set_query(schema, model, &metrics, &dimension_attrs)
        }
        QueryKind::QualifiedMulti(refs) => {
            plan_multi_grain_set_query(schema, model, request, &dimension_attrs, &refs)
        }
        QueryKind::QualifiedSingle(target_group) => {
            plan_single_grain_set_query(schema, model, request, &dimension_attrs, target_group)
        }
        QueryKind::Default => {
            plan_default_no_qualified(schema, model, request, &dimension_attrs, is_conformed)
        }
    }
}

/// Query classification for router dispatch.
#[derive(Debug)]
enum QueryKind<'a> {
    /// Single cross-grain-set metric
    CrossSingle(&'a Metric),
    /// Multiple cross-grain-set metrics
    CrossMulti(Vec<&'a Metric>),
    /// Path-qualified dimensions from more than one grain set (or group)
    QualifiedMulti(HashSet<&'a str>),
    /// Path-qualified dimensions from exactly one grain set
    QualifiedSingle(&'a str),
    /// No cross metrics, no path qualifiers: selector → single table, conformed union, or join
    Default,
}

fn classify_query<'a>(
    cross_dataset_metrics: &'a [&'a Metric],
    qualified_groups: &'a HashSet<String>,
) -> QueryKind<'a> {
    if cross_dataset_metrics.len() == 1 {
        QueryKind::CrossSingle(cross_dataset_metrics[0])
    } else if cross_dataset_metrics.len() > 1 {
        QueryKind::CrossMulti(cross_dataset_metrics.to_vec())
    } else if qualified_groups.len() > 1 {
        let refs: HashSet<&str> = qualified_groups.iter().map(String::as_str).collect();
        QueryKind::QualifiedMulti(refs)
    } else if qualified_groups.len() == 1 {
        QueryKind::QualifiedSingle(qualified_groups.iter().next().unwrap().as_str())
    } else {
        QueryKind::Default
    }
}

/// Plan when there are no cross-grain-set metrics and no path-qualified dimensions.
/// Uses selector; then either conformed union, single table, or same-grain-set join.
fn plan_default_no_qualified(
    schema: &Schema,
    model: &SemanticModel,
    request: &QueryRequest,
    dimension_attrs: &[String],
    is_conformed: bool,
) -> Result<PlanNode, PlanError> {
    let metric_names: Vec<String> = request.metrics.clone().unwrap_or_default();
    let required_measures: Vec<String> = collect_required_measure_names(model, &metric_names);

    let grain_sets = model.grain_sets();
    match select_datasets(schema, model, &grain_sets, dimension_attrs, &required_measures) {
        Ok(selected_tables) => {
            if is_conformed && grain_sets.len() > 1 {
                plan_conformed_query(schema, model, request, dimension_attrs)
            } else {
                let selected = selected_tables.into_iter().next()
                    .ok_or_else(|| PlanError::InvalidQuery("No feasible table found for query".to_string()))?;
                let resolved = resolve_query(schema, request, &selected, &grain_sets)
                    .map_err(|e| PlanError::InvalidQuery(format!("Query resolution error: {:?}", e)))?;
                plan_query(&resolved)
            }
        }
        Err(select_err) => {
            if is_conformed && model.grain_sets().len() > 1 {
                plan_conformed_query(schema, model, request, dimension_attrs)
            } else {
                let multi_selection = select_datasets_for_join(
                    schema,
                    model,
                    &grain_sets,
                    dimension_attrs,
                    &required_measures,
                )
                .map_err(|_| PlanError::InvalidQuery(format!("Dataset selection error: {:?}", select_err)))?;

                if multi_selection.datasets.len() == 1 {
                    let selected = SelectedDataset {
                        group: multi_selection.group,
                        dataset: multi_selection.datasets[0].dataset,
                    };
                    let resolved = resolve_query(schema, request, &selected, &grain_sets)
                        .map_err(|e| PlanError::InvalidQuery(format!("Query resolution error: {:?}", e)))?;
                    plan_query(&resolved)
                } else {
                    plan_same_grain_set_join(
                        model,
                        &multi_selection,
                        dimension_attrs,
                        &metric_names,
                    )
                }
            }
        }
    }
}
