//! Integration tests for cross-grain-set queries
//!
//! Tests that cross-grain-set metrics (using _dataset.path conditions)
//! generate correct UNION plans.

mod common;

use common::{has_union, load_fixture};
use semstrait::{parser, planner::plan_cross_grain_set_query};

#[test]
fn test_cross_grain_set_metric_detection() {
    let schema = load_fixture("cross_grain_set.yaml");
    let model = schema.get_model("marketing").unwrap();

    // Get the unified_cost metric
    let metric = model.get_metric("unified_cost").unwrap();

    // Should be detected as cross-grain-set
    assert!(
        metric.is_cross_grain_set(),
        "unified_cost should be detected as cross-grain-set metric"
    );

    // Should have mappings for both grain sets
    let mappings = metric.grain_set_measures();
    assert_eq!(mappings.len(), 2, "Should have 2 grain set mappings");

    assert!(
        mappings.iter().any(|(tg, m)| tg == "google_ads" && m == "ad_cost"),
        "Should map google_ads → ad_cost"
    );
    assert!(
        mappings.iter().any(|(tg, m)| tg == "meta_ads" && m == "media_spend"),
        "Should map meta_ads → media_spend"
    );
}

#[test]
fn test_cross_grain_set_match_metric() {
    // Metric using match (glob) returns empty grain_set_measures() but is still cross-grain-set.
    // Planner expands via first-match-wins to get (google_ads, ad_cost), (meta_ads, media_spend).
    let schema = load_fixture("cross_grain_set.yaml");
    let model = schema.get_model("marketing").unwrap();
    let metric = model.get_metric("unified_cost_glob").unwrap();

    assert!(
        metric.is_cross_grain_set(),
        "unified_cost_glob should be detected as cross-grain-set (match conditions)"
    );
    assert!(
        metric.grain_set_measures().is_empty(),
        "match-based metric should have empty grain_set_measures (expansion in planner)"
    );

    let plan_node = plan_cross_grain_set_query(
        &schema,
        model,
        metric,
        &["dates.year".to_string()],
    )
    .expect("Cross-grain-set planning with match should succeed");

    let substrait = semstrait::emit_plan(&plan_node, None).expect("Emission should succeed");
    assert!(
        has_union(&substrait),
        "Match-based cross-grain-set query should produce a UNION plan"
    );
}

#[test]
fn test_cross_grain_set_union_plan() {
    let schema = load_fixture("cross_grain_set.yaml");
    let model = schema.get_model("marketing").unwrap();
    let metric = model.get_metric("unified_cost").unwrap();

    // Plan a cross-grain-set query
    let plan_node = plan_cross_grain_set_query(
        &schema,
        model,
        metric,
        &["dates.year".to_string()],
    )
    .expect("Cross-grain-set planning should succeed");

    // Convert to Substrait to verify structure
    let substrait = semstrait::emit_plan(&plan_node, None).expect("Emission should succeed");

    // Should contain a UNION
    assert!(
        has_union(&substrait),
        "Cross-grain-set query should produce a UNION plan"
    );
}

#[test]
fn test_single_grain_set_metric_not_cross() {
    // Verify that normal metrics are NOT detected as cross-grain-set
    let schema = parser::parse_file("test_data/steelwheels.yaml").unwrap();
    let model = schema.get_model("steelwheels").unwrap();

    if let Some(metric) = model.get_metric("avg_unit_price") {
        assert!(
            !metric.is_cross_grain_set(),
            "Normal metric should NOT be cross-grain-set"
        );
    }
}

// =============================================================================
// Model-Level Dimension Tests
// =============================================================================

#[test]
fn test_conformed_dimension_detection() {
    let schema = load_fixture("cross_grain_set.yaml");
    let model = schema.get_model("marketing").unwrap();

    // dates is at model level - all attributes are conformed
    assert!(model.is_conformed("dates", "year"), "dates.year should be conformed (model-level)");
    assert!(model.is_conformed("dates", "date"), "dates.date should be conformed (model-level)");
    
    // _dataset is at model level (virtual) - all attributes are conformed
    assert!(model.is_conformed("_dataset", "path"), "_dataset.path should be conformed (virtual)");
    
    // campaign is NOT at model level (inline only) - NOT conformed
    assert!(!model.is_conformed("campaign", "campaign_id"), "campaign.campaign_id should NOT be conformed (inline only)");
    assert!(!model.is_conformed("campaign", "campaign_name"), "campaign.campaign_name should NOT be conformed (inline only)");
    
    // Non-existent dimensions are not conformed
    assert!(!model.is_conformed("other", "attr"), "non-existent dimension should NOT be conformed");
}

#[test]
fn test_conformed_query_detection() {
    let schema = load_fixture("cross_grain_set.yaml");
    let model = schema.get_model("marketing").unwrap();

    // Query with only model-level dimensions
    let conformed_query = vec!["dates.year".to_string(), "_dataset.path".to_string()];
    assert!(model.is_conformed_query(&conformed_query), "Query with dates.year and _dataset should be conformed");
    
    // Query with inline-only dimension (not at model level)
    let non_conformed_query = vec!["campaign.campaign_name".to_string()];
    assert!(!model.is_conformed_query(&non_conformed_query), "Query with inline dimension should NOT be conformed");
    
    // Query with mix of model-level and inline dimensions
    let mixed_query = vec!["dates.year".to_string(), "campaign.campaign_name".to_string()];
    assert!(!model.is_conformed_query(&mixed_query), "Mixed query should NOT be conformed");
}

#[test]
fn test_conformed_dimension_union_plan() {
    use common::run_pipeline;
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");

    // Query conformed dimension with a metric that exists in both grain sets
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec!["dates.year".to_string()]),
        metrics: Some(vec!["clicks".to_string()]),
        ..Default::default()
    };

    let plan = run_pipeline(&schema, &request)
        .expect("Conformed dimension query should succeed");
    
    // Should produce a UNION plan (querying across both grain sets)
    assert!(
        has_union(&plan),
        "Conformed dimension query should produce a UNION plan"
    );
}

#[test]
fn test_conformed_dimension_with_table_metadata() {
    use common::run_pipeline;
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");

    // Query conformed dimension + _dataset.path + metric
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "dates.year".to_string(),
            "_dataset.path".to_string(),
        ]),
        metrics: Some(vec!["clicks".to_string()]),
        ..Default::default()
    };

    let plan = run_pipeline(&schema, &request)
        .expect("Conformed dimension + _dataset query should succeed");
    
    // Should produce a UNION plan
    assert!(
        has_union(&plan),
        "Conformed dimension + _dataset query should produce a UNION plan"
    );
}

#[test]
fn test_virtual_dimension_implicitly_conformed() {
    use common::run_pipeline;
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");

    // Query ONLY _dataset.path (virtual dimension) + metric
    // Virtual dimensions should be implicitly conformed
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "_dataset.path".to_string(),
        ]),
        metrics: Some(vec!["clicks".to_string()]),
        ..Default::default()
    };

    let plan = run_pipeline(&schema, &request)
        .expect("Virtual dimension only query should succeed (implicitly conformed)");
    
    // Should produce a UNION plan (querying across both grain sets)
    assert!(
        has_union(&plan),
        "Virtual dimension query should produce a UNION plan"
    );
}

#[test]
fn test_virtual_only_query_no_table_scan() {
    use common::run_pipeline;
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");

    // Query ONLY _dataset.path (virtual dimension) with NO metrics
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "_dataset.path".to_string(),
        ]),
        metrics: None,
        ..Default::default()
    };

    let plan = run_pipeline(&schema, &request)
        .expect("Virtual-only query should succeed without table scans");
    
    // Should NOT produce a UNION - should be a VirtualTable
    assert!(
        !has_union(&plan),
        "Virtual-only query should NOT produce a UNION plan"
    );
}

// =============================================================================
// Grain-set-qualified dimension tests
// =============================================================================

#[test]
fn test_qualified_dimension_parsing() {
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");
    
    // Query with grain-set-qualified dimension
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "google_ads.dates.year".to_string(),
        ]),
        metrics: Some(vec!["unified_cost".to_string()]),
        ..Default::default()
    };

    let result = common::run_pipeline(&schema, &request);
    assert!(
        result.is_ok(),
        "Grain-set-qualified dimension query should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_qualified_dimension_cross_grain_set_metric() {
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");
    
    // Query with grain-set-qualified dimensions from BOTH grain sets
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "google_ads.dates.year".to_string(),
            "meta_ads.dates.year".to_string(),
        ]),
        metrics: Some(vec!["unified_cost".to_string()]),
        ..Default::default()
    };

    let result = common::run_pipeline(&schema, &request);
    assert!(
        result.is_ok(),
        "Query with qualified dimensions from both grain sets should succeed: {:?}",
        result.err()
    );
    
    let plan = result.unwrap();
    
    // Should produce a UNION plan
    assert!(
        has_union(&plan),
        "Query with qualified dimensions should produce a UNION plan"
    );
}

#[test]
fn test_qualified_with_virtual_dimension() {
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");
    
    // Query with grain-set-qualified dimension + virtual _dataset dimension
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "google_ads.dates.year".to_string(),
            "_dataset.path".to_string(),
        ]),
        metrics: Some(vec!["unified_cost".to_string()]),
        ..Default::default()
    };

    let result = common::run_pipeline(&schema, &request);
    assert!(
        result.is_ok(),
        "Query with qualified + virtual dimensions should succeed: {:?}",
        result.err()
    );
    
    let plan = result.unwrap();
    assert!(
        has_union(&plan),
        "Query with qualified + virtual dimensions should produce a UNION plan"
    );
}

#[test]
fn test_invalid_grain_set_qualifier_fails() {
    use semstrait::QueryRequest;
    
    let schema = load_fixture("cross_grain_set.yaml");
    
    // Query with non-existent grain set qualifier should fail
    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "nonexistent_tg.dates.year".to_string(),
        ]),
        metrics: Some(vec!["unified_cost".to_string()]),
        ..Default::default()
    };

    let result = common::run_pipeline(&schema, &request);
    assert!(
        result.is_err(),
        "Query with invalid grain set qualifier should fail"
    );
}

/// Group-qualified path: "facebookads" is a union group with two leaf grain sets.
/// Querying "facebookads.campaign.campaign" should succeed and UNION both leaves.
#[test]
fn test_group_qualified_dimension_path() {
    use semstrait::QueryRequest;

    let schema = load_fixture("group_qualified_path.yaml");

    let request = QueryRequest {
        model: "marketing".to_string(),
        rows: Some(vec![
            "campaign.campaign".to_string(),
            "facebookads.campaign.campaign".to_string(),
        ]),
        metrics: Some(vec!["clicks".to_string(), "impressions".to_string()]),
        ..Default::default()
    };

    let result = common::run_pipeline(&schema, &request);
    assert!(
        result.is_ok(),
        "Group-qualified dimension facebookads.campaign.campaign should succeed: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    assert!(
        has_union(&plan),
        "Query with group-qualified dimension should produce a UNION plan"
    );
}

// ============================================================================
// Multiple cross-grain-set metrics tests
// ============================================================================

#[test]
fn test_multiple_cross_grain_set_metrics_detection() {
    let schema = parser::parse_file("test_data/marketing.yaml").unwrap();
    let model = schema.get_model("-ObDoDFVQGxxCGa5vw_Z").unwrap();

    // Both metrics should be detected as cross-grain-set
    let cost_metric = model.get_metric("fun-cost").unwrap();
    let impressions_metric = model.get_metric("fun-impressions").unwrap();

    assert!(cost_metric.is_cross_grain_set(), "fun-cost should be cross-grain-set");
    assert!(impressions_metric.is_cross_grain_set(), "fun-impressions should be cross-grain-set");
}

#[test]
fn test_multiple_cross_grain_set_metrics_planning() {
    use semstrait::planner::plan_semantic_query;
    use semstrait::query::QueryRequest;

    let schema = parser::parse_file("test_data/marketing.yaml").unwrap();
    let model = schema.get_model("-ObDoDFVQGxxCGa5vw_Z").unwrap();

    // Query with BOTH cross-grain-set metrics
    let request = QueryRequest {
        model: "-ObDoDFVQGxxCGa5vw_Z".to_string(),
        dimensions: None,
        rows: Some(vec!["dates.date".to_string()]),
        columns: None,
        metrics: Some(vec!["fun-cost".to_string(), "fun-impressions".to_string()]),
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request);
    assert!(plan.is_ok(), "Multiple cross-grain-set metrics should be supported: {:?}", plan.err());

    let plan_node = plan.unwrap();
    let substrait = semstrait::emit_plan(&plan_node, None).expect("Emission should succeed");
    
    assert!(has_union(&substrait), "Multiple cross-grain-set metrics should produce UNION plan");
}

#[test]
fn test_multiple_cross_grain_set_metrics_union_structure() {
    use semstrait::planner::plan_semantic_query;
    use semstrait::query::QueryRequest;
    use semstrait::plan::PlanNode;

    let schema = parser::parse_file("test_data/marketing.yaml").unwrap();
    let model = schema.get_model("-ObDoDFVQGxxCGa5vw_Z").unwrap();

    let request = QueryRequest {
        model: "-ObDoDFVQGxxCGa5vw_Z".to_string(),
        dimensions: None,
        rows: Some(vec!["dates.date".to_string()]),
        columns: None,
        metrics: Some(vec!["fun-cost".to_string(), "fun-impressions".to_string()]),
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request).unwrap();

    // The plan should be: Sort(Aggregate(Union([branch1, branch2])))
    match plan {
        PlanNode::Sort(sort) => {
            match *sort.input {
                PlanNode::Aggregate(agg) => {
                    assert_eq!(agg.aggregates.len(), 2, "Should have 2 aggregates (one per metric)");
                    
                    let aliases: Vec<&str> = agg.aggregates.iter()
                        .map(|a| a.alias.as_str())
                        .collect();
                    assert!(aliases.contains(&"fun-cost"), "Should have fun-cost aggregate");
                    assert!(aliases.contains(&"fun-impressions"), "Should have fun-impressions aggregate");
                    
                    match *agg.input {
                        PlanNode::Union(union) => {
                            assert_eq!(union.inputs.len(), 2, "Union should have 2 branches");
                        }
                        _ => panic!("Expected Union as input to Aggregate"),
                    }
                }
                _ => panic!("Expected Aggregate inside Sort"),
            }
        }
        _ => panic!("Expected Sort at top level"),
    }
}

#[test]
fn test_single_cross_grain_set_metric_still_works() {
    use semstrait::planner::plan_semantic_query;
    use semstrait::query::QueryRequest;

    let schema = parser::parse_file("test_data/marketing.yaml").unwrap();
    let model = schema.get_model("-ObDoDFVQGxxCGa5vw_Z").unwrap();

    let request = QueryRequest {
        model: "-ObDoDFVQGxxCGa5vw_Z".to_string(),
        dimensions: None,
        rows: Some(vec!["dates.date".to_string()]),
        columns: None,
        metrics: Some(vec!["fun-cost".to_string()]),
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request);
    assert!(plan.is_ok(), "Single cross-grain-set metric should work: {:?}", plan.err());
}
