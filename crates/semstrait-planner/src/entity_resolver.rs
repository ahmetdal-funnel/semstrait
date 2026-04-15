//! Entity resolution — unified API for finding covering entities from requested fields.
//!
//! When a query omits `from`, the planner uses this module to score all entities
//! in the manifest against the requested field names and find the best covering set.
//!
//! Coverage scoring:
//! - **agg_coverage** = measures + metrics
//! - **group_by_coverage** = dimensions + keys (both contribute to GROUP BY)
//! - **total_coverage** = agg_coverage + group_by_coverage

use crate::error::PlannerError;
use semstrait_manifest::{CompiledDataKind, CompiledInterface, CompiledManifest};
use std::collections::HashSet;

/// A single entity's coverage of the requested fields.
/// All four semantic categories tracked explicitly.
#[derive(Debug, Clone)]
pub struct MatchedEntity {
    pub entity_name: String,
    pub covered_dimensions: Vec<String>,
    pub covered_keys: Vec<String>,
    pub covered_measures: Vec<String>,
    pub covered_metrics: Vec<String>,
}

impl MatchedEntity {
    /// Fields that go into GROUP BY (dimensions + keys).
    pub fn group_by_fields(&self) -> Vec<&str> {
        self.covered_dimensions
            .iter()
            .chain(self.covered_keys.iter())
            .map(|s| s.as_str())
            .collect()
    }

    /// Fields that go into aggregation (measures + metrics).
    pub fn aggregate_fields(&self) -> Vec<&str> {
        self.covered_measures
            .iter()
            .chain(self.covered_metrics.iter())
            .map(|s| s.as_str())
            .collect()
    }

    /// Total coverage count.
    pub fn total_coverage(&self) -> usize {
        self.covered_dimensions.len()
            + self.covered_keys.len()
            + self.covered_measures.len()
            + self.covered_metrics.len()
    }

    /// Aggregation coverage (measures + metrics).
    pub fn agg_coverage(&self) -> usize {
        self.covered_measures.len() + self.covered_metrics.len()
    }

    /// Grouping coverage (dimensions + keys).
    pub fn group_by_coverage(&self) -> usize {
        self.covered_dimensions.len() + self.covered_keys.len()
    }

    /// All covered field names.
    fn all_covered(&self) -> HashSet<String> {
        self.covered_dimensions
            .iter()
            .chain(self.covered_keys.iter())
            .chain(self.covered_measures.iter())
            .chain(self.covered_metrics.iter())
            .cloned()
            .collect()
    }
}

/// Result of entity matching — one or more entities covering the requested fields.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Matched entities, ordered by coverage (best first). Always non-empty.
    pub entities: Vec<MatchedEntity>,
    /// Verified relationship indices connecting entities (from RelationshipGraph).
    /// Empty if single entity. For N entities, contains the union of shortest paths
    /// between all pairs — each index references manifest.relationships[i].
    pub join_path: Vec<usize>,
}

impl MatchResult {
    pub fn is_single(&self) -> bool {
        self.entities.len() == 1
    }
}

/// Reclassified fields for a single entity — all four semantic categories.
#[derive(Debug, Clone)]
pub struct ReclassifiedFields {
    pub dimensions: Vec<String>,
    pub keys: Vec<String>,
    pub measures: Vec<String>,
    pub metrics: Vec<String>,
}

/// Find the set of entities that cover the requested fields.
///
/// Scoring: entities are ranked by (agg_coverage DESC, group_by_coverage DESC, kind_rank ASC).
/// - kind_rank: 0 = Dataset (Simple), 1 = Complex (Grainset/Unionset/Joinset)
/// - Entities with zero total coverage are skipped.
/// - When aggregation fields are requested, entities with zero agg_coverage are skipped.
///
/// Single-entity fast path: if the best entity covers all requested fields, returns immediately.
/// Multi-entity path: greedy set cover with relationship verification via RelationshipGraph.
pub fn find_covering_entities(
    requested_fields: &[String],
    manifest: &CompiledManifest,
) -> Result<MatchResult, PlannerError> {
    if requested_fields.is_empty() {
        return Err(PlannerError::Internal(
            "find_covering_entities called with empty field list".to_string(),
        ));
    }

    // Step 1: Score each entity across all 4 semantic categories.
    let mut candidates: Vec<(MatchedEntity, u8)> = Vec::new();

    for (entity_name, dk) in &manifest.entities {
        let iface = dk.interface();
        let key_names: HashSet<String> = iface
            .keys
            .as_ref()
            .map(|k| k.all_column_names().into_iter().collect())
            .unwrap_or_default();

        let covered_dimensions: Vec<String> = requested_fields
            .iter()
            .filter(|f| iface.dimensions.contains_key(f.as_str()))
            .cloned()
            .collect();

        let covered_keys: Vec<String> = requested_fields
            .iter()
            .filter(|f| key_names.contains(f.as_str()) && !iface.dimensions.contains_key(f.as_str()))
            .cloned()
            .collect();

        let covered_measures: Vec<String> = requested_fields
            .iter()
            .filter(|f| iface.measures.contains_key(f.as_str()))
            .cloned()
            .collect();

        let covered_metrics: Vec<String> = requested_fields
            .iter()
            .filter(|f| iface.metrics.contains_key(f.as_str()))
            .cloned()
            .collect();

        let matched = MatchedEntity {
            entity_name: entity_name.clone(),
            covered_dimensions,
            covered_keys,
            covered_measures,
            covered_metrics,
        };

        if matched.total_coverage() == 0 {
            continue;
        }

        let kind_rank = match dk {
            CompiledDataKind::Simple(_) => 0u8,
            _ => 1u8,
        };

        candidates.push((matched, kind_rank));
    }

    // Sort by (agg_coverage DESC, group_by_coverage DESC, kind_rank ASC).
    candidates.sort_by(|a, b| {
        b.0.agg_coverage()
            .cmp(&a.0.agg_coverage())
            .then_with(|| b.0.group_by_coverage().cmp(&a.0.group_by_coverage()))
            .then_with(|| a.1.cmp(&b.1))
    });

    if candidates.is_empty() {
        return Err(PlannerError::NoCoveringDataset {
            kind: "(ad-hoc)".to_string(),
            reason: format!(
                "no entity provides any of the requested fields: [{}]",
                requested_fields.join(", ")
            ),
        });
    }

    // Step 2: Single-entity fast path.
    let best = &candidates[0].0;
    if best.total_coverage() == requested_fields.len() {
        return Ok(MatchResult {
            entities: vec![candidates.into_iter().next().unwrap().0],
            join_path: vec![],
        });
    }

    // Step 3: Greedy set cover with relationship verification.
    let rel_graph = &manifest.relationship_graph;
    let first = candidates.remove(0).0;
    let mut covered: HashSet<String> = first.all_covered();
    let mut selected: Vec<MatchedEntity> = vec![first];
    let mut remaining: HashSet<String> = requested_fields
        .iter()
        .filter(|f| !covered.contains(f.as_str()))
        .cloned()
        .collect();
    let mut all_join_indices: Vec<usize> = Vec::new();

    while !remaining.is_empty() {
        let mut best_next_idx: Option<usize> = None;
        let mut best_next_new_coverage = 0usize;
        let mut best_next_path: Vec<usize> = Vec::new();

        for (idx, (candidate, _)) in candidates.iter().enumerate() {
            // Count new fields covered by this candidate.
            let candidate_covered = candidate.all_covered();
            let new_count = candidate_covered.intersection(&remaining).count();

            if new_count == 0 || new_count <= best_next_new_coverage {
                continue;
            }

            // Verify join path to ANY already-selected entity.
            let mut found_path = false;
            for sel in &selected {
                if let Some(path) = rel_graph.shortest_path(&sel.entity_name, &candidate.entity_name) {
                    best_next_idx = Some(idx);
                    best_next_new_coverage = new_count;
                    best_next_path = path.clone();
                    found_path = true;
                    break;
                }
                if let Some(path) = rel_graph.shortest_path(&candidate.entity_name, &sel.entity_name) {
                    best_next_idx = Some(idx);
                    best_next_new_coverage = new_count;
                    best_next_path = path.clone();
                    found_path = true;
                    break;
                }
            }

            if !found_path {
                continue;
            }
        }

        match best_next_idx {
            Some(idx) => {
                let (next_entity, _) = candidates.remove(idx);
                let next_covered = next_entity.all_covered();
                remaining.retain(|f| !next_covered.contains(f));
                covered.extend(next_covered);
                selected.push(next_entity);
                all_join_indices.extend(best_next_path);
            }
            None => {
                return Err(PlannerError::NoCoveringDataset {
                    kind: "(ad-hoc)".to_string(),
                    reason: format!(
                        "remaining fields [{}] cannot be covered by any reachable entity",
                        remaining.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                });
            }
        }
    }

    all_join_indices.sort();
    all_join_indices.dedup();

    Ok(MatchResult {
        entities: selected,
        join_path: all_join_indices,
    })
}

/// Reclassify flat field names into the four semantic categories
/// using a specific entity's interface.
pub fn reclassify_fields(
    fields: &[String],
    iface: &CompiledInterface,
) -> Result<ReclassifiedFields, PlannerError> {
    let key_names: HashSet<String> = iface
        .keys
        .as_ref()
        .map(|k| k.all_column_names().into_iter().collect())
        .unwrap_or_default();

    let mut dimensions = Vec::new();
    let mut keys = Vec::new();
    let mut measures = Vec::new();
    let mut metrics = Vec::new();

    for field in fields {
        if iface.dimensions.contains_key(field) {
            dimensions.push(field.clone());
        } else if key_names.contains(field) {
            keys.push(field.clone());
        } else if iface.measures.contains_key(field) {
            measures.push(field.clone());
        } else if iface.metrics.contains_key(field) {
            metrics.push(field.clone());
        } else {
            return Err(PlannerError::Internal(format!(
                "field '{}' not found in entity interface",
                field
            )));
        }
    }

    Ok(ReclassifiedFields {
        dimensions,
        keys,
        measures,
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use std::collections::HashMap;
    use semstrait_manifest::{
        CompiledDimension, CompiledManifest, CompiledMeasure, CompiledMetric,
        DimensionType, CategoricalDimension, MetricType,
    };
    use semstrait_manifest::acceleration::{
        CompiledDataKind, CompiledSimpleKind, CompiledInterface,
        DatasetBinding, ResolvedColumnMapping, CompiledGrainsetKind,
    };
    use semstrait_model::Keys;

    fn dim(name: &str) -> CompiledDimension {
        CompiledDimension {
            name: name.to_string(),
            description: None,
            data_type: semstrait_core::DataType::String,
            dim_type: DimensionType::Categorical(CategoricalDimension { enum_values: None }),
            expr: None,
            expr_source: None,
        }
    }

    fn measure(name: &str) -> CompiledMeasure {
        CompiledMeasure {
            name: name.to_string(),
            description: None,
            data_type: semstrait_core::DataType::Number,
            agg: semstrait_core::Aggregation::Sum,
            expr: semstrait_core::Expr::entity_ref(name),
            expr_source: name.to_string(),
            additivity: None,
            constraints: None,
            filters: vec![],
        }
    }

    fn metric(name: &str) -> CompiledMetric {
        CompiledMetric {
            name: name.to_string(),
            description: None,
            data_type: semstrait_core::DataType::Number,
            metric_type: MetricType::Simple,
            agg: None,
            expr: semstrait_core::Expr::entity_ref(name),
            expr_source: name.to_string(),
            additivity: None,
            constraints: None,
            filters: vec![],
            depth: 0,
        }
    }

    fn binding(name: &str) -> DatasetBinding {
        DatasetBinding {
            dataset_name: name.to_string(),
            column_mapping: ResolvedColumnMapping {
                physical: IndexMap::new(),
                literals: HashMap::new(),
                temporal: HashMap::new(),
                anchored: HashMap::new(),
            },
            resolved_sources: vec![],
        }
    }

    fn dataset_entity(
        name: &str,
        dims: &[&str],
        measures_list: &[&str],
        metrics_list: &[&str],
        keys: Option<Keys>,
    ) -> (String, CompiledDataKind) {
        let mut dimensions = IndexMap::new();
        for d in dims {
            dimensions.insert(d.to_string(), dim(d));
        }
        let mut meas = IndexMap::new();
        for m in measures_list {
            meas.insert(m.to_string(), measure(m));
        }
        let mut met = IndexMap::new();
        for m in metrics_list {
            met.insert(m.to_string(), metric(m));
        }
        let iface = CompiledInterface {
            name: name.to_string(),
            description: None,
            dimensions,
            measures: meas,
            metrics: met,
            keys,
            filters: vec![],
            temporal_dim: None,
        };
        let dk = CompiledDataKind::Simple(Box::new(CompiledSimpleKind {
            interface: iface,
            binding: binding(&format!("{}_ds", name)),
        }));
        (name.to_string(), dk)
    }

    fn grainset_entity(
        name: &str,
        dims: &[&str],
        measures_list: &[&str],
    ) -> (String, CompiledDataKind) {
        let mut dimensions = IndexMap::new();
        for d in dims {
            dimensions.insert(d.to_string(), dim(d));
        }
        let mut meas = IndexMap::new();
        for m in measures_list {
            meas.insert(m.to_string(), measure(m));
        }
        let bindings = vec![binding(&format!("{}_ds", name))];
        let iface = CompiledInterface {
            name: name.to_string(),
            description: None,
            dimensions: dimensions.clone(),
            measures: meas.clone(),
            metrics: IndexMap::new(),
            keys: None,
            filters: vec![],
            temporal_dim: None,
        };
        let dk = CompiledDataKind::Grainset(Box::new(CompiledGrainsetKind {
            interface: iface,
            coverage_index: semstrait_manifest::acceleration::CoverageIndex::build(
                &dimensions, &meas, &bindings,
            ),
            dimension_index: semstrait_manifest::acceleration::DimensionIndex::build(
                &dimensions, &bindings,
            ),
            bindings,
            metric_order: None,
            grain_map: None,
        }));
        (name.to_string(), dk)
    }

    fn manifest_with(entities: Vec<(String, CompiledDataKind)>) -> CompiledManifest {
        let mut ent = IndexMap::new();
        for (name, dk) in entities {
            ent.insert(name, dk);
        }
        CompiledManifest {
            version: 3,
            compiled_at: chrono::Utc::now(),
            source_hash: "test".to_string(),
            entities: ent,
            relationships: vec![],
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            semantic_graph: semstrait_manifest::SemanticGraph::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            model_name: "test".to_string(),
            model_description: None,
            catalog_snapshot: None,
        }
    }

    // ── Single-entity tests ─────────────────────────────────────────────

    #[test]
    fn test_find_covering_single_entity() {
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date", "region"], &["revenue"], &[], None),
        ]);
        let fields = vec!["date".into(), "region".into(), "revenue".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(result.is_single());
        assert_eq!(result.entities[0].entity_name, "orders");
        assert_eq!(result.entities[0].total_coverage(), 3);
        assert!(result.join_path.is_empty());
    }

    #[test]
    fn test_find_covering_agg_priority() {
        // "products" has only dims, "orders" has dims + measures.
        // When measures are requested, "orders" should win.
        let manifest = manifest_with(vec![
            dataset_entity("products", &["date", "region", "category"], &[], &[], None),
            dataset_entity("orders", &["date", "region"], &["revenue"], &[], None),
        ]);
        let fields = vec!["date".into(), "region".into(), "revenue".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(result.is_single());
        assert_eq!(result.entities[0].entity_name, "orders");
    }

    #[test]
    fn test_find_covering_simple_preferred() {
        // Dataset (kind_rank=0) preferred over Grainset (kind_rank=1) at equal coverage.
        let manifest = manifest_with(vec![
            grainset_entity("orders_grain", &["date", "region"], &["revenue"]),
            dataset_entity("orders", &["date", "region"], &["revenue"], &[], None),
        ]);
        let fields = vec!["date".into(), "revenue".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(result.is_single());
        assert_eq!(result.entities[0].entity_name, "orders");
    }

    #[test]
    fn test_find_covering_keys_contribute() {
        // Keys should count toward group_by_coverage.
        let keys = Keys {
            primary: Some(vec!["order_id".to_string()]),
            unique: None,
            foreign: None,
        };
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date"], &["revenue"], &[], Some(keys)),
        ]);
        let fields = vec!["date".into(), "order_id".into(), "revenue".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(result.is_single());
        let matched = &result.entities[0];
        assert_eq!(matched.covered_dimensions, vec!["date"]);
        assert_eq!(matched.covered_keys, vec!["order_id"]);
        assert_eq!(matched.covered_measures, vec!["revenue"]);
        assert_eq!(matched.group_by_coverage(), 2); // dim + key
        assert_eq!(matched.total_coverage(), 3);
    }

    #[test]
    fn test_find_covering_no_agg_skipped() {
        // Entity with only dims skipped when measures are requested.
        let manifest = manifest_with(vec![
            dataset_entity("dim_only", &["date", "region"], &[], &[], None),
        ]);
        let fields = vec!["date".into(), "revenue".into()];
        let result = find_covering_entities(&fields, &manifest);

        assert!(result.is_err());
    }

    #[test]
    fn test_find_covering_dim_only_query() {
        // Pure dimension query (no measures) should still find best coverage.
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date", "region"], &["revenue"], &[], None),
            dataset_entity("products", &["date", "category", "sku"], &[], &[], None),
        ]);
        // Only dimensions — no agg fields requested.
        let fields = vec!["date".into(), "category".into(), "sku".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(result.is_single());
        assert_eq!(result.entities[0].entity_name, "products");
    }

    #[test]
    fn test_find_covering_metrics() {
        // Metrics should be counted in agg_coverage.
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date"], &["revenue"], &["profit_margin"], None),
        ]);
        let fields = vec!["date".into(), "profit_margin".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(result.is_single());
        let matched = &result.entities[0];
        assert_eq!(matched.covered_metrics, vec!["profit_margin"]);
        assert_eq!(matched.agg_coverage(), 1);
    }

    #[test]
    fn test_find_covering_empty_fields_error() {
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date"], &["revenue"], &[], None),
        ]);
        let result = find_covering_entities(&[], &manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_covering_no_entity_covers() {
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date"], &["revenue"], &[], None),
        ]);
        let fields = vec!["nonexistent".into()];
        let result = find_covering_entities(&fields, &manifest);
        assert!(result.is_err());
    }

    // ── Multi-entity tests ──────────────────────────────────────────────

    #[test]
    fn test_find_covering_multi_entity() {
        let mut manifest = manifest_with(vec![
            dataset_entity("orders", &["date", "region"], &["revenue"], &[], None),
            dataset_entity("customers", &["customer_name"], &[], &[], None),
        ]);
        // Set up relationship graph for join path.
        manifest.relationship_graph.set_shortest_path("orders", "customers", vec![0]);

        let fields = vec!["date".into(), "revenue".into(), "customer_name".into()];
        let result = find_covering_entities(&fields, &manifest).unwrap();

        assert!(!result.is_single());
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.join_path, vec![0]);
    }

    #[test]
    fn test_find_covering_no_join_path_error() {
        // Two entities needed but no relationship between them → error.
        let manifest = manifest_with(vec![
            dataset_entity("orders", &["date"], &["revenue"], &[], None),
            dataset_entity("customers", &["customer_name"], &[], &[], None),
        ]);
        let fields = vec!["date".into(), "revenue".into(), "customer_name".into()];
        let result = find_covering_entities(&fields, &manifest);

        assert!(result.is_err());
    }

    // ── Reclassify tests ────────────────────────────────────────────────

    #[test]
    fn test_reclassify_fields() {
        let keys = Keys {
            primary: Some(vec!["order_id".to_string()]),
            unique: None,
            foreign: None,
        };
        let mut dimensions = IndexMap::new();
        dimensions.insert("date".to_string(), dim("date"));
        let mut measures = IndexMap::new();
        measures.insert("revenue".to_string(), measure("revenue"));
        let mut metrics = IndexMap::new();
        metrics.insert("margin".to_string(), metric("margin"));

        let iface = CompiledInterface {
            name: "test".to_string(),
            description: None,
            dimensions,
            measures,
            metrics,
            keys: Some(keys),
            filters: vec![],
            temporal_dim: None,
        };

        let fields = vec![
            "date".into(), "order_id".into(), "revenue".into(), "margin".into(),
        ];
        let result = reclassify_fields(&fields, &iface).unwrap();

        assert_eq!(result.dimensions, vec!["date"]);
        assert_eq!(result.keys, vec!["order_id"]);
        assert_eq!(result.measures, vec!["revenue"]);
        assert_eq!(result.metrics, vec!["margin"]);
    }

    #[test]
    fn test_reclassify_unknown_field_error() {
        let iface = CompiledInterface {
            name: "test".to_string(),
            description: None,
            dimensions: IndexMap::new(),
            measures: IndexMap::new(),
            metrics: IndexMap::new(),
            keys: None,
            filters: vec![],
            temporal_dim: None,
        };

        let fields = vec!["nonexistent".into()];
        let result = reclassify_fields(&fields, &iface);
        assert!(result.is_err());
    }
}
