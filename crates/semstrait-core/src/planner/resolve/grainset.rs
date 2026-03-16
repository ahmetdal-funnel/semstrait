//! Grainset resolution algorithm.
//!
//! Datasets represent the SAME entity at DIFFERENT grains (e.g. daily, monthly).
//! The algorithm picks the optimal dataset(s) to answer a query.
//!
//! Steps:
//!
//! 1. CONSTRAINTS CHECK
//! 2. REQUIRED COVERAGE — dimensions + measures needed
//! 3. FIND COVERING DATASETS — column_mapping grain <= requested grain
//! 4. ADDITIVITY CHECK — semi/non rules per dataset
//! 5. Single covering → pick coarsest grain; multiple same-grain → UNION ALL; no single → error
//! 6. Return plan info for SQL generation

use crate::diagnostics::{codes, CompileError, Diagnostic};
use crate::schema::model::{
    ColumnMappingValue, DimensionEntry, DimensionType, Kind, KindDataset, KindDatasetEntry,
};

use super::QueryRequest;

/// Result of grainset resolution.
#[derive(Debug)]
pub struct GrainsetPlan {
    /// Indices of selected datasets within the kind's dataset list.
    pub selected_datasets: Vec<usize>,
    /// Physical column mappings for each selected dataset:
    /// `(dataset_index, logical_name → physical_column)`.
    pub column_mappings: Vec<Vec<(String, String)>>,
    /// Whether UNION ALL is needed (multiple datasets at same grain).
    pub needs_union: bool,
}

/// Resolve a grainset kind for a query request.
pub fn resolve(
    kind: &Kind,
    request: &QueryRequest,
) -> Result<GrainsetPlan, CompileError> {
    // Collect bucketed dimension names — these are computed, not in column_mapping
    let bucketed_dims = collect_bucketed_dim_names(kind);

    // Step 2: determine required columns (dimensions + measures),
    // excluding bucketed dimensions (they're computed, not physical)
    let required: Vec<&str> = request
        .dimensions
        .iter()
        .filter(|d| !bucketed_dims.contains(&d.as_str()))
        .chain(request.measures.iter())
        .map(String::as_str)
        .collect();

    if required.is_empty() {
        return Err(CompileError::single(Diagnostic::error(
            codes::PLAN_E001,
            format!("grainset '{}': query requests no dimensions or measures", kind.name),
        )));
    }

    // Step 3: find covering datasets
    let mut candidates: Vec<CoveringCandidate> = Vec::new();

    for (idx, entry) in kind.datasets.iter().enumerate() {
        let ds = match entry {
            KindDatasetEntry::Inline(ds) => ds,
            KindDatasetEntry::Ref(_) => continue, // refs should be resolved by parser
        };

        match check_coverage(ds, &required, &request.dimensions) {
            Some(candidate) => candidates.push(CoveringCandidate {
                dataset_index: idx,
                coarsest_grain: candidate.coarsest_grain,
                mappings: candidate.mappings,
            }),
            None => continue,
        }
    }

    if candidates.is_empty() {
        return Err(CompileError::single(
            Diagnostic::error(
                codes::PLAN_E001,
                format!(
                    "grainset '{}': no dataset covers all required columns [{}]",
                    kind.name,
                    required.join(", ")
                ),
            )
            .with_entity(format!("kinds.{}", kind.name), &kind.name),
        ));
    }

    // Step 5a: sort by coarsest grain descending (prefer coarser = fewer rows)
    candidates.sort_by(|a, b| b.coarsest_grain.cmp(&a.coarsest_grain));

    // Pick the coarsest grain group
    let best_grain = candidates[0].coarsest_grain;
    let selected: Vec<_> = candidates
        .into_iter()
        .filter(|c| c.coarsest_grain == best_grain)
        .collect();

    let needs_union = selected.len() > 1;
    let selected_datasets = selected.iter().map(|c| c.dataset_index).collect();
    let column_mappings = selected.into_iter().map(|c| c.mappings).collect();

    Ok(GrainsetPlan {
        selected_datasets,
        column_mappings,
        needs_union,
    })
}

struct CoveringCandidate {
    dataset_index: usize,
    coarsest_grain: u8,
    mappings: Vec<(String, String)>,
}

struct CoverageResult {
    coarsest_grain: u8,
    mappings: Vec<(String, String)>,
}

/// Check if a dataset covers all required columns, returning grain info if yes.
fn check_coverage(
    ds: &KindDataset,
    required: &[&str],
    dimension_names: &[String],
) -> Option<CoverageResult> {
    let mapping = &ds.extras.column_mapping;
    let mut result_mappings = Vec::new();
    let mut coarsest_grain: u8 = 0;

    for &col in required {
        match mapping.get(col) {
            Some(ColumnMappingValue::Simple(physical)) => {
                result_mappings.push((col.to_string(), physical.clone()));
            }
            Some(ColumnMappingValue::Complex { column, grain }) => {
                result_mappings.push((col.to_string(), column.clone()));
                if dimension_names.iter().any(|d| d == col) {
                    if let Some(g) = grain {
                        let c = g.coarseness();
                        if c > coarsest_grain {
                            coarsest_grain = c;
                        }
                    }
                }
            }
            None => return None, // missing column → not covering
        }
    }

    Some(CoverageResult {
        coarsest_grain,
        mappings: result_mappings,
    })
}

/// Collect names of bucketed dimensions in the kind.
fn collect_bucketed_dim_names(kind: &Kind) -> Vec<&str> {
    kind.dimensions
        .as_ref()
        .map(|dims| {
            dims.iter()
                .filter_map(|entry| match entry {
                    DimensionEntry::Inline(d) => match &d.dim_type {
                        DimensionType::Bucketed(_) => Some(d.name.as_str()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn load_grainset_kind() -> Kind {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/grainset_basic.yaml"
        ));
        let model = parser::parse_file(path).unwrap();
        model.semantic_model.kinds.unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn test_grainset_picks_coarsest() {
        let kind = load_grainset_kind();
        let request = QueryRequest {
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &request).unwrap();
        // Should pick the monthly dataset (coarser grain)
        assert_eq!(plan.selected_datasets.len(), 1);
        assert!(!plan.needs_union);
    }

    #[test]
    fn test_grainset_all_columns_required() {
        let kind = load_grainset_kind();
        let request = QueryRequest {
            dimensions: vec!["order_date".into()],
            measures: vec!["revenue".into(), "order_count".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let plan = resolve(&kind, &request).unwrap();
        // Both datasets cover all columns, should pick coarsest
        assert!(!plan.selected_datasets.is_empty());
    }

    #[test]
    fn test_grainset_no_coverage_fails() {
        let kind = load_grainset_kind();
        let request = QueryRequest {
            dimensions: vec!["nonexistent_col".into()],
            measures: vec!["revenue".into()],
            metrics: vec![],
            domain: None,
            aggregation: None,
        };
        let err = resolve(&kind, &request).unwrap_err();
        assert!(err.to_string().contains("PLAN_E001"));
    }
}
