//! Test helper functions for constructing test fixtures.
//!
//! These helpers create minimal CompiledManifest and ResolvedQueryRequest
//! instances for unit testing.

use indexmap::IndexMap;
use std::collections::HashMap;

use semstrait_manifest::{
    CompiledDimension, CompiledKind, CompiledKindDataset, CompiledKindType, CompiledManifest,
    CompiledMeasure,
};
use semstrait_manifest::{ColumnMappingValue, DimensionType, KindDatasetExtras};

use crate::request::ResolvedQueryRequest;

/// Create a basic test manifest with a single Grainset kind "orders".
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
                data_type: "string".to_string(),
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
            data_type: "float64".to_string(),
            expr: semstrait_core::Expr::entity_ref("SUM(amount)"),
            expr_source: "SUM(amount)".to_string(),
            additivity: None,
            constraints,
            filters: vec![],
        },
    );

    // Build the column mapping for the dataset.
    let mut column_mapping = HashMap::new();
    column_mapping.insert(
        "date".to_string(),
        ColumnMappingValue::Simple("order_date".to_string()),
    );
    column_mapping.insert(
        "region".to_string(),
        ColumnMappingValue::Simple("region_name".to_string()),
    );
    column_mapping.insert(
        "customer".to_string(),
        ColumnMappingValue::Simple("customer_name".to_string()),
    );
    column_mapping.insert(
        "user_id".to_string(),
        ColumnMappingValue::Simple("user_id".to_string()),
    );
    column_mapping.insert(
        "revenue".to_string(),
        ColumnMappingValue::Simple("amount".to_string()),
    );

    let dataset = CompiledKindDataset {
        name: "orders_daily".to_string(),
        extras: KindDatasetExtras {
            column_mapping: column_mapping.into(),
            temporal: None,
            storage: None,
            catalog: None,
        },
    };

    let kind = CompiledKind {
        name: "orders".to_string(),
        description: None,
        dimensions,
        measures,
        metrics: IndexMap::new(),
        keys: None,
        kind_type: CompiledKindType::Grainset,
        datasets: vec![dataset],
        relationships: vec![],
        domain: None,
        filters: vec![],
    };

    let mut kinds = IndexMap::new();
    kinds.insert("orders".to_string(), kind);

    CompiledManifest {
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
                data_type: "string".to_string(),
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
            data_type: "float64".to_string(),
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
            data_type: "float64".to_string(),
            expr: semstrait_core::Expr::entity_ref("SUM(rev_amount)"),
            expr_source: "SUM(rev_amount)".to_string(),
            additivity: None,
            constraints: None,
            filters: vec![],
        },
    );

    // Dataset 1: covers date, region, cost
    let mut mapping1 = HashMap::new();
    mapping1.insert("date".to_string(), ColumnMappingValue::Simple("order_date".to_string()));
    mapping1.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
    mapping1.insert("cost".to_string(), ColumnMappingValue::Simple("cost_amount".to_string()));

    let ds1 = CompiledKindDataset {
        name: "cost_daily".to_string(),
        extras: KindDatasetExtras {
            column_mapping: mapping1.into(),
            temporal: None,
            storage: None,
            catalog: None,
        },
    };

    // Dataset 2: covers date, region, revenue
    let mut mapping2 = HashMap::new();
    mapping2.insert("date".to_string(), ColumnMappingValue::Simple("order_date".to_string()));
    mapping2.insert("region".to_string(), ColumnMappingValue::Simple("region_name".to_string()));
    mapping2.insert("revenue".to_string(), ColumnMappingValue::Simple("rev_amount".to_string()));

    let ds2 = CompiledKindDataset {
        name: "revenue_daily".to_string(),
        extras: KindDatasetExtras {
            column_mapping: mapping2.into(),
            temporal: None,
            storage: None,
            catalog: None,
        },
    };

    let kind = CompiledKind {
        name: "orders".to_string(),
        description: None,
        dimensions,
        measures,
        metrics: IndexMap::new(),
        keys: None,
        kind_type: CompiledKindType::Grainset,
        datasets: vec![ds1, ds2],
        relationships: vec![],
        domain: None,
        filters: vec![],
    };

    let mut kinds = IndexMap::new();
    kinds.insert("orders".to_string(), kind);

    CompiledManifest {
        version: 1,
        compiled_at: chrono::Utc::now(),
        source_hash: "test_multi".to_string(),
        datasets: IndexMap::new(),
        kinds,
        relationships: vec![],
        model_name: "test_multi_model".to_string(),
        model_description: None,
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
