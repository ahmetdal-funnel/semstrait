//! Builder API facade tests — `32 §9.7.8`.
//!
//! Per-track tests for the ergonomic facade layer that delegates to the
//! primary structural surface.

use semstrait_core::{DataType, Grain};
use semstrait_model::expr_ast::ExprSource;
use semstrait_model::*;

// ── Track A: entity facade ───────────────────────────────────────

#[test]
fn dimension_facade_temporal() {
    let d = Dimension::builder("ts")
        .data_type(DataType::Timestamp { precision: 6 })
        .temporal([Grain::Day, Grain::Hour])
        .build();
    assert!(matches!(d.dim_type, DimensionType::Temporal(_)));
}

#[test]
fn dimension_facade_categorical() {
    let d = Dimension::builder("country")
        .data_type(DataType::String)
        .categorical()
        .build();
    assert!(matches!(d.dim_type, DimensionType::Categorical));
}

#[test]
fn measure_facade_sum_full() {
    let m = Measure::builder("revenue")
        .data_type(DataType::Decimal {
            precision: 18,
            scale: 2,
        })
        .sum()
        .full()
        .build();
    assert!(matches!(m.agg, AggregationType::Sum));
    assert!(matches!(m.additivity, Some(AdditivityType::Full)));
}

#[test]
fn measure_facade_filter_push() {
    let f = AggregationFilter::builder("non_null")
        .expr(ExprSource::Inline("amount IS NOT NULL".into()))
        .build();
    let m = Measure::builder("revenue")
        .data_type(DataType::Decimal {
            precision: 18,
            scale: 2,
        })
        .count()
        .filter(f)
        .build();
    assert_eq!(m.filters.len(), 1);
}

#[test]
fn metric_facade_sum_some_wrapper() {
    let m = Metric::builder("revenue_ytd")
        .data_type(DataType::Decimal {
            precision: 18,
            scale: 2,
        })
        .expr(ExprSource::Inline("SUM(revenue)".into()))
        .sum()
        .build();
    assert_eq!(m.agg, Some(AggregationType::Sum));
}

#[test]
fn relationship_facade_field_and_many_to_one() {
    let r = Relationship::builder()
        .name("o_to_c")
        .from("orders")
        .to("customers")
        .field("customer_id", "id")
        .many_to_one()
        .build()
        .expect("build clean");
    assert_eq!(r.keys.len(), 1);
    assert!(matches!(r.cardinality, Cardinality::ManyToOne));
}

// ── Track D: container facade ────────────────────────────────────

#[test]
fn semantic_interface_per_item_inserters() {
    let iface = SemanticInterface::builder()
        .dimension(DimensionEntry::r#ref("dim_a"))
        .measure(MeasureEntry::r#ref("m_a"))
        .metric(MetricEntry::r#ref("met_a"))
        .build();
    assert_eq!(iface.dimensions.len(), 1);
    assert_eq!(iface.measures.len(), 1);
    assert_eq!(iface.metrics.len(), 1);
}

#[test]
fn semantic_interface_per_key_shortcuts_rmw() {
    let iface = SemanticInterface::builder()
        .primary_key(KeyDecl::builder().fields(vec!["id".into()]).build())
        .unique_key(KeyDecl::builder().fields(vec!["sku".into()]).build())
        .build();
    let keys = iface.keys.expect("keys present");
    assert!(keys.primary.is_some());
    assert_eq!(keys.unique.len(), 1);
}

#[test]
fn semantic_mapping_entries_bulk_insert() {
    let m = SemanticMapping::builder()
        .entries([
            ("revenue", SemanticMappingValue::Column("amount".into())),
            ("currency", SemanticMappingValue::Literal(LiteralValue::String("USD".into()))),
        ])
        .build();
    let SemanticMapping::Explicit(e) = m else { panic!("expected Explicit") };
    assert_eq!(e.len(), 2);
}

// ── Track B: leaf DataKind facade ────────────────────────────────

#[test]
fn dataset_facade_path_and_format_rmw() {
    let d = Dataset::builder("orders")
        .catalog("polaris")
        .format(StorageFormat::Parquet)
        .path("s3://bucket/orders/")
        .build();
    let storage = d.body.base.extras.storage.as_ref().expect("storage set");
    assert_eq!(storage.format, Some(StorageFormat::Parquet));
    assert_eq!(storage.paths.len(), 1);
    assert_eq!(
        d.body.base.extras.catalog.as_ref().expect("catalog set").alias,
        "polaris",
    );
}

#[test]
fn dataset_facade_extras_then_path_rmw_preserves_other_fields() {
    let pre = LeafExtras::builder()
        .catalog(CatalogRef::new("polaris"))
        .build();
    let d = Dataset::builder("orders")
        .extras(pre)
        .path("s3://bucket/orders/")
        .build();
    assert_eq!(
        d.body.base.extras.catalog.as_ref().expect("catalog preserved").alias,
        "polaris",
    );
    assert_eq!(
        d.body.base.extras.storage.as_ref().expect("storage set").paths.len(),
        1,
    );
}

#[test]
fn dataset_facade_semantic_interface_then_dimension_rmw_preserves_other_fields() {
    let pre = SemanticInterface::builder()
        .measure(MeasureEntry::r#ref("revenue"))
        .build();
    let d = Dataset::builder("orders")
        .semantic_interface(pre)
        .dimension(DimensionEntry::r#ref("order_ts"))
        .build();
    assert_eq!(d.semantic_interface.measures.len(), 1, "pre.measures preserved");
    assert_eq!(d.semantic_interface.dimensions.len(), 1, "facade dimension added");
}

#[test]
fn dataset_facade_dimension_and_primary_key() {
    let d = Dataset::builder("orders")
        .dimension(DimensionEntry::r#ref("order_ts"))
        .measure(MeasureEntry::r#ref("revenue"))
        .primary_key(KeyDecl::builder().fields(vec!["order_id".into()]).build())
        .build();
    assert_eq!(d.semantic_interface.dimensions.len(), 1);
    assert_eq!(d.semantic_interface.measures.len(), 1);
    assert!(
        d.semantic_interface
            .keys
            .as_ref()
            .expect("keys set")
            .primary
            .is_some(),
    );
}

#[test]
fn nested_dataset_facade_format_and_paths() {
    let n = NestedDataset::builder("returns")
        .format(StorageFormat::Parquet)
        .paths(["s3://bucket/returns/2026/", "s3://bucket/returns/2025/"])
        .build();
    let storage = n.body.base.extras.storage.as_ref().expect("storage set");
    assert_eq!(storage.format, Some(StorageFormat::Parquet));
    assert_eq!(storage.paths.len(), 2);
}

// ── Track C: complex DataKind facade ────────────────────────────────

#[test]
fn grainset_facade_temporal_and_dimension() {
    use semstrait_core::Grain;
    let nested_a = NestedDataset::builder("a").build();
    let nested_b = NestedDataset::builder("b").build();
    let g = Grainset::builder("evt")
        .dataset(nested_a)
        .dataset(nested_b)
        .temporal(TemporalShape::events("ts", Some(Grain::Day)))
        .dimension(DimensionEntry::r#ref("ts"))
        .build();
    assert!(g.body.base.extras.temporal.is_some());
    assert_eq!(g.semantic_interface.dimensions.len(), 1);
    assert_eq!(g.body.datasets.len(), 2);
}

#[test]
fn unionset_facade_union_all_and_union_unique() {
    let nested_a = NestedDataset::builder("a").build();
    let nested_b = NestedDataset::builder("b").build();
    let u_all = Unionset::builder("u_all")
        .dataset(nested_a.clone())
        .dataset(nested_b.clone())
        .union_all()
        .build();
    assert_eq!(u_all.body.mode, UnionMode::All);
    let u_uniq = Unionset::builder("u_uniq")
        .dataset(nested_a)
        .dataset(nested_b)
        .union_unique()
        .build();
    assert_eq!(u_uniq.body.mode, UnionMode::Unique);
}

#[test]
fn joinset_facade_dimension_and_temporal() {
    use semstrait_core::Grain;
    let nested_a = NestedDataset::builder("a").build();
    let nested_b = NestedDataset::builder("b").build();
    let j = Joinset::builder("j")
        .dataset(nested_a)
        .dataset(nested_b)
        .temporal(TemporalShape::events("ts", Some(Grain::Day)))
        .dimension(DimensionEntry::r#ref("ts"))
        .build();
    assert!(j.body.base.extras.temporal.is_some());
    assert_eq!(j.semantic_interface.dimensions.len(), 1);
}

#[test]
fn nested_grainset_facade_temporal_only() {
    use semstrait_core::Grain;
    let inner_a = NestedDataset::builder("a").build();
    let inner_b = NestedDataset::builder("b").build();
    let n = NestedGrainset::builder("ng")
        .dataset(inner_a)
        .dataset(inner_b)
        .temporal(TemporalShape::events("ts", Some(Grain::Hour)))
        .build();
    assert!(n.body.base.extras.temporal.is_some());
    assert_eq!(n.body.datasets.len(), 2);
}

#[test]
fn nested_unionset_facade_union_all() {
    let inner_a = NestedDataset::builder("a").build();
    let inner_b = NestedDataset::builder("b").build();
    let n = NestedUnionset::builder("nu")
        .dataset(inner_a)
        .dataset(inner_b)
        .union_all()
        .build();
    assert_eq!(n.body.mode, UnionMode::All);
}

// ── Track E: primary-surface symmetry ────────────────────────────

#[test]
fn nested_joinset_relationships_plural_inserter() {
    let r1 = Relationship::builder()
        .name("r1")
        .from("a")
        .to("b")
        .field("k", "k")
        .many_to_one()
        .build()
        .expect("build clean");
    let r2 = Relationship::builder()
        .name("r2")
        .from("c")
        .to("d")
        .field("k", "k")
        .many_to_one()
        .build()
        .expect("build clean");
    let inner_a = NestedDataset::builder("a").build();
    let inner_b = NestedDataset::builder("b").build();
    let nested = NestedJoinset::builder("j")
        .datasets([inner_a, inner_b])
        .relationships([r1, r2])
        .build();
    assert_eq!(nested.body.datasets.len(), 2);
    assert_eq!(nested.body.relationships.len(), 2);
}

#[test]
fn semantic_model_plural_inserters() {
    use semstrait_core::DataType;
    let d1 = Dimension::builder("d1")
        .data_type(DataType::String)
        .categorical()
        .build();
    let d2 = Dimension::builder("d2")
        .data_type(DataType::String)
        .categorical()
        .build();
    let m1 = Measure::builder("m1")
        .data_type(DataType::Integer)
        .count()
        .build();
    let m2 = Measure::builder("m2")
        .data_type(DataType::Integer)
        .count()
        .build();
    let ds1 = Dataset::builder("ds1")
        .dimension(DimensionEntry::r#ref("d1"))
        .measure(MeasureEntry::r#ref("m1"))
        .build();
    let ds2 = Dataset::builder("ds2")
        .dimension(DimensionEntry::r#ref("d2"))
        .measure(MeasureEntry::r#ref("m2"))
        .build();
    let (model, _) = SemanticModel::builder()
        .name("syms")
        .datasets([ds1, ds2])
        .dimensions([d1, d2])
        .measures([m1, m2])
        .build()
        .expect("build clean");
    assert_eq!(model.datasets.len(), 2);
    assert_eq!(model.dimensions.len(), 2);
    assert_eq!(model.measures.len(), 2);
}
