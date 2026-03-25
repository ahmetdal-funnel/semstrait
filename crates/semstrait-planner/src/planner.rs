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
use semstrait_core::{ConsumerProfile, EngineProfile};
use semstrait_ir::{
    BinaryOp, Expr, FetchNode, FilterNode, LogicalPlan, NodeMeta, PlanNode, SortKey,
    SortNode,
};
use semstrait_manifest::{CompiledManifest, DimensionType};

use crate::additivity_resolver::AdditivityResolver;
use crate::constraint_evaluator::ConstraintEvaluator;
use crate::error::PlannerError;
use crate::kind::{extract_metadata_value, KindPlannerRegistry, PlannerContext};
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
    profile: Arc<dyn EngineProfile>,
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
            .get_kind(&request.entity_name)
            .ok_or_else(|| PlannerError::KindNotFound(request.entity_name.clone()))?;

        // Step 3: Apply domain filter — narrow datasets if domain_hint is present.
        let kind = if let Some(ref domain_hint) = request.domain_hint {
            let mut filtered_kind = kind.clone();
            filtered_kind.datasets.retain(|_ds| {
                // Keep datasets that belong to the requested domain.
                // Kind-level domain is checked: if the kind's domain list
                // doesn't include the hint, all datasets are excluded.
                if let Some(ref kind_domains) = filtered_kind.domain {
                    if !kind_domains.iter().any(|d| d == domain_hint) {
                        return false;
                    }
                }
                true
            });

            if filtered_kind.datasets.is_empty() {
                return Err(PlannerError::NoCoveringDataset {
                    kind: kind.name.clone(),
                    reason: format!(
                        "no datasets match domain hint '{}'",
                        domain_hint
                    ),
                });
            }
            std::borrow::Cow::Owned(filtered_kind)
        } else {
            std::borrow::Cow::Borrowed(kind)
        };

        // Step 3b: Metadata dimension filter — prune datasets based on
        // equality filters on metadata dimensions. If a user filter is
        // `metadata_dim = 'value'` and a dataset's extracted value doesn't
        // match, exclude that dataset from candidates.
        let kind = prune_by_metadata_filters(kind, request)?;

        // Step 4: Dispatch to kind-specific planner.
        let kind_planner = self.planners.dispatch(&kind.kind_type)?;

        // Step 5: Build planning context.
        let ctx = PlannerContext {
            manifest,
            profile: self.profile.as_ref(),
            catalog: self.catalog.as_deref(),
            session: &request.session_variables,
        };

        // Step 6: Resolve plan fragment via kind planner.
        let mut fragment = kind_planner.resolve(&kind, request, &ctx)?;

        // Step 7: Additivity resolution for each measure.
        for measure_name in &request.measures {
            if let Some(measure) = kind.measures.get(measure_name) {
                fragment =
                    AdditivityResolver::resolve(fragment, measure, request, self.profile.as_ref())?;
            }
        }

        // Step 8: Inject filters.
        let mut root = fragment.root;

        // 8a: Inject dataset-level filters.
        // (Dataset filters come from the dataset binding; skipped in v1 simplified flow.)

        // 8b: Measure-level filters are applied as conditional aggregation inside
        // the KindPlanner (GrainsetPlanner wraps aggregate expressions in
        // CASE WHEN filter THEN expr ELSE NULL END). No extra FilterNode needed here.

        // 8c: Metric-level filters follow the same conditional aggregation pattern
        // as measure filters — applied during expression lowering in KindPlanner.

        // 8d: Inject kind-level filters (applied to all queries against this kind).
        root = inject_kind_filters(root, &kind)?;

        // 8e: Inject user filters from the request.
        root = inject_user_filters(root, request)?;

        // Step 9: Apply ORDER BY.
        root = apply_order_by(root, request)?;

        // Step 10: Apply LIMIT.
        root = apply_limit(root, request)?;

        // Step 11: Build LogicalPlan.
        let output_names: Vec<String> = request
            .dimensions
            .iter()
            .chain(request.measures.iter())
            .cloned()
            .collect();

        let plan = LogicalPlan::new(root, output_names);

        // Step 12: Optimizer pass.
        self.optimizer.apply(plan)
    }
}

/// Builder for SemanticPlanner.
pub struct SemanticPlannerBuilder {
    catalog: Option<Arc<dyn CatalogProvider>>,
    passes: Vec<Box<dyn OptimizerPass>>,
    profile: Arc<dyn EngineProfile>,
}

impl SemanticPlannerBuilder {
    pub fn new() -> Self {
        Self {
            catalog: None,
            passes: Vec::new(),
            profile: Arc::new(ConsumerProfile::default()),
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
    pub fn with_profile(mut self, profile: Arc<dyn EngineProfile>) -> Self {
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
// Metadata dimension dataset pruning
// ============================================================================

/// Prune datasets based on user equality filters on metadata dimensions.
///
/// When a user filter references a metadata dimension with `Eq` operator and a
/// string value, we extract the metadata value for each dataset and exclude
/// datasets whose value doesn't match. This is a pre-planning optimization
/// similar to domain filtering.
fn prune_by_metadata_filters<'a>(
    kind: std::borrow::Cow<'a, semstrait_manifest::CompiledKind>,
    request: &ResolvedQueryRequest,
) -> Result<std::borrow::Cow<'a, semstrait_manifest::CompiledKind>, PlannerError> {
    // Collect metadata equality filters: (expected_value, MetadataDimension).
    // We clone the MetadataDimension to avoid borrowing from `kind`.
    let mut metadata_filters: Vec<(String, semstrait_manifest::MetadataDimension)> = Vec::new();

    for filter in &request.filters {
        if !matches!(filter.operator, FilterOperator::Eq) {
            continue;
        }
        if let Some(dim) = kind.dimensions.get(&filter.field) {
            if let DimensionType::Metadata(ref meta) = dim.dim_type {
                if let Some(FilterValue::String(ref val)) = filter.values.first() {
                    metadata_filters.push((val.clone(), meta.clone()));
                }
            }
        }
    }

    if metadata_filters.is_empty() {
        return Ok(kind);
    }

    let mut filtered_kind = kind.into_owned();
    filtered_kind.datasets.retain(|ds| {
        metadata_filters.iter().all(|(expected, meta)| {
            match extract_metadata_value(meta, ds) {
                Some(ref actual) => actual == expected,
                None => true, // Keep dataset if extraction fails (conservative)
            }
        })
    });

    if filtered_kind.datasets.is_empty() {
        return Err(PlannerError::NoCoveringDataset {
            kind: filtered_kind.name.clone(),
            reason: "no datasets match metadata dimension filters".to_string(),
        });
    }

    Ok(std::borrow::Cow::Owned(filtered_kind))
}

// ============================================================================
// Filter injection helpers
// ============================================================================

/// Inject kind-level filters as FilterNodes wrapping the plan root.
///
/// Kind filters apply to all queries against a kind, regardless of dataset or user request.
/// Expressions use semantic names (post-projection), so we lower with an empty mapping.
fn inject_kind_filters(
    mut root: PlanNode,
    kind: &semstrait_manifest::CompiledKind,
) -> Result<PlanNode, PlannerError> {
    let empty_mapping = std::collections::HashMap::new();
    for filter in &kind.filters {
        let predicate = crate::expr_lower::lower_expr(&filter.expr, &empty_mapping)?;

        let schema = root.meta().output_schema.clone();
        root = PlanNode::Filter(FilterNode {
            meta: NodeMeta::new(schema),
            input: Box::new(root),
            predicate,
        });
    }
    Ok(root)
}

/// Convert user QueryFilters into FilterNodes wrapping the plan root.
fn inject_user_filters(
    mut root: PlanNode,
    request: &ResolvedQueryRequest,
) -> Result<PlanNode, PlannerError> {
    for filter in &request.filters {
        let predicate = query_filter_to_expr(filter)?;
        let schema = root.meta().output_schema.clone();
        root = PlanNode::Filter(FilterNode {
            meta: NodeMeta::new(schema),
            input: Box::new(root),
            predicate,
        });
    }
    Ok(root)
}

/// Convert a QueryFilter into an Expr predicate.
fn query_filter_to_expr(
    filter: &crate::request::QueryFilter,
) -> Result<Expr, PlannerError> {
    let column = Expr::column(filter.field.clone());

    match &filter.operator {
        FilterOperator::Eq
        | FilterOperator::NotEq
        | FilterOperator::Lt
        | FilterOperator::LtEq
        | FilterOperator::Gt
        | FilterOperator::GtEq => {
            let first = filter.values.first().ok_or_else(|| {
                PlannerError::Internal(format!(
                    "{:?} filter requires at least 1 value",
                    filter.operator
                ))
            })?;
            let value = filter_value_to_expr(first)?;
            let op = match filter.operator {
                FilterOperator::Eq => BinaryOp::Eq,
                FilterOperator::NotEq => BinaryOp::NotEq,
                FilterOperator::Lt => BinaryOp::Lt,
                FilterOperator::LtEq => BinaryOp::LtEq,
                FilterOperator::Gt => BinaryOp::Gt,
                FilterOperator::GtEq => BinaryOp::GtEq,
                _ => unreachable!(),
            };
            Ok(Expr::binary(column, op, value))
        }
        FilterOperator::In => {
            // IN is translated as OR chain: col = v1 OR col = v2 OR ...
            let mut expr: Option<Expr> = None;
            for val in &filter.values {
                let eq = Expr::eq(column.clone(), filter_value_to_expr(val)?);
                expr = Some(match expr {
                    None => eq,
                    Some(prev) => Expr::or(prev, eq),
                });
            }
            expr.ok_or_else(|| PlannerError::Internal("IN filter with no values".to_string()))
        }
        FilterOperator::NotIn => {
            // NOT IN is translated as AND chain: col != v1 AND col != v2 AND ...
            let mut expr: Option<Expr> = None;
            for val in &filter.values {
                let neq = Expr::ne(column.clone(), filter_value_to_expr(val)?);
                expr = Some(match expr {
                    None => neq,
                    Some(prev) => Expr::and(prev, neq),
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
            Ok(Expr::and(
                Expr::gte(column.clone(), low),
                Expr::lte(column, high),
            ))
        }
        FilterOperator::IsNull => Ok(Expr::is_null(column)),
        FilterOperator::IsNotNull => Ok(Expr::is_not_null(column)),
    }
}

/// Convert a FilterValue to an Expr.
fn filter_value_to_expr(value: &FilterValue) -> Result<Expr, PlannerError> {
    match value {
        FilterValue::String(s) => Ok(Expr::string(s)),
        FilterValue::Number(n) => Ok(Expr::float(*n)),
        FilterValue::Bool(b) => Ok(Expr::boolean(*b)),
        FilterValue::Null => Ok(Expr::null()),
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
            expr: Expr::column(ob.field.clone()),
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

    #[test]
    fn test_plan_with_domain_hint_accepted() {
        let mut manifest = make_test_manifest();
        // Set domain on the kind.
        if let Some(kind) = manifest.kinds.get_mut("orders") {
            kind.domain = Some(vec!["financial".to_string()]);
        }

        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.domain_hint = Some("financial".to_string());

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);
        assert!(result.is_ok(), "domain hint matching kind domain should succeed");
    }

    #[test]
    fn test_plan_with_domain_hint_rejected() {
        let mut manifest = make_test_manifest();
        if let Some(kind) = manifest.kinds.get_mut("orders") {
            kind.domain = Some(vec!["financial".to_string()]);
        }

        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        request.domain_hint = Some("marketing".to_string());

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);
        assert!(result.is_err(), "domain hint not matching should fail");
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::NoCoveringDataset { .. }
        ));
    }

    #[test]
    fn test_plan_no_domain_hint_passes_through() {
        let mut manifest = make_test_manifest();
        if let Some(kind) = manifest.kinds.get_mut("orders") {
            kind.domain = Some(vec!["financial".to_string()]);
        }

        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        // No domain_hint set.

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);
        assert!(result.is_ok(), "no domain hint should pass through all datasets");
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
            PlanNode::Union(n) => n.inputs.iter().any(contains_filter_node),
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
            PlanNode::Union(n) => n.inputs.iter().any(contains_sort_node),
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
            PlanNode::Union(n) => n.inputs.iter().any(contains_fetch_node),
            PlanNode::Scan(_) => false,
        }
    }

    #[test]
    fn test_plan_with_kind_filter() {
        let mut manifest = make_test_manifest();
        // Add a kind-level filter: region = 'US'.
        if let Some(kind) = manifest.kinds.get_mut("orders") {
            kind.filters.push(semstrait_manifest::CompiledFilter {
                name: "us_only".to_string(),
                expr: semstrait_core::Expr::eq(
                    semstrait_core::Expr::column("region"),
                    semstrait_core::Expr::string("US"),
                ),
                expr_source: "region = 'US'".to_string(),
            });
        }

        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);
        assert!(result.is_ok(), "plan with kind filter should succeed: {:?}", result.err());

        let plan = result.unwrap();
        // Should have a FilterNode from the kind-level filter.
        assert!(contains_filter_node(&plan.root), "plan should contain kind-level filter");
    }

    #[test]
    fn test_plan_kind_filter_combined_with_user_filter() {
        let mut manifest = make_test_manifest();
        // Add a kind-level filter.
        if let Some(kind) = manifest.kinds.get_mut("orders") {
            kind.filters.push(semstrait_manifest::CompiledFilter {
                name: "active_only".to_string(),
                expr: semstrait_core::Expr::eq(
                    semstrait_core::Expr::column("region"),
                    semstrait_core::Expr::string("US"),
                ),
                expr_source: "region = 'US'".to_string(),
            });
        }

        let mut request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);
        // Also add a user filter.
        request.filters = vec![QueryFilter {
            field: "date".to_string(),
            operator: FilterOperator::Eq,
            values: vec![FilterValue::String("2024-01-01".to_string())],
        }];

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);
        assert!(result.is_ok(), "plan with both filters should succeed");

        let plan = result.unwrap();
        // Count filter nodes — should be at least 2 (kind + user).
        let filter_count = count_filter_nodes(&plan.root);
        assert!(filter_count >= 2, "should have at least 2 filter nodes (kind + user), got {}", filter_count);
    }

    fn count_filter_nodes(node: &PlanNode) -> usize {
        match node {
            PlanNode::Filter(n) => 1 + count_filter_nodes(&n.input),
            PlanNode::Project(n) => count_filter_nodes(&n.input),
            PlanNode::Aggregate(n) => count_filter_nodes(&n.input),
            PlanNode::Sort(n) => count_filter_nodes(&n.input),
            PlanNode::Fetch(n) => count_filter_nodes(&n.input),
            PlanNode::Join(n) => count_filter_nodes(&n.left) + count_filter_nodes(&n.right),
            PlanNode::Union(n) => n.inputs.iter().map(count_filter_nodes).sum(),
            PlanNode::Scan(_) => 0,
        }
    }

    #[test]
    fn test_plan_no_kind_filters() {
        // Verify baseline: no kind filters means no extra filter nodes
        // (unless user adds one).
        let manifest = make_test_manifest();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let plan = planner.plan(&request, &manifest).expect("should succeed");
        assert!(!contains_filter_node(&plan.root), "no filter should be present without user/kind filters");
    }

    #[test]
    fn test_metadata_dimension_filter_prunes_datasets() {
        use indexmap::IndexMap;
        use semstrait_manifest::{
            ColumnMappingValue, CompiledDimension, CompiledKind, CompiledKindDataset,
            CompiledKindType, CompiledMeasure, DimensionType, KindDatasetExtras,
            MetadataDimension, PathExtraction,
        };

        // Create a kind with 2 datasets, each with a different source path.
        let mut dimensions = IndexMap::new();
        dimensions.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Categorical(
                    semstrait_manifest::CategoricalDimension { enum_values: None },
                ),
            },
        );
        dimensions.insert(
            "source".to_string(),
            CompiledDimension {
                name: "source".to_string(),
                description: None,
                data_type: "string".to_string(),
                dim_type: DimensionType::Metadata(MetadataDimension {
                    path: Some(PathExtraction { token: 1 }),
                    partition: None,
                }),
            },
        );

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: "float64".to_string(),
                agg: None,
                expr: semstrait_core::Expr::entity_ref("SUM(amount)"),
                expr_source: "SUM(amount)".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        let make_ds = |name: &str, source: &str| -> CompiledKindDataset {
            let mut mapping = std::collections::HashMap::new();
            mapping.insert(
                "date".to_string(),
                ColumnMappingValue::Simple("order_date".to_string()),
            );
            mapping.insert(
                "revenue".to_string(),
                ColumnMappingValue::Simple("amount".to_string()),
            );
            CompiledKindDataset {
                name: name.to_string(),
                extras: KindDatasetExtras {
                    column_mapping: mapping.into(),
                    temporal: None,
                    storage: None,
                    catalog: None,
                },
                resolved_sources: vec![source.to_string()],
            }
        };

        let kind = CompiledKind {
            name: "multi_source".to_string(),
            description: None,
            dimensions,
            measures,
            metrics: IndexMap::new(),
            keys: None,
            kind_type: CompiledKindType::Grainset,
            datasets: vec![
                make_ds("shopify_data", "bucket/shopify/data.parquet"),
                make_ds("ga4_data", "bucket/ga4/data.parquet"),
            ],
            relationships: vec![],
            domain: None,
            filters: vec![],
        };

        let mut kinds = IndexMap::new();
        kinds.insert("multi_source".to_string(), kind);

        let manifest = semstrait_manifest::CompiledManifest {
            version: 1,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_meta_filter".to_string(),
            datasets: IndexMap::new(),
            kinds,
            relationships: vec![],
            model_name: "test_meta_filter".to_string(),
            model_description: None,
        };

        // Query with a metadata filter: source = 'shopify'
        let mut request = make_test_request(
            "multi_source",
            vec!["date", "source"],
            vec!["revenue"],
        );
        request.filters = vec![QueryFilter {
            field: "source".to_string(),
            operator: FilterOperator::Eq,
            values: vec![FilterValue::String("shopify".to_string())],
        }];

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);
        assert!(result.is_ok(), "metadata filter should succeed: {:?}", result.err());

        // The plan should only scan from the shopify dataset.
        let plan = result.unwrap();
        let scan_tables = collect_scan_tables(&plan.root);
        assert_eq!(scan_tables.len(), 1);
        assert!(
            scan_tables[0].contains("shopify"),
            "should scan shopify dataset, got: {:?}",
            scan_tables
        );
    }

    fn collect_scan_tables(node: &PlanNode) -> Vec<String> {
        match node {
            PlanNode::Scan(s) => vec![s.table_name.clone()],
            PlanNode::Project(n) => collect_scan_tables(&n.input),
            PlanNode::Aggregate(n) => collect_scan_tables(&n.input),
            PlanNode::Filter(n) => collect_scan_tables(&n.input),
            PlanNode::Sort(n) => collect_scan_tables(&n.input),
            PlanNode::Fetch(n) => collect_scan_tables(&n.input),
            PlanNode::Join(n) => {
                let mut v = collect_scan_tables(&n.left);
                v.extend(collect_scan_tables(&n.right));
                v
            }
            PlanNode::Union(n) => n.inputs.iter().flat_map(collect_scan_tables).collect(),
        }
    }
}
