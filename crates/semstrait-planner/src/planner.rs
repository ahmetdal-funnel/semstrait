//! SemanticPlanner — the main entry point for query planning.
//!
//! Orchestrates the full planning pipeline:
//! 1. Constraint evaluation
//! 2. Kind dispatch
//! 3. KindPlanner resolution
//! 4. Additivity resolution
//! 5. Filter injection
//! 6. Optimizer application

use std::sync::Arc;

use semstrait_catalog::CatalogProvider;
use semstrait_core::ConsumerProfile;
use semstrait_ir::{
    BinaryOp, DslExpr, FetchNode, FilterNode, LogicalPlan, NodeMeta, PlanNode, SortKey,
    SortNode,
};
use semstrait_manifest::CompiledManifest;

use crate::additivity_resolver::AdditivityResolver;
use crate::constraint_evaluator::ConstraintEvaluator;
use crate::error::PlannerError;
use crate::kind_planner::{KindPlannerRegistry, PlannerContext};
use crate::optimizer::{Optimizer, OptimizerPass};
use crate::request::{FilterOperator, FilterValue, ResolvedQueryRequest, SortDirection};

/// The semantic query planner.
///
/// Builds a `LogicalPlan` from a `ResolvedQueryRequest` + `CompiledManifest`.
/// Constructed via `SemanticPlannerBuilder`.
pub struct SemanticPlanner {
    catalog: Option<Arc<dyn CatalogProvider>>,
    optimizer: Optimizer,
    planners: KindPlannerRegistry,
    profile: ConsumerProfile,
}

impl SemanticPlanner {
    /// Create a builder for configuring and constructing a SemanticPlanner.
    pub fn builder() -> SemanticPlannerBuilder {
        SemanticPlannerBuilder::new()
    }

    /// Plan a query request against the compiled manifest.
    pub fn plan(
        &self,
        request: &ResolvedQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<LogicalPlan, PlannerError> {
        // Step 1: Constraint evaluation (step 0 in CONTEXT.md).
        ConstraintEvaluator::check(request, manifest)?;

        // Step 2: Look up the kind.
        let kind = manifest
            .get_kind(&request.kind_name)
            .ok_or_else(|| PlannerError::KindNotFound(request.kind_name.clone()))?;

        // Step 3: Dispatch to kind-specific planner.
        let kind_planner = self.planners.dispatch(&kind.kind_type)?;

        // Step 4: Build planning context.
        let ctx = PlannerContext {
            manifest,
            profile: &self.profile,
            catalog: self.catalog.as_deref(),
            session: &request.session_variables,
        };

        // Step 5: Resolve plan fragment via kind planner.
        let mut fragment = kind_planner.resolve(kind, request, &ctx)?;

        // Step 6: Additivity resolution for each measure.
        for measure_name in &request.measures {
            if let Some(measure) = kind.measures.get(measure_name) {
                fragment =
                    AdditivityResolver::resolve(fragment, measure, request, &self.profile)?;
            }
        }

        // Step 7: Inject filters.
        let mut root = fragment.root;

        // 7a: Inject dataset-level filters.
        // (Dataset filters come from the dataset binding; skipped in v1 simplified flow.)

        // 7b: Measure-level filters are applied as conditional aggregation inside
        // the KindPlanner (GrainsetPlanner wraps aggregate expressions in
        // CASE WHEN filter THEN expr ELSE NULL END). No extra FilterNode needed here.

        // 7c: Metric-level filters follow the same conditional aggregation pattern
        // as measure filters — applied during expression lowering in KindPlanner.

        // 7d: Inject user filters from the request.
        root = inject_user_filters(root, request)?;

        // Step 8: Apply ORDER BY.
        root = apply_order_by(root, request)?;

        // Step 9: Apply LIMIT.
        root = apply_limit(root, request)?;

        // Step 10: Build LogicalPlan.
        let output_names: Vec<String> = request
            .dimensions
            .iter()
            .chain(request.measures.iter())
            .cloned()
            .collect();

        let plan = LogicalPlan::new(root, output_names);

        // Step 11: Optimizer pass.
        self.optimizer.apply(plan)
    }
}

/// Builder for SemanticPlanner.
pub struct SemanticPlannerBuilder {
    catalog: Option<Arc<dyn CatalogProvider>>,
    passes: Vec<Box<dyn OptimizerPass>>,
    profile: ConsumerProfile,
}

impl SemanticPlannerBuilder {
    pub fn new() -> Self {
        Self {
            catalog: None,
            passes: Vec::new(),
            profile: ConsumerProfile::default(),
        }
    }

    /// Set the catalog provider.
    pub fn with_catalog(mut self, catalog: Arc<dyn CatalogProvider>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Add an optimizer pass.
    pub fn with_optimizer_pass(mut self, pass: impl OptimizerPass + 'static) -> Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// Set the consumer profile (engine capabilities).
    pub fn with_profile(mut self, profile: ConsumerProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Build the SemanticPlanner.
    pub fn build(self) -> SemanticPlanner {
        SemanticPlanner {
            catalog: self.catalog,
            optimizer: Optimizer::new(self.passes),
            planners: KindPlannerRegistry::new(),
            profile: self.profile,
        }
    }
}

impl Default for SemanticPlannerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Filter injection helpers
// ============================================================================

/// Convert user QueryFilters into FilterNodes wrapping the plan root.
fn inject_user_filters(
    mut root: PlanNode,
    request: &ResolvedQueryRequest,
) -> Result<PlanNode, PlannerError> {
    for filter in &request.filters {
        let predicate = query_filter_to_dsl_expr(filter)?;
        let schema = root.meta().output_schema.clone();
        root = PlanNode::Filter(FilterNode {
            meta: NodeMeta::new(schema),
            input: Box::new(root),
            predicate,
        });
    }
    Ok(root)
}

/// Convert a QueryFilter into a DslExpr predicate.
fn query_filter_to_dsl_expr(
    filter: &crate::request::QueryFilter,
) -> Result<DslExpr, PlannerError> {
    let column = DslExpr::Column {
        name: filter.field.clone(),
        qualifier: None,
    };

    match &filter.operator {
        FilterOperator::Eq => {
            let value = filter_value_to_expr(&filter.values[0])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(column),
                op: BinaryOp::Eq,
                right: Box::new(value),
            })
        }
        FilterOperator::NotEq => {
            let value = filter_value_to_expr(&filter.values[0])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(column),
                op: BinaryOp::NotEq,
                right: Box::new(value),
            })
        }
        FilterOperator::Lt => {
            let value = filter_value_to_expr(&filter.values[0])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(column),
                op: BinaryOp::Lt,
                right: Box::new(value),
            })
        }
        FilterOperator::LtEq => {
            let value = filter_value_to_expr(&filter.values[0])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(column),
                op: BinaryOp::LtEq,
                right: Box::new(value),
            })
        }
        FilterOperator::Gt => {
            let value = filter_value_to_expr(&filter.values[0])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(column),
                op: BinaryOp::Gt,
                right: Box::new(value),
            })
        }
        FilterOperator::GtEq => {
            let value = filter_value_to_expr(&filter.values[0])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(column),
                op: BinaryOp::GtEq,
                right: Box::new(value),
            })
        }
        FilterOperator::In => {
            // IN is translated as OR chain: col = v1 OR col = v2 OR ...
            let mut expr: Option<DslExpr> = None;
            for val in &filter.values {
                let eq = DslExpr::BinaryOp {
                    left: Box::new(column.clone()),
                    op: BinaryOp::Eq,
                    right: Box::new(filter_value_to_expr(val)?),
                };
                expr = Some(match expr {
                    None => eq,
                    Some(prev) => DslExpr::BinaryOp {
                        left: Box::new(prev),
                        op: BinaryOp::Or,
                        right: Box::new(eq),
                    },
                });
            }
            expr.ok_or_else(|| PlannerError::Internal("IN filter with no values".to_string()))
        }
        FilterOperator::NotIn => {
            // NOT IN is translated as AND chain: col != v1 AND col != v2 AND ...
            let mut expr: Option<DslExpr> = None;
            for val in &filter.values {
                let neq = DslExpr::BinaryOp {
                    left: Box::new(column.clone()),
                    op: BinaryOp::NotEq,
                    right: Box::new(filter_value_to_expr(val)?),
                };
                expr = Some(match expr {
                    None => neq,
                    Some(prev) => DslExpr::BinaryOp {
                        left: Box::new(prev),
                        op: BinaryOp::And,
                        right: Box::new(neq),
                    },
                });
            }
            expr.ok_or_else(|| {
                PlannerError::Internal("NOT IN filter with no values".to_string())
            })
        }
        FilterOperator::Between => {
            // BETWEEN is: col >= low AND col <= high
            if filter.values.len() != 2 {
                return Err(PlannerError::Internal(
                    "BETWEEN filter requires exactly 2 values".to_string(),
                ));
            }
            let low = filter_value_to_expr(&filter.values[0])?;
            let high = filter_value_to_expr(&filter.values[1])?;
            Ok(DslExpr::BinaryOp {
                left: Box::new(DslExpr::BinaryOp {
                    left: Box::new(column.clone()),
                    op: BinaryOp::GtEq,
                    right: Box::new(low),
                }),
                op: BinaryOp::And,
                right: Box::new(DslExpr::BinaryOp {
                    left: Box::new(column),
                    op: BinaryOp::LtEq,
                    right: Box::new(high),
                }),
            })
        }
        FilterOperator::IsNull => Ok(DslExpr::IsNull(Box::new(column))),
        FilterOperator::IsNotNull => Ok(DslExpr::IsNotNull(Box::new(column))),
    }
}

/// Convert a FilterValue to a DslExpr.
fn filter_value_to_expr(value: &FilterValue) -> Result<DslExpr, PlannerError> {
    match value {
        FilterValue::String(s) => Ok(DslExpr::StringLit(s.clone())),
        FilterValue::Number(n) => Ok(DslExpr::Number(*n)),
        FilterValue::Bool(b) => Ok(DslExpr::Bool(*b)),
        FilterValue::Null => Ok(DslExpr::Null),
    }
}

// ============================================================================
// ORDER BY and LIMIT helpers
// ============================================================================

/// Apply ORDER BY clauses from the request.
fn apply_order_by(
    root: PlanNode,
    request: &ResolvedQueryRequest,
) -> Result<PlanNode, PlannerError> {
    if request.order_by.is_empty() {
        return Ok(root);
    }

    let sort_keys: Vec<SortKey> = request
        .order_by
        .iter()
        .map(|ob| SortKey {
            expr: DslExpr::Column {
                name: ob.field.clone(),
                qualifier: None,
            },
            direction: match ob.direction {
                SortDirection::Ascending => semstrait_ir::SortDirection::Ascending,
                SortDirection::Descending => semstrait_ir::SortDirection::Descending,
            },
        })
        .collect();

    let schema = root.meta().output_schema.clone();
    Ok(PlanNode::Sort(SortNode {
        meta: NodeMeta::new(schema),
        input: Box::new(root),
        sort_keys,
    }))
}

/// Apply LIMIT from the request.
fn apply_limit(root: PlanNode, request: &ResolvedQueryRequest) -> Result<PlanNode, PlannerError> {
    match request.limit {
        None => Ok(root),
        Some(limit) => {
            let schema = root.meta().output_schema.clone();
            Ok(PlanNode::Fetch(FetchNode {
                meta: NodeMeta::new(schema),
                input: Box::new(root),
                count: Some(limit as i64),
                offset: 0,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use crate::request::{FilterOperator, FilterValue, OrderByClause, QueryFilter, SortDirection};

    #[test]
    fn test_plan_basic_grainset() {
        let manifest = make_test_manifest();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "basic grainset planning should succeed");
        let plan = result.unwrap();
        assert_eq!(plan.output_names.len(), 3); // date, region, revenue
        assert_eq!(plan.output_names, vec!["date", "region", "revenue"]);
    }

    #[test]
    fn test_plan_with_filters() {
        let manifest = make_test_manifest();
        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.filters = vec![QueryFilter {
            field: "region".to_string(),
            operator: FilterOperator::Eq,
            values: vec![FilterValue::String("US".to_string())],
        }];

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "planning with filters should succeed");
        let plan = result.unwrap();

        // Verify FilterNode exists in the plan
        let has_filter = contains_filter_node(&plan.root);
        assert!(has_filter, "plan should contain a FilterNode");
    }

    #[test]
    fn test_plan_with_order_by() {
        let manifest = make_test_manifest();
        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.order_by = vec![OrderByClause {
            field: "revenue".to_string(),
            direction: SortDirection::Descending,
        }];

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "planning with order_by should succeed");
        let plan = result.unwrap();

        // Verify SortNode exists in the plan
        let has_sort = contains_sort_node(&plan.root);
        assert!(has_sort, "plan should contain a SortNode");
    }

    #[test]
    fn test_plan_with_limit() {
        let manifest = make_test_manifest();
        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.limit = Some(100);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "planning with limit should succeed");
        let plan = result.unwrap();

        // Verify FetchNode exists in the plan
        let has_fetch = contains_fetch_node(&plan.root);
        assert!(has_fetch, "plan should contain a FetchNode");
    }

    #[test]
    fn test_plan_kind_not_found() {
        let manifest = make_test_manifest();
        let request = make_test_request("nonexistent_kind", vec!["date"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_err(), "should error when kind doesn't exist");
        assert!(matches!(result.unwrap_err(), PlannerError::KindNotFound(_)));
    }

    // Helper functions to check for node types in the plan tree
    fn contains_filter_node(node: &PlanNode) -> bool {
        match node {
            PlanNode::Filter(_) => true,
            PlanNode::Project(n) => contains_filter_node(&n.input),
            PlanNode::Aggregate(n) => contains_filter_node(&n.input),
            PlanNode::Sort(n) => contains_filter_node(&n.input),
            PlanNode::Fetch(n) => contains_filter_node(&n.input),
            PlanNode::Join(n) => contains_filter_node(&n.left) || contains_filter_node(&n.right),
            PlanNode::Union(n) => n.inputs.iter().any(|i| contains_filter_node(i)),
            PlanNode::Scan(_) => false,
        }
    }

    fn contains_sort_node(node: &PlanNode) -> bool {
        match node {
            PlanNode::Sort(_) => true,
            PlanNode::Project(n) => contains_sort_node(&n.input),
            PlanNode::Aggregate(n) => contains_sort_node(&n.input),
            PlanNode::Filter(n) => contains_sort_node(&n.input),
            PlanNode::Fetch(n) => contains_sort_node(&n.input),
            PlanNode::Join(n) => contains_sort_node(&n.left) || contains_sort_node(&n.right),
            PlanNode::Union(n) => n.inputs.iter().any(|i| contains_sort_node(i)),
            PlanNode::Scan(_) => false,
        }
    }

    fn contains_fetch_node(node: &PlanNode) -> bool {
        match node {
            PlanNode::Fetch(_) => true,
            PlanNode::Project(n) => contains_fetch_node(&n.input),
            PlanNode::Aggregate(n) => contains_fetch_node(&n.input),
            PlanNode::Filter(n) => contains_fetch_node(&n.input),
            PlanNode::Sort(n) => contains_fetch_node(&n.input),
            PlanNode::Join(n) => contains_fetch_node(&n.left) || contains_fetch_node(&n.right),
            PlanNode::Union(n) => n.inputs.iter().any(|i| contains_fetch_node(i)),
            PlanNode::Scan(_) => false,
        }
    }
}
