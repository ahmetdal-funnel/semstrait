//! Custom `Deserialize` for [`super::ExprSource<L>`] per spec
//! `[14 §6.4.1](../../../../docs/design/foundations/14_expressions.md)`
//! using the **Option γ reserved-tag catalog** (single-key map).
//!
//! YAML serde shape:
//!
//! - `String` value → [`ExprSource::Inline`]. The Inline DSL grammar is
//!   deferred per `14 §6.3`; `parse_semantic` / `parse_physical` raise
//!   [`ParseError::InlineDslNotImplemented`] when called on an Inline
//!   source.
//! - `Mapping` value → [`ExprSource::Block`]. The mapping is dispatched
//!   by single-key tag per `14 §6.4.1`.
//! - Bare scalar (`Number` / `Bool` / `Null`) at the top level → wrapped
//!   into `Block(Expr::Leaf(L::from_literal(...)))`.
//!
//! Inside a recursive `Expr<L>` walk, scalars also resolve as leaves
//! (string → bare-identifier per `14 §6.5`; number / bool / null →
//! literal).
//!
//! ## Leaf-set abstraction — sealed [`LeafResolver`]
//!
//! The deserializer is parametric in the leaf set via the private sealed
//! [`LeafResolver`] trait. Two impls live here:
//!
//! - `SemanticLeaf` — bare identifiers resolve to `Field(name)`; `col`
//!   produces `SemanticLeaf::Column`; semantic tags (`field` / `dim` /
//!   `measure` / `metric` / `key`) produce the corresponding typed
//!   leaves.
//! - `PhysicalLeaf` — bare identifiers resolve to `Column(name)`; `col`
//!   produces `PhysicalLeaf::Column`; semantic tags are rejected via
//!   `TagNotAllowedAtSite { site: "physical-mapping" }`.

use super::error::ParseError;
use super::ExprSource;
use semstrait_ir::{
    AggregationOp, BinaryOpKind, CanonicalFn, CastFailure, ColumnRef, DimensionAccessor, Expr,
    ExprLeaf, KeyAccessor, LikeKind, Literal, MeasureAccessor, MetricAccessor, PhysicalLeaf,
    SemanticLeaf, SemanticsName, Tree, UnaryOpKind,
};
use semstrait_core::DataType;
use serde::Deserialize;
use serde_yaml::{Mapping, Value};

// ── Sealed leaf-set abstraction ─────────────────────────────────────────

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::SemanticLeaf {}
    impl Sealed for super::PhysicalLeaf {}
}

/// Per-leaf-set behaviour the YAML deserializer needs. Sealed inside
/// this module — the only impls are for the two canonical leaf sets
/// (`SemanticLeaf`, `PhysicalLeaf`).
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

// ── Public Deserialize impl ─────────────────────────────────────────────

impl<'de, L> Deserialize<'de> for ExprSource<L>
where
    L: LeafResolver,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match parse_top_level::<L>(&value) {
            Ok(src) => Ok(src),
            Err(e) => Err(serde::de::Error::custom(e.to_string())),
        }
    }
}

// ── Top-level dispatch ──────────────────────────────────────────────────

fn parse_top_level<L: LeafResolver>(value: &Value) -> Result<ExprSource<L>, ParseError> {
    match value {
        // Bare string at the *top* of the parse is the Inline DSL form.
        Value::String(s) => Ok(ExprSource::Inline(s.clone())),

        // Bare scalars (number / bool / null) at the top level are wrapped
        // into a single-leaf `Block(Expr::Leaf(L::from_literal(...)))`.
        Value::Number(n) => {
            let lit = number_to_literal(n)?;
            Ok(ExprSource::Block(Expr::Leaf(L::from_literal(lit))))
        }
        Value::Bool(b) => Ok(ExprSource::Block(Expr::Leaf(L::from_literal(
            Literal::Boolean(*b),
        )))),
        Value::Null => Ok(ExprSource::Block(Expr::Leaf(L::from_literal(Literal::Null)))),

        // Mapping → block parse; recursive walk dispatches on the
        // single tag.
        Value::Mapping(_) => {
            let expr = parse_block_value::<L>(value)?;
            Ok(ExprSource::Block(expr))
        }

        // Sequences are not legal at the top level — every reserved
        // structural tag wraps its sub-list in a key (`coalesce: [...]`).
        other => Err(ParseError::UnexpectedShape(format!(
            "top-level expects string, scalar, or mapping; got {other:?}"
        ))),
    }
}

// ── Recursive descent — `parse_block_value` ─────────────────────────────

/// Recursively resolve a YAML node into an [`Expr<L>`] inside a Block
/// context. Bare strings resolve as identifiers (per `L::resolve_bare_ident`);
/// bare scalars resolve as literals; single-key mappings dispatch on
/// the reserved tag catalog (`14 §6.4.1`).
fn parse_block_value<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    match value {
        // String inside an Expr context resolves as a bare identifier
        // per `14 §6.5`.
        Value::String(s) => Ok(Expr::Leaf(L::resolve_bare_ident(s.clone()))),

        // Bare scalars are literals.
        Value::Number(n) => Ok(Expr::Leaf(L::from_literal(number_to_literal(n)?))),
        Value::Bool(b) => Ok(Expr::Leaf(L::from_literal(Literal::Boolean(*b)))),
        Value::Null => Ok(Expr::Leaf(L::from_literal(Literal::Null))),

        // Single-key mapping → tag dispatch.
        Value::Mapping(map) => parse_tagged_mapping::<L>(map),

        other => Err(ParseError::UnexpectedShape(format!(
            "expected scalar, mapping, or single-key tag inside an expression; got {other:?}"
        ))),
    }
}

fn parse_tagged_mapping<L: LeafResolver>(map: &Mapping) -> Result<Expr<L>, ParseError> {
    if map.len() != 1 {
        return Err(ParseError::AmbiguousTag(map.len()));
    }
    let (k, v) = map.iter().next().expect("len == 1");
    let tag = k.as_str().ok_or_else(|| ParseError::UnexpectedShape(
        "tag key must be a string".into(),
    ))?;

    // 1. Leaf tags first — `lit` / `col` / semantic family.
    if let Some(leaf) = parse_leaf_tag::<L>(tag, v)? {
        return Ok(Expr::Leaf(leaf));
    }

    // 2. Structural tags.
    match tag {
        "binary_op" => parse_binary_op::<L>(v),
        "unary_op" => parse_unary_op::<L>(v),
        "function_call" => parse_function_call::<L>(v),
        "cast" => parse_cast::<L>(v),
        "case" => parse_case::<L>(v),
        "in_list" => parse_in_list::<L>(v),
        "between" => parse_between::<L>(v),
        "like" => parse_like::<L>(v),
        "is_null" => parse_is_null::<L>(v),
        "coalesce" => parse_coalesce::<L>(v),
        "null_if" => parse_null_if::<L>(v),
        "aggregate" => parse_aggregate::<L>(v),

        // Window is compile-emitted only per `14 §3.3`.
        "window" => Err(ParseError::TagNotAllowedAtSite {
            tag: "window".into(),
            site: "expr",
        }),

        other => Err(ParseError::UnknownTag(other.to_owned())),
    }
}

// ── Leaf-tag handlers ───────────────────────────────────────────────────

fn parse_leaf_tag<L: LeafResolver>(tag: &str, value: &Value) -> Result<Option<L>, ParseError> {
    match tag {
        "lit" => {
            let lit = parse_literal_body(value)?;
            Ok(Some(L::from_literal(lit)))
        }
        "col" => {
            let name = value
                .as_str()
                .ok_or_else(|| ParseError::InvalidValue {
                    field: "col",
                    reason: "expected string name".into(),
                })?
                .to_owned();
            Ok(Some(L::from_col_tag(name)))
        }
        // Delegate semantic tags to the leaf-set's resolver. Unknown
        // tags fall through (Ok(None) ⇒ try structural).
        other => L::from_semantic_tag(other, value),
    }
}

/// Parse the body of a `lit:` tag.
///
/// Short forms:
/// - `lit: 42` → `Integer(42)`
/// - `lit: 1.5` → `Float(1.5)`
/// - `lit: "s"` → `String("s")`
/// - `lit: true` → `Boolean(true)`
/// - `lit: null` → `Null`
///
/// Long form: `lit: { value: ..., precision?: ..., scale?: ... }` for
/// the typed-literal carriers (Decimal, Time, Timestamp). v1 supports
/// the short forms exhaustively; long-form decimal / time / timestamp
/// are accepted but optional fields default per spec.
fn parse_literal_body(value: &Value) -> Result<Literal, ParseError> {
    match value {
        Value::Bool(b) => Ok(Literal::Boolean(*b)),
        Value::Number(n) => number_to_literal(n),
        Value::String(s) => Ok(Literal::String(s.clone())),
        Value::Null => Ok(Literal::Null),
        Value::Mapping(m) => parse_literal_long_form(m),
        Value::Sequence(_) => Ok(Literal::Null), // unreachable in practice
        Value::Tagged(t) => parse_literal_body(&t.value),
    }
}

/// Long-form literal: a mapping carrying a `value:` field plus optional
/// `precision:` / `scale:` for Decimal / Time / Timestamp. The mapping
/// shape is:
///
/// ```yaml
/// lit:
///   decimal: { value: "1.23", precision: 4, scale: 2 }
/// ```
///
/// or the plain `{ value: "..." }` form for String / Date / Interval.
/// v1 keeps the Decimal / Time / Timestamp shapes flexible — a
/// `precision` / `scale` absence picks v1 defaults (precision 0, scale
/// 0); downstream stages refine.
fn parse_literal_long_form(m: &Mapping) -> Result<Literal, ParseError> {
    // Single-key tagged form — `{ decimal: {...} }` etc. Let the
    // standard single-key dispatch identify the typed literal carrier.
    if m.len() == 1 {
        let (k, v) = m.iter().next().expect("len == 1");
        let tag = k.as_str().ok_or_else(|| ParseError::InvalidValue {
            field: "lit",
            reason: "tag key must be a string".into(),
        })?;
        return parse_typed_literal_tag(tag, v);
    }
    Err(ParseError::InvalidValue {
        field: "lit",
        reason: "expected scalar value or single-key typed-literal map".into(),
    })
}

fn parse_typed_literal_tag(tag: &str, body: &Value) -> Result<Literal, ParseError> {
    match tag {
        "boolean" => body
            .as_bool()
            .map(Literal::Boolean)
            .ok_or_else(|| ParseError::InvalidValue {
                field: "lit.boolean",
                reason: "expected boolean".into(),
            }),
        "integer" => body
            .as_i64()
            .map(Literal::Integer)
            .ok_or_else(|| ParseError::InvalidValue {
                field: "lit.integer",
                reason: "expected integer".into(),
            }),
        "float" => body
            .as_f64()
            .map(Literal::Float)
            .ok_or_else(|| ParseError::InvalidValue {
                field: "lit.float",
                reason: "expected float".into(),
            }),
        "string" => body
            .as_str()
            .map(|s| Literal::String(s.to_owned()))
            .ok_or_else(|| ParseError::InvalidValue {
                field: "lit.string",
                reason: "expected string".into(),
            }),
        "null" => Ok(Literal::Null),
        "decimal" => parse_decimal_body(body),
        "date" => body
            .as_str()
            .map(|s| Literal::Date(s.to_owned()))
            .ok_or_else(|| ParseError::InvalidValue {
                field: "lit.date",
                reason: "expected ISO-8601 date string".into(),
            }),
        "time" => parse_time_body(body),
        "timestamp" => parse_timestamp_body(body),
        "interval" => body
            .as_str()
            .map(|s| Literal::Interval(s.to_owned()))
            .ok_or_else(|| ParseError::InvalidValue {
                field: "lit.interval",
                reason: "expected ISO-8601 interval string".into(),
            }),
        "binary" => parse_binary_body(body),
        other => Err(ParseError::UnknownTag(format!("lit.{other}"))),
    }
}

fn parse_decimal_body(body: &Value) -> Result<Literal, ParseError> {
    let m = body.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "lit.decimal",
        reason: "expected mapping with `value`, `precision`, `scale`".into(),
    })?;
    let value = get_required_str(m, "value", "lit.decimal.value")?.to_owned();
    let precision = get_optional_u8(m, "precision")?.unwrap_or(0);
    let scale = get_optional_i8(m, "scale")?.unwrap_or(0);
    Ok(Literal::Decimal { value, precision, scale })
}

fn parse_time_body(body: &Value) -> Result<Literal, ParseError> {
    if let Some(s) = body.as_str() {
        return Ok(Literal::Time {
            value: s.to_owned(),
            precision: 0,
        });
    }
    let m = body.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "lit.time",
        reason: "expected string or mapping with `value`, `precision`".into(),
    })?;
    let value = get_required_str(m, "value", "lit.time.value")?.to_owned();
    let precision = get_optional_u8(m, "precision")?.unwrap_or(0);
    Ok(Literal::Time { value, precision })
}

fn parse_timestamp_body(body: &Value) -> Result<Literal, ParseError> {
    if let Some(s) = body.as_str() {
        return Ok(Literal::Timestamp {
            value: s.to_owned(),
            precision: 0,
        });
    }
    let m = body.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "lit.timestamp",
        reason: "expected string or mapping with `value`, `precision`".into(),
    })?;
    let value = get_required_str(m, "value", "lit.timestamp.value")?.to_owned();
    let precision = get_optional_u8(m, "precision")?.unwrap_or(0);
    Ok(Literal::Timestamp { value, precision })
}

fn parse_binary_body(body: &Value) -> Result<Literal, ParseError> {
    let seq = body.as_sequence().ok_or_else(|| ParseError::InvalidValue {
        field: "lit.binary",
        reason: "expected sequence of bytes (0..=255)".into(),
    })?;
    let mut bytes = Vec::with_capacity(seq.len());
    for v in seq {
        let n = v.as_u64().ok_or_else(|| ParseError::InvalidValue {
            field: "lit.binary",
            reason: "expected byte (u8) values".into(),
        })?;
        if n > u8::MAX as u64 {
            return Err(ParseError::InvalidValue {
                field: "lit.binary",
                reason: format!("byte {n} out of range 0..=255"),
            });
        }
        bytes.push(n as u8);
    }
    Ok(Literal::Binary(bytes))
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

// ── Structural-tag parsers ──────────────────────────────────────────────

fn parse_binary_op<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "binary_op",
        reason: "expected mapping with `op`, `left`, `right`".into(),
    })?;
    let op_str = get_required_str(m, "op", "binary_op.op")?;
    let op = parse_binary_op_kind(op_str)?;
    let left = parse_block_value::<L>(get_required_value(m, "left", "binary_op.left")?)?;
    let right = parse_block_value::<L>(get_required_value(m, "right", "binary_op.right")?)?;
    deny_unknown_fields(m, "binary_op", &["op", "left", "right"])?;
    rebuild_with_check(
        Expr::BinaryOp {
            op,
            left: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            right: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
        },
        vec![left, right],
    )
}

fn parse_binary_op_kind(s: &str) -> Result<BinaryOpKind, ParseError> {
    match s {
        "add" => Ok(BinaryOpKind::Add),
        "subtract" => Ok(BinaryOpKind::Subtract),
        "multiply" => Ok(BinaryOpKind::Multiply),
        "divide" => Ok(BinaryOpKind::Divide),
        "safe_divide" => Ok(BinaryOpKind::SafeDivide),
        "mod" => Ok(BinaryOpKind::Mod),
        "eq" => Ok(BinaryOpKind::Eq),
        "not_eq" => Ok(BinaryOpKind::NotEq),
        "lt" => Ok(BinaryOpKind::Lt),
        "lt_eq" => Ok(BinaryOpKind::LtEq),
        "gt" => Ok(BinaryOpKind::Gt),
        "gt_eq" => Ok(BinaryOpKind::GtEq),
        "and" => Ok(BinaryOpKind::And),
        "or" => Ok(BinaryOpKind::Or),
        other => Err(ParseError::InvalidValue {
            field: "op",
            reason: format!("unknown BinaryOpKind `{other}`"),
        }),
    }
}

fn parse_unary_op<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "unary_op",
        reason: "expected mapping with `op`, `operand`".into(),
    })?;
    let op_str = get_required_str(m, "op", "unary_op.op")?;
    let op = match op_str {
        "negate" => UnaryOpKind::Negate,
        "not" => UnaryOpKind::Not,
        other => {
            return Err(ParseError::InvalidValue {
                field: "op",
                reason: format!("unknown UnaryOpKind `{other}`"),
            });
        }
    };
    let operand = parse_block_value::<L>(get_required_value(m, "operand", "unary_op.operand")?)?;
    deny_unknown_fields(m, "unary_op", &["op", "operand"])?;
    rebuild_with_check(
        Expr::UnaryOp {
            op,
            operand: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
        },
        vec![operand],
    )
}

fn parse_function_call<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "function_call",
        reason: "expected mapping with `name`, `args`".into(),
    })?;
    let name = get_required_str(m, "name", "function_call.name")?
        .to_uppercase();
    let args_node = get_required_value(m, "args", "function_call.args")?;
    let args = parse_seq::<L>(args_node, "function_call.args")?;
    deny_unknown_fields(m, "function_call", &["name", "args"])?;
    let arg_count = args.len();
    rebuild_with_check(
        Expr::FunctionCall {
            name: CanonicalFn(name),
            args: vec![Expr::Leaf(L::from_literal(Literal::Null)); arg_count],
        },
        args,
    )
}

fn parse_cast<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "cast",
        reason: "expected mapping with `input`, `target`, `on_failure`".into(),
    })?;
    let input = parse_block_value::<L>(get_required_value(m, "input", "cast.input")?)?;
    let target_node = get_required_value(m, "target", "cast.target")?;
    let target = parse_data_type(target_node)?;
    let on_failure = match get_optional_str(m, "on_failure")? {
        Some(s) => parse_cast_failure(s)?,
        None => CastFailure::Error,
    };
    deny_unknown_fields(m, "cast", &["input", "target", "on_failure"])?;
    rebuild_with_check(
        Expr::Cast {
            input: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            target,
            on_failure,
        },
        vec![input],
    )
}

fn parse_cast_failure(s: &str) -> Result<CastFailure, ParseError> {
    match s {
        "error" => Ok(CastFailure::Error),
        "null" => Ok(CastFailure::Null),
        other => Err(ParseError::InvalidValue {
            field: "on_failure",
            reason: format!("unknown CastFailure `{other}`"),
        }),
    }
}

/// Parse a `target:` field on a `cast` block. Accepts the canonical
/// snake_case spellings of the body-less [`DataType`] variants:
/// `boolean`, `integer`, `number`, `string`, `date`, `binary`. Body-
/// bearing variants (`decimal`, `timestamp`) accept the standard
/// single-key tagged form. The roster mirrors `[13](../../../../docs/design/foundations/13_types_and_grain.md)`'s
/// 8-variant `DataType` exactly.
fn parse_data_type(value: &Value) -> Result<DataType, ParseError> {
    if let Some(s) = value.as_str() {
        return match s {
            "boolean" => Ok(DataType::Boolean),
            "integer" => Ok(DataType::Integer),
            "number" => Ok(DataType::Number),
            "string" => Ok(DataType::String),
            "date" => Ok(DataType::Date),
            "binary" => Ok(DataType::Binary),
            other => Err(ParseError::InvalidValue {
                field: "target",
                reason: format!("unknown DataType `{other}`"),
            }),
        };
    }
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "target",
        reason: "expected snake_case string or single-key tagged map".into(),
    })?;
    if m.len() != 1 {
        return Err(ParseError::AmbiguousTag(m.len()));
    }
    let (k, v) = m.iter().next().expect("len == 1");
    let tag = k.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "target",
        reason: "tag key must be a string".into(),
    })?;
    match tag {
        "decimal" => {
            let body = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
                field: "target.decimal",
                reason: "expected mapping with `precision`, `scale`".into(),
            })?;
            let precision = get_required_u8(body, "precision", "target.decimal.precision")?;
            let scale = get_required_i8(body, "scale", "target.decimal.scale")?;
            Ok(DataType::Decimal { precision, scale })
        }
        "timestamp" => {
            let body = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
                field: "target.timestamp",
                reason: "expected mapping with `precision`".into(),
            })?;
            let precision = get_required_u8(body, "precision", "target.timestamp.precision")?;
            Ok(DataType::Timestamp { precision })
        }
        other => Err(ParseError::InvalidValue {
            field: "target",
            reason: format!("unknown DataType tag `{other}`"),
        }),
    }
}

fn parse_case<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "case",
        reason: "expected mapping with `whens`, optional `else`".into(),
    })?;
    let whens_node = get_required_value(m, "whens", "case.whens")?;
    let whens_seq = whens_node.as_sequence().ok_or_else(|| ParseError::InvalidValue {
        field: "case.whens",
        reason: "expected sequence of `{ when, then }` pairs".into(),
    })?;
    let mut whens: Vec<(Expr<L>, Expr<L>)> = Vec::with_capacity(whens_seq.len());
    for entry in whens_seq {
        let pair = entry.as_mapping().ok_or_else(|| ParseError::InvalidValue {
            field: "case.whens[]",
            reason: "expected `{ when, then }` mapping".into(),
        })?;
        let when_node = get_required_value(pair, "when", "case.whens[].when")?;
        let then_node = get_required_value(pair, "then", "case.whens[].then")?;
        let when = parse_block_value::<L>(when_node)?;
        let then = parse_block_value::<L>(then_node)?;
        deny_unknown_fields(pair, "case.whens[]", &["when", "then"])?;
        whens.push((when, then));
    }
    let else_ = match get_optional_value(m, "else") {
        Some(v) => Some(Box::new(parse_block_value::<L>(v)?)),
        None => None,
    };
    deny_unknown_fields(m, "case", &["whens", "else"])?;

    // Build directly without going through with_new_children — the
    // construction-boundary checks for emptiness fire when we then
    // fold the rebuild check via Tree::with_new_children below.
    let candidate = Expr::Case {
        whens: whens.clone(),
        else_,
    };
    // Re-run the structural well-formedness rules by issuing an
    // identity rebuild — gives us EmptyCase enforcement.
    let kids: Vec<Expr<L>> = candidate.children().into_iter().cloned().collect();
    candidate.with_new_children(kids).map_err(ParseError::Ir)
}

fn parse_in_list<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "in_list",
        reason: "expected mapping with `value`, `list`, optional `negated`".into(),
    })?;
    let value_node = get_required_value(m, "value", "in_list.value")?;
    let list_node = get_required_value(m, "list", "in_list.list")?;
    let value_expr = parse_block_value::<L>(value_node)?;
    let list = parse_seq::<L>(list_node, "in_list.list")?;
    let negated = get_optional_bool(m, "negated")?.unwrap_or(false);
    deny_unknown_fields(m, "in_list", &["value", "list", "negated"])?;
    let candidate = Expr::InList {
        value: Box::new(value_expr),
        list,
        negated,
    };
    let kids: Vec<Expr<L>> = candidate.children().into_iter().cloned().collect();
    candidate.with_new_children(kids).map_err(ParseError::Ir)
}

fn parse_between<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "between",
        reason: "expected mapping with `value`, `low`, `high`, optional `negated`".into(),
    })?;
    let v_expr = parse_block_value::<L>(get_required_value(m, "value", "between.value")?)?;
    let low = parse_block_value::<L>(get_required_value(m, "low", "between.low")?)?;
    let high = parse_block_value::<L>(get_required_value(m, "high", "between.high")?)?;
    let negated = get_optional_bool(m, "negated")?.unwrap_or(false);
    deny_unknown_fields(m, "between", &["value", "low", "high", "negated"])?;
    rebuild_with_check(
        Expr::Between {
            value: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            low: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            high: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            negated,
        },
        vec![v_expr, low, high],
    )
}

fn parse_like<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "like",
        reason: "expected mapping with `value`, `pattern`, `kind`".into(),
    })?;
    let v_expr = parse_block_value::<L>(get_required_value(m, "value", "like.value")?)?;
    let pattern = parse_block_value::<L>(get_required_value(m, "pattern", "like.pattern")?)?;
    let kind_str = get_required_str(m, "kind", "like.kind")?;
    let kind = match kind_str {
        "like" => LikeKind::Like,
        "not_like" => LikeKind::NotLike,
        "i_like" => LikeKind::ILike,
        "not_i_like" => LikeKind::NotILike,
        other => {
            return Err(ParseError::InvalidValue {
                field: "kind",
                reason: format!("unknown LikeKind `{other}`"),
            });
        }
    };
    deny_unknown_fields(m, "like", &["value", "pattern", "kind"])?;
    rebuild_with_check(
        Expr::Like {
            value: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            pattern: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            kind,
        },
        vec![v_expr, pattern],
    )
}

fn parse_is_null<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    // `is_null:` takes its operand directly (no field wrapper) per
    // `14 §6.4.1`.
    let inner = parse_block_value::<L>(value)?;
    rebuild_with_check(
        Expr::IsNull(Box::new(Expr::Leaf(L::from_literal(Literal::Null)))),
        vec![inner],
    )
}

fn parse_coalesce<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let args = parse_seq::<L>(value, "coalesce")?;
    let candidate = Expr::Coalesce(args);
    let kids: Vec<Expr<L>> = candidate.children().into_iter().cloned().collect();
    candidate.with_new_children(kids).map_err(ParseError::Ir)
}

fn parse_null_if<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "null_if",
        reason: "expected mapping with `left`, `right`".into(),
    })?;
    let left = parse_block_value::<L>(get_required_value(m, "left", "null_if.left")?)?;
    let right = parse_block_value::<L>(get_required_value(m, "right", "null_if.right")?)?;
    deny_unknown_fields(m, "null_if", &["left", "right"])?;
    rebuild_with_check(
        Expr::NullIf {
            left: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            right: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
        },
        vec![left, right],
    )
}

fn parse_aggregate<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "aggregate",
        reason: "expected mapping with `op`, `args`, optional `distinct`, `filter`".into(),
    })?;
    let op_str = get_required_str(m, "op", "aggregate.op")?;
    let op = match op_str {
        "sum" => AggregationOp::Sum,
        "avg" => AggregationOp::Avg,
        "count" => AggregationOp::Count,
        "min" => AggregationOp::Min,
        "max" => AggregationOp::Max,
        other => {
            return Err(ParseError::InvalidValue {
                field: "op",
                reason: format!("unknown AggregationOp `{other}`"),
            });
        }
    };
    let args_node = get_required_value(m, "args", "aggregate.args")?;
    let args = parse_seq::<L>(args_node, "aggregate.args")?;
    let distinct = get_optional_bool(m, "distinct")?.unwrap_or(false);
    let filter = match get_optional_value(m, "filter") {
        Some(Value::Null) | None => None,
        Some(v) => Some(Box::new(parse_block_value::<L>(v)?)),
    };
    deny_unknown_fields(m, "aggregate", &["op", "args", "distinct", "filter"])?;

    let candidate = Expr::Aggregate {
        op,
        args,
        distinct,
        filter,
    };
    let kids: Vec<Expr<L>> = candidate.children().into_iter().cloned().collect();
    candidate.with_new_children(kids).map_err(ParseError::Ir)
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn parse_seq<L: LeafResolver>(value: &Value, field: &'static str) -> Result<Vec<Expr<L>>, ParseError> {
    let seq = value.as_sequence().ok_or_else(|| ParseError::InvalidValue {
        field,
        reason: "expected sequence".into(),
    })?;
    seq.iter().map(parse_block_value::<L>).collect()
}

fn rebuild_with_check<L: LeafResolver>(
    template: Expr<L>,
    kids: Vec<Expr<L>>,
) -> Result<Expr<L>, ParseError> {
    template.with_new_children(kids).map_err(ParseError::Ir)
}

fn number_to_literal(n: &serde_yaml::Number) -> Result<Literal, ParseError> {
    if let Some(i) = n.as_i64() {
        return Ok(Literal::Integer(i));
    }
    if let Some(f) = n.as_f64() {
        return Ok(Literal::Float(f));
    }
    Err(ParseError::InvalidValue {
        field: "number",
        reason: "yaml number neither i64 nor f64".into(),
    })
}

fn lookup<'m>(map: &'m Mapping, key: &str) -> Option<&'m Value> {
    map.get(Value::String(key.into()))
}

fn get_required_value<'m>(
    map: &'m Mapping,
    key: &'static str,
    field: &'static str,
) -> Result<&'m Value, ParseError> {
    lookup(map, key).ok_or(ParseError::MissingField(field))
}

fn get_optional_value<'m>(map: &'m Mapping, key: &str) -> Option<&'m Value> {
    lookup(map, key)
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

fn get_optional_str<'m>(map: &'m Mapping, key: &str) -> Result<Option<&'m str>, ParseError> {
    match lookup(map, key) {
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| ParseError::InvalidValue {
                field: "value",
                reason: format!("expected string for `{key}`"),
            }),
        None => Ok(None),
    }
}

fn get_optional_bool(map: &Mapping, key: &str) -> Result<Option<bool>, ParseError> {
    match lookup(map, key) {
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| ParseError::InvalidValue {
                field: "value",
                reason: format!("expected bool for `{key}`"),
            }),
        None => Ok(None),
    }
}

fn get_optional_u8(map: &Mapping, key: &str) -> Result<Option<u8>, ParseError> {
    match lookup(map, key) {
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| ParseError::InvalidValue {
                field: "value",
                reason: format!("expected non-negative integer for `{key}`"),
            })?;
            if n > u8::MAX as u64 {
                return Err(ParseError::InvalidValue {
                    field: "value",
                    reason: format!("`{key}` value {n} exceeds u8::MAX"),
                });
            }
            Ok(Some(n as u8))
        }
        None => Ok(None),
    }
}

fn get_required_u8(
    map: &Mapping,
    key: &str,
    field: &'static str,
) -> Result<u8, ParseError> {
    get_optional_u8(map, key)?.ok_or(ParseError::MissingField(field))
}

fn get_optional_i8(map: &Mapping, key: &str) -> Result<Option<i8>, ParseError> {
    match lookup(map, key) {
        Some(v) => {
            let n = v.as_i64().ok_or_else(|| ParseError::InvalidValue {
                field: "value",
                reason: format!("expected integer for `{key}`"),
            })?;
            if !(i8::MIN as i64..=i8::MAX as i64).contains(&n) {
                return Err(ParseError::InvalidValue {
                    field: "value",
                    reason: format!("`{key}` value {n} out of i8 range"),
                });
            }
            Ok(Some(n as i8))
        }
        None => Ok(None),
    }
}

fn get_required_i8(
    map: &Mapping,
    key: &str,
    field: &'static str,
) -> Result<i8, ParseError> {
    get_optional_i8(map, key)?.ok_or(ParseError::MissingField(field))
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

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_ir::{
        BinaryOpKind, ColumnRef, DimensionAccessor, Expr, Literal, MeasureAccessor, PhysicalLeaf,
        SemanticLeaf, SemanticsName,
    };

    type SemanticSrc = ExprSource<SemanticLeaf>;
    type PhysicalSrc = ExprSource<PhysicalLeaf>;

    fn parse_semantic_yaml(yaml: &str) -> SemanticSrc {
        serde_yaml::from_str(yaml).expect("valid semantic YAML")
    }

    fn parse_physical_yaml(yaml: &str) -> PhysicalSrc {
        serde_yaml::from_str(yaml).expect("valid physical YAML")
    }

    fn try_parse_semantic_yaml(yaml: &str) -> Result<SemanticSrc, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    fn try_parse_physical_yaml(yaml: &str) -> Result<PhysicalSrc, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    fn unwrap_block(src: SemanticSrc) -> Expr<SemanticLeaf> {
        match src {
            ExprSource::Block(e) => e,
            other => panic!("expected Block, got {other:?}"),
        }
    }

    fn unwrap_physical_block(src: PhysicalSrc) -> Expr<PhysicalLeaf> {
        match src {
            ExprSource::Block(e) => e,
            other => panic!("expected Block, got {other:?}"),
        }
    }

    // ── Inline (string at top level) ───────────────────────────────────

    #[test]
    fn top_level_string_is_inline() {
        // A bare string at the top level is the Inline DSL form.
        // `'revenue'` gives us the explicit-string form via YAML quoting.
        let src: SemanticSrc = serde_yaml::from_str("'revenue'").unwrap();
        match src {
            ExprSource::Inline(s) => assert_eq!(s, "revenue"),
            other => panic!("expected Inline, got {other:?}"),
        }
    }

    // ── Bare scalars at the top level ──────────────────────────────────

    #[test]
    fn bare_int_top_level_becomes_literal_block() {
        let src = parse_semantic_yaml("42");
        let e = unwrap_block(src);
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(42)))
        );
    }

    #[test]
    fn bare_float_top_level_becomes_literal_block() {
        let src = parse_semantic_yaml("1.5");
        let e = unwrap_block(src);
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Float(1.5))));
    }

    #[test]
    fn bare_bool_top_level_becomes_literal_block() {
        let src = parse_semantic_yaml("true");
        let e = unwrap_block(src);
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Literal(Literal::Boolean(true)))
        );
    }

    #[test]
    fn bare_null_top_level_becomes_literal_block() {
        let src = parse_semantic_yaml("null");
        let e = unwrap_block(src);
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Null)));
    }

    // ── Leaf tags — `lit` ──────────────────────────────────────────────

    #[test]
    fn lit_string_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ lit: hello }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Literal(Literal::String("hello".into())))
        );
    }

    #[test]
    fn lit_int_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ lit: 7 }"));
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(7))));
    }

    #[test]
    fn lit_bool_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ lit: true }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Literal(Literal::Boolean(true)))
        );
    }

    #[test]
    fn lit_null_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ lit: null }"));
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Null)));
    }

    #[test]
    fn lit_decimal_long_form() {
        let yaml = r#"
lit:
  decimal:
    value: "1.23"
    precision: 4
    scale: 2
"#;
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::Leaf(SemanticLeaf::Literal(Literal::Decimal {
                value,
                precision,
                scale,
            })) => {
                assert_eq!(value, "1.23");
                assert_eq!(precision, 4);
                assert_eq!(scale, 2);
            }
            other => panic!("expected Decimal, got {other:?}"),
        }
    }

    // ── Leaf tags — `col` ──────────────────────────────────────────────

    #[test]
    fn col_in_semantic_context_becomes_semantic_column() {
        let e = unwrap_block(parse_semantic_yaml("{ col: revenue }"));
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Column(ColumnRef("revenue".into()))));
    }

    #[test]
    fn col_in_physical_context_becomes_physical_column() {
        let e = unwrap_physical_block(parse_physical_yaml("{ col: revenue }"));
        assert_eq!(
            e,
            Expr::Leaf(PhysicalLeaf::Column(ColumnRef("revenue".into())))
        );
    }

    // ── Bare identifier resolution (semantic vs physical) ───────────────

    #[test]
    fn bare_string_inside_expr_is_field_at_semantic_site() {
        // The bare string sits *inside* a binary_op slot; that's an
        // expression context (not the top level), so it resolves as a
        // bare identifier rather than as Inline.
        let e = unwrap_block(parse_semantic_yaml(
            "{ binary_op: { op: add, left: revenue, right: cost } }",
        ));
        match e {
            Expr::BinaryOp { op, left, right } => {
                assert_eq!(op, BinaryOpKind::Add);
                assert_eq!(
                    *left,
                    Expr::Leaf(SemanticLeaf::Field(SemanticsName("revenue".into())))
                );
                assert_eq!(
                    *right,
                    Expr::Leaf(SemanticLeaf::Field(SemanticsName("cost".into())))
                );
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn bare_string_inside_expr_is_column_at_physical_site() {
        let e = unwrap_physical_block(parse_physical_yaml(
            "{ binary_op: { op: add, left: amount, right: tax } }",
        ));
        match e {
            Expr::BinaryOp { op, left, right } => {
                assert_eq!(op, BinaryOpKind::Add);
                assert_eq!(
                    *left,
                    Expr::Leaf(PhysicalLeaf::Column(ColumnRef("amount".into())))
                );
                assert_eq!(
                    *right,
                    Expr::Leaf(PhysicalLeaf::Column(ColumnRef("tax".into())))
                );
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    // ── Semantic tags — `field` / `dim` / `measure` / `metric` / `key` ─

    #[test]
    fn field_tag_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ field: revenue }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Field(SemanticsName("revenue".into())))
        );
    }

    #[test]
    fn dim_tag_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ dim: region }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Dimension {
                name: SemanticsName("region".into()),
                accessor: None,
            })
        );
    }

    #[test]
    fn dim_tag_with_string_accessor() {
        let yaml = "{ dim: { name: region, accessor: first } }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Dimension {
                name: SemanticsName("region".into()),
                accessor: Some(DimensionAccessor::First),
            })
        );
    }

    #[test]
    fn dim_tag_with_lag_accessor() {
        let yaml = "{ dim: { name: dt, accessor: { lag: 2 } } }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Dimension {
                name: SemanticsName("dt".into()),
                accessor: Some(DimensionAccessor::Lag(2)),
            })
        );
    }

    #[test]
    fn measure_tag_short_form() {
        let e = unwrap_block(parse_semantic_yaml("{ measure: revenue }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Measure {
                name: SemanticsName("revenue".into()),
                accessor: None,
            })
        );
    }

    #[test]
    fn measure_tag_with_previous_accessor() {
        let yaml = "{ measure: { name: revenue, accessor: previous } }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Measure {
                name: SemanticsName("revenue".into()),
                accessor: Some(MeasureAccessor::Previous),
            })
        );
    }

    #[test]
    fn metric_tag_long_form_with_accessor() {
        let yaml = "{ metric: { name: conv_rate, accessor: delta } }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::Leaf(SemanticLeaf::Metric { name, accessor }) => {
                assert_eq!(name.0, "conv_rate");
                assert!(accessor.is_some());
            }
            other => panic!("expected Metric, got {other:?}"),
        }
    }

    #[test]
    fn key_tag_long_form_with_accessor() {
        let yaml = "{ key: { name: order_id, accessor: { lead: 1 } } }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::Leaf(SemanticLeaf::Key { name, accessor }) => {
                assert_eq!(name.0, "order_id");
                assert!(accessor.is_some());
            }
            other => panic!("expected Key, got {other:?}"),
        }
    }

    // ── Cross-leaf-set rejection — semantic tags at physical sites ─────

    #[test]
    fn field_at_physical_site_is_rejected() {
        let err = try_parse_physical_yaml("{ field: revenue }").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("field") && msg.contains("physical-mapping"),
            "expected TagNotAllowedAtSite, got: {msg}"
        );
    }

    #[test]
    fn dim_at_physical_site_is_rejected() {
        let err = try_parse_physical_yaml("{ dim: x }").unwrap_err();
        assert!(err.to_string().contains("physical-mapping"));
    }

    #[test]
    fn measure_at_physical_site_is_rejected() {
        let err = try_parse_physical_yaml("{ measure: x }").unwrap_err();
        assert!(err.to_string().contains("physical-mapping"));
    }

    #[test]
    fn bare_string_inside_physical_expr_is_column_not_field() {
        // A bare string at the top-level of a physical site is Inline
        // (handled by the inline_at_physical_site test); inside a sub-
        // expression it resolves as Column.
        let e = unwrap_physical_block(parse_physical_yaml("{ unary_op: { op: not, operand: flag } }"));
        match e {
            Expr::UnaryOp { operand, .. } => {
                assert_eq!(
                    *operand,
                    Expr::Leaf(PhysicalLeaf::Column(ColumnRef("flag".into())))
                );
            }
            other => panic!("expected UnaryOp, got {other:?}"),
        }
    }

    // ── Structural tags ────────────────────────────────────────────────

    #[test]
    fn binary_op_add_with_measure_and_lit() {
        let yaml = r#"
binary_op:
  op: add
  left: { measure: revenue }
  right: { lit: 1 }
"#;
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::BinaryOp { op, left, right } => {
                assert_eq!(op, BinaryOpKind::Add);
                assert!(matches!(*left, Expr::Leaf(SemanticLeaf::Measure { .. })));
                assert_eq!(
                    *right,
                    Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(1)))
                );
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn aggregate_sum_default_distinct_and_filter() {
        let yaml = r#"
aggregate:
  op: sum
  args: [{ measure: revenue }]
"#;
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::Aggregate {
                op,
                args,
                distinct,
                filter,
            } => {
                use semstrait_ir::AggregationOp;
                assert_eq!(op, AggregationOp::Sum);
                assert_eq!(args.len(), 1);
                assert!(!distinct);
                assert!(filter.is_none());
            }
            other => panic!("expected Aggregate, got {other:?}"),
        }
    }

    #[test]
    fn case_with_else() {
        let yaml = r#"
case:
  whens:
    - when: { binary_op: { op: gt, left: revenue, right: { lit: 0 } } }
      then: { lit: 1 }
  else: { lit: 0 }
"#;
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::Case { whens, else_ } => {
                assert_eq!(whens.len(), 1);
                assert!(else_.is_some());
            }
            other => panic!("expected Case, got {other:?}"),
        }
    }

    #[test]
    fn coalesce_with_three_args() {
        let yaml = "{ coalesce: [a, b, { lit: 0 }] }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::Coalesce(args) => assert_eq!(args.len(), 3),
            other => panic!("expected Coalesce, got {other:?}"),
        }
    }

    #[test]
    fn nested_binary_op_with_field_resolution() {
        let yaml = "{ binary_op: { op: subtract, left: revenue, right: cost } }";
        let e = unwrap_block(parse_semantic_yaml(yaml));
        match e {
            Expr::BinaryOp { op, left, right } => {
                assert_eq!(op, BinaryOpKind::Subtract);
                // Both operands are bare strings -> Field at semantic site.
                assert!(matches!(*left, Expr::Leaf(SemanticLeaf::Field(_))));
                assert!(matches!(*right, Expr::Leaf(SemanticLeaf::Field(_))));
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    // ── Errors ─────────────────────────────────────────────────────────

    #[test]
    fn window_tag_is_rejected_at_expr_site() {
        let yaml = "{ window: { function: lag, args: [], partition_by: [], order_by: [] } }";
        let err = try_parse_semantic_yaml(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("window"));
    }

    #[test]
    fn unknown_tag_is_rejected() {
        let err = try_parse_semantic_yaml("{ unknown_tag: x }").unwrap_err();
        assert!(err.to_string().contains("unknown_tag"));
    }

    #[test]
    fn binary_op_with_unknown_op_is_rejected() {
        let yaml = "{ binary_op: { op: nonsense, left: a, right: b } }";
        let err = try_parse_semantic_yaml(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nonsense") || msg.contains("op"));
    }

    #[test]
    fn ambiguous_two_key_top_level_is_rejected() {
        let yaml = "{ lit: 1, col: x }";
        let err = try_parse_semantic_yaml(yaml).unwrap_err();
        let _ = err; // Either AmbiguousTag or downstream — we only need the rejection.
    }

    #[test]
    fn empty_coalesce_is_rejected_via_ir() {
        let yaml = "{ coalesce: [] }";
        let err = try_parse_semantic_yaml(yaml).unwrap_err();
        let msg = err.to_string();
        // Surface either "EmptyCoalesce" via Debug or any "ir validation"
        // wrapper text; we only assert that some failure happened.
        assert!(
            msg.contains("Empty") || msg.contains("ir validation") || msg.contains("Coalesce"),
            "unexpected error: {msg}"
        );
    }
}
