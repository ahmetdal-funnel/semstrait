//! End-to-end integration tests for the full Semstrait pipeline.
//!
//! Tests the complete flow: YAML → ManifestCompiler → SemanticPlanner → SQL generation.
//! Model definitions are loaded from `tests/fixtures/models/`.

mod test_helpers;

use semstrait_api::types::RawQueryRequest;
use semstrait_api::SemstraitEngine;
use semstrait_manifest::{CompileSource, ManifestCompiler};
use semstrait_planner::{ResolvedQueryRequest, SemanticPlanner};
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};
use std::collections::HashMap;
use test_helpers::load_model;

// =============================================================================
// Test 1: Full pipeline - compile, plan, generate SQL
// =============================================================================

#[tokio::test]
async fn test_yaml_compile_plan_sql() {
    let yaml = load_model("orders_3dim");

    // Step 1: Compile the YAML model
    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml))
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
    let yaml = load_model("sales_constrained");

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml))
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
    let result = planner.plan(&request, &manifest);

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
    let yaml = load_model("raw_sql_invalid");

    let compiler = ManifestCompiler::new();
    let result = compiler
        .compile(CompileSource::Yaml(yaml))
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
    let yaml = load_model("products");

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml))
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
    let yaml = load_model("transactions_multi_measure");

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml))
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
    let yaml = load_model("orders_simple");

    let compiler = ManifestCompiler::new();
    let manifest = compiler
        .compile(CompileSource::Yaml(yaml))
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
    let result = planner.plan(&request, &manifest);

    assert!(result.is_err(), "planning should fail for unknown kind");

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("nonexistent_kind"),
        "error should mention kind not found, got: {}",
        err_msg
    );
}

// =============================================================================
// Test 7: Explain includes Substrait JSON
// =============================================================================

#[tokio::test]
async fn test_explain_includes_substrait() {
    let yaml = load_model("orders_with_metrics");

    let engine = SemstraitEngine::with_manifest_yaml(&yaml)
        .await
        .expect("engine should compile manifest");

    let raw = RawQueryRequest {
        kind: "orders".to_string(),
        dimensions: vec!["date".to_string(), "region".to_string()],
        measures: vec!["revenue".to_string()],
        ..Default::default()
    };

    let result = engine.explain(&raw).await.expect("explain should succeed");

    assert!(result.sql.is_some(), "should have SQL");
    assert!(
        result.substrait_json.is_some(),
        "should have Substrait JSON"
    );

    let substrait = result.substrait_json.unwrap();
    // Substrait JSON should be valid JSON containing plan structure
    let parsed: serde_json::Value =
        serde_json::from_str(&substrait).expect("Substrait output should be valid JSON");
    assert!(
        parsed.is_object(),
        "Substrait JSON should be an object"
    );
}

// =============================================================================
// Test 8: DataFusion query execution (feature-gated)
// =============================================================================

#[cfg(feature = "datafusion")]
mod datafusion_tests {
    use super::*;
    use semstrait_connectors::datafusion::DataFusionConnector;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_query_via_datafusion() {
        let yaml = load_model("orders_datafusion");

        let compiler = ManifestCompiler::new();
        let compiled = compiler
            .compile(CompileSource::Yaml(yaml))
            .await
            .expect("compilation should succeed");

        // Create DataFusion connector and register CSV as the dataset table.
        let connector = DataFusionConnector::new();
        let csv_path = format!(
            "{}/tests/fixtures/data/orders.csv",
            env!("CARGO_MANIFEST_DIR")
        );
        // The planner resolves to dataset name "orders_data" as table name.
        connector
            .register_csv("orders_data", &csv_path)
            .await
            .expect("CSV registration should succeed");

        let engine = SemstraitEngine::with_connector(compiled, Arc::new(connector));

        let raw = RawQueryRequest {
            kind: "orders".to_string(),
            dimensions: vec!["region".to_string()],
            measures: vec!["revenue".to_string()],
            ..Default::default()
        };

        let result = engine.query(&raw).await.expect("query should succeed");

        // Should return JSON with rows and stats
        assert!(result.is_object(), "result should be a JSON object");
        let stats = result.get("stats").expect("should have stats field");
        let rows_returned = stats.get("rows_returned").expect("should have rows_returned");
        assert_eq!(rows_returned, 2, "should have 2 rows (US, EU)");

        // Verify actual row data is present (not just stats)
        let rows = result.get("rows").expect("should have rows field");
        let rows_array = rows.as_array().expect("rows should be an array");
        assert_eq!(rows_array.len(), 2, "should have 2 row objects");
        assert!(
            rows_array[0].is_object(),
            "each row should be a JSON object with column values"
        );
    }
}
