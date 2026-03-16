//! Unionset resolution algorithm.
//!
//! Datasets are semantically equal and combined via UNION ALL.
//! Columns missing in a dataset are NULL-filled.
//!
//! Steps:
//! 1. CONSTRAINTS CHECK (done by caller)
//! 2. BUILD UNION BRANCHES — map or NULL-fill each kind-level column per dataset
//! 3. PRUNE BRANCHES — if a filter excludes NULLs and dataset lacks the column → prune
//! 4. Return plan info for UNION ALL generation

use crate::diagnostics::{codes, CompileError, Diagnostic};
use crate::schema::model::{ColumnMappingValue, Kind, KindDataset, KindDatasetEntry};

use super::QueryRequest;

/// Result of unionset resolution.
#[derive(Debug)]
pub struct UnionsetPlan {
    /// One branch per dataset. Each branch maps logical columns to either
    /// a physical column name or `None` (NULL-fill).
    pub branches: Vec<UnionBranch>,
}

/// A single branch in a UNION ALL.
#[derive(Debug)]
pub struct UnionBranch {
    /// Index of the dataset in the kind's datasets list.
    pub dataset_index: usize,
    /// Dataset name (for storage path lookup).
    pub dataset_name: String,
    /// Mapping of each requested column: `(logical_name, physical_or_null)`.
    pub column_map: Vec<(String, Option<String>)>,
}

/// Resolve a unionset kind for a query request.
pub fn resolve(
    kind: &Kind,
    request: &QueryRequest,
) -> Result<UnionsetPlan, CompileError> {
    let required: Vec<&str> = request
        .dimensions
        .iter()
        .chain(request.measures.iter())
        .map(String::as_str)
        .collect();

    if required.is_empty() {
        return Err(CompileError::single(Diagnostic::error(
            codes::PLAN_E001,
            format!("unionset '{}': query requests no columns", kind.name),
        )));
    }

    let mut branches = Vec::new();

    for (idx, entry) in kind.datasets.iter().enumerate() {
        let ds = match entry {
            KindDatasetEntry::Inline(ds) => ds,
            KindDatasetEntry::Ref(_) => continue,
        };

        let column_map = build_branch_mapping(ds, &required);
        branches.push(UnionBranch {
            dataset_index: idx,
            dataset_name: ds.name.clone(),
            column_map,
        });
    }

    if branches.is_empty() {
        return Err(CompileError::single(
            Diagnostic::error(
                codes::PLAN_E001,
                format!("unionset '{}': no datasets available", kind.name),
            )
            .with_entity(format!("kinds.{}", kind.name), &kind.name),
        ));
    }

    Ok(UnionsetPlan { branches })
}

/// Build the column mapping for a single branch, NULL-filling missing columns.
fn build_branch_mapping(
    ds: &KindDataset,
    required: &[&str],
) -> Vec<(String, Option<String>)> {
    let mapping = &ds.extras.column_mapping;

    required
        .iter()
        .map(|&col| {
            let physical = match mapping.get(col) {
                Some(ColumnMappingValue::Simple(p)) => Some(p.clone()),
                Some(ColumnMappingValue::Complex { column, .. }) => Some(column.clone()),
                None => None, // NULL-fill
            };
            (col.to_string(), physical)
        })
        .collect()
}

/// Prune branches where all requested columns are NULL-filled (useless branch).
#[allow(dead_code)]
pub fn prune_empty_branches(plan: &mut UnionsetPlan) {
    plan.branches
        .retain(|b| b.column_map.iter().any(|(_, phys)| phys.is_some()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn load_unionset_kind() -> Kind {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/unionset_basic.yaml"
        ));
        let model = parser::parse_file(path).unwrap();
        model.semantic_model.kinds.unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn test_unionset_both_branches() {
        let kind = load_unionset_kind();
        let request = QueryRequest {
            dimensions: vec!["event_date".into(), "event_type".into()],
            measures: vec!["event_count".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &request).unwrap();
        assert_eq!(plan.branches.len(), 2);
    }

    #[test]
    fn test_unionset_null_fill() {
        let kind = load_unionset_kind();
        let request = QueryRequest {
            dimensions: vec!["event_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &request).unwrap();
        // click_events has no revenue mapping → should be NULL-filled
        let click_branch = &plan.branches[0];
        let revenue_entry = click_branch
            .column_map
            .iter()
            .find(|(name, _)| name == "revenue")
            .unwrap();
        assert!(revenue_entry.1.is_none()); // NULL-fill
    }

    #[test]
    fn test_prune_empty_branches() {
        let kind = load_unionset_kind();
        // Request only revenue — click_events has no revenue mapping
        let request = QueryRequest {
            dimensions: vec![],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let mut plan = resolve(&kind, &request).unwrap();
        prune_empty_branches(&mut plan);
        // After pruning, click_events branch should be removed
        assert_eq!(plan.branches.len(), 1);
        assert_eq!(plan.branches[0].dataset_name, "purchase_events");
    }
}
