//! Semantic compiler — the public API for compiling semantic queries.
//!
//! Takes a semantic query (dimensions + measures + filters), loads the model
//! from a registry, resolves kinds, checks constraints, and produces a
//! `CompiledPlan` with SQL and/or Substrait output.

use crate::diagnostics::{codes, CompileError, Diagnostic, ValidationReport};
use crate::output::{ColumnRole, CompileOpts, CompiledPlan, OutputColumn};
use crate::registry::{ModelRef, SchemaRegistry};
use crate::schema::model::{
    DimensionEntry, Kind, MeasureEntry, MetricEntry, SemanticModel,
};
use crate::planner::validate::constraints::{self, QueryContext};
use crate::planner::validate::domain;
use crate::planner::resolve::{self as kind_resolver, QueryRequest};
use crate::planner::build as plan_builder;
use crate::planner::emit::sql as sql_emitter;
use crate::planner::emit::substrait as substrait_conv;

/// A semantic query to compile.
#[derive(Debug, Clone)]
pub struct SemanticQuery {
    /// Model to query.
    pub model: ModelRef,
    /// Requested dimension names.
    pub dimensions: Vec<String>,
    /// Requested measure names.
    pub measures: Vec<String>,
    /// Requested metric names.
    pub metrics: Vec<String>,
    /// Optional domain filter.
    pub domain: Option<String>,
    /// Optional aggregation override.
    pub aggregation: Option<String>,
    /// Runtime user attributes for row-level security filters.
    #[allow(clippy::zero_sized_map_values)]
    pub user_attributes: std::collections::HashMap<String, String>,
}

/// Trait for semantic compilation.
pub trait SemanticCompiler {
    fn compile(
        &self,
        query: &SemanticQuery,
        opts: &CompileOpts,
    ) -> Result<CompiledPlan, CompileError>;
}

/// Stateless compiler — no caching, loads model fresh each time.
pub struct StatelessCompiler<R: SchemaRegistry> {
    registry: R,
}

impl<R: SchemaRegistry> StatelessCompiler<R> {
    pub fn new(registry: R) -> Self {
        Self { registry }
    }
}

impl<R: SchemaRegistry> SemanticCompiler for StatelessCompiler<R> {
    fn compile(
        &self,
        query: &SemanticQuery,
        opts: &CompileOpts,
    ) -> Result<CompiledPlan, CompileError> {
        // 1. Load model
        let model_file = self.registry.load(&query.model)?;
        let model = &model_file.semantic_model;

        // 2. Build output column metadata
        let columns = build_output_columns(model, query);

        // 3. Try kind-based resolution first
        if let Some(kinds) = &model.kinds {
            if let Some(kind) = find_matching_kind(kinds, query) {
                return compile_kind(kind, query, opts, columns);
            }
        }

        // 4. Fallback: dataset-based resolution
        compile_datasets(model, query, opts, columns)
    }
}

fn build_output_columns(model: &SemanticModel, query: &SemanticQuery) -> Vec<OutputColumn> {
    let mut columns = Vec::new();

    for dim_name in &query.dimensions {
        let data_type = find_dimension_type(model, dim_name).unwrap_or_else(|| "string".into());
        columns.push(OutputColumn {
            name: dim_name.clone(),
            data_type,
            role: ColumnRole::Dimension,
        });
    }

    for measure_name in &query.measures {
        let data_type =
            find_measure_type(model, measure_name).unwrap_or_else(|| "float64".into());
        columns.push(OutputColumn {
            name: measure_name.clone(),
            data_type,
            role: ColumnRole::Measure,
        });
    }

    for metric_name in &query.metrics {
        let data_type = find_metric_type(model, metric_name).unwrap_or_else(|| "float64".into());
        columns.push(OutputColumn {
            name: metric_name.clone(),
            data_type,
            role: ColumnRole::Metric,
        });
    }

    columns
}

fn find_dimension_type(model: &SemanticModel, name: &str) -> Option<String> {
    // Search top-level dimensions
    if let Some(dims) = &model.dimensions {
        for d in dims {
            if d.name == name {
                return Some(d.data_type.to_string());
            }
        }
    }
    // Search dataset dimensions
    if let Some(datasets) = &model.datasets {
        for ds in datasets {
            if let Some(dims) = &ds.dimensions {
                for entry in dims {
                    if let DimensionEntry::Inline(d) = entry {
                        if d.name == name {
                            return Some(d.data_type.to_string());
                        }
                    }
                }
            }
        }
    }
    // Search kind dimensions
    if let Some(kinds) = &model.kinds {
        for kind in kinds {
            if let Some(dims) = &kind.dimensions {
                for entry in dims {
                    if let DimensionEntry::Inline(d) = entry {
                        if d.name == name {
                            return Some(d.data_type.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_measure_type(model: &SemanticModel, name: &str) -> Option<String> {
    if let Some(measures) = &model.measures {
        for m in measures {
            if m.name == name {
                return Some(m.data_type.to_string());
            }
        }
    }
    if let Some(datasets) = &model.datasets {
        for ds in datasets {
            if let Some(measures) = &ds.measures {
                for entry in measures {
                    if let MeasureEntry::Inline(m) = entry {
                        if m.name == name {
                            return Some(m.data_type.to_string());
                        }
                    }
                }
            }
        }
    }
    if let Some(kinds) = &model.kinds {
        for kind in kinds {
            if let Some(measures) = &kind.measures {
                for entry in measures {
                    if let MeasureEntry::Inline(m) = entry {
                        if m.name == name {
                            return Some(m.data_type.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_metric_type(model: &SemanticModel, name: &str) -> Option<String> {
    if let Some(metrics) = &model.metrics {
        for m in metrics {
            if m.name == name {
                return Some(m.data_type.to_string());
            }
        }
    }
    if let Some(kinds) = &model.kinds {
        for kind in kinds {
            if let Some(metrics) = &kind.metrics {
                for entry in metrics {
                    if let MetricEntry::Inline(m) = entry {
                        if m.name == name {
                            return Some(m.data_type.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find the best matching kind for a query (by column availability and domain).
fn find_matching_kind<'a>(kinds: &'a [Kind], query: &SemanticQuery) -> Option<&'a Kind> {
    for kind in kinds {
        // Domain pre-filter
        if !domain::domain_matches(kind.domain.as_ref(), query.domain.as_deref()) {
            continue;
        }
        let has_dims = query.dimensions.iter().all(|d| kind_has_dimension(kind, d));
        let has_measures = query.measures.iter().all(|m| kind_has_measure(kind, m));
        let has_metrics = query.metrics.iter().all(|m| kind_has_metric(kind, m));
        if has_dims && has_measures && has_metrics {
            return Some(kind);
        }
    }
    None
}

fn kind_has_dimension(kind: &Kind, name: &str) -> bool {
    kind.dimensions.as_ref().is_some_and(|dims| {
        dims.iter().any(|entry| match entry {
            DimensionEntry::Inline(d) => d.name == name,
            DimensionEntry::Ref(r) => r.ref_name == name,
        })
    })
}

fn kind_has_measure(kind: &Kind, name: &str) -> bool {
    kind.measures.as_ref().is_some_and(|measures| {
        measures.iter().any(|entry| match entry {
            MeasureEntry::Inline(m) => m.name == name,
            MeasureEntry::Ref(r) => r.ref_name == name,
        })
    })
}

fn kind_has_metric(kind: &Kind, name: &str) -> bool {
    kind.metrics.as_ref().is_some_and(|metrics| {
        metrics.iter().any(|entry| match entry {
            MetricEntry::Inline(m) => m.name == name,
            MetricEntry::Ref(r) => r.ref_name == name,
        })
    })
}

/// Check measure constraints against the query context.
fn check_measure_constraints(
    kind: &Kind,
    measure_name: &str,
    query: &SemanticQuery,
    report: &mut ValidationReport,
) {
    if let Some(measures) = &kind.measures {
        for entry in measures {
            if let MeasureEntry::Inline(m) = entry {
                if m.name == measure_name {
                    if let Some(c) = &m.constraints {
                        let ctx = QueryContext {
                            dimensions: &query.dimensions,
                            aggregation: query.aggregation.as_deref(),
                        };
                        let sub_report = constraints::check_constraints(&m.name, c, &ctx);
                        if sub_report.has_errors() {
                            let sub_err = sub_report.finish().unwrap_err();
                            for d in sub_err.diagnostics() {
                                report.push(d.clone());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Find a metric in the kind and return its inline definition.
fn find_kind_metric_def<'a>(kind: &'a Kind, name: &str) -> Option<&'a crate::schema::model::Metric> {
    kind.metrics.as_ref()?.iter().find_map(|entry| match entry {
        MetricEntry::Inline(m) if m.name == name => Some(m),
        _ => None,
    })
}

/// Find a measure in the kind and return its inline definition.
fn find_kind_measure_def<'a>(kind: &'a Kind, name: &str) -> Option<&'a crate::schema::model::Measure> {
    kind.measures.as_ref()?.iter().find_map(|entry| match entry {
        MeasureEntry::Inline(m) if m.name == name => Some(m),
        _ => None,
    })
}

fn compile_kind(
    kind: &Kind,
    query: &SemanticQuery,
    opts: &CompileOpts,
    columns: Vec<OutputColumn>,
) -> Result<CompiledPlan, CompileError> {
    let mut report = ValidationReport::new();

    // Check constraints on requested measures
    for measure_name in &query.measures {
        check_measure_constraints(kind, measure_name, query, &mut report);
    }

    // Check constraints on metrics: inherit from dependent measures + own constraints
    for metric_name in &query.metrics {
        // Check the metric's own constraints (if any)
        if let Some(metric) = find_kind_metric_def(kind, metric_name) {
            if let Some(c) = &metric.constraints {
                let ctx = QueryContext {
                    dimensions: &query.dimensions,
                    aggregation: query.aggregation.as_deref(),
                };
                let sub_report = constraints::check_constraints(&metric.name, c, &ctx);
                if sub_report.has_errors() {
                    let sub_err = sub_report.finish().unwrap_err();
                    for d in sub_err.diagnostics() {
                        report.push(d.clone());
                    }
                }
            }

            // Check constraints inherited from dependent measures
            if let Ok(ast) = crate::dsl::parse_dsl(&metric.expr) {
                if let Ok(expr) = crate::dsl::lower_expr(&ast) {
                    let refs = collect_metric_measure_refs(&expr);
                    for dep_name in refs {
                        if find_kind_measure_def(kind, &dep_name).is_some() {
                            check_measure_constraints(kind, &dep_name, query, &mut report);
                        }
                    }
                }
            }
        }
    }

    report.finish_discard_warnings()?;

    // Resolve kind
    let request = QueryRequest {
        dimensions: query.dimensions.clone(),
        measures: query.measures.clone(),
        metrics: query.metrics.clone(),
        domain: query.domain.clone(),
        aggregation: query.aggregation.clone(),
    };

    let resolved = kind_resolver::resolve_kind(kind, &request)?;

    // Build PlanNode tree from resolved kind
    let plan_node = plan_builder::build_plan(
        kind,
        &resolved,
        &query.dimensions,
        &query.measures,
        &query.metrics,
    )?;

    // Emit SQL
    let sql = if opts.emit_sql {
        Some(sql_emitter::emit_sql(&plan_node, None).map_err(CompileError::from)?)
    } else {
        None
    };

    // Emit Substrait
    let substrait = if opts.emit_substrait {
        Some(emit_substrait_bytes(&plan_node, &columns)?)
    } else {
        None
    };

    Ok(CompiledPlan {
        sql,
        substrait,
        columns,
        warnings: vec![],
    })
}

fn compile_datasets(
    model: &SemanticModel,
    query: &SemanticQuery,
    opts: &CompileOpts,
    columns: Vec<OutputColumn>,
) -> Result<CompiledPlan, CompileError> {
    let datasets = model.datasets.as_ref().ok_or_else(|| {
        CompileError::single(Diagnostic::error(
            codes::PLAN_E001,
            format!("model '{}': no datasets or kinds available", model.name),
        ))
    })?;

    if datasets.is_empty() {
        return Err(CompileError::single(Diagnostic::error(
            codes::PLAN_E001,
            format!("model '{}': no datasets or kinds available", model.name),
        )));
    }

    // Find a dataset that has all requested dimensions and measures
    let ds = datasets
        .iter()
        .find(|ds| {
            let has_dims = query.dimensions.iter().all(|d| dataset_has_dimension(ds, d));
            let has_measures = query.measures.iter().all(|m| dataset_has_measure(ds, m));
            has_dims && has_measures
        })
        .ok_or_else(|| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E001,
                format!(
                    "model '{}': no dataset covers all requested columns",
                    model.name
                ),
            ))
        })?;

    if !opts.emit_sql && !opts.emit_substrait {
        return Ok(CompiledPlan {
            sql: None,
            substrait: None,
            columns,
            warnings: vec![],
        });
    }

    // Build plan: Scan → Aggregate → Project
    use crate::planner::ir::expr::{AggregateExpr, Column};
    use crate::planner::ir::plan_node::*;
    use crate::dsl;

    // Table path
    let table = ds
        .extras
        .as_ref()
        .and_then(|e| e.storage.as_ref())
        .map(|s| s.path.clone())
        .unwrap_or_else(|| ds.name.clone());

    // Scan columns: dimensions + measure source columns (semantic names, no mapping)
    let mut scan_cols = Vec::new();
    let mut scan_types = Vec::new();
    for dim_name in &query.dimensions {
        scan_cols.push(dim_name.clone());
        scan_types.push(
            find_dataset_dimension(ds, dim_name)
                .map(|d| d.data_type.to_string())
                .unwrap_or_else(|| "string".into()),
        );
    }
    // For measures, add the columns referenced in the expression
    // (semantic names — dataset columns are named the same)
    for m_name in &query.measures {
        if let Some(m) = find_dataset_measure(ds, m_name) {
            if let Ok(ast) = dsl::parse_dsl(&m.expr) {
                if let Ok((_, inner, _)) = dsl::lower_aggregate(&ast, m_name) {
                    collect_column_names(&inner, &mut scan_cols, &mut scan_types, ds);
                }
            }
        }
    }

    let mut node = PlanNode::Scan(
        Scan::new(&table)
            .with_alias(&ds.name)
            .with_columns(scan_cols, scan_types),
    );

    // Inject dataset-level filters (including RLS user attribute filters)
    node = inject_dataset_filters(node, ds, query)?;

    // Aggregate: GROUP BY dimensions, aggregate measures
    let group_by: Vec<Column> = query
        .dimensions
        .iter()
        .map(Column::unqualified)
        .collect();

    let mut aggregates = Vec::new();
    for m_name in &query.measures {
        if let Some(m) = find_dataset_measure(ds, m_name) {
            let ast = dsl::parse_dsl(&m.expr).map_err(|e| {
                CompileError::single(Diagnostic::error(
                    codes::PLAN_E004,
                    format!("measure '{}': DSL parse error: {}", m_name, e),
                ))
            })?;
            let (agg, inner, alias) = dsl::lower_aggregate(&ast, m_name).map_err(|e| {
                CompileError::single(Diagnostic::error(
                    codes::PLAN_E004,
                    format!("measure '{}': lower error: {}", m_name, e),
                ))
            })?;
            aggregates.push(AggregateExpr {
                func: agg,
                expr: inner,
                alias,
            });
        }
    }

    let node = PlanNode::Aggregate(Aggregate {
        input: Box::new(node),
        group_by,
        aggregates,
    });

    let sql = if opts.emit_sql {
        Some(sql_emitter::emit_sql(&node, None).map_err(CompileError::from)?)
    } else {
        None
    };

    let substrait = if opts.emit_substrait {
        Some(emit_substrait_bytes(&node, &columns)?)
    } else {
        None
    };

    Ok(CompiledPlan {
        sql,
        substrait,
        columns,
        warnings: vec![],
    })
}

fn emit_substrait_bytes(
    plan_node: &crate::planner::ir::plan_node::PlanNode,
    columns: &[OutputColumn],
) -> Result<Vec<u8>, CompileError> {
    use prost::Message;
    let output_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    let plan = substrait_conv::emit_plan(plan_node, Some(output_names))
        .map_err(CompileError::from)?;
    Ok(plan.encode_to_vec())
}

/// Inject dataset-level filters (static and user-attribute-based) as Filter nodes.
fn inject_dataset_filters(
    mut node: crate::planner::ir::plan_node::PlanNode,
    ds: &crate::schema::model::Dataset,
    query: &SemanticQuery,
) -> Result<crate::planner::ir::plan_node::PlanNode, CompileError> {
    use crate::planner::ir::plan_node::Filter;
    use crate::dsl;

    let filters = match &ds.filters {
        Some(f) if !f.is_empty() => f,
        _ => return Ok(node),
    };

    for filter in filters {
        // Resolve the expression: substitute {user_attribute} if present
        let resolved_expr = if let Some(attr_name) = &filter.user_attribute {
            let attr_value = query.user_attributes.get(attr_name).ok_or_else(|| {
                CompileError::single(Diagnostic::error(
                    codes::ATTR_E001,
                    format!(
                        "dataset '{}': filter '{}' requires user attribute '{}' but it was not provided",
                        ds.name, filter.name, attr_name
                    ),
                ))
            })?;
            // Replace {user_attribute} placeholder with a quoted literal value
            filter.expr.replace("{user_attribute}", &format!("'{}'", attr_value.replace('\'', "''")))
        } else {
            filter.expr.clone()
        };

        // Parse and lower the filter expression
        let dsl_ast = dsl::parse_dsl(&resolved_expr).map_err(|e| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!(
                    "dataset '{}': filter '{}' parse error: {}",
                    ds.name, filter.name, e
                ),
            ))
        })?;
        let predicate = dsl::lower_expr(&dsl_ast).map_err(|e| {
            CompileError::single(Diagnostic::error(
                codes::PLAN_E004,
                format!(
                    "dataset '{}': filter '{}' lower error: {}",
                    ds.name, filter.name, e
                ),
            ))
        })?;

        node = crate::planner::ir::plan_node::PlanNode::Filter(Filter {
            input: Box::new(node),
            predicate,
        });
    }

    Ok(node)
}

fn dataset_has_dimension(ds: &crate::schema::model::Dataset, name: &str) -> bool {
    ds.dimensions.as_ref().is_some_and(|dims| {
        dims.iter().any(|entry| match entry {
            DimensionEntry::Inline(d) => d.name == name,
            DimensionEntry::Ref(r) => r.ref_name == name,
        })
    })
}

fn dataset_has_measure(ds: &crate::schema::model::Dataset, name: &str) -> bool {
    ds.measures.as_ref().is_some_and(|measures| {
        measures.iter().any(|entry| match entry {
            MeasureEntry::Inline(m) => m.name == name,
            MeasureEntry::Ref(r) => r.ref_name == name,
        })
    })
}

fn find_dataset_dimension<'a>(
    ds: &'a crate::schema::model::Dataset,
    name: &str,
) -> Option<&'a crate::schema::model::Dimension> {
    ds.dimensions.as_ref()?.iter().find_map(|entry| match entry {
        DimensionEntry::Inline(d) if d.name == name => Some(d),
        _ => None,
    })
}

fn find_dataset_measure<'a>(
    ds: &'a crate::schema::model::Dataset,
    name: &str,
) -> Option<&'a crate::schema::model::Measure> {
    ds.measures.as_ref()?.iter().find_map(|entry| match entry {
        MeasureEntry::Inline(m) if m.name == name => Some(m),
        _ => None,
    })
}

/// Collect column names from an Expr into the scan columns list (deduplicating).
fn collect_column_names(
    expr: &crate::planner::ir::expr::Expr,
    cols: &mut Vec<String>,
    types: &mut Vec<String>,
    ds: &crate::schema::model::Dataset,
) {
    use crate::planner::ir::expr::Expr;
    match expr {
        Expr::Column(col) => {
            if !cols.contains(&col.name) {
                let dt = find_dataset_dimension(ds, &col.name)
                    .map(|d| d.data_type.to_string())
                    .unwrap_or_else(|| "string".into());
                cols.push(col.name.clone());
                types.push(dt);
            }
        }
        Expr::Add(l, r) | Expr::Subtract(l, r) | Expr::Multiply(l, r) | Expr::Divide(l, r) => {
            collect_column_names(l, cols, types, ds);
            collect_column_names(r, cols, types, ds);
        }
        _ => {}
    }
}

/// Collect unqualified column names referenced in an expression.
fn collect_metric_measure_refs(expr: &crate::planner::ir::expr::Expr) -> Vec<String> {
    use crate::planner::ir::expr::Expr;
    let mut refs = Vec::new();
    fn walk(e: &Expr, refs: &mut Vec<String>) {
        match e {
            Expr::Column(col) if col.table.is_empty() => {
                if !refs.contains(&col.name) {
                    refs.push(col.name.clone());
                }
            }
            Expr::Add(l, r) | Expr::Subtract(l, r) | Expr::Multiply(l, r) | Expr::Divide(l, r) => {
                walk(l, refs);
                walk(r, refs);
            }
            _ => {}
        }
    }
    walk(expr, &mut refs);
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{FileSystemRegistry, InMemoryRegistry};

    fn test_registry() -> FileSystemRegistry {
        let base = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data"
        ));
        FileSystemRegistry::new(base)
    }

    #[test]
    fn test_compile_grainset_kind() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("grainset_basic"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        assert!(plan.sql.is_some());
        assert_eq!(plan.columns.len(), 2);
        assert_eq!(plan.columns[0].role, ColumnRole::Dimension);
        assert_eq!(plan.columns[1].role, ColumnRole::Measure);
    }

    #[test]
    fn test_compile_unionset_kind() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("unionset_basic"),
            dimensions: vec!["event_date".into()],
            measures: vec!["event_count".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        assert!(plan.sql.is_some());
    }

    #[test]
    fn test_compile_joinset_kind() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("joinset_basic"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        assert!(plan.sql.is_some());
    }

    #[test]
    fn test_compile_minimal_dataset() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("minimal"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        assert!(plan.sql.is_some());
        assert_eq!(plan.columns.len(), 2);
    }

    #[test]
    fn test_compile_missing_model() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("nonexistent"),
            dimensions: vec![],
            measures: vec![],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let err = compiler.compile(&query, &CompileOpts::default()).unwrap_err();
        assert!(err.to_string().contains("PARSE_E001"));
    }

    #[test]
    fn test_compile_with_in_memory_registry() {
        let yaml = r#"
semantic_model:
  name: mem_test
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
        let model = crate::parser::parse_str(yaml).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("mem_test", model);
        let compiler = StatelessCompiler::new(registry);

        let query = SemanticQuery {
            model: ModelRef::new("mem_test"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        assert_eq!(plan.columns.len(), 2);
    }

    #[test]
    fn test_metric_inherits_measure_constraint_violated() {
        // revenue requires order_date dimension; metric references revenue
        // querying the metric without order_date should fail
        let yaml = r#"
semantic_model:
  name: constrained_metrics
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
          constraints:
            dimensions:
              all:
                - order_date
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
              region: region
              revenue: amount
              order_count: order_id
            storage:
              path: warehouse.orders
"#;
        let model = crate::parser::parse_str(yaml).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("constrained_metrics", model);
        let compiler = StatelessCompiler::new(registry);

        // Query metric without required dimension → should fail
        let err = compiler
            .compile(
                &SemanticQuery {
                    model: ModelRef::new("constrained_metrics"),
                    dimensions: vec!["region".into()],
                    measures: vec![],
                    metrics: vec!["avg_order_value".into()],
                    domain: None,
                    aggregation: None,
                    user_attributes: Default::default(),
                },
                &CompileOpts::default(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("CONST_E"), "should fail with constraint error: {err}");
    }

    #[test]
    fn test_metric_inherits_measure_constraint_satisfied() {
        let yaml = r#"
semantic_model:
  name: constrained_metrics_ok
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
          constraints:
            dimensions:
              all:
                - order_date
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
              region: region
              revenue: amount
              order_count: order_id
            storage:
              path: warehouse.orders
"#;
        let model = crate::parser::parse_str(yaml).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("constrained_metrics_ok", model);
        let compiler = StatelessCompiler::new(registry);

        // Query metric with required dimension → should succeed
        let plan = compiler
            .compile(
                &SemanticQuery {
                    model: ModelRef::new("constrained_metrics_ok"),
                    dimensions: vec!["order_date".into()],
                    measures: vec![],
                    metrics: vec!["avg_order_value".into()],
                    domain: None,
                    aggregation: None,
                    user_attributes: Default::default(),
                },
                &CompileOpts::default(),
            )
            .unwrap();
        assert!(plan.sql.is_some());
    }

    #[test]
    fn test_substrait_output_from_kind() {
        // Use InMemory model where DSL column names match physical columns
        let yaml = r#"
semantic_model:
  name: substrait_kind
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
              order_date: order_date
              revenue: amount
            storage:
              path: warehouse.orders
"#;
        let model = crate::parser::parse_str(yaml).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("substrait_kind", model);
        let compiler = StatelessCompiler::new(registry);

        let query = SemanticQuery {
            model: ModelRef::new("substrait_kind"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let opts = CompileOpts {
            emit_sql: false,
            emit_substrait: true,
            ..CompileOpts::default()
        };
        let plan = compiler.compile(&query, &opts).unwrap();
        assert!(plan.sql.is_none());
        assert!(plan.substrait.is_some());
        // Verify the bytes decode as a valid Substrait plan
        let bytes = plan.substrait.as_ref().unwrap();
        assert!(!bytes.is_empty());
        use prost::Message;
        let decoded = substrait::proto::Plan::decode(bytes.as_slice());
        assert!(decoded.is_ok(), "Substrait bytes should decode: {:?}", decoded.err());
    }

    #[test]
    fn test_substrait_output_from_dataset() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("minimal"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let opts = CompileOpts {
            emit_sql: false,
            emit_substrait: true,
            ..CompileOpts::default()
        };
        let plan = compiler.compile(&query, &opts).unwrap();
        assert!(plan.sql.is_none());
        assert!(plan.substrait.is_some());
    }

    #[test]
    fn test_both_sql_and_substrait() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("minimal"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let opts = CompileOpts {
            emit_sql: true,
            emit_substrait: true,
            ..CompileOpts::default()
        };
        let plan = compiler.compile(&query, &opts).unwrap();
        assert!(plan.sql.is_some());
        assert!(plan.substrait.is_some());
    }

    #[test]
    fn test_neither_sql_nor_substrait() {
        let compiler = StatelessCompiler::new(test_registry());
        let query = SemanticQuery {
            model: ModelRef::new("grainset_basic"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let opts = CompileOpts {
            emit_sql: false,
            emit_substrait: false,
            ..CompileOpts::default()
        };
        let plan = compiler.compile(&query, &opts).unwrap();
        assert!(plan.sql.is_none());
        assert!(plan.substrait.is_none());
    }

    fn dataset_with_filters_yaml() -> &'static str {
        r#"
semantic_model:
  name: filtered_model
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
      filters:
        - name: tenant_filter
          expr: "tenant_id = {user_attribute}"
          user_attribute: tenant_id
        - name: active_filter
          expr: "status = 'active'"
"#
    }

    #[test]
    fn test_user_attribute_filter_injected() {
        let model = crate::parser::parse_str(dataset_with_filters_yaml()).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("filtered_model", model);
        let compiler = StatelessCompiler::new(registry);

        let mut attrs = std::collections::HashMap::new();
        attrs.insert("tenant_id".into(), "acme".into());
        let query = SemanticQuery {
            model: ModelRef::new("filtered_model"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: attrs,
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        let sql = plan.sql.as_ref().unwrap();
        assert!(sql.contains("WHERE"), "SQL should have WHERE clause: {sql}");
        assert!(sql.contains("'acme'"), "SQL should contain the user attribute value: {sql}");
    }

    #[test]
    fn test_user_attribute_missing_error() {
        let model = crate::parser::parse_str(dataset_with_filters_yaml()).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("filtered_model", model);
        let compiler = StatelessCompiler::new(registry);

        let query = SemanticQuery {
            model: ModelRef::new("filtered_model"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let err = compiler.compile(&query, &CompileOpts::default()).unwrap_err();
        assert!(err.to_string().contains("ATTR_E001"), "should fail with ATTR_E001: {err}");
    }

    #[test]
    fn test_static_dataset_filter() {
        let yaml = r#"
semantic_model:
  name: static_filter_model
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
      filters:
        - name: active_only
          expr: "status = 'active'"
"#;
        let model = crate::parser::parse_str(yaml).unwrap();
        let mut registry = InMemoryRegistry::new();
        registry.insert("static_filter_model", model);
        let compiler = StatelessCompiler::new(registry);

        let query = SemanticQuery {
            model: ModelRef::new("static_filter_model"),
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
            user_attributes: Default::default(),
        };
        let plan = compiler.compile(&query, &CompileOpts::default()).unwrap();
        let sql = plan.sql.as_ref().unwrap();
        assert!(sql.contains("WHERE"), "SQL should have WHERE clause: {sql}");
        assert!(sql.contains("'active'"), "SQL should contain filter value: {sql}");
    }
}
