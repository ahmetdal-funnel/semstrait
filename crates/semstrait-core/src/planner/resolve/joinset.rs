//! Joinset resolution algorithm.
//!
//! Datasets are joined together to construct one entity. Only datasets
//! needed for the query are included (join pruning).
//!
//! Steps:
//! 1. CONSTRAINTS CHECK (done by caller)
//! 2. IDENTIFY NEEDED DATASETS — via column_mapping lookup
//! 3. ANCHOR — always included; inferred as in-degree=0 node in relationship graph
//! 4. BUILD JOIN TREE — BFS from anchor; include intermediate datasets on path
//! 5. PRUNE — exclude unreachable datasets
//! 6. Return plan info for join tree generation

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::graph::{DiGraph, NodeIndex};

use crate::diagnostics::{codes, CompileError, Diagnostic};
use crate::schema::model::{
    ColumnMappingValue, JoinsetConfig, Kind, KindDataset, KindDatasetEntry, KindRelationship,
};

use super::QueryRequest;

/// Result of joinset resolution.
#[derive(Debug)]
pub struct JoinsetPlan {
    /// The anchor dataset name.
    pub anchor: String,
    /// Ordered join edges from anchor outward.
    pub join_edges: Vec<JoinEdge>,
    /// Column mappings per dataset: dataset_name → [(logical, physical)].
    pub column_mappings: HashMap<String, Vec<(String, String)>>,
}

/// A single join edge in the join tree.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct JoinEdge {
    pub from_dataset: String,
    pub to_dataset: String,
    /// Column pairs for the join condition (supports composite keys).
    pub column_pairs: Vec<(String, String)>,
    pub join_type: crate::schema::model::JoinType,
}

/// Resolve a joinset kind for a query request.
pub fn resolve(
    kind: &Kind,
    config: &JoinsetConfig,
    request: &QueryRequest,
) -> Result<JoinsetPlan, CompileError> {
    let required: Vec<&str> = request
        .dimensions
        .iter()
        .chain(request.measures.iter())
        .map(String::as_str)
        .collect();

    if required.is_empty() {
        return Err(CompileError::single(Diagnostic::error(
            codes::PLAN_E001,
            format!("joinset '{}': query requests no columns", kind.name),
        )));
    }

    // Collect inline datasets
    let datasets: Vec<&KindDataset> = kind
        .datasets
        .iter()
        .filter_map(|e| match e {
            KindDatasetEntry::Inline(ds) => Some(ds),
            KindDatasetEntry::Ref(_) => None,
        })
        .collect();

    let relationships = kind.relationships.as_deref().unwrap_or(&[]);

    // Step 2: identify which datasets provide which columns
    let mut needed_datasets: HashSet<&str> = HashSet::new();
    let mut column_mappings: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for &col in &required {
        let mut found = false;
        for ds in &datasets {
            if let Some(physical) = resolve_column(&ds.extras.column_mapping, col) {
                needed_datasets.insert(&ds.name);
                column_mappings
                    .entry(ds.name.clone())
                    .or_default()
                    .push((col.to_string(), physical));
                found = true;
                break;
            }
        }
        if !found {
            return Err(CompileError::single(
                Diagnostic::error(
                    codes::PLAN_E004,
                    format!(
                        "joinset '{}': column '{}' not found in any dataset column_mapping",
                        kind.name, col
                    ),
                )
                .with_entity(format!("kinds.{}", kind.name), &kind.name),
            ));
        }
    }

    // Step 3: build relationship graph and find anchor (in-degree=0)
    let (graph, node_map) = build_relationship_graph(&datasets, relationships);
    let anchor = find_anchor(&graph, &node_map).ok_or_else(|| {
        CompileError::single(
            Diagnostic::error(
                codes::PLAN_E002,
                format!("joinset '{}': cannot determine anchor dataset", kind.name),
            )
            .with_entity(format!("kinds.{}", kind.name), &kind.name),
        )
    })?;

    // The anchor is always needed
    needed_datasets.insert(anchor);

    // Step 4: BFS from anchor to build join tree including intermediate nodes
    let join_edges = build_join_tree(
        anchor,
        &needed_datasets,
        relationships,
        &datasets,
        config,
    )?;

    Ok(JoinsetPlan {
        anchor: anchor.to_string(),
        join_edges,
        column_mappings,
    })
}

fn resolve_column(
    mapping: &HashMap<String, ColumnMappingValue>,
    logical: &str,
) -> Option<String> {
    match mapping.get(logical)? {
        ColumnMappingValue::Simple(p) => Some(p.clone()),
        ColumnMappingValue::Complex { column, .. } => Some(column.clone()),
    }
}

fn build_relationship_graph<'a>(
    datasets: &[&'a KindDataset],
    relationships: &[KindRelationship],
) -> (DiGraph<&'a str, ()>, HashMap<&'a str, NodeIndex>) {
    let mut graph = DiGraph::new();
    let mut node_map: HashMap<&str, NodeIndex> = HashMap::new();

    // Add all datasets as nodes
    for ds in datasets {
        let idx = graph.add_node(ds.name.as_str());
        node_map.insert(ds.name.as_str(), idx);
    }

    // Add relationships as directed edges (from → to)
    for rel in relationships {
        if let (Some(&from_idx), Some(&to_idx)) =
            (node_map.get(rel.from.as_str()), node_map.get(rel.to.as_str()))
        {
            graph.add_edge(from_idx, to_idx, ());
        }
    }

    (graph, node_map)
}

/// Find the anchor: the node with in-degree=0 in the relationship graph.
/// If multiple exist, pick the first one (by dataset order).
fn find_anchor<'a>(
    graph: &DiGraph<&'a str, ()>,
    node_map: &HashMap<&'a str, NodeIndex>,
) -> Option<&'a str> {
    // Find nodes with in-degree = 0
    let anchors: Vec<&str> = node_map
        .iter()
        .filter(|(_, &idx)| {
            graph
                .neighbors_directed(idx, petgraph::Direction::Incoming)
                .next()
                .is_none()
        })
        .map(|(&name, _)| name)
        .collect();

    anchors.into_iter().next()
}

/// BFS from anchor to build the join tree, including intermediate datasets
/// on the path to needed datasets.
fn build_join_tree(
    anchor: &str,
    needed: &HashSet<&str>,
    relationships: &[KindRelationship],
    _datasets: &[&KindDataset],
    _config: &JoinsetConfig,
) -> Result<Vec<JoinEdge>, CompileError> {
    // Build adjacency for BFS (undirected for path finding)
    let mut adj: HashMap<&str, Vec<(&str, &KindRelationship)>> = HashMap::new();
    for rel in relationships {
        adj.entry(rel.from.as_str())
            .or_default()
            .push((rel.to.as_str(), rel));
        adj.entry(rel.to.as_str())
            .or_default()
            .push((rel.from.as_str(), rel));
    }

    // BFS from anchor
    let mut visited: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    let mut parent: HashMap<&str, (&str, &KindRelationship)> = HashMap::new();

    visited.insert(anchor);
    queue.push_back(anchor);

    while let Some(current) = queue.pop_front() {
        if let Some(neighbors) = adj.get(current) {
            for &(next, rel) in neighbors {
                if !visited.contains(next) {
                    visited.insert(next);
                    parent.insert(next, (current, rel));
                    queue.push_back(next);
                }
            }
        }
    }

    // Collect edges for all needed datasets by tracing back to anchor
    let mut edge_set: Vec<JoinEdge> = Vec::new();
    let mut included_edges: HashSet<String> = HashSet::new();

    for &ds_name in needed {
        if ds_name == anchor {
            continue;
        }

        // Trace path back to anchor
        let mut current = ds_name;
        while current != anchor {
            let (prev, rel) = parent.get(current).ok_or_else(|| {
                CompileError::single(Diagnostic::error(
                    codes::PLAN_E003,
                    format!("joinset: dataset '{}' unreachable from anchor '{}'", current, anchor),
                ))
            })?;

            let edge_key = format!("{}→{}", rel.from, rel.to);
            if included_edges.insert(edge_key) {
                // Determine if we're traversing the relationship in reverse
                // (BFS went from `current` to `prev`, but rel is from→to)
                let is_reversed = rel.to.as_str() == *prev;

                let column_pairs: Vec<(String, String)> = rel
                    .columns
                    .iter()
                    .map(|c| {
                        if is_reversed {
                            (c.to.clone(), c.from.clone())
                        } else {
                            (c.from.clone(), c.to.clone())
                        }
                    })
                    .collect();

                let join_type = if is_reversed {
                    flip_join_type(rel.join_type)
                } else {
                    rel.join_type
                };

                let (from_ds, to_ds) = if is_reversed {
                    (rel.to.clone(), rel.from.clone())
                } else {
                    (rel.from.clone(), rel.to.clone())
                };

                edge_set.push(JoinEdge {
                    from_dataset: from_ds,
                    to_dataset: to_ds,
                    column_pairs,
                    join_type,
                });
            }
            current = prev;
        }
    }

    Ok(edge_set)
}

/// Flip a join type when traversing a relationship in reverse direction.
fn flip_join_type(jt: crate::schema::model::JoinType) -> crate::schema::model::JoinType {
    use crate::schema::model::JoinType;
    match jt {
        JoinType::Left => JoinType::Right,
        JoinType::Right => JoinType::Left,
        other => other, // Inner and Full are symmetric
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::schema::model::JoinAssociativity;

    fn load_joinset_kind() -> Kind {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/joinset_basic.yaml"
        ));
        let model = parser::parse_file(path).unwrap();
        model.semantic_model.kinds.unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn test_joinset_resolve_basic() {
        let kind = load_joinset_kind();
        let config = JoinsetConfig {
            associativity: JoinAssociativity::Left,
        };
        let request = QueryRequest {
            dimensions: vec!["order_date".into(), "customer_name".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &config, &request).unwrap();
        assert_eq!(plan.anchor, "orders");
        assert_eq!(plan.join_edges.len(), 1);
        assert_eq!(plan.join_edges[0].from_dataset, "orders");
        assert_eq!(plan.join_edges[0].to_dataset, "customers");
    }

    #[test]
    fn test_joinset_prune_unused() {
        let kind = load_joinset_kind();
        let config = JoinsetConfig {
            associativity: JoinAssociativity::Left,
        };
        // Only request columns from orders — customers should not be joined
        let request = QueryRequest {
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &config, &request).unwrap();
        assert_eq!(plan.anchor, "orders");
        assert!(plan.join_edges.is_empty()); // no join needed
    }

    #[test]
    fn test_joinset_missing_column_fails() {
        let kind = load_joinset_kind();
        let config = JoinsetConfig {
            associativity: JoinAssociativity::Left,
        };
        let request = QueryRequest {
            dimensions: vec!["nonexistent".into()],
            measures: vec![],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let err = resolve(&kind, &config, &request).unwrap_err();
        assert!(err.to_string().contains("PLAN_E004"));
    }

    #[test]
    fn test_joinset_column_pairs() {
        let kind = load_joinset_kind();
        let config = JoinsetConfig {
            associativity: JoinAssociativity::Left,
        };
        let request = QueryRequest {
            dimensions: vec!["order_date".into(), "customer_name".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &config, &request).unwrap();
        let edge = &plan.join_edges[0];
        assert_eq!(edge.column_pairs.len(), 1);
        assert_eq!(edge.column_pairs[0], ("customer_id".into(), "id".into()));
    }

    #[test]
    fn test_flip_join_type() {
        use crate::schema::model::JoinType;
        assert_eq!(flip_join_type(JoinType::Left), JoinType::Right);
        assert_eq!(flip_join_type(JoinType::Right), JoinType::Left);
        assert_eq!(flip_join_type(JoinType::Inner), JoinType::Inner);
        assert_eq!(flip_join_type(JoinType::Full), JoinType::Full);
    }
}
