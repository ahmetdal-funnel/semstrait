//! Integration tests — compile roundtrip through the full pipeline.
//!
//! Loads models from test_data/ via FileSystemRegistry, compiles queries
//! via StatelessCompiler, and asserts output structure.

use semstrait_core::compiler::{SemanticCompiler, SemanticQuery, StatelessCompiler};
use semstrait_core::output::{ColumnRole, CompileOpts};
use semstrait_core::registry::{FileSystemRegistry, ModelRef};

fn test_registry() -> FileSystemRegistry {
    let base = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test_data"
    ));
    FileSystemRegistry::new(base)
}

fn compiler() -> StatelessCompiler<FileSystemRegistry> {
    StatelessCompiler::new(test_registry())
}

#[test]
fn test_minimal_model_roundtrip() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("minimal"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns.len(), 2);
    assert_eq!(plan.columns[0].name, "order_date");
    assert_eq!(plan.columns[0].role, ColumnRole::Dimension);
    assert_eq!(plan.columns[0].data_type, "date");
    assert_eq!(plan.columns[1].name, "revenue");
    assert_eq!(plan.columns[1].role, ColumnRole::Measure);
    assert_eq!(plan.columns[1].data_type, "f64");

    // Minimal model uses dataset fallback — now produces real SQL
    let sql = plan.sql.as_ref().unwrap();
    assert!(sql.contains("SELECT"), "dataset fallback should emit SELECT: {sql}");
    assert!(sql.contains("FROM"), "dataset fallback should emit FROM: {sql}");
    assert!(sql.contains("GROUP BY"), "dataset fallback should emit GROUP BY: {sql}");
    assert!(sql.contains("SUM("), "dataset fallback should emit aggregate: {sql}");
}

#[test]
fn test_grainset_roundtrip() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("grainset_basic"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into(), "order_count".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns.len(), 3);
    assert_eq!(plan.columns[0].role, ColumnRole::Dimension);
    assert_eq!(plan.columns[1].role, ColumnRole::Measure);
    assert_eq!(plan.columns[2].role, ColumnRole::Measure);

    let sql = plan.sql.as_ref().unwrap();
    assert!(sql.contains("SELECT"), "grainset SQL must have SELECT: {sql}");
    assert!(sql.contains("FROM"), "grainset SQL must have FROM: {sql}");
    assert!(sql.contains("GROUP BY"), "grainset SQL must have GROUP BY: {sql}");
    assert!(sql.contains("SUM"), "grainset SQL must have aggregate function: {sql}");
}

#[test]
fn test_unionset_roundtrip() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("unionset_basic"),
                dimensions: vec!["event_date".into(), "event_type".into()],
                measures: vec!["event_count".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns.len(), 3);

    let sql = plan.sql.as_ref().unwrap();
    assert!(sql.contains("SELECT"), "unionset SQL must have SELECT: {sql}");
    assert!(sql.contains("UNION ALL"), "unionset SQL must have UNION ALL: {sql}");
    assert!(sql.contains("GROUP BY"), "unionset SQL must have GROUP BY: {sql}");
}

#[test]
fn test_joinset_roundtrip() {
    // Request columns from both anchor (orders) and joined (customers) datasets
    // to force a JOIN in the plan.
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("joinset_basic"),
                dimensions: vec!["order_date".into(), "customer_name".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns.len(), 3);

    let sql = plan.sql.as_ref().unwrap();
    assert!(sql.contains("SELECT"), "joinset SQL must have SELECT: {sql}");
    assert!(sql.contains("JOIN"), "joinset SQL must have JOIN: {sql}");
    assert!(sql.contains("GROUP BY"), "joinset SQL must have GROUP BY: {sql}");
}

#[test]
fn test_joinset_prune_when_anchor_only() {
    // When only anchor columns are requested, the join should be pruned.
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("joinset_basic"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    let sql = plan.sql.as_ref().unwrap();
    assert!(!sql.contains("JOIN"), "anchor-only query should not JOIN: {sql}");
    assert!(sql.contains("GROUP BY"), "still needs GROUP BY: {sql}");
}

#[test]
fn test_full_model_roundtrip() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("full_model"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns.len(), 2);

    let sql = plan.sql.as_ref().unwrap();
    assert!(sql.contains("SELECT"), "full model SQL must have SELECT: {sql}");
    assert!(sql.contains("GROUP BY"), "full model SQL must have GROUP BY: {sql}");
}

#[test]
fn test_missing_model_fails() {
    let err = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("does_not_exist"),
                dimensions: vec![],
                measures: vec![],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap_err();

    assert!(err.to_string().contains("PARSE_E001"));
}

#[test]
fn test_no_sql_when_disabled() {
    let opts = CompileOpts {
        emit_sql: false,
        ..CompileOpts::default()
    };
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("minimal"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &opts,
        )
        .unwrap();

    assert!(plan.sql.is_none());
}

#[test]
fn test_output_column_types_from_schema() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("grainset_basic"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns[0].data_type, "date");
    assert_eq!(plan.columns[1].data_type, "f64");
}

#[test]
fn test_grainset_metrics_roundtrip() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("grainset_metrics"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec!["avg_order_value".into()],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();

    assert_eq!(plan.columns.len(), 3);
    assert_eq!(plan.columns[0].name, "order_date");
    assert_eq!(plan.columns[0].role, ColumnRole::Dimension);
    assert_eq!(plan.columns[1].name, "revenue");
    assert_eq!(plan.columns[1].role, ColumnRole::Measure);
    assert_eq!(plan.columns[2].name, "avg_order_value");
    assert_eq!(plan.columns[2].role, ColumnRole::Metric);

    let sql = plan.sql.as_ref().unwrap();
    assert!(sql.contains("SELECT"), "metric SQL must have SELECT: {sql}");
    assert!(sql.contains("avg_order_value"), "metric alias should appear in SQL: {sql}");
    assert!(sql.contains("/"), "metric division should appear: {sql}");
}

#[test]
fn test_empty_query_produces_empty_columns() {
    // An empty query (no dims/measures/metrics) compiles but returns zero columns.
    // This documents the current behavior; a future version may reject it.
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("minimal"),
                dimensions: vec![],
                measures: vec![],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();
    assert!(plan.columns.is_empty());
}

#[test]
fn test_nonexistent_dimension_fails() {
    let err = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("grainset_basic"),
                dimensions: vec!["nonexistent_dim".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn test_sql_identifiers_are_quoted() {
    let plan = compiler()
        .compile(
            &SemanticQuery {
                model: ModelRef::new("grainset_basic"),
                dimensions: vec!["order_date".into()],
                measures: vec!["revenue".into()],
                metrics: vec![],
                domain: None,
                aggregation: None,
                user_attributes: Default::default(),
            },
            &CompileOpts::default(),
        )
        .unwrap();
    let sql = plan.sql.as_ref().unwrap();
    // Table name should be double-quoted (ANSI SQL identifier quoting)
    assert!(
        sql.contains("\"warehouse.orders_monthly\""),
        "table name should be quoted: {sql}"
    );
}
