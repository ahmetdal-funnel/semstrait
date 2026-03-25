//! Integration tests using actual test_data files.

use semstrait_model::{parse, resolve_refs};
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

#[test]
fn test_parse_minimal_yaml() {
    let model = load_and_parse("minimal.yaml").unwrap();
    assert_eq!(model.name, "minimal_test");
    assert_eq!(model.datasets.len(), 1);
    assert_eq!(model.datasets[0].name, "orders");

    // Check dimensions
    assert_eq!(model.datasets[0].dimensions.len(), 1);
    match &model.datasets[0].dimensions[0] {
        semstrait_model::DimensionEntry::Inline(d) => {
            assert_eq!(d.name, "order_date");
        }
        _ => panic!("expected inline dimension"),
    }

    // Check measures
    assert_eq!(model.datasets[0].measures.len(), 2);

    // Check metrics
    assert_eq!(model.datasets[0].metrics.len(), 1);
}

#[test]
fn test_parse_grainset_basic_yaml() {
    let model = load_and_parse("grainset_basic.yaml").unwrap();
    assert_eq!(model.name, "grainset_basic");
    assert_eq!(model.kinds.len(), 1);

    let kind = &model.kinds[0];
    assert_eq!(kind.name, "sales");

    // Check kind type
    match &kind.kind_type {
        semstrait_model::KindTypeSpec::Grainset => {},
        _ => panic!("expected grainset"),
    }

    // Check datasets
    assert_eq!(kind.datasets.len(), 2);

    // First dataset
    match &kind.datasets[0] {
        semstrait_model::KindDatasetEntry::Inline(ds) => {
            match &ds.name {
                semstrait_model::DatasetName::Literal(name) => {
                    assert_eq!(name, "orders_daily");
                }
                _ => panic!("expected literal dataset name"),
            }

            // Check column mapping
            assert!(ds.extras.column_mapping.contains_key("order_date"));
            assert!(ds.extras.column_mapping.contains_key("revenue"));
        }
        _ => panic!("expected inline dataset"),
    }

    // Second dataset with grain override
    match &kind.datasets[1] {
        semstrait_model::KindDatasetEntry::Inline(ds) => {
            match &ds.name {
                semstrait_model::DatasetName::Literal(name) => {
                    assert_eq!(name, "orders_monthly");
                }
                _ => panic!("expected literal dataset name"),
            }

            // Check column mapping with grain
            match ds.extras.column_mapping.get("order_date").unwrap() {
                semstrait_model::ColumnMappingValue::WithGrain { column, grain } => {
                    assert_eq!(column, "order_month");
                    assert_eq!(*grain, Some(semstrait_model::TemporalGrain::Month));
                }
                _ => panic!("expected with_grain mapping"),
            }
        }
        _ => panic!("expected inline dataset"),
    }
}

#[test]
fn test_parse_unionset_basic_yaml() {
    let model = load_and_parse("unionset_basic.yaml").unwrap();
    assert_eq!(model.name, "unionset_basic");
    assert_eq!(model.kinds.len(), 1);

    let kind = &model.kinds[0];
    assert_eq!(kind.name, "all_events");

    match &kind.kind_type {
        semstrait_model::KindTypeSpec::Unionset(_) => {},
        _ => panic!("expected unionset"),
    }
}

#[test]
fn test_parse_joinset_basic_yaml() {
    let model = load_and_parse("joinset_basic.yaml").unwrap();
    assert_eq!(model.name, "joinset_basic");
    assert_eq!(model.kinds.len(), 1);

    let kind = &model.kinds[0];
    assert_eq!(kind.name, "order_details");

    match &kind.kind_type {
        semstrait_model::KindTypeSpec::Joinset(config) => {
            assert_eq!(config.associativity, semstrait_model::JoinAssociativity::Left);
        }
        _ => panic!("expected joinset"),
    }

    // Check relationships
    assert!(!kind.relationships.is_empty());
}

#[test]
fn test_parse_full_model_yaml() {
    let model = load_and_parse("full_model.yaml").unwrap();
    assert_eq!(model.name, "full_model");

    // Should have datasets, kinds, and relationships
    assert!(!model.datasets.is_empty() || !model.kinds.is_empty());
}

#[test]
fn test_parse_grainset_semi_additive_yaml() {
    let model = load_and_parse("grainset_semi_additive.yaml").unwrap();

    // Find a measure with semi-additive additivity
    let mut found_semi = false;

    for kind in &model.kinds {
        for measure_entry in &kind.measures {
            if let semstrait_model::MeasureEntry::Inline(m) = measure_entry {
                if let Some(additivity) = &m.additivity {
                    if matches!(additivity, semstrait_model::AdditivityType::Semi(_)) {
                        found_semi = true;
                    }
                }
            }
        }
    }

    assert!(found_semi, "Expected to find semi-additive measure");
}

#[test]
fn test_parse_grainset_bucketed_yaml() {
    let model = load_and_parse("grainset_bucketed.yaml").unwrap();

    // Find a bucketed dimension
    let mut found_bucketed = false;

    for kind in &model.kinds {
        for dim_entry in &kind.dimensions {
            if let semstrait_model::DimensionEntry::Inline(d) = dim_entry {
                if matches!(&d.dim_type, semstrait_model::DimensionType::Bucketed(_)) {
                    found_bucketed = true;
                }
            }
        }
    }

    assert!(found_bucketed, "Expected to find bucketed dimension");
}

#[test]
fn test_parse_grainset_metrics_yaml() {
    let model = load_and_parse("grainset_metrics.yaml").unwrap();

    // Check that metrics are present
    let mut found_metrics = false;

    for kind in &model.kinds {
        if !kind.metrics.is_empty() {
            found_metrics = true;
            break;
        }
    }

    assert!(found_metrics, "Expected to find metrics in model");
}

#[test]
fn test_parse_grainset_measure_filter_yaml() {
    let model = load_and_parse("grainset_measure_filter.yaml").unwrap();

    // Find a measure with filters
    let mut found_filtered = false;

    for kind in &model.kinds {
        for measure_entry in &kind.measures {
            if let semstrait_model::MeasureEntry::Inline(m) = measure_entry {
                if !m.filters.is_empty() {
                    found_filtered = true;
                }
            }
        }
    }

    assert!(found_filtered, "Expected to find measure with filters");
}

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

    let dim = match &model.datasets[0].dimensions[0] {
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
