//! Test helper functions for constructing test fixtures.
//!
//! These helpers create minimal CompiledManifest and ResolvedQueryRequest
//! instances for unit testing. All fixtures construct DataKind directly
//! (no bridge from v1 CompiledKind).

use indexmap::IndexMap;
use std::collections::HashMap;

use semstrait_manifest::{
    CompiledDimension, CompiledManifest, CompiledMeasure, DimensionType,
};
use semstrait_manifest::acceleration::{
    CoverageIndex, DataKind, DatasetBinding, DatasetKind, DimensionIndex, GrainsetKind,
    KindInterface, ResolvedColumnMapping,
};

use crate::request::ResolvedQueryRequest;

/// Build a KindInterface from dimensions, measures, and metrics.
fn build_interface(
    name: &str,
    dimensions: IndexMap<String, CompiledDimension>,
    measures: IndexMap<String, CompiledMeasure>,
) -> KindInterface {
    let temporal_dim = dimensions
        .iter()
        .find(|(_, d)| matches!(d.dim_type, DimensionType::Temporal(_)))
        .map(|(name, _)| name.clone());

    KindInterface {
        name: name.to_string(),
        description: None,
        dimensions,
        measures,
        metrics: IndexMap::new(),
        keys: None,
        filters: vec![],
        domain: None,
        temporal_dim,
    }
}

/// Build a DatasetBinding from a name and physical column mapping pairs.
fn build_binding(
    name: &str,
    physical_pairs: Vec<(&str, &str)>,
    sources: Vec<&str>,
) -> DatasetBinding {
    let mut physical = IndexMap::new();
    for (semantic, phys) in physical_pairs {
        physical.insert(semantic.to_string(), phys.to_string());
    }
    DatasetBinding {
        dataset_name: name.to_string(),
        column_mapping: ResolvedColumnMapping {
            physical,
            literals: HashMap::new(),
            temporal: HashMap::new(),
            anchored: HashMap::new(),
        },
        resolved_sources: sources
            .into_iter()
            .map(semstrait_manifest::ResolvedSource::path)
            .collect(),
    }
}

/// Create a basic test manifest with a single Dataset kind "orders".
///
/// The kind has dimensions [date, region, customer, user_id] and measure [revenue].
/// It has one dataset "orders_daily" that covers all fields.
pub fn make_test_manifest() -> CompiledManifest {
    make_test_manifest_with_constraints(None, None)
}

/// Create a test manifest with optional constraints on the "revenue" measure.
pub fn make_test_manifest_with_constraints(
    dim_constraints: Option<semstrait_manifest::DimensionConstraints>,
    agg_constraints: Option<semstrait_manifest::AggregationConstraints>,
) -> CompiledManifest {
    let constraints = if dim_constraints.is_some() || agg_constraints.is_some() {
        Some(semstrait_manifest::MeasureConstraints {
            dimensions: dim_constraints,
            aggregations: agg_constraints,
        })
    } else {
        None
    };

    let mut dimensions = IndexMap::new();
    for name in &["date", "region", "customer", "user_id"] {
        dimensions.insert(
            name.to_string(),
            CompiledDimension {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );
    }

    let mut measures = IndexMap::new();
    measures.insert(
        "revenue".to_string(),
        CompiledMeasure {
            name: "revenue".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Float64,
            agg: None,
            expr: semstrait_core::Expr::entity_ref("SUM(amount)"),
            expr_source: "SUM(amount)".to_string(),
            additivity: None,
            constraints,
            filters: vec![],
        },
    );

    let interface = build_interface("orders", dimensions, measures);

    let binding = build_binding(
        "orders_daily",
        vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("customer", "customer_name"),
            ("user_id", "user_id"),
            ("revenue", "amount"),
        ],
        vec![],
    );

    // Single dataset → DataKind::Dataset (fast path).
    let data_kind = DataKind::Dataset(Box::new(DatasetKind { interface, binding }));

    let mut data_kinds = IndexMap::new();
    data_kinds.insert("orders".to_string(), data_kind);

    CompiledManifest {
        version: 2,
        compiled_at: chrono::Utc::now(),
        source_hash: "test".to_string(),
        datasets: IndexMap::new(),
        kinds: IndexMap::new(),
        relationships: vec![],
        model_name: "test_model".to_string(),
        model_description: None,
        data_kinds,
        relationship_graph: semstrait_manifest::RelationshipGraph::default(),
        field_index: semstrait_manifest::FieldIndex::default(),
        diagnostics: semstrait_manifest::CompileDiagnostics::default(),
        catalog_snapshot: None,
    }
}

/// Create a test manifest with two partial-coverage datasets for horizontal join testing.
///
/// Dataset "cost_daily" covers dimensions [date, region] + measure [cost].
/// Dataset "revenue_daily" covers dimensions [date, region] + measure [revenue].
/// Neither alone covers both measures.
pub fn make_multi_dataset_manifest() -> CompiledManifest {
    let mut dimensions = IndexMap::new();
    for name in &["date", "region"] {
        dimensions.insert(
            name.to_string(),
            CompiledDimension {
                name: name.to_string(),
                description: None,
                data_type: semstrait_core::DataType::Utf8,
                dim_type: DimensionType::Categorical(semstrait_manifest::CategoricalDimension {
                    enum_values: None,
                }),
            },
        );
    }

    let mut measures = IndexMap::new();
    measures.insert(
        "cost".to_string(),
        CompiledMeasure {
            name: "cost".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Float64,
            agg: None,
            expr: semstrait_core::Expr::entity_ref("SUM(cost_amount)"),
            expr_source: "SUM(cost_amount)".to_string(),
            additivity: None,
            constraints: None,
            filters: vec![],
        },
    );
    measures.insert(
        "revenue".to_string(),
        CompiledMeasure {
            name: "revenue".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Float64,
            agg: None,
            expr: semstrait_core::Expr::entity_ref("SUM(rev_amount)"),
            expr_source: "SUM(rev_amount)".to_string(),
            additivity: None,
            constraints: None,
            filters: vec![],
        },
    );

    let interface = build_interface("orders", dimensions.clone(), measures.clone());

    let binding1 = build_binding(
        "cost_daily",
        vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("cost", "cost_amount"),
        ],
        vec![],
    );

    let binding2 = build_binding(
        "revenue_daily",
        vec![
            ("date", "order_date"),
            ("region", "region_name"),
            ("revenue", "rev_amount"),
        ],
        vec![],
    );

    let bindings = vec![binding1, binding2];
    let coverage_index = CoverageIndex::build(&dimensions, &measures, &bindings);
    let dimension_index = DimensionIndex::build(&dimensions, &bindings);

    let data_kind = DataKind::Grainset(Box::new(GrainsetKind {
        interface,
        bindings,
        coverage_index,
        dimension_index,
        metric_order: None,
        grain_map: None,
    }));

    let mut data_kinds = IndexMap::new();
    data_kinds.insert("orders".to_string(), data_kind);

    CompiledManifest {
        version: 1,
        compiled_at: chrono::Utc::now(),
        source_hash: "test_multi".to_string(),
        datasets: IndexMap::new(),
        kinds: IndexMap::new(),
        relationships: vec![],
        model_name: "test_multi_model".to_string(),
        model_description: None,
        data_kinds,
        relationship_graph: semstrait_manifest::RelationshipGraph::default(),
        field_index: semstrait_manifest::FieldIndex::default(),
        diagnostics: semstrait_manifest::CompileDiagnostics::default(),
        catalog_snapshot: None,
    }
}

/// Create a basic test request.
pub fn make_test_request(
    kind_name: &str,
    dimensions: Vec<&str>,
    measures: Vec<&str>,
) -> ResolvedQueryRequest {
    ResolvedQueryRequest {
        entity_name: kind_name.to_string(),
        dimensions: dimensions.into_iter().map(String::from).collect(),
        measures: measures.into_iter().map(String::from).collect(),
        filters: vec![],
        grain: None,
        limit: None,
        order_by: vec![],
        domain_hint: None,
        session_variables: HashMap::new(),
    }
}
