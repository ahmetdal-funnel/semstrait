//! Ref resolution: replaces `DimensionEntry::Ref`, `MeasureEntry::Ref`,
//! and `MetricEntry::Ref` with the corresponding top-level inline definitions.

use std::collections::HashMap;

use crate::parser::ParseError;
use crate::schema::model::{
    Dimension, DimensionEntry, Measure, MeasureEntry, Metric, MetricEntry, SemanticModelFile,
};

/// Resolve all `ref:` entries in the model by expanding them from top-level definitions.
pub fn resolve_refs(model: &mut SemanticModelFile) -> Result<(), ParseError> {
    let sm = &model.semantic_model;

    // Collect top-level reusable definitions (these are always inline, not ref entries)
    let top_dims: HashMap<String, Dimension> = sm
        .dimensions
        .as_ref()
        .map(|v| v.iter().map(|d| (d.name.clone(), d.clone())).collect())
        .unwrap_or_default();

    let top_measures: HashMap<String, Measure> = sm
        .measures
        .as_ref()
        .map(|v| v.iter().map(|m| (m.name.clone(), m.clone())).collect())
        .unwrap_or_default();

    let top_metrics: HashMap<String, Metric> = sm
        .metrics
        .as_ref()
        .map(|v| v.iter().map(|m| (m.name.clone(), m.clone())).collect())
        .unwrap_or_default();

    // Resolve refs in datasets
    let sm = &mut model.semantic_model;
    if let Some(datasets) = &mut sm.datasets {
        for ds in datasets.iter_mut() {
            if let Some(dims) = &mut ds.dimensions {
                resolve_dimension_refs(dims, &top_dims)?;
            }
            if let Some(measures) = &mut ds.measures {
                resolve_measure_refs(measures, &top_measures)?;
            }
            if let Some(metrics) = &mut ds.metrics {
                resolve_metric_refs(metrics, &top_metrics)?;
            }
        }
    }

    // Resolve refs in kinds
    if let Some(kinds) = &mut sm.kinds {
        for kind in kinds.iter_mut() {
            if let Some(dims) = &mut kind.dimensions {
                resolve_dimension_refs(dims, &top_dims)?;
            }
            if let Some(measures) = &mut kind.measures {
                resolve_measure_refs(measures, &top_measures)?;
            }
            if let Some(metrics) = &mut kind.metrics {
                resolve_metric_refs(metrics, &top_metrics)?;
            }
        }
    }

    Ok(())
}

fn resolve_dimension_refs(
    entries: &mut [DimensionEntry],
    top: &HashMap<String, Dimension>,
) -> Result<(), ParseError> {
    for entry in entries.iter_mut() {
        if let DimensionEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = top.get(name).ok_or_else(|| {
                ParseError::RefResolution(format!("unknown dimension ref: '{}'", name))
            })?;
            *entry = DimensionEntry::Inline(resolved.clone());
        }
    }
    Ok(())
}

fn resolve_measure_refs(
    entries: &mut [MeasureEntry],
    top: &HashMap<String, Measure>,
) -> Result<(), ParseError> {
    for entry in entries.iter_mut() {
        if let MeasureEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = top.get(name).ok_or_else(|| {
                ParseError::RefResolution(format!("unknown measure ref: '{}'", name))
            })?;
            *entry = MeasureEntry::Inline(resolved.clone());
        }
    }
    Ok(())
}

fn resolve_metric_refs(
    entries: &mut [MetricEntry],
    top: &HashMap<String, Metric>,
) -> Result<(), ParseError> {
    for entry in entries.iter_mut() {
        if let MetricEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = top.get(name).ok_or_else(|| {
                ParseError::RefResolution(format!("unknown metric ref: '{}'", name))
            })?;
            *entry = MetricEntry::Inline(resolved.clone());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_str;
    use crate::schema::model::DimensionEntry;

    #[test]
    fn test_resolve_dimension_ref() {
        let yaml = r#"
semantic_model:
  name: ref_test
  dimensions:
    - name: order_date
      data_type: date
      type:
        temporal:
          grains:
            - day
  datasets:
    - name: orders
      dimensions:
        - ref: order_date
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let dims = datasets[0].dimensions.as_ref().unwrap();
        match &dims[0] {
            DimensionEntry::Inline(d) => assert_eq!(d.name, "order_date"),
            DimensionEntry::Ref(_) => panic!("expected ref to be resolved"),
        }
    }

    #[test]
    fn test_unknown_ref_fails() {
        let yaml = r#"
semantic_model:
  name: ref_test
  datasets:
    - name: orders
      dimensions:
        - ref: nonexistent
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, crate::parser::ParseError::RefResolution(_)));
    }
}
