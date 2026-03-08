//! Integration tests for union_set-based "partition" scenario
//!
//! The same logical scenario (e.g. Facebook Ads by account) is modeled with nested union_set:
//! one grain set per slice (facebookads_111, facebookads_222, facebookads_333) instead of
//! one grain set with multiple partitioned datasets. Union plans come from the conformed path.

use semstrait::semantic_model::Schema;
use semstrait::selector::select_datasets;
use semstrait::planner::{plan_semantic_query, plan_cross_grain_set_query};
use semstrait::query::QueryRequest;
use semstrait::plan::PlanNode;

fn load_schema() -> Schema {
    Schema::from_file("test_data/partitioned.yaml").unwrap()
}

#[test]
fn test_selector_returns_single_for_qualified_grain_set() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();
    let grain_sets = model.grain_sets();

    // Qualified to one grain set: should return single dataset
    let datasets = select_datasets(
        &schema,
        model,
        &grain_sets,
        &["facebookads_111.dates.date".to_string()],
        &["spend".to_string()],
    ).unwrap();

    assert_eq!(datasets.len(), 1);
    assert_eq!(datasets[0].group.name, "facebookads_111");
    assert_eq!(datasets[0].dataset.name, "fb-account-111");
}

#[test]
fn test_non_partitioned_group_returns_single() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();

    let grain_sets = model.grain_sets();
    let datasets = select_datasets(
        &schema,
        model,
        &grain_sets,
        &["dates.date".to_string()],
        &["cost".to_string()],
    ).unwrap();

    assert_eq!(datasets.len(), 1);
    assert_eq!(datasets[0].group.name, "adwords");
}

#[test]
fn test_conformed_query_produces_union_plan() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();

    let request = QueryRequest {
        model: "partitioned_ads".to_string(),
        dimensions: None,
        rows: Some(vec!["dates.date".to_string()]),
        columns: None,
        metrics: Some(vec!["spend".to_string()]),
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request).unwrap();

    // Conformed path: 3 grain sets (facebookads_111, _222, _333) have spend -> Union with 3 branches
    match &plan {
        PlanNode::Union(union) => {
            assert_eq!(union.inputs.len(), 3, "Expected 3 union branches for 3 fb grain sets");
        }
        other => panic!("Expected Union plan, got: {:?}", std::mem::discriminant(other)),
    }
}

#[test]
fn test_adwords_query_no_union() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();

    let request = QueryRequest {
        model: "partitioned_ads".to_string(),
        dimensions: None,
        rows: Some(vec!["dates.date".to_string()]),
        columns: None,
        metrics: Some(vec!["cost".to_string()]),
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request).unwrap();

    match &plan {
        PlanNode::Union(_) => panic!("Expected non-Union plan for single adwords grain set"),
        _ => {}
    }
}

#[test]
fn test_grain_set_in_resolver() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();
    let grain_sets = model.grain_sets();

    for gs_name in ["facebookads_111", "facebookads_222", "facebookads_333"] {
        let dim_path = format!("facebookads.{}.dates.date", gs_name);
        let expected_path = format!("facebookads.{}", gs_name);
        let datasets = select_datasets(
            &schema,
            model,
            &grain_sets,
            &[dim_path.clone()],
            &["spend".to_string()],
        ).unwrap();
        assert_eq!(datasets.len(), 1);
        let selected = &datasets[0];
        let request = QueryRequest {
            model: "partitioned_ads".to_string(),
            dimensions: None,
            rows: Some(vec![
                dim_path,
                "_dataset.path".to_string(),
            ]),
            columns: None,
            metrics: Some(vec!["spend".to_string()]),
            filter: None,
        };
        let resolved = semstrait::resolver::resolve_query(&schema, &request, selected, &grain_sets).unwrap();
        let ds_attr = resolved.row_attributes.iter()
            .find(|a| a.dimension_name() == "_dataset" && a.attribute_name() == "path")
            .expect("Should have _dataset.path");
        assert!(ds_attr.is_meta());
        assert_eq!(ds_attr.meta_value(), Some(expected_path.as_str()));
    }
}

#[test]
fn test_cross_grain_set_union() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();

    let metric = model.get_metric("total_clicks").unwrap();

    let plan = plan_cross_grain_set_query(
        &schema,
        model,
        metric,
        &["_dataset.path".to_string()],
    ).unwrap();

    fn count_union_inputs(node: &PlanNode) -> Option<usize> {
        match node {
            PlanNode::Union(u) => Some(u.inputs.len()),
            PlanNode::Sort(s) => count_union_inputs(&s.input),
            PlanNode::Aggregate(a) => count_union_inputs(&a.input),
            PlanNode::Project(p) => count_union_inputs(&p.input),
            _ => None,
        }
    }

    let branch_count = count_union_inputs(&plan).expect("Expected a Union somewhere in the plan");
    assert_eq!(branch_count, 4, "Expected 4 union branches: 3 fb grain sets + 1 adwords");
}

#[test]
fn test_cross_grain_set_literals() {
    use semstrait::plan::{Literal, Expr};

    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();
    let metric = model.get_metric("total_clicks").unwrap();

    let plan = plan_cross_grain_set_query(
        &schema,
        model,
        metric,
        &["_dataset.path".to_string()],
    ).unwrap();

    fn find_union(node: &PlanNode) -> Option<&PlanNode> {
        match node {
            PlanNode::Union(_) => Some(node),
            PlanNode::Sort(s) => find_union(&s.input),
            PlanNode::Aggregate(a) => find_union(&a.input),
            PlanNode::Project(p) => find_union(&p.input),
            _ => None,
        }
    }

    fn extract_dataset_path_literal(node: &PlanNode) -> Option<String> {
        match node {
            PlanNode::Project(p) => {
                for pe in &p.expressions {
                    if pe.alias == "_dataset.path" {
                        match &pe.expr {
                            Expr::Literal(Literal::String(s)) => return Some(s.clone()),
                            Expr::Literal(Literal::Null(_)) => return Some("NULL".to_string()),
                            _ => {}
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    let union_node = find_union(&plan).expect("Should have a Union node");
    if let PlanNode::Union(u) = union_node {
        let mut values: Vec<String> = Vec::new();
        for branch in &u.inputs {
            if let Some(val) = extract_dataset_path_literal(branch) {
                values.push(val);
            }
        }
        assert_eq!(values.len(), 4);
        assert!(values.contains(&"facebookads.facebookads_111".to_string()));
        assert!(values.contains(&"facebookads.facebookads_222".to_string()));
        assert!(values.contains(&"facebookads.facebookads_333".to_string()));
        assert!(values.contains(&"adwords".to_string()));
    }
}

#[test]
fn test_virtual_only_dataset_path_query() {
    use semstrait::plan::LiteralValue;

    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();

    let request = QueryRequest {
        model: "partitioned_ads".to_string(),
        dimensions: None,
        rows: Some(vec!["_dataset.path".to_string()]),
        columns: None,
        metrics: None,
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request).unwrap();

    match &plan {
        PlanNode::VirtualTable(vt) => {
            assert_eq!(vt.rows.len(), 4, "Expected 4 rows (3 fb grain sets + 1 adwords)");
            let values: Vec<String> = vt.rows.iter()
                .map(|row| match &row[0] {
                    LiteralValue::String(s) => s.clone(),
                    LiteralValue::Null => "NULL".to_string(),
                    other => format!("{:?}", other),
                })
                .collect();
            assert!(values.contains(&"facebookads.facebookads_111".to_string()));
            assert!(values.contains(&"facebookads.facebookads_222".to_string()));
            assert!(values.contains(&"facebookads.facebookads_333".to_string()));
            assert!(values.contains(&"adwords".to_string()));
        }
        other => panic!("Expected VirtualTable, got: {:?}", std::mem::discriminant(other)),
    }
}

#[test]
fn test_conformed_dimension_no_metrics() {
    let schema = load_schema();
    let model = schema.get_model("partitioned_ads").unwrap();

    let request = QueryRequest {
        model: "partitioned_ads".to_string(),
        dimensions: None,
        rows: Some(vec!["dates.date".to_string()]),
        columns: None,
        metrics: None,
        filter: None,
    };

    let plan = plan_semantic_query(&schema, model, &request).unwrap();

    fn count_union_inputs(node: &PlanNode) -> Option<usize> {
        match node {
            PlanNode::Union(u) => Some(u.inputs.len()),
            PlanNode::Sort(s) => count_union_inputs(&s.input),
            PlanNode::Aggregate(a) => count_union_inputs(&a.input),
            PlanNode::Project(p) => count_union_inputs(&p.input),
            _ => None,
        }
    }

    let branch_count = count_union_inputs(&plan).expect("Expected a Union in the plan");
    assert_eq!(branch_count, 4, "Expected 4 union branches: 3 fb + 1 adwords");
}
