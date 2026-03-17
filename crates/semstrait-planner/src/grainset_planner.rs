//! GrainsetPlanner — kind planner for Grainset kinds.
//!
//! Routes to the cheapest covering dataset. A covering dataset is one whose
//! column_mapping includes all requested dimensions and measures.
//! If no single dataset covers everything, returns an error (v1).
//! v2 will support FULL OUTER JOIN across multiple datasets.

use crate::error::PlannerError;
use crate::expr_lower;
use crate::kind_planner::{KindPlanner, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, Aggregation, AggregateMeasure, DslExpr, NodeMeta, PlanNode, ProjectNode, ScanNode,
    Schema, Field,
};
use semstrait_manifest::{CompiledKind, CompiledKindDataset, CompiledKindType};
use std::collections::HashSet;
use semstrait_manifest::ColumnMappingValue;

/// Recursively collect all column references from a DslExpr tree.
fn collect_column_refs(expr: &DslExpr, columns: &mut Vec<String>, seen: &mut HashSet<String>) {
    match expr {
        DslExpr::Column { name, .. } => {
            if seen.insert(name.clone()) {
                columns.push(name.clone());
            }
        }
        DslExpr::BinaryOp { left, right, .. } => {
            collect_column_refs(left, columns, seen);
            collect_column_refs(right, columns, seen);
        }
        DslExpr::Case { when_then, else_expr } => {
            for (when, then) in when_then {
                collect_column_refs(when, columns, seen);
                collect_column_refs(then, columns, seen);
            }
            if let Some(e) = else_expr {
                collect_column_refs(e, columns, seen);
            }
        }
        DslExpr::FunctionCall { args, .. } => {
            for arg in args {
                collect_column_refs(arg, columns, seen);
            }
        }
        DslExpr::Negate(e) | DslExpr::Not(e) | DslExpr::IsNull(e) | DslExpr::IsNotNull(e) => {
            collect_column_refs(e, columns, seen);
        }
        DslExpr::InList { expr, list, .. } => {
            collect_column_refs(expr, columns, seen);
            for item in list {
                collect_column_refs(item, columns, seen);
            }
        }
        DslExpr::Between { expr, low, high, .. } => {
            collect_column_refs(expr, columns, seen);
            collect_column_refs(low, columns, seen);
            collect_column_refs(high, columns, seen);
        }
        DslExpr::Like { expr, pattern } => {
            collect_column_refs(expr, columns, seen);
            collect_column_refs(pattern, columns, seen);
        }
        DslExpr::Coalesce(exprs) => {
            for e in exprs {
                collect_column_refs(e, columns, seen);
            }
        }
        DslExpr::NullIf { expr, null_expr } => {
            collect_column_refs(expr, columns, seen);
            collect_column_refs(null_expr, columns, seen);
        }
        DslExpr::DateTrunc { expr, .. } => {
            collect_column_refs(expr, columns, seen);
        }
        DslExpr::Number(_) | DslExpr::StringLit(_) | DslExpr::Bool(_) | DslExpr::Null => {}
    }
}

/// Planner for Grainset kinds — route to cheapest covering dataset.
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
        // Find a covering dataset.
        let dataset_binding = find_covering_dataset(kind, request)?;

        // Resolve the physical table name from the manifest.
        let table_name = resolve_table_name(dataset_binding, ctx)?;

        // Build the plan: Scan -> Aggregate -> Project.
        build_grainset_plan(kind, request, dataset_binding, table_name)
    }
}

/// Find the first dataset whose column_mapping covers all requested
/// dimensions and measures.
fn find_covering_dataset<'a>(
    kind: &'a CompiledKind,
    request: &ResolvedQueryRequest,
) -> Result<&'a CompiledKindDataset, PlannerError> {
    let needed: Vec<&str> = request
        .dimensions
        .iter()
        .chain(request.measures.iter())
        .map(|s| s.as_str())
        .collect();

    // Score each dataset by how many needed fields it covers.
    let mut best: Option<(&CompiledKindDataset, usize)> = None;

    for ds in &kind.datasets {
        let covered = needed
            .iter()
            .filter(|name| ds.extras.column_mapping.contains_key(**name))
            .count();

        if covered == needed.len() {
            // Full coverage — pick this one (first wins as "cheapest" heuristic).
            return Ok(ds);
        }

        match best {
            None => best = Some((ds, covered)),
            Some((_, prev_covered)) if covered > prev_covered => {
                best = Some((ds, covered));
            }
            _ => {}
        }
    }

    Err(PlannerError::NoCoveringDataset {
        kind: kind.name.clone(),
        reason: format!(
            "no single dataset covers all requested fields: [{}]",
            needed.join(", ")
        ),
    })
}

/// Resolve the physical table name for a dataset binding.
fn resolve_table_name<'a>(
    dataset_binding: &'a CompiledKindDataset,
    ctx: &'a PlannerContext<'_>,
) -> Result<&'a str, PlannerError> {
    // Look up the dataset in the manifest to get the table_name.
    // For v1, we use the dataset name as the table name if not found.
    if let Some(dataset) = ctx.manifest.get_dataset(&dataset_binding.name) {
        Ok(&dataset.name)
    } else {
        Ok(&dataset_binding.name)
    }
}

/// Resolve the physical column name from a column mapping value.
fn resolve_column_name(mapping_value: &ColumnMappingValue) -> &str {
    match mapping_value {
        ColumnMappingValue::Simple(s) => s.as_str(),
        ColumnMappingValue::WithGrain { column, .. } => column.as_str(),
    }
}

/// Build Scan -> Aggregate -> Project for a grainset query.
fn build_grainset_plan(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    dataset_binding: &CompiledKindDataset,
    table_name: &str,
) -> Result<PlanFragment, PlannerError> {
    let mapping = &dataset_binding.extras.column_mapping;

    // Collect all physical columns we need to scan (preserving insertion order).
    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_columns_seen: HashSet<String> = HashSet::new();

    // Map dimensions to physical columns.
    let mut dim_physical: Vec<(String, String)> = Vec::new(); // (semantic, physical)
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
        if scan_columns_seen.insert(phys.clone()) {
            scan_columns.push(phys);
        }
    }

    // Lower measures via the parsed DslExpr tree (not raw string parsing).
    // Each entry: (measure_name, LoweredMeasure)
    let mut lowered_measures: Vec<(String, expr_lower::LoweredMeasure)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            let lowered =
                expr_lower::lower_measure_with_filters(
                    measure_name,
                    &measure.expr,
                    mapping,
                    &measure.filters,
                )?;

            // Collect all physical columns referenced by aggregate expressions,
            // including those nested inside CASE/binary/function expressions.
            for agg_measure in &lowered.aggregates {
                collect_column_refs(&agg_measure.expr, &mut scan_columns, &mut scan_columns_seen);
            }
            lowered_measures.push((measure_name.clone(), lowered));
        } else if kind.metrics.contains_key(measure_name) {
            // Metrics are derived — for v1 we treat them like a measure column.
            let physical = mapping
                .get(measure_name)
                .map(resolve_column_name)
                .unwrap_or(measure_name.as_str());
            let phys = physical.to_string();
            let lowered = expr_lower::LoweredMeasure {
                aggregates: vec![AggregateMeasure {
                    function: Aggregation::Sum,
                    expr: DslExpr::Column {
                        name: phys.clone(),
                        qualifier: None,
                    },
                    distinct: false,
                }],
                post_agg_expr: DslExpr::Column {
                    name: measure_name.clone(),
                    qualifier: None,
                },
            };
            if scan_columns_seen.insert(phys.clone()) {
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

    // Build Scan node.
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

    // Build Aggregate node.
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

    // Aggregate output schema: group_by columns + one field per AggregateMeasure.
    // For composed measures (e.g. SUM(a)/COUNT(b)), there are multiple aggregates
    // per semantic measure. Primary aggregate gets the semantic name; extras get synthetic names.
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

    // Build Project node — maps physical names back to semantic names.
    // Dimensions are simple column refs; measures use the lowered post_agg_expr.
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

    let project_fields: Vec<Field> = request.dimensions.iter()
        .map(|name| Field::new(name.clone(), DataType::Utf8))
        .chain(lowered_measures.iter().map(|(name, _)| Field::new(name.clone(), DataType::Float64)))
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

/// Parse an aggregation function from a measure expression source string.
/// This is a simplified parser for v1 — supports common patterns like "SUM(col)".
/// Kept for backward compatibility and test coverage; primary path now uses expr_lower.
#[allow(dead_code)]
fn parse_aggregation(expr_source: &str) -> Aggregation {
    let upper = expr_source.trim().to_uppercase();
    if upper.starts_with("SUM") {
        Aggregation::Sum
    } else if upper.starts_with("AVG") {
        Aggregation::Avg
    } else if upper.starts_with("COUNT_DISTINCT") || upper.starts_with("COUNT(DISTINCT") {
        Aggregation::CountDistinct
    } else if upper.starts_with("COUNT") {
        Aggregation::Count
    } else if upper.starts_with("MIN") {
        Aggregation::Min
    } else if upper.starts_with("MAX") {
        Aggregation::Max
    } else {
        // Default to Sum for unrecognized patterns.
        Aggregation::Sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[test]
    fn test_parse_aggregation() {
        assert_eq!(parse_aggregation("SUM(amount)"), Aggregation::Sum);
        assert_eq!(parse_aggregation("avg(price)"), Aggregation::Avg);
        assert_eq!(parse_aggregation("COUNT(*)"), Aggregation::Count);
        assert_eq!(
            parse_aggregation("COUNT_DISTINCT(user_id)"),
            Aggregation::CountDistinct
        );
        assert_eq!(parse_aggregation("MIN(ts)"), Aggregation::Min);
        assert_eq!(parse_aggregation("MAX(ts)"), Aggregation::Max);
        assert_eq!(parse_aggregation("unknown_func"), Aggregation::Sum);
    }

    #[test]
    fn test_find_covering_dataset() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let result = find_covering_dataset(kind, &request);
        assert!(result.is_ok(), "should find a covering dataset");
        let dataset = result.unwrap();
        assert_eq!(dataset.name, "orders_daily");
    }

    #[test]
    fn test_no_covering_dataset() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        // Request a dimension that doesn't exist in the mapping.
        let request = make_test_request("orders", vec!["nonexistent_dim"], vec!["revenue"]);

        let result = find_covering_dataset(kind, &request);
        assert!(result.is_err(), "should fail when no dataset covers fields");
        assert!(matches!(result.unwrap_err(), PlannerError::NoCoveringDataset { .. }));
    }

    #[test]
    fn test_build_grainset_plan() {
        let manifest = make_test_manifest();
        let kind = manifest.get_kind("orders").unwrap();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        let dataset = find_covering_dataset(kind, &request).unwrap();

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &std::collections::HashMap::new(),
        };

        let table_name = resolve_table_name(dataset, &ctx).unwrap();
        let result = build_grainset_plan(kind, &request, dataset, &table_name);

        assert!(result.is_ok(), "build_grainset_plan should succeed");
        let fragment = result.unwrap();

        // Verify plan structure: should have Project -> Aggregate -> Scan
        match &fragment.root {
            PlanNode::Project(project_node) => {
                // Check that the input is an Aggregate node
                match project_node.input.as_ref() {
                    PlanNode::Aggregate(agg_node) => {
                        // Check that the input is a Scan node
                        assert!(matches!(agg_node.input.as_ref(), PlanNode::Scan(_)));
                    }
                    _ => panic!("Expected Aggregate node under Project"),
                }
            }
            _ => panic!("Expected Project node as root"),
        }
    }
}
