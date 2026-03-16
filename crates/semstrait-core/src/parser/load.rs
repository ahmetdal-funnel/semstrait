//! Core parsing functions: YAML → SemanticModelFile.

use std::collections::HashSet;
use std::path::Path;

use crate::schema::model::{DimensionEntry, MeasureEntry, SemanticModelFile};

/// Error from parsing a semantic model file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML deserialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("ref resolution error: {0}")]
    RefResolution(String),
    #[error("nesting error: {0}")]
    Nesting(String),
}

/// Parse a semantic model from a YAML file on disk.
pub fn parse_file(path: &Path) -> Result<SemanticModelFile, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_str(&content)
}

/// Parse a semantic model from a YAML string.
pub fn parse_str(yaml: &str) -> Result<SemanticModelFile, ParseError> {
    let mut model: SemanticModelFile = serde_yaml::from_str(yaml)?;
    validate_structure(&model)?;
    super::refs::resolve_refs(&mut model)?;
    super::nesting::validate_nesting(&model)?;
    Ok(model)
}

/// Validate basic structural requirements of the parsed model.
fn validate_structure(model: &SemanticModelFile) -> Result<(), ParseError> {
    let sm = &model.semantic_model;

    if sm.name.is_empty() {
        return Err(ParseError::Validation(
            "semantic_model.name is required and cannot be empty".to_string(),
        ));
    }

    // Validate that all datasets have names
    if let Some(datasets) = &sm.datasets {
        for (i, ds) in datasets.iter().enumerate() {
            if ds.name.is_empty() {
                return Err(ParseError::Validation(format!(
                    "dataset at index {} has an empty name",
                    i
                )));
            }
        }
    }

    // Validate that all kinds have names and at least one dataset
    if let Some(kinds) = &sm.kinds {
        let mut kind_names = HashSet::new();
        for (i, kind) in kinds.iter().enumerate() {
            if kind.name.is_empty() {
                return Err(ParseError::Validation(format!(
                    "kind at index {} has an empty name",
                    i
                )));
            }
            if !kind_names.insert(&kind.name) {
                return Err(ParseError::Validation(format!(
                    "duplicate kind name '{}'",
                    kind.name
                )));
            }
            if kind.datasets.is_empty() {
                return Err(ParseError::Validation(format!(
                    "kind '{}' must have at least one dataset",
                    kind.name
                )));
            }

            // Check duplicate dataset names within a kind
            let mut ds_names = HashSet::new();
            for ds_entry in &kind.datasets {
                let ds_name = match ds_entry {
                    crate::schema::model::KindDatasetEntry::Inline(ds) => &ds.name,
                    crate::schema::model::KindDatasetEntry::Ref(r) => &r.ref_name,
                };
                if !ds_names.insert(ds_name) {
                    return Err(ParseError::Validation(format!(
                        "kind '{}': duplicate dataset name '{}'",
                        kind.name, ds_name
                    )));
                }
            }

            // Check duplicate dimension names within a kind
            if let Some(dims) = &kind.dimensions {
                let mut dim_names = HashSet::new();
                for dim_entry in dims {
                    let dim_name = match dim_entry {
                        DimensionEntry::Inline(d) => &d.name,
                        DimensionEntry::Ref(r) => &r.ref_name,
                    };
                    if !dim_names.insert(dim_name) {
                        return Err(ParseError::Validation(format!(
                            "kind '{}': duplicate dimension name '{}'",
                            kind.name, dim_name
                        )));
                    }
                }
            }

            // Check duplicate measure names within a kind
            if let Some(measures) = &kind.measures {
                let mut measure_names = HashSet::new();
                for m_entry in measures {
                    let m_name = match m_entry {
                        MeasureEntry::Inline(m) => &m.name,
                        MeasureEntry::Ref(r) => &r.ref_name,
                    };
                    if !measure_names.insert(m_name) {
                        return Err(ParseError::Validation(format!(
                            "kind '{}': duplicate measure name '{}'",
                            kind.name, m_name
                        )));
                    }
                }
            }
        }
    }

    // Check duplicate top-level dataset names
    if let Some(datasets) = &sm.datasets {
        let mut ds_names = HashSet::new();
        for ds in datasets {
            if !ds_names.insert(&ds.name) {
                return Err(ParseError::Validation(format!(
                    "duplicate dataset name '{}'",
                    ds.name
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_model() {
        let yaml = r#"
semantic_model:
  name: test_model
  datasets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
                - month
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parse_str(yaml).unwrap();
        assert_eq!(model.semantic_model.name, "test_model");
        let datasets = model.semantic_model.datasets.unwrap();
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].name, "orders");
        let dims = datasets[0].dimensions.as_ref().unwrap();
        assert_eq!(dims.len(), 1);
        let measures = datasets[0].measures.as_ref().unwrap();
        assert_eq!(measures.len(), 1);
    }

    #[test]
    fn test_parse_empty_name_fails() {
        let yaml = r#"
semantic_model:
  name: ""
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
    }

    #[test]
    fn test_parse_kind_with_grainset() {
        let yaml = r#"
semantic_model:
  name: kind_test
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
        - name: orders_daily
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount_usd
            storage:
              path: warehouse.orders_daily
"#;
        let model = parse_str(yaml).unwrap();
        let kinds = model.semantic_model.kinds.unwrap();
        assert_eq!(kinds.len(), 1);
        assert_eq!(kinds[0].name, "sales");
        assert_eq!(kinds[0].datasets.len(), 1);
    }

    #[test]
    fn test_parse_kind_empty_datasets_fails() {
        let yaml = r#"
semantic_model:
  name: bad_kind
  kinds:
    - name: empty_kind
      type:
        grainset:
      datasets: []
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
    }

    #[test]
    fn test_parse_dimension_types() {
        let yaml = r#"
semantic_model:
  name: dim_types
  datasets:
    - name: events
      dimensions:
        - name: event_date
          data_type: date
          type:
            temporal:
              grains:
                - day
                - week
                - month
        - name: category
          data_type: string
          type:
            categorical:
              enum:
                - web
                - mobile
                - api
        - name: is_active
          data_type: bool
          type:
            binary:
              type: boolean
        - name: price_bucket
          data_type: string
          type:
            bucketed:
              column: price
              buckets:
                - name: "low"
                  start: 0
                  end: 100
                - name: "high"
                  start: 100
                  end: 10000
      measures:
        - name: count
          data_type: int64
          expr: "COUNT(id)"
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let dims = datasets[0].dimensions.as_ref().unwrap();
        assert_eq!(dims.len(), 4);
    }

    #[test]
    fn test_parse_additivity() {
        let yaml = r#"
semantic_model:
  name: additivity_test
  datasets:
    - name: accounts
      measures:
        - name: balance
          data_type: float64
          expr: "SUM(balance)"
          additivity:
            type:
              semi:
                non_additive_dimensions:
                  - account_date
                resolution_strategy: latest
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let measures = datasets[0].measures.as_ref().unwrap();
        assert_eq!(measures.len(), 1);
    }

    #[test]
    fn test_parse_constraints() {
        let yaml = r#"
semantic_model:
  name: constraints_test
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
          constraints:
            dimensions:
              one_of:
                - order_date
                - created_at
              none_of:
                - internal_id
            aggregations:
              prohibited:
                - AVG
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let measures = datasets[0].measures.as_ref().unwrap();
        let m = match &measures[0] {
            crate::schema::model::MeasureEntry::Inline(m) => m,
            _ => panic!("expected inline measure"),
        };
        let constraints = m.constraints.as_ref().unwrap();
        assert!(constraints.dimensions.as_ref().unwrap().one_of.is_some());
        assert!(constraints.aggregations.as_ref().unwrap().prohibited.is_some());
    }

    #[test]
    fn test_parse_file_minimal() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/minimal.yaml"
        ));
        if path.exists() {
            let model = parse_file(path).unwrap();
            assert_eq!(model.semantic_model.name, "minimal_test");
        }
    }

    #[test]
    fn test_parse_ref_entry() {
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
        assert_eq!(dims.len(), 1);
        // Ref is resolved to inline by resolve_refs
        match &dims[0] {
            crate::schema::model::DimensionEntry::Inline(d) => {
                assert_eq!(d.name, "order_date");
            }
            _ => panic!("expected resolved inline dimension"),
        }
    }

    #[test]
    fn test_parse_domain() {
        let yaml = r#"
semantic_model:
  name: domain_test
  datasets:
    - name: orders
      domain: financial.transactions
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let domain = datasets[0].domain.as_ref().unwrap();
        assert_eq!(domain.0, vec!["financial.transactions"]);
    }

    #[test]
    fn test_parse_domain_array() {
        let yaml = r#"
semantic_model:
  name: domain_test
  datasets:
    - name: orders
      domain:
        - financial.transactions
        - marketing.orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let domain = datasets[0].domain.as_ref().unwrap();
        assert_eq!(domain.0.len(), 2);
    }

    #[test]
    fn test_parse_temporal_historization() {
        let yaml = r#"
semantic_model:
  name: temporal_test
  datasets:
    - name: accounts
      extras:
        temporal:
          type:
            scd:
              type_2:
                valid_from: effective_date
                valid_to: expiry_date
        storage:
          path: warehouse.accounts
      measures:
        - name: balance
          data_type: float64
          expr: "SUM(balance)"
"#;
        let model = parse_str(yaml).unwrap();
        let datasets = model.semantic_model.datasets.unwrap();
        let extras = datasets[0].extras.as_ref().unwrap();
        assert!(extras.temporal.is_some());
    }

    #[test]
    fn test_parse_file_grainset() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/grainset_basic.yaml"
        ));
        let model = parse_file(path).unwrap();
        let kinds = model.semantic_model.kinds.unwrap();
        assert_eq!(kinds[0].name, "sales");
        assert_eq!(kinds[0].datasets.len(), 2);
    }

    #[test]
    fn test_parse_file_unionset() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/unionset_basic.yaml"
        ));
        let model = parse_file(path).unwrap();
        let kinds = model.semantic_model.kinds.unwrap();
        assert_eq!(kinds[0].name, "all_events");
    }

    #[test]
    fn test_parse_file_joinset() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/joinset_basic.yaml"
        ));
        let model = parse_file(path).unwrap();
        let kinds = model.semantic_model.kinds.unwrap();
        assert_eq!(kinds[0].name, "order_details");
        assert!(kinds[0].relationships.is_some());
    }

    #[test]
    fn test_parse_file_full_model() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/full_model.yaml"
        ));
        let model = parse_file(path).unwrap();
        let sm = &model.semantic_model;
        assert_eq!(sm.name, "full_model");
        assert!(sm.datasets.is_some());
        assert!(sm.kinds.is_some());
        assert!(sm.relationships.is_some());
        // Refs should be resolved
        let datasets = sm.datasets.as_ref().unwrap();
        let dims = datasets[0].dimensions.as_ref().unwrap();
        // First dim was a ref: order_date, should now be inline
        match &dims[0] {
            crate::schema::model::DimensionEntry::Inline(d) => assert_eq!(d.name, "order_date"),
            _ => panic!("expected resolved inline dimension"),
        }
    }

    #[test]
    fn test_parse_file_invalid_missing_name() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/invalid/missing_name.yaml"
        ));
        let err = parse_file(path).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
    }

    #[test]
    fn test_parse_file_invalid_unknown_ref() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/invalid/unknown_ref.yaml"
        ));
        let err = parse_file(path).unwrap_err();
        assert!(matches!(err, ParseError::RefResolution(_)));
    }

    #[test]
    fn test_parse_file_invalid_empty_datasets() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test_data/invalid/grainset_empty_datasets.yaml"
        ));
        let err = parse_file(path).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
    }

    #[test]
    fn test_duplicate_kind_names_error() {
        let yaml = r#"
semantic_model:
  name: dupe_kind
  kinds:
    - name: sales
      type:
        grainset:
      datasets:
        - name: ds1
          extras:
            column_mapping: {}
    - name: sales
      type:
        grainset:
      datasets:
        - name: ds2
          extras:
            column_mapping: {}
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
        assert!(err.to_string().contains("duplicate kind name 'sales'"));
    }

    #[test]
    fn test_duplicate_dataset_in_kind_error() {
        let yaml = r#"
semantic_model:
  name: dupe_ds
  kinds:
    - name: sales
      type:
        grainset:
      datasets:
        - name: orders
          extras:
            column_mapping: {}
        - name: orders
          extras:
            column_mapping: {}
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
        assert!(err.to_string().contains("duplicate dataset name 'orders'"));
    }

    #[test]
    fn test_duplicate_dimension_in_kind_error() {
        let yaml = r#"
semantic_model:
  name: dupe_dim
  kinds:
    - name: sales
      type:
        grainset:
      dimensions:
        - name: order_date
          data_type: date
          type:
            categorical:
        - name: order_date
          data_type: string
          type:
            categorical:
      datasets:
        - name: ds1
          extras:
            column_mapping: {}
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
        assert!(err.to_string().contains("duplicate dimension name 'order_date'"));
    }

    #[test]
    fn test_duplicate_measure_in_kind_error() {
        let yaml = r#"
semantic_model:
  name: dupe_measure
  kinds:
    - name: sales
      type:
        grainset:
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
        - name: revenue
          data_type: float64
          expr: "SUM(total)"
      datasets:
        - name: ds1
          extras:
            column_mapping: {}
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
        assert!(err.to_string().contains("duplicate measure name 'revenue'"));
    }

    #[test]
    fn test_duplicate_top_level_dataset_error() {
        let yaml = r#"
semantic_model:
  name: dupe_top_ds
  datasets:
    - name: orders
      dimensions:
        - name: d
          data_type: string
          type:
            categorical:
    - name: orders
      dimensions:
        - name: d
          data_type: string
          type:
            categorical:
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)));
        assert!(err.to_string().contains("duplicate dataset name 'orders'"));
    }

    #[test]
    fn test_parse_relationships() {
        let yaml = r#"
semantic_model:
  name: rel_test
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
    - name: customers
      measures:
        - name: customer_count
          data_type: int64
          expr: "COUNT(id)"
  relationships:
    - name: orders_customers
      from: orders
      to: customers
      type: left
      columns:
        - from: customer_id
          to: id
      cardinality: many_to_one
"#;
        let model = parse_str(yaml).unwrap();
        let rels = model.semantic_model.relationships.unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].name, "orders_customers");
    }
}
