//! Compilation pipeline steps 3-9.
//!
//! Steps 1 (parse) and 2 (resolve_refs) are handled by semstrait-model.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::DiGraph;

use semstrait_catalog::CatalogProvider;
use semstrait_core::DslExpr;
use semstrait_model::*;

use crate::compiled::*;
use crate::error::CompileError;

// ============================================================================
// Step 3: Expand Globs
// ============================================================================

/// Expand `DatasetName::Glob` entries into concrete `DatasetName::Literal` entries.
///
/// If any dataset uses a GlobPattern and no catalog is provided, returns
/// `CompileError::GlobRequiresCatalog`.
pub(crate) async fn expand_globs(
    mut model: SemanticModel,
    catalog: Option<&dyn CatalogProvider>,
) -> Result<SemanticModel, CompileError> {
    let namespace = model.namespace.as_deref().unwrap_or("default");

    for kind in &mut model.kinds {
        let mut expanded_datasets = Vec::new();
        let mut has_globs = false;

        for entry in kind.datasets.iter() {
            match entry {
                KindDatasetEntry::Inline(ds) => match &ds.name {
                    DatasetName::Glob(pattern) => {
                        has_globs = true;
                        let cat = catalog.ok_or_else(|| CompileError::GlobRequiresCatalog {
                            pattern: pattern.0.clone(),
                            kind: kind.name.clone(),
                        })?;

                        let glob = semstrait_core::GlobPattern::new(&pattern.0);
                        let tables = cat
                            .list_tables(namespace, &glob)
                            .await
                            .map_err(|e| CompileError::CatalogError(e.to_string()))?;

                        for table in tables {
                            let mut new_ds = ds.clone();
                            new_ds.name = DatasetName::Literal(table.name.clone());
                            expanded_datasets.push(KindDatasetEntry::Inline(new_ds));
                        }
                    }
                    DatasetName::Literal(_) => {
                        expanded_datasets.push(entry.clone());
                    }
                },
                KindDatasetEntry::Ref(_) => {
                    expanded_datasets.push(entry.clone());
                }
            }
        }

        if has_globs {
            kind.datasets = expanded_datasets;
        }
    }

    Ok(model)
}

// ============================================================================
// Step 4: Validate Structure
// ============================================================================

/// Validate structural integrity of the model.
pub(crate) fn validate_structure(model: &SemanticModel) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    // Check dataset name uniqueness
    let mut ds_names = HashSet::new();
    for ds in &model.datasets {
        if !ds_names.insert(&ds.name) {
            errors.push(format!("duplicate dataset name: '{}'", ds.name));
        }
    }

    // Check kind name uniqueness
    let mut kind_names = HashSet::new();
    for kind in &model.kinds {
        if !kind_names.insert(&kind.name) {
            errors.push(format!("duplicate kind name: '{}'", kind.name));
        }

        // Kind must have at least one dataset
        if kind.datasets.is_empty() {
            errors.push(format!(
                "kind '{}' must have at least one dataset",
                kind.name
            ));
        }

        // Joinsets must have relationships
        if matches!(kind.kind_type, KindTypeSpec::Joinset(_)) && kind.relationships.is_empty() {
            errors.push(format!(
                "joinset kind '{}' must have at least one relationship",
                kind.name
            ));
        }

        // Check duplicate dimension/measure/metric names within each kind
        check_dim_uniqueness(&kind.dimensions, &kind.name, &mut errors);
        check_measure_uniqueness(&kind.measures, &kind.name, &mut errors);
        check_metric_uniqueness(&kind.metrics, &kind.name, &mut errors);
    }

    // Check dimension/measure/metric uniqueness in top-level datasets
    for ds in &model.datasets {
        check_dim_uniqueness(&ds.dimensions, &ds.name, &mut errors);
        check_measure_uniqueness(&ds.measures, &ds.name, &mut errors);
        check_metric_uniqueness(&ds.metrics, &ds.name, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::StructureValidation(errors))
    }
}

fn check_dim_uniqueness(entries: &[DimensionEntry], container: &str, errors: &mut Vec<String>) {
    let mut names = HashSet::new();
    for entry in entries {
        if let DimensionEntry::Inline(d) = entry {
            if !names.insert(&d.name) {
                errors.push(format!(
                    "duplicate dimension '{}' in '{}'",
                    d.name, container
                ));
            }
        }
    }
}

fn check_measure_uniqueness(entries: &[MeasureEntry], container: &str, errors: &mut Vec<String>) {
    let mut names = HashSet::new();
    for entry in entries {
        if let MeasureEntry::Inline(m) = entry {
            if !names.insert(&m.name) {
                errors.push(format!(
                    "duplicate measure '{}' in '{}'",
                    m.name, container
                ));
            }
        }
    }
}

fn check_metric_uniqueness(entries: &[MetricEntry], container: &str, errors: &mut Vec<String>) {
    let mut names = HashSet::new();
    for entry in entries {
        if let MetricEntry::Inline(m) = entry {
            if !names.insert(&m.name) {
                errors.push(format!(
                    "duplicate metric '{}' in '{}'",
                    m.name, container
                ));
            }
        }
    }
}

// ============================================================================
// Step 5: Validate Mappings
// ============================================================================

/// Validate that column_mapping keys in kind datasets correspond to
/// dimensions/measures/metrics declared in the kind's interface.
pub(crate) fn validate_mappings(model: &SemanticModel) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    for kind in &model.kinds {
        // Build set of interface names (dimensions + measures + metrics)
        let interface_names: HashSet<String> = kind
            .dimensions
            .iter()
            .map(|d| match d {
                DimensionEntry::Inline(dim) => dim.name.clone(),
                DimensionEntry::Ref(r) => r.ref_name.clone(),
            })
            .chain(kind.measures.iter().map(|m| match m {
                MeasureEntry::Inline(mea) => mea.name.clone(),
                MeasureEntry::Ref(r) => r.ref_name.clone(),
            }))
            .chain(kind.metrics.iter().map(|m| match m {
                MetricEntry::Inline(met) => met.name.clone(),
                MetricEntry::Ref(r) => r.ref_name.clone(),
            }))
            .collect();

        // Check each dataset's column_mapping
        for ds_entry in &kind.datasets {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                let ds_display = dataset_display_name(&ds.name);
                for key in ds.extras.column_mapping.keys() {
                    if !interface_names.contains(key) {
                        errors.push(format!(
                            "kind '{}', dataset '{}': column_mapping key '{}' \
                             does not match any dimension, measure, or metric in the interface",
                            kind.name, ds_display, key
                        ));
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::MappingValidation(errors))
    }
}

// ============================================================================
// Step 6: Build Metric Graph
// ============================================================================

const MAX_METRIC_DEPTH: usize = 3;

/// Build metric dependency graph, detect cycles, enforce depth <= 3.
/// Returns a map of metric name -> depth.
pub(crate) fn build_metric_graph(
    model: &SemanticModel,
) -> Result<HashMap<String, usize>, CompileError> {
    let mut depths: HashMap<String, usize> = HashMap::new();

    // Collect all metric/measure names
    let mut metric_names: HashSet<String> = HashSet::new();
    let mut measure_names: HashSet<String> = HashSet::new();

    for m in &model.metrics {
        metric_names.insert(m.name.clone());
    }
    for m in &model.measures {
        measure_names.insert(m.name.clone());
    }
    for ds in &model.datasets {
        for m in &ds.metrics {
            if let MetricEntry::Inline(met) = m {
                metric_names.insert(met.name.clone());
            }
        }
        for m in &ds.measures {
            if let MeasureEntry::Inline(mea) = m {
                measure_names.insert(mea.name.clone());
            }
        }
    }
    for kind in &model.kinds {
        for m in &kind.metrics {
            if let MetricEntry::Inline(met) = m {
                metric_names.insert(met.name.clone());
            }
        }
        for m in &kind.measures {
            if let MeasureEntry::Inline(mea) = m {
                measure_names.insert(mea.name.clone());
            }
        }
    }

    // Build graph nodes for all metrics and measures
    let all_names: Vec<String> = metric_names
        .iter()
        .chain(measure_names.iter())
        .cloned()
        .collect();
    let name_to_idx: HashMap<&str, usize> = all_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let mut graph = DiGraph::<String, ()>::new();
    let nodes: Vec<_> = all_names.iter().map(|n| graph.add_node(n.clone())).collect();

    // Add edges: metric -> its dependencies
    let all_metrics = collect_all_metrics(model);
    for met in &all_metrics {
        if let Some(&src_idx) = name_to_idx.get(met.name.as_str()) {
            let deps = extract_identifiers_from_expr(&met.expr);
            for dep in deps {
                if let Some(&dst_idx) = name_to_idx.get(dep.as_str()) {
                    if src_idx != dst_idx {
                        graph.add_edge(nodes[src_idx], nodes[dst_idx], ());
                    }
                }
            }
        }
    }

    // Check for cycles
    if is_cyclic_directed(&graph) {
        return Err(CompileError::MetricCycle {
            cycle: vec!["(cycle detected in metric graph)".to_string()],
        });
    }

    // Measures have depth 0
    for name in &measure_names {
        depths.insert(name.clone(), 0);
    }

    // Iterative depth computation for metrics
    let mut changed = true;
    while changed {
        changed = false;
        for met in &all_metrics {
            let deps = extract_identifiers_from_expr(&met.expr);
            let max_dep_depth = deps
                .iter()
                .filter_map(|d| depths.get(d.as_str()))
                .max()
                .copied()
                .unwrap_or(0);
            let new_depth = if deps.is_empty() { 0 } else { max_dep_depth + 1 };

            match depths.get(&met.name) {
                Some(&existing) if new_depth <= existing => {}
                _ => {
                    depths.insert(met.name.clone(), new_depth);
                    changed = true;
                }
            }
        }
    }

    // Check depth limit
    for (name, depth) in &depths {
        if *depth > MAX_METRIC_DEPTH {
            return Err(CompileError::MetricDepthExceeded {
                metric: name.clone(),
                depth: *depth,
                max_depth: MAX_METRIC_DEPTH,
            });
        }
    }

    Ok(depths)
}

// ============================================================================
// Step 7: Build Relationship Graph
// ============================================================================

/// Build relationship graph for joinset anchor inference.
/// Returns a map of kind_name -> list of anchor dataset names.
pub(crate) fn build_rel_graph(
    model: &SemanticModel,
) -> Result<HashMap<String, Vec<String>>, CompileError> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();

    for kind in &model.kinds {
        if !matches!(kind.kind_type, KindTypeSpec::Joinset(_)) {
            continue;
        }

        let mut graph = DiGraph::<String, ()>::new();
        let mut node_map: HashMap<String, petgraph::graph::NodeIndex> = HashMap::new();

        // Add nodes for each dataset
        for entry in &kind.datasets {
            if let KindDatasetEntry::Inline(ds) = entry {
                let name = dataset_display_name(&ds.name);
                if !node_map.contains_key(name) {
                    let owned = name.to_string();
                    let idx = graph.add_node(owned.clone());
                    node_map.insert(owned, idx);
                }
            }
        }

        // Add edges from relationships
        for rel in &kind.relationships {
            if let (Some(&from_idx), Some(&to_idx)) =
                (node_map.get(&rel.from), node_map.get(&rel.to))
            {
                graph.add_edge(from_idx, to_idx, ());
            }
        }

        // Infer anchors: nodes with in-degree 0
        let anchors: Vec<String> = graph
            .node_indices()
            .filter(|&n| {
                graph
                    .neighbors_directed(n, petgraph::Direction::Incoming)
                    .count()
                    == 0
            })
            .map(|n| graph[n].clone())
            .collect();

        result.insert(kind.name.clone(), anchors);
    }

    Ok(result)
}

// ============================================================================
// Steps 8-9: Compile Expressions and Emit
// ============================================================================

/// Emit the final CompiledManifest (steps 8 + 9).
pub(crate) fn emit(
    model: SemanticModel,
    source_hash: String,
    metric_depths: &HashMap<String, usize>,
) -> Result<CompiledManifest, CompileError> {
    let mut datasets = IndexMap::new();
    let mut kinds = IndexMap::new();
    let mut relationships = Vec::new();

    for ds in &model.datasets {
        let compiled = compile_dataset(ds)?;
        datasets.insert(ds.name.clone(), compiled);
    }

    for kind in &model.kinds {
        let compiled = compile_kind(kind, metric_depths)?;
        kinds.insert(kind.name.clone(), compiled);
    }

    for rel in &model.relationships {
        relationships.push(CompiledRelationship {
            name: rel.name.clone(),
            from: rel.from.clone(),
            to: rel.to.clone(),
            join_type: rel.join_type,
            columns: rel.columns.clone(),
            cardinality: rel.cardinality,
        });
    }

    Ok(CompiledManifest {
        version: 1,
        compiled_at: chrono::Utc::now(),
        source_hash,
        datasets,
        kinds,
        relationships,
        model_name: model.name,
        model_description: model.description,
    })
}

// ============================================================================
// Internal helpers
// ============================================================================

fn compile_dataset(ds: &Dataset) -> Result<CompiledDataset, CompileError> {
    let mut dimensions = IndexMap::new();
    let mut measures = IndexMap::new();
    let mut metrics = IndexMap::new();

    for d in &ds.dimensions {
        if let DimensionEntry::Inline(dim) = d {
            dimensions.insert(
                dim.name.clone(),
                CompiledDimension {
                    name: dim.name.clone(),
                    description: dim.description.clone(),
                    data_type: dim.data_type.to_string(),
                    dim_type: dim.dim_type.clone(),
                },
            );
        }
    }

    for m in &ds.measures {
        if let MeasureEntry::Inline(mea) = m {
            let expr = parse_expr(&mea.expr, &mea.name)?;
            let filters = compile_measure_filters(&mea.filters)?;
            measures.insert(
                mea.name.clone(),
                CompiledMeasure {
                    name: mea.name.clone(),
                    description: mea.description.clone(),
                    data_type: mea.data_type.to_string(),
                    expr,
                    expr_source: mea.expr.clone(),
                    additivity: mea.additivity.clone(),
                    constraints: mea.constraints.clone(),
                    filters,
                },
            );
        }
    }

    for m in &ds.metrics {
        if let MetricEntry::Inline(met) = m {
            let expr = parse_expr(&met.expr, &met.name)?;
            let filters = compile_measure_filters(&met.filters)?;
            metrics.insert(
                met.name.clone(),
                CompiledMetric {
                    name: met.name.clone(),
                    description: met.description.clone(),
                    data_type: met.data_type.to_string(),
                    expr,
                    expr_source: met.expr.clone(),
                    additivity: met.additivity.clone(),
                    constraints: met.constraints.clone(),
                    filters,
                    depth: 0,
                },
            );
        }
    }

    Ok(CompiledDataset {
        name: ds.name.clone(),
        description: ds.description.clone(),
        domain: ds.domain.as_ref().map(|d| d.0.clone()),
        keys: ds.keys.clone(),
        dimensions,
        measures,
        metrics,
    })
}

fn compile_kind(
    kind: &Kind,
    metric_depths: &HashMap<String, usize>,
) -> Result<CompiledKind, CompileError> {
    let mut dimensions = IndexMap::new();
    let mut measures = IndexMap::new();
    let mut metrics = IndexMap::new();

    for d in &kind.dimensions {
        if let DimensionEntry::Inline(dim) = d {
            dimensions.insert(
                dim.name.clone(),
                CompiledDimension {
                    name: dim.name.clone(),
                    description: dim.description.clone(),
                    data_type: dim.data_type.to_string(),
                    dim_type: dim.dim_type.clone(),
                },
            );
        }
    }

    for m in &kind.measures {
        if let MeasureEntry::Inline(mea) = m {
            let expr = parse_expr(&mea.expr, &mea.name)?;
            let filters = compile_measure_filters(&mea.filters)?;
            measures.insert(
                mea.name.clone(),
                CompiledMeasure {
                    name: mea.name.clone(),
                    description: mea.description.clone(),
                    data_type: mea.data_type.to_string(),
                    expr,
                    expr_source: mea.expr.clone(),
                    additivity: mea.additivity.clone(),
                    constraints: mea.constraints.clone(),
                    filters,
                },
            );
        }
    }

    for m in &kind.metrics {
        if let MetricEntry::Inline(met) = m {
            let expr = parse_expr(&met.expr, &met.name)?;
            let depth = metric_depths.get(&met.name).copied().unwrap_or(0);
            let filters = compile_measure_filters(&met.filters)?;
            metrics.insert(
                met.name.clone(),
                CompiledMetric {
                    name: met.name.clone(),
                    description: met.description.clone(),
                    data_type: met.data_type.to_string(),
                    expr,
                    expr_source: met.expr.clone(),
                    additivity: met.additivity.clone(),
                    constraints: met.constraints.clone(),
                    filters,
                    depth,
                },
            );
        }
    }

    // Compile kind datasets
    let compiled_datasets: Vec<CompiledKindDataset> = kind
        .datasets
        .iter()
        .filter_map(|ds_entry| {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                Some(CompiledKindDataset {
                    name: dataset_display_name(&ds.name).to_string(),
                    extras: ds.extras.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    // Compile kind relationships
    let compiled_rels: Vec<CompiledRelationship> = kind
        .relationships
        .iter()
        .map(|rel| CompiledRelationship {
            name: rel.name.clone(),
            from: rel.from.clone(),
            to: rel.to.clone(),
            join_type: rel.join_type,
            columns: rel.columns.clone(),
            cardinality: rel.cardinality,
        })
        .collect();

    Ok(CompiledKind {
        name: kind.name.clone(),
        description: kind.description.clone(),
        dimensions,
        measures,
        metrics,
        keys: kind.keys.clone(),
        kind_type: CompiledKindType::from(&kind.kind_type),
        datasets: compiled_datasets,
        relationships: compiled_rels,
        domain: kind.domain.as_ref().map(|d| d.0.clone()),
        filters: compile_measure_filters(&kind.filters)?,
    })
}

fn compile_measure_filters(
    filters: &[MeasureFilter],
) -> Result<Vec<CompiledFilter>, CompileError> {
    let mut compiled = Vec::new();
    for mf in filters {
        let expr = parse_expr(&mf.expr, &mf.name)?;
        compiled.push(CompiledFilter::from_measure_filter(mf, expr));
    }
    Ok(compiled)
}

/// Parse a DSL expression string into a DslExpr.
///
/// For v1, we parse common aggregation patterns (SUM, COUNT, etc.)
/// and store other expressions as entity refs. Full DSL parsing
/// will use the semstrait-core DSL lexer/parser when stabilized.
fn parse_expr(expr: &str, entity_name: &str) -> Result<DslExpr, CompileError> {
    let trimmed = expr.trim();

    if trimmed.is_empty() {
        return Err(CompileError::ExprCompilation(vec![format!(
            "empty expression for '{}'",
            entity_name
        )]));
    }

    // Reject raw SQL
    if looks_like_raw_sql(trimmed) {
        return Err(CompileError::RawSqlRejected {
            entity: entity_name.to_string(),
            expr: trimmed.to_string(),
        });
    }

    // Try parsing aggregation patterns: SUM(col), COUNT(col), etc.
    if let Some(parsed) = try_parse_aggregation(trimmed) {
        return Ok(parsed);
    }

    // Try simple arithmetic: a op b (before entity ref, since
    // "{{ a }} - {{ b }}" starts with {{ and ends with }} but is arithmetic)
    if let Some(parsed) = try_parse_arithmetic(trimmed) {
        return Ok(parsed);
    }

    // Try entity ref: {{ name }}
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let inner = trimmed[2..trimmed.len() - 2].trim();
        return Ok(DslExpr::entity_ref(inner));
    }

    // Bare identifier => entity ref
    if is_identifier(trimmed) {
        return Ok(DslExpr::entity_ref(trimmed));
    }

    // Numeric literal
    if let Ok(v) = trimmed.parse::<i64>() {
        return Ok(DslExpr::int(v));
    }
    if let Ok(v) = trimmed.parse::<f64>() {
        return Ok(DslExpr::float(v));
    }

    // Fallback: store as entity ref
    Ok(DslExpr::entity_ref(trimmed))
}

/// SQL keywords that indicate raw SQL (rejected in v1).
const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "JOIN", "UNION",
    "GROUP BY", "ORDER BY", "HAVING", "LIMIT", "CREATE", "ALTER", "DROP",
];

fn looks_like_raw_sql(expr: &str) -> bool {
    let upper = expr.to_uppercase();
    SQL_KEYWORDS.iter().any(|kw| upper.contains(kw))
}

fn try_parse_aggregation(expr: &str) -> Option<DslExpr> {
    let upper = expr.to_uppercase();

    #[allow(clippy::type_complexity)]
    let agg_patterns: &[(&str, fn(DslExpr) -> DslExpr)] = &[
        ("SUM(", DslExpr::sum),
        ("COUNT_DISTINCT(", DslExpr::count_distinct),
        ("COUNT(", DslExpr::count),
        ("AVG(", DslExpr::avg),
        ("MIN(", DslExpr::min),
        ("MAX(", DslExpr::max),
    ];

    for (prefix, constructor) in agg_patterns {
        if upper.starts_with(prefix) && expr.ends_with(')') {
            let inner = expr[prefix.len()..expr.len() - 1].trim();
            let inner_expr = if is_identifier(inner) {
                DslExpr::column(inner)
            } else {
                DslExpr::entity_ref(inner)
            };
            return Some(constructor(inner_expr));
        }
    }

    None
}

fn try_parse_arithmetic(expr: &str) -> Option<DslExpr> {
    let bytes = expr.as_bytes();
    let mut paren_depth = 0i32;
    let mut last_add_sub = None;
    let mut last_mul_div = None;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'+' | b'-' if paren_depth == 0 && i > 0 => {
                last_add_sub = Some(i);
            }
            b'*' | b'/' if paren_depth == 0 && i > 0 => {
                last_mul_div = Some(i);
            }
            _ => {}
        }
    }

    let split_pos = last_add_sub.or(last_mul_div)?;
    let op = bytes[split_pos] as char;
    let left = expr[..split_pos].trim();
    let right = expr[split_pos + 1..].trim();

    if left.is_empty() || right.is_empty() {
        return None;
    }

    let left_expr = parse_operand(left);
    let right_expr = parse_operand(right);

    Some(match op {
        '+' => DslExpr::add(left_expr, right_expr),
        '-' => DslExpr::subtract(left_expr, right_expr),
        '*' => DslExpr::multiply(left_expr, right_expr),
        '/' => DslExpr::divide(left_expr, right_expr),
        _ => return None,
    })
}

fn parse_operand(s: &str) -> DslExpr {
    let trimmed = s.trim();

    // Strip outer parens
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(agg) = try_parse_aggregation(inner) {
            return agg;
        }
        if let Some(arith) = try_parse_arithmetic(inner) {
            return arith;
        }
    }

    if let Some(agg) = try_parse_aggregation(trimmed) {
        return agg;
    }

    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let inner = trimmed[2..trimmed.len() - 2].trim();
        return DslExpr::entity_ref(inner);
    }

    if let Ok(v) = trimmed.parse::<i64>() {
        return DslExpr::int(v);
    }
    if let Ok(v) = trimmed.parse::<f64>() {
        return DslExpr::float(v);
    }

    if is_identifier(trimmed) {
        return DslExpr::entity_ref(trimmed);
    }

    DslExpr::entity_ref(trimmed)
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
}

fn dataset_display_name(name: &DatasetName) -> &str {
    match name {
        DatasetName::Literal(n) => n.as_str(),
        DatasetName::Glob(g) => g.0.as_str(),
    }
}

/// Collect all metrics from all scopes in the model.
fn collect_all_metrics(model: &SemanticModel) -> Vec<&Metric> {
    let mut all = Vec::new();
    all.extend(model.metrics.iter());

    for ds in &model.datasets {
        for m in &ds.metrics {
            if let MetricEntry::Inline(met) = m {
                all.push(met);
            }
        }
    }

    for kind in &model.kinds {
        for m in &kind.metrics {
            if let MetricEntry::Inline(met) = m {
                all.push(met);
            }
        }
    }

    // Deduplicate by name (keep first occurrence)
    let mut seen = HashSet::new();
    all.retain(|m| seen.insert(m.name.clone()));

    all
}

/// Extract identifiers from an expression string.
/// Filters out SQL keywords and numeric literals.
fn extract_identifiers_from_expr(expr: &str) -> Vec<String> {
    let sql_keywords: HashSet<&str> = [
        "sum", "avg", "count", "min", "max", "distinct", "case", "when", "then",
        "else", "end", "and", "or", "not", "null", "true", "false", "is", "in",
        "between", "like", "as", "if", "coalesce", "count_distinct",
    ]
    .into_iter()
    .collect();

    expr.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .filter(|s| !sql_keywords.contains(&s.to_lowercase().as_str()))
        .filter(|s| s.parse::<f64>().is_err())
        .map(String::from)
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_identifiers() {
        let ids = extract_identifiers_from_expr("revenue / order_count");
        assert!(ids.contains(&"revenue".to_string()));
        assert!(ids.contains(&"order_count".to_string()));
    }

    #[test]
    fn test_extract_identifiers_with_aggregates() {
        let ids = extract_identifiers_from_expr("SUM(amount)");
        assert!(ids.contains(&"amount".to_string()));
        assert!(!ids.iter().any(|i| i.to_lowercase() == "sum"));
    }

    #[test]
    fn test_parse_expr_sum() {
        let expr = parse_expr("SUM(amount)", "revenue").unwrap();
        match &expr {
            DslExpr::Sum(agg) => match agg.expr.as_ref() {
                DslExpr::Column(col) => assert_eq!(col.name, "amount"),
                _ => panic!("expected Column inside Sum"),
            },
            _ => panic!("expected Sum, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_expr_count_distinct() {
        let expr = parse_expr("COUNT_DISTINCT(customer_id)", "unique_customers").unwrap();
        assert!(matches!(expr, DslExpr::CountDistinct(_)));
    }

    #[test]
    fn test_parse_expr_entity_ref() {
        let expr = parse_expr("{{ revenue }}", "margin").unwrap();
        match &expr {
            DslExpr::EntityRef(e) => assert_eq!(e.name, "revenue"),
            _ => panic!("expected EntityRef"),
        }
    }

    #[test]
    fn test_parse_expr_arithmetic() {
        let expr = parse_expr("{{ revenue }} - {{ cost }}", "profit").unwrap();
        assert!(matches!(expr, DslExpr::Subtract(_)));
    }

    #[test]
    fn test_parse_expr_reject_raw_sql() {
        let result = parse_expr("SELECT sum(amount) FROM orders", "bad_metric");
        assert!(matches!(result, Err(CompileError::RawSqlRejected { .. })));
    }

    #[test]
    fn test_validate_structure_duplicate_dataset() {
        let model = SemanticModel {
            name: "test".to_string(),
            description: None,
            ai_context: None,
            labels: vec![],
            namespace: None,
            datasets: vec![
                Dataset {
                    name: "orders".to_string(),
                    description: None,
                    domain: None,
                    keys: None,
                    dimensions: vec![],
                    measures: vec![],
                    metrics: vec![],
                    filters: vec![],
                    extras: None,
                },
                Dataset {
                    name: "orders".to_string(),
                    description: None,
                    domain: None,
                    keys: None,
                    dimensions: vec![],
                    measures: vec![],
                    metrics: vec![],
                    filters: vec![],
                    extras: None,
                },
            ],
            kinds: vec![],
            relationships: vec![],
            dimensions: vec![],
            measures: vec![],
            metrics: vec![],
        };

        let result = validate_structure(&model);
        assert!(matches!(result, Err(CompileError::StructureValidation(_))));
    }

    #[test]
    fn test_validate_structure_empty_kind() {
        let model = SemanticModel {
            name: "test".to_string(),
            description: None,
            ai_context: None,
            labels: vec![],
            namespace: None,
            datasets: vec![],
            kinds: vec![Kind {
                name: "empty_kind".to_string(),
                description: None,
                kind_type: KindTypeSpec::Grainset,
                domain: None,
                keys: None,
                dimensions: vec![],
                measures: vec![],
                metrics: vec![],
                datasets: vec![],
                relationships: vec![],
                filters: vec![],
            }],
            relationships: vec![],
            dimensions: vec![],
            measures: vec![],
            metrics: vec![],
        };

        let result = validate_structure(&model);
        assert!(matches!(result, Err(CompileError::StructureValidation(_))));
    }

    #[test]
    fn test_is_identifier() {
        assert!(is_identifier("revenue"));
        assert!(is_identifier("order_count"));
        assert!(is_identifier("_private"));
        assert!(!is_identifier("123abc"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("a b"));
    }
}
