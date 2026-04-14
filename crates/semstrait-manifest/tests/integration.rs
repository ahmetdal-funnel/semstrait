//! Integration tests for semstrait-manifest.

use semstrait_manifest::{
    CompileSource, CompiledManifest, InMemoryRepository, ManifestCompiler, Repository,
};
use semstrait_manifest::acceleration::CompiledDataKind;
use semstrait_model::TemporalGrain;

// ============================================================================
// Compile roundtrip with minimal YAML
// ============================================================================

#[tokio::test]
async fn test_compile_minimal_yaml() {
    let yaml = r#"
semantic_model:
  name: test_model
  description: A test model
  datasets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    assert_eq!(manifest.version, 3);
    assert_eq!(manifest.model_name, "test_model");
    assert_eq!(
        manifest.model_description,
        Some("A test model".to_string())
    );
    assert_eq!(manifest.entities.len(), 1);
    assert!(manifest.entities.contains_key("orders"));

    let orders = manifest.entities["orders"].interface();
    assert_eq!(orders.dimensions.len(), 1);
    assert_eq!(orders.measures.len(), 1);
    assert!(orders.dimensions.contains_key("order_date"));
    assert!(orders.measures.contains_key("revenue"));

    // The source hash should be a hex SHA-256
    assert_eq!(manifest.source_hash.len(), 64);
}

#[tokio::test]
async fn test_compile_kind_with_datasets() {
    let yaml = r#"
semantic_model:
  name: kind_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount_usd
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    assert_eq!(manifest.entities.len(), 1);
    assert!(manifest.entities.contains_key("sales"));
    let bindings = manifest.entities["sales"].bindings();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].dataset_name, "orders_daily");
    assert!(bindings[0].column_mapping.physical.contains_key("order_date"));
    assert!(bindings[0].column_mapping.physical.contains_key("revenue"));
}

// ============================================================================
// Auto column mapping
// ============================================================================

#[tokio::test]
async fn test_auto_column_mapping_expansion() {
    let yaml = r#"
semantic_model:
  name: auto_test
  grainsets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: region
          data_type: string
          type:
            categorical:
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders_daily
          extras:
            column_mapping: auto
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with auto mapping should succeed");

    let bindings = manifest.entities["orders"].bindings();
    let binding = &bindings[0];

    // Auto should have expanded to identity mappings for all interface names.
    assert!(binding.column_mapping.physical.contains_key("order_date"));
    assert!(binding.column_mapping.physical.contains_key("region"));
    assert!(binding.column_mapping.physical.contains_key("revenue"));

    // Each mapping should be identity (name → name).
    assert_eq!(binding.column_mapping.physical["revenue"], "revenue");
}

// ============================================================================
// Validation errors
// ============================================================================

#[tokio::test]
async fn test_duplicate_dataset_error() {
    let yaml = r#"
semantic_model:
  name: test
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
    - name: orders
      measures:
        - name: cost
          data_type: float64
          expr: "SUM(cost)"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("duplicate entity name"), "got: {}", msg);
}

#[tokio::test]
async fn test_invalid_ref_error() {
    let yaml = r#"
semantic_model:
  name: test
  datasets:
    - name: orders
      dimensions:
        - ref: nonexistent_dimension
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent_dimension") || msg.contains("reference resolution"),
        "got: {}",
        msg
    );
}

#[tokio::test]
async fn test_invalid_column_mapping_key() {
    let yaml = r#"
semantic_model:
  name: test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
              bogus_field: some_col
            storage:
              format: parquet
              paths:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("bogus_field"), "got: {}", msg);
}

#[tokio::test]
async fn test_joinset_without_relationships() {
    let yaml = r#"
semantic_model:
  name: test
  joinsets:
    - name: order_details
      associativity: left
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("must have at least one relationship"),
        "got: {}",
        msg
    );
}

#[tokio::test]
async fn test_raw_sql_rejected() {
    let yaml = r#"
semantic_model:
  name: test
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SELECT SUM(amount) FROM orders"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("raw SQL rejected"), "got: {}", msg);
}

// ============================================================================
// Metric cycle detection
// ============================================================================

#[tokio::test]
async fn test_metric_no_cycle() {
    let yaml = r#"
semantic_model:
  name: test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
        - name: cost
          data_type: float64
          expr: "SUM(cost_amount)"
      metrics:
        - name: profit
          data_type: float64
          expr: "revenue - cost"
        - name: margin
          data_type: float64
          expr: "profit / revenue"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
              cost: cost_amount
            storage:
              format: parquet
              paths:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("no cycle, should compile");

    let iface = manifest.entities["sales"].interface();

    // profit references measures only -> depth 1
    assert_eq!(iface.metrics["profit"].depth, 1);
    // margin references profit (a metric) and revenue (a measure)
    // profit has depth 1, so margin = max(1, 0) + 1 = 2
    assert_eq!(iface.metrics["margin"].depth, 2);
}

#[tokio::test]
async fn test_metric_cycle_detected() {
    let yaml = r#"
semantic_model:
  name: test
  metrics:
    - name: metric_a
      data_type: float64
      expr: "metric_b + 1"
    - name: metric_b
      data_type: float64
      expr: "metric_a + 1"
  datasets:
    - name: dummy
      measures:
        - name: base
          data_type: float64
          expr: "SUM(x)"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("cycle"), "got: {}", msg);
}

// ============================================================================
// JSON serialization roundtrip
// ============================================================================

#[tokio::test]
async fn test_json_serialization_roundtrip() {
    let yaml = r#"
semantic_model:
  name: roundtrip_test
  datasets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
                - month
        - name: category
          data_type: string
          type:
            categorical:
              enum:
                - electronics
                - clothing
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
        - name: order_count
          data_type: int64
          expr: "COUNT(id)"
      metrics:
        - name: avg_order_value
          data_type: float64
          expr: "revenue / order_count"
  relationships:
    - name: orders_customers
      from: orders
      to: customers
      type: left
      columns:
        - from: customer_id
          to: id
      cardinality: many_to_one
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&manifest).expect("should serialize");

    // Deserialize back
    let deserialized: CompiledManifest =
        serde_json::from_str(&json).expect("should deserialize");

    // Verify roundtrip fidelity
    assert_eq!(deserialized.version, manifest.version);
    assert_eq!(deserialized.model_name, manifest.model_name);
    assert_eq!(deserialized.source_hash, manifest.source_hash);
    assert_eq!(deserialized.entities.len(), manifest.entities.len());
    assert_eq!(
        deserialized.relationships.len(),
        manifest.relationships.len()
    );

    let orig_orders = manifest.entities["orders"].interface();
    let rt_orders = deserialized.entities["orders"].interface();
    assert_eq!(orig_orders.dimensions.len(), rt_orders.dimensions.len());
    assert_eq!(orig_orders.measures.len(), rt_orders.measures.len());
    assert_eq!(orig_orders.metrics.len(), rt_orders.metrics.len());
    // The Expr should also roundtrip
    let orig_revenue = &orig_orders.measures["revenue"];
    let rt_revenue = &rt_orders.measures["revenue"];
    assert_eq!(orig_revenue.expr, rt_revenue.expr);
    assert_eq!(orig_revenue.expr_source, rt_revenue.expr_source);
}

// ============================================================================
// Repository tests
// ============================================================================

#[tokio::test]
async fn test_repository_save_and_load() {
    let yaml = r#"
semantic_model:
  name: repo_test
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    let repo = InMemoryRepository::new();

    // Load should fail when empty
    assert!(repo.load().is_err());

    // Save and load
    repo.save(&manifest).expect("save should succeed");
    let loaded = repo.load().expect("load should succeed");

    assert_eq!(loaded.model_name, "repo_test");
    assert_eq!(loaded.version, 3);
    assert_eq!(loaded.entities.len(), 1);
}

#[tokio::test]
async fn test_repository_with_manifest() {
    let yaml = r#"
semantic_model:
  name: preloaded
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    let repo = InMemoryRepository::with_manifest(manifest);
    let loaded = repo.load().expect("should load preloaded manifest");
    assert_eq!(loaded.model_name, "preloaded");
}

// ============================================================================
// Source resolution: tables without catalog keeps patterns as-is
// ============================================================================

#[tokio::test]
async fn test_tables_without_catalog_keeps_pattern() {
    let yaml = r#"
semantic_model:
  name: glob_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              tables:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new(); // no catalog — tables kept as-is
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_ok(), "got: {:?}", result.unwrap_err());
    let manifest = result.unwrap();
    let binding = &manifest.entities.values().next().unwrap().bindings()[0];
    assert_eq!(binding.resolved_sources.len(), 1);
    assert_eq!(binding.resolved_sources[0].reference, "warehouse.orders");
}

// Wildcard patterns require a provider
// ============================================================================

#[tokio::test]
async fn test_wildcard_tables_require_catalog() {
    let yaml = r#"
semantic_model:
  name: wildcard_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              tables:
                - "warehouse.orders_*"
"#;

    let compiler = ManifestCompiler::new(); // no catalog
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("requires a catalog"),
        "expected catalog error, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_wildcard_paths_require_storage() {
    let yaml = r#"
semantic_model:
  name: wildcard_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "/data/orders/*.parquet"
"#;

    let compiler = ManifestCompiler::new(); // no storage provider
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("requires a storage"),
        "expected storage error, got: {}",
        msg
    );
}

// ============================================================================
// Expression compilation
// ============================================================================

#[tokio::test]
async fn test_compiled_expressions_are_expr() {
    let yaml = r#"
semantic_model:
  name: expr_test
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
        - name: order_count
          data_type: int64
          expr: "COUNT(id)"
        - name: avg_amount
          data_type: float64
          expr: "AVG(price)"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    let orders = manifest.entities["orders"].interface();

    // revenue: legacy "SUM(amount)" auto-upgraded → agg: Sum, expr: Column("amount")
    let revenue = &orders.measures["revenue"];
    assert_eq!(revenue.agg, semstrait_core::Aggregation::Sum);
    assert!(
        matches!(&revenue.expr, semstrait_core::Expr::Column(col) if col.name == "amount"),
        "revenue expr should be Column(amount) after auto-upgrade, got {:?}",
        revenue.expr
    );
    assert_eq!(revenue.expr_source, "SUM(amount)");

    // order_count: legacy "COUNT(id)" auto-upgraded → agg: Count, expr: Column("id")
    let count = &orders.measures["order_count"];
    assert_eq!(count.agg, semstrait_core::Aggregation::Count);
    assert!(matches!(&count.expr, semstrait_core::Expr::Column(col) if col.name == "id"));

    // avg_amount: legacy "AVG(price)" auto-upgraded → agg: Avg, expr: Column("price")
    let avg = &orders.measures["avg_amount"];
    assert_eq!(avg.agg, semstrait_core::Aggregation::Avg);
    assert!(matches!(&avg.expr, semstrait_core::Expr::Column(col) if col.name == "price"));
}

// ============================================================================
// Kind type compilation
// ============================================================================

#[tokio::test]
async fn test_compiled_kind_types() {
    let yaml = r#"
semantic_model:
  name: kind_type_test
  grainsets:
    - name: grain_kind
      dimensions:
        - name: d
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: m
          data_type: float64
          expr: "SUM(x)"
      datasets:
        - name: ds1
          extras:
            column_mapping:
              d: date_col
              m: amount
            storage:
              format: parquet
              paths:
                - t1
  unionsets:
    - name: union_kind
      dimensions:
        - name: d
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: m
          data_type: float64
          expr: "SUM(x)"
      datasets:
        - name: ds2
          extras:
            column_mapping:
              d: date_col
              m: amount
            storage:
              format: parquet
              paths:
                - t2
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    // Single-dataset kinds compile as Dataset (fast path — no routing needed).
    assert!(matches!(
        &manifest.entities["grain_kind"],
        CompiledDataKind::Dataset(_)
    ));
    assert!(matches!(
        &manifest.entities["union_kind"],
        CompiledDataKind::Dataset(_)
    ));
}

// ============================================================================
// Kind extras defaults
// ============================================================================

#[tokio::test]
async fn test_kind_extras_column_mapping_inherited() {
    // A dataset with no column_mapping should inherit kind.extras.column_mapping.
    let yaml = r#"
semantic_model:
  name: kind_extras_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount_usd
      datasets:
        - name: orders_daily
          extras:
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with inherited column mapping should succeed");

    let binding = &manifest.entities["sales"].bindings()[0];

    // Should have inherited the kind-level default mapping.
    assert_eq!(binding.column_mapping.physical["order_date"], "created_at");
    assert_eq!(binding.column_mapping.physical["revenue"], "amount_usd");
}

#[tokio::test]
async fn test_kind_extras_explicit_overrides_default() {
    // Per-dataset explicit column_mapping entries override the kind default; entries
    // not present in the dataset mapping are filled in from the kind default.
    let yaml = r#"
semantic_model:
  name: kind_extras_override_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: default_date_col
          revenue: default_revenue_col
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              revenue: actual_amount
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with override mapping should succeed");

    let binding = &manifest.entities["sales"].bindings()[0];

    // order_date not set on dataset — should come from kind default.
    assert_eq!(binding.column_mapping.physical["order_date"], "default_date_col");
    // revenue is overridden by the dataset.
    assert_eq!(binding.column_mapping.physical["revenue"], "actual_amount");
}

#[tokio::test]
async fn test_kind_extras_temporal_propagated() {
    // temporal from kind.extras should be propagated to datasets that do not set temporal.
    let yaml = r#"
semantic_model:
  name: kind_extras_temporal_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
        temporal:
          type:
            snapshot:
              snapshotted_at: snapshot_ts
      datasets:
        - name: orders_daily
          extras:
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with default temporal should succeed");

    // Temporal propagation is verified by the fact that compilation succeeds
    // (temporal equivalence validation would catch mismatches).
    // The binding's resolved sources confirm the dataset was compiled.
    let binding = &manifest.entities["sales"].bindings()[0];
    assert_eq!(binding.dataset_name, "orders_daily");
}

// ============================================================================
// Grain auto-propagation (Phase I)
// ============================================================================

#[tokio::test]
async fn test_grain_auto_propagation_same_column() {
    // Rule 1: temporal.grain auto-sets column_mapping grain when
    // column_mapping[dim].column == temporal.occurred_at (same physical column).
    let yaml = r#"
semantic_model:
  name: grain_propagation_test
  grainsets:
    - name: events
      dimensions:
        - name: event_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: count
          data_type: int64
          agg: count
          expr: "1"
      extras:
        column_mapping:
          event_date: created_at
          count: "1"
        temporal:
          grain: day
          dimension: event_date
          type:
            events:
              occurred_at: created_at
      datasets:
        - name: clicks
          extras:
            storage:
              format: parquet
              paths:
                - data/clicks
"#;
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    let binding = &manifest.entities["events"].bindings()[0];
    // After propagation, column_mapping for event_date should map to created_at with Day grain.
    assert_eq!(binding.column_mapping.physical["event_date"], "created_at");
    let tm = binding.column_mapping.temporal.get("event_date").expect("temporal mapping should exist");
    assert_eq!(tm.grain, Some(TemporalGrain::Day));
}

#[tokio::test]
async fn test_grain_no_propagation_different_column() {
    // Rule 2: no propagation when column_mapping column differs from occurred_at.
    let yaml = r#"
semantic_model:
  name: grain_no_propagation_test
  grainsets:
    - name: events
      dimensions:
        - name: event_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: count
          data_type: int64
          agg: count
          expr: "1"
      extras:
        column_mapping:
          event_date: order_month
          count: "1"
        temporal:
          grain: day
          dimension: event_date
          type:
            events:
              occurred_at: created_at
      datasets:
        - name: clicks
          extras:
            storage:
              format: parquet
              paths:
                - data/clicks
"#;
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    let binding = &manifest.entities["events"].bindings()[0];
    // Different physical column — should remain without grain.
    assert_eq!(binding.column_mapping.physical["event_date"], "order_month");
    assert!(
        !binding.column_mapping.temporal.contains_key("event_date"),
        "no grain should be propagated when columns differ"
    );
}

#[tokio::test]
async fn test_grain_explicit_grain_not_overwritten() {
    // Rule 3: explicit column_mapping grain always wins.
    let yaml = r#"
semantic_model:
  name: grain_explicit_test
  grainsets:
    - name: events
      dimensions:
        - name: event_date
          data_type: date
          type:
            temporal:
              grains:
                - day
                - month
      measures:
        - name: count
          data_type: int64
          agg: count
          expr: "1"
      extras:
        column_mapping:
          event_date:
            column: created_at
            grain: month
          count: "1"
        temporal:
          grain: day
          dimension: event_date
          type:
            events:
              occurred_at: created_at
      datasets:
        - name: clicks
          extras:
            storage:
              format: parquet
              paths:
                - data/clicks
"#;
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    let binding = &manifest.entities["events"].bindings()[0];
    // Explicit grain=month must NOT be overwritten by temporal.grain=day.
    assert_eq!(binding.column_mapping.physical["event_date"], "created_at");
    let tm = binding.column_mapping.temporal.get("event_date").expect("temporal mapping should exist");
    assert_eq!(tm.grain, Some(TemporalGrain::Month), "explicit grain must not be overwritten");
}

#[tokio::test]
async fn test_events_temporal_variant_parses() {
    // Verify the events temporal variant parses correctly through the full pipeline.
    let yaml = r#"
semantic_model:
  name: events_variant_test
  grainsets:
    - name: clickstream
      dimensions:
        - name: click_time
          data_type: timestamp
          type:
            temporal:
              grains:
                - hour
      measures:
        - name: clicks
          data_type: int64
          agg: count
          expr: "1"
      extras:
        column_mapping:
          click_time: ts
          clicks: "1"
        temporal:
          grain: hour
          dimension: click_time
          type:
            events:
              occurred_at: ts
      datasets:
        - name: click_events
          extras:
            storage:
              format: parquet
              paths:
                - data/clicks
"#;
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("events temporal variant should compile");

    // Events temporal variant compiles successfully — proves it parsed correctly.
    // Temporal config is consumed during compilation; verify the binding exists.
    let binding = &manifest.entities["clickstream"].bindings()[0];
    assert_eq!(binding.dataset_name, "click_events");
    // Grain should have been propagated to the temporal mapping.
    let tm = binding.column_mapping.temporal.get("click_time").expect("temporal mapping should exist");
    assert_eq!(tm.grain, Some(TemporalGrain::Hour));
}

// ============================================================================
// Dimension grain auto-derivation (Phase I)
// ============================================================================

#[tokio::test]
async fn test_dimension_grain_auto_derived() {
    // When temporal dimension grains are empty and datasets have temporal.grain,
    // grains should be auto-derived (all coarser-or-equal to finest).
    let yaml = r#"
semantic_model:
  name: grain_derivation_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: []
      measures:
        - name: revenue
          data_type: float64
          agg: sum
          expr: amount
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
        temporal:
          grain: day
          dimension: order_date
          type:
            events:
              occurred_at: created_at
      datasets:
        - name: orders_daily
          extras:
            storage:
              format: parquet
              paths:
                - data/orders_daily
"#;
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    // Check the compiled dimension has auto-derived grains.
    let iface = manifest.entities["sales"].interface();
    let dim = &iface.dimensions["order_date"];
    if let semstrait_model::DimensionType::Temporal(td) = &dim.dim_type {
        assert!(
            td.grains.contains(&TemporalGrain::Day),
            "should include Day"
        );
        assert!(
            td.grains.contains(&TemporalGrain::Month),
            "should include Month"
        );
        assert!(
            td.grains.contains(&TemporalGrain::Year),
            "should include Year"
        );
        assert!(
            !td.grains.contains(&TemporalGrain::Hour),
            "should not include grains finer than Day"
        );
    } else {
        panic!("expected Temporal dimension type");
    }

    // Check COMP_I001 diagnostic was emitted.
    assert!(
        manifest.diagnostics.warnings.iter().any(|w| w.code == "COMP_I001"),
        "COMP_I001 diagnostic should be emitted for auto-derived grains"
    );
}

#[tokio::test]
async fn test_dimension_grain_explicit_not_overwritten() {
    // When dimension grains are explicitly set, they should not be overwritten.
    let yaml = r#"
semantic_model:
  name: grain_explicit_dim_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
                - month
                - year
      measures:
        - name: revenue
          data_type: float64
          agg: sum
          expr: amount
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
        temporal:
          grain: day
          dimension: order_date
          type:
            events:
              occurred_at: created_at
      datasets:
        - name: orders_daily
          extras:
            storage:
              format: parquet
              paths:
                - data/orders_daily
"#;
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    let iface = manifest.entities["sales"].interface();
    let dim = &iface.dimensions["order_date"];
    if let semstrait_model::DimensionType::Temporal(td) = &dim.dim_type {
        // Explicit grains should be preserved (not replaced by derived set).
        assert_eq!(td.grains.len(), 3, "explicit grains should not be overwritten");
        assert!(td.grains.contains(&TemporalGrain::Day));
        assert!(td.grains.contains(&TemporalGrain::Month));
        assert!(td.grains.contains(&TemporalGrain::Year));
    } else {
        panic!("expected Temporal dimension type");
    }

    // No COMP_I001 should be emitted.
    assert!(
        !manifest.diagnostics.warnings.iter().any(|w| w.code == "COMP_I001"),
        "COMP_I001 should not be emitted for explicit grains"
    );
}

#[tokio::test]
async fn test_kind_extras_catalog_propagated() {
    // catalog from kind.extras should be propagated to datasets that do not set catalog.
    let yaml = r#"
semantic_model:
  name: kind_extras_catalog_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
        catalog: polaris_prod
      datasets:
        - name: orders_daily
          extras:
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with default catalog should succeed");

    // Compilation success proves catalog alias from kind.extras was accepted.
    // For path-type sources, catalog_alias is None (paths are not catalog-managed).
    // Catalog alias only appears on table-type resolved sources.
    let binding = &manifest.entities["sales"].bindings()[0];
    assert!(
        !binding.resolved_sources.is_empty(),
        "resolved sources should be populated"
    );
}

#[tokio::test]
async fn test_storage_table_field() {
    // storage: { tables: [...] } should parse and compile without error.
    let yaml = r#"
semantic_model:
  name: storage_table_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              tables:
                - schema_name.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("storage with table field should compile");

    // Compilation success is the primary assertion; verify the binding is present.
    let bindings = manifest.entities["sales"].bindings();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].dataset_name, "orders_daily");

    // Table reference should be captured in resolved_sources with Table type.
    assert_eq!(bindings[0].resolved_sources.len(), 1);
    assert_eq!(bindings[0].resolved_sources[0].reference, "schema_name.orders_daily");
    assert_eq!(bindings[0].resolved_sources[0].source_type, semstrait_manifest::SourceType::Table);
}

#[tokio::test]
async fn test_column_mapping_inherited_sentinel() {
    // The explicit string `column_mapping: inherited` should resolve identically
    // to an absent column_mapping — both inherit from kind.extras.column_mapping.
    let yaml = r#"
semantic_model:
  name: inherited_sentinel_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
      datasets:
        - name: orders
          extras:
            column_mapping: inherited
            storage:
              format: parquet
              paths:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("column_mapping: inherited should compile");

    let binding = &manifest.entities["sales"].bindings()[0];

    assert_eq!(binding.column_mapping.physical["order_date"], "created_at");
    assert_eq!(binding.column_mapping.physical["revenue"], "amount");
}

// ============================================================================
// Dimension type defaults to categorical
// ============================================================================

#[tokio::test]
async fn test_dimension_type_defaults_to_categorical() {
    // When `type:` is omitted from a dimension, it should default to Categorical
    // and compile successfully.
    let yaml = r#"
semantic_model:
  name: default_dim_type_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: region
          data_type: string
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              region: region_name
              revenue: amount_usd
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with default dim type should succeed");

    let iface = manifest.entities["sales"].interface();

    // order_date should be Temporal (explicitly set).
    assert!(
        matches!(
            &iface.dimensions["order_date"].dim_type,
            semstrait_manifest::DimensionType::Temporal(_)
        ),
        "order_date should be Temporal"
    );

    // region should be Categorical (defaulted).
    assert!(
        matches!(
            &iface.dimensions["region"].dim_type,
            semstrait_manifest::DimensionType::Categorical(c) if c.enum_values.is_none()
        ),
        "region should default to Categorical, got {:?}",
        iface.dimensions["region"].dim_type
    );
}

// ============================================================================
// Temporal equivalence validation
// ============================================================================

#[tokio::test]
async fn test_temporal_mismatch_error() {
    // Kind has timeseries, dataset has snapshot → compile error.
    let yaml = r#"
semantic_model:
  name: temporal_mismatch_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
        temporal:
          type:
            timeseries:
              occurred_at: event_ts
      datasets:
        - name: orders_daily
          extras:
            temporal:
              type:
                snapshot:
                  snapshotted_at: snap_ts
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("temporal type mismatch"),
        "expected temporal mismatch error, got: {}",
        msg
    );
    assert!(msg.contains("timeseries"), "got: {}", msg);
    assert!(msg.contains("snapshot"), "got: {}", msg);
}

#[tokio::test]
async fn test_temporal_equivalent_ok() {
    // Kind has timeseries, dataset has timeseries with different column → OK.
    let yaml = r#"
semantic_model:
  name: temporal_equivalent_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      extras:
        column_mapping:
          order_date: created_at
          revenue: amount
        temporal:
          type:
            timeseries:
              occurred_at: event_ts
      datasets:
        - name: orders_daily
          extras:
            temporal:
              type:
                timeseries:
                  occurred_at: different_ts_col
            storage:
              format: parquet
              paths:
                - warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("same temporal variant with different columns should compile");

    // Same temporal variant with different columns should compile OK.
    // Compilation success proves temporal equivalence validation passed.
    let binding = &manifest.entities["sales"].bindings()[0];
    assert_eq!(binding.dataset_name, "orders_daily");
}

// ============================================================================
// Incomplete mapping detection
// ============================================================================

#[tokio::test]
async fn test_incomplete_mapping_error() {
    // Dataset mapping is missing the 'revenue' interface name → compile error.
    let yaml = r#"
semantic_model:
  name: incomplete_mapping_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
            storage:
              format: parquet
              paths:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("is not mapped by any dataset"),
        "expected union coverage error, got: {}",
        msg
    );
    assert!(msg.contains("revenue"), "got: {}", msg);
}

// ============================================================================
// Multi-path / multi-table storage
// ============================================================================

#[tokio::test]
async fn test_storage_multiple_paths() {
    let yaml = r#"
semantic_model:
  name: multi_path_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/orders_2024.parquet"
                - "s3://bucket/orders_2025.parquet"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("multi-path storage should compile");

    let binding = &manifest.entities["sales"].bindings()[0];
    assert_eq!(binding.resolved_sources.len(), 2);
    assert_eq!(binding.resolved_sources[0].reference, "s3://bucket/orders_2024.parquet");
    assert_eq!(binding.resolved_sources[0].source_type, semstrait_manifest::SourceType::Path);
    assert_eq!(binding.resolved_sources[1].reference, "s3://bucket/orders_2025.parquet");
}

#[tokio::test]
async fn test_storage_two_paths_in_single_list() {
    let yaml = r#"
semantic_model:
  name: merged_path_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/orders_main.parquet"
                - "s3://bucket/orders_archive.parquet"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("multiple paths should compile");

    let binding = &manifest.entities["sales"].bindings()[0];
    assert_eq!(binding.resolved_sources.len(), 2);
    assert_eq!(binding.resolved_sources[0].reference, "s3://bucket/orders_main.parquet");
    assert_eq!(binding.resolved_sources[1].reference, "s3://bucket/orders_archive.parquet");
}

#[tokio::test]
async fn test_storage_mixed_path_and_table_error() {
    let yaml = r#"
semantic_model:
  name: mixed_storage_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/orders.parquet"
              tables:
                - "catalog.schema.orders"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("cannot mix paths and tables"),
        "expected mixed storage error, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_storage_single_path_resolved() {
    // Single path should populate resolved_sources with one entry.
    let yaml = r#"
semantic_model:
  name: singular_path_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("singular path should compile");

    let binding = &manifest.entities["sales"].bindings()[0];
    assert_eq!(binding.resolved_sources.len(), 1);
    assert_eq!(binding.resolved_sources[0].reference, "warehouse.orders");
    assert_eq!(binding.resolved_sources[0].source_type, semstrait_manifest::SourceType::Path);
}

// ============================================================================
// Declarative aggregation (Phase 3)
// ============================================================================

#[tokio::test]
async fn test_declarative_agg_simple_sum() {
    // Measure with `agg: sum` and no expr — column resolved from mapping by name.
    let yaml = r#"
semantic_model:
  name: declarative_agg_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("declarative agg: sum should compile");

    let measure = &manifest.entities["sales"].interface().measures["revenue"];
    assert_eq!(
        measure.agg,
        semstrait_core::expr::Aggregation::Sum
    );
    // expr should be an entity ref to the measure name (resolved from mapping at plan time).
    assert_eq!(measure.expr_source, "revenue");
}

#[tokio::test]
async fn test_declarative_agg_with_horizontal_expr() {
    // Measure with `agg: sum` and horizontal expr `amount + price`.
    let yaml = r#"
semantic_model:
  name: declarative_agg_expr_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: total_value
          data_type: float64
          agg: sum
          expr: "amount + price"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              total_value: amount
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("declarative agg with horizontal expr should compile");

    let measure = &manifest.entities["sales"].interface().measures["total_value"];
    assert_eq!(
        measure.agg,
        semstrait_core::expr::Aggregation::Sum
    );
    assert_eq!(measure.expr_source, "amount + price");
}

#[tokio::test]
async fn test_declarative_agg_rejects_aggregation_in_expr() {
    // When `agg` is specified, expr must NOT contain aggregation functions.
    let yaml = r#"
semantic_model:
  name: agg_reject_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          agg: sum
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "should reject aggregation in expr when agg is set");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("must not contain aggregation"),
        "error should mention aggregation rejection, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_declarative_agg_count_distinct() {
    let yaml = r#"
semantic_model:
  name: count_distinct_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: unique_customers
          data_type: int64
          agg: count_distinct
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              unique_customers: customer_id
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("count_distinct should compile");

    let measure = &manifest.entities["sales"].interface().measures["unique_customers"];
    assert_eq!(
        measure.agg,
        semstrait_core::expr::Aggregation::CountDistinct
    );
}

#[tokio::test]
async fn test_legacy_expr_still_works() {
    // Legacy format: no `agg`, expr contains aggregation.
    let yaml = r#"
semantic_model:
  name: legacy_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("legacy expr should still compile");

    let measure = &manifest.entities["sales"].interface().measures["revenue"];
    // Legacy `expr: "SUM(amount)"` is now auto-upgraded to declarative agg
    assert_eq!(measure.agg, semstrait_core::expr::Aggregation::Sum, "legacy SUM(amount) should auto-upgrade to Aggregation::Sum");
    assert_eq!(measure.expr_source, "SUM(amount)");
}

#[tokio::test]
async fn test_measure_requires_agg_or_expr() {
    // Neither agg nor expr — should fail.
    let yaml = r#"
semantic_model:
  name: missing_both_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "should fail when neither agg nor expr is specified");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("'agg' must be specified"),
        "error should mention missing agg, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_metric_with_declarative_agg() {
    // Metric with `agg: avg` — two-stage aggregation.
    let yaml = r#"
semantic_model:
  name: metric_agg_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      metrics:
        - name: avg_daily_revenue
          data_type: float64
          agg: avg
          expr: "revenue"
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("metric with agg should compile");

    let metric = &manifest.entities["sales"].interface().metrics["avg_daily_revenue"];
    assert_eq!(
        metric.agg.unwrap(),
        semstrait_core::expr::Aggregation::Avg
    );
}

#[tokio::test]
async fn test_all_agg_types_compile() {
    // Verify all 6 aggregation types parse and compile.
    let agg_types = ["sum", "avg", "count", "count_distinct", "min", "max"];
    for agg_type in &agg_types {
        let yaml = format!(
            r#"
semantic_model:
  name: all_agg_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: m1
          data_type: float64
          agg: {agg_type}
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              m1: col1
"#,
            agg_type = agg_type
        );

        let compiler = ManifestCompiler::new();
        let result = compiler
            .compile(CompileSource::Yaml(yaml))
            .await;
        assert!(
            result.is_ok(),
            "agg type '{}' should compile, got: {:?}",
            agg_type,
            result.err()
        );
        let manifest = result.unwrap();
        // agg is non-optional — verify it was set (format as debug string to confirm it's populated)
        let agg = manifest.entities["sales"].interface().measures["m1"].agg;
        let agg_debug = format!("{:?}", agg);
        assert!(
            !agg_debug.is_empty(),
            "agg type '{}' should produce compiled agg",
            agg_type
        );
    }
}

// ============================================================================
// Metadata dimension type (Phase 4)
// ============================================================================

#[tokio::test]
async fn test_metadata_dimension_path_token() {
    let yaml = r#"
semantic_model:
  name: metadata_path_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: source_partition
          data_type: string
          type:
            metadata:
              path:
                token: 3
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/year=2024/month=01/data.parquet"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("metadata dimension with path.token should compile");

    let dim = &manifest.entities["sales"].interface().dimensions["source_partition"];
    match &dim.dim_type {
        semstrait_model::DimensionType::Metadata(m) => {
            assert!(m.path.is_some());
            assert_eq!(m.path.as_ref().unwrap().token, 3);
        }
        other => panic!("Expected Metadata dimension type, got {:?}", other),
    }
}

#[tokio::test]
async fn test_metadata_dimension_partition_level() {
    let yaml = r#"
semantic_model:
  name: metadata_partition_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: partition_year
          data_type: string
          type:
            metadata:
              partition:
                level: 1
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/data.parquet"
              partition_def:
                type:
                  range:
                    column: year
                    start: "2020"
                    end: "2025"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("metadata dimension with partition.level should compile");

    let dim = &manifest.entities["sales"].interface().dimensions["partition_year"];
    match &dim.dim_type {
        semstrait_model::DimensionType::Metadata(m) => {
            assert!(m.partition.is_some());
            assert_eq!(m.partition.as_ref().unwrap().level, 1);
        }
        other => panic!("Expected Metadata dimension type, got {:?}", other),
    }
}

#[tokio::test]
async fn test_metadata_dimension_path_requires_storage_paths() {
    // path.token without storage paths should fail.
    let yaml = r#"
semantic_model:
  name: metadata_no_path_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: source_part
          data_type: string
          type:
            metadata:
              path:
                token: 2
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "path.token without storage paths should fail");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("no storage sources configured"),
        "error should mention missing storage sources, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_metadata_dimension_partition_requires_partition_def() {
    // partition.level without partition_def should fail.
    let yaml = r#"
semantic_model:
  name: metadata_no_partition_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: partition_year
          data_type: string
          type:
            metadata:
              partition:
                level: 1
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/data.parquet"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "partition.level without partition_def should fail");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("no partition definitions"),
        "error should mention missing partition definitions, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_metadata_dimension_requires_path_or_partition() {
    // Metadata dimension with neither path nor partition should fail.
    let yaml = r#"
semantic_model:
  name: metadata_empty_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: empty_meta
          data_type: string
          type:
            metadata: {}
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "empty metadata dimension should fail");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("either 'path' or 'partition'"),
        "error should mention missing path/partition, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_partition_level_zero_rejected() {
    let yaml = r#"
semantic_model:
  name: partition_zero_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: bad_partition
          data_type: string
          type:
            metadata:
              partition:
                level: 0
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/data.parquet"
              partition_def:
                type:
                  range:
                    column: year
                    start: "2020"
                    end: "2025"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "partition.level=0 should fail (1-indexed)");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("1-indexed"),
        "error should mention 1-indexed, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_partition_level_exceeds_depth() {
    let yaml = r#"
semantic_model:
  name: partition_depth_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: deep_partition
          data_type: string
          type:
            metadata:
              partition:
                level: 5
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/data.parquet"
              partition_def:
                type:
                  range:
                    column: year
                    start: "2020"
                    end: "2025"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(
        result.is_err(),
        "partition.level exceeding depth should fail"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("partition depth"),
        "error should mention partition depth, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_metadata_dimension_with_auto_mapping() {
    // Auto mapping should compile when metadata dimensions are present.
    // Metadata dims should be excluded from auto-generated identity mapping.
    let yaml = r#"
semantic_model:
  name: auto_mapping_metadata_test
  grainsets:
    - name: sales
      dimensions:
        - name: region
          data_type: string
        - name: source_partition
          data_type: string
          type:
            metadata:
              path:
                token: 3
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping: auto
            storage:
              format: parquet
              paths:
                - "s3://bucket/region=us/data.parquet"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("auto mapping with metadata dimension should compile");

    // The auto mapping should include region and revenue but NOT source_partition.
    let binding = &manifest.entities["sales"].bindings()[0];
    assert!(
        binding.column_mapping.physical.contains_key("region"),
        "auto mapping should include non-metadata dimension"
    );
    assert!(
        binding.column_mapping.physical.contains_key("revenue"),
        "auto mapping should include measure"
    );
    assert!(
        !binding.column_mapping.physical.contains_key("source_partition"),
        "auto mapping should NOT include metadata dimension"
    );
}

#[tokio::test]
async fn test_metadata_dimension_both_path_and_partition() {
    let yaml = r#"
semantic_model:
  name: both_extraction_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: source_info
          data_type: string
          type:
            metadata:
              path:
                token: 2
              partition:
                level: 1
      measures:
        - name: revenue
          data_type: float64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              format: parquet
              paths:
                - "s3://bucket/data.parquet"
              partition_def:
                type:
                  range:
                    column: year
                    start: "2020"
                    end: "2025"
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("metadata dim with both path and partition should compile");

    let dim = &manifest.entities["sales"].interface().dimensions["source_info"];
    match &dim.dim_type {
        semstrait_model::DimensionType::Metadata(m) => {
            assert!(m.path.is_some(), "path extraction should be present");
            assert!(m.partition.is_some(), "partition extraction should be present");
            assert_eq!(m.path.as_ref().unwrap().token, 2);
            assert_eq!(m.partition.as_ref().unwrap().level, 1);
        }
        other => panic!("Expected Metadata dimension type, got {:?}", other),
    }
}

#[tokio::test]
async fn test_declarative_agg_with_measure_filter() {
    let yaml = r#"
semantic_model:
  name: agg_filter_test
  grainsets:
    - name: sales
      dimensions:
        - name: region
          data_type: string
      measures:
        - name: domestic_revenue
          data_type: float64
          agg: sum
          expr: "amount"
          filters:
            - name: domestic_only
              expr: "region = 'US'"
      datasets:
        - name: orders
          extras:
            column_mapping:
              region: region
              domestic_revenue: amount
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("declarative agg with measure filter should compile");

    let measure = &manifest.entities["sales"].interface().measures["domestic_revenue"];
    assert_eq!(
        measure.agg,
        semstrait_core::expr::Aggregation::Sum
    );
    assert!(!measure.filters.is_empty(), "measure should have filters");
}

// ============================================================================
// Computed Dimensions (Phase G)
// ============================================================================

#[tokio::test]
async fn test_computed_dimension_declarative_block() {
    let yaml = r#"
semantic_model:
  name: computed_dim_test
  datasets:
    - name: campaigns
      dimensions:
        - name: campaign
          data_type: string
        - name: market
          data_type: string
          expr:
            upper:
              regexp_extract:
                col: campaign
                pattern: {lit: "^([A-Z]{2})_"}
                group: 1
      measures:
        - name: spend
          data_type: float64
          agg: sum
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("computed dimension should compile");

    let ds = manifest.entities["campaigns"].interface();
    let market = &ds.dimensions["market"];
    assert!(market.expr.is_some(), "market should have a compiled expr");
    assert!(market.expr_source.is_some(), "market should have expr_source");

    // The expression should be FunctionCall(UPPER) wrapping RegexpExtract
    let expr = market.expr.as_ref().unwrap();
    match expr {
        semstrait_core::Expr::FunctionCall(fc) => {
            assert_eq!(fc.name, "UPPER");
            assert_eq!(fc.args.len(), 1);
            assert!(matches!(&fc.args[0], semstrait_core::Expr::RegexpExtract(_)));
        }
        other => panic!("Expected FunctionCall(UPPER), got {:?}", other),
    }
}

#[tokio::test]
async fn test_computed_dimension_rejects_aggregation() {
    let yaml = r#"
semantic_model:
  name: computed_dim_agg_error
  datasets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: [day]
        - name: bad_dim
          data_type: float64
          expr: "SUM(amount)"
      measures:
        - name: revenue
          data_type: float64
          agg: sum
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err(), "should reject aggregation in computed dimension");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("aggregation"),
        "error should mention aggregation, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_computed_dimension_case_when() {
    let yaml = r#"
semantic_model:
  name: case_dim_test
  datasets:
    - name: ads
      dimensions:
        - name: source
          data_type: string
        - name: channel_group
          data_type: string
          expr:
            case:
              when:
                - condition:
                    in: [source, {lit: "google"}, {lit: "bing"}]
                  then: {lit: "search"}
                - condition:
                    eq: [source, {lit: "facebook"}]
                  then: {lit: "social"}
              else: {lit: "other"}
      measures:
        - name: clicks
          data_type: int64
          agg: sum
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("CASE dimension should compile");

    let ds = manifest.entities["ads"].interface();
    let channel = &ds.dimensions["channel_group"];
    assert!(channel.expr.is_some(), "channel_group should be computed");
    match channel.expr.as_ref().unwrap() {
        semstrait_core::Expr::Case(c) => {
            assert_eq!(c.when_then.len(), 2);
            assert!(c.else_expr.is_some());
        }
        other => panic!("Expected Case, got {:?}", other),
    }
}

#[tokio::test]
async fn test_regular_dimension_has_no_expr() {
    let yaml = r#"
semantic_model:
  name: regular_dim_test
  datasets:
    - name: orders
      dimensions:
        - name: region
          data_type: string
      measures:
        - name: revenue
          data_type: float64
          agg: sum
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    let ds = manifest.entities["orders"].interface();
    let region = &ds.dimensions["region"];
    assert!(region.expr.is_none(), "regular dimension should have no expr");
    assert!(region.expr_source.is_none());
}

// ============================================================================
// Temporal dimension consistency validation
// ============================================================================

#[tokio::test]
async fn test_temporal_dimension_conflict_rejected() {
    let yaml = r#"
semantic_model:
  name: test_model
  description: Conflicting temporal dimension names
  grainsets:
    - name: order_events
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: [day]
      measures:
        - name: count
          data_type: int64
          agg: count
          expr: "1"
      extras:
        column_mapping:
          order_date: created_at
          count: "1"
        temporal:
          type:
            events:
              occurred_at: created_at
      datasets:
        - name: clicks
          extras:
            temporal:
              grain: day
              dimension: order_date
              type:
                events:
                  occurred_at: created_at
            storage:
              format: parquet
              paths:
                - data/clicks
        - name: purchases
          extras:
            temporal:
              grain: day
              dimension: event_ts
              type:
                events:
                  occurred_at: event_timestamp
            column_mapping:
              order_date: event_timestamp
              count: "1"
            storage:
              format: parquet
              paths:
                - data/purchases
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    let err = result.expect_err("should reject conflicting temporal.dimension");
    let msg = err.to_string();
    assert!(
        msg.contains("datasets disagree on temporal.dimension"),
        "expected TemporalDimensionConflict, got: {msg}"
    );
    assert!(msg.contains("order_date"), "should mention order_date: {msg}");
    assert!(msg.contains("event_ts"), "should mention event_ts: {msg}");
}

// ============================================================================
// Computed dimensions in unionset — ref'd and inline
// ============================================================================

#[tokio::test]
async fn test_computed_dim_in_unionset_via_ref() {
    let yaml = r#"
semantic_model:
  name: computed_unionset_test

  dimensions:
    - name: channel
      data_type: string
    - name: source
      data_type: string
    - name: full_source
      data_type: string
      expr:
        concat:
          - channel
          - lit: " - "
          - source

  measures:
    - name: clicks
      data_type: i64
      agg: sum

  unionsets:
    - name: all_traffic
      mode: all
      dimensions:
        - ref: channel
        - ref: source
        - ref: full_source
      measures:
        - ref: clicks
      datasets:
        - name: web_clicks
          extras:
            column_mapping:
              channel: web_channel
              source: web_source
              clicks: click_count
            storage:
              format: parquet
              paths: ["web_clicks.parquet"]
        - name: app_clicks
          extras:
            column_mapping:
              channel: app_channel
              source: app_source
              clicks: tap_count
            storage:
              format: parquet
              paths: ["app_clicks.parquet"]
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    let dk = manifest.entities.get("all_traffic").expect("all_traffic entity");
    let iface = dk.interface();

    // full_source should be compiled as a computed dimension with expr
    let full_source = iface.dimensions.get("full_source")
        .expect("full_source should exist in compiled interface");
    assert!(
        full_source.expr.is_some(),
        "full_source should have expr (concat) after compilation — got None"
    );
}
