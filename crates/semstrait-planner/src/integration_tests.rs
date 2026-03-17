//! Integration tests for the full planning pipeline.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use semstrait_ir::{DslExpr, LogicalPlan, PlanNode};

    use crate::error::PlannerError;
    use crate::optimizer::OptimizerPass;
    use crate::planner::SemanticPlanner;
    use crate::request::{
        FilterOperator, FilterValue, OrderByClause, QueryFilter, ResolvedQueryRequest,
        SortDirection,
    };
    use crate::test_helpers::*;

    // ========================================================================
    // Simple grainset planning
    // ========================================================================

    #[tokio::test]
    async fn test_simple_grainset_plan() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();
        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);

        let plan = planner.plan(&request, &manifest).await;
        assert!(plan.is_ok(), "planning should succeed: {:?}", plan.err());

        let plan = plan.unwrap();
        assert_eq!(
            plan.output_names,
            vec!["date".to_string(), "revenue".to_string()]
        );

        // Verify the plan structure: Project -> Aggregate -> Scan
        assert!(
            matches!(&plan.root, PlanNode::Project(_)),
            "root should be Project, got {:?}",
            std::mem::discriminant(&plan.root)
        );

        if let PlanNode::Project(proj) = &plan.root {
            assert!(
                matches!(proj.input.as_ref(), PlanNode::Aggregate(_)),
                "Project input should be Aggregate"
            );
            if let PlanNode::Aggregate(agg) = proj.input.as_ref() {
                assert!(
                    matches!(agg.input.as_ref(), PlanNode::Scan(_)),
                    "Aggregate input should be Scan"
                );

                // Verify GROUP BY has the date dimension.
                assert_eq!(agg.group_by.len(), 1);

                // Verify there is one aggregate measure.
                assert_eq!(agg.aggregates.len(), 1);

                if let PlanNode::Scan(scan) = agg.input.as_ref() {
                    assert_eq!(scan.table_name, "orders_daily");
                }
            }
        }
    }

    #[tokio::test]
    async fn test_grainset_plan_multiple_dims() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();
        let request = make_test_request(
            "orders",
            vec!["date", "region"],
            vec!["revenue"],
        );

        let plan = planner.plan(&request, &manifest).await.unwrap();
        assert_eq!(
            plan.output_names,
            vec!["date".to_string(), "region".to_string(), "revenue".to_string()]
        );
    }

    #[tokio::test]
    async fn test_grainset_plan_kind_not_found() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();
        let request = make_test_request("nonexistent", vec!["date"], vec!["revenue"]);

        let result = planner.plan(&request, &manifest).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::KindNotFound(_)
        ));
    }

    // ========================================================================
    // Constraint evaluation (integration)
    // ========================================================================

    #[tokio::test]
    async fn test_constraint_violation_blocks_planning() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: Some(vec!["date".to_string()]),
                none_of: None,
                all: None,
            }),
            None,
        );

        // Request without date — should fail constraint check.
        let request = make_test_request("orders", vec!["region"], vec!["revenue"]);
        let result = planner.plan(&request, &manifest).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::ConstraintViolation { .. }
        ));
    }

    #[tokio::test]
    async fn test_constraint_satisfied_allows_planning() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest_with_constraints(
            Some(semstrait_manifest::DimensionConstraints {
                one_of: Some(vec!["date".to_string()]),
                none_of: None,
                all: None,
            }),
            None,
        );

        // Request with date — should pass.
        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        let result = planner.plan(&request, &manifest).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // Filter injection
    // ========================================================================

    #[tokio::test]
    async fn test_user_filter_injection() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();

        let request = ResolvedQueryRequest {
            kind_name: "orders".to_string(),
            dimensions: vec!["date".to_string()],
            measures: vec!["revenue".to_string()],
            filters: vec![QueryFilter {
                field: "region".to_string(),
                operator: FilterOperator::Eq,
                values: vec![FilterValue::String("US".to_string())],
            }],
            grain: None,
            limit: None,
            order_by: vec![],
            domain_hint: None,
            session_variables: HashMap::new(),
        };

        let plan = planner.plan(&request, &manifest).await.unwrap();

        // Root should be a Filter node wrapping the Project.
        assert!(
            matches!(&plan.root, PlanNode::Filter(_)),
            "root should be Filter when user filters are present, got {:?}",
            std::mem::discriminant(&plan.root)
        );

        if let PlanNode::Filter(filter) = &plan.root {
            // Check the predicate is region = 'US'.
            match &filter.predicate {
                DslExpr::BinaryOp { left, op, right } => {
                    assert_eq!(
                        *op,
                        semstrait_ir::BinaryOp::Eq,
                        "should be equality filter"
                    );
                    assert!(
                        matches!(left.as_ref(), DslExpr::Column { name, .. } if name == "region"),
                        "left should be column 'region'"
                    );
                    assert!(
                        matches!(right.as_ref(), DslExpr::StringLit(s) if s == "US"),
                        "right should be string 'US'"
                    );
                }
                other => panic!("expected BinaryOp, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_multiple_user_filters() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();

        let request = ResolvedQueryRequest {
            kind_name: "orders".to_string(),
            dimensions: vec!["date".to_string()],
            measures: vec!["revenue".to_string()],
            filters: vec![
                QueryFilter {
                    field: "region".to_string(),
                    operator: FilterOperator::Eq,
                    values: vec![FilterValue::String("US".to_string())],
                },
                QueryFilter {
                    field: "revenue".to_string(),
                    operator: FilterOperator::Gt,
                    values: vec![FilterValue::Number(1000.0)],
                },
            ],
            grain: None,
            limit: None,
            order_by: vec![],
            domain_hint: None,
            session_variables: HashMap::new(),
        };

        let plan = planner.plan(&request, &manifest).await.unwrap();

        // Should have Filter -> Filter -> Project -> ...
        assert!(matches!(&plan.root, PlanNode::Filter(_)));
        if let PlanNode::Filter(f1) = &plan.root {
            assert!(matches!(f1.input.as_ref(), PlanNode::Filter(_)));
        }
    }

    // ========================================================================
    // ORDER BY and LIMIT
    // ========================================================================

    #[tokio::test]
    async fn test_order_by() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();

        let request = ResolvedQueryRequest {
            kind_name: "orders".to_string(),
            dimensions: vec!["date".to_string()],
            measures: vec!["revenue".to_string()],
            filters: vec![],
            grain: None,
            limit: None,
            order_by: vec![OrderByClause {
                field: "revenue".to_string(),
                direction: SortDirection::Descending,
            }],
            domain_hint: None,
            session_variables: HashMap::new(),
        };

        let plan = planner.plan(&request, &manifest).await.unwrap();
        assert!(
            matches!(&plan.root, PlanNode::Sort(_)),
            "root should be Sort when ORDER BY is specified"
        );
    }

    #[tokio::test]
    async fn test_limit() {
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();

        let request = ResolvedQueryRequest {
            kind_name: "orders".to_string(),
            dimensions: vec!["date".to_string()],
            measures: vec!["revenue".to_string()],
            filters: vec![],
            grain: None,
            limit: Some(10),
            order_by: vec![],
            domain_hint: None,
            session_variables: HashMap::new(),
        };

        let plan = planner.plan(&request, &manifest).await.unwrap();
        assert!(
            matches!(&plan.root, PlanNode::Fetch(_)),
            "root should be Fetch when LIMIT is specified"
        );
        if let PlanNode::Fetch(fetch) = &plan.root {
            assert_eq!(fetch.count, Some(10));
            assert_eq!(fetch.offset, 0);
        }
    }

    // ========================================================================
    // Optimizer pass-through
    // ========================================================================

    #[tokio::test]
    async fn test_optimizer_identity_pass_through() {
        // Build planner with NO optimizer passes (default).
        let planner = SemanticPlanner::builder().build();
        let manifest = make_test_manifest();
        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);

        let plan = planner.plan(&request, &manifest).await.unwrap();
        // Should succeed and produce a valid plan.
        assert!(!plan.output_names.is_empty());
    }

    /// A counting pass that tracks how many times it was invoked.
    struct CountingPass {
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl OptimizerPass for CountingPass {
        fn name(&self) -> &str {
            "counting"
        }

        fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError> {
            self.count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(plan)
        }
    }

    #[tokio::test]
    async fn test_optimizer_custom_pass_invoked() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let planner = SemanticPlanner::builder()
            .with_optimizer_pass(CountingPass {
                count: count.clone(),
            })
            .build();

        let manifest = make_test_manifest();
        let request = make_test_request("orders", vec!["date"], vec!["revenue"]);

        let plan = planner.plan(&request, &manifest).await;
        assert!(plan.is_ok());
        assert_eq!(
            count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "optimizer pass should have been invoked exactly once"
        );
    }
}
