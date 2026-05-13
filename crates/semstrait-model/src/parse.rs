//! Public `parse` and `parse_catalogs` entry points (`32 §9.1`,
//! `32b §5.1`).
//!
//! Per D-10, `parse` is now a thin delegate over
//! [`YamlRoot::lower`]: it env-substitutes, decodes the YAML, lowers
//! into a [`SemanticModelBuilder`], and runs the SR-8 identifier-grammar
//! pass. Duplicate-name detection (SR-3 / SR-E-3) has moved to
//! [`SemanticModelBuilder::build`] so single-file and cross-file merges
//! are checked uniformly.

use crate::builder::model::semantic_model_builder;
use crate::builder::SemanticModelBuilder;
use crate::catalogs::CatalogsConfig;
use crate::data_kind::{AnyDataKindRef, ComplexDataKind, ComplexDataKindRef, DataKind};
use crate::entities::{DimensionEntry, MeasureEntry, MetricEntry};
use crate::error::catalogs::CatalogsParseErrorKind;
use crate::error::parse::ParseErrorKind;
use crate::yaml::env::{substitute_env_for_catalogs, substitute_env_for_model};
use crate::yaml::YamlRoot;
use semstrait_core::diagnostic::{Diagnostic, Diagnostics, Location};

const DEFAULT_SOURCE: &str = "<inline>";

/// YAML `&str` → [`SemanticModelBuilder`]. Pure, synchronous,
/// accumulating per `32 §9.1`. Callers chain
/// [`SemanticModelBuilder::build`] to materialise the canonical
/// [`crate::SemanticModel`] together with any validate / dup
/// diagnostics. Equivalent to `parse_with_source(input, "<inline>")`.
pub fn parse(
    input: &str,
) -> Result<SemanticModelBuilder, Diagnostics<ParseErrorKind>> {
    parse_with_source(input, DEFAULT_SOURCE)
}

/// As [`parse`], but the supplied `source` label is attached to every
/// diagnostic [`semstrait_core::Location`].
pub fn parse_with_source(
    input: &str,
    source: &str,
) -> Result<SemanticModelBuilder, Diagnostics<ParseErrorKind>> {
    let expanded = match substitute_env_for_model(input) {
        Ok(s) => s,
        Err(diag) => return Err(vec![diag]),
    };

    let root: YamlRoot = match serde_yaml::from_str(&expanded) {
        Ok(r) => r,
        Err(e) => {
            let kind = classify_yaml_error(&e);
            return Err(vec![Diagnostic::new(kind)]);
        }
    };

    let builder = root.lower(source);

    // SR-8 — identifier-grammar validation. The lowered builder still
    // carries every parsed entry; we walk its Vec storage rather than
    // the canonical BTreeMap form (the latter only materialises at
    // `.build()` time).
    let mut diagnostics: Diagnostics<ParseErrorKind> = Vec::new();
    check_identifiers(&builder, source, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(builder)
    } else {
        Err(diagnostics)
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
pub(crate) fn classify_yaml_error(e: &serde_yaml::Error) -> ParseErrorKind {
    let msg = e.to_string();

    // serde_yaml emits messages like
    //   "semantic_model.datasets[0]: missing field `name` at line N column M"
    //   "semantic_model: unknown field `xyz`, expected one of …"
    // The path-prefix means we cannot rely on `strip_prefix`; we use
    // `find` to locate the canonical `<kw> \`<field>\`` substring.
    if let Some(field) = extract_backticked(&msg, "unknown field `") {
        return ParseErrorKind::UnknownField {
            field,
            parent: "<unknown>".to_string(),
        };
    }

    // Required-field-missing classifier. The serde_yaml carriage doesn't
    // surface which struct was being deserialized, so the carrier/name
    // payload uses the only fact we know for sure (the field name).
    // SR-E-4 / SR-E-9 / SR-E-10 — fields uniquely identifying these
    // variants are `cardinality`, `agg`, `data_type`.
    if let Some(field) = extract_backticked(&msg, "missing field `") {
        match field.as_str() {
            "cardinality" => {
                return ParseErrorKind::RelationshipMissingCardinality {
                    relationship: "<unknown>".to_string(),
                };
            }
            "agg" => {
                return ParseErrorKind::MeasureMissingAgg {
                    carrier: "Measure".to_string(),
                    name: "<unknown>".to_string(),
                };
            }
            "data_type" => {
                return ParseErrorKind::SemanticsMissingDataType {
                    carrier: "<unknown>".to_string(),
                    name: "<unknown>".to_string(),
                };
            }
            _ => {}
        }
    }

    ParseErrorKind::YamlSyntax { message: msg }
}

/// Returns the backticked token immediately following `marker` if `msg`
/// contains it, otherwise `None`.
fn extract_backticked(msg: &str, marker: &str) -> Option<String> {
    let idx = msg.find(marker)?;
    let rest = &msg[idx + marker.len()..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
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

// ── Identifier grammar validation (SR-8) ────────────────────────────

/// Identifier grammar per `11 §13`: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_valid_identifier(raw: &str) -> bool {
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

const INVALID_IDENT_REASON: &str = "must match `[A-Za-z_][A-Za-z0-9_]*`";

fn push_invalid(
    raw: &str,
    source: &str,
    path: String,
    diagnostics: &mut Diagnostics<ParseErrorKind>,
) {
    if !is_valid_identifier(raw) {
        let loc = Location::new(source.to_string()).with_path(path);
        diagnostics.push(
            Diagnostic::new(ParseErrorKind::InvalidIdentifier {
                raw: raw.to_string(),
                reason: INVALID_IDENT_REASON.to_string(),
            })
            .at(loc),
        );
    }
}

/// Walk a [`SemanticModelBuilder`]'s Vec storage and push
/// [`ParseErrorKind::InvalidIdentifier`] for every non-conforming
/// name. Used after [`YamlRoot::lower`] inside [`parse_with_source`].
pub(crate) fn check_identifiers<S: semantic_model_builder::State>(
    builder: &SemanticModelBuilder<S>,
    source: &str,
    diagnostics: &mut Diagnostics<ParseErrorKind>,
) {
    for (_, d) in builder.datasets_view() {
        check_any_data_kind(AnyDataKindRef::Dataset(d), source, diagnostics);
    }
    for (_, g) in builder.grainsets_view() {
        check_any_data_kind(AnyDataKindRef::Grainset(g), source, diagnostics);
    }
    for (_, u) in builder.unionsets_view() {
        check_any_data_kind(AnyDataKindRef::Unionset(u), source, diagnostics);
    }
    for (_, j) in builder.joinsets_view() {
        check_any_data_kind(AnyDataKindRef::Joinset(j), source, diagnostics);
    }

    for (_, d) in builder.dimensions_view() {
        let name = d.name.as_str();
        push_invalid(name, source, format!("/dimensions/{}/name", name), diagnostics);
    }
    for (_, m) in builder.measures_view() {
        let name = m.name.as_str();
        push_invalid(name, source, format!("/measures/{}/name", name), diagnostics);
    }
    for (_, m) in builder.metrics_view() {
        let name = m.name.as_str();
        push_invalid(name, source, format!("/metrics/{}/name", name), diagnostics);
    }

    for r in builder.relationships_view() {
        push_invalid(
            &r.name,
            source,
            format!("/relationships/{}/name", r.name),
            diagnostics,
        );
    }
}

fn check_any_data_kind(
    any: AnyDataKindRef<'_>,
    source: &str,
    diagnostics: &mut Diagnostics<ParseErrorKind>,
) {
    match any {
        AnyDataKindRef::Dataset(d) => {
            let name = &d.body.base.name;
            push_invalid(name, source, format!("/datasets/{}/name", name), diagnostics);
            check_interface(&d.semantic_interface, source, &format!("/datasets/{}", name), diagnostics);
        }
        AnyDataKindRef::Grainset(g) => {
            let name = &g.body.base.name;
            push_invalid(name, source, format!("/grainsets/{}/name", name), diagnostics);
            check_interface(&g.semantic_interface, source, &format!("/grainsets/{}", name), diagnostics);
            check_complex_children(
                ComplexDataKindRef::Grainset(g),
                source,
                &format!("/grainsets/{}", name),
                diagnostics,
            );
        }
        AnyDataKindRef::Unionset(u) => {
            let name = &u.body.base.name;
            push_invalid(name, source, format!("/unionsets/{}/name", name), diagnostics);
            check_interface(&u.semantic_interface, source, &format!("/unionsets/{}", name), diagnostics);
            check_complex_children(
                ComplexDataKindRef::Unionset(u),
                source,
                &format!("/unionsets/{}", name),
                diagnostics,
            );
        }
        AnyDataKindRef::Joinset(j) => {
            let name = &j.body.base.name;
            push_invalid(name, source, format!("/joinsets/{}/name", name), diagnostics);
            check_interface(&j.semantic_interface, source, &format!("/joinsets/{}", name), diagnostics);
            check_complex_children(
                ComplexDataKindRef::Joinset(j),
                source,
                &format!("/joinsets/{}", name),
                diagnostics,
            );
            for r in &j.body.relationships {
                push_invalid(
                    &r.name,
                    source,
                    format!("/joinsets/{}/relationships/{}/name", name, r.name),
                    diagnostics,
                );
            }
        }
        _ => {}
    }
}

fn check_complex_children(
    parent: ComplexDataKindRef<'_>,
    source: &str,
    parent_path: &str,
    diagnostics: &mut Diagnostics<ParseErrorKind>,
) {
    for child in parent.children_ref() {
        let child_name = DataKind::name(&child);
        let child_path = format!("{}/children/{}", parent_path, child_name);
        push_invalid(child_name, source, format!("{}/name", child_path), diagnostics);

        // Recurse into nested complex variants. NestedDataset is the
        // only leaf and has no `semantic_interface` / children.
        match child {
            crate::data_kind::NestedDataKindRef::Grainset(ng) => {
                check_complex_children(
                    ComplexDataKindRef::NestedGrainset(ng),
                    source,
                    &child_path,
                    diagnostics,
                );
            }
            crate::data_kind::NestedDataKindRef::Unionset(nu) => {
                check_complex_children(
                    ComplexDataKindRef::NestedUnionset(nu),
                    source,
                    &child_path,
                    diagnostics,
                );
            }
            crate::data_kind::NestedDataKindRef::Joinset(nj) => {
                check_complex_children(
                    ComplexDataKindRef::NestedJoinset(nj),
                    source,
                    &child_path,
                    diagnostics,
                );
                for r in &nj.body.relationships {
                    push_invalid(
                        &r.name,
                        source,
                        format!("{}/relationships/{}/name", child_path, r.name),
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
}

fn check_interface(
    iface: &crate::entities::SemanticInterface,
    source: &str,
    parent_path: &str,
    diagnostics: &mut Diagnostics<ParseErrorKind>,
) {
    for entry in &iface.dimensions {
        if let DimensionEntry::Inline(d) = entry {
            let name = d.name.as_str();
            push_invalid(
                name,
                source,
                format!("{}/dimensions/{}/name", parent_path, name),
                diagnostics,
            );
        }
    }
    for entry in &iface.measures {
        if let MeasureEntry::Inline(m) = entry {
            let name = m.name.as_str();
            push_invalid(
                name,
                source,
                format!("{}/measures/{}/name", parent_path, name),
                diagnostics,
            );
        }
    }
    for entry in &iface.metrics {
        if let MetricEntry::Inline(m) = entry {
            let name = m.name.as_str();
            push_invalid(
                name,
                source,
                format!("{}/metrics/{}/name", parent_path, name),
                diagnostics,
            );
        }
    }
    for f in &iface.filters {
        let name = f.name.as_str();
        push_invalid(
            name,
            source,
            format!("{}/filters/{}/name", parent_path, name),
            diagnostics,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::build::ModelBuildErrorKind;
    use crate::error::validate::ValidateErrorKind;

    #[test]
    fn parse_minimal_model() {
        let yaml = r#"
semantic_model:
  name: tiny
"#;
        let builder = parse(yaml).expect("parse should succeed");
        assert_eq!(builder.name_view(), "tiny");
        assert!(builder.datasets_view().is_empty());
        assert!(builder.grainsets_view().is_empty());
        // Materialising via `.build()` fails with `EmptyModel` per
        // SR-* — keep the assertion as a regression check.
        let err = builder.build().unwrap_err();
        assert!(err.iter().any(|d| matches!(
            d.kind,
            ModelBuildErrorKind::Validate(ValidateErrorKind::EmptyModel)
        )));
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
        let builder = parse(yaml).expect("parse ok");
        let (m, _diags) = builder.build().expect("build ok");
        let ds = m.datasets.get("orders").expect("orders present");
        assert_eq!(ds.body.base.name, "orders");
        assert_eq!(ds.semantic_interface.dimensions.len(), 1);
    }

    #[test]
    fn parse_then_build_emits_duplicate_data_kind_name() {
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
        let builder = parse(yaml).expect("parse ok");
        let err = builder.build().unwrap_err();
        assert!(err.iter().any(|d| matches!(
            &d.kind,
            ModelBuildErrorKind::Validate(ValidateErrorKind::DuplicateDataKindName { .. })
        )));
    }

    #[test]
    fn parse_substitutes_env_var() {
        std::env::set_var("SEMSTRAIT_TEST_VAR", "ana");
        let yaml = r#"
semantic_model:
  name: ${SEMSTRAIT_TEST_VAR}
"#;
        let builder = parse(yaml).expect("parse ok");
        assert_eq!(builder.name_view(), "ana");
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
