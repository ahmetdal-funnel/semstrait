//! Block-form expression parser per spec
//! `[14 §6.4.1](../../../../docs/design/foundations/14_expressions.md)`
//! — Phase 8.
//!
//! Thin recursive-descent over `serde_yaml::Value` that fans out
//! through three closed sugar-tag tables in [`super::sugar`] plus a
//! small set of structural-tag arms handled locally. The leaf-set
//! abstraction lives in [`super::leaf::LeafResolver`].
//!
//! ## Author surface
//!
//! Inside a [`crate::expr_source::ExprSource::Block`] body, every
//! mapping is a single-key tagged form. The dispatcher in
//! [`parse_tagged_mapping`] tries:
//!
//! 1. [`super::sugar::binary_op_for_tag`] — closed binary roster.
//! 2. [`super::sugar::unary_op_for_tag`] — closed unary roster.
//! 3. [`super::sugar::function_for_tag`] — closed function roster.
//! 4. Direct-handled special tags (`lit`, `col`, semantic family,
//!    `not`, `case`, `if`, `coalesce`, `null_if`, `in`, `between`,
//!    `like`, `ilike`, `is_null`, `cast`, aggregate sugar).
//! 5. Anything else → [`ParseError::UnknownTag`].
//!
//! ## Negation fold
//!
//! The `not:` arm recurses on its body and, if the result is an
//! [`Expr::InList`], [`Expr::Like`] or [`Expr::Between`], flips the
//! kind/negation in-place. Otherwise it wraps the inner in
//! [`Expr::UnaryOp`] with [`UnaryOpKind::Not`]. This is the only path
//! that emits [`LikeKind::NotLike`] / [`LikeKind::NotILike`].

use crate::expr_source::ExprSource;
use crate::parser::error::ParseError;
use crate::parser::leaf::LeafResolver;
use crate::parser::sugar;
use crate::parser::token::{tokenize_leaf, LeafToken};
use semstrait_core::DataType;
use semstrait_ir::{
    AggregationOp, CanonicalFn, CastFailure, Expr, LikeKind, Literal, Tree, UnaryOpKind,
};
use serde_yaml::{Mapping, Value};

// ── Top-level entry point ───────────────────────────────────────────────

/// Parse a `serde_yaml::Value` into an [`ExprSource<L>`] per spec
/// `14 §6.4.1`.
///
/// - `Value::String(s)` → [`ExprSource::Inline`] (Inline DSL lowering is
///   deferred per `14 §6.3`; the string body is preserved).
/// - Bare scalar (`Number` / `Bool` / `Null`) → [`ExprSource::Block`]
///   wrapping `Expr::Leaf(L::from_literal(...))`.
/// - `Value::Mapping(_)` → [`ExprSource::Block`] containing the
///   dispatched tagged form.
/// - Anything else → [`ParseError::UnexpectedShape`].
pub fn parse_block<L: LeafResolver>(value: &Value) -> Result<ExprSource<L>, ParseError> {
    match value {
        Value::String(s) => Ok(ExprSource::Inline(s.clone())),
        Value::Number(n) => Ok(ExprSource::Block(Expr::Leaf(L::from_literal(
            number_to_literal(n)?,
        )))),
        Value::Bool(b) => Ok(ExprSource::Block(Expr::Leaf(L::from_literal(
            Literal::Boolean(*b),
        )))),
        Value::Null => Ok(ExprSource::Block(Expr::Leaf(L::from_literal(Literal::Null)))),
        Value::Mapping(m) => Ok(ExprSource::Block(parse_tagged_mapping::<L>(m)?)),
        other => Err(ParseError::UnexpectedShape(format!(
            "top-level expects string, scalar, or mapping; got {other:?}"
        ))),
    }
}

// ── Recursive-descent: leaf scalar dispatch ─────────────────────────────

/// Resolve a YAML node into an [`Expr<L>`] inside an expression context.
/// Strings tokenize via [`tokenize_leaf`]; bare scalars become literals;
/// single-key mappings dispatch through [`parse_tagged_mapping`].
fn parse_block_value<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    match value {
        Value::String(s) => match tokenize_leaf(s)? {
            LeafToken::Boolean(b) => Ok(Expr::Leaf(L::from_literal(Literal::Boolean(b)))),
            LeafToken::Integer(i) => Ok(Expr::Leaf(L::from_literal(Literal::Integer(i)))),
            LeafToken::Float(f) => Ok(Expr::Leaf(L::from_literal(Literal::Float(f)))),
            LeafToken::Null => Ok(Expr::Leaf(L::from_literal(Literal::Null))),
            LeafToken::String(s) => Ok(Expr::Leaf(L::from_literal(Literal::String(s)))),
            LeafToken::Name(n) => Ok(Expr::Leaf(L::resolve_bare_ident(n))),
            LeafToken::NameWithAccessor { .. } => Err(ParseError::InvalidValue {
                field: "expression",
                reason: "accessor only valid in dim:/measure:/metric:/key: bodies".into(),
            }),
        },
        Value::Number(n) => Ok(Expr::Leaf(L::from_literal(number_to_literal(n)?))),
        Value::Bool(b) => Ok(Expr::Leaf(L::from_literal(Literal::Boolean(*b)))),
        Value::Null => Ok(Expr::Leaf(L::from_literal(Literal::Null))),
        Value::Mapping(m) => parse_tagged_mapping::<L>(m),
        other => Err(ParseError::UnexpectedShape(format!(
            "expected scalar, mapping, or single-key tag inside an expression; got {other:?}"
        ))),
    }
}

// ── Tag dispatch ─────────────────────────────────────────────────────────

fn parse_tagged_mapping<L: LeafResolver>(map: &Mapping) -> Result<Expr<L>, ParseError> {
    if map.len() != 1 {
        return Err(ParseError::AmbiguousTag(map.len()));
    }
    let (k, v) = map.iter().next().expect("len == 1");
    let tag = k
        .as_str()
        .ok_or_else(|| ParseError::UnexpectedShape("tag key must be a string".into()))?;

    // 1. Sugar binary operators.
    if let Some(op) = sugar::binary_op_for_tag(tag) {
        return parse_binary_op_sugar::<L>(op, v);
    }

    // 2. Sugar unary operators (`negate:`).
    if let Some(op) = sugar::unary_op_for_tag(tag) {
        return parse_unary_op_sugar::<L>(op, v);
    }

    // 3. Sugar function calls.
    if let Some(name) = sugar::function_for_tag(tag) {
        return parse_function_call_sugar::<L>(name, v);
    }

    // 4. Special tags.
    match tag {
        "lit" => {
            let lit = parse_literal_body(v)?;
            Ok(Expr::Leaf(L::from_literal(lit)))
        }
        "col" => {
            let name = v
                .as_str()
                .ok_or_else(|| ParseError::InvalidValue {
                    field: "col",
                    reason: "expected string name".into(),
                })?
                .to_owned();
            Ok(Expr::Leaf(L::from_col_tag(name)))
        }
        "field" | "dim" | "measure" | "metric" | "key" => {
            match L::from_semantic_tag(tag, v)? {
                Some(leaf) => Ok(Expr::Leaf(leaf)),
                None => Err(ParseError::TagNotAllowedAtSite {
                    tag: tag.to_owned(),
                    site: "physical-mapping",
                }),
            }
        }
        "not" => parse_not_fold::<L>(v),
        "case" => parse_case::<L>(v),
        "if" => parse_if::<L>(v),
        "coalesce" => parse_coalesce::<L>(v),
        "null_if" => parse_null_if::<L>(v),
        "in" => parse_in::<L>(v),
        "between" => parse_between::<L>(v, false),
        "like" => parse_like_kind::<L>(v, LikeKind::Like),
        "ilike" => parse_like_kind::<L>(v, LikeKind::ILike),
        "is_null" => parse_is_null::<L>(v),
        "cast" => parse_cast::<L>(v),
        // Aggregate sugar tags.
        "sum" => parse_aggregate_sugar::<L>(v, AggregationOp::Sum, false),
        "avg" => parse_aggregate_sugar::<L>(v, AggregationOp::Avg, false),
        "count" => parse_aggregate_sugar::<L>(v, AggregationOp::Count, false),
        "count_distinct" => parse_aggregate_sugar::<L>(v, AggregationOp::Count, true),
        "min" => parse_aggregate_sugar::<L>(v, AggregationOp::Min, false),
        "max" => parse_aggregate_sugar::<L>(v, AggregationOp::Max, false),

        // `window:` is compile-emitted only per `14 §3.3`.
        "window" => Err(ParseError::TagNotAllowedAtSite {
            tag: "window".into(),
            site: "expr",
        }),

        other => Err(ParseError::UnknownTag(other.to_owned())),
    }
}

// ── Sugar arm helpers ────────────────────────────────────────────────────

fn parse_binary_op_sugar<L: LeafResolver>(
    op: semstrait_ir::BinaryOpKind,
    body: &Value,
) -> Result<Expr<L>, ParseError> {
    let (left_node, right_node) = parse_pair_body(body, "binary_op")?;
    let left = parse_block_value::<L>(left_node)?;
    let right = parse_block_value::<L>(right_node)?;
    rebuild_with_check(
        Expr::BinaryOp {
            op,
            left: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            right: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
        },
        vec![left, right],
    )
}

fn parse_unary_op_sugar<L: LeafResolver>(
    op: UnaryOpKind,
    body: &Value,
) -> Result<Expr<L>, ParseError> {
    let operand = parse_block_value::<L>(body)?;
    rebuild_with_check(
        Expr::UnaryOp {
            op,
            operand: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
        },
        vec![operand],
    )
}

fn parse_function_call_sugar<L: LeafResolver>(
    name: CanonicalFn,
    body: &Value,
) -> Result<Expr<L>, ParseError> {
    // 1-arg shorthand: `upper: x` → `FunctionCall { name: UPPER, args: [x] }`.
    // n-arg form: `concat: [a, b, c]`.
    let args = if let Some(seq) = body.as_sequence() {
        seq.iter()
            .map(parse_block_value::<L>)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![parse_block_value::<L>(body)?]
    };
    let arg_count = args.len();
    rebuild_with_check(
        Expr::FunctionCall {
            name,
            args: vec![Expr::Leaf(L::from_literal(Literal::Null)); arg_count],
        },
        args,
    )
}

/// Parse `[a, b]` *or* `{ left, right }` body shapes for binary sugar.
fn parse_pair_body<'v>(
    body: &'v Value,
    tag: &'static str,
) -> Result<(&'v Value, &'v Value), ParseError> {
    if let Some(seq) = body.as_sequence() {
        if seq.len() != 2 {
            return Err(ParseError::InvalidValue {
                field: tag,
                reason: format!("expected 2-element sequence, got {}", seq.len()),
            });
        }
        return Ok((&seq[0], &seq[1]));
    }
    let m = body.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: tag,
        reason: "expected `[left, right]` sequence or `{ left, right }` mapping".into(),
    })?;
    let left = get_required_value(m, "left", "left")?;
    let right = get_required_value(m, "right", "right")?;
    deny_unknown_fields(m, tag, &["left", "right"])?;
    Ok((left, right))
}

// ── `not:` negation fold ─────────────────────────────────────────────────

fn parse_not_fold<L: LeafResolver>(body: &Value) -> Result<Expr<L>, ParseError> {
    let inner = parse_block_value::<L>(body)?;
    match inner {
        Expr::InList {
            value,
            list,
            negated,
        } => {
            let candidate = Expr::InList {
                value,
                list,
                negated: !negated,
            };
            // Re-run structural well-formedness via identity rebuild.
            identity_rebuild(candidate)
        }
        Expr::Like {
            value,
            pattern,
            kind,
        } => Ok(Expr::Like {
            value,
            pattern,
            kind: flip_like_kind(kind),
        }),
        Expr::Between {
            value,
            low,
            high,
            negated,
        } => Ok(Expr::Between {
            value,
            low,
            high,
            negated: !negated,
        }),
        other => rebuild_with_check(
            Expr::UnaryOp {
                op: UnaryOpKind::Not,
                operand: Box::new(Expr::Leaf(L::from_literal(Literal::Null))),
            },
            vec![other],
        ),
    }
}

fn flip_like_kind(kind: LikeKind) -> LikeKind {
    // `LikeKind` is `#[non_exhaustive]`; the four current variants are the
    // entire roster per `14 §3.3`. New variants added later require this
    // arm to be revisited.
    match kind {
        LikeKind::Like => LikeKind::NotLike,
        LikeKind::NotLike => LikeKind::Like,
        LikeKind::ILike => LikeKind::NotILike,
        LikeKind::NotILike => LikeKind::ILike,
        _ => kind,
    }
}

// ── Special structural tags ─────────────────────────────────────────────

fn parse_case<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "case",
        reason: "expected mapping with `whens`, optional `else`".into(),
    })?;
    let whens_node = get_required_value(m, "whens", "case.whens")?;
    let whens_seq = whens_node
        .as_sequence()
        .ok_or_else(|| ParseError::InvalidValue {
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
    identity_rebuild(Expr::Case { whens, else_ })
}

/// `if: { cond, then, else? }` — sugar for a single-when [`Expr::Case`].
fn parse_if<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "if",
        reason: "expected mapping with `cond`, `then`, optional `else`".into(),
    })?;
    let cond = parse_block_value::<L>(get_required_value(m, "cond", "if.cond")?)?;
    let then = parse_block_value::<L>(get_required_value(m, "then", "if.then")?)?;
    let else_ = match get_optional_value(m, "else") {
        Some(v) => Some(Box::new(parse_block_value::<L>(v)?)),
        None => None,
    };
    deny_unknown_fields(m, "if", &["cond", "then", "else"])?;
    identity_rebuild(Expr::Case {
        whens: vec![(cond, then)],
        else_,
    })
}

fn parse_coalesce<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let args = parse_seq::<L>(value, "coalesce")?;
    identity_rebuild(Expr::Coalesce(args))
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

fn parse_in<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "in",
        reason: "expected mapping with `value`, `list`".into(),
    })?;
    let value_expr = parse_block_value::<L>(get_required_value(m, "value", "in.value")?)?;
    let list = parse_seq::<L>(get_required_value(m, "list", "in.list")?, "in.list")?;
    deny_unknown_fields(m, "in", &["value", "list"])?;
    identity_rebuild(Expr::InList {
        value: Box::new(value_expr),
        list,
        negated: false,
    })
}

fn parse_between<L: LeafResolver>(
    value: &Value,
    negated: bool,
) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "between",
        reason: "expected mapping with `value`, `low`, `high`".into(),
    })?;
    let v_expr = parse_block_value::<L>(get_required_value(m, "value", "between.value")?)?;
    let low = parse_block_value::<L>(get_required_value(m, "low", "between.low")?)?;
    let high = parse_block_value::<L>(get_required_value(m, "high", "between.high")?)?;
    deny_unknown_fields(m, "between", &["value", "low", "high"])?;
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

fn parse_like_kind<L: LeafResolver>(value: &Value, kind: LikeKind) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "like",
        reason: "expected mapping with `value`, `pattern`".into(),
    })?;
    let v_expr = parse_block_value::<L>(get_required_value(m, "value", "like.value")?)?;
    let pattern = parse_block_value::<L>(get_required_value(m, "pattern", "like.pattern")?)?;
    deny_unknown_fields(m, "like", &["value", "pattern"])?;
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
    let inner = parse_block_value::<L>(value)?;
    rebuild_with_check(
        Expr::IsNull(Box::new(Expr::Leaf(L::from_literal(Literal::Null)))),
        vec![inner],
    )
}

fn parse_cast<L: LeafResolver>(value: &Value) -> Result<Expr<L>, ParseError> {
    let m = value.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "cast",
        reason: "expected mapping with `input`, `target`, optional `on_failure`".into(),
    })?;
    let input = parse_block_value::<L>(get_required_value(m, "input", "cast.input")?)?;
    let target = parse_data_type(get_required_value(m, "target", "cast.target")?)?;
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

/// Parse a `target:` field on a `cast:` block. Mirrors the legacy
/// roster — bare snake_case strings for body-less variants, single-key
/// tagged map for `decimal` / `timestamp`.
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

// ── Aggregate sugar ─────────────────────────────────────────────────────

/// Aggregate sugar: `sum: x` → 1-arg form; `count_distinct: x` lowers to
/// `Aggregate { op: Count, distinct: true, ... }`. Body-map form
/// `{ args, distinct?, filter? }` honours all three.
fn parse_aggregate_sugar<L: LeafResolver>(
    body: &Value,
    op: AggregationOp,
    force_distinct: bool,
) -> Result<Expr<L>, ParseError> {
    // Detect body-map form by presence of any of the recognised keys.
    let (args, distinct, filter) = match body {
        Value::Mapping(m) if has_aggregate_body_shape(m) => {
            let args_node = get_required_value(m, "args", "args")?;
            let args = parse_seq::<L>(args_node, "args")?;
            let distinct = get_optional_bool(m, "distinct")?.unwrap_or(false);
            let filter = match get_optional_value(m, "filter") {
                Some(Value::Null) | None => None,
                Some(v) => Some(Box::new(parse_block_value::<L>(v)?)),
            };
            deny_unknown_fields(m, "aggregate", &["args", "distinct", "filter"])?;
            (args, distinct, filter)
        }
        _ => {
            // Single-arg shorthand — the body itself is the sole argument.
            let arg = parse_block_value::<L>(body)?;
            (vec![arg], false, None)
        }
    };

    let final_distinct = force_distinct || distinct;
    identity_rebuild(Expr::Aggregate {
        op,
        args,
        distinct: final_distinct,
        filter,
    })
}

/// Heuristic: a body mapping that carries any of the aggregate-only
/// knobs (`args` / `distinct` / `filter`) is interpreted as the body-map
/// form. Other mappings stay in the single-arg shorthand and reach
/// [`parse_block_value`] directly (e.g. `sum: { col: x }`).
fn has_aggregate_body_shape(m: &Mapping) -> bool {
    m.iter()
        .any(|(k, _)| matches!(k.as_str(), Some("args") | Some("distinct") | Some("filter")))
}

// ── `lit:` body parser (long form + scalar shortcuts) ───────────────────

fn parse_literal_body(value: &Value) -> Result<Literal, ParseError> {
    match value {
        Value::Bool(b) => Ok(Literal::Boolean(*b)),
        Value::Number(n) => number_to_literal(n),
        Value::String(s) => Ok(Literal::String(s.clone())),
        Value::Null => Ok(Literal::Null),
        Value::Mapping(m) => parse_literal_long_form(m),
        Value::Sequence(_) => Err(ParseError::InvalidValue {
            field: "lit",
            reason: "expected scalar value or single-key typed-literal map".into(),
        }),
        Value::Tagged(t) => parse_literal_body(&t.value),
    }
}

fn parse_literal_long_form(m: &Mapping) -> Result<Literal, ParseError> {
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
    Ok(Literal::Decimal {
        value,
        precision,
        scale,
    })
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

// ── Tiny YAML helpers (scoped to this module) ───────────────────────────

fn parse_seq<L: LeafResolver>(
    value: &Value,
    field: &'static str,
) -> Result<Vec<Expr<L>>, ParseError> {
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

/// Re-run [`Tree::with_new_children`] with the candidate's own children
/// — fires the structural well-formedness rules (empty-coalesce / case /
/// in_list, aggregate-in-aggregate) without reshaping the tree.
fn identity_rebuild<L: LeafResolver>(candidate: Expr<L>) -> Result<Expr<L>, ParseError> {
    let kids: Vec<Expr<L>> = candidate.children().into_iter().cloned().collect();
    candidate.with_new_children(kids).map_err(ParseError::Ir)
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

fn get_required_u8(map: &Mapping, key: &str, field: &'static str) -> Result<u8, ParseError> {
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

fn get_required_i8(map: &Mapping, key: &str, field: &'static str) -> Result<i8, ParseError> {
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
        AggregationOp, BinaryOpKind, ColumnRef, DimensionAccessor, Expr, LikeKind, Literal,
        MeasureAccessor, PhysicalLeaf, SemanticLeaf, SemanticsName,
    };

    fn parse_yaml<L: LeafResolver>(yaml: &str) -> ExprSource<L> {
        let value: Value = serde_yaml::from_str(yaml).expect("valid YAML");
        parse_block::<L>(&value).expect("parse_block succeeds")
    }

    fn try_parse_yaml<L: LeafResolver>(yaml: &str) -> Result<ExprSource<L>, ParseError> {
        let value: Value = serde_yaml::from_str(yaml).expect("valid YAML");
        parse_block::<L>(&value)
    }

    fn unwrap_block<L: LeafResolver>(src: ExprSource<L>) -> Expr<L> {
        match src {
            ExprSource::Block(e) => e,
            other => panic!("expected Block, got {other:?}"),
        }
    }

    fn unwrap_inline<L: LeafResolver>(src: ExprSource<L>) -> String {
        match src {
            ExprSource::Inline(s) => s,
            other => panic!("expected Inline, got {other:?}"),
        }
    }

    // ── Inline (string at top level) ───────────────────────────────────

    #[test]
    fn top_level_string_is_inline() {
        let src = parse_yaml::<SemanticLeaf>("'revenue'");
        assert_eq!(unwrap_inline(src), "revenue");
    }

    // ── Bare scalars at top level ──────────────────────────────────────

    #[test]
    fn bare_int_top_level_becomes_literal_block() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("42"));
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(42))));
    }

    #[test]
    fn bare_float_top_level_becomes_literal_block() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("1.5"));
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Float(1.5))));
    }

    #[test]
    fn bare_bool_top_level_becomes_literal_block() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("true"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Literal(Literal::Boolean(true)))
        );
    }

    #[test]
    fn bare_null_top_level_becomes_literal_block() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("null"));
        assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(Literal::Null)));
    }

    // ── Bare-identifier string inside expression (token path) ─────────

    #[test]
    fn bare_identifier_inside_expr_becomes_field_at_semantic_site() {
        let yaml = "{ add: [revenue, cost] }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
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
    fn bare_identifier_inside_expr_becomes_column_at_physical_site() {
        let yaml = "{ add: [amount, tax] }";
        let e = unwrap_block(parse_yaml::<PhysicalLeaf>(yaml));
        match e {
            Expr::BinaryOp { left, right, .. } => {
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

    #[test]
    fn quoted_string_inside_expr_becomes_string_literal() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ eq: [country, \"'DE'\"] }"));
        match e {
            Expr::BinaryOp { right, .. } => {
                assert_eq!(
                    *right,
                    Expr::Leaf(SemanticLeaf::Literal(Literal::String("DE".into())))
                );
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn accessor_in_plain_expr_is_rejected() {
        // `revenue.previous` would only be legal in a `measure:` body.
        let err = try_parse_yaml::<SemanticLeaf>("{ add: [revenue.previous, 1] }").unwrap_err();
        assert!(matches!(
            err,
            ParseError::InvalidValue { field: "expression", .. }
        ));
    }

    // ── Sugar binary operators ─────────────────────────────────────────

    #[test]
    fn sugar_binary_seq_form() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ add: [1, 2] }"));
        match e {
            Expr::BinaryOp { op, .. } => assert_eq!(op, BinaryOpKind::Add),
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn sugar_binary_pair_form() {
        let yaml = "{ subtract: { left: 10, right: 3 } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::BinaryOp { op, left, right } => {
                assert_eq!(op, BinaryOpKind::Subtract);
                assert_eq!(*left, Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(10))));
                assert_eq!(*right, Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(3))));
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn sugar_binary_eq_lt_gteq_and_or() {
        for (tag, want) in [
            ("eq", BinaryOpKind::Eq),
            ("not_eq", BinaryOpKind::NotEq),
            ("lt", BinaryOpKind::Lt),
            ("lt_eq", BinaryOpKind::LtEq),
            ("gt", BinaryOpKind::Gt),
            ("gt_eq", BinaryOpKind::GtEq),
            ("and", BinaryOpKind::And),
            ("or", BinaryOpKind::Or),
            ("multiply", BinaryOpKind::Multiply),
            ("divide", BinaryOpKind::Divide),
            ("safe_divide", BinaryOpKind::SafeDivide),
            ("mod", BinaryOpKind::Mod),
        ] {
            let yaml = format!("{{ {tag}: [1, 2] }}");
            let e = unwrap_block(parse_yaml::<SemanticLeaf>(&yaml));
            match e {
                Expr::BinaryOp { op, .. } => {
                    assert_eq!(op, want, "tag {tag}");
                }
                other => panic!("expected BinaryOp for {tag}, got {other:?}"),
            }
        }
    }

    #[test]
    fn sugar_binary_seq_wrong_arity_rejected() {
        let err = try_parse_yaml::<SemanticLeaf>("{ add: [1, 2, 3] }").unwrap_err();
        assert!(matches!(err, ParseError::InvalidValue { .. }));
    }

    // ── Sugar unary operator (`negate`) ────────────────────────────────

    #[test]
    fn sugar_unary_negate() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ negate: 5 }"));
        match e {
            Expr::UnaryOp { op, operand } => {
                assert_eq!(op, UnaryOpKind::Negate);
                assert_eq!(
                    *operand,
                    Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(5)))
                );
            }
            other => panic!("expected UnaryOp, got {other:?}"),
        }
    }

    // ── Sugar function calls ───────────────────────────────────────────

    #[test]
    fn sugar_function_one_arg_short_form() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ upper: name }"));
        match e {
            Expr::FunctionCall { name, args } => {
                assert_eq!(name.0, "UPPER");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    #[test]
    fn sugar_function_n_arg_seq_form() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ concat: [a, b, c] }"));
        match e {
            Expr::FunctionCall { name, args } => {
                assert_eq!(name.0, "CONCAT");
                assert_eq!(args.len(), 3);
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    #[test]
    fn sugar_function_zero_arg_temporal() {
        // `current_date:` typically takes no args; sequence is the
        // canonical zero-arg shape.
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ current_date: [] }"));
        match e {
            Expr::FunctionCall { name, args } => {
                assert_eq!(name.0, "CURRENT_DATE");
                assert!(args.is_empty());
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    // ── `lit:` forms ───────────────────────────────────────────────────

    #[test]
    fn lit_short_forms() {
        for (yaml, want) in [
            ("{ lit: 7 }", Literal::Integer(7)),
            ("{ lit: 1.5 }", Literal::Float(1.5)),
            ("{ lit: hello }", Literal::String("hello".into())),
            ("{ lit: true }", Literal::Boolean(true)),
            ("{ lit: null }", Literal::Null),
        ] {
            let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
            assert_eq!(e, Expr::Leaf(SemanticLeaf::Literal(want)));
        }
    }

    #[test]
    fn lit_long_form_decimal() {
        let yaml = r#"
lit:
  decimal: { value: "1.23", precision: 4, scale: 2 }
"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
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

    #[test]
    fn lit_long_form_date_and_interval() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(
            r#"{ lit: { date: "2026-01-01" } }"#,
        ));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Literal(Literal::Date("2026-01-01".into())))
        );
        let e2 = unwrap_block(parse_yaml::<SemanticLeaf>(
            r#"{ lit: { interval: "P1D" } }"#,
        ));
        assert_eq!(
            e2,
            Expr::Leaf(SemanticLeaf::Literal(Literal::Interval("P1D".into())))
        );
    }

    #[test]
    fn lit_unknown_typed_tag_is_rejected() {
        let err = try_parse_yaml::<SemanticLeaf>("{ lit: { weirdo: 1 } }").unwrap_err();
        assert!(matches!(err, ParseError::UnknownTag(_)));
    }

    // ── `col:` ──────────────────────────────────────────────────────────

    #[test]
    fn col_at_semantic_site() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ col: revenue }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Column(ColumnRef("revenue".into())))
        );
    }

    #[test]
    fn col_at_physical_site() {
        let e = unwrap_block(parse_yaml::<PhysicalLeaf>("{ col: revenue }"));
        assert_eq!(
            e,
            Expr::Leaf(PhysicalLeaf::Column(ColumnRef("revenue".into())))
        );
    }

    // ── Semantic tags ──────────────────────────────────────────────────

    #[test]
    fn field_tag_short_form() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ field: revenue }"));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Field(SemanticsName("revenue".into())))
        );
    }

    #[test]
    fn dim_tag_with_lag_accessor() {
        let yaml = "{ dim: { name: dt, accessor: { lag: 2 } } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Dimension {
                name: SemanticsName("dt".into()),
                accessor: Some(DimensionAccessor::Lag(2)),
            })
        );
    }

    #[test]
    fn measure_tag_with_previous_accessor() {
        let yaml = "{ measure: { name: revenue, accessor: previous } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        assert_eq!(
            e,
            Expr::Leaf(SemanticLeaf::Measure {
                name: SemanticsName("revenue".into()),
                accessor: Some(MeasureAccessor::Previous),
            })
        );
    }

    #[test]
    fn semantic_tag_at_physical_site_is_rejected() {
        let err = try_parse_yaml::<PhysicalLeaf>("{ field: revenue }").unwrap_err();
        assert!(matches!(
            err,
            ParseError::TagNotAllowedAtSite { site: "physical-mapping", .. }
        ));
    }

    #[test]
    fn dim_at_physical_site_is_rejected() {
        let err = try_parse_yaml::<PhysicalLeaf>("{ dim: x }").unwrap_err();
        assert!(matches!(
            err,
            ParseError::TagNotAllowedAtSite { site: "physical-mapping", .. }
        ));
    }

    // ── `not:` negation fold ───────────────────────────────────────────

    #[test]
    fn not_in_flips_in_list_negation() {
        let yaml = "{ not: { in: { value: country, list: [\"'DE'\", \"'FR'\"] } } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::InList { negated, .. } => assert!(negated),
            other => panic!("expected InList(negated), got {other:?}"),
        }
    }

    #[test]
    fn not_like_flips_to_not_like_kind() {
        let yaml = "{ not: { like: { value: name, pattern: \"'%a%'\" } } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Like { kind, .. } => assert_eq!(kind, LikeKind::NotLike),
            other => panic!("expected Like(NotLike), got {other:?}"),
        }
    }

    #[test]
    fn not_ilike_flips_to_not_ilike_kind() {
        let yaml = "{ not: { ilike: { value: name, pattern: \"'%a%'\" } } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Like { kind, .. } => assert_eq!(kind, LikeKind::NotILike),
            other => panic!("expected Like(NotILike), got {other:?}"),
        }
    }

    #[test]
    fn not_between_flips_negation() {
        let yaml = "{ not: { between: { value: x, low: 1, high: 10 } } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Between { negated, .. } => assert!(negated),
            other => panic!("expected Between(negated), got {other:?}"),
        }
    }

    #[test]
    fn not_on_eq_wraps_in_unary_op() {
        let yaml = "{ not: { eq: [a, b] } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::UnaryOp { op, .. } => assert_eq!(op, UnaryOpKind::Not),
            other => panic!("expected UnaryOp(Not), got {other:?}"),
        }
    }

    // ── `case:` and `if:` ──────────────────────────────────────────────

    #[test]
    fn case_with_else() {
        let yaml = r#"
case:
  whens:
    - when: { gt: [revenue, 0] }
      then: { lit: 1 }
  else: { lit: 0 }
"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Case { whens, else_ } => {
                assert_eq!(whens.len(), 1);
                assert!(else_.is_some());
            }
            other => panic!("expected Case, got {other:?}"),
        }
    }

    #[test]
    fn empty_case_is_rejected() {
        let yaml = "{ case: { whens: [] } }";
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Empty") || msg.contains("ir validation"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn if_lowers_to_case_with_one_when() {
        let yaml = r#"
if:
  cond: { gt: [revenue, 0] }
  then: { lit: 1 }
  else: { lit: 0 }
"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Case { whens, else_ } => {
                assert_eq!(whens.len(), 1);
                assert!(else_.is_some());
            }
            other => panic!("expected Case, got {other:?}"),
        }
    }

    #[test]
    fn if_without_else_is_legal() {
        let yaml = r#"
if:
  cond: { eq: [a, b] }
  then: { lit: 1 }
"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Case { whens, else_ } => {
                assert_eq!(whens.len(), 1);
                assert!(else_.is_none());
            }
            other => panic!("expected Case, got {other:?}"),
        }
    }

    // ── `coalesce:` / `null_if:` ───────────────────────────────────────

    #[test]
    fn coalesce_with_three_args() {
        let yaml = "{ coalesce: [a, b, { lit: 0 }] }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Coalesce(args) => assert_eq!(args.len(), 3),
            other => panic!("expected Coalesce, got {other:?}"),
        }
    }

    #[test]
    fn empty_coalesce_is_rejected() {
        let yaml = "{ coalesce: [] }";
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Empty") || msg.contains("Coalesce") || msg.contains("ir validation"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn null_if_pair() {
        let yaml = "{ null_if: { left: a, right: { lit: 0 } } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        assert!(matches!(e, Expr::NullIf { .. }));
    }

    // ── `in:` / `between:` ─────────────────────────────────────────────

    #[test]
    fn in_emits_non_negated() {
        let yaml = "{ in: { value: country, list: [\"'DE'\", \"'FR'\"] } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::InList { negated, list, .. } => {
                assert!(!negated);
                assert_eq!(list.len(), 2);
            }
            other => panic!("expected InList, got {other:?}"),
        }
    }

    #[test]
    fn empty_in_list_is_rejected() {
        let yaml = "{ in: { value: country, list: [] } }";
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("EmptyInList") || msg.contains("Empty") || msg.contains("ir validation"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn between_emits_non_negated() {
        let yaml = "{ between: { value: x, low: 1, high: 10 } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Between { negated, .. } => assert!(!negated),
            other => panic!("expected Between, got {other:?}"),
        }
    }

    // ── `like:` / `ilike:` only — `not_like:` is rejected as a tag ────

    #[test]
    fn like_emits_like_kind() {
        let yaml = "{ like: { value: name, pattern: \"'%a%'\" } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Like { kind, .. } => assert_eq!(kind, LikeKind::Like),
            other => panic!("expected Like, got {other:?}"),
        }
    }

    #[test]
    fn ilike_emits_ilike_kind() {
        let yaml = "{ ilike: { value: name, pattern: \"'%a%'\" } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Like { kind, .. } => assert_eq!(kind, LikeKind::ILike),
            other => panic!("expected Like(ILike), got {other:?}"),
        }
    }

    #[test]
    fn not_like_tag_is_unknown() {
        let err = try_parse_yaml::<SemanticLeaf>(
            "{ not_like: { value: name, pattern: \"'%a%'\" } }",
        )
        .unwrap_err();
        assert!(matches!(err, ParseError::UnknownTag(t) if t == "not_like"));
    }

    #[test]
    fn not_ilike_tag_is_unknown() {
        let err = try_parse_yaml::<SemanticLeaf>(
            "{ not_ilike: { value: name, pattern: \"'%a%'\" } }",
        )
        .unwrap_err();
        assert!(matches!(err, ParseError::UnknownTag(t) if t == "not_ilike"));
    }

    // ── `is_null:` / `cast:` ───────────────────────────────────────────

    #[test]
    fn is_null_with_bare_arg() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ is_null: revenue }"));
        assert!(matches!(e, Expr::IsNull(_)));
    }

    #[test]
    fn cast_to_string_default_failure() {
        let yaml = "{ cast: { input: x, target: string } }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Cast {
                target,
                on_failure,
                ..
            } => {
                assert!(matches!(target, DataType::String));
                assert!(matches!(on_failure, CastFailure::Error));
            }
            other => panic!("expected Cast, got {other:?}"),
        }
    }

    #[test]
    fn cast_with_on_failure_null() {
        // `null` is a YAML reserved scalar; the author has to quote the
        // word "null" to keep it as the string the parser expects.
        let yaml = r#"{ cast: { input: x, target: integer, on_failure: "null" } }"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Cast { on_failure, .. } => assert!(matches!(on_failure, CastFailure::Null)),
            other => panic!("expected Cast, got {other:?}"),
        }
    }

    #[test]
    fn cast_to_decimal_long_form() {
        let yaml = r#"
cast:
  input: x
  target: { decimal: { precision: 10, scale: 2 } }
"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Cast { target, .. } => match target {
                DataType::Decimal { precision, scale } => {
                    assert_eq!(precision, 10);
                    assert_eq!(scale, 2);
                }
                other => panic!("expected Decimal, got {other:?}"),
            },
            other => panic!("expected Cast, got {other:?}"),
        }
    }

    // ── Aggregate sugar ────────────────────────────────────────────────

    #[test]
    fn sum_short_form() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ sum: revenue }"));
        match e {
            Expr::Aggregate {
                op,
                args,
                distinct,
                filter,
            } => {
                assert_eq!(op, AggregationOp::Sum);
                assert_eq!(args.len(), 1);
                assert!(!distinct);
                assert!(filter.is_none());
            }
            other => panic!("expected Aggregate, got {other:?}"),
        }
    }

    #[test]
    fn count_distinct_lowers_to_count_with_distinct() {
        let e = unwrap_block(parse_yaml::<SemanticLeaf>("{ count_distinct: x }"));
        match e {
            Expr::Aggregate { op, distinct, .. } => {
                assert_eq!(op, AggregationOp::Count);
                assert!(distinct);
            }
            other => panic!("expected Aggregate(Count, distinct), got {other:?}"),
        }
    }

    #[test]
    fn aggregate_body_map_form_with_filter() {
        let yaml = r#"
sum:
  args: [revenue]
  filter: { gt: [revenue, 0] }
  distinct: true
"#;
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml));
        match e {
            Expr::Aggregate {
                op,
                args,
                distinct,
                filter,
            } => {
                assert_eq!(op, AggregationOp::Sum);
                assert_eq!(args.len(), 1);
                assert!(distinct);
                assert!(filter.is_some());
            }
            other => panic!("expected Aggregate, got {other:?}"),
        }
    }

    #[test]
    fn aggregate_in_aggregate_is_rejected() {
        let yaml = r#"
sum:
  args: [{ avg: revenue }]
"#;
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("AggregateInAggregate")
                || msg.contains("ir validation")
                || msg.contains("Aggregate"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn min_and_max_short_form() {
        let yaml_min = "{ min: x }";
        let e = unwrap_block(parse_yaml::<SemanticLeaf>(yaml_min));
        match e {
            Expr::Aggregate { op, .. } => assert_eq!(op, AggregationOp::Min),
            other => panic!("expected Aggregate, got {other:?}"),
        }
        let yaml_max = "{ max: x }";
        let e2 = unwrap_block(parse_yaml::<SemanticLeaf>(yaml_max));
        match e2 {
            Expr::Aggregate { op, .. } => assert_eq!(op, AggregationOp::Max),
            other => panic!("expected Aggregate, got {other:?}"),
        }
    }

    // ── Errors ─────────────────────────────────────────────────────────

    #[test]
    fn window_tag_is_rejected_at_expr_site() {
        let yaml = "{ window: { function: lag, args: [], partition_by: [], order_by: [] } }";
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        assert!(matches!(
            err,
            ParseError::TagNotAllowedAtSite { site: "expr", .. }
        ));
    }

    #[test]
    fn unknown_tag_is_rejected() {
        let err = try_parse_yaml::<SemanticLeaf>("{ unknown_tag: x }").unwrap_err();
        assert!(matches!(err, ParseError::UnknownTag(t) if t == "unknown_tag"));
    }

    #[test]
    fn ambiguous_two_key_top_level_is_rejected() {
        let err = try_parse_yaml::<SemanticLeaf>("{ lit: 1, col: x }").unwrap_err();
        assert!(matches!(err, ParseError::AmbiguousTag(2)));
    }

    #[test]
    fn case_unknown_field_is_rejected() {
        let yaml = r#"
case:
  whens: [{ when: a, then: 1 }]
  bogus: 1
"#;
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        assert!(matches!(err, ParseError::InvalidValue { field: "case", .. }));
    }

    #[test]
    fn binary_pair_form_unknown_field_is_rejected() {
        let yaml = "{ add: { left: 1, right: 2, bogus: 3 } }";
        let err = try_parse_yaml::<SemanticLeaf>(yaml).unwrap_err();
        assert!(matches!(err, ParseError::InvalidValue { .. }));
    }
}
