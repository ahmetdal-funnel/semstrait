//! Integration tests for semstrait-manifest.

use semstrait_manifest::{
    CompileSource, CompiledManifest, InMemoryRepository, ManifestCompiler, Repository,
};

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

    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.model_name, "test_model");
    assert_eq!(
        manifest.model_description,
        Some("A test model".to_string())
    );
    assert_eq!(manifest.datasets.len(), 1);
    assert!(manifest.datasets.contains_key("orders"));

    let orders = &manifest.datasets["orders"];
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
  kinds:
    - name: sales
      type:
        grainset:
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
              path: warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    assert_eq!(manifest.kinds.len(), 1);
    let kind = &manifest.kinds["sales"];
    assert_eq!(kind.name, "sales");
    assert_eq!(kind.datasets.len(), 1);
    assert_eq!(kind.datasets[0].name, "orders_daily");
    assert!(kind.datasets[0].extras.column_mapping.contains_key("order_date"));
    assert!(kind.datasets[0].extras.column_mapping.contains_key("revenue"));
}

// ============================================================================
// Auto column mapping
// ============================================================================

#[tokio::test]
async fn test_auto_column_mapping_expansion() {
    let yaml = r#"
semantic_model:
  name: auto_test
  kinds:
    - name: orders
      type:
        grainset:
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
              path: warehouse.orders_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation with auto mapping should succeed");

    let kind = &manifest.kinds["orders"];
    let ds = &kind.datasets[0];

    // Auto should have expanded to identity mappings for all interface names.
    assert!(ds.extras.column_mapping.contains_key("order_date"));
    assert!(ds.extras.column_mapping.contains_key("region"));
    assert!(ds.extras.column_mapping.contains_key("revenue"));

    // Each mapping should be identity (name → name).
    use semstrait_manifest::ColumnMappingValue;
    match ds.extras.column_mapping.get("revenue").unwrap() {
        ColumnMappingValue::Simple(s) => assert_eq!(s, "revenue"),
        _ => panic!("expected Simple mapping"),
    }
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
    assert!(msg.contains("duplicate dataset name"), "got: {}", msg);
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
  kinds:
    - name: sales
      type:
        grainset:
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
              path: warehouse.orders
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
  kinds:
    - name: order_details
      type:
        joinset:
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
              path: warehouse.orders
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
  kinds:
    - name: sales
      type:
        grainset:
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
              path: warehouse.orders
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("no cycle, should compile");

    let kind = &manifest.kinds["sales"];

    // profit references measures only -> depth 1
    assert_eq!(kind.metrics["profit"].depth, 1);
    // margin references profit (a metric) and revenue (a measure)
    // profit has depth 1, so margin = max(1, 0) + 1 = 2
    assert_eq!(kind.metrics["margin"].depth, 2);
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
      domain: financial.transactions
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
    assert_eq!(deserialized.datasets.len(), manifest.datasets.len());
    assert_eq!(
        deserialized.relationships.len(),
        manifest.relationships.len()
    );

    let orig_orders = &manifest.datasets["orders"];
    let rt_orders = &deserialized.datasets["orders"];
    assert_eq!(orig_orders.dimensions.len(), rt_orders.dimensions.len());
    assert_eq!(orig_orders.measures.len(), rt_orders.measures.len());
    assert_eq!(orig_orders.metrics.len(), rt_orders.metrics.len());
    assert_eq!(orig_orders.domain, rt_orders.domain);

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
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.datasets.len(), 1);
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
// Glob expansion requires catalog
// ============================================================================

#[tokio::test]
async fn test_glob_requires_catalog() {
    let yaml = r#"
semantic_model:
  name: glob_test
  kinds:
    - name: sales
      type:
        grainset:
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
        - name: "orders_*"
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              path: warehouse.orders
"#;

    let compiler = ManifestCompiler::new(); // no catalog
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("requires a catalog"), "got: {}", msg);
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

    let orders = &manifest.datasets["orders"];

    // revenue should be Aggregate(Sum, Column("amount"))
    let revenue = &orders.measures["revenue"];
    assert!(
        matches!(&revenue.expr, semstrait_core::Expr::Aggregate(agg)
            if matches!(agg.function, semstrait_core::Aggregation::Sum)),
        "revenue expr should be Aggregate(Sum), got {:?}",
        revenue.expr
    );
    assert_eq!(revenue.expr_source, "SUM(amount)");

    // order_count should be Aggregate(Count, Column("id"))
    let count = &orders.measures["order_count"];
    assert!(matches!(&count.expr, semstrait_core::Expr::Aggregate(agg)
        if matches!(agg.function, semstrait_core::Aggregation::Count)));

    // avg_amount should be Aggregate(Avg, Column("price"))
    let avg = &orders.measures["avg_amount"];
    assert!(matches!(&avg.expr, semstrait_core::Expr::Aggregate(agg)
        if matches!(agg.function, semstrait_core::Aggregation::Avg)));
}

// ============================================================================
// Kind type compilation
// ============================================================================

#[tokio::test]
async fn test_compiled_kind_types() {
    let yaml = r#"
semantic_model:
  name: kind_type_test
  kinds:
    - name: grain_kind
      type:
        grainset:
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
              path: t1
    - name: union_kind
      type:
        unionset:
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
              path: t2
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("should compile");

    assert!(matches!(
        manifest.kinds["grain_kind"].kind_type,
        semstrait_manifest::CompiledKindType::Grainset
    ));
    assert!(matches!(
        manifest.kinds["union_kind"].kind_type,
        semstrait_manifest::CompiledKindType::Unionset { .. }
    ));
}
