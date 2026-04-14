//! Integration tests using actual test_data files.

use semstrait_model::{parse, resolve_refs, DataKind, ComplexDataKind};
use std::fs;
use std::path::Path;

fn load_and_parse(filename: &str) -> Result<semstrait_model::SemanticModel, semstrait_model::ModelError> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test_data")
        .join(filename);

    let yaml = fs::read_to_string(path).expect("Failed to read test file");
    let model = parse(&yaml)?;
    resolve_refs(model)
}

// =============================================================================
// Comprehensive model parsing
// =============================================================================

#[test]
fn test_parse_comprehensive_ecommerce() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();
    assert_eq!(model.name, "ecommerce_analytics");

    // Should have 14 entities (9 datasets + 5 complex), 2+ relationships
    let dataset_count = model.entities.values().filter(|dk| dk.is_simple()).count();
    let complex_count = model.entities.values().filter(|dk| dk.is_complex()).count();
    assert!(dataset_count >= 9, "expected ≥9 datasets, got {}", dataset_count);
    assert!(complex_count >= 5, "expected ≥5 complex entities, got {}", complex_count);
    assert!(!model.relationships.is_empty(), "expected relationships");
}

// =============================================================================
// Grainset parsing — grain-aware routing with WithGrain column mapping
// =============================================================================

#[test]
fn test_parse_grainset_with_grain() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    // Find the "sales" grainset
    let sales = match model.entities.get("sales").expect("sales entity") {
        DataKind::Complex(ComplexDataKind::Grainset(g)) => g,
        other => panic!("expected Grainset, got {:?}", other.variant()),
    };

    // Check children
    assert_eq!(sales.children.len(), 2, "sales should have 2 children");

    // Second child (orders_monthly) should have WithGrain mapping
    if let semstrait_model::ChildEntry::Inline(ds) = &sales.children[1] {
        match ds.extras.column_mapping.get("order_date").unwrap() {
            semstrait_model::ColumnMappingValue::WithGrain { column, grain } => {
                assert_eq!(column, "month_start");
                assert_eq!(*grain, Some(semstrait_model::TemporalGrain::Month));
            }
            other => panic!("expected WithGrain mapping for order_date, got {:?}", other),
        }
    } else {
        panic!("expected inline dataset");
    }
}

// =============================================================================
// Unionset parsing — mode, literal dimensions
// =============================================================================

#[test]
fn test_parse_unionset() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    let traffic = match model.entities.get("all_traffic").expect("all_traffic entity") {
        DataKind::Complex(ComplexDataKind::Unionset(u)) => u,
        other => panic!("expected Unionset, got {:?}", other.variant()),
    };

    assert_eq!(traffic.mode, semstrait_model::UnionMode::All);

    // 3 children: web_clicks, mobile_clicks, email_events
    assert_eq!(traffic.children.len(), 3);

    // Check literal column mapping in first child (platform = 'web')
    if let semstrait_model::ChildEntry::Inline(ds) = &traffic.children[0] {
        match ds.extras.column_mapping.get("platform").unwrap() {
            semstrait_model::ColumnMappingValue::Literal(semstrait_model::LiteralValue::String(s)) => {
                assert_eq!(s, "web");
            }
            other => panic!("expected Literal('web'), got {:?}", other),
        }
    }
}

// =============================================================================
// Joinset parsing — associativity + relationships
// =============================================================================

#[test]
fn test_parse_joinset() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    let od = match model.entities.get("order_details").expect("order_details entity") {
        DataKind::Complex(ComplexDataKind::Joinset(j)) => j,
        other => panic!("expected Joinset, got {:?}", other.variant()),
    };

    assert_eq!(od.associativity, semstrait_model::JoinAssociativity::Left);

    // Should have relationships
    assert!(od.relationships.len() >= 2, "order_details needs ≥2 relationships");

    // 3 children: orders_daily, customers, products
    assert_eq!(od.children.len(), 3);
}

// =============================================================================
// Semi-additive measure parsing
// =============================================================================

#[test]
fn test_parse_semi_additive() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    let inv = model.entities.get("inventory").expect("inventory entity");

    let mut found_semi = false;
    for measure_entry in inv.interface().measures.values() {
        if let semstrait_model::MeasureEntry::Inline(m) = measure_entry {
            if let Some(semstrait_model::AdditivityType::Semi(semi)) = &m.additivity {
                found_semi = true;
                assert!(!semi.non_additive_dimensions.is_empty());
                assert_eq!(
                    semi.resolution_strategy,
                    semstrait_model::ResolutionStrategy::Latest
                );
            }
        }
    }
    assert!(found_semi, "Expected to find semi-additive measure in inventory");
}

// =============================================================================
// Bucketed dimension parsing
// =============================================================================

#[test]
fn test_parse_bucketed_dimension() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    // price_bucket in orders_daily dataset
    let orders = model.entities.get("orders_daily").expect("orders_daily");
    let mut found_bucketed = false;
    for dim_entry in orders.interface().dimensions.values() {
        if let semstrait_model::DimensionEntry::Inline(d) = dim_entry {
            if matches!(&d.dim_type, semstrait_model::DimensionType::Bucketed(_)) {
                found_bucketed = true;
                assert_eq!(d.name, "price_bucket");
            }
        }
    }
    assert!(found_bucketed, "Expected to find bucketed dimension");
}

// =============================================================================
// Metrics parsing
// =============================================================================

#[test]
fn test_parse_metrics() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    let sales = model.entities.get("sales").expect("sales entity");
    assert!(sales.interface().metrics.len() >= 3, "sales should have ≥3 metrics (avg_order_value, profit, roi)");
}

// =============================================================================
// Measure filter parsing
// =============================================================================

#[test]
fn test_parse_measure_filter() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    // revenue (top-level shared) has a filter: exclude_cancelled
    let mut found_filtered = false;
    for m in &model.measures {
        if m.name == "revenue" && !m.filters.is_empty() {
            found_filtered = true;
            assert_eq!(m.filters[0].name, "exclude_cancelled");
        }
    }
    assert!(found_filtered, "Expected revenue measure with filter");
}

// =============================================================================
// Measure constraints parsing
// =============================================================================

#[test]
fn test_parse_measure_constraints() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    // revenue has constraints: dimensions.one_of + aggregations.prohibited
    let mut found_constraints = false;
    for m in &model.measures {
        if m.name == "revenue" {
            if let Some(ref constraints) = m.constraints {
                found_constraints = true;
                let dim_c = constraints.dimensions.as_ref().unwrap();
                assert!(dim_c.one_of.as_ref().unwrap().contains(&"order_date".to_string()));
                let agg_c = constraints.aggregations.as_ref().unwrap();
                assert!(agg_c.prohibited.as_ref().unwrap().contains(&"AVG".to_string()));
            }
        }
    }
    assert!(found_constraints, "Expected revenue with constraints");
}

// =============================================================================
// Metadata dimension parsing
// =============================================================================

#[test]
fn test_parse_metadata_dimension() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    // web_clicks has source_path metadata dimension with path extraction
    let web = model.entities.get("web_clicks").expect("web_clicks");
    let mut found_metadata = false;
    for dim_entry in web.interface().dimensions.values() {
        if let semstrait_model::DimensionEntry::Inline(d) = dim_entry {
            if let semstrait_model::DimensionType::Metadata(meta) = &d.dim_type {
                found_metadata = true;
                assert_eq!(d.name, "source_path");
                assert!(meta.path.is_some(), "expected path extraction");
                    assert_eq!(meta.path.as_ref().unwrap().token, 3);
            }
        }
    }
    assert!(found_metadata, "Expected metadata dimension in web_clicks");
}

// =============================================================================
// Top-level ref resolution
// =============================================================================

#[test]
fn test_ref_resolution() {
    let model = load_and_parse("comprehensive_ecommerce.yaml").unwrap();

    // After resolve_refs, the sales entity should have order_date and region as inline dims
    let sales = model.entities.get("sales").expect("sales entity");
    let dim_names: Vec<String> = sales.interface().dimensions.values().map(|d| match d {
        semstrait_model::DimensionEntry::Inline(dim) => dim.name.clone(),
        semstrait_model::DimensionEntry::Ref(r) => r.ref_name.clone(),
    }).collect();

    assert!(dim_names.contains(&"order_date".to_string()), "should have order_date from ref");
    assert!(dim_names.contains(&"region".to_string()), "should have region from ref");
}

// =============================================================================
// E2E full coverage model parsing
// =============================================================================

#[test]
fn test_parse_e2e_full_coverage() {
    let model = load_and_parse("e2e_full_coverage.yaml").unwrap();
    assert_eq!(model.name, "e2e_full_coverage");

    // Should have all entity types represented.
    let entity_names: Vec<&str> = model.entities.keys().map(|k| k.as_str()).collect();

    // Standalone datasets
    assert!(entity_names.contains(&"transactions"), "missing transactions dataset");
    assert!(entity_names.contains(&"accounts"), "missing accounts dataset");
    assert!(entity_names.contains(&"sensor_readings"), "missing sensor_readings dataset");
    assert!(entity_names.contains(&"products"), "missing products dataset");
    assert!(entity_names.contains(&"regions"), "missing regions dataset");

    // Grainsets
    assert!(entity_names.contains(&"txn_by_grain"), "missing txn_by_grain grainset");
    assert!(entity_names.contains(&"sensor_analytics"), "missing sensor_analytics grainset");

    // Unionsets
    assert!(entity_names.contains(&"all_transactions"), "missing all_transactions unionset");
    assert!(entity_names.contains(&"unique_events"), "missing unique_events unionset");
    assert!(entity_names.contains(&"unified_analytics"), "missing unified_analytics unionset");

    // Joinsets
    assert!(entity_names.contains(&"txn_details"), "missing txn_details joinset");
    assert!(entity_names.contains(&"product_inventory"), "missing product_inventory joinset");
    assert!(entity_names.contains(&"account_sensor_full"), "missing account_sensor_full joinset");

    // Verify top-level relationships exist
    assert!(!model.relationships.is_empty(), "should have top-level relationships");

    // Verify transactions dataset has all dimension types
    let txn = model.entities.get("transactions").expect("transactions");
    let txn_iface = txn.interface();
    let dim_count = txn_iface.dimensions.len();
    assert!(dim_count >= 12, "transactions should have 12+ dimensions (all types), got {}", dim_count);
}

#[test]
fn test_e2e_full_coverage_data_types() {
    let model = load_and_parse("e2e_full_coverage.yaml").unwrap();
    let txn = model.entities.get("transactions").expect("transactions");
    let txn_iface = txn.interface();

    // Verify diverse data types across measures
    let measure_types: Vec<String> = txn_iface.measures.values().filter_map(|m| {
        match m {
            semstrait_model::MeasureEntry::Inline(m) => m.data_type.as_ref().map(|dt| format!("{}", dt)),
            _ => None,
        }
    }).collect();

    assert!(measure_types.contains(&"i8".to_string()), "missing i8 data type");
    assert!(measure_types.contains(&"i16".to_string()), "missing i16 data type");
    assert!(measure_types.contains(&"f32".to_string()), "missing f32 data type");
    assert!(measure_types.contains(&"f64".to_string()), "missing f64 data type");
    assert!(measure_types.iter().any(|t| t.starts_with("decimal")), "missing decimal data type");
}

// =============================================================================
// Dimension type default (inline test — not from file)
// =============================================================================

#[test]
fn test_dimension_type_defaults_to_categorical() {
    // When `type:` is omitted from a dimension YAML, it should default to Categorical.
    let yaml = r#"
semantic_model:
  name: default_dim_type_test
  datasets:
    - name: orders
      dimensions:
        - name: region
          data_type: string
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;

    let model = semstrait_model::parse(yaml).unwrap();
    let model = semstrait_model::resolve_refs(model).unwrap();

    let orders = model.entities.get("orders").expect("orders dataset");
    let dim = match orders.interface().dimensions.values().next().unwrap() {
        semstrait_model::DimensionEntry::Inline(d) => d,
        _ => panic!("expected inline dimension"),
    };

    assert_eq!(dim.name, "region");
    assert!(
        matches!(&dim.dim_type, semstrait_model::DimensionType::Categorical(c) if c.enum_values.is_none()),
        "expected Categorical with no enum_values, got {:?}",
        dim.dim_type
    );
}
