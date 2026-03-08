//! Cross-grain-set query planning
//!
//! Handles metrics that span multiple grain sets. These produce UNION plans
//! where each branch aggregates its own grain set, then a re-aggregation
//! combines the results.

use std::collections::{HashMap, HashSet};
use crate::semantic_model::{Aggregation, Dataset, GrainSet, Measure, MeasureExpr, Metric, Schema, SemanticModel};
use crate::plan::{
    Aggregate, AggregateExpr, Column, Expr, Join, JoinType,
    Literal, PlanNode, Scan, Project, ProjectExpr, Sort, SortKey, SortDirection, Union,
    LiteralValue as PlanLiteralValue,
};
use super::error::PlanError;
use super::util::{needs_join_for_dimension, ParsedDimensionAttr, get_virtual_attribute_value_with_dataset};
use super::expr::convert_measure_expr;
use super::table::build_grain_set_branch;

/// Glob match: * matches zero or more characters. Pattern is matched against the whole value.
fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    if !pattern.contains('*') {
        return value == pattern;
    }
    let segments: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            continue;
        }
        if i == 0 {
            if !value.starts_with(seg) {
                return false;
            }
            pos = seg.len();
        } else {
            match value[pos..].find(seg) {
                None => return false,
                Some(j) => pos = pos + j + seg.len(),
            }
        }
    }
    if !pattern.ends_with('*') {
        if let Some(last) = segments.last().filter(|s| !s.is_empty()) {
            if !value.ends_with(last) {
                return false;
            }
        }
    }
    true
}

/// Expand metric CASE WHEN (eq + match) to (path, measure) with first-match-wins per grain set.
fn expanded_grain_set_measures(metric: &Metric, model: &SemanticModel) -> Vec<(String, String)> {
    let Some(when_branches) = metric.case_when_branches() else {
        return vec![];
    };
    let mut out = Vec::new();
    for gs in model.grain_sets() {
        let path = gs
            .container_path
            .as_ref()
            .map(|p| p.join("."))
            .unwrap_or_else(|| gs.name.clone());
        for w in when_branches {
            let measure = match w.then.measure_name() {
                Some(m) => m,
                None => continue,
            };
            if let Some(exact) = w.condition.grain_set_value() {
                if path == exact {
                    out.push((path, measure));
                    break;
                }
            } else if let Some(pat) = w.condition.grain_set_pattern() {
                if glob_match(&pat, &path) {
                    out.push((path, measure));
                    break;
                }
            }
        }
    }
    out
}

/// Resolve a grain set from a metric CASE value: either a grain set name (e.g. "adwords")
/// or a container path string (e.g. "facebookads.facebookads_111").
fn resolve_grain_set_for_cross(model: &SemanticModel, gs_name: &str) -> Option<GrainSet> {
    model.get_grain_set(gs_name).or_else(|| {
        if gs_name.contains('.') {
            let path: Vec<&str> = gs_name.split('.').collect();
            model.get_grain_set_by_path(&path)
        } else {
            None
        }
    })
}

/// A branch in a cross-grain-set query
#[derive(Debug)]
pub struct CrossGrainSetBranch<'a> {
    pub grain_set: &'a GrainSet,
    pub measure: &'a Measure,
    pub table: &'a Dataset,
}

/// Plan a cross-grain-set query for a single metric.
pub fn plan_cross_grain_set_query<'a>(
    _schema: &'a Schema,
    model: &'a SemanticModel,
    metric: &'a Metric,
    dimension_attrs: &[String],
) -> Result<PlanNode, PlanError> {
    for attr_path in dimension_attrs {
        let parts: Vec<&str> = attr_path.split('.').collect();
        if parts.len() >= 3 {
            let path_segments = &parts[0..parts.len() - 2];
            if model.grain_sets_under_path(path_segments).is_empty() {
                return Err(PlanError::InvalidQuery(
                    format!("Container path '{}' not found in qualified dimension '{}'", path_segments.join("."), attr_path)
                ));
            }
        }
    }

    let mappings: Vec<(String, String)> = if !metric.grain_set_measures().is_empty() {
        metric.grain_set_measures()
    } else if metric.is_cross_grain_set() {
        expanded_grain_set_measures(metric, model)
    } else {
        vec![]
    };
    if mappings.is_empty() {
        return Err(PlanError::InvalidQuery(
            format!("Metric '{}' is not a cross-grain-set metric", metric.name)
        ));
    }

    let mut branches: Vec<PlanNode> = Vec::new();

    for (gs_name, measure_name) in &mappings {
        let grain_set = resolve_grain_set_for_cross(model, gs_name)
            .ok_or_else(|| PlanError::InvalidQuery(
                format!("Grain set '{}' not found", gs_name)
            ))?;
        let measure = grain_set.get_measure(measure_name)
            .ok_or_else(|| PlanError::InvalidQuery(
                format!("Measure '{}' not found in grain set '{}'", measure_name, gs_name)
            ))?;

        let table = grain_set.datasets.iter()
            .find(|t| t.has_measure(measure_name))
            .ok_or_else(|| PlanError::InvalidQuery(
                format!("No table in grain set '{}' has measure '{}'", gs_name, measure_name)
            ))?;
        let branch = build_cross_grain_set_branch(
            model, &grain_set, table, measure, dimension_attrs, &metric.name,
        )?;
        branches.push(branch);
    }

    if branches.len() == 1 {
        return Ok(branches.into_iter().next().unwrap());
    }

    let union = PlanNode::Union(Union { inputs: branches });

    let group_by: Vec<Column> = dimension_attrs.iter()
        .map(|attr| Column::unqualified(attr))
        .collect();
    let aggregates = vec![
        AggregateExpr {
            func: Aggregation::Sum,
            expr: Expr::Column(Column::unqualified(&metric.name)),
            alias: metric.name.clone(),
        }
    ];

    let plan = PlanNode::Aggregate(Aggregate {
        input: Box::new(union),
        group_by: group_by.clone(),
        aggregates,
    });

    let sort_keys: Vec<SortKey> = dimension_attrs.iter()
        .map(|attr| SortKey {
            column: attr.clone(),
            direction: SortDirection::Ascending,
        })
        .collect();

    if !sort_keys.is_empty() {
        Ok(PlanNode::Sort(Sort {
            input: Box::new(plan),
            sort_keys,
        }))
    } else {
        Ok(plan)
    }
}

/// Plan a cross-grain-set query for multiple metrics.
pub fn plan_multi_cross_grain_set_query<'a>(
    _schema: &'a Schema,
    model: &'a SemanticModel,
    metrics: &[&'a Metric],
    dimension_attrs: &[String],
) -> Result<PlanNode, PlanError> {
    for attr_path in dimension_attrs {
        let parts: Vec<&str> = attr_path.split('.').collect();
        if parts.len() >= 3 {
            let path_segments = &parts[0..parts.len() - 2];
            if model.grain_sets_under_path(path_segments).is_empty() {
                return Err(PlanError::InvalidQuery(
                    format!("Container path '{}' not found in qualified dimension '{}'", path_segments.join("."), attr_path)
                ));
            }
        }
    }

    let metric_gs_measures: Vec<(String, Vec<(String, String)>)> = metrics.iter()
        .map(|metric| {
            let mappings: Vec<(String, String)> = if !metric.grain_set_measures().is_empty() {
                metric.grain_set_measures()
            } else if metric.is_cross_grain_set() {
                expanded_grain_set_measures(metric, model)
            } else {
                vec![]
            };
            (metric.name.clone(), mappings)
        })
        .collect();

    for (metric_name, mappings) in &metric_gs_measures {
        if mappings.is_empty() {
            return Err(PlanError::InvalidQuery(
                format!("Metric '{}' is not a cross-grain-set metric", metric_name)
            ));
        }
    }

    plan_cross_grain_set_union(model, dimension_attrs, &metric_gs_measures)
}

/// Unified cross-grain-set UNION planner.
///
/// 1. Build a branch per grain set using build_grain_set_branch
/// 2. Project each branch to common schema (NULLs for missing columns)
/// 3. UNION all branches
/// 4. Re-aggregate to combine rows
pub fn plan_cross_grain_set_union(
    model: &SemanticModel,
    dimension_attrs: &[String],
    metric_gs_measures: &[(String, Vec<(String, String)>)],
) -> Result<PlanNode, PlanError> {
    let metric_names: Vec<&str> = metric_gs_measures.iter()
        .map(|(name, _)| name.as_str())
        .collect();

    let mut gs_to_metric_measures: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for (metric_name, gs_measures) in metric_gs_measures {
        for (gs_name, measure_name) in gs_measures {
            gs_to_metric_measures
                .entry(gs_name.clone())
                .or_default()
                .push((metric_name.clone(), measure_name.clone()));
        }
    }

    let mut branches: Vec<PlanNode> = Vec::new();

    for (gs_name, metric_measure_pairs) in &gs_to_metric_measures {
        let grain_set = resolve_grain_set_for_cross(model, gs_name)
            .ok_or_else(|| PlanError::InvalidQuery(
                format!("Grain set '{}' not found", gs_name)
            ))?;

        let measure_aliases: Vec<(String, String)> = metric_measure_pairs.iter()
            .map(|(metric, measure)| (metric.clone(), measure.clone()))
            .collect();

        let _measure_names_for_check: Vec<&str> = measure_aliases.iter()
            .map(|(_, m)| m.as_str())
            .collect();

        let branch = build_grain_set_branch(model, &grain_set, dimension_attrs, &measure_aliases)?;
        let projected = project_branch_for_union(
            model, &grain_set, None, branch,
            dimension_attrs, &metric_names, metric_measure_pairs,
        )?;
        branches.push(projected);
    }

    if branches.len() == 1 {
        return Ok(branches.into_iter().next().unwrap());
    }

    let union = PlanNode::Union(Union { inputs: branches });

    let group_by: Vec<Column> = dimension_attrs.iter()
        .map(|attr| Column::unqualified(attr))
        .collect();

    let aggregates: Vec<AggregateExpr> = metric_names.iter()
        .map(|name| AggregateExpr {
            func: Aggregation::Sum,
            expr: Expr::Column(Column::unqualified(*name)),
            alias: name.to_string(),
        })
        .collect();

    let plan = PlanNode::Aggregate(Aggregate {
        input: Box::new(union),
        group_by: group_by.clone(),
        aggregates,
    });

    let sort_keys: Vec<SortKey> = dimension_attrs.iter()
        .map(|attr| SortKey {
            column: attr.clone(),
            direction: SortDirection::Ascending,
        })
        .collect();

    if !sort_keys.is_empty() {
        Ok(PlanNode::Sort(Sort {
            input: Box::new(plan),
            sort_keys,
        }))
    } else {
        Ok(plan)
    }
}

/// Project a grain-set branch to the common UNION schema.
fn project_branch_for_union(
    model: &SemanticModel,
    grain_set: &GrainSet,
    dataset: Option<&Dataset>,
    input: PlanNode,
    dimension_attrs: &[String],
    all_metric_names: &[&str],
    gs_metrics: &[(String, String)],
) -> Result<PlanNode, PlanError> {
    let gs_metric_set: HashSet<&str> = gs_metrics.iter()
        .map(|(m, _)| m.as_str())
        .collect();

    let mut projections = Vec::new();

    for attr_path in dimension_attrs {
        let parts: Vec<&str> = attr_path.split('.').collect();
        let (path_segments, dim_name, attr_name) = match parts.len() {
            2 => (None, parts[0], parts[1]),
            n if n >= 3 => (Some(&parts[0..n - 2]), parts[n - 2], parts[n - 1]),
            _ => continue,
        };

        if model.get_dimension(dim_name).map(|d| d.is_virtual()).unwrap_or(false) {
            let value = get_virtual_attribute_value_with_dataset(model, grain_set, dataset, dim_name, attr_name);
            let expr = match value {
                PlanLiteralValue::String(s) => Expr::Literal(Literal::String(s)),
                PlanLiteralValue::Int64(i) => Expr::Literal(Literal::Int(i)),
                PlanLiteralValue::Float64(f) => Expr::Literal(Literal::Float(f)),
                PlanLiteralValue::Bool(b) => Expr::Literal(Literal::Bool(b)),
                _ => Expr::Literal(Literal::Null("string".to_string())),
            };
            projections.push(ProjectExpr {
                expr,
                alias: attr_path.clone(),
            });
        } else if let Some(path) = path_segments {
            let belongs = model.grain_sets_under_path(path).iter().any(|gs| gs.name == grain_set.name);
            if belongs {
                let semantic_name = format!("{}.{}", dim_name, attr_name);
                projections.push(ProjectExpr {
                    expr: Expr::Column(Column::unqualified(&semantic_name)),
                    alias: attr_path.clone(),
                });
            } else {
                let data_type = model.get_dimension(dim_name)
                    .and_then(|d| d.get_attribute(attr_name))
                    .map(|a| a.data_type.to_string())
                    .unwrap_or_else(|| "string".to_string());
                projections.push(ProjectExpr {
                    expr: Expr::Literal(Literal::Null(data_type)),
                    alias: attr_path.clone(),
                });
            }
        } else {
            let semantic_name = format!("{}.{}", dim_name, attr_name);
            projections.push(ProjectExpr {
                expr: Expr::Column(Column::unqualified(&semantic_name)),
                alias: attr_path.clone(),
            });
        }
    }

    for metric_name in all_metric_names {
        let expr = if gs_metric_set.contains(metric_name) {
            Expr::Column(Column::unqualified(*metric_name))
        } else {
            Expr::Literal(Literal::Null("f64".to_string()))
        };
        projections.push(ProjectExpr {
            expr,
            alias: metric_name.to_string(),
        });
    }

    Ok(PlanNode::Project(Project {
        input: Box::new(input),
        expressions: projections,
    }))
}

/// Build a single branch of a cross-grain-set query for one measure/grain set.
fn build_cross_grain_set_branch(
    model: &SemanticModel,
    grain_set: &GrainSet,
    table: &Dataset,
    measure: &Measure,
    dimension_attrs: &[String],
    output_alias: &str,
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

    if let MeasureExpr::Column(col) = &measure.expr {
        columns.push(col.clone());
        types.push(measure.data_type().to_string());
    }

    let mut plan = PlanNode::Scan(
        Scan::new(&table.name)
            .with_alias(fact_alias)
            .with_columns(columns, types)
    );

    for (dim_name, _) in &unique_dim_attrs {
        if joined_dimensions.contains(dim_name) {
            continue;
        }
        if let Some(group_dim) = grain_set.get_dimension(dim_name) {
            if let Some(join_spec) = &group_dim.join {
                if let Some(dimension) = model.get_dimension(dim_name) {
                    if needs_join_for_dimension(table, group_dim, dimension) {
                        let dim_alias = dimension.alias.as_deref().unwrap_or(&dimension.name);
                        let dim_cols: Vec<String> = dimension.attributes.iter()
                            .map(|a| a.column_name().to_string())
                            .collect();
                        let dim_types: Vec<String> = dimension.attributes.iter()
                            .map(|a| a.data_type.to_string())
                            .collect();
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

    let aggregates = vec![
        AggregateExpr {
            func: measure.aggregation,
            expr: convert_measure_expr(&measure.expr),
            alias: output_alias.to_string(),
        }
    ];

    plan = PlanNode::Aggregate(Aggregate {
        input: Box::new(plan),
        group_by: group_by.clone(),
        aggregates,
    });

    let mut projections = Vec::new();

    for (attr_path, parsed) in &parsed_attrs {
        let expr = if parsed.is_virtual() {
            let dim_name = parsed.dim_name();
            let attr_name = parsed.attr_name();
            let value = get_virtual_attribute_value_with_dataset(model, grain_set, Some(table), dim_name, attr_name);
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

    projections.push(ProjectExpr {
        expr: Expr::Column(Column::unqualified(output_alias)),
        alias: output_alias.to_string(),
    });

    Ok(PlanNode::Project(Project {
        input: Box::new(plan),
        expressions: projections,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("google_ads", "google_ads"));
        assert!(!glob_match("google_ads", "meta_ads"));
        assert!(!glob_match("", "x"));
        assert!(glob_match("", ""));
    }

    #[test]
    fn glob_match_star_prefix() {
        assert!(glob_match("*.ads", "google_ads"));
        assert!(glob_match("*.ads", "meta_ads"));
        assert!(glob_match("*.ads", "ads"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn glob_match_star_suffix() {
        assert!(glob_match("google*", "google_ads"));
        assert!(glob_match("meta*", "meta_ads"));
        assert!(!glob_match("google*", "meta_ads"));
    }

    #[test]
    fn glob_match_middle_star() {
        assert!(glob_match("*.facebookads.*", "facebookads.account_a"));
        assert!(glob_match("*.facebookads.*", "foo.facebookads.bar"));
        assert!(!glob_match("*.facebookads.*", "adwords.campaign"));
    }
}
