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
use semstrait_ir::{
    BinaryOp, DefaultPlanBuilder, Expr, FetchNode, FilterNode, LogicalPlan, NodeMeta, PlanBuilder,
    PlanNode, SortKey, SortNode,
};
use semstrait_manifest::CompiledManifest;

use crate::additivity::AdditivityResolver;
use crate::resolver::ExprResolver;
use crate::validator::ConstraintValidator;
use crate::error::PlannerError;
use crate::kind::joinset::resolve_from_fields;
use crate::kind::{
    KindPlannerRegistry, PlanFragment, PlannerContext, PrunedView,
};
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
    plan_builder: Box<dyn PlanBuilder>,
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
        ConstraintValidator::check(request, manifest)?;

        // Step 2: Resolve entity via CompiledDataKind hierarchy.
        let ctx = PlannerContext {
            manifest,
            catalog: self.catalog.as_deref(),
            session: &request.session_variables,
            plan_builder: self.plan_builder.as_ref(),
        };

        let (fragment, entity_measures, entity_filters) =
            self.resolve_entity(request, manifest, &ctx)?;

        // Step 7: Additivity resolution for each measure.
        let mut fragment = fragment;
        for measure_name in &request.measures {
            if let Some(measure) = entity_measures.get(measure_name) {
                fragment =
                    AdditivityResolver::resolve(fragment, measure, request)?;
            }
        }

        // Step 8: Inject filters.
        let mut root = fragment.root;

        // 8d: Inject entity-level filters (applied to all queries against this entity).
        root = inject_entity_filters(root, &entity_filters)?;

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

    /// Resolve the entity and build the plan fragment.
    ///
    /// Returns (fragment, measures_map, filters) where measures_map and filters
    /// are borrowed from the entity for post-resolution processing.
    ///
    /// Unified dispatch through CompiledDataKind. Dataset variants use the fast path;
    /// complex kinds (grainset/unionset/joinset) delegate to KindPlanner registry.
    #[allow(clippy::type_complexity)]
    fn resolve_entity<'a>(
        &self,
        request: &ResolvedQueryRequest,
        manifest: &'a CompiledManifest,
        ctx: &PlannerContext<'_>,
    ) -> Result<
        (
            PlanFragment,
            &'a indexmap::IndexMap<String, semstrait_manifest::CompiledMeasure>,
            Vec<semstrait_manifest::CompiledFilter>,
        ),
        PlannerError,
    > {
        // Resolve via CompiledDataKind (primary path).
        let data_kind = manifest
            .resolve(&request.entity_name)
            .ok_or_else(|| PlannerError::KindNotFound(request.entity_name.clone()))?;
        let iface = data_kind.interface();

        // Prune bindings by metadata and literal filters (borrow-only, no clone).
        let mut pruned = PrunedView::all(data_kind);
        pruned.prune_by_metadata(request)?;
        pruned.prune_by_literals(request)?;

        // Dispatch through CompiledDataKind.
        let fragment =
            crate::kind::dispatch_data_kind(&pruned, request, ctx, &self.planners)?;

        let filters = iface.filters.clone();
        Ok((fragment, &iface.measures, filters))
    }

    /// Plan an ad-hoc query where `FROM` is omitted.
    ///
    /// Uses `resolve_from_fields()` to infer the target entity from the requested
    /// field names (dimensions + measures). If all fields resolve to a single dataset,
    /// plans against that dataset. If multiple datasets are needed, returns the
    /// resolved join path for future join plan construction.
    ///
    /// Currently supports single-dataset resolution only. Multi-dataset ad-hoc joins
    /// will be supported in a future phase.
    pub fn plan_ad_hoc(
        &self,
        request: &ResolvedQueryRequest,
        manifest: &CompiledManifest,
    ) -> Result<LogicalPlan, PlannerError> {
        // Collect all requested field names.
        let all_fields: Vec<String> = request
            .dimensions
            .iter()
            .chain(request.measures.iter())
            .cloned()
            .collect();

        // Resolve datasets and join path from field names.
        let resolved = resolve_from_fields(&all_fields, manifest)?;

        if resolved.datasets.len() == 1 {
            // Single dataset — resolve via the unified CompiledDataKind dispatch path.
            let ds_name = &resolved.datasets[0];

            // Build a request targeting the resolved dataset/kind.
            let mut ad_hoc_request = request.clone();
            ad_hoc_request.entity_name = ds_name.clone();

            let ctx = PlannerContext {
                manifest,
                catalog: self.catalog.as_deref(),
                session: &request.session_variables,
                plan_builder: self.plan_builder.as_ref(),
            };

            let (fragment, entity_measures, entity_filters) =
                self.resolve_entity(&ad_hoc_request, manifest, &ctx)?;

            // Additivity resolution.
            let mut fragment = fragment;
            for measure_name in &request.measures {
                if let Some(measure) = entity_measures.get(measure_name) {
                    fragment = AdditivityResolver::resolve(
                        fragment,
                        measure,
                        request,
                    )?;
                }
            }

            // Filter injection.
            let mut root = fragment.root;
            root = inject_entity_filters(root, &entity_filters)?;
            root = inject_user_filters(root, request)?;

            // ORDER BY + LIMIT.
            root = apply_order_by(root, request)?;
            root = apply_limit(root, request)?;

            // Build LogicalPlan.
            let output_names: Vec<String> = request
                .dimensions
                .iter()
                .chain(request.measures.iter())
                .cloned()
                .collect();

            let plan = LogicalPlan::new(root, output_names);
            self.optimizer.apply(plan)
        } else {
            // Multi-dataset ad-hoc join — not yet implemented.
            // The resolution found that fields span multiple datasets
            // connected by join path: {:?}.
            Err(PlannerError::Internal(format!(
                "ad-hoc multi-dataset join not yet supported: fields span {} datasets ({}) \
                 with join path {:?}",
                resolved.datasets.len(),
                resolved.datasets.join(", "),
                resolved.join_path,
            )))
        }
    }
}

/// Builder for SemanticPlanner.
pub struct SemanticPlannerBuilder {
    catalog: Option<Arc<dyn CatalogProvider>>,
    passes: Vec<Box<dyn OptimizerPass>>,
    plan_builder: Box<dyn PlanBuilder>,
}

impl SemanticPlannerBuilder {
    pub fn new() -> Self {
        Self {
            catalog: None,
            passes: Vec::new(),
            plan_builder: Box::new(DefaultPlanBuilder),
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

    /// Set the engine-specific plan builder.
    pub fn with_plan_builder(mut self, builder: impl PlanBuilder + 'static) -> Self {
        self.plan_builder = Box::new(builder);
        self
    }

    /// Build the SemanticPlanner.
    pub fn build(self) -> SemanticPlanner {
        SemanticPlanner {
            catalog: self.catalog,
            optimizer: Optimizer::new(self.passes),
            planners: KindPlannerRegistry::new(),
            plan_builder: self.plan_builder,
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

/// Inject entity-level filters as FilterNodes wrapping the plan root.
///
/// Entity filters apply to all queries against an entity, regardless of dataset or user request.
/// Expressions use semantic names (post-projection), so we lower with an empty mapping.
fn inject_entity_filters(
    mut root: PlanNode,
    filters: &[semstrait_manifest::CompiledFilter],
) -> Result<PlanNode, PlannerError> {
    let empty_mapping = std::collections::HashMap::new();
    let resolver = crate::resolver::MappingResolver::new(&empty_mapping);
    for filter in filters {
        let predicate = resolver.resolve_expr(&filter.expr)?;

        let schema = Arc::clone(&root.meta().output_schema);
        root = PlanNode::Filter(FilterNode {
            meta: NodeMeta::new_shared(schema),
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
        let schema = Arc::clone(&root.meta().output_schema);
        root = PlanNode::Filter(FilterNode {
            meta: NodeMeta::new_shared(schema),
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

    let schema = Arc::clone(&root.meta().output_schema);
    Ok(PlanNode::Sort(SortNode {
        meta: NodeMeta::new_shared(schema),
        input: Box::new(root),
        sort_keys,
    }))
}

/// Apply LIMIT from the request.
fn apply_limit(root: PlanNode, request: &ResolvedQueryRequest) -> Result<PlanNode, PlannerError> {
    match request.limit {
        None => Ok(root),
        Some(limit) => {
            let schema = Arc::clone(&root.meta().output_schema);
            Ok(PlanNode::Fetch(FetchNode {
                meta: NodeMeta::new_shared(schema),
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
    use crate::tests::helpers::*;
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
        if let Some(dk) = manifest.data_kinds.get_mut("orders") {
            dk.interface_mut().filters.push(semstrait_manifest::CompiledFilter {
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
        if let Some(dk) = manifest.data_kinds.get_mut("orders") {
            dk.interface_mut().filters.push(semstrait_manifest::CompiledFilter {
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
            CompiledDimension, CompiledMeasure, DimensionType,
            MetadataDimension, PathExtraction,
        };
        use semstrait_manifest::acceleration::{
            CoverageIndex, CompiledDataKind, DatasetBinding, DimensionIndex, CompiledGrainsetKind,
            CompiledInterface, ResolvedColumnMapping,
        };

        // Create a kind with 2 datasets, each with a different source path.
        let mut dimensions = IndexMap::new();
        dimensions.insert(
            "date".to_string(),
            CompiledDimension {
                name: "date".to_string(),
                description: None,
                data_type: semstrait_core::DataType::String,
                dim_type: DimensionType::Categorical(
                    semstrait_manifest::CategoricalDimension { enum_values: None },
                ),
                expr: None,
                expr_source: None,
            },
        );
        dimensions.insert(
            "source".to_string(),
            CompiledDimension {
                name: "source".to_string(),
                description: None,
                data_type: semstrait_core::DataType::String,
                dim_type: DimensionType::Metadata(MetadataDimension {
                    path: Some(PathExtraction { token: 1 }),
                    partition: None,
                }),
                expr: None,
                expr_source: None,
            },
        );

        let mut measures = IndexMap::new();
        measures.insert(
            "revenue".to_string(),
            CompiledMeasure {
                name: "revenue".to_string(),
                description: None,
                data_type: semstrait_core::DataType::Number,
                agg: semstrait_core::Aggregation::Sum,
                expr: semstrait_core::Expr::entity_ref("amount"),
                expr_source: "amount".to_string(),
                additivity: None,
                constraints: None,
                filters: vec![],
            },
        );

        let make_binding = |name: &str, source: &str| -> DatasetBinding {
            let mut physical = IndexMap::new();
            physical.insert("date".to_string(), "order_date".to_string());
            physical.insert("revenue".to_string(), "amount".to_string());
            DatasetBinding {
                dataset_name: name.to_string(),
                column_mapping: ResolvedColumnMapping {
                    physical,
                    literals: std::collections::HashMap::new(),
                    temporal: std::collections::HashMap::new(),
                    anchored: std::collections::HashMap::new(),
                },
                resolved_sources: vec![semstrait_manifest::ResolvedSource::path(source)],
            }
        };

        let bindings = vec![
            make_binding("shopify_data", "bucket/shopify/data.parquet"),
            make_binding("ga4_data", "bucket/ga4/data.parquet"),
        ];

        let interface = CompiledInterface {
            name: "multi_source".to_string(),
            description: None,
            dimensions: dimensions.clone(),
            measures: measures.clone(),
            metrics: IndexMap::new(),
            keys: None,
            filters: vec![],
            temporal_dim: None,
        };

        let coverage_index = CoverageIndex::build(&dimensions, &measures, &bindings);
        let dimension_index = DimensionIndex::build(&dimensions, &bindings);

        let data_kind = CompiledDataKind::Grainset(Box::new(CompiledGrainsetKind {
            interface,
            bindings,
            coverage_index,
            dimension_index,
            metric_order: None,
            grain_map: None,
        }));

        let mut data_kinds = IndexMap::new();
        data_kinds.insert("multi_source".to_string(), data_kind);

        let manifest = semstrait_manifest::CompiledManifest {
            version: 3,
            compiled_at: chrono::Utc::now(),
            source_hash: "test_meta_filter".to_string(),
            relationships: vec![],
            model_name: "test_meta_filter".to_string(),
            model_description: None,
            data_kinds,
            relationship_graph: semstrait_manifest::RelationshipGraph::default(),
            field_index: semstrait_manifest::FieldIndex::default(),
            diagnostics: semstrait_manifest::CompileDiagnostics::default(),
            semantic_graph: semstrait_manifest::SemanticGraph::default(),
            catalog_snapshot: None,
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

    // ================================================================
    // Dataset planning tests
    // ================================================================

    #[test]
    fn test_plan_dataset_basic() {
        let manifest = make_test_manifest();
        let request = make_test_request("orders", vec!["date", "region"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "Dataset planning should succeed: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.output_names.len(), 3);
        assert_eq!(plan.output_names, vec!["date", "region", "revenue"]);
    }

    #[test]
    fn test_plan_dataset_with_filters() {
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
        assert!(contains_filter_node(&plan.root), "plan should contain a FilterNode");
    }

    #[test]
    fn test_plan_dataset_with_order_by_and_limit() {
        let manifest = make_test_manifest();
        let mut request = make_test_request("orders", vec!["date"], vec!["revenue"]);
        request.order_by = vec![OrderByClause {
            field: "revenue".to_string(),
            direction: SortDirection::Descending,
        }];
        request.limit = Some(100);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "planning with order_by + limit should succeed");
        let plan = result.unwrap();
        assert!(contains_sort_node(&plan.root));
        assert!(contains_fetch_node(&plan.root));
    }

    // ================================================================
    // Ad-hoc resolution tests
    // ================================================================

    #[test]
    fn test_ad_hoc_single_dataset_resolution() {
        use semstrait_manifest::{FieldIndex, CompiledDimension, CompiledMeasure};
        use semstrait_manifest::acceleration::{CompiledDataKind, CompiledDatasetKind, DatasetBinding, ResolvedColumnMapping};
        use std::collections::HashSet;

        let mut manifest = make_test_manifest();

        let mut dims = indexmap::IndexMap::new();
        dims.insert("date".to_string(), CompiledDimension {
            name: "date".to_string(),
            description: None,
            data_type: semstrait_core::DataType::String,
            dim_type: semstrait_manifest::DimensionType::Categorical(
                semstrait_manifest::CategoricalDimension { enum_values: None },
            ),
            expr: None,
            expr_source: None,
        });
        dims.insert("region".to_string(), CompiledDimension {
            name: "region".to_string(),
            description: None,
            data_type: semstrait_core::DataType::String,
            dim_type: semstrait_manifest::DimensionType::Categorical(
                semstrait_manifest::CategoricalDimension { enum_values: None },
            ),
            expr: None,
            expr_source: None,
        });

        let mut measures = indexmap::IndexMap::new();
        measures.insert("revenue".to_string(), CompiledMeasure {
            name: "revenue".to_string(),
            description: None,
            data_type: semstrait_core::DataType::Number,
            agg: semstrait_core::Aggregation::Sum,
            expr: semstrait_core::Expr::entity_ref("amount"),
            expr_source: "amount".to_string(),
            additivity: None,
            constraints: None,
            filters: vec![],
        });

        // Build CompiledDataKind for ad-hoc resolution.
        let mut physical = indexmap::IndexMap::new();
        physical.insert("date".to_string(), "order_date".to_string());
        physical.insert("region".to_string(), "region_name".to_string());
        physical.insert("revenue".to_string(), "amount".to_string());

        let binding = DatasetBinding {
            dataset_name: "orders_ds".to_string(),
            column_mapping: ResolvedColumnMapping {
                physical,
                literals: std::collections::HashMap::new(),
                temporal: std::collections::HashMap::new(),
                anchored: std::collections::HashMap::new(),
            },
            resolved_sources: vec![],
        };

        let iface = semstrait_manifest::CompiledInterface {
            name: "orders_ds".to_string(),
            description: None,
            dimensions: dims,
            measures,
            metrics: indexmap::IndexMap::new(),
            keys: None,
            filters: vec![],
            temporal_dim: None,
        };

        manifest.data_kinds.insert(
            "orders_ds".to_string(),
            CompiledDataKind::Dataset(Box::new(CompiledDatasetKind { interface: iface, binding })),
        );

        // Build a FieldIndex pointing to orders_ds.
        let mut providers = std::collections::HashMap::new();
        providers.insert("date".to_string(), vec!["orders_ds".to_string()]);
        providers.insert("region".to_string(), vec!["orders_ds".to_string()]);
        providers.insert("revenue".to_string(), vec!["orders_ds".to_string()]);

        manifest.field_index = FieldIndex {
            providers,
            all_dimensions: ["date", "region"].iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
            all_measures: ["revenue"].iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
            all_metrics: HashSet::new(),
        };

        // Request with empty entity_name — ad-hoc resolution should find orders_ds.
        let request = make_test_request("", vec!["date", "region"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan_ad_hoc(&request, &manifest);

        assert!(result.is_ok(), "ad-hoc single dataset should succeed: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.output_names, vec!["date", "region", "revenue"]);
    }

    #[test]
    fn test_ad_hoc_unknown_field_error() {
        let manifest = make_test_manifest();
        let request = make_test_request("", vec!["nonexistent_field"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan_ad_hoc(&request, &manifest);

        assert!(result.is_err(), "unknown field should fail");
    }

    #[test]
    fn test_v2_plan_kind_not_found() {
        let manifest = make_test_manifest();
        let request = make_test_request("nonexistent", vec!["date"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::KindNotFound(_)));
    }

    #[test]
    fn test_plan_computed_dimension() {
        let manifest = make_computed_dim_manifest();
        let request = make_test_request("orders", vec!["date", "market"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let result = planner.plan(&request, &manifest);

        assert!(result.is_ok(), "computed dimension planning should succeed: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.output_names, vec!["date", "market", "revenue"]);

        // The root should be a ProjectNode (possibly wrapped in Sort/Fetch).
        // Verify the computed "market" dimension is a FunctionCall expression,
        // not a plain column reference.
        let project = find_project_node(&plan.root)
            .expect("plan should contain a ProjectNode");
        // market is the 2nd dimension (index 1) in the project expressions
        let market_expr = &project.expressions[1];
        assert!(
            matches!(market_expr, semstrait_ir::Expr::FunctionCall(_)),
            "computed dim 'market' should be a FunctionCall, got: {:?}",
            market_expr
        );
    }

    #[test]
    fn test_plan_computed_dim_not_in_group_by() {
        let manifest = make_computed_dim_manifest();
        // Request only the computed dim + measure (no physical dims except date).
        let request = make_test_request("orders", vec!["date", "market"], vec!["revenue"]);

        let planner = SemanticPlanner::builder().build();
        let plan = planner.plan(&request, &manifest).unwrap();

        // The AggNode should group by only "date" (physical), not "market" (computed).
        let agg = find_agg_node(&plan.root)
            .expect("plan should contain an AggNode");
        assert_eq!(agg.group_by.len(), 1, "only physical dims should be in group_by");
        assert!(
            matches!(&agg.group_by[0], semstrait_ir::Expr::Column(c) if c.name == "order_date"),
            "group_by should contain physical 'order_date', got: {:?}",
            agg.group_by[0]
        );
    }

    fn find_project_node(node: &PlanNode) -> Option<&semstrait_ir::ProjectNode> {
        match node {
            PlanNode::Project(p) => Some(p),
            PlanNode::Sort(n) => find_project_node(&n.input),
            PlanNode::Fetch(n) => find_project_node(&n.input),
            PlanNode::Filter(n) => find_project_node(&n.input),
            _ => None,
        }
    }

    fn find_agg_node(node: &PlanNode) -> Option<&semstrait_ir::AggNode> {
        match node {
            PlanNode::Aggregate(a) => Some(a),
            PlanNode::Project(n) => find_agg_node(&n.input),
            PlanNode::Sort(n) => find_agg_node(&n.input),
            PlanNode::Fetch(n) => find_agg_node(&n.input),
            PlanNode::Filter(n) => find_agg_node(&n.input),
            _ => None,
        }
    }
}
