//! Builder API end-to-end smoke test — `32 §9.7.6`.
//!
//! Constructs a non-trivial model (Dataset + Grainset with two nested
//! Datasets + cross-public Relationship) entirely from code, runs the
//! validate pipeline, and asserts the resulting [`SemanticModel`]
//! holds the expected shape.

use semstrait_core::{DataType, Grain};
use semstrait_model::{
    AdditivityType, AggregationType, ComplexExtras, Dataset, Dimension, DimensionEntry,
    DimensionType, Grainset, JoinKeyExprPair, KeyDecl, Keys, LeafExtras, Measure, MeasureEntry,
    NestedDataset, Relationship, SemanticInterface, SemanticMapping, SemanticModel, TemporalShape,
};

#[test]
fn builds_minimal_model_with_dataset_and_grainset() {
    let order_ts = Dimension::builder("order_ts")
        .data_type(DataType::Timestamp { precision: 6 })
        .dim_type(DimensionType::temporal([
            Grain::Minute,
            Grain::Hour,
            Grain::Day,
        ]))
        .build();

    let revenue = Measure::builder("revenue")
        .data_type(DataType::Decimal {
            precision: 18,
            scale: 2,
        })
        .agg(AggregationType::Sum)
        .additivity(AdditivityType::Full)
        .build();

    let extras = LeafExtras::builder()
        .semantic_mapping(SemanticMapping::builder().column("revenue", "amount_cents").build())
        .temporal(TemporalShape::events("order_ts", Some(Grain::Minute)))
        .build();

    // Reference the root-pool entries (SR-E-3 requires every shared-pool
    // declaration to be referenced from at least one DataKind).
    let interface = SemanticInterface::builder()
        .dimensions(vec![DimensionEntry::r#ref("order_ts")])
        .measures(vec![MeasureEntry::r#ref("revenue")])
        .keys(
            Keys::builder()
                .primary(KeyDecl::builder().columns(vec!["order_id".into()]).build())
                .build(),
        )
        .build();

    let orders = Dataset::builder("orders")
        .extras(extras.clone())
        .description("Order-line fact dataset.")
        .semantic_interface(interface.clone())
        .build();

    let returns = NestedDataset::builder("returns").extras(extras.clone()).build();
    let refunds = NestedDataset::builder("refunds").extras(extras).build();

    let order_events = Grainset::builder("order_events")
        .extras(ComplexExtras::default())
        .dataset(returns)
        .dataset(refunds)
        .description("Roll-up of order-side events.")
        .semantic_interface(interface.clone())
        .build();

    let orders_to_events = Relationship::builder()
        .name("orders_to_events")
        .from("orders")
        .to("order_events")
        .keys(vec![JoinKeyExprPair::columns("order_id", "order_id")])
        .cardinality(semstrait_model::Cardinality::ManyToOne)
        .build()
        .expect("relationship build");

    let (model, _diags) = SemanticModel::builder()
        .name("analytics-v1")
        .description("Smoke-test model.")
        .dataset(orders)
        .grainset(order_events)
        .dimension(order_ts)
        .measure(revenue)
        .relationship(orders_to_events)
        .build()
        .expect("validate clean");

    assert_eq!(model.name, "analytics-v1");
    assert_eq!(model.datasets.len(), 1);
    assert_eq!(model.grainsets.len(), 1);
    assert_eq!(model.relationships.len(), 1);
    assert!(model.datasets.contains_key("orders"));
    assert!(model.grainsets.contains_key("order_events"));
}

#[test]
fn relationship_symmetric_cardinality_requires_optional_and_cross_filter() {
    // SR-E-13: OneToOne without optional/cross_filter is rejected at build.
    let r = Relationship::builder()
        .name("one_to_one_missing")
        .from("a")
        .to("b")
        .keys(vec![JoinKeyExprPair::columns("k", "k")])
        .cardinality(semstrait_model::Cardinality::OneToOne)
        .build();

    assert!(r.is_err());
}

#[test]
fn loader_inmemory_round_trip_with_minimal_yaml() {
    let yaml = r#"
semantic_model:
  name: smoke-test
  datasets:
    - name: orders
      extras:
        semantic_mapping: auto
"#;

    let mut fs = semstrait_model::InMemoryFs::new();
    fs.insert("model.yaml", yaml);

    let (model, _diags) = SemanticModel::loader()
        .with_fs(fs)
        .from_yaml_file("model.yaml")
        .build()
        .expect("loader build clean");

    assert_eq!(model.name, "smoke-test");
    assert!(model.datasets.contains_key("orders"));
}

#[test]
fn loader_no_source_returns_no_source_diagnostic() {
    let result = SemanticModel::loader().build();
    assert!(result.is_err());
    let diags = result.unwrap_err();
    assert!(matches!(
        diags[0].kind,
        semstrait_model::ModelBuildErrorKind::NoSource
    ));
}
