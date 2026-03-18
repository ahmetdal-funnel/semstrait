//! UnionsetPlanner — kind planner for Unionset kinds.
//!
//! Builds UNION ALL branches with NULL-fill for unmapped columns.
//! Each dataset becomes a branch: Scan -> Aggregate -> Project (with NULLs for missing cols).
//! Branches are combined with UNION ALL, then re-aggregated.

use crate::error::PlannerError;
use crate::expr_lower;
use crate::grainset_planner::collect_column_refs;
use crate::kind_planner::{resolve_column_name, KindPlanner, PlanFragment, PlannerContext};
use crate::request::ResolvedQueryRequest;
use semstrait_core::DataType;
use semstrait_ir::{
    AggNode, AggregateMeasure, Aggregation, DslExpr, Field, NodeMeta, PlanNode, ProjectNode,
    ScanNode, Schema, UnionNode,
};
use semstrait_manifest::{CompiledKind, CompiledKindDataset, CompiledKindType, UnionMode};
use std::collections::HashSet;

/// Planner for Unionset kinds — UNION ALL across multiple datasets.
pub struct UnionsetPlanner;

impl KindPlanner for UnionsetPlanner {
    fn supports(&self, kind_type: &CompiledKindType) -> bool {
        matches!(kind_type, CompiledKindType::Unionset { .. })
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
                reason: "unionset kind has no datasets".to_string(),
            });
        }

        // Extract union mode from the kind type.
        let distinct = matches!(
            kind.kind_type,
            CompiledKindType::Unionset { union_mode: UnionMode::Distinct }
        );

        // Build one branch per dataset.
        let branches: Vec<PlanNode> = kind
            .datasets
            .iter()
            .map(|ds| build_union_branch(kind, request, ds, ctx))
            .collect::<Result<Vec<_>, _>>()?;

        // If only one branch, skip the UNION node.
        let union_input = if branches.len() == 1 {
            // Safe: we just verified len == 1.
            branches.into_iter().next().expect("checked len == 1")
        } else {
            // All branches share the same output schema (the unified schema).
            let schema = branches[0].meta().output_schema.clone();
            PlanNode::Union(UnionNode {
                meta: NodeMeta::new(schema),
                inputs: branches,
                distinct,
            })
        };

        // Re-aggregate across all branches.
        let unified_schema = build_unified_schema(request);

        let group_by: Vec<DslExpr> = request
            .dimensions
            .iter()
            .map(|name| DslExpr::Column {
                name: name.clone(),
                qualifier: None,
            })
            .collect();

        let aggregates: Vec<AggregateMeasure> = request
            .measures
            .iter()
            .map(|name| AggregateMeasure {
                function: infer_aggregation(kind, name),
                expr: DslExpr::Column {
                    name: name.clone(),
                    qualifier: None,
                },
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
}

/// Build the unified output schema: dimensions (Utf8) + measures (Float64).
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

/// Infer the top-level re-aggregation function for a measure.
/// For most measures this is SUM (re-summing partial sums).
fn infer_aggregation(kind: &CompiledKind, measure_name: &str) -> Aggregation {
    if let Some(measure) = kind.measures.get(measure_name) {
        let upper = measure.expr_source.to_uppercase();
        if upper.starts_with("COUNT_DISTINCT") || upper.contains("COUNT(DISTINCT") {
            return Aggregation::Sum; // re-sum partial count_distincts is lossy but v1 semantics
        } else if upper.starts_with("MIN") {
            return Aggregation::Min;
        } else if upper.starts_with("MAX") {
            return Aggregation::Max;
        } else if upper.starts_with("AVG") {
            return Aggregation::Sum; // re-aggregating AVG as SUM is approximate, v1 semantics
        }
    }
    Aggregation::Sum
}

/// Build a single UNION branch for one dataset.
///
/// Produces: Scan -> Aggregate -> Project
/// The Project outputs the unified schema, using NULL for unmapped fields.
fn build_union_branch(
    kind: &CompiledKind,
    request: &ResolvedQueryRequest,
    dataset: &CompiledKindDataset,
    ctx: &PlannerContext<'_>,
) -> Result<PlanNode, PlannerError> {
    let mapping = &dataset.extras.column_mapping;

    // Determine which requested fields this dataset covers.
    let mut scan_columns: Vec<String> = Vec::new();
    let mut scan_seen: HashSet<String> = HashSet::new();

    // Map dimensions to physical columns (or mark as NULL-fill).
    let mut dim_physical: Vec<(String, Option<String>)> = Vec::new(); // (semantic, physical?)
    for dim_name in &request.dimensions {
        if let Some(mv) = mapping.get(dim_name) {
            let phys = resolve_column_name(mv).to_string();
            dim_physical.push((dim_name.clone(), Some(phys.clone())));
            if scan_seen.insert(phys.clone()) {
                scan_columns.push(phys);
            }
        } else {
            dim_physical.push((dim_name.clone(), None));
        }
    }

    // Lower measures that this dataset covers.
    let mut lowered_measures: Vec<(String, Option<expr_lower::LoweredMeasure>)> = Vec::new();
    for measure_name in &request.measures {
        if let Some(measure) = kind.measures.get(measure_name) {
            if mapping.contains_key(measure_name) {
                let lowered = expr_lower::lower_measure_with_filters(
                    measure_name,
                    &measure.expr,
                    mapping,
                    &measure.filters,
                )?;
                for agg_measure in &lowered.aggregates {
                    collect_column_refs(
                        &agg_measure.expr,
                        &mut scan_columns,
                        &mut scan_seen,
                    );
                }
                lowered_measures.push((measure_name.clone(), Some(lowered)));
            } else {
                lowered_measures.push((measure_name.clone(), None));
            }
        } else {
            lowered_measures.push((measure_name.clone(), None));
        }
    }

    // Resolve table name.
    let table_name = if let Some(ds) = ctx.manifest.get_dataset(&dataset.name) {
        &ds.name
    } else {
        &dataset.name
    };

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

    // Build Aggregate node (only for covered measures).
    let group_by: Vec<DslExpr> = dim_physical
        .iter()
        .filter_map(|(_, phys)| {
            phys.as_ref().map(|p| DslExpr::Column {
                name: p.clone(),
                qualifier: None,
            })
        })
        .collect();

    let aggregates: Vec<AggregateMeasure> = lowered_measures
        .iter()
        .filter_map(|(_, lowered)| lowered.as_ref())
        .flat_map(|l| l.aggregates.clone())
        .collect();

    // Aggregate schema: covered dimensions + aggregate outputs.
    let mut agg_fields: Vec<Field> = dim_physical
        .iter()
        .filter_map(|(semantic, phys)| {
            phys.as_ref()
                .map(|_| Field::new(semantic.clone(), DataType::Utf8))
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

    // Build Project node — outputs the unified schema.
    // Uncovered dimensions/measures are NULL-filled.
    let unified_schema = build_unified_schema(request);

    let mut project_exprs: Vec<DslExpr> = Vec::new();
    for (semantic, phys) in &dim_physical {
        if phys.is_some() {
            project_exprs.push(DslExpr::Column {
                name: semantic.clone(),
                qualifier: None,
            });
        } else {
            project_exprs.push(DslExpr::Null);
        }
    }
    for (_, lowered) in &lowered_measures {
        project_exprs.push(
            lowered
                .as_ref()
                .map_or(DslExpr::Null, |l| l.post_agg_expr.clone()),
        );
    }

    let project = PlanNode::Project(ProjectNode {
        meta: NodeMeta::new(unified_schema),
        input: Box::new(agg),
        expressions: project_exprs,
    });

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind_planner::PlannerContext;
    use crate::test_helpers::*;
    use indexmap::IndexMap;
    use semstrait_manifest::{
        CompiledKind, CompiledKindDataset, CompiledKindType, CompiledMeasure,
        ColumnMappingValue, KindDatasetExtras,
    };
    use std::collections::HashMap;

    /// Create a Unionset kind with two datasets, each covering different columns.
    fn make_unionset_manifest() -> semstrait_manifest::CompiledManifest {
        let mut dimensions = IndexMap::new();
        for name in &["date", "region"] {
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

        // Dataset 1: covers date + revenue
        let mut mapping1 = HashMap::new();
        mapping1.insert("date".to_string(), ColumnMappingValue::Simple("order_date".to_string()));
        mapping1.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
        mapping1.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));

        let ds1 = CompiledKindDataset {
            name: "orders_us".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping1,
                temporal: None,
                storage: None,
                catalog: None,
            },
        };

        // Dataset 2: covers date + revenue (different physical names)
        let mut mapping2 = HashMap::new();
        mapping2.insert("date".to_string(), ColumnMappingValue::Simple("sale_date".to_string()));
        mapping2.insert("region".to_string(), ColumnMappingValue::Simple("region".to_string()));
        mapping2.insert("revenue".to_string(), ColumnMappingValue::Simple("total".to_string()));

        let ds2 = CompiledKindDataset {
            name: "orders_eu".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping2,
                temporal: None,
                storage: None,
                catalog: None,
            },
        };

        let kind = CompiledKind {
            name: "all_orders".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Unionset { union_mode: UnionMode::All },
            datasets: vec![ds1, ds2],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let mut kinds = IndexMap::new();
        kinds.insert("all_orders".to_string(), kind);

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
    fn test_unionset_two_datasets() {
        let manifest = make_unionset_manifest();
        let kind = manifest.get_kind("all_orders").unwrap();
        let request = make_test_request("all_orders", vec!["date", "region"], vec!["revenue"]);

        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = UnionsetPlanner;
        let result = planner.resolve(kind, &request, &ctx);
        assert!(result.is_ok(), "unionset resolve should succeed: {:?}", result.err());

        let fragment = result.unwrap();

        // Root should be Aggregate (re-aggregation over UNION).
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                // Input should be Union node.
                match agg.input.as_ref() {
                    PlanNode::Union(union_node) => {
                        assert_eq!(union_node.inputs.len(), 2, "should have 2 UNION branches");
                        // Each branch should be a Project node.
                        for branch in &union_node.inputs {
                            assert!(
                                matches!(branch, PlanNode::Project(_)),
                                "each branch should be a Project"
                            );
                        }
                    }
                    _ => panic!("expected Union node under Aggregate"),
                }
            }
            _ => panic!("expected Aggregate node as root"),
        }

        // Check output schema.
        assert_eq!(fragment.output_schema.fields.len(), 3); // date, region, revenue
    }

    #[test]
    fn test_unionset_single_dataset() {
        let manifest = make_unionset_manifest();
        // Modify: create a kind with just one dataset.
        let mut kind = manifest.get_kind("all_orders").unwrap().clone();
        kind.datasets.truncate(1);

        let request = make_test_request("all_orders", vec!["date"], vec!["revenue"]);
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = UnionsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_ok());

        let fragment = result.unwrap();
        // With single dataset, no Union node — just Aggregate over Project.
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                assert!(
                    matches!(agg.input.as_ref(), PlanNode::Project(_)),
                    "single dataset should skip Union node"
                );
            }
            _ => panic!("expected Aggregate as root"),
        }
    }

    #[test]
    fn test_unionset_null_fill() {
        let mut dimensions = IndexMap::new();
        for name in &["date", "region"] {
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

        // Dataset 1: covers date + revenue but NOT region.
        let mut mapping1 = HashMap::new();
        mapping1.insert("date".to_string(), ColumnMappingValue::Simple("order_date".to_string()));
        mapping1.insert("revenue".to_string(), ColumnMappingValue::Simple("amount".to_string()));

        let ds1 = CompiledKindDataset {
            name: "partial_orders".to_string(),
            extras: KindDatasetExtras {
                column_mapping: mapping1,
                temporal: None,
                storage: None,
                catalog: None,
            },
        };

        let kind = CompiledKind {
            name: "partial".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Unionset { union_mode: UnionMode::All },
            datasets: vec![ds1],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let request = make_test_request("partial", vec!["date", "region"], vec!["revenue"]);
        let mut kinds = IndexMap::new();
        kinds.insert("partial".to_string(), kind.clone());
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

        let planner = UnionsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_ok(), "should succeed with NULL-fill for missing region");

        let fragment = result.unwrap();
        // Verify the branch's Project has a NULL expression for the unmapped "region".
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                match agg.input.as_ref() {
                    PlanNode::Project(proj) => {
                        // expressions[1] should be NULL (region is unmapped).
                        assert_eq!(proj.expressions[1], DslExpr::Null);
                    }
                    _ => panic!("expected Project under Aggregate"),
                }
            }
            _ => panic!("expected Aggregate as root"),
        }
    }

    #[test]
    fn test_unionset_empty_datasets() {
        let kind = CompiledKind {
            name: "empty".to_string(),
            description: None,
            dimensions: IndexMap::new(),
            measures: IndexMap::new(),
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Unionset { union_mode: UnionMode::All },
            datasets: vec![],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let request = make_test_request("empty", vec![], vec![]);
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

        let planner = UnionsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::NoCoveringDataset { .. }
        ));
    }

    #[test]
    fn test_unionset_distinct_mode() {
        let manifest = make_unionset_manifest();
        let mut kind = manifest.get_kind("all_orders").unwrap().clone();
        kind.kind_type = CompiledKindType::Unionset { union_mode: UnionMode::Distinct };

        let request = make_test_request("all_orders", vec!["date", "region"], vec!["revenue"]);
        let ctx = PlannerContext {
            manifest: &manifest,
            profile: &semstrait_core::ConsumerProfile::default(),
            catalog: None,
            session: &HashMap::new(),
        };

        let planner = UnionsetPlanner;
        let result = planner.resolve(&kind, &request, &ctx);
        assert!(result.is_ok(), "unionset distinct should succeed");

        let fragment = result.unwrap();
        // Root should be Aggregate over Union with distinct=true.
        match &fragment.root {
            PlanNode::Aggregate(agg) => {
                match agg.input.as_ref() {
                    PlanNode::Union(union_node) => {
                        assert!(union_node.distinct, "union node should be distinct");
                        assert_eq!(union_node.inputs.len(), 2);
                    }
                    _ => panic!("expected Union node under Aggregate"),
                }
            }
            _ => panic!("expected Aggregate as root"),
        }
    }
}
