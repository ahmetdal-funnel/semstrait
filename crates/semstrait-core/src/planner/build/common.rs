//! Shared helpers for plan building.

use crate::diagnostics::{codes, CompileError, Diagnostic};
use crate::dsl;
use crate::schema::model::{
    Dimension, DimensionEntry, DimensionType, Kind, KindDataset, KindDatasetEntry, MeasureEntry,
};

use crate::planner::validate::additivity::{self, AdditivityAction};
use crate::planner::transform::bucketed;
use crate::planner::validate::constraints;

use crate::planner::ir::expr::{AggregateExpr, Column, Expr, Literal};
use crate::planner::ir::plan_node::*;
use crate::planner::transform::temporal::{self, TemporalFilter};
use crate::planner::resolve::joinset::JoinsetPlan;

pub(super) fn get_dataset(kind: &Kind, index: usize) -> Result<&KindDataset, CompileError> {
    match kind.datasets.get(index) {
        Some(KindDatasetEntry::Inline(ds)) => Ok(ds),
        _ => Err(CompileError::single(Diagnostic::error(
            codes::PLAN_E001,
            format!("kind '{}': dataset index {} not found or is a ref", kind.name, index),
        ))),
    }
}

pub(super) fn find_dataset_by_name<'a>(kind: &'a Kind, name: &str) -> Result<&'a KindDataset, CompileError> {
    kind.datasets
        .iter()
        .find_map(|e| match e {
            KindDatasetEntry::Inline(ds) if ds.name == name => Some(ds),
            _ => None,
        })
        .ok_or_else(|| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E001,
                format!("kind '{}': dataset '{}' not found", kind.name, name),
            ))
        })
}

pub(super) fn resolve_table_path(ds: &KindDataset) -> String {
    ds.extras
        .storage
        .as_ref()
        .map(|s| s.path.clone())
        .unwrap_or_else(|| ds.name.clone())
}

pub(super) fn build_temporal_filter(ds: &KindDataset, alias: &str) -> Option<Expr> {
    ds.extras.temporal.as_ref().and_then(|tc| {
        match temporal::temporal_filter(tc, alias) {
            TemporalFilter::Predicate(expr) => Some(expr),
            TemporalFilter::None => None,
        }
    })
}

/// Look up the data_type for a semantic column name from kind dimensions/measures.
pub(super) fn lookup_column_type(kind: &Kind, name: &str) -> String {
    // Check dimensions
    if let Some(dims) = &kind.dimensions {
        for entry in dims {
            if let DimensionEntry::Inline(d) = entry {
                if d.name == name {
                    return d.data_type.to_string();
                }
            }
        }
    }
    // Check measures
    if let Some(measures) = &kind.measures {
        for entry in measures {
            if let MeasureEntry::Inline(m) = entry {
                if m.name == name {
                    return m.data_type.to_string();
                }
            }
        }
    }
    "string".to_string()
}

/// Collect bucketed dimensions from the requested dimension names.
/// Returns Vec of (dim_name, source_column) for bucketed dims.
pub(super) fn collect_bucketed_dims(kind: &Kind, dimensions: &[String]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for dim_name in dimensions {
        if let Some(dim) = find_kind_dimension(kind, dim_name) {
            if let DimensionType::Bucketed(bd) = &dim.dim_type {
                result.push((dim_name.clone(), bd.column.clone()));
            }
        }
    }
    result
}

/// Like `scan_columns_from_mappings` but also includes source columns for bucketed dimensions.
pub(super) fn scan_columns_from_mappings_with_bucketed(
    kind: &Kind,
    mappings: &[(String, String)],
    dimensions: &[String],
    measures: &[String],
    bucketed_dims: &[(String, String)],
) -> (Vec<String>, Vec<String>) {
    let (mut columns, mut types) = scan_columns_from_mappings(kind, mappings, dimensions, measures);
    let seen: std::collections::HashSet<String> = columns.iter().cloned().collect();

    // Add source columns for bucketed dims (the physical column they reference)
    for (_dim_name, source_col) in bucketed_dims {
        // Look up physical name for the source column in mappings
        if let Some(phys) = find_physical(mappings, source_col) {
            if !seen.contains(&phys) {
                columns.push(phys);
                types.push(lookup_column_type(kind, source_col));
            }
        } else if !seen.contains(source_col) {
            // Source column name is the physical name directly
            columns.push(source_col.clone());
            types.push("f64".to_string());
        }
    }

    (columns, types)
}

/// Build a pre-aggregate Project that computes bucketed dimensions as CASE WHEN
/// and passes through other columns.
pub(super) fn build_bucketed_project(
    input: PlanNode,
    kind: &Kind,
    dimensions: &[String],
    measures: &[String],
    mappings: &[(String, String)],
    bucketed_dims: &[(String, String)],
) -> PlanNode {
    let bucketed_names: std::collections::HashSet<&str> =
        bucketed_dims.iter().map(|(name, _)| name.as_str()).collect();

    let mut expressions = Vec::new();

    for dim in dimensions {
        if bucketed_names.contains(dim.as_str()) {
            // Compute CASE WHEN for this bucketed dimension
            if let Some(d) = find_kind_dimension(kind, dim) {
                if let DimensionType::Bucketed(bd) = &d.dim_type {
                    // Find the physical name for the source column
                    let phys_source = find_physical(mappings, &bd.column)
                        .unwrap_or_else(|| bd.column.clone());
                    let case_expr = bucketed::compile_bucketed(bd, "");
                    // Rewrite the column ref to use the physical source column name
                    let case_expr = rewrite_bucketed_column(case_expr, &bd.column, &phys_source);
                    expressions.push(ProjectExpr {
                        expr: case_expr,
                        alias: dim.clone(),
                    });
                    continue;
                }
            }
        }
        // Non-bucketed dim: pass through physical column
        if let Some(phys) = find_physical(mappings, dim) {
            expressions.push(ProjectExpr {
                expr: Expr::Column(Column::unqualified(&phys)),
                alias: dim.clone(),
            });
        }
    }

    // Pass through measure source columns (physical names)
    for measure in measures {
        if let Some(phys) = find_physical(mappings, measure) {
            expressions.push(ProjectExpr {
                expr: Expr::Column(Column::unqualified(&phys)),
                alias: phys.clone(),
            });
        }
    }

    // Pass through any extra columns needed by measure filters
    // (they're already in the Scan, just need to be in the Project output)
    for measure_name in measures {
        if let Some(m) = find_kind_measure(kind, measure_name) {
            if let Some(filters) = &m.filters {
                for mf in filters {
                    for (semantic, physical) in mappings {
                        if mf.expr.contains(semantic.as_str()) {
                            let already_included = expressions.iter().any(|e| e.alias == *physical);
                            if !already_included {
                                expressions.push(ProjectExpr {
                                    expr: Expr::Column(Column::unqualified(physical)),
                                    alias: physical.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    PlanNode::Project(Project {
        input: Box::new(input),
        expressions,
    })
}

/// Rewrite column references in a bucketed CASE expression.
fn rewrite_bucketed_column(expr: Expr, logical_col: &str, physical_col: &str) -> Expr {
    match expr {
        Expr::Column(col) if col.name == logical_col => {
            Expr::Column(Column::unqualified(physical_col))
        }
        Expr::Case { when_then, else_result } => Expr::Case {
            when_then: when_then
                .into_iter()
                .map(|(cond, result)| {
                    (
                        rewrite_bucketed_column(cond, logical_col, physical_col),
                        rewrite_bucketed_column(result, logical_col, physical_col),
                    )
                })
                .collect(),
            else_result: else_result
                .map(|e| Box::new(rewrite_bucketed_column(*e, logical_col, physical_col))),
        },
        Expr::And(exprs) => Expr::And(
            exprs
                .into_iter()
                .map(|e| rewrite_bucketed_column(e, logical_col, physical_col))
                .collect(),
        ),
        Expr::BinaryOp { left, op, right } => Expr::BinaryOp {
            left: Box::new(rewrite_bucketed_column(*left, logical_col, physical_col)),
            op,
            right: Box::new(rewrite_bucketed_column(*right, logical_col, physical_col)),
        },
        other => other,
    }
}

/// Find a dimension definition by name in the kind.
pub(super) fn find_kind_dimension<'a>(kind: &'a Kind, name: &str) -> Option<&'a Dimension> {
    kind.dimensions.as_ref()?.iter().find_map(|entry| match entry {
        DimensionEntry::Inline(d) if d.name == name => Some(d),
        _ => None,
    })
}

/// Find a measure definition by name in the kind.
pub(super) fn find_kind_measure<'a>(kind: &'a Kind, name: &str) -> Option<&'a crate::schema::model::Measure> {
    kind.measures.as_ref()?.iter().find_map(|entry| match entry {
        MeasureEntry::Inline(m) if m.name == name => Some(m),
        _ => None,
    })
}

/// Find the physical column name for a semantic name in a mapping.
pub(super) fn find_physical(mappings: &[(String, String)], semantic: &str) -> Option<String> {
    mappings
        .iter()
        .find(|(s, _)| s == semantic)
        .map(|(_, p)| p.clone())
}

/// Build Scan columns and types from column mappings (for grainset single-dataset).
pub(super) fn scan_columns_from_mappings(
    kind: &Kind,
    mappings: &[(String, String)],
    dimensions: &[String],
    measures: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut columns = Vec::new();
    let mut types = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for dim in dimensions {
        if let Some(phys) = find_physical(mappings, dim) {
            if seen.insert(phys.clone()) {
                columns.push(phys);
                types.push(lookup_column_type(kind, dim));
            }
        }
    }
    for measure in measures {
        if let Some(phys) = find_physical(mappings, measure) {
            if seen.insert(phys.clone()) {
                columns.push(phys);
                types.push(lookup_column_type(kind, measure));
            }
        }
    }

    // Include columns referenced by measure filters
    for measure_name in measures {
        if let Some(m) = find_kind_measure(kind, measure_name) {
            if let Some(filters) = &m.filters {
                for mf in filters {
                    for (semantic, physical) in mappings {
                        if mf.expr.contains(semantic.as_str()) && seen.insert(physical.clone()) {
                            columns.push(physical.clone());
                            types.push(lookup_column_type(kind, semantic));
                        }
                    }
                }
            }
        }
    }

    (columns, types)
}

/// Build Scan columns for joinset datasets (all mapped columns).
pub(super) fn scan_columns_for_joinset(
    kind: &Kind,
    mappings: Option<&Vec<(String, String)>>,
    ds: &KindDataset,
) -> (Vec<String>, Vec<String>) {
    match mappings {
        Some(maps) => {
            let mut columns = Vec::new();
            let mut types = Vec::new();
            for (semantic, physical) in maps {
                columns.push(physical.clone());
                types.push(lookup_column_type(kind, semantic));
            }
            (columns, types)
        }
        None => {
            // No columns needed from this dataset; include join key columns
            let mut columns = Vec::new();
            let mut types = Vec::new();
            for value in ds.extras.column_mapping.values() {
                let phys = match value {
                    crate::schema::model::ColumnMappingValue::Simple(p) => p.clone(),
                    crate::schema::model::ColumnMappingValue::Complex { column, .. } => {
                        column.clone()
                    }
                };
                columns.push(phys);
                types.push("string".to_string());
            }
            (columns, types)
        }
    }
}

/// Parse a measure's DSL expr into an AggregateExpr with physical column references.
fn build_aggregate_expr(
    kind: &Kind,
    measure_name: &str,
    mappings: &[(String, String)],
) -> Result<AggregateExpr, CompileError> {
    let measure = find_kind_measure(kind, measure_name).ok_or_else(|| {
        CompileError::single(Diagnostic::error(
            codes::PLAN_E004,
            format!("kind '{}': measure '{}' not found", kind.name, measure_name),
        ))
    })?;

    let dsl_ast = dsl::parse_dsl(&measure.expr).map_err(|e| {
        CompileError::single(Diagnostic::error(
            codes::PLAN_E004,
            format!("measure '{}': DSL parse error: {}", measure_name, e),
        ))
    })?;

    let (agg, inner, alias) = dsl::lower_aggregate(&dsl_ast, measure_name).map_err(|e| {
        CompileError::single(Diagnostic::error(
            codes::PLAN_E004,
            format!("measure '{}': lower error: {}", measure_name, e),
        ))
    })?;

    // Rewrite unqualified column refs to use physical names from column_mapping
    let inner = rewrite_columns(inner, mappings);

    // Wrap in CASE WHEN if measure has filters
    let inner = wrap_measure_filters(inner, measure, mappings)?;

    Ok(AggregateExpr {
        func: agg,
        expr: inner,
        alias,
    })
}

/// If the measure has filters, wrap the expression in:
/// `CASE WHEN <filter1> AND <filter2> ... THEN <expr> ELSE NULL END`
fn wrap_measure_filters(
    inner: Expr,
    measure: &crate::schema::model::Measure,
    mappings: &[(String, String)],
) -> Result<Expr, CompileError> {
    let filters = match &measure.filters {
        Some(f) if !f.is_empty() => f,
        _ => return Ok(inner),
    };

    // Parse and lower each filter expression, then combine with AND
    let mut filter_exprs = Vec::new();
    for mf in filters {
        let dsl_ast = dsl::parse_dsl(&mf.expr).map_err(|e| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!("measure filter '{}': DSL parse error: {}", mf.name, e),
            ))
        })?;
        let expr = dsl::lower_expr(&dsl_ast).map_err(|e| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!("measure filter '{}': lower error: {}", mf.name, e),
            ))
        })?;
        filter_exprs.push(rewrite_columns(expr, mappings));
    }

    let condition = if filter_exprs.len() == 1 {
        filter_exprs.into_iter().next().unwrap()
    } else {
        Expr::And(filter_exprs)
    };

    Ok(Expr::Case {
        when_then: vec![(condition, inner)],
        else_result: Some(Box::new(Expr::Literal(Literal::Null(
            measure.data_type.to_string(),
        )))),
    })
}

/// Collect extra dimensions needed for semi-additive pre-resolution.
pub(super) fn collect_additivity_extra_dims(
    kind: &Kind,
    measures: &[String],
    dimensions: &[String],
) -> Vec<String> {
    let mut extra = Vec::new();
    for measure_name in measures {
        let measure = find_kind_measure(kind, measure_name);
        let additivity = measure.and_then(|m| m.additivity.as_ref());
        if let Ok(AdditivityAction::PreResolve { dimensions: dims, .. }) =
            additivity::resolve_additivity(measure_name, additivity, dimensions)
        {
            for d in dims {
                if !extra.contains(&d) && !dimensions.contains(&d) {
                    extra.push(d);
                }
            }
        }
    }
    extra
}

/// Check if any measure is semi-additive and wrap the Aggregate node in a two-stage
/// aggregation if needed.
pub(super) fn maybe_wrap_additivity(
    node: PlanNode,
    kind: &Kind,
    measures: &[String],
    dimensions: &[String],
    mappings: Option<&[(String, String)]>,
) -> Result<PlanNode, CompileError> {
    // Resolve additivity for each measure
    let mut actions = Vec::new();
    for measure_name in measures {
        let measure = find_kind_measure(kind, measure_name);
        let additivity = measure.and_then(|m| m.additivity.as_ref());
        let action = additivity::resolve_additivity(measure_name, additivity, dimensions)
            .map_err(CompileError::single)?;
        actions.push(action);
    }

    // Check for SourceGrainRequired
    for (i, action) in actions.iter().enumerate() {
        if matches!(action, AdditivityAction::SourceGrainRequired) {
            return Err(CompileError::single(Diagnostic::error(
                codes::PLAN_E001,
                format!(
                    "measure '{}' is non-additive and requires source-grain dataset",
                    measures[i]
                ),
            )));
        }
    }

    // If all Standard, no wrapping needed
    if actions.iter().all(|a| matches!(a, AdditivityAction::Standard)) {
        return Ok(node);
    }

    // Collect pre-resolve dimensions and per-measure strategy info
    let mut extra_dims = Vec::new();
    let mut pre_resolve_info: Vec<Option<crate::schema::model::ResolutionStrategy>> = Vec::new();

    for action in &actions {
        match action {
            AdditivityAction::PreResolve { dimensions: dims, strategy } => {
                for d in dims {
                    if !extra_dims.contains(d) && !dimensions.contains(d) {
                        extra_dims.push(d.clone());
                    }
                }
                pre_resolve_info.push(Some(*strategy));
            }
            _ => {
                pre_resolve_info.push(None);
            }
        }
    }

    if extra_dims.is_empty() {
        return Ok(node);
    }

    // Extract the existing Aggregate node
    let (input, old_group_by, old_aggregates) = match node {
        PlanNode::Aggregate(agg) => (*agg.input, agg.group_by, agg.aggregates),
        _ => return Ok(node),
    };

    // Inner GROUP BY = old GROUP BY + extra dims (converted to physical if mappings present)
    let mut inner_group_by = old_group_by.clone();
    for dim in &extra_dims {
        let col = if let Some(maps) = mappings {
            let phys = find_physical(maps, dim).unwrap_or_else(|| dim.clone());
            Column::unqualified(&phys)
        } else {
            Column::unqualified(dim)
        };
        inner_group_by.push(col);
    }

    // Inner aggregates: semi-additive measures use resolution strategy agg
    let inner_aggregates: Vec<AggregateExpr> = old_aggregates
        .iter()
        .zip(pre_resolve_info.iter())
        .map(|(agg, strategy)| {
            if let Some(strat) = strategy {
                let func_str = additivity::resolution_strategy_agg(*strat);
                let func = match func_str {
                    "MAX" => crate::schema::Aggregation::Max,
                    "MIN" => crate::schema::Aggregation::Min,
                    _ => agg.func,
                };
                AggregateExpr {
                    func,
                    expr: agg.expr.clone(),
                    alias: agg.alias.clone(),
                }
            } else {
                agg.clone()
            }
        })
        .collect();

    let inner_node = PlanNode::Aggregate(Aggregate {
        input: Box::new(input),
        group_by: inner_group_by,
        aggregates: inner_aggregates,
    });

    // Outer GROUP BY = old GROUP BY (no extra dims)
    let outer_group_by = old_group_by;

    // Outer aggregates: reference inner output aliases, use declared agg function
    let outer_aggregates: Vec<AggregateExpr> = old_aggregates
        .into_iter()
        .map(|agg| AggregateExpr {
            func: agg.func,
            expr: Expr::Column(Column::unqualified(&agg.alias)),
            alias: agg.alias,
        })
        .collect();

    Ok(PlanNode::Aggregate(Aggregate {
        input: Box::new(inner_node),
        group_by: outer_group_by,
        aggregates: outer_aggregates,
    }))
}

/// Validate that aggregate expressions don't apply distributional aggs (SUM, AVG) on key columns.
pub(super) fn validate_key_aggregations(
    kind: &Kind,
    aggregates: &[AggregateExpr],
    measures: &[String],
) -> Result<(), CompileError> {
    let keys = match &kind.keys {
        Some(k) => k,
        None => return Ok(()),
    };

    for (agg, measure_name) in aggregates.iter().zip(measures.iter()) {
        // Extract column name from the aggregate's inner expression
        let col_name = match &agg.expr {
            Expr::Column(col) => Some(col.name.as_str()),
            _ => None,
        };
        if let Some(col) = col_name {
            constraints::check_key_aggregation(
                measure_name,
                &agg.func.to_string(),
                col,
                keys,
            )
            .map_err(CompileError::single)?;
        }
    }

    Ok(())
}

/// Build AggregateExprs for measures using physical column mappings.
pub(super) fn build_aggregates(
    kind: &Kind,
    measures: &[String],
    mappings: &[(String, String)],
) -> Result<Vec<AggregateExpr>, CompileError> {
    let aggregates: Vec<AggregateExpr> = measures
        .iter()
        .map(|m| build_aggregate_expr(kind, m, mappings))
        .collect::<Result<_, _>>()?;
    validate_key_aggregations(kind, &aggregates, measures)?;
    Ok(aggregates)
}

/// Build AggregateExprs for measures using semantic column names (post-Project/Union).
pub(super) fn build_aggregates_semantic(
    kind: &Kind,
    measures: &[String],
) -> Result<Vec<AggregateExpr>, CompileError> {
    let identity_mappings: Vec<(String, String)> =
        measures.iter().map(|m| (m.clone(), m.clone())).collect();
    build_aggregates(kind, measures, &identity_mappings)
}

/// Rewrite unqualified Column references in an Expr to use physical names.
fn rewrite_columns(expr: Expr, mappings: &[(String, String)]) -> Expr {
    match expr {
        Expr::Column(col) if col.table.is_empty() => {
            // Look up physical name for this semantic column
            let phys = mappings
                .iter()
                .find(|(s, _)| s == &col.name)
                .map(|(_, p)| p.clone())
                .unwrap_or(col.name);
            Expr::Column(Column::unqualified(&phys))
        }
        Expr::Add(l, r) => Expr::Add(
            Box::new(rewrite_columns(*l, mappings)),
            Box::new(rewrite_columns(*r, mappings)),
        ),
        Expr::Subtract(l, r) => Expr::Subtract(
            Box::new(rewrite_columns(*l, mappings)),
            Box::new(rewrite_columns(*r, mappings)),
        ),
        Expr::Multiply(l, r) => Expr::Multiply(
            Box::new(rewrite_columns(*l, mappings)),
            Box::new(rewrite_columns(*r, mappings)),
        ),
        Expr::Divide(l, r) => Expr::Divide(
            Box::new(rewrite_columns(*l, mappings)),
            Box::new(rewrite_columns(*r, mappings)),
        ),
        other => other,
    }
}

/// Build a Project that renames physical columns to semantic names (for single-dataset grainset).
pub(super) fn build_rename_project(
    input: PlanNode,
    dimensions: &[String],
    measures: &[String],
    mappings: &[(String, String)],
) -> PlanNode {
    let mut expressions = Vec::new();

    for dim in dimensions {
        if let Some(phys) = find_physical(mappings, dim) {
            expressions.push(ProjectExpr {
                expr: Expr::Column(Column::unqualified(&phys)),
                alias: dim.clone(),
            });
        }
    }

    for measure in measures {
        // Aggregate output alias is the measure name (set in build_aggregate_expr)
        expressions.push(ProjectExpr {
            expr: Expr::Column(Column::unqualified(measure)),
            alias: measure.clone(),
        });
    }

    PlanNode::Project(Project {
        input: Box::new(input),
        expressions,
    })
}

/// Build a Project that aligns schema for grainset union branches (physical → semantic rename).
pub(super) fn build_align_project(
    input: PlanNode,
    dimensions: &[String],
    measures: &[String],
    mappings: &[(String, String)],
) -> PlanNode {
    let mut expressions = Vec::new();

    for dim in dimensions {
        let expr = match find_physical(mappings, dim) {
            Some(phys) => Expr::Column(Column::unqualified(&phys)),
            None => Expr::Literal(Literal::Null("string".to_string())),
        };
        expressions.push(ProjectExpr {
            expr,
            alias: dim.clone(),
        });
    }

    for measure in measures {
        let expr = match find_physical(mappings, measure) {
            Some(phys) => Expr::Column(Column::unqualified(&phys)),
            None => Expr::Literal(Literal::Null("float64".to_string())),
        };
        expressions.push(ProjectExpr {
            expr,
            alias: measure.clone(),
        });
    }

    PlanNode::Project(Project {
        input: Box::new(input),
        expressions,
    })
}

/// Build Project expressions for joinset (select from _left/_right aliases).
pub(super) fn build_joinset_project_exprs(
    _kind: &Kind,
    plan: &JoinsetPlan,
    dimensions: &[String],
    measures: &[String],
) -> Vec<ProjectExpr> {
    let mut expressions = Vec::new();

    for col_name in dimensions.iter().chain(measures.iter()) {
        // Find which dataset has this column
        let physical = plan
            .column_mappings
            .iter()
            .find_map(|(_ds_name, mappings)| {
                mappings
                    .iter()
                    .find(|(sem, _)| sem == col_name)
                    .map(|(_, phys)| phys.clone())
            });

        let expr = match physical {
            Some(phys) => Expr::Column(Column::unqualified(&phys)),
            None => Expr::Literal(Literal::Null("string".to_string())),
        };

        expressions.push(ProjectExpr {
            expr,
            alias: col_name.clone(),
        });
    }

    expressions
}

/// Convert schema JoinType to plan_node JoinType.
pub(super) fn convert_join_type(jt: crate::schema::model::JoinType) -> JoinType {
    match jt {
        crate::schema::model::JoinType::Inner => JoinType::Inner,
        crate::schema::model::JoinType::Left => JoinType::Left,
        crate::schema::model::JoinType::Right => JoinType::Right,
        crate::schema::model::JoinType::Full => JoinType::Full,
    }
}
