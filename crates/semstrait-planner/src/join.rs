//! Shared join utilities and ad-hoc join resolution.
//!
//! Provides:
//! - `resolve_from_fields()` — infer the `FROM` entity when omitted, using FieldIndex
//! - `map_join_type()` — model JoinType → IR JoinType mapping
//!
//! The FieldIndex and RelationshipGraph are pre-computed during manifest compilation.
//! This module consumes those structures to resolve ad-hoc join queries.

use semstrait_ir::JoinType as IrJoinType;
use semstrait_manifest::{CompiledManifest, CompiledRelationship, JoinType as ModelJoinType};

use crate::error::PlannerError;

/// Map model JoinType to IR JoinType.
#[allow(dead_code)] // Used during ad-hoc join plan construction
pub fn map_join_type(jt: &ModelJoinType) -> IrJoinType {
    match jt {
        ModelJoinType::Inner => IrJoinType::Inner,
        ModelJoinType::Left => IrJoinType::Left,
        ModelJoinType::Right => IrJoinType::Right,
        ModelJoinType::Full => IrJoinType::Full,
    }
}

/// Result of field-based entity resolution.
#[derive(Debug, Clone)]
pub struct ResolvedFromFields {
    /// The datasets needed to satisfy the requested fields.
    pub datasets: Vec<String>,
    /// The join path (relationship indices) connecting the datasets.
    /// Empty if only one dataset is needed.
    pub join_path: Vec<usize>,
    /// The classified fields: which field comes from which dataset.
    #[allow(dead_code)] // Used during ad-hoc join plan construction
    pub field_providers: Vec<(String, String)>,
}

/// Resolve which datasets and joins are needed to satisfy a set of requested fields.
///
/// When `FROM` is omitted in a query, this function uses the pre-computed `FieldIndex`
/// to find provider datasets for each requested field, then uses the `RelationshipGraph`
/// to find the shortest join path connecting all needed datasets.
///
/// Returns an error if:
/// - Any requested field has no provider
/// - Multiple providers exist for a field (ambiguity)
/// - No join path exists between needed datasets
pub fn resolve_from_fields(
    requested_fields: &[String],
    manifest: &CompiledManifest,
) -> Result<ResolvedFromFields, PlannerError> {
    let field_index = &manifest.field_index;
    let rel_graph = &manifest.relationship_graph;

    // Step 1: For each field, find the provider dataset(s).
    let mut field_providers: Vec<(String, String)> = Vec::new();
    let mut needed_datasets: Vec<String> = Vec::new();
    let mut seen_datasets = std::collections::HashSet::new();

    for field in requested_fields {
        let providers = field_index.providers.get(field);

        match providers {
            None => {
                // Check if it's a metric (kind-level, no single provider dataset).
                if field_index.all_metrics.contains(field) {
                    // Metrics are resolved at kind level — skip provider assignment.
                    // They'll be handled during metric resolution in the planner.
                    continue;
                }
                return Err(PlannerError::Internal(format!(
                    "field '{}' has no provider dataset in the field index",
                    field
                )));
            }
            Some(ds_list) if ds_list.is_empty() => {
                return Err(PlannerError::Internal(format!(
                    "field '{}' has an empty provider list",
                    field
                )));
            }
            Some(ds_list) => {
                // Use the first provider (deterministic ordering from compilation).
                let provider = &ds_list[0];
                field_providers.push((field.clone(), provider.clone()));
                if seen_datasets.insert(provider.clone()) {
                    needed_datasets.push(provider.clone());
                }
            }
        }
    }

    if needed_datasets.is_empty() {
        return Err(PlannerError::Internal(
            "no datasets needed for the requested fields".to_string(),
        ));
    }

    // Step 2: If only one dataset, no join needed.
    if needed_datasets.len() == 1 {
        return Ok(ResolvedFromFields {
            datasets: needed_datasets,
            join_path: vec![],
            field_providers,
        });
    }

    // Step 3: Find the join path connecting all needed datasets.
    // Strategy: start from the first dataset and greedily connect others.
    let mut connected = vec![needed_datasets[0].clone()];
    let mut all_join_indices: Vec<usize> = Vec::new();

    for ds in &needed_datasets[1..] {
        // Try to find a shortest path from any connected dataset to this one.
        let mut found = false;
        for connected_ds in &connected {
            if let Some(path) = rel_graph.shortest_path(connected_ds, ds) {
                all_join_indices.extend(path);
                found = true;
                break;
            }
            // Try reverse direction.
            if let Some(path) = rel_graph.shortest_path(ds, connected_ds) {
                all_join_indices.extend(path);
                found = true;
                break;
            }
        }

        if !found {
            return Err(PlannerError::NoCoveringDataset {
                kind: "(ad-hoc)".to_string(),
                reason: format!(
                    "no join path between datasets '{}' and '{}'",
                    connected[0], ds
                ),
            });
        }
        connected.push(ds.clone());
    }

    // Deduplicate join indices (same relationship may appear in multiple paths).
    all_join_indices.sort();
    all_join_indices.dedup();

    Ok(ResolvedFromFields {
        datasets: needed_datasets,
        join_path: all_join_indices,
        field_providers,
    })
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
    use indexmap::IndexMap;
    use semstrait_manifest::{
        CompiledDataset, CompiledDimension, CompiledManifest, CompiledMeasure,
        CompileDiagnostics, DimensionType, FieldIndex, RelationshipGraph,
    };
    use std::collections::{HashMap, HashSet};

    fn make_manifest_with_field_index() -> CompiledManifest {
        let mut datasets = IndexMap::new();

        // Dataset: orders — has date, region, revenue
        let mut order_dims = IndexMap::new();
        order_dims.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );
        order_dims.insert(
            "region".to_string(),
            CompiledDimension {
                name: "region".to_string(),
                description: None,
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );

        let mut order_measures = IndexMap::new();
        order_measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: semstrait_core::DataType::Float64,
                agg: None,
                expr: semstrait_core::Expr::entity_ref("SUM(amount)"),
                expr_source: "SUM(amount)".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        datasets.insert(
            "orders".to_string(),
            CompiledDataset {
                name: "orders".to_string(),
                description: None,
                domain: None,
                keys: None,
                dimensions: order_dims,
                measures: order_measures,
                metrics: IndexMap::new(),
                compiled_schema: None,
            },
        );

        // Dataset: customers — has customer_name
        let mut cust_dims = IndexMap::new();
        cust_dims.insert(
            "customer_name".to_string(),
            CompiledDimension {
                name: "customer_name".to_string(),
                description: None,
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );

        datasets.insert(
            "customers".to_string(),
            CompiledDataset {
                name: "customers".to_string(),
                description: None,
                domain: None,
                keys: None,
                dimensions: cust_dims,
                measures: IndexMap::new(),
                metrics: IndexMap::new(),
                compiled_schema: None,
            },
        );

        // Build FieldIndex.
        let mut providers: HashMap<String, Vec<String>> = HashMap::new();
        providers.insert("date".to_string(), vec!["orders".to_string()]);
        providers.insert("region".to_string(), vec!["orders".to_string()]);
        providers.insert("revenue".to_string(), vec!["orders".to_string()]);
        providers.insert("customer_name".to_string(), vec!["customers".to_string()]);

        let all_dimensions: HashSet<String> =
            ["date", "region", "customer_name"].iter().map(|s| s.to_string()).collect();
        let all_measures: HashSet<String> = ["revenue"].iter().map(|s| s.to_string()).collect();

        let field_index = FieldIndex {
            providers,
            all_dimensions,
            all_measures,
            all_metrics: HashSet::new(),
        };

        // Build RelationshipGraph with a path from orders to customers.
        let mut rel_graph = RelationshipGraph::default();
        rel_graph
            .forward
            .insert("orders".to_string(), vec![("customers".to_string(), 0)]);
        rel_graph
            .reverse
            .insert("customers".to_string(), vec![("orders".to_string(), 0)]);
        rel_graph.set_shortest_path("orders", "customers", vec![0]);

        let relationships = vec![CompiledRelationship {
            name: "orders_customers".to_string(),
            from: "orders".to_string(),
            to: "customers".to_string(),
            join_type: semstrait_manifest::JoinType::Left,
            columns: vec![semstrait_manifest::JoinColumnPair {
                from: "customer_id".to_string(),
                to: "id".to_string(),
            }],
            cardinality: semstrait_manifest::Cardinality::ManyToOne,
        }];

        CompiledManifest {
            version: 2,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_join".to_string(),
            datasets,
            kinds: IndexMap::new(),
            data_kinds: IndexMap::new(),
            relationships,
            relationship_graph: rel_graph,
            field_index,
            diagnostics: CompileDiagnostics::default(),
            model_name: "test_join".to_string(),
            model_description: None,
            catalog_snapshot: None,
        }
    }

    #[test]
    fn test_resolve_single_dataset() {
        let manifest = make_manifest_with_field_index();
        let fields = vec!["date".to_string(), "revenue".to_string()];

        let result = resolve_from_fields(&fields, &manifest);
        assert!(result.is_ok(), "{:?}", result.err());

        let resolved = result.unwrap();
        assert_eq!(resolved.datasets, vec!["orders"]);
        assert!(resolved.join_path.is_empty());
    }

    #[test]
    fn test_resolve_cross_dataset_join() {
        let manifest = make_manifest_with_field_index();
        let fields = vec![
            "date".to_string(),
            "revenue".to_string(),
            "customer_name".to_string(),
        ];

        let result = resolve_from_fields(&fields, &manifest);
        assert!(result.is_ok(), "{:?}", result.err());

        let resolved = result.unwrap();
        assert_eq!(resolved.datasets.len(), 2);
        assert!(resolved.datasets.contains(&"orders".to_string()));
        assert!(resolved.datasets.contains(&"customers".to_string()));
        assert_eq!(resolved.join_path, vec![0]); // relationship index 0
    }

    #[test]
    fn test_resolve_unknown_field() {
        let manifest = make_manifest_with_field_index();
        let fields = vec!["nonexistent_field".to_string()];

        let result = resolve_from_fields(&fields, &manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_no_join_path() {
        let mut manifest = make_manifest_with_field_index();
        // Clear the relationship graph to simulate disconnected datasets.
        manifest.relationship_graph = RelationshipGraph::default();

        let fields = vec!["date".to_string(), "customer_name".to_string()];

        let result = resolve_from_fields(&fields, &manifest);
        assert!(result.is_err());
    }
}
