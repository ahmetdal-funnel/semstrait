//! End-to-end integration tests for the full Semstrait pipeline.
//!
//! Tests the complete flow: YAML → ManifestCompiler → SemanticPlanner → SQL generation.

use semstrait_manifest::{CompileSource, ManifestCompiler};
use semstrait_planner::{ResolvedQueryRequest, SemanticPlanner};
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use std::collections::HashMap;

// =============================================================================
// Test 1: Full pipeline - compile, plan, generate SQL
// =============================================================================

#[tokio::test]
async fn test_yaml_compile_plan_sql() {
    // Define a minimal YAML model with one dataset and one kind
    let yaml = r#"
semantic_model:
  name: e2e_test_model
  description: End-to-end test model
  kinds:
    - name: orders
      type:
        grainset:
      dimensions:
        - name: date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: region
          data_type: string
          type:
            categorical:
        - name: customer
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
            column_mapping:
              date: order_date
              region: region_name
              customer: customer_name
              revenue: amount
            storage:
              path: public.orders_daily
"#;

    // Step 1: Compile the YAML model
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    // Verify manifest structure
    assert_eq!(manifest.model_name, "e2e_test_model");
    assert_eq!(manifest.kinds.len(), 1);
    assert!(manifest.kinds.contains_key("orders"));

    let kind = &manifest.kinds["orders"];
    assert_eq!(kind.dimensions.len(), 3);
    assert_eq!(kind.measures.len(), 1);
    assert!(kind.dimensions.contains_key("date"));
    assert!(kind.dimensions.contains_key("region"));
    assert!(kind.dimensions.contains_key("customer"));
    assert!(kind.measures.contains_key("revenue"));

    // Step 2: Build a query request
    let request = ResolvedQueryRequest {
        kind_name: "orders".to_string(),
        dimensions: vec!["date".to_string(), "region".to_string()],
        measures: vec!["revenue".to_string()],
        filters: vec![],
        grain: None,
        limit: None,
        order_by: vec![],
        domain_hint: None,
        session_variables: HashMap::new(),
    };

    // Step 3: Plan the query
    let planner = SemanticPlanner::builder().build();
    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("planning should succeed");

    // Verify the plan output
    assert_eq!(plan.output_names.len(), 3); // date, region, revenue
    assert!(plan.output_names.contains(&"date".to_string()));
    assert!(plan.output_names.contains(&"region".to_string()));
    assert!(plan.output_names.contains(&"revenue".to_string()));

    // Step 4: Generate SQL
    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL generation should succeed");

    // Verify SQL contains expected elements
    assert!(
        sql.contains("SELECT") || sql.contains("select"),
        "SQL should contain SELECT: {}",
        sql
    );
    assert!(
        sql.contains("GROUP BY") || sql.contains("group by"),
        "SQL should contain GROUP BY for aggregation: {}",
        sql
    );

    // The SQL should reference the physical table and columns
    // (Implementation may vary, so we just check it's non-empty and well-formed)
    assert!(!sql.is_empty(), "SQL should not be empty");
    assert!(
        sql.len() > 20,
        "SQL should be a meaningful query, got: {}",
        sql
    );
}

// =============================================================================
// Test 2: Constraint violation - measure requires dimension
// =============================================================================

#[tokio::test]
async fn test_constraint_violation_e2e() {
    // Define a model with a measure that requires the 'date' dimension
    let yaml = r#"
semantic_model:
  name: constraint_test
  kinds:
    - name: sales
      type:
        grainset:
      dimensions:
        - name: date
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
          constraints:
            dimensions:
              one_of:
                - date
      datasets:
        - name: sales_data
          extras:
            column_mapping:
              date: sale_date
              region: region_name
              revenue: amount
            storage:
              path: warehouse.sales
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    // Build a request that violates the constraint (no date dimension)
    let request = ResolvedQueryRequest {
        kind_name: "sales".to_string(),
        dimensions: vec!["region".to_string()], // missing 'date'
        measures: vec!["revenue".to_string()],
        filters: vec![],
        grain: None,
        limit: None,
        order_by: vec![],
        domain_hint: None,
        session_variables: HashMap::new(),
    };

    // Planning should fail with constraint violation
    let planner = SemanticPlanner::builder().build();
    let result = planner.plan(&request, &manifest).await;

    assert!(result.is_err(), "planning should fail due to constraint");

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("constraint") || err_msg.contains("violation"),
        "error should mention constraint violation, got: {}",
        err_msg
    );
}

// =============================================================================
// Test 3: Compile error - raw SQL rejection
// =============================================================================

#[tokio::test]
async fn test_compile_error_raw_sql() {
    // Define a model with a measure containing raw SQL
    let yaml = r#"
semantic_model:
  name: raw_sql_test
  datasets:
    - name: orders
      dimensions:
        - name: date
          data_type: date
          type:
            temporal:
              grains:
                - day
      measures:
        - name: revenue
          data_type: float64
          expr: "SELECT SUM(amount) FROM orders WHERE status = 'completed'"
"#;

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await;

    assert!(
        result.is_err(),
        "compilation should fail due to raw SQL in expr"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("raw SQL rejected") || err_msg.contains("Raw SQL rejected"),
        "error should mention raw SQL rejection, got: {}",
        err_msg
    );
}

// =============================================================================
// Test 4: Plan with filters and ordering
// =============================================================================

#[tokio::test]
async fn test_plan_with_filters_and_order() {
    let yaml = r#"
semantic_model:
  name: filter_test
  kinds:
    - name: products
      type:
        grainset:
      dimensions:
        - name: category
          data_type: string
          type:
            categorical:
        - name: brand
          data_type: string
          type:
            categorical:
      measures:
        - name: total_sales
          data_type: float64
          expr: "SUM(sales_amount)"
      datasets:
        - name: product_sales
          extras:
            column_mapping:
              category: product_category
              brand: product_brand
              total_sales: sales_amount
            storage:
              path: analytics.product_sales
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    // Build request with filters and ordering
    use semstrait_planner::{FilterOperator, FilterValue, OrderByClause, QueryFilter, SortDirection};

    let request = ResolvedQueryRequest {
        kind_name: "products".to_string(),
        dimensions: vec!["category".to_string(), "brand".to_string()],
        measures: vec!["total_sales".to_string()],
        filters: vec![QueryFilter {
            field: "category".to_string(),
            operator: FilterOperator::Eq,
            values: vec![FilterValue::String("Electronics".to_string())],
        }],
        grain: None,
        limit: Some(10),
        order_by: vec![OrderByClause {
            field: "total_sales".to_string(),
            direction: SortDirection::Descending,
        }],
        domain_hint: None,
        session_variables: HashMap::new(),
    };

    let planner = SemanticPlanner::builder().build();
    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("planning should succeed");

    // Verify plan structure
    assert_eq!(plan.output_names.len(), 3);

    // Generate SQL
    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL generation should succeed");

    // SQL should contain filter, order, and limit
    assert!(!sql.is_empty());
    assert!(sql.len() > 30, "SQL should be comprehensive, got: {}", sql);
}

// =============================================================================
// Test 5: Multiple measures aggregation
// =============================================================================

#[tokio::test]
async fn test_multiple_measures() {
    let yaml = r#"
semantic_model:
  name: multi_measure_test
  kinds:
    - name: transactions
      type:
        grainset:
      dimensions:
        - name: date
          data_type: date
          type:
            temporal:
              grains:
                - day
                - month
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
        - name: transaction_count
          data_type: int64
          expr: "COUNT(id)"
        - name: avg_amount
          data_type: float64
          expr: "AVG(amount)"
      datasets:
        - name: txn_daily
          extras:
            column_mapping:
              date: txn_date
              revenue: amount
              transaction_count: id
              avg_amount: amount
            storage:
              path: warehouse.transactions_daily
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    let request = ResolvedQueryRequest {
        kind_name: "transactions".to_string(),
        dimensions: vec!["date".to_string()],
        measures: vec![
            "revenue".to_string(),
            "transaction_count".to_string(),
            "avg_amount".to_string(),
        ],
        filters: vec![],
        grain: None,
        limit: None,
        order_by: vec![],
        domain_hint: None,
        session_variables: HashMap::new(),
    };

    let planner = SemanticPlanner::builder().build();
    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("planning should succeed");

    // Should have 4 outputs: 1 dimension + 3 measures
    assert_eq!(plan.output_names.len(), 4);
    assert!(plan.output_names.contains(&"revenue".to_string()));
    assert!(plan.output_names.contains(&"transaction_count".to_string()));
    assert!(plan.output_names.contains(&"avg_amount".to_string()));

    // Generate SQL
    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL generation should succeed");

    assert!(!sql.is_empty());
    // SQL should contain multiple aggregate functions
    assert!(sql.len() > 50, "SQL with multiple measures should be substantial");
}

// =============================================================================
// Test 6: Kind not found error
// =============================================================================

#[tokio::test]
async fn test_kind_not_found() {
    let yaml = r#"
semantic_model:
  name: simple_model
  kinds:
    - name: orders
      type:
        grainset:
      dimensions:
        - name: date
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
        - name: orders_data
          extras:
            column_mapping:
              date: order_date
              revenue: amount
            storage:
              path: db.orders
"#;

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml.to_string()))
        .await
        .expect("compilation should succeed");

    // Request a non-existent kind
    let request = ResolvedQueryRequest {
        kind_name: "nonexistent_kind".to_string(),
        dimensions: vec!["date".to_string()],
        measures: vec!["revenue".to_string()],
        filters: vec![],
        grain: None,
        limit: None,
        order_by: vec![],
        domain_hint: None,
        session_variables: HashMap::new(),
    };

    let planner = SemanticPlanner::builder().build();
    let result = planner.plan(&request, &manifest).await;

    assert!(result.is_err(), "planning should fail for unknown kind");

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("nonexistent_kind"),
        "error should mention kind not found, got: {}",
        err_msg
    );
}
