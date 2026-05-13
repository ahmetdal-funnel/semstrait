//! Parsing and reference resolution functions.

use crate::error::ModelError;
use crate::types::*;
use std::collections::{BTreeMap, HashMap};

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

    /// Intermediate YAML dataset with Vec fields (for array deserialization).
    /// Converted to SimpleDataKind with BTreeMap-based SemanticInterface after parse.
    #[derive(serde::Deserialize)]
    struct YamlDataset {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        ai_context: Option<AiContext>,
        #[serde(default)]
        keys: Option<Keys>,
        #[serde(default)]
        dimensions: Vec<DimensionEntry>,
        #[serde(default)]
        measures: Vec<MeasureEntry>,
        #[serde(default)]
        metrics: Vec<MetricEntry>,
        #[serde(default)]
        filters: Vec<MeasureFilter>,
        #[serde(default)]
        extras: Option<DatasetExtras>,
    }

    #[derive(serde::Deserialize)]
    struct YamlModel {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        ai_context: Option<AiContext>,
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        namespace: Option<String>,
        #[serde(default)]
        datasets: Vec<YamlDataset>,
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

    // Build unified entities map from datasets + grainsets + unionsets + joinsets.
    let mut entities = BTreeMap::new();

    // Convert YAML datasets to DataKind::Simple with BTreeMap-based SemanticInterface.
    for yd in m.datasets {
        let name = yd.name.clone();
        let dk = DataKind::Simple(SimpleDataKind {
            name: yd.name,
            interface: build_semantic_interface(
                &name,
                yd.description,
                yd.ai_context,
                yd.keys,
                yd.dimensions,
                yd.measures,
                yd.metrics,
                yd.filters,
            )?,
            extras: yd.extras,
        });
        insert_unique_entity(&mut entities, name, dk)?;
    }

    // Convert grainsets/unionsets/joinsets to DataKind::Complex variants.
    // Nested kind blocks are flattened: extracted as top-level entries with
    // ChildEntry::Ref added to the parent's children.
    for g in m.grainsets {
        flatten_grainset(&mut entities, g)?;
    }
    for u in m.unionsets {
        flatten_unionset(&mut entities, u)?;
    }
    for j in m.joinsets {
        flatten_joinset(&mut entities, j)?;
    }

    Ok(SemanticModel {
        name: m.name,
        description: m.description,
        ai_context: m.ai_context,
        labels: m.labels,
        namespace: m.namespace,
        entities,
        relationships: m.relationships,
        dimensions: m.dimensions,
        measures: m.measures,
        metrics: m.metrics,
    })
}

// =============================================================================
// Nested kind flattening
// =============================================================================
// Nested kind blocks (e.g., `unionsets:` inside a grainset) are syntactic sugar.
// Each nested kind is extracted as a standalone top-level entry in `entities`
// and a `ChildEntry::Ref` is added to the parent's `children` array.
// The nesting matrix restricts which combinations are valid (enforced in
// validate_structure, step 4).

fn insert_unique_entity(
    map: &mut BTreeMap<String, DataKind>,
    name: String,
    dk: DataKind,
) -> Result<(), ModelError> {
    if map.contains_key(&name) {
        return Err(ModelError::Validation(format!(
            "duplicate entity name: '{}'",
            name
        )));
    }
    map.insert(name, dk);
    Ok(())
}

fn flatten_grainset(
    entities: &mut BTreeMap<String, DataKind>,
    mut g: YamlGrainset,
) -> Result<(), ModelError> {
    // Extract nested kinds, flatten recursively, add refs to parent.
    for u in std::mem::take(&mut g.unionsets) {
        let ref_name = u.name.clone();
        flatten_unionset(entities, u)?;
        g.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Unionset)));
    }
    for j in std::mem::take(&mut g.joinsets) {
        let ref_name = j.name.clone();
        flatten_joinset(entities, j)?;
        g.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Joinset)));
    }
    let name = g.name.clone();
    insert_unique_entity(entities, name, DataKind::try_from(g)?)
}

fn flatten_unionset(
    entities: &mut BTreeMap<String, DataKind>,
    mut u: YamlUnionset,
) -> Result<(), ModelError> {
    for g in std::mem::take(&mut u.grainsets) {
        let ref_name = g.name.clone();
        flatten_grainset(entities, g)?;
        u.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Grainset)));
    }
    for nested_u in std::mem::take(&mut u.unionsets) {
        let ref_name = nested_u.name.clone();
        flatten_unionset(entities, nested_u)?;
        u.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Unionset)));
    }
    for j in std::mem::take(&mut u.joinsets) {
        let ref_name = j.name.clone();
        flatten_joinset(entities, j)?;
        u.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Joinset)));
    }
    let name = u.name.clone();
    insert_unique_entity(entities, name, DataKind::try_from(u)?)
}

fn flatten_joinset(
    entities: &mut BTreeMap<String, DataKind>,
    mut j: YamlJoinset,
) -> Result<(), ModelError> {
    for g in std::mem::take(&mut j.grainsets) {
        let ref_name = g.name.clone();
        flatten_grainset(entities, g)?;
        j.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Grainset)));
    }
    for u in std::mem::take(&mut j.unionsets) {
        let ref_name = u.name.clone();
        flatten_unionset(entities, u)?;
        j.datasets.push(ChildEntry::Ref(ChildRef::new(ref_name, DataKindVariant::Unionset)));
    }
    let name = j.name.clone();
    insert_unique_entity(entities, name, DataKind::try_from(j)?)
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

    // Resolve refs in all entities (datasets + grainsets + unionsets + joinsets)
    for (entity_name, dk) in &mut model.entities {
        let iface = dk.interface_mut();
        resolve_dimension_entries(&mut iface.dimensions, &dim_map, entity_name)?;
        resolve_measure_entries(&mut iface.measures, &measure_map, entity_name)?;
        resolve_metric_entries(&mut iface.metrics, &metric_map, entity_name)?;
    }

    Ok(model)
}

fn resolve_dimension_entries(
    entries: &mut BTreeMap<String, DimensionEntry>,
    map: &HashMap<&str, &Dimension>,
    entity_name: &str,
) -> Result<(), ModelError> {
    for entry in entries.values_mut() {
        if let DimensionEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = map.get(name.as_str()).ok_or_else(|| {
                let available: Vec<&str> = map.keys().copied().collect();
                ModelError::RefResolution(format!(
                    "unknown dimension ref '{}' in entity '{}'. Available dimensions: [{}]",
                    name,
                    entity_name,
                    available.join(", ")
                ))
            })?;
            *entry = DimensionEntry::Inline((*resolved).clone());
        }
    }
    Ok(())
}

fn resolve_measure_entries(
    entries: &mut BTreeMap<String, MeasureEntry>,
    map: &HashMap<&str, &Measure>,
    entity_name: &str,
) -> Result<(), ModelError> {
    for entry in entries.values_mut() {
        if let MeasureEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = map.get(name.as_str()).ok_or_else(|| {
                let available: Vec<&str> = map.keys().copied().collect();
                ModelError::RefResolution(format!(
                    "unknown measure ref '{}' in entity '{}'. Available measures: [{}]",
                    name,
                    entity_name,
                    available.join(", ")
                ))
            })?;
            *entry = MeasureEntry::Inline((*resolved).clone());
        }
    }
    Ok(())
}

fn resolve_metric_entries(
    entries: &mut BTreeMap<String, MetricEntry>,
    map: &HashMap<&str, &Metric>,
    entity_name: &str,
) -> Result<(), ModelError> {
    for entry in entries.values_mut() {
        if let MetricEntry::Ref(r) = entry {
            let name = &r.ref_name;
            let resolved = map.get(name.as_str()).ok_or_else(|| {
                let available: Vec<&str> = map.keys().copied().collect();
                ModelError::RefResolution(format!(
                    "unknown metric ref '{}' in entity '{}'. Available metrics: [{}]",
                    name,
                    entity_name,
                    available.join(", ")
                ))
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
        assert_eq!(model.entities.len(), 1);
        assert_eq!(model.entities.get("orders").unwrap().name(), "orders");
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
        let sales = model.entities.get("sales").unwrap();
        assert!(matches!(sales, DataKind::Complex(ComplexDataKind::Grainset(_))));
        assert_eq!(sales.children().unwrap().len(), 1);
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

        let orders = resolved.entities.get("orders").unwrap();
        assert_eq!(orders.interface().dimensions.len(), 1);

        match orders.interface().dimensions.values().next().unwrap() {
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

        let orders = resolved.entities.get("orders").unwrap();
        match orders.interface().measures.values().next().unwrap() {
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
        let sales = model.entities.get("sales").unwrap();
        let children = sales.children().unwrap();

        match &children[0] {
            ChildEntry::Inline(ds) => {
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
        let sales = model.entities.get("sales").unwrap();
        let children = sales.children().unwrap();

        match &children[0] {
            ChildEntry::Inline(ds) => {
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
        let sales = model.entities.get("sales").unwrap();
        let children = sales.children().unwrap();

        match &children[0] {
            ChildEntry::Inline(ds) => {
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
        let dataset = model.entities.get("accounts").unwrap();

        match dataset.interface().measures.values().next().unwrap() {
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
        let dk = model.entities.get("order_details").unwrap();

        match dk {
            DataKind::Complex(ComplexDataKind::Joinset(j)) => {
                assert_eq!(j.associativity, JoinAssociativity::Left);
                assert_eq!(j.relationships.len(), 1);
            }
            _ => panic!("expected joinset"),
        }
    }

    /// DL-049: Declarative ExprSource at kind level (Tier 2) must parse correctly.
    /// Previously failed because ExprSource used #[serde(untagged)] nested inside
    /// DimensionEntry (also untagged) — serde_yaml 0.9 limitation.
    #[test]
    fn test_kind_level_declarative_expr_dl049() {
        let yaml = r#"
semantic_model:
  name: dl049_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains:
                - day
        - name: raw_name
          data_type: string
        - name: name_upper
          data_type: string
          expr:
            upper: raw_name
      measures:
        - name: revenue
          data_type: f64
          agg: sum
      datasets:
        - name: orders
          extras:
            column_mapping:
              order_date: created_at
              raw_name: name
              revenue: amount
            storage:
              paths:
                - s3://data/orders/
"#;
        let model = parse(yaml).unwrap();
        let sales = model.entities.get("sales").unwrap();
        assert!(matches!(sales, DataKind::Complex(ComplexDataKind::Grainset(_))));

        // Verify declarative expr parsed on the kind-level dimension
        let iface = sales.interface();
        let dim = match iface.dimensions.get("name_upper").unwrap() {
            DimensionEntry::Inline(d) => d,
            DimensionEntry::Ref(_) => panic!("expected inline"),
        };
        assert!(dim.expr.is_some(), "declarative expr must parse at kind level");
        match dim.expr.as_ref().unwrap() {
            crate::expr_block::ExprSource::Declarative(_) => {} // correct
            crate::expr_block::ExprSource::Inline(s) => {
                panic!("expected Declarative, got Inline(\"{}\")", s)
            }
        }
    }

    /// SR-1 / SR-3: Duplicate dimension name in same container → error at parse time.
    #[test]
    fn test_duplicate_dimension_name_error() {
        let yaml = r#"
semantic_model:
  name: dup_test
  datasets:
    - name: orders
      dimensions:
        - name: country
          data_type: string
        - name: country
          data_type: string
      measures:
        - name: revenue
          data_type: f64
          agg: sum
"#;
        let result = parse(yaml);
        assert!(result.is_err(), "duplicate dimension should cause error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate") && err.contains("country"), "error: {}", err);
    }

    /// SR-4: Dataset nested in a kind with semantic fields → deserialization error.
    #[test]
    fn test_dataset_binding_rejects_semantic_fields() {
        let yaml = r#"
semantic_model:
  name: sr4_test
  grainsets:
    - name: sales
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: [day]
      measures:
        - name: revenue
          data_type: f64
          agg: sum
      datasets:
        - name: orders
          dimensions:
            - name: country
              data_type: string
          extras:
            column_mapping:
              order_date: created_at
              revenue: amount
            storage:
              paths:
                - s3://data/orders/
"#;
        let result = parse(yaml);
        assert!(result.is_err(), "dataset binding with dimensions should be rejected");
    }

    /// SR-1: Duplicate measure name in same container → error.
    #[test]
    fn test_duplicate_measure_name_error() {
        let yaml = r#"
semantic_model:
  name: dup_test
  datasets:
    - name: orders
      dimensions:
        - name: order_date
          data_type: date
          type:
            temporal:
              grains: [day]
      measures:
        - name: revenue
          data_type: f64
          agg: sum
        - name: revenue
          data_type: f64
          agg: avg
"#;
        let result = parse(yaml);
        assert!(result.is_err(), "duplicate measure should cause error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate") && err.contains("revenue"), "error: {}", err);
    }

    #[test]
    fn test_unknown_ref_error_includes_entity_name() {
        let yaml = r#"
semantic_model:
  name: ref_test
  datasets:
    - name: orders
      dimensions:
        - ref: nonexistent_dim
      measures: []
      metrics: []
  dimensions:
    - name: date
      data_type: date
      type:
        temporal:
          grains: [day]
"#;
        let model = parse(yaml).unwrap();
        let result = resolve_refs(model);
        match result {
            Err(ModelError::RefResolution(msg)) => {
                assert!(msg.contains("orders"),
                    "error must include entity name 'orders': {}", msg);
                assert!(msg.contains("nonexistent_dim"),
                    "error must include ref name: {}", msg);
                assert!(msg.contains("date"),
                    "error must list available dimensions: {}", msg);
            }
            other => panic!("expected RefResolution, got {:?}", other),
        }
    }

    #[test]
    fn test_unknown_measure_ref_error_includes_entity_name() {
        let yaml = r#"
semantic_model:
  name: ref_test
  datasets:
    - name: orders
      dimensions: []
      measures:
        - ref: nonexistent_measure
      metrics: []
  measures:
    - name: revenue
      data_type: f64
      agg: sum
"#;
        let model = parse(yaml).unwrap();
        let result = resolve_refs(model);
        match result {
            Err(ModelError::RefResolution(msg)) => {
                assert!(msg.contains("orders"),
                    "error must include entity name 'orders': {}", msg);
                assert!(msg.contains("nonexistent_measure"),
                    "error must include ref name: {}", msg);
                assert!(msg.contains("revenue"),
                    "error must list available measures: {}", msg);
            }
            other => panic!("expected RefResolution, got {:?}", other),
        }
    }
}
