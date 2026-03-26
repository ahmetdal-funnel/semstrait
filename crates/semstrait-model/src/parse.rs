//! Parsing and reference resolution functions.

use crate::error::ModelError;
use crate::types::*;
use std::collections::HashMap;

/// Substitute `${VAR}` patterns in a string with environment variable values.
///
/// Only `${IDENTIFIER}` syntax is supported (not bare `$VAR`).
/// Returns an error if any referenced environment variable is not set.
pub(crate) fn substitute_env_vars(input: &str) -> Result<String, ModelError> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(c) => var_name.push(c),
                    None => {
                        return Err(ModelError::EnvVar(
                            "unterminated ${...} expression".to_owned(),
                        ));
                    }
                }
            }
            if var_name.is_empty() {
                return Err(ModelError::EnvVar("empty variable name in ${}".to_owned()));
            }
            match std::env::var(&var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => {
                    return Err(ModelError::EnvVar(format!(
                        "environment variable '{}' is not set",
                        var_name
                    )));
                }
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

/// Parse a YAML string into a SemanticModel.
///
/// Performs environment variable substitution (`${VAR}`) followed by YAML
/// deserialization. Reference resolution is separate.
///
/// # Example
///
/// ```rust,ignore
/// let yaml = std::fs::read_to_string("model.yaml")?;
/// let model = semstrait_model::parse(&yaml)?;
/// ```
pub fn parse(yaml: &str) -> Result<SemanticModel, ModelError> {
    let yaml = &substitute_env_vars(yaml)?;
    // Intermediate YAML-facing root with implicit kind types.
    #[derive(serde::Deserialize)]
    struct YamlRoot {
        semantic_model: YamlModel,
    }

    #[derive(serde::Deserialize)]
    struct YamlModel {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        ai_context: Option<String>,
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        namespace: Option<String>,
        #[serde(default)]
        datasets: Vec<Dataset>,
        #[serde(default)]
        grainsets: Vec<YamlGrainset>,
        #[serde(default)]
        unionsets: Vec<YamlUnionset>,
        #[serde(default)]
        joinsets: Vec<YamlJoinset>,
        #[serde(default)]
        relationships: Vec<Relationship>,
        #[serde(default)]
        dimensions: Vec<Dimension>,
        #[serde(default)]
        measures: Vec<Measure>,
        #[serde(default)]
        metrics: Vec<Metric>,
    }

    let root: YamlRoot = serde_yaml::from_str(yaml)?;
    let m = root.semantic_model;

    // Merge grainsets/unionsets/joinsets into kinds.
    let mut kinds: Vec<Kind> = Vec::new();
    kinds.extend(m.grainsets.into_iter().map(Kind::from));
    kinds.extend(m.unionsets.into_iter().map(Kind::from));
    kinds.extend(m.joinsets.into_iter().map(Kind::from));

    Ok(SemanticModel {
        name: m.name,
        description: m.description,
        ai_context: m.ai_context,
        labels: m.labels,
        namespace: m.namespace,
        datasets: m.datasets,
        kinds,
        relationships: m.relationships,
        dimensions: m.dimensions,
        measures: m.measures,
        metrics: m.metrics,
    })
}

/// Resolve all `ref:` entries in the model.
///
/// Replaces `DimensionEntry::Ref`, `MeasureEntry::Ref`, and `MetricEntry::Ref`
/// with inline definitions from the top-level arrays.
///
/// Returns an error if a reference target is not found.
///
/// # Example
///
/// ```rust,ignore
/// let model = semstrait_model::parse(&yaml)?;
/// let resolved = semstrait_model::resolve_refs(model)?;
/// ```
pub fn resolve_refs(mut model: SemanticModel) -> Result<SemanticModel, ModelError> {
    // Build lookup maps from top-level reusable definitions (references avoid cloning).
    let dim_map: HashMap<&str, &Dimension> = model
        .dimensions
        .iter()
        .map(|d| (d.name.as_str(), d))
        .collect();

    let measure_map: HashMap<&str, &Measure> = model
        .measures
        .iter()
        .map(|m| (m.name.as_str(), m))
        .collect();

    let metric_map: HashMap<&str, &Metric> = model
        .metrics
        .iter()
        .map(|m| (m.name.as_str(), m))
        .collect();

    // Resolve refs in datasets
    for dataset in &mut model.datasets {
        resolve_dimension_entries(&mut dataset.dimensions, &dim_map)?;
        resolve_measure_entries(&mut dataset.measures, &measure_map)?;
        resolve_metric_entries(&mut dataset.metrics, &metric_map)?;
    }

    // Resolve refs in kinds
    for kind in &mut model.kinds {
        resolve_dimension_entries(&mut kind.dimensions, &dim_map)?;
        resolve_measure_entries(&mut kind.measures, &measure_map)?;
        resolve_metric_entries(&mut kind.metrics, &metric_map)?;
    }

    Ok(model)
}

fn resolve_dimension_entries(
    entries: &mut [DimensionEntry],
    map: &HashMap<&str, &Dimension>,
) -> Result<(), ModelError> {
    for entry in entries.iter_mut() {
        if let DimensionEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = map.get(name.as_str()).ok_or_else(|| {
                ModelError::RefResolution(format!("unknown dimension ref: '{}'", name))
            })?;
            *entry = DimensionEntry::Inline((*resolved).clone());
        }
    }
    Ok(())
}

fn resolve_measure_entries(
    entries: &mut [MeasureEntry],
    map: &HashMap<&str, &Measure>,
) -> Result<(), ModelError> {
    for entry in entries.iter_mut() {
        if let MeasureEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = map.get(name.as_str()).ok_or_else(|| {
                ModelError::RefResolution(format!("unknown measure ref: '{}'", name))
            })?;
            *entry = MeasureEntry::Inline((*resolved).clone());
        }
    }
    Ok(())
}

fn resolve_metric_entries(
    entries: &mut [MetricEntry],
    map: &HashMap<&str, &Metric>,
) -> Result<(), ModelError> {
    for entry in entries.iter_mut() {
        if let MetricEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = map.get(name.as_str()).ok_or_else(|| {
                ModelError::RefResolution(format!("unknown metric ref: '{}'", name))
            })?;
            *entry = MetricEntry::Inline((*resolved).clone());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal() {
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
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parse(yaml).unwrap();
        assert_eq!(model.name, "test_model");
        assert_eq!(model.datasets.len(), 1);
        assert_eq!(model.datasets[0].name, "orders");
    }

    #[test]
    fn test_parse_with_kinds() {
        let yaml = r#"
semantic_model:
  name: kind_test
  grainsets:
    - name: sales
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
              paths:
                - warehouse.orders_daily
"#;
        let model = parse(yaml).unwrap();
        assert_eq!(model.kinds.len(), 1);
        assert_eq!(model.kinds[0].name, "sales");
        assert_eq!(model.kinds[0].datasets.len(), 1);
    }

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
        let model = parse(yaml).unwrap();
        let resolved = resolve_refs(model).unwrap();

        assert_eq!(resolved.datasets.len(), 1);
        assert_eq!(resolved.datasets[0].dimensions.len(), 1);

        match &resolved.datasets[0].dimensions[0] {
            DimensionEntry::Inline(d) => assert_eq!(d.name, "order_date"),
            DimensionEntry::Ref(_) => panic!("expected ref to be resolved"),
        }
    }

    #[test]
    fn test_resolve_measure_ref() {
        let yaml = r#"
semantic_model:
  name: ref_test
  measures:
    - name: revenue
      data_type: float64
      expr: "SUM(amount)"
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
        - ref: revenue
"#;
        let model = parse(yaml).unwrap();
        let resolved = resolve_refs(model).unwrap();

        match &resolved.datasets[0].measures[0] {
            MeasureEntry::Inline(m) => assert_eq!(m.name, "revenue"),
            MeasureEntry::Ref(_) => panic!("expected ref to be resolved"),
        }
    }

    #[test]
    fn test_unknown_ref_error() {
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
        let model = parse(yaml).unwrap();
        let result = resolve_refs(model);
        assert!(result.is_err());

        match result {
            Err(ModelError::RefResolution(msg)) => {
                assert!(msg.contains("nonexistent"));
            }
            _ => panic!("expected RefResolution error"),
        }
    }

    #[test]
    fn test_parse_domain_single() {
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
        let model = parse(yaml).unwrap();
        let domain = model.datasets[0].domain.as_ref().unwrap();
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
        let model = parse(yaml).unwrap();
        let domain = model.datasets[0].domain.as_ref().unwrap();
        assert_eq!(domain.0.len(), 2);
        assert_eq!(domain.0[0], "financial.transactions");
        assert_eq!(domain.0[1], "marketing.orders");
    }

    #[test]
    fn test_parse_column_mapping_simple() {
        let yaml = r#"
semantic_model:
  name: mapping_test
  grainsets:
    - name: sales
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
              paths:
                - warehouse.orders_daily
"#;
        let model = parse(yaml).unwrap();
        let kind = &model.kinds[0];

        match &kind.datasets[0] {
            KindDatasetEntry::Inline(ds) => {
                let mapping = &ds.extras.column_mapping;
                assert!(mapping.contains_key("order_date"));
                assert!(mapping.contains_key("revenue"));

                match mapping.get("order_date").unwrap() {
                    ColumnMappingValue::Simple(s) => assert_eq!(s, "created_at"),
                    _ => panic!("expected simple mapping"),
                }
            }
            _ => panic!("expected inline dataset"),
        }
    }

    #[test]
    fn test_parse_column_mapping_with_grain() {
        let yaml = r#"
semantic_model:
  name: mapping_test
  grainsets:
    - name: sales
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
      datasets:
        - name: orders_monthly
          extras:
            column_mapping:
              order_date:
                column: order_month
                grain: month
              revenue: total_revenue
            storage:
              paths:
                - warehouse.orders_monthly
"#;
        let model = parse(yaml).unwrap();
        let kind = &model.kinds[0];

        match &kind.datasets[0] {
            KindDatasetEntry::Inline(ds) => {
                let mapping = &ds.extras.column_mapping;

                match mapping.get("order_date").unwrap() {
                    ColumnMappingValue::WithGrain { column, grain } => {
                        assert_eq!(column, "order_month");
                        assert_eq!(*grain, Some(TemporalGrain::Month));
                    }
                    _ => panic!("expected with_grain mapping"),
                }
            }
            _ => panic!("expected inline dataset"),
        }
    }

    #[test]
    fn test_parse_glob_pattern() {
        let yaml = r#"
semantic_model:
  name: glob_test
  grainsets:
    - name: sales
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
        - name: "orders_*"
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              paths:
                - warehouse.orders
"#;
        let model = parse(yaml).unwrap();
        let kind = &model.kinds[0];

        match &kind.datasets[0] {
            KindDatasetEntry::Inline(ds) => {
                match &ds.name {
                    DatasetName::Glob(pattern) => {
                        assert_eq!(pattern.0, "orders_*");
                    }
                    _ => panic!("expected glob pattern"),
                }
            }
            _ => panic!("expected inline dataset"),
        }
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
            semi:
              non_additive_dimensions:
                - account_date
              resolution_strategy: latest
"#;
        let model = parse(yaml).unwrap();
        let dataset = &model.datasets[0];

        match &dataset.measures[0] {
            MeasureEntry::Inline(m) => {
                assert!(m.additivity.is_some());
                match m.additivity.as_ref().unwrap() {
                    AdditivityType::Semi(semi) => {
                        assert_eq!(semi.non_additive_dimensions.len(), 1);
                        assert_eq!(semi.resolution_strategy, ResolutionStrategy::Latest);
                    }
                    _ => panic!("expected semi additivity"),
                }
            }
            _ => panic!("expected inline measure"),
        }
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
        let model = parse(yaml).unwrap();
        assert_eq!(model.relationships.len(), 1);

        let rel = &model.relationships[0];
        assert_eq!(rel.name, "orders_customers");
        assert_eq!(rel.from, "orders");
        assert_eq!(rel.to, "customers");
        assert_eq!(rel.join_type, JoinType::Left);
        assert_eq!(rel.cardinality, Cardinality::ManyToOne);
        assert_eq!(rel.columns.len(), 1);
    }

    #[test]
    fn test_parse_labels() {
        let yaml = r#"
semantic_model:
  name: labels_test
  labels:
    - production
    - finance
  datasets:
    - name: orders
      measures:
        - name: revenue
          data_type: float64
          expr: "SUM(amount)"
"#;
        let model = parse(yaml).unwrap();
        assert_eq!(model.labels.len(), 2);
        assert_eq!(model.labels[0], "production");
        assert_eq!(model.labels[1], "finance");
    }

    #[test]
    fn test_substitute_env_vars_basic() {
        std::env::set_var("SEMSTRAIT_TEST_VAR", "hello");
        let result = substitute_env_vars("value: ${SEMSTRAIT_TEST_VAR}").unwrap();
        assert_eq!(result, "value: hello");
        std::env::remove_var("SEMSTRAIT_TEST_VAR");
    }

    #[test]
    fn test_substitute_env_vars_missing() {
        std::env::remove_var("SEMSTRAIT_NONEXISTENT_VAR");
        let result = substitute_env_vars("value: ${SEMSTRAIT_NONEXISTENT_VAR}");
        assert!(result.is_err());
        match result {
            Err(ModelError::EnvVar(msg)) => {
                assert!(msg.contains("SEMSTRAIT_NONEXISTENT_VAR"));
            }
            _ => panic!("expected EnvVar error"),
        }
    }

    #[test]
    fn test_substitute_env_vars_no_placeholders() {
        let input = "plain: yaml\nno: vars";
        let result = substitute_env_vars(input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_substitute_env_vars_multiple() {
        std::env::set_var("SEMSTRAIT_A", "one");
        std::env::set_var("SEMSTRAIT_B", "two");
        let result = substitute_env_vars("${SEMSTRAIT_A} and ${SEMSTRAIT_B}").unwrap();
        assert_eq!(result, "one and two");
        std::env::remove_var("SEMSTRAIT_A");
        std::env::remove_var("SEMSTRAIT_B");
    }

    #[test]
    fn test_substitute_env_vars_unterminated() {
        let result = substitute_env_vars("value: ${UNTERMINATED");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_joinset() {
        let yaml = r#"
semantic_model:
  name: joinset_test
  joinsets:
    - name: order_details
      associativity: left
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
              order_date: created_at
              revenue: amount
            storage:
              paths:
                - warehouse.orders
        - name: customers
          extras:
            column_mapping: {}
            storage:
              paths:
                - warehouse.customers
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
        let model = parse(yaml).unwrap();
        let kind = &model.kinds[0];

        match &kind.kind_type {
            KindTypeSpec::Joinset(config) => {
                assert_eq!(config.associativity, JoinAssociativity::Left);
            }
            _ => panic!("expected joinset"),
        }

        assert_eq!(kind.relationships.len(), 1);
    }
}
