//! Nesting matrix enforcement for kind-in-kind structures.
//!
//! Rules (from schema v1.2):
//! - grainset → grainset: ERROR
//! - joinset  → joinset:  ERROR
//! - unionset → unionset: WARNING (COMP_W010) — allowed at parse time
//! - Max nesting depth: 2

use std::collections::HashMap;

use crate::parser::ParseError;
use crate::schema::model::{KindDatasetEntry, KindType, SemanticModelFile};

/// Validate the nesting matrix rules for kinds.
///
/// Detects kind-in-kind nesting by matching dataset entry names against kind
/// names. Returns `Err(ParseError::Nesting(...))` for illegal nesting
/// (grainset→grainset, joinset→joinset, or depth > 2). Unionset→unionset
/// is permitted at parse time (a warning can be emitted downstream).
pub fn validate_nesting(model: &SemanticModelFile) -> Result<(), ParseError> {
    let kinds = match &model.semantic_model.kinds {
        Some(k) if !k.is_empty() => k,
        _ => return Ok(()),
    };

    // Build kind_name → kind_type lookup
    let kind_types: HashMap<&str, &KindType> = kinds
        .iter()
        .map(|k| (k.name.as_str(), &k.kind_type))
        .collect();

    // Build adjacency: kind_name → vec of referenced kind names
    let mut refs: HashMap<&str, Vec<&str>> = HashMap::new();
    for kind in kinds {
        let mut kind_refs = Vec::new();
        for ds_entry in &kind.datasets {
            let ds_name = dataset_entry_name(ds_entry);
            if ds_name != kind.name && kind_types.contains_key(ds_name) {
                kind_refs.push(ds_name);
            }
        }
        refs.insert(kind.name.as_str(), kind_refs);
    }

    // Check nesting rules
    for kind in kinds {
        let empty = Vec::new();
        let children = refs.get(kind.name.as_str()).unwrap_or(&empty);
        for &child_name in children {
            let child_type = kind_types[child_name];

            // Same-type nesting: grainset→grainset and joinset→joinset are errors
            if is_same_type_error(&kind.kind_type, child_type) {
                let type_name = kind_type_name(&kind.kind_type);
                return Err(ParseError::Nesting(format!(
                    "{type_name} '{}' cannot nest {type_name} '{child_name}'",
                    kind.name,
                )));
            }

            // Depth check: child must not itself reference any other kind
            let grandchildren = refs.get(child_name).unwrap_or(&empty);
            if !grandchildren.is_empty() {
                return Err(ParseError::Nesting(format!(
                    "nesting depth exceeds 2: '{}' -> '{child_name}' -> '{}'",
                    kind.name, grandchildren[0],
                )));
            }
        }
    }

    Ok(())
}

fn dataset_entry_name(entry: &KindDatasetEntry) -> &str {
    match entry {
        KindDatasetEntry::Inline(ds) => &ds.name,
        KindDatasetEntry::Ref(r) => &r.ref_name,
    }
}

fn is_same_type_error(parent: &KindType, child: &KindType) -> bool {
    matches!(
        (parent, child),
        (KindType::Grainset, KindType::Grainset)
            | (KindType::Joinset(_), KindType::Joinset(_))
    )
}

fn kind_type_name(kt: &KindType) -> &'static str {
    match kt {
        KindType::Grainset => "grainset",
        KindType::Unionset => "unionset",
        KindType::Joinset(_) => "joinset",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;

    #[test]
    fn test_nesting_validation_passes_simple() {
        let yaml = r#"
semantic_model:
  name: nesting_test
  kinds:
    - name: sales
      type:
        grainset:
      datasets:
        - name: orders_daily
          extras:
            column_mapping: {}
"#;
        let model = parse_str(yaml).unwrap();
        assert!(validate_nesting(&model).is_ok());
    }

    #[test]
    fn test_grainset_nesting_grainset_error() {
        let yaml = r#"
semantic_model:
  name: nested
  kinds:
    - name: inner_grain
      type:
        grainset:
      datasets:
        - name: raw_data
          extras:
            column_mapping: {}
    - name: outer_grain
      type:
        grainset:
      datasets:
        - name: inner_grain
          extras:
            column_mapping: {}
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Nesting(_)), "expected Nesting error: {err}");
        assert!(err.to_string().contains("grainset"), "error should mention grainset: {err}");
    }

    #[test]
    fn test_joinset_nesting_joinset_error() {
        let yaml = r#"
semantic_model:
  name: nested_joinset
  kinds:
    - name: inner_join
      type:
        joinset:
          associativity: left
      datasets:
        - name: table_a
          extras:
            column_mapping: {}
        - name: table_b
          extras:
            column_mapping: {}
      relationships:
        - name: a_to_b
          from: table_a
          to: table_b
          type: left
          cardinality: many_to_one
          columns:
            - from: id
              to: id
    - name: outer_join
      type:
        joinset:
          associativity: left
      datasets:
        - name: inner_join
          extras:
            column_mapping: {}
        - name: table_c
          extras:
            column_mapping: {}
      relationships:
        - name: ij_to_c
          from: inner_join
          to: table_c
          type: left
          cardinality: many_to_one
          columns:
            - from: id
              to: id
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Nesting(_)), "expected Nesting error: {err}");
        assert!(err.to_string().contains("joinset"), "error should mention joinset: {err}");
    }

    #[test]
    fn test_unionset_nesting_unionset_ok() {
        let yaml = r#"
semantic_model:
  name: nested_union
  kinds:
    - name: inner_union
      type:
        unionset:
      datasets:
        - name: source_a
          extras:
            column_mapping: {}
    - name: outer_union
      type:
        unionset:
      datasets:
        - name: inner_union
          extras:
            column_mapping: {}
"#;
        let model = parse_str(yaml).unwrap();
        assert!(validate_nesting(&model).is_ok());
    }

    #[test]
    fn test_cross_type_nesting_ok() {
        let yaml = r#"
semantic_model:
  name: cross_type
  kinds:
    - name: inner_union
      type:
        unionset:
      datasets:
        - name: source_a
          extras:
            column_mapping: {}
    - name: outer_grain
      type:
        grainset:
      datasets:
        - name: inner_union
          extras:
            column_mapping: {}
"#;
        let model = parse_str(yaml).unwrap();
        assert!(validate_nesting(&model).is_ok());
    }

    #[test]
    fn test_depth_exceeds_two_error() {
        // Use cross-type nesting so same-type check doesn't fire
        let yaml = r#"
semantic_model:
  name: deep_nesting
  kinds:
    - name: level_c
      type:
        unionset:
      datasets:
        - name: raw_data
          extras:
            column_mapping: {}
    - name: level_b
      type:
        unionset:
      datasets:
        - name: level_c
          extras:
            column_mapping: {}
    - name: level_a
      type:
        grainset:
      datasets:
        - name: level_b
          extras:
            column_mapping: {}
"#;
        let err = parse_str(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Nesting(_)), "expected Nesting error: {err}");
        assert!(err.to_string().contains("depth"), "error should mention depth: {err}");
    }

    #[test]
    fn test_no_nesting_no_kinds() {
        let yaml = r#"
semantic_model:
  name: no_kinds
  datasets:
    - name: orders
      dimensions:
        - name: d
          data_type: string
          type:
            categorical:
"#;
        let model = parse_str(yaml).unwrap();
        assert!(validate_nesting(&model).is_ok());
    }

    #[test]
    fn test_dataset_not_a_kind_ok() {
        let yaml = r#"
semantic_model:
  name: no_match
  kinds:
    - name: sales
      type:
        grainset:
      datasets:
        - name: regular_dataset
          extras:
            column_mapping: {}
    - name: returns
      type:
        grainset:
      datasets:
        - name: another_dataset
          extras:
            column_mapping: {}
"#;
        let model = parse_str(yaml).unwrap();
        assert!(validate_nesting(&model).is_ok());
    }
}
