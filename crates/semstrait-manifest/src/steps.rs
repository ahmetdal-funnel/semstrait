//! Compilation pipeline steps 3-9.
//!
//! Steps 1 (parse) and 2 (resolve_refs) are handled by semstrait-model.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::DiGraph;

use semstrait_catalog::CatalogProvider;
use semstrait_core::Expr;
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

    // Check that dataset and kind names don't overlap (unique entity namespace).
    for ds_name in &ds_names {
        if kind_names.contains(ds_name) {
            errors.push(format!(
                "name '{}' is used as both a dataset and a kind; all entity names must be unique",
                ds_name
            ));
        }
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
// Step 4.6: Validate Temporal Equivalence
// ============================================================================

/// Validate that when both a kind and a dataset define a temporal type,
/// their temporal variant (timeseries/snapshot/scd) must match.
///
/// This must run BEFORE `expand_auto_mappings` because that step propagates
/// kind-level temporal defaults, which would overwrite dataset values.
pub(crate) fn validate_temporal_equivalence(
    model: &SemanticModel,
) -> Result<(), CompileError> {
    for kind in &model.kinds {
        let kind_temporal = kind
            .extras
            .as_ref()
            .and_then(|e| e.temporal.as_ref());

        let kind_temporal = match kind_temporal {
            Some(t) => t,
            None => continue, // No kind-level temporal; nothing to conflict with.
        };

        for ds_entry in &kind.datasets {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                if let Some(ds_temporal) = &ds.extras.temporal {
                    let kind_variant = kind_temporal.temporal_type.variant_name();
                    let ds_variant = ds_temporal.temporal_type.variant_name();
                    if kind_variant != ds_variant {
                        return Err(CompileError::TemporalMismatch {
                            kind: kind.name.clone(),
                            dataset: dataset_display_name(&ds.name).to_string(),
                            kind_type: kind_variant.to_string(),
                            dataset_type: ds_variant.to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Step 4.7: Validate Storage Config
// ============================================================================

/// Validate storage config preconditions: paths/tables mutually exclusive,
/// at least one source when storage is defined, no empty strings.
pub(crate) fn validate_storage(model: &SemanticModel) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    for kind in &model.kinds {
        for ds_entry in &kind.datasets {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                if let Some(ref storage) = ds.extras.storage {
                    let sources = storage.all_sources();
                    let ds_display = dataset_display_name(&ds.name);

                    if sources.is_mixed() {
                        errors.push(format!(
                            "kind '{}', dataset '{}': storage cannot mix paths and tables",
                            kind.name, ds_display
                        ));
                    }
                    if sources.is_empty() {
                        errors.push(format!(
                            "kind '{}', dataset '{}': storage must specify at least one path or table",
                            kind.name, ds_display
                        ));
                    }
                    for src in sources.all() {
                        if src.trim().is_empty() {
                            errors.push(format!(
                                "kind '{}', dataset '{}': storage source must not be empty",
                                kind.name, ds_display
                            ));
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::StructureValidation(errors))
    }
}

// ============================================================================
// Step 4.8: Validate Metadata Dimensions
// ============================================================================

/// Validate that metadata dimensions have the required preconditions:
/// - `path.token` requires storage config with at least one path (file/object store).
/// - `partition.level` requires partition_defs on the dataset (or kind) extras.
/// - `partition.level` must not exceed the partition depth.
pub(crate) fn validate_metadata_dimensions(model: &SemanticModel) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    for kind in &model.kinds {
        // Collect kind-level dimensions that are metadata type.
        for dim in &kind.dimensions {
            if let DimensionEntry::Inline(dim_def) = dim {
                if let DimensionType::Metadata(ref meta) = dim_def.dim_type {
                    // Check each dataset for metadata dimension preconditions.
                    for ds_entry in &kind.datasets {
                        if let KindDatasetEntry::Inline(ds) = ds_entry {
                            let ds_display = dataset_display_name(&ds.name);
                            validate_metadata_for_dataset(
                                &kind.name,
                                ds_display,
                                &dim_def.name,
                                meta,
                                &ds.extras,
                                &mut errors,
                            );
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::StructureValidation(errors))
    }
}

fn validate_metadata_for_dataset(
    kind_name: &str,
    ds_display: &str,
    dim_name: &str,
    meta: &MetadataDimension,
    extras: &KindDatasetExtras,
    errors: &mut Vec<String>,
) {
    if let Some(ref path_ext) = meta.path {
        // path.token requires storage with file paths.
        let has_paths = extras.storage.as_ref().is_some_and(|s| {
            let sources = s.all_sources();
            !sources.paths.is_empty()
        });
        if !has_paths {
            errors.push(format!(
                "kind '{}', dataset '{}': metadata dimension '{}' uses path.token={} \
                 but dataset has no storage paths configured",
                kind_name, ds_display, dim_name, path_ext.token
            ));
        }
    }

    if let Some(ref part_ext) = meta.partition {
        if part_ext.level == 0 {
            errors.push(format!(
                "kind '{}', dataset '{}': metadata dimension '{}' uses partition.level=0 \
                 but level is 1-indexed (must be >= 1)",
                kind_name, ds_display, dim_name
            ));
        }

        // partition.level requires partition_def on the dataset's storage config.
        // StorageConfig.partition_def is a single PartitionDef (depth=1).
        let storage_partition = extras
            .storage
            .as_ref()
            .and_then(|s| s.partition_def.as_ref())
            .map(|_| 1usize) // single partition_def = depth 1
            .unwrap_or(0);

        let partition_depth = storage_partition;

        if partition_depth == 0 {
            errors.push(format!(
                "kind '{}', dataset '{}': metadata dimension '{}' uses partition.level={} \
                 but dataset has no partition definitions",
                kind_name, ds_display, dim_name, part_ext.level
            ));
        } else if part_ext.level > partition_depth {
            errors.push(format!(
                "kind '{}', dataset '{}': metadata dimension '{}' uses partition.level={} \
                 but partition depth is only {}",
                kind_name, ds_display, dim_name, part_ext.level, partition_depth
            ));
        }
    }

    // At least one of path or partition must be specified.
    if meta.path.is_none() && meta.partition.is_none() {
        errors.push(format!(
            "kind '{}', dataset '{}': metadata dimension '{}' must specify \
             either 'path' or 'partition' extraction",
            kind_name, ds_display, dim_name
        ));
    }
}

// ============================================================================
// Step 4.5: Expand Auto Column Mappings
// ============================================================================

/// Expand `column_mapping: auto` / `inherited` into explicit identity mappings,
/// and merge kind-level defaults into each dataset's extras.
///
/// Handles three cases for a dataset's `column_mapping`:
///   - `Auto`:      1:1 identity from all kind interface names.
///   - `Inherited`: use `kind.extras.column_mapping`, falling back to identity.
///   - `Explicit`:  start from kind default (if any), then apply dataset overrides.
///
/// After this step every dataset has `ColumnMapping::Explicit`. `temporal` and
/// `catalog` defaults from `kind.extras` are also propagated (dataset value wins).
pub(crate) fn expand_auto_mappings(model: &mut SemanticModel) {
    for kind in &mut model.kinds {
        // Use mappable names (excludes metadata dimensions and metrics)
        // since those entities don't require physical column mapping.
        let interface_names: Vec<String> = collect_mappable_names(kind).collect();

        // Resolve the kind-level default mapping once per kind.
        let kind_default: Option<HashMap<String, ColumnMappingValue>> =
            kind.extras.as_ref().and_then(|e| e.column_mapping.as_ref()).map(|cm| {
                match cm {
                    ColumnMapping::Auto | ColumnMapping::Inherited => interface_names
                        .iter()
                        .map(|n| (n.clone(), ColumnMappingValue::Simple(n.clone())))
                        .collect(),
                    ColumnMapping::Explicit(m) => m.clone(),
                }
            });

        for ds_entry in &mut kind.datasets {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                let effective: HashMap<String, ColumnMappingValue> = match &ds.extras.column_mapping {
                    ColumnMapping::Auto => {
                        // Identity map — same behaviour as before.
                        interface_names
                            .iter()
                            .map(|n| (n.clone(), ColumnMappingValue::Simple(n.clone())))
                            .collect()
                    }
                    ColumnMapping::Inherited => {
                        // Use kind default; fall back to identity if no kind default exists.
                        kind_default.clone().unwrap_or_else(|| {
                            interface_names
                                .iter()
                                .map(|n| (n.clone(), ColumnMappingValue::Simple(n.clone())))
                                .collect()
                        })
                    }
                    ColumnMapping::Explicit(ds_map) => {
                        // Merge: kind default is the base; dataset entries override.
                        let mut merged = kind_default.clone().unwrap_or_default();
                        merged.extend(ds_map.clone());
                        merged
                    }
                };
                ds.extras.column_mapping = ColumnMapping::Explicit(effective);

                // Propagate temporal and catalog defaults (dataset value always wins).
                if let Some(kind_extras) = &kind.extras {
                    if ds.extras.temporal.is_none() {
                        ds.extras.temporal = kind_extras.temporal.clone();
                    }
                    if ds.extras.catalog.is_none() {
                        ds.extras.catalog = kind_extras.catalog.clone();
                    }
                }
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
        let interface_names: HashSet<String> = collect_interface_names(kind).collect();

        // Check each dataset's column_mapping
        for ds_entry in &kind.datasets {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                let ds_display = dataset_display_name(&ds.name);

                // Safety: expand_auto_mappings (step 4.5) must run before this step
                // to convert all Auto/Inherited mappings to Explicit.
                debug_assert!(
                    matches!(ds.extras.column_mapping, ColumnMapping::Explicit(_)),
                    "validate_mappings must run after expand_auto_mappings; \
                     found non-Explicit mapping for dataset '{}'",
                    ds_display
                );

                // Check that mapping keys reference existing interface names.
                for key in ds.extras.column_mapping.keys() {
                    if !interface_names.contains(key) {
                        errors.push(format!(
                            "kind '{}', dataset '{}': column_mapping key '{}' \
                             does not match any dimension, measure, or metric in the interface",
                            kind.name, ds_display, key
                        ));
                    }
                }

                // Check completeness: every mappable interface name (dimensions + measures)
                // must appear as a mapping key. Metrics are derived expressions and
                // do not require column mappings.
                let mappable_names: HashSet<String> = collect_mappable_names(kind).collect();
                for iname in &mappable_names {
                    if !ds.extras.column_mapping.keys().any(|k| k == iname) {
                        errors.push(format!(
                            "kind '{}', dataset '{}': mapping is incomplete — missing key '{}'",
                            kind.name, ds_display, iname
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
        compiled_at: chrono::DateTime::default(), // overwritten by compiler.compile()
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

/// Collect mappable names (dimensions + measures) from a kind.
/// Metrics are derived and do not require column mappings.
fn collect_mappable_names(kind: &Kind) -> impl Iterator<Item = String> + '_ {
    kind.dimensions
        .iter()
        .filter_map(|d| match d {
            DimensionEntry::Inline(dim) => {
                // Metadata dimensions are extracted from source metadata,
                // not physical columns — they don't need column mapping.
                if matches!(dim.dim_type, DimensionType::Metadata(_)) {
                    None
                } else {
                    Some(dim.name.clone())
                }
            }
            DimensionEntry::Ref(r) => Some(r.ref_name.clone()),
        })
        .chain(kind.measures.iter().map(|m| match m {
            MeasureEntry::Inline(mea) => mea.name.clone(),
            MeasureEntry::Ref(r) => r.ref_name.clone(),
        }))
}

/// Collect interface names (dimensions + measures + metrics) from a kind.
fn collect_interface_names(kind: &Kind) -> impl Iterator<Item = String> + '_ {
    kind.dimensions
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
}

fn compile_dimensions(entries: &[DimensionEntry]) -> IndexMap<String, CompiledDimension> {
    let mut dimensions = IndexMap::new();
    for d in entries {
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
    dimensions
}

/// Map model-layer AggregationType to core Aggregation.
fn map_aggregation_type(agg: &AggregationType) -> semstrait_core::Aggregation {
    match agg {
        AggregationType::Sum => semstrait_core::Aggregation::Sum,
        AggregationType::Avg => semstrait_core::Aggregation::Avg,
        AggregationType::Count => semstrait_core::Aggregation::Count,
        AggregationType::CountDistinct => semstrait_core::Aggregation::CountDistinct,
        AggregationType::Min => semstrait_core::Aggregation::Min,
        AggregationType::Max => semstrait_core::Aggregation::Max,
    }
}

/// Check if a parsed Expr contains any aggregation functions.
/// Exhaustive match ensures new Expr variants are explicitly handled.
fn contains_aggregation(expr: &Expr) -> bool {
    match expr {
        Expr::Aggregate(_) => true,
        Expr::BinaryOp(bin) => {
            contains_aggregation(&bin.left) || contains_aggregation(&bin.right)
        }
        Expr::Case(case) => {
            case.when_then.iter().any(|wt| {
                contains_aggregation(&wt.condition) || contains_aggregation(&wt.result)
            }) || case.else_expr.as_ref().is_some_and(|e| contains_aggregation(e))
        }
        Expr::Guard(g) => {
            contains_aggregation(&g.condition) || contains_aggregation(&g.expr)
        }
        Expr::Negate(u) | Expr::Not(u) | Expr::IsNull(u) | Expr::IsNotNull(u) => {
            contains_aggregation(&u.expr)
        }
        Expr::Coalesce(c) => c.exprs.iter().any(contains_aggregation),
        Expr::NullIf(n) => {
            contains_aggregation(&n.expr) || contains_aggregation(&n.null_expr)
        }
        Expr::DateTrunc(d) => contains_aggregation(&d.expr),
        Expr::FunctionCall(f) => f.args.iter().any(contains_aggregation),
        Expr::InList(il) => {
            contains_aggregation(&il.expr) || il.list.iter().any(contains_aggregation)
        }
        Expr::Between(b) => {
            contains_aggregation(&b.expr)
                || contains_aggregation(&b.low)
                || contains_aggregation(&b.high)
        }
        Expr::Like(l) => contains_aggregation(&l.expr),
        // Leaf nodes: no children to recurse into.
        Expr::Column(_) | Expr::Literal(_) | Expr::EntityRef(_) => false,
    }
}

fn compile_measures(
    entries: &[MeasureEntry],
) -> Result<IndexMap<String, CompiledMeasure>, CompileError> {
    let mut measures = IndexMap::new();
    let mut errors = Vec::new();

    for m in entries {
        if let MeasureEntry::Inline(mea) = m {
            let filters = compile_measure_filters(&mea.filters)?;

            let (compiled_agg, compiled_expr, expr_source) = if let Some(ref agg) = mea.agg {
                // Declarative path: agg tag present.
                let core_agg = map_aggregation_type(agg);
                let expr_source = mea.expr.as_deref().unwrap_or(&mea.name).to_string();

                if let Some(ref expr_str) = mea.expr {
                    // Parse horizontal expr, validate no aggregation.
                    let parsed = parse_expr(expr_str, &mea.name)?;
                    if contains_aggregation(&parsed) {
                        errors.push(format!(
                            "measure '{}': expr must not contain aggregation functions \
                             when 'agg' is specified; use horizontal expressions only",
                            mea.name
                        ));
                        continue;
                    }
                    (Some(core_agg), parsed, expr_source)
                } else {
                    // No expr — the column is resolved from mapping by name.
                    (Some(core_agg), Expr::entity_ref(&mea.name), expr_source)
                }
            } else if let Some(ref expr_str) = mea.expr {
                // Legacy path: no agg tag, expr contains aggregation.
                let parsed = parse_expr(expr_str, &mea.name)?;
                (None, parsed, expr_str.clone())
            } else {
                // Neither agg nor expr specified — error.
                errors.push(format!(
                    "measure '{}': either 'agg' or 'expr' must be specified",
                    mea.name
                ));
                continue;
            };

            measures.insert(
                mea.name.clone(),
                CompiledMeasure {
                    name: mea.name.clone(),
                    description: mea.description.clone(),
                    data_type: mea.data_type.to_string(),
                    agg: compiled_agg,
                    expr: compiled_expr,
                    expr_source,
                    additivity: mea.additivity.clone(),
                    constraints: mea.constraints.clone(),
                    filters,
                },
            );
        }
    }

    if !errors.is_empty() {
        return Err(CompileError::ExprCompilation(errors));
    }
    Ok(measures)
}

fn compile_metrics(
    entries: &[MetricEntry],
    metric_depths: &HashMap<String, usize>,
) -> Result<IndexMap<String, CompiledMetric>, CompileError> {
    let mut metrics = IndexMap::new();
    for m in entries {
        if let MetricEntry::Inline(met) = m {
            let expr = parse_expr(&met.expr, &met.name)?;
            let depth = metric_depths.get(&met.name).copied().unwrap_or(0);
            let filters = compile_measure_filters(&met.filters)?;
            let compiled_agg = met.agg.as_ref().map(map_aggregation_type);
            metrics.insert(
                met.name.clone(),
                CompiledMetric {
                    name: met.name.clone(),
                    description: met.description.clone(),
                    data_type: met.data_type.to_string(),
                    agg: compiled_agg,
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
    Ok(metrics)
}

fn compile_dataset(ds: &Dataset) -> Result<CompiledDataset, CompileError> {
    let no_depths = HashMap::new();
    Ok(CompiledDataset {
        name: ds.name.clone(),
        description: ds.description.clone(),
        domain: ds.domain.as_ref().map(|d| d.0.clone()),
        keys: ds.keys.clone(),
        dimensions: compile_dimensions(&ds.dimensions),
        measures: compile_measures(&ds.measures)?,
        metrics: compile_metrics(&ds.metrics, &no_depths)?,
        compiled_schema: None,
    })
}

fn compile_kind(
    kind: &Kind,
    metric_depths: &HashMap<String, usize>,
) -> Result<CompiledKind, CompileError> {
    // Compile kind datasets
    let compiled_datasets: Vec<CompiledKindDataset> = kind
        .datasets
        .iter()
        .filter_map(|ds_entry| {
            if let KindDatasetEntry::Inline(ds) = ds_entry {
                let resolved_sources = ds.extras.storage.as_ref()
                    .map(|s| s.all_sources().all().map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                Some(CompiledKindDataset {
                    name: dataset_display_name(&ds.name).to_string(),
                    extras: ds.extras.clone(),
                    resolved_sources,
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
        dimensions: compile_dimensions(&kind.dimensions),
        measures: compile_measures(&kind.measures)?,
        metrics: compile_metrics(&kind.metrics, metric_depths)?,
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

/// Parse a DSL expression string into a Expr.
///
/// For v1, we parse common aggregation patterns (SUM, COUNT, etc.)
/// and store other expressions as entity refs. Full DSL parsing
/// will use the semstrait-core DSL lexer/parser when stabilized.
fn parse_expr(expr: &str, entity_name: &str) -> Result<Expr, CompileError> {
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
        return Ok(Expr::entity_ref(inner));
    }

    // Bare identifier => entity ref
    if is_identifier(trimmed) {
        return Ok(Expr::entity_ref(trimmed));
    }

    // Numeric literal
    if let Ok(v) = trimmed.parse::<i64>() {
        return Ok(Expr::int(v));
    }
    if let Ok(v) = trimmed.parse::<f64>() {
        return Ok(Expr::float(v));
    }

    // Fallback: store as entity ref
    Ok(Expr::entity_ref(trimmed))
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

fn try_parse_aggregation(expr: &str) -> Option<Expr> {
    let upper = expr.to_uppercase();

    #[allow(clippy::type_complexity)]
    let agg_patterns: &[(&str, fn(Expr) -> Expr)] = &[
        ("SUM(", Expr::sum),
        ("COUNT_DISTINCT(", Expr::count_distinct),
        ("COUNT(", Expr::count),
        ("AVG(", Expr::avg),
        ("MIN(", Expr::min),
        ("MAX(", Expr::max),
    ];

    for (prefix, constructor) in agg_patterns {
        if upper.starts_with(prefix) && expr.ends_with(')') {
            let inner = expr[prefix.len()..expr.len() - 1].trim();
            let inner_expr = if is_identifier(inner) {
                Expr::column(inner)
            } else {
                Expr::entity_ref(inner)
            };
            return Some(constructor(inner_expr));
        }
    }

    None
}

fn try_parse_arithmetic(expr: &str) -> Option<Expr> {
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
        '+' => Expr::add(left_expr, right_expr),
        '-' => Expr::subtract(left_expr, right_expr),
        '*' => Expr::multiply(left_expr, right_expr),
        '/' => Expr::divide(left_expr, right_expr),
        _ => return None,
    })
}

fn parse_operand(s: &str) -> Expr {
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
        return Expr::entity_ref(inner);
    }

    if let Ok(v) = trimmed.parse::<i64>() {
        return Expr::int(v);
    }
    if let Ok(v) = trimmed.parse::<f64>() {
        return Expr::float(v);
    }

    if is_identifier(trimmed) {
        return Expr::entity_ref(trimmed);
    }

    Expr::entity_ref(trimmed)
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
            Expr::Aggregate(agg) => {
                assert_eq!(agg.function, semstrait_core::Aggregation::Sum);
                match agg.expr.as_ref() {
                    Expr::Column(col) => assert_eq!(col.name, "amount"),
                    _ => panic!("expected Column inside Sum"),
                }
            }
            _ => panic!("expected Aggregate(Sum), got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_expr_count_distinct() {
        let expr = parse_expr("COUNT_DISTINCT(customer_id)", "unique_customers").unwrap();
        match &expr {
            Expr::Aggregate(agg) => {
                assert_eq!(agg.function, semstrait_core::Aggregation::CountDistinct);
            }
            _ => panic!("expected Aggregate(CountDistinct), got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_expr_entity_ref() {
        let expr = parse_expr("{{ revenue }}", "margin").unwrap();
        match &expr {
            Expr::EntityRef(e) => assert_eq!(e.name, "revenue"),
            _ => panic!("expected EntityRef"),
        }
    }

    #[test]
    fn test_parse_expr_arithmetic() {
        let expr = parse_expr("{{ revenue }} - {{ cost }}", "profit").unwrap();
        match &expr {
            Expr::BinaryOp(bin) => {
                assert_eq!(bin.op, semstrait_core::BinaryOp::Subtract);
            }
            _ => panic!("expected BinaryOp(Subtract), got {:?}", expr),
        }
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
                extras: None,
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
