//! End-to-end pipeline tests: YAML → ManifestCompiler → SemanticPlanner → SQL.
//!
//! Exercises the full stack through the facade crate's dependencies.

use std::collections::HashMap;

use semstrait_manifest::{CompileSource, ManifestCompiler};
use semstrait_planner::{
    FilterOperator, FilterValue, OrderByClause, QueryFilter, ResolvedQueryRequest,
    SemanticPlanner, SortDirection,
};
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};

/// Minimal grainset YAML model for testing.
const GRAINSET_YAML: &str = r#"
semantic_model:
  name: e2e_test
  description: E2E pipeline test model
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
            categorical: {}
        - name: customer
          data_type: string
          type:
            categorical: {}
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
        - name: order_count
          data_type: int64
          expr: "COUNT(order_id)"
      metrics:
        - name: avg_order_value
          data_type: float64
          expr: "revenue / order_count"
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              region: region_name
              customer: customer_name
              revenue: amount
              order_count: order_id
            storage:
              path: warehouse.orders_daily
"#;

/// YAML model with measure constraints.
const CONSTRAINED_YAML: &str = r#"
semantic_model:
  name: constrained_test
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
            categorical: {}
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
          constraints:
            dimensions:
              one_of:
                - order_date
      datasets:
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              region: region_name
              revenue: amount
            storage:
              path: warehouse.orders_daily
"#;

fn make_request(
    kind: &str,
    dims: &[&str],
    measures: &[&str],
) -> ResolvedQueryRequest {
    ResolvedQueryRequest {
        kind_name: kind.to_string(),
        dimensions: dims.iter().map(|s| s.to_string()).collect(),
        measures: measures.iter().map(|s| s.to_string()).collect(),
        filters: vec![],
        grain: None,
        limit: None,
        order_by: vec![],
        domain_hint: None,
        session_variables: HashMap::new(),
    }
}

// ============================================================================
// Full pipeline: YAML → compile → plan → SQL
// ============================================================================

#[tokio::test]
async fn e2e_compile_plan_sql() {
    // Step 1: Compile YAML → CompiledManifest
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(GRAINSET_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    assert_eq!(manifest.model_name, "e2e_test");
    assert_eq!(manifest.kinds.len(), 1);
    assert!(manifest.kinds.contains_key("orders"));

    // Step 2: Plan a query
    let planner = SemanticPlanner::builder().build();
    let request = make_request("orders", &["order_date", "region"], &["revenue"]);

    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("planning should succeed");

    assert_eq!(plan.output_names, vec!["order_date", "region", "revenue"]);

    // Step 3: Emit SQL
    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL emission should succeed");

    // Verify the SQL contains expected elements
    let sql_upper = sql.to_uppercase();
    assert!(sql_upper.contains("SELECT"), "SQL should contain SELECT: {}", sql);
    assert!(sql_upper.contains("FROM"), "SQL should contain FROM: {}", sql);
    assert!(
        sql_upper.contains("ORDERS_DAILY"),
        "SQL should reference the table: {}",
        sql
    );
}

#[tokio::test]
async fn e2e_compile_plan_sql_with_filters() {
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(GRAINSET_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    let planner = SemanticPlanner::builder().build();
    let mut request = make_request("orders", &["order_date"], &["revenue"]);
    request.filters.push(QueryFilter {
        field: "region".to_string(),
        operator: FilterOperator::Eq,
        values: vec![FilterValue::String("US".to_string())],
    });

    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("planning with filter should succeed");

    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL emission should succeed");

    let sql_upper = sql.to_uppercase();
    assert!(sql_upper.contains("WHERE"), "Filtered SQL should contain WHERE: {}", sql);
}

#[tokio::test]
async fn e2e_compile_plan_sql_with_order_and_limit() {
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(GRAINSET_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    let planner = SemanticPlanner::builder().build();
    let mut request = make_request("orders", &["region"], &["revenue"]);
    request.order_by.push(OrderByClause {
        field: "revenue".to_string(),
        direction: SortDirection::Descending,
    });
    request.limit = Some(10);

    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("planning with order+limit should succeed");

    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL emission should succeed");

    let sql_upper = sql.to_uppercase();
    assert!(
        sql_upper.contains("ORDER BY"),
        "SQL should contain ORDER BY: {}",
        sql
    );
    assert!(
        sql_upper.contains("LIMIT") || sql_upper.contains("FETCH"),
        "SQL should contain LIMIT or FETCH: {}",
        sql
    );
}

// ============================================================================
// Constraint violation through full pipeline
// ============================================================================

#[tokio::test]
async fn e2e_constraint_violation() {
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(CONSTRAINED_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    let planner = SemanticPlanner::builder().build();
    // Request without order_date — violates one_of constraint
    let request = make_request("orders", &["region"], &["revenue"]);

    let result = planner.plan(&request, &manifest).await;
    assert!(result.is_err(), "should fail with constraint violation");

    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("one_of") || msg.contains("constraint"),
        "error should mention constraint: {}",
        msg
    );
}

#[tokio::test]
async fn e2e_constraint_satisfied() {
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(CONSTRAINED_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    let planner = SemanticPlanner::builder().build();
    // Request with order_date — satisfies one_of constraint
    let request = make_request("orders", &["order_date", "region"], &["revenue"]);

    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("should succeed when constraint is satisfied");

    assert_eq!(
        plan.output_names,
        vec!["order_date", "region", "revenue"]
    );
}

// ============================================================================
// Error cases through full pipeline
// ============================================================================

#[tokio::test]
async fn e2e_kind_not_found() {
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(GRAINSET_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    let planner = SemanticPlanner::builder().build();
    let request = make_request("nonexistent_kind", &["date"], &["revenue"]);

    let result = planner.plan(&request, &manifest).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn e2e_raw_sql_rejected_at_compile() {
    let yaml = r#"
semantic_model:
  name: bad_model
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
    assert!(
        msg.contains("raw SQL rejected"),
        "should reject raw SQL: {}",
        msg
    );
}

#[tokio::test]
async fn e2e_multiple_measures() {
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(GRAINSET_YAML.to_string()))
        .await
        .expect("compilation should succeed");

    let planner = SemanticPlanner::builder().build();
    let request = make_request("orders", &["order_date"], &["revenue", "order_count"]);

    let plan = planner
        .plan(&request, &manifest)
        .await
        .expect("multi-measure query should succeed");

    assert_eq!(
        plan.output_names,
        vec!["order_date", "revenue", "order_count"]
    );

    let emitter = AnsiSqlEmitter::new(AnsiDialect);
    let sql = emitter.emit(&plan).expect("SQL emission should succeed");
    assert!(!sql.is_empty(), "SQL should not be empty");
}
