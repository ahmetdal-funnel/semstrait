//! Public `parse` and `parse_catalogs` entry points (`32 §9.1`,
//! `32b §5.1`).

use crate::catalogs::CatalogsConfig;
use crate::error::catalogs::CatalogsParseErrorKind;
use crate::error::parse::ParseErrorKind;
use crate::model::SemanticModel;
use crate::yaml::env::{substitute_env_for_catalogs, substitute_env_for_model};
use crate::yaml::YamlRoot;
use semstrait_core::diagnostic::{split_by_severity, Diagnostic, Diagnostics};

const DEFAULT_SOURCE: &str = "<inline>";

/// YAML `&str` → [`SemanticModel`]. Pure, synchronous, accumulating
/// per `32 §9.1`. Equivalent to `parse_with_source(input, "<inline>")`.
pub fn parse(
    input: &str,
) -> Result<(SemanticModel, Diagnostics<ParseErrorKind>), Diagnostics<ParseErrorKind>> {
    parse_with_source(input, DEFAULT_SOURCE)
}

/// As [`parse`], but the supplied `source` label is attached to every
/// diagnostic [`semstrait_core::Location`].
pub fn parse_with_source(
    input: &str,
    source: &str,
) -> Result<(SemanticModel, Diagnostics<ParseErrorKind>), Diagnostics<ParseErrorKind>> {
    // ── §8 env-var substitution. Catastrophic failure short-circuits.
    let expanded = match substitute_env_for_model(input) {
        Ok(s) => s,
        Err(diag) => return Err(vec![diag]),
    };

    // ── YAML decode into the array-form intermediate.
    let root: YamlRoot = match serde_yaml::from_str(&expanded) {
        Ok(r) => r,
        Err(e) => {
            let kind = classify_yaml_error(&e);
            return Err(vec![Diagnostic::new(kind)]);
        }
    };

    let (model, diagnostics) = root.lower(source);

    let (errors, warnings) = split_by_severity(diagnostics);
    if errors.is_empty() {
        Ok((model, warnings))
    } else {
        let mut combined = errors;
        combined.extend(warnings);
        Err(combined)
    }
}

/// `catalogs.yaml` → [`CatalogsConfig`]. Pure, synchronous,
/// accumulating per `32b §5.1`.
pub fn parse_catalogs(
    input: &str,
) -> Result<
    (CatalogsConfig, Diagnostics<CatalogsParseErrorKind>),
    Diagnostics<CatalogsParseErrorKind>,
> {
    parse_catalogs_with_source(input, DEFAULT_SOURCE)
}

pub fn parse_catalogs_with_source(
    input: &str,
    _source: &str,
) -> Result<
    (CatalogsConfig, Diagnostics<CatalogsParseErrorKind>),
    Diagnostics<CatalogsParseErrorKind>,
> {
    let expanded = match substitute_env_for_catalogs(input) {
        Ok(s) => s,
        Err(diag) => return Err(vec![diag]),
    };

    let cfg: CatalogsConfig = match serde_yaml::from_str(&expanded) {
        Ok(c) => c,
        Err(e) => {
            let kind = classify_catalogs_yaml_error(&e);
            return Err(vec![Diagnostic::new(kind)]);
        }
    };

    Ok((cfg, Vec::new()))
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Map a `serde_yaml::Error` to the most specific [`ParseErrorKind`]
/// we can derive from its message text. We deliberately avoid pulling
/// `serde_yaml`'s internal location / span APIs here — the diagnostic
/// envelope's `Location` field is the primary site for span detail
/// (`30 §5.1`), and parse-stage callers can attach a richer location
/// at the [`Diagnostic`] envelope when they have one.
fn classify_yaml_error(e: &serde_yaml::Error) -> ParseErrorKind {
    let msg = e.to_string();

    // serde_yaml emits messages like
    //   "missing field `name` at line N column M"
    //   "unknown field `xyz`, expected one of …"
    // We parse the prefix to identify SR-* matches; everything else
    // falls through to YamlSyntax.
    if msg.starts_with("unknown field `") {
        // unknown field `xyz` -> capture xyz between back-ticks.
        if let Some(rest) = msg.strip_prefix("unknown field `") {
            if let Some(end) = rest.find('`') {
                let field = &rest[..end];
                return ParseErrorKind::UnknownField {
                    field: field.to_string(),
                    parent: "<unknown>".to_string(),
                };
            }
        }
    }

    ParseErrorKind::YamlSyntax { message: msg }
}

fn classify_catalogs_yaml_error(e: &serde_yaml::Error) -> CatalogsParseErrorKind {
    let msg = e.to_string();
    if msg.starts_with("unknown field `") {
        if let Some(rest) = msg.strip_prefix("unknown field `") {
            if let Some(end) = rest.find('`') {
                let field = &rest[..end];
                return CatalogsParseErrorKind::UnknownField {
                    field: field.to_string(),
                    parent: "<unknown>".to_string(),
                };
            }
        }
    }

    if msg.starts_with("missing field `") {
        // missing field `name` -> CatalogMissingField
        if let Some(rest) = msg.strip_prefix("missing field `") {
            if let Some(end) = rest.find('`') {
                let field = &rest[..end];
                return CatalogsParseErrorKind::CatalogMissingField {
                    alias: "<unknown>".to_string(),
                    field: field.to_string(),
                };
            }
        }
    }

    CatalogsParseErrorKind::YamlSyntax { message: msg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_model() {
        let yaml = r#"
semantic_model:
  name: tiny
"#;
        let (m, warnings) = parse(yaml).expect("parse should succeed");
        assert_eq!(m.name, "tiny");
        assert!(m.is_empty_model());
        assert!(warnings.is_empty());
    }

    #[test]
    fn parse_unknown_top_level_block() {
        let yaml = r#"
semantic_model:
  name: x
  bogus_block: 1
"#;
        let err = parse(yaml).unwrap_err();
        assert!(err.iter().any(|d| matches!(
            d.kind,
            ParseErrorKind::UnknownField { .. } | ParseErrorKind::YamlSyntax { .. }
        )));
    }

    #[test]
    fn parse_dataset_with_single_dimension() {
        let yaml = r#"
semantic_model:
  name: ana
  datasets:
    - name: orders
      dimensions:
        - name: country
          data_type: string
          type: categorical
"#;
        let (m, warnings) = parse(yaml).expect("ok");
        assert!(warnings.is_empty());
        let ds = m.datasets.get("orders").expect("orders present");
        assert_eq!(ds.body.base.name, "orders");
        assert_eq!(ds.semantic_interface.dimensions.len(), 1);
    }

    #[test]
    fn parse_collects_duplicate_data_kind_names() {
        let yaml = r#"
semantic_model:
  name: dup
  datasets:
    - name: x
  grainsets:
    - name: x
      datasets:
        - name: a
        - name: b
"#;
        let err = parse(yaml).unwrap_err();
        assert!(err
            .iter()
            .any(|d| matches!(d.kind, ParseErrorKind::DuplicateDataKindName { .. })));
    }

    #[test]
    fn parse_substitutes_env_var() {
        std::env::set_var("SEMSTRAIT_TEST_VAR", "ana");
        let yaml = r#"
semantic_model:
  name: ${SEMSTRAIT_TEST_VAR}
"#;
        let (m, _) = parse(yaml).unwrap();
        assert_eq!(m.name, "ana");
        std::env::remove_var("SEMSTRAIT_TEST_VAR");
    }

    #[test]
    fn parse_unset_env_var_is_fatal() {
        let yaml = r#"
semantic_model:
  name: ${THIS_VAR_DOES_NOT_EXIST_xyz}
"#;
        let err = parse(yaml).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(err[0].kind, ParseErrorKind::UnsetEnvVar { .. }));
    }
}
