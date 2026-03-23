//! Root schema definition

use super::datasetgroup::{Dataset, GrainSet, RootContainer};
use super::dimension::Dimension;
use super::measure::Measure;
use super::metric::Metric;
use crate::error::ParseError;
use serde::Deserialize;
use std::path::Path;

/// The root semantic schema containing semantic models
#[derive(Debug, Deserialize)]
pub struct Schema {
    pub semantic_models: Vec<SemanticModel>,
}

/// A semantic model - the queryable business entity
///
/// Has a root container: either a single grain set or a union_set of grain sets.
/// The selector picks the optimal dataset within a grain set based on query requirements.
#[derive(Debug, Deserialize)]
pub struct SemanticModel {
    pub name: String,
    /// Namespace for the model (e.g., organization or project identifier)
    pub namespace: Option<String>,
    /// Model-level dimensions - queryable with 2-part paths across all grain sets
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    /// Root container: grain_set (single) or union_set (list of grain sets)
    #[serde(flatten)]
    pub root: RootContainer,
    /// Metrics - derived calculations from measures (model-level, shared across grain sets)
    pub metrics: Option<Vec<Metric>>,
    /// Row-level security filter
    pub data_filter: Option<Vec<DataFilter>>,
}

/// Row-level security filter
#[derive(Debug, Deserialize)]
pub struct DataFilter {
    pub field: String,
    pub user_attribute: Option<String>,
}

impl Schema {
    /// Load a schema from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ParseError> {
        let path_str = path.as_ref().display().to_string();
        let contents = std::fs::read_to_string(&path).map_err(|e| ParseError::Io {
            path: path_str,
            source: e,
        })?;
        serde_yaml::from_str(&contents).map_err(ParseError::from)
    }

    /// Get a semantic model by name
    pub fn get_model(&self, name: &str) -> Option<&SemanticModel> {
        self.semantic_models.iter().find(|m| m.name == name)
    }

    /// Get all unique dataset names referenced in the schema.
    ///
    /// Returns fully qualified dataset names (e.g., "warehouse.orderfact")
    /// from both models (fact datasets) and dimensions.
    pub fn datasets(&self) -> Vec<String> {
        let mut datasets = Vec::new();

        for model in &self.semantic_models {
            for grain_set in model.grain_sets() {
                for dataset in &grain_set.datasets {
                    datasets.push(dataset.name.clone());
                }
            }

            // Dimension tables (non-virtual only)
            for dim in &model.dimensions {
                if let Some(table) = &dim.table {
                    datasets.push(table.clone());
                }
            }
        }

        // Deduplicate and sort
        datasets.sort();
        datasets.dedup();
        datasets
    }

    /// Get all datasets across all models and grain sets
    ///
    /// Returns owned Dataset structs with full source configuration.
    pub fn all_datasets(&self) -> Vec<Dataset> {
        self.semantic_models
            .iter()
            .flat_map(|m| m.grain_sets())
            .flat_map(|g| g.datasets.into_iter())
            .collect()
    }
}

impl SemanticModel {
    /// All grain sets with effective dimensions and measures (inherited from ancestor union groups).
    /// For a single root grain set, returns that grain set. For a union_set tree, recursively
    /// walks the tree and merges each group's dimensions/measures into descendant grain sets.
    pub fn grain_sets(&self) -> Vec<GrainSet> {
        self.root.effective_grain_sets()
    }

    /// Get a dimension by name
    pub fn get_dimension(&self, name: &str) -> Option<&Dimension> {
        self.dimensions.iter().find(|d| d.name == name)
    }

    /// Get a metric by name
    pub fn get_metric(&self, name: &str) -> Option<&Metric> {
        self.metrics.as_ref()?.iter().find(|m| m.name == name)
    }

    /// Get a grain set by name (leaf container name)
    pub fn get_grain_set(&self, name: &str) -> Option<GrainSet> {
        self.grain_sets().into_iter().find(|g| g.name == name)
    }

    /// Get a grain set by container path. Path reflects nested levels (e.g. ["facebookads", "facebookads_account_a"]).
    /// Path must point to a leaf (container with datasets). Returns None if path is invalid or points to a union group.
    pub fn get_grain_set_by_path(&self, path: &[&str]) -> Option<GrainSet> {
        self.root.get_grain_set_by_path(path)
    }

    /// Return all leaf grain sets at or under the given path. Path may be a leaf (one result)
    /// or a group (all leaves under that group). Use for group-qualified dimension queries (e.g. "facebookads.campaign.name").
    pub fn grain_sets_under_path(&self, path: &[&str]) -> Vec<GrainSet> {
        self.root.grain_sets_under_path(path)
    }

    /// True if the given grain set (by name) is at or under the given container path.
    /// Use when checking whether a qualified dimension path applies to a specific grain set.
    pub fn grain_set_under_path(&self, path: &[&str], grain_set_name: &str) -> bool {
        self.grain_sets_under_path(path)
            .iter()
            .any(|gs| gs.name == grain_set_name)
    }

    /// Get the first grain set (convenience for single-grain-set models)
    pub fn first_grain_set(&self) -> Option<GrainSet> {
        self.grain_sets().into_iter().next()
    }

    /// Get a dataset by physical dataset name (searches all grain sets)
    pub fn get_dataset(&self, dataset_name: &str) -> Option<Dataset> {
        self.grain_sets()
            .into_iter()
            .flat_map(|g| g.datasets.into_iter())
            .find(|t| t.name == dataset_name)
    }

    /// Get a measure by name (searches all grain sets)
    pub fn get_measure(&self, name: &str) -> Option<Measure> {
        self.grain_sets()
            .into_iter()
            .flat_map(|g| g.measures.into_iter())
            .find(|m| m.name == name)
    }

    /// Check if a measure exists in any grain set
    pub fn has_measure(&self, name: &str) -> bool {
        self.grain_sets()
            .iter()
            .any(|g| g.get_measure(name).is_some())
    }

    /// Get all unique measure names across all grain sets
    pub fn measure_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .grain_sets()
            .iter()
            .flat_map(|g| g.measures.iter().map(|m| m.name.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Get all datasets across all grain sets
    pub fn all_datasets(&self) -> Vec<Dataset> {
        self.grain_sets()
            .into_iter()
            .flat_map(|g| g.datasets.into_iter())
            .collect()
    }

    /// Check if a dimension is defined at model level (can be queried with 2-part path)
    ///
    /// Model-level dimensions are queryable across all grain sets that reference them.
    /// The attr_name parameter is kept for API compatibility but not used in the check.
    pub fn is_conformed(&self, dim_name: &str, _attr_name: &str) -> bool {
        self.dimensions.iter().any(|d| d.name == dim_name)
    }

    /// Check if all dimension attributes in a query can use the cross-grain-set UNION path
    ///
    /// Returns true if all dimensions are either:
    /// - Virtual dimensions (like `_dataset`) - implicitly work across grain sets
    /// - Model-level dimensions - defined at model.dimensions, queryable with 2-part paths
    pub fn is_conformed_query(&self, dimension_attrs: &[String]) -> bool {
        if dimension_attrs.is_empty() {
            return false;
        }

        dimension_attrs.iter().all(|dim_attr| {
            let parts: Vec<&str> = dim_attr.split('.').collect();
            if parts.len() != 2 {
                return false;
            }
            // Check if dimension exists at model level (includes virtual dimensions)
            self.get_dimension(parts[0]).is_some()
        })
    }
}
