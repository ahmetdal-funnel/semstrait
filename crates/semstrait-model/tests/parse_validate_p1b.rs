//! P1b regression tests — SR-* / SR-E-* variants implemented during
//! the Phase P1b audit-and-implement pass. Each rule has a positive
//! (variant fires) and a negative (variant does not fire) test.

use semstrait_core::{DataType, Grain};
use semstrait_model::{
    parse, AggregationType, Dataset, Dimension, DimensionEntry, DimensionType, LeafExtras, Measure,
    MeasureEntry, ParseErrorKind, SemanticInterface, SemanticMapping, SemanticModel, TemporalShape,
    ValidateErrorKind,
};

// ─────────────────────── SR-8 — InvalidIdentifier ───────────────────

#[test]
fn parse_emits_invalid_identifier_when_dataset_name_starts_with_digit() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: 1bogus
      extras:
        semantic_mapping: auto
"#;
    let err = parse(yaml).unwrap_err();
    assert!(err.iter().any(|d| matches!(
        &d.kind,
        ParseErrorKind::InvalidIdentifier { raw, .. } if raw == "1bogus"
    )));
}

#[test]
fn parse_emits_invalid_identifier_when_inline_dimension_name_has_dash() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      dimensions:
        - name: bad-dim
          data_type: string
          type: categorical
"#;
    let err = parse(yaml).unwrap_err();
    assert!(err.iter().any(|d| matches!(
        &d.kind,
        ParseErrorKind::InvalidIdentifier { raw, .. } if raw == "bad-dim"
    )));
}

#[test]
fn parse_does_not_emit_invalid_identifier_when_names_match_grammar() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      dimensions:
        - name: country_code
          data_type: string
          type: categorical
"#;
    // Post-P4: `parse` returns `Ok(builder)` only when no parse-stage
    // diagnostic fired. Success here is the regression assertion.
    let _builder = parse(yaml).expect("parse ok");
}

// ─────────────────── SR-E-4 — RelationshipMissingCardinality ────────

#[test]
fn parse_emits_relationship_missing_cardinality_when_cardinality_omitted() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      extras:
        semantic_mapping: auto
    - name: customers
      extras:
        semantic_mapping: auto
  relationships:
    - name: orders_to_customers
      from: orders
      to: customers
      keys:
        - from: customer_id
          to: id
"#;
    let err = parse(yaml).unwrap_err();
    assert!(err.iter().any(|d| matches!(
        d.kind,
        ParseErrorKind::RelationshipMissingCardinality { .. }
    )));
}

#[test]
fn parse_does_not_emit_relationship_missing_cardinality_when_cardinality_present() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      extras:
        semantic_mapping: auto
    - name: customers
      extras:
        semantic_mapping: auto
  relationships:
    - name: orders_to_customers
      from: orders
      to: customers
      cardinality: many_to_one
      keys:
        - from: customer_id
          to: id
"#;
    let _builder = parse(yaml).expect("parse ok");
}

// ─────────────────────── SR-E-9 — MeasureMissingAgg ─────────────────

#[test]
fn parse_emits_measure_missing_agg_when_agg_omitted() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: integer
"#;
    let err = parse(yaml).unwrap_err();
    assert!(err.iter().any(|d| matches!(
        d.kind,
        ParseErrorKind::MeasureMissingAgg { .. }
    )));
}

#[test]
fn parse_does_not_emit_measure_missing_agg_when_agg_present() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: integer
          agg: sum
"#;
    let _builder = parse(yaml).expect("parse ok");
}

// ─────────────────── SR-E-10 — SemanticsMissingDataType ─────────────

#[test]
fn parse_emits_semantics_missing_data_type_when_dimension_lacks_data_type() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      dimensions:
        - name: country
          type: categorical
"#;
    let err = parse(yaml).unwrap_err();
    assert!(err.iter().any(|d| matches!(
        d.kind,
        ParseErrorKind::SemanticsMissingDataType { .. }
    )));
}

#[test]
fn parse_does_not_emit_semantics_missing_data_type_when_data_type_present() {
    let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      dimensions:
        - name: country
          data_type: string
          type: categorical
"#;
    let _builder = parse(yaml).expect("parse ok");
}

// ─────────────── 18 §1.5 — SemanticsShadowRootPool warning ──────────

fn build_revenue_pool_measure() -> Measure {
    Measure::builder("revenue")
        .data_type(DataType::Decimal {
            precision: 18,
            scale: 2,
        })
        .agg(AggregationType::Sum)
        .build()
}

fn build_order_ts_pool_dimension() -> Dimension {
    Dimension::builder("order_ts")
        .data_type(DataType::Timestamp { precision: 6 })
        .dim_type(DimensionType::temporal([Grain::Day]))
        .build()
}

fn build_dataset(name: &str, interface: SemanticInterface) -> Dataset {
    let extras = LeafExtras::builder()
        .semantic_mapping(SemanticMapping::builder().column("revenue", "amount_cents").build())
        .temporal(TemporalShape::events("order_ts", Some(Grain::Day)))
        .build();
    Dataset::builder(name)
        .extras(extras)
        .semantic_interface(interface)
        .build()
}

#[test]
fn validate_emits_semantics_shadow_root_pool_when_inline_collides_with_pool_name() {
    let revenue_inline = Measure::builder("revenue")
        .data_type(DataType::Decimal {
            precision: 18,
            scale: 2,
        })
        .agg(AggregationType::Max)
        .build();

    let shadowing_iface = SemanticInterface::builder()
        .dimensions(vec![DimensionEntry::r#ref("order_ts")])
        .measures(vec![MeasureEntry::inline(revenue_inline)])
        .build();
    let referring_iface = SemanticInterface::builder()
        .dimensions(vec![DimensionEntry::r#ref("order_ts")])
        .measures(vec![MeasureEntry::r#ref("revenue")])
        .build();

    let (_m, diags) = SemanticModel::builder()
        .name("shadow-case")
        .dataset(build_dataset("shadowing", shadowing_iface))
        .dataset(build_dataset("referring", referring_iface))
        .dimension(build_order_ts_pool_dimension())
        .measure(build_revenue_pool_measure())
        .build()
        .expect("validate clean (warning-only)");

    assert!(diags.iter().any(|d| matches!(
        &d.kind,
        semstrait_model::ModelBuildErrorKind::Validate(
            ValidateErrorKind::SemanticsShadowRootPool { carrier, name }
        ) if carrier == "Measure" && name == "revenue"
    )));
}

#[test]
fn validate_does_not_emit_semantics_shadow_root_pool_when_ref_used_instead() {
    let iface = SemanticInterface::builder()
        .dimensions(vec![DimensionEntry::r#ref("order_ts")])
        .measures(vec![MeasureEntry::r#ref("revenue")])
        .build();

    let (_m, diags) = SemanticModel::builder()
        .name("no-shadow")
        .dataset(build_dataset("orders", iface))
        .dimension(build_order_ts_pool_dimension())
        .measure(build_revenue_pool_measure())
        .build()
        .expect("validate clean");

    assert!(!diags.iter().any(|d| matches!(
        d.kind,
        semstrait_model::ModelBuildErrorKind::Validate(
            ValidateErrorKind::SemanticsShadowRootPool { .. }
        )
    )));
}
