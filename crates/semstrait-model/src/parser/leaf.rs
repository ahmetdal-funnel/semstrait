//! Leaf-set abstraction for the YAML authoring surface — sealed
//! [`LeafResolver`] trait plus the two canonical impls.
//!
//! Phase 8 Pass C extracts this concern out of the legacy
//! `expr_source/deserialize.rs` so the parser owns one error type
//! (`parser::error::ParseError`) and one entry point
//! (`parser::block::parse_block`).
//!
//! ## Roles
//!
//! - [`SemanticLeaf`] — bare identifiers resolve to `Field(name)`; `col`
//!   produces `SemanticLeaf::Column`; semantic tags (`field` / `dim` /
//!   `measure` / `metric` / `key`) produce the corresponding typed
//!   leaves.
//! - [`PhysicalLeaf`] — bare identifiers resolve to `Column(name)`;
//!   `col` produces `PhysicalLeaf::Column`; semantic tags are rejected
//!   via [`ParseError::TagNotAllowedAtSite`] with `site:
//!   "physical-mapping"`.
//!
//! Body parsers for the semantic-tag family (`dim` / `measure` /
//! `metric` / `key`) live here and route their accessor sub-grammar
//! through one helper per accessor kind.

use crate::parser::error::ParseError;
use semstrait_ir::{
    ColumnRef, DimensionAccessor, ExprLeaf, KeyAccessor, Literal, MeasureAccessor, MetricAccessor,
    PhysicalLeaf, SemanticLeaf, SemanticsName,
};
use serde_yaml::{Mapping, Value};

// ── Sealed leaf-set abstraction ─────────────────────────────────────────

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::SemanticLeaf {}
    impl Sealed for super::PhysicalLeaf {}
}

/// Per-leaf-set behaviour the YAML deserializer needs. Sealed inside
/// this module — the only impls are for the two canonical leaf sets
/// ([`SemanticLeaf`], [`PhysicalLeaf`]).
pub trait LeafResolver: ExprLeaf + sealed::Sealed {
    /// Bare-identifier resolution per `14 §6.5`. At semantic sites this
    /// produces `Field(name)`; at physical-mapping sites this produces
    /// `Column(name)`.
    fn resolve_bare_ident(name: String) -> Self;

    /// Build a leaf from a `col: name` tag. At physical-mapping sites
    /// this is the same shape as bare-identifier resolution; at
    /// semantic sites this disambiguates against name-collision under
    /// `semantic_mapping: auto` (`14 §6.5`).
    fn from_col_tag(name: String) -> Self;

    /// Build a leaf from a semantic-tag mapping (`field` / `dim` /
    /// `measure` / `metric` / `key`).
    ///
    /// Returns `Ok(Some(leaf))` when the tag is recognised and the
    /// body is well-formed; `Ok(None)` when the tag is not a semantic
    /// tag (caller falls through to structural / unknown handling);
    /// `Err(_)` when the tag is rejected at this leaf-set's parse
    /// site, or the body is malformed.
    fn from_semantic_tag(tag: &str, value: &Value) -> Result<Option<Self>, ParseError>;

    /// Wrap a typed [`Literal`] into this leaf set's `Literal` variant.
    fn from_literal(lit: Literal) -> Self;
}

impl LeafResolver for SemanticLeaf {
    fn resolve_bare_ident(name: String) -> Self {
        SemanticLeaf::Field(SemanticsName(name))
    }

    fn from_col_tag(name: String) -> Self {
        SemanticLeaf::Column(ColumnRef(name))
    }

    fn from_semantic_tag(tag: &str, value: &Value) -> Result<Option<Self>, ParseError> {
        match tag {
            "field" => {
                let name = value
                    .as_str()
                    .ok_or_else(|| ParseError::InvalidValue {
                        field: "field",
                        reason: "expected string name".into(),
                    })?
                    .to_owned();
                Ok(Some(SemanticLeaf::Field(SemanticsName(name))))
            }
            "dim" => {
                let (name, accessor) = parse_dim_body(value)?;
                Ok(Some(SemanticLeaf::Dimension { name, accessor }))
            }
            "measure" => {
                let (name, accessor) = parse_measure_body(value)?;
                Ok(Some(SemanticLeaf::Measure { name, accessor }))
            }
            "metric" => {
                let (name, accessor) = parse_metric_body(value)?;
                Ok(Some(SemanticLeaf::Metric { name, accessor }))
            }
            "key" => {
                let (name, accessor) = parse_key_body(value)?;
                Ok(Some(SemanticLeaf::Key { name, accessor }))
            }
            _ => Ok(None),
        }
    }

    fn from_literal(lit: Literal) -> Self {
        SemanticLeaf::Literal(lit)
    }
}

impl LeafResolver for PhysicalLeaf {
    fn resolve_bare_ident(name: String) -> Self {
        PhysicalLeaf::Column(ColumnRef(name))
    }

    fn from_col_tag(name: String) -> Self {
        PhysicalLeaf::Column(ColumnRef(name))
    }

    fn from_semantic_tag(tag: &str, _value: &Value) -> Result<Option<Self>, ParseError> {
        // PhysicalExpr cannot reference semantics — every semantic tag
        // is rejected at this site per `14 §6.4.1`'s site-legality
        // column.
        match tag {
            "field" | "dim" | "measure" | "metric" | "key" => {
                Err(ParseError::TagNotAllowedAtSite {
                    tag: tag.to_owned(),
                    site: "physical-mapping",
                })
            }
            _ => Ok(None),
        }
    }

    fn from_literal(lit: Literal) -> Self {
        PhysicalLeaf::Literal(lit)
    }
}

// ── Semantic-tag body parsers ───────────────────────────────────────────

fn parse_dim_body(value: &Value) -> Result<(SemanticsName, Option<DimensionAccessor>), ParseError> {
    parse_dim_or_key_body(value, "dim", parse_dimension_accessor)
}

fn parse_key_body(value: &Value) -> Result<(SemanticsName, Option<KeyAccessor>), ParseError> {
    parse_dim_or_key_body(value, "key", parse_key_accessor)
}

/// Helper for `dim` / `key` long forms — both share the
/// `DimensionAccessor`-shaped roster (`First` / `Last` / `Lag(n)` /
/// `Lead(n)`).
fn parse_dim_or_key_body<A>(
    value: &Value,
    tag: &'static str,
    parse_accessor: fn(&Value) -> Result<A, ParseError>,
) -> Result<(SemanticsName, Option<A>), ParseError> {
    if let Some(s) = value.as_str() {
        return Ok((SemanticsName(s.to_owned()), None));
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: tag,
        reason: "expected string name or mapping with `name` and optional `accessor`".into(),
    })?;
    let name = get_required_str(m, "name", tag)?.to_owned();
    let accessor = match get_optional_value(m, "accessor") {
        Some(v) => Some(parse_accessor(v)?),
        None => None,
    };
    // Reject unknown fields to keep the surface tight.
    deny_unknown_fields(m, tag, &["name", "accessor"])?;
    Ok((SemanticsName(name), accessor))
}

fn parse_measure_body(
    value: &Value,
) -> Result<(SemanticsName, Option<MeasureAccessor>), ParseError> {
    parse_measure_or_metric_body(value, "measure", parse_measure_accessor)
}

fn parse_metric_body(
    value: &Value,
) -> Result<(SemanticsName, Option<MetricAccessor>), ParseError> {
    parse_measure_or_metric_body(value, "metric", parse_metric_accessor)
}

fn parse_measure_or_metric_body<A>(
    value: &Value,
    tag: &'static str,
    parse_accessor: fn(&Value) -> Result<A, ParseError>,
) -> Result<(SemanticsName, Option<A>), ParseError> {
    if let Some(s) = value.as_str() {
        return Ok((SemanticsName(s.to_owned()), None));
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: tag,
        reason: "expected string name or mapping with `name` and optional `accessor`".into(),
    })?;
    let name = get_required_str(m, "name", tag)?.to_owned();
    let accessor = match get_optional_value(m, "accessor") {
        Some(v) => Some(parse_accessor(v)?),
        None => None,
    };
    deny_unknown_fields(m, tag, &["name", "accessor"])?;
    Ok((SemanticsName(name), accessor))
}

// ── Accessor parsers ────────────────────────────────────────────────────

fn parse_dimension_accessor(value: &Value) -> Result<DimensionAccessor, ParseError> {
    if let Some(s) = value.as_str() {
        return match s {
            "first" => Ok(DimensionAccessor::First),
            "last" => Ok(DimensionAccessor::Last),
            other => Err(ParseError::InvalidValue {
                field: "accessor",
                reason: format!("unknown DimensionAccessor variant `{other}`"),
            }),
        };
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "expected string variant or single-key tagged map".into(),
    })?;
    if m.len() != 1 {
        return Err(ParseError::AmbiguousTag(m.len()));
    }
    let (k, v) = m.iter().next().expect("len == 1");
    let tag = k.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "tag key must be a string".into(),
    })?;
    let n = parse_u32_body(v, tag)?;
    match tag {
        "lag" => Ok(DimensionAccessor::Lag(n)),
        "lead" => Ok(DimensionAccessor::Lead(n)),
        "first" => Ok(DimensionAccessor::First),
        "last" => Ok(DimensionAccessor::Last),
        other => Err(ParseError::InvalidValue {
            field: "accessor",
            reason: format!("unknown DimensionAccessor tag `{other}`"),
        }),
    }
}

fn parse_key_accessor(value: &Value) -> Result<KeyAccessor, ParseError> {
    if let Some(s) = value.as_str() {
        return match s {
            "first" => Ok(KeyAccessor::First),
            "last" => Ok(KeyAccessor::Last),
            other => Err(ParseError::InvalidValue {
                field: "accessor",
                reason: format!("unknown KeyAccessor variant `{other}`"),
            }),
        };
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "expected string variant or single-key tagged map".into(),
    })?;
    if m.len() != 1 {
        return Err(ParseError::AmbiguousTag(m.len()));
    }
    let (k, v) = m.iter().next().expect("len == 1");
    let tag = k.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "tag key must be a string".into(),
    })?;
    let n = parse_u32_body(v, tag)?;
    match tag {
        "lag" => Ok(KeyAccessor::Lag(n)),
        "lead" => Ok(KeyAccessor::Lead(n)),
        "first" => Ok(KeyAccessor::First),
        "last" => Ok(KeyAccessor::Last),
        other => Err(ParseError::InvalidValue {
            field: "accessor",
            reason: format!("unknown KeyAccessor tag `{other}`"),
        }),
    }
}

fn parse_measure_accessor(value: &Value) -> Result<MeasureAccessor, ParseError> {
    if let Some(s) = value.as_str() {
        return match s {
            "previous" => Ok(MeasureAccessor::Previous),
            "next" => Ok(MeasureAccessor::Next),
            "delta" => Ok(MeasureAccessor::Delta),
            "percent_change" => Ok(MeasureAccessor::PercentChange),
            other => Err(ParseError::InvalidValue {
                field: "accessor",
                reason: format!("unknown MeasureAccessor variant `{other}`"),
            }),
        };
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "expected string variant or single-key tagged map".into(),
    })?;
    if m.len() != 1 {
        return Err(ParseError::AmbiguousTag(m.len()));
    }
    let (k, v) = m.iter().next().expect("len == 1");
    let tag = k.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "tag key must be a string".into(),
    })?;
    let n = parse_u32_body(v, tag)?;
    match tag {
        "lag" => Ok(MeasureAccessor::Lag(n)),
        "lead" => Ok(MeasureAccessor::Lead(n)),
        "previous" => Ok(MeasureAccessor::Previous),
        "next" => Ok(MeasureAccessor::Next),
        "delta" => Ok(MeasureAccessor::Delta),
        "percent_change" => Ok(MeasureAccessor::PercentChange),
        other => Err(ParseError::InvalidValue {
            field: "accessor",
            reason: format!("unknown MeasureAccessor tag `{other}`"),
        }),
    }
}

fn parse_metric_accessor(value: &Value) -> Result<MetricAccessor, ParseError> {
    if let Some(s) = value.as_str() {
        return match s {
            "previous" => Ok(MetricAccessor::Previous),
            "next" => Ok(MetricAccessor::Next),
            "delta" => Ok(MetricAccessor::Delta),
            "percent_change" => Ok(MetricAccessor::PercentChange),
            other => Err(ParseError::InvalidValue {
                field: "accessor",
                reason: format!("unknown MetricAccessor variant `{other}`"),
            }),
        };
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "expected string variant or single-key tagged map".into(),
    })?;
    if m.len() != 1 {
        return Err(ParseError::AmbiguousTag(m.len()));
    }
    let (k, v) = m.iter().next().expect("len == 1");
    let tag = k.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: "tag key must be a string".into(),
    })?;
    let n = parse_u32_body(v, tag)?;
    match tag {
        "lag" => Ok(MetricAccessor::Lag(n)),
        "lead" => Ok(MetricAccessor::Lead(n)),
        "previous" => Ok(MetricAccessor::Previous),
        "next" => Ok(MetricAccessor::Next),
        "delta" => Ok(MetricAccessor::Delta),
        "percent_change" => Ok(MetricAccessor::PercentChange),
        other => Err(ParseError::InvalidValue {
            field: "accessor",
            reason: format!("unknown MetricAccessor tag `{other}`"),
        }),
    }
}

fn parse_u32_body(value: &Value, tag: &str) -> Result<u32, ParseError> {
    let n = value.as_u64().ok_or_else(|| ParseError::InvalidValue {
        field: "accessor",
        reason: format!("`{tag}` body must be a non-negative integer"),
    })?;
    if n > u32::MAX as u64 {
        return Err(ParseError::InvalidValue {
            field: "accessor",
            reason: format!("`{tag}` body {n} exceeds u32::MAX"),
        });
    }
    Ok(n as u32)
}

// ── Local YAML helpers (scoped to this module) ──────────────────────────

fn lookup<'m>(map: &'m Mapping, key: &str) -> Option<&'m Value> {
    map.get(Value::String(key.into()))
}

fn get_required_str<'m>(
    map: &'m Mapping,
    key: &str,
    field: &'static str,
) -> Result<&'m str, ParseError> {
    let v = lookup(map, key).ok_or(ParseError::MissingField(field))?;
    v.as_str().ok_or(ParseError::InvalidValue {
        field,
        reason: "expected string".into(),
    })
}

fn get_optional_value<'m>(map: &'m Mapping, key: &str) -> Option<&'m Value> {
    lookup(map, key)
}

fn deny_unknown_fields(
    map: &Mapping,
    parent: &'static str,
    allowed: &[&str],
) -> Result<(), ParseError> {
    for (k, _) in map.iter() {
        let key = k.as_str().ok_or_else(|| ParseError::InvalidValue {
            field: parent,
            reason: "non-string field key".into(),
        })?;
        if !allowed.contains(&key) {
            return Err(ParseError::InvalidValue {
                field: parent,
                reason: format!("unknown field `{key}`"),
            });
        }
    }
    Ok(())
}
