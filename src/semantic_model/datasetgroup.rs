//! Container and grain set types for aggregate awareness
//!
//! A model has a root container: either a single GrainSet or a UnionSet of union members.
//! Union members form a tree: each member is either a grain set (leaf) or a union group
//! (optional shared dimensions/measures + nested union_set). Dimensions and measures
//! from ancestor groups are merged into each leaf grain set when resolving "effective" grain sets.

use super::column::Column;
use super::dimension::{Attribute, Join};
use super::measure::Measure;
use serde::Deserialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Root and recursive union tree
// ---------------------------------------------------------------------------

/// Root container for a semantic model. Exactly one per model.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootContainer {
    /// Single grain set (one grain, one or more datasets)
    GrainSet(GrainSet),
    /// Union of members (each member is a grain set or a nested group with its own union_set)
    UnionSet(Vec<UnionMember>),
}

/// A member of a union_set: either a leaf grain set or a nested group with shared dimensions/measures.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum UnionMember {
    /// Nested group: optional name, dimensions, measures, and child union_set (recursive).
    /// Deserialized when the object has an "union_set" key.
    UnionGroup(UnionGroup),
    /// Leaf: a single grain set. Deserialized when the object has a "grain_set" key.
    GrainSetLeaf(GrainSetLeaf),
}

/// A nested group in the union tree. Shares dimensions and measures with all descendant grain sets.
#[derive(Debug, Deserialize)]
pub struct UnionGroup {
    pub name: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    /// Dimensions shared by all grain sets under this group (merged with child definitions).
    #[serde(default)]
    pub dimensions: Vec<GrainSetDimension>,
    /// Measures shared by all grain sets under this group (merged with child definitions).
    #[serde(default)]
    pub measures: Vec<Measure>,
    /// Child members: grain sets or further nested groups.
    pub union_set: Vec<UnionMember>,
}

/// Wrapper for a leaf grain set in a union_set (YAML: `grain_set: { name, dimensions, ... }`).
#[derive(Debug, Deserialize)]
pub struct GrainSetLeaf {
    pub grain_set: GrainSet,
}

// ---------------------------------------------------------------------------
// Tree walk: collect effective grain sets (with inherited dimensions/measures)
// ---------------------------------------------------------------------------

/// Merge parent dimensions with child dimensions. Child overrides by name.
fn merge_dimensions(
    parent: &[GrainSetDimension],
    child: &[GrainSetDimension],
) -> Vec<GrainSetDimension> {
    let mut out: Vec<GrainSetDimension> = parent.to_vec();
    for d in child {
        if let Some(pos) = out.iter().position(|x| x.name == d.name) {
            out[pos] = d.clone();
        } else {
            out.push(d.clone());
        }
    }
    out
}

/// Merge parent measures with child measures. Child overrides by name.
fn merge_measures(parent: &[Measure], child: &[Measure]) -> Vec<Measure> {
    let mut out: Vec<Measure> = parent.to_vec();
    for m in child {
        if let Some(pos) = out.iter().position(|x| x.name == m.name) {
            out[pos] = m.clone();
        } else {
            out.push(m.clone());
        }
    }
    out
}

impl RootContainer {
    /// Recursively collect all leaf grain sets with dimensions and measures merged from ancestor groups.
    /// Each grain set gets a `container_path` from root to that leaf (e.g. ["adwords"] or ["facebookads", "facebookads_111"]).
    pub fn effective_grain_sets(&self) -> Vec<GrainSet> {
        match self {
            RootContainer::GrainSet(gs) => {
                let mut gs = gs.clone();
                gs.container_path = Some(vec![gs.name.clone()]);
                vec![gs]
            }
            RootContainer::UnionSet(members) => {
                let mut out = Vec::new();
                collect_effective_impl(members, &[], &[], &[], &mut out);
                out
            }
        }
    }
}

fn collect_effective_impl(
    members: &[UnionMember],
    parent_dims: &[GrainSetDimension],
    parent_measures: &[Measure],
    path: &[String],
    out: &mut Vec<GrainSet>,
) {
    for member in members {
        let segment = match member.container_name() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let mut path_here = path.to_vec();
        path_here.push(segment.clone());
        match member {
            UnionMember::GrainSetLeaf(leaf) => {
                let dims = merge_dimensions(parent_dims, &leaf.grain_set.dimensions);
                let measures = merge_measures(parent_measures, &leaf.grain_set.measures);
                let mut gs = leaf.grain_set.clone();
                gs.dimensions = dims;
                gs.measures = measures;
                gs.container_path = Some(path_here);
                out.push(gs);
            }
            UnionMember::UnionGroup(group) => {
                let dims = merge_dimensions(parent_dims, &group.dimensions);
                let measures = merge_measures(parent_measures, &group.measures);
                collect_effective_impl(&group.union_set, &dims, &measures, &path_here, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Container path resolution (path reflects levels of nested containers)
// ---------------------------------------------------------------------------

impl UnionMember {
    /// Name of this member for path lookup: UnionGroup's name or the grain set's name.
    pub fn container_name(&self) -> Option<&str> {
        match self {
            UnionMember::UnionGroup(g) => g.name.as_deref(),
            UnionMember::GrainSetLeaf(leaf) => Some(leaf.grain_set.name.as_str()),
        }
    }
}

/// Resolve a grain set by walking the container path. Path must point to a leaf (container with datasets).
/// full_path is the original path used so that container_path on the returned grain set is the full path (e.g. ["facebookads", "facebookads_111"]), not just the tail.
fn resolve_grain_set_by_path_impl(
    members: &[UnionMember],
    parent_dims: &[GrainSetDimension],
    parent_measures: &[Measure],
    path: &[&str],
    full_path: &[&str],
) -> Option<GrainSet> {
    if path.is_empty() {
        return None;
    }
    let segment = path[0];
    let member = members
        .iter()
        .find(|m| m.container_name() == Some(segment))?;
    match member {
        UnionMember::GrainSetLeaf(leaf) => {
            if path.len() == 1 {
                let dims = merge_dimensions(parent_dims, &leaf.grain_set.dimensions);
                let measures = merge_measures(parent_measures, &leaf.grain_set.measures);
                let mut gs = leaf.grain_set.clone();
                gs.dimensions = dims;
                gs.measures = measures;
                gs.container_path = Some(full_path.iter().map(|s| s.to_string()).collect());
                Some(gs)
            } else {
                None
            }
        }
        UnionMember::UnionGroup(group) => {
            if path.len() < 2 {
                return None;
            }
            let dims = merge_dimensions(parent_dims, &group.dimensions);
            let measures = merge_measures(parent_measures, &group.measures);
            resolve_grain_set_by_path_impl(
                &group.union_set,
                &dims,
                &measures,
                &path[1..],
                full_path,
            )
        }
    }
}

/// Collect all leaf grain sets at or under the given path. Path may point to a leaf (returns one)
/// or a group (returns all leaves under that group). Used for group-qualified dimension queries.
fn grain_sets_under_path_impl(
    members: &[UnionMember],
    parent_dims: &[GrainSetDimension],
    parent_measures: &[Measure],
    path_remaining: &[&str],
    path_so_far: &[String],
    out: &mut Vec<GrainSet>,
) {
    if path_remaining.is_empty() {
        collect_effective_impl(members, parent_dims, parent_measures, path_so_far, out);
        return;
    }
    let segment = path_remaining[0];
    let member = match members.iter().find(|m| m.container_name() == Some(segment)) {
        Some(m) => m,
        None => return,
    };
    let mut path_here = path_so_far.to_vec();
    path_here.push(segment.to_string());
    match member {
        UnionMember::GrainSetLeaf(leaf) => {
            if path_remaining.len() == 1 {
                let dims = merge_dimensions(parent_dims, &leaf.grain_set.dimensions);
                let measures = merge_measures(parent_measures, &leaf.grain_set.measures);
                let mut gs = leaf.grain_set.clone();
                gs.dimensions = dims;
                gs.measures = measures;
                gs.container_path = Some(path_here.clone());
                out.push(gs);
            }
        }
        UnionMember::UnionGroup(group) => {
            let dims = merge_dimensions(parent_dims, &group.dimensions);
            let measures = merge_measures(parent_measures, &group.measures);
            if path_remaining.len() == 1 {
                collect_effective_impl(&group.union_set, &dims, &measures, &path_here, out);
            } else {
                grain_sets_under_path_impl(
                    &group.union_set,
                    &dims,
                    &measures,
                    &path_remaining[1..],
                    &path_here,
                    out,
                );
            }
        }
    }
}

impl RootContainer {
    /// Return all leaf grain sets at or under the given path. Path may be a leaf (one result)
    /// or a group (all leaves under that group). Empty path returns all grain sets.
    pub fn grain_sets_under_path(&self, path: &[&str]) -> Vec<GrainSet> {
        let mut out = Vec::new();
        match self {
            RootContainer::GrainSet(gs) => {
                if path.is_empty() || (path.len() == 1 && path[0] == gs.name) {
                    let mut gs = gs.clone();
                    gs.container_path = Some(vec![gs.name.clone()]);
                    out.push(gs);
                }
            }
            RootContainer::UnionSet(members) => {
                grain_sets_under_path_impl(members, &[], &[], path, &[], &mut out);
            }
        }
        out
    }

    /// Resolve a container path to the grain set at that path. Path must point to a leaf
    /// (a container that has datasets). Returns None if path is invalid or points to a union group.
    /// For a single root GrainSet, path may be empty or the single segment matching that grain set's name.
    pub fn get_grain_set_by_path(&self, path: &[&str]) -> Option<GrainSet> {
        match self {
            RootContainer::GrainSet(gs) => {
                if path.is_empty() || (path.len() == 1 && path[0] == gs.name) {
                    Some(gs.clone())
                } else {
                    None
                }
            }
            RootContainer::UnionSet(members) => {
                resolve_grain_set_by_path_impl(members, &[], &[], path, path)
            }
        }
    }
}

/// Data source configuration for a dataset
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Source {
    /// Parquet file source
    #[serde(rename = "parquet")]
    Parquet { path: String },
    /// Iceberg table source (catalog resolution is the service layer's responsibility)
    #[serde(rename = "iceberg")]
    Iceberg { table: String },
}

/// Resolve template variables in a path string
///
/// Supports the following variables:
/// - `{model.name}` - Model name
/// - `{model.namespace}` - Model namespace (errors if not set)
/// - `{container.path}` - Container path from root to grain set (e.g. "adwords" or "facebookads.facebookads_111")
/// - `{dataset.name}` - Physical dataset name
/// - `{dataset.uuid}` - Dataset UUID (errors if not set)
///
/// # Example
/// ```ignore
/// let path = resolve_path_template(
///     "{model.namespace}/{dataset.uuid}/data.parquet",
///     "sales",
///     Some("tenant-123"),
///     "orders",
///     "warehouse.orderfact",
///     Some("abc-123"),
/// )?;
/// // Returns: "tenant-123/abc-123/data.parquet"
/// ```
pub fn resolve_path_template(
    template: &str,
    model_name: &str,
    model_namespace: Option<&str>,
    container_path: &str,
    dataset_name: &str,
    dataset_uuid: Option<&str>,
) -> Result<String, String> {
    let mut path = template.to_string();

    // Required variables (always available)
    path = path.replace("{model.name}", model_name);
    path = path.replace("{container.path}", container_path);
    path = path.replace("{dataset.name}", dataset_name);

    // Optional variables - error if used but not present
    if path.contains("{model.namespace}") {
        match model_namespace {
            Some(ns) => path = path.replace("{model.namespace}", ns),
            None => {
                return Err(format!(
                "Path template uses {{model.namespace}} but model '{}' has no namespace defined",
                model_name
            ))
            }
        }
    }

    if path.contains("{dataset.uuid}") {
        match dataset_uuid {
            Some(uuid) => path = path.replace("{dataset.uuid}", uuid),
            None => {
                return Err(format!(
                    "Path template uses {{dataset.uuid}} but dataset '{}' has no uuid defined",
                    dataset_name
                ))
            }
        }
    }

    // Check for unresolved variables
    if let Some(start) = path.find('{') {
        if let Some(end) = path[start..].find('}') {
            let var = &path[start..start + end + 1];
            return Err(format!("Unknown variable in path template: {}", var));
        }
    }

    Ok(path)
}

/// Resolve template variables in a dimension path string
///
/// Supports the following variables:
/// - `{model.name}` - Model name
/// - `{model.namespace}` - Model namespace (errors if not set)
/// - `{dimension.name}` - Dimension name
/// - `{dimension.table}` - Dimension table name
///
/// # Example
/// ```ignore
/// let path = resolve_dimension_path_template(
///     "{model.namespace}/dimensions/{dimension.name}.parquet",
///     "sales",
///     Some("tenant-123"),
///     "dates",
///     "warehouse.dates",
/// )?;
/// // Returns: "tenant-123/dimensions/dates.parquet"
/// ```
pub fn resolve_dimension_path_template(
    template: &str,
    model_name: &str,
    model_namespace: Option<&str>,
    dimension_name: &str,
    dimension_table: &str,
) -> Result<String, String> {
    let mut path = template.to_string();

    // Required variables (always available)
    path = path.replace("{model.name}", model_name);
    path = path.replace("{dimension.name}", dimension_name);
    path = path.replace("{dimension.table}", dimension_table);

    // Optional variables - error if used but not present
    if path.contains("{model.namespace}") {
        match model_namespace {
            Some(ns) => path = path.replace("{model.namespace}", ns),
            None => {
                return Err(format!(
                "Path template uses {{model.namespace}} but model '{}' has no namespace defined",
                model_name
            ))
            }
        }
    }

    // Check for unresolved variables
    if let Some(start) = path.find('{') {
        if let Some(end) = path[start..].find('}') {
            let var = &path[start..start + end + 1];
            return Err(format!(
                "Unknown variable in dimension path template: {}",
                var
            ));
        }
    }

    Ok(path)
}

/// A grain set - datasets sharing dimension and measure definitions (same grain).
#[derive(Debug, Deserialize, Clone)]
pub struct GrainSet {
    pub name: String,
    pub label: Option<String>,
    /// Human-readable description for UIs and LLMs
    pub description: Option<String>,
    /// Dimensions available to datasets in this grain set
    pub dimensions: Vec<GrainSetDimension>,
    /// Measures shared by all datasets in this grain set
    pub measures: Vec<Measure>,
    /// Physical datasets, each declaring which subset of fields it has
    pub datasets: Vec<Dataset>,
    /// Container path from root to this grain set (e.g. ["facebookads", "facebookads_111"]).
    /// Set when building effective grain sets; not in YAML.
    #[serde(default)]
    pub container_path: Option<Vec<String>>,
}

/// A dimension reference within a grain set
///
/// Can be either:
/// - A reference to a top-level dimension (has join)
/// - A degenerate dimension (no join, has inline attributes)
#[derive(Debug, Deserialize, Clone)]
pub struct GrainSetDimension {
    pub name: String,
    pub label: Option<String>,
    /// Join specification - if None, this is a degenerate dimension
    pub join: Option<Join>,
    /// Inline attributes for degenerate dimensions
    pub attributes: Option<Vec<Attribute>>,
}

/// A physical dataset within a grain set
#[derive(Debug, Deserialize, Clone)]
pub struct Dataset {
    /// Physical dataset name (e.g., "warehouse.orderfact")
    pub name: String,
    pub label: Option<String>,
    /// Human-readable description for UIs and LLMs
    pub description: Option<String>,
    /// Data source configuration (parquet path, iceberg table, etc.)
    pub source: Source,
    /// Unique identifier for this dataset (e.g., Iceberg table UUID)
    pub uuid: Option<String>,
    /// Custom key-value properties (e.g., connectorType, sourceSystem)
    pub properties: Option<HashMap<String, String>>,
    /// Column definitions - optional, used for explicit schema documentation
    /// Join detection is now based on dimension attribute inclusion, not column presence
    #[serde(default)]
    pub columns: Option<Vec<Column>>,
    /// Dimension attributes available on this dataset
    /// Map from dimension name to list of attribute names
    pub dimensions: HashMap<String, Vec<String>>,
    /// Measure names available on this dataset (references group-level measures)
    pub measures: Vec<String>,
}

impl GrainSet {
    /// Get a dimension by name
    pub fn get_dimension(&self, name: &str) -> Option<&GrainSetDimension> {
        self.dimensions.iter().find(|d| d.name == name)
    }

    /// Get a measure by name
    pub fn get_measure(&self, name: &str) -> Option<&Measure> {
        self.measures.iter().find(|m| m.name == name)
    }

    /// Get a dataset by physical dataset name
    pub fn get_dataset(&self, dataset_name: &str) -> Option<&Dataset> {
        self.datasets.iter().find(|t| t.name == dataset_name)
    }

    /// Get all unique measure names
    pub fn measure_names(&self) -> Vec<&str> {
        self.measures.iter().map(|m| m.name.as_str()).collect()
    }
}

impl GrainSetDimension {
    /// Returns true if this is a degenerate dimension (no join, has inline attributes)
    pub fn is_degenerate(&self) -> bool {
        self.join.is_none()
    }

    /// Returns true if this references a top-level dimension (has join)
    pub fn is_reference(&self) -> bool {
        self.join.is_some()
    }

    /// Get an inline attribute by name (for degenerate dimensions)
    pub fn get_attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes.as_ref()?.iter().find(|a| a.name == name)
    }

    /// Get the join key (left key) if this is a joined dimension
    pub fn join_key(&self) -> Option<&str> {
        self.join.as_ref().map(|j| j.left_key.as_str())
    }
}

impl Dataset {
    /// Get the parquet path if source is Parquet
    pub fn parquet_path(&self) -> Option<&str> {
        match &self.source {
            Source::Parquet { path } => Some(path),
            _ => None,
        }
    }

    /// Get the Iceberg table identifier if source is Iceberg
    pub fn iceberg_table(&self) -> Option<&str> {
        match &self.source {
            Source::Iceberg { table } => Some(table),
            _ => None,
        }
    }

    /// Get the primary source identifier regardless of source type.
    ///
    /// Returns the parquet path for Parquet sources, or the table identifier
    /// for Iceberg sources.
    pub fn source_ref(&self) -> &str {
        match &self.source {
            Source::Parquet { path } => path,
            Source::Iceberg { table } => table,
        }
    }

    /// Get a column definition by name (from optional columns list)
    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.as_ref()?.iter().find(|c| c.name == name)
    }

    /// Check if this dataset has a specific column in the explicit columns list
    pub fn has_column(&self, name: &str) -> bool {
        self.columns
            .as_ref()
            .map(|cols| cols.iter().any(|c| c.name == name))
            .unwrap_or(false)
    }

    /// Get the list of attributes for a dimension
    pub fn get_dimension_attributes(&self, dim_name: &str) -> Option<&Vec<String>> {
        self.dimensions.get(dim_name)
    }

    /// Check if this dataset has a dimension
    pub fn has_dimension(&self, name: &str) -> bool {
        self.dimensions.contains_key(name)
    }

    /// Check if this dataset has a measure (by name)
    pub fn has_measure(&self, name: &str) -> bool {
        self.measures.iter().any(|m| m == name)
    }

    /// Count total available attributes across all dimensions
    pub fn attribute_count(&self) -> usize {
        self.dimensions.values().map(|attrs| attrs.len()).sum()
    }

    /// Check if a dimension needs a join on this dataset (legacy method)
    ///
    /// If the join key column exists on this dataset, a join is needed.
    /// If the join key is absent, assume attributes are denormalized.
    #[deprecated(
        note = "Use needs_join_for_dimension instead, which uses attribute-based detection"
    )]
    pub fn needs_join(&self, dim: &GrainSetDimension) -> bool {
        match dim.join_key() {
            Some(key) => self.has_column(key),
            None => false, // Degenerate dimensions never need joins
        }
    }

    /// Check if this dataset has a specific attribute for a dimension
    pub fn has_dimension_attribute(&self, dim_name: &str, attr_name: &str) -> bool {
        self.dimensions
            .get(dim_name)
            .map(|attrs| attrs.iter().any(|a| a == attr_name))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_grain_set() -> GrainSet {
        let yaml = r#"
name: orders
dimensions:
  - name: dates
    join:
      left_key: time_id
      right_key: time_id
  - name: flags
    attributes:
      - name: is_premium
        column: is_premium_order
        type: bool
measures:
  - name: sales
    aggregation: sum
    expr: totalprice
    type: f64
datasets:
  - name: warehouse.orderfact
    source:
      type: parquet
      path: /data/warehouse/orderfact.parquet
    dimensions:
      dates: [year, month]
      flags: [is_premium]
    measures: [sales]
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    fn sample_grain_set_with_columns() -> GrainSet {
        let yaml = r#"
name: orders
dimensions:
  - name: dates
    join:
      left_key: time_id
      right_key: time_id
  - name: flags
    attributes:
      - name: is_premium
        column: is_premium_order
        type: bool
measures:
  - name: sales
    aggregation: sum
    expr: totalprice
    type: f64
datasets:
  - name: warehouse.orderfact
    source:
      type: parquet
      path: /data/warehouse/orderfact.parquet
    columns:
      - name: time_id
        type: i32
      - name: totalprice
        type: f64
      - name: is_premium_order
        type: bool
    dimensions:
      dates: [year, month]
      flags: [is_premium]
    measures: [sales]
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_parse_grain_set() {
        let group = sample_grain_set();
        assert_eq!(group.name, "orders");
        assert_eq!(group.dimensions.len(), 2);
        assert_eq!(group.measures.len(), 1);
        assert_eq!(group.datasets.len(), 1);
    }

    #[test]
    fn test_parse_grain_set_without_columns() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        // columns should be None when not specified
        assert!(dataset.columns.is_none());
    }

    #[test]
    fn test_parse_grain_set_with_columns() {
        let group = sample_grain_set_with_columns();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        // columns should be Some when specified
        assert!(dataset.columns.is_some());
        assert_eq!(dataset.columns.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_dimension_types() {
        let group = sample_grain_set();

        let dates = group.get_dimension("dates").unwrap();
        assert!(dates.is_reference());
        assert!(!dates.is_degenerate());
        assert_eq!(dates.join_key(), Some("time_id"));

        let flags = group.get_dimension("flags").unwrap();
        assert!(!flags.is_reference());
        assert!(flags.is_degenerate());
        assert_eq!(flags.join_key(), None);
    }

    #[test]
    fn test_dataset_columns_optional() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        // Without explicit columns, has_column returns false
        assert!(!dataset.has_column("time_id"));
        assert!(!dataset.has_column("totalprice"));
        assert!(!dataset.has_column("nonexistent"));
    }

    #[test]
    fn test_dataset_columns_explicit() {
        let group = sample_grain_set_with_columns();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        // With explicit columns, has_column works
        assert!(dataset.has_column("time_id"));
        assert!(dataset.has_column("totalprice"));
        assert!(!dataset.has_column("nonexistent"));
    }

    #[test]
    fn test_dataset_dimensions() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        assert!(dataset.has_dimension("dates"));
        assert!(dataset.has_dimension("flags"));
        assert!(!dataset.has_dimension("markets"));

        let attrs = dataset.get_dimension_attributes("dates").unwrap();
        assert_eq!(attrs, &vec!["year".to_string(), "month".to_string()]);
    }

    #[test]
    fn test_has_dimension_attribute() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        assert!(dataset.has_dimension_attribute("dates", "year"));
        assert!(dataset.has_dimension_attribute("dates", "month"));
        assert!(!dataset.has_dimension_attribute("dates", "quarter"));
        assert!(dataset.has_dimension_attribute("flags", "is_premium"));
        assert!(!dataset.has_dimension_attribute("nonexistent", "attr"));
    }

    #[test]
    fn test_dataset_measures() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        assert!(dataset.has_measure("sales"));
        assert!(!dataset.has_measure("quantity"));
    }

    #[test]
    fn test_attribute_count() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        // dates: [year, month] = 2, flags: [is_premium] = 1
        assert_eq!(dataset.attribute_count(), 3);
    }

    #[test]
    fn test_resolve_path_template_all_variables() {
        let result = resolve_path_template(
            "{model.namespace}/{dataset.uuid}/data.parquet",
            "sales",
            Some("tenant-123"),
            "orders",
            "warehouse.orderfact",
            Some("abc-def-123"),
        );
        assert_eq!(result.unwrap(), "tenant-123/abc-def-123/data.parquet");
    }

    #[test]
    fn test_resolve_path_template_required_only() {
        let result = resolve_path_template(
            "/data/{model.name}/{container.path}/{dataset.name}.parquet",
            "sales",
            None,
            "orders",
            "warehouse.orderfact",
            None,
        );
        assert_eq!(
            result.unwrap(),
            "/data/sales/orders/warehouse.orderfact.parquet"
        );
    }

    #[test]
    fn test_resolve_path_template_no_variables() {
        let result = resolve_path_template(
            "/static/path/data.parquet",
            "sales",
            Some("tenant"),
            "orders",
            "dataset",
            Some("uuid"),
        );
        assert_eq!(result.unwrap(), "/static/path/data.parquet");
    }

    #[test]
    fn test_resolve_path_template_missing_namespace() {
        let result = resolve_path_template(
            "{model.namespace}/data.parquet",
            "sales",
            None, // namespace not set
            "orders",
            "dataset",
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("model.namespace"));
    }

    #[test]
    fn test_resolve_path_template_missing_uuid() {
        let result = resolve_path_template(
            "{dataset.uuid}/data.parquet",
            "sales",
            None,
            "orders",
            "dataset",
            None, // uuid not set
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("dataset.uuid"));
    }

    #[test]
    fn test_resolve_path_template_unknown_variable() {
        let result = resolve_path_template(
            "{unknown.var}/data.parquet",
            "sales",
            None,
            "orders",
            "dataset",
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown variable"));
    }

    #[test]
    fn test_parse_iceberg_source() {
        let yaml = r#"
name: orders
dimensions:
  - name: dates
    join:
      left_key: time_id
      right_key: time_id
measures:
  - name: sales
    aggregation: sum
    expr: totalprice
    type: f64
datasets:
  - name: warehouse.orderfact
    source:
      type: iceberg
      table: warehouse.orderfact
    dimensions:
      dates: [year, month]
    measures: [sales]
"#;
        let group: GrainSet = serde_yaml::from_str(yaml).unwrap();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        assert!(dataset.parquet_path().is_none());
        assert_eq!(dataset.iceberg_table(), Some("warehouse.orderfact"));
        assert_eq!(dataset.source_ref(), "warehouse.orderfact");
    }

    #[test]
    fn test_source_ref_parquet() {
        let group = sample_grain_set();
        let dataset = group.get_dataset("warehouse.orderfact").unwrap();

        assert_eq!(dataset.source_ref(), "/data/warehouse/orderfact.parquet");
        assert_eq!(
            dataset.parquet_path(),
            Some("/data/warehouse/orderfact.parquet")
        );
        assert!(dataset.iceberg_table().is_none());
    }
}
