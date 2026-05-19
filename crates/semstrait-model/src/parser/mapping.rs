//! `semantic_mapping:` value parser (Phase 8 Pass B).
//!
//! Decomposes a [`serde_yaml::Value`] in a `semantic_mapping:` arm into a
//! [`SemanticMappingValue`] per the three author-facing forms of `18 §10`:
//!
//! - Bare scalar (`Value::String`) → [`SemanticMappingValue::Column`].
//! - `{lit: <body>}` → [`SemanticMappingValue::Literal`]. Body parsed by
//!   the existing [`LiteralValue`] `Deserialize` (bare scalars +
//!   externally-tagged single-key map roster).
//! - `{expr: <body>}` → [`SemanticMappingValue::Expr`]. Body parsed by
//!   the existing [`ExprSource<PhysicalLeaf>`] `Deserialize` — physical-
//!   mapping-site rules (`field` / `dim` / `measure` / `metric` / `key`
//!   tags rejected) come for free from the leaf-set's `LeafResolver`
//!   impl.
//!
//! The fourth `SemanticMappingValue::Metadata` arm is compile-synthesized
//! only (`18 §10.4`); it has no YAML representation here.
//!
//! Anything else surfaces as [`ParseError::InvalidMappingValue`].

use crate::entities::mapping::{LiteralValue, SemanticMappingValue};
use crate::expr_source::ExprSource;
use crate::parser::error::ParseError;
use semstrait_ir::PhysicalLeaf;
use serde_yaml::Value;

/// Reserved key for the literal arm. Renamed from `literal` to `lit`
/// (Phase 8) for symmetry with the inline / block-form `lit:` tag in
/// `parser::block`.
const LIT_KEY: &str = "lit";
/// Reserved key for the expression arm.
const EXPR_KEY: &str = "expr";

/// Parse a single `semantic_mapping:` arm value.
///
/// Drives the bare-scalar / `{lit: ...}` / `{expr: ...}` dispatch laid
/// out at the module level. `Metadata` is unreachable here by design —
/// it is compile-synthesized only.
pub fn deserialize_mapping_value(value: &Value) -> Result<SemanticMappingValue, ParseError> {
    match value {
        Value::String(s) => Ok(SemanticMappingValue::Column(s.clone())),
        Value::Mapping(map) => {
            // Single-key dispatch keyed off `lit` vs `expr`. Authors
            // cannot mix the two in one arm — that's an `InvalidMappingValue`.
            let lit = map.get(Value::String(LIT_KEY.into()));
            let expr = map.get(Value::String(EXPR_KEY.into()));
            match (lit, expr, map.len()) {
                (Some(body), None, 1) => {
                    let v: LiteralValue = serde_yaml::from_value(body.clone()).map_err(|e| {
                        ParseError::InvalidMappingValue(format!("`lit:` body: {e}"))
                    })?;
                    Ok(SemanticMappingValue::Literal(v))
                }
                (None, Some(body), 1) => {
                    let v: ExprSource<PhysicalLeaf> = serde_yaml::from_value(body.clone())
                        .map_err(|e| {
                            ParseError::InvalidMappingValue(format!("`expr:` body: {e}"))
                        })?;
                    Ok(SemanticMappingValue::Expr(v))
                }
                _ => Err(ParseError::InvalidMappingValue(
                    "expected bare column string or `{lit: ...}` / `{expr: ...}` map".into(),
                )),
            }
        }
        other => Err(ParseError::InvalidMappingValue(format!(
            "unexpected YAML shape {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_ir::{ColumnRef, Expr, Literal as IrLiteral, PhysicalLeaf};

    fn yaml(s: &str) -> Value {
        serde_yaml::from_str(s).expect("valid YAML")
    }

    // ── bare scalar = Column ──────────────────────────────────────────

    #[test]
    fn bare_string_is_column() {
        let v = yaml("amount_cents");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(out, SemanticMappingValue::Column("amount_cents".into()));
    }

    // ── `lit:` arm — bare-scalar body shortcuts ───────────────────────

    #[test]
    fn lit_bare_int_is_int_literal() {
        let v = yaml("{ lit: 0 }");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(out, SemanticMappingValue::Literal(LiteralValue::Int(0)));
    }

    #[test]
    fn lit_bare_string_is_string_literal() {
        let v = yaml(r#"{ lit: "USD" }"#);
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(
            out,
            SemanticMappingValue::Literal(LiteralValue::String("USD".into()))
        );
    }

    #[test]
    fn lit_bare_bool_is_bool_literal() {
        let v = yaml("{ lit: true }");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(out, SemanticMappingValue::Literal(LiteralValue::Bool(true)));
    }

    #[test]
    fn lit_bare_null_is_null_literal() {
        let v = yaml("{ lit: null }");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(out, SemanticMappingValue::Literal(LiteralValue::Null));
    }

    // ── `lit:` arm — tagged-form body ─────────────────────────────────

    #[test]
    fn lit_tagged_int() {
        let v = yaml("{ lit: { int: 5 } }");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(out, SemanticMappingValue::Literal(LiteralValue::Int(5)));
    }

    #[test]
    fn lit_tagged_string() {
        let v = yaml(r#"{ lit: { string: "Paid Search" } }"#);
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(
            out,
            SemanticMappingValue::Literal(LiteralValue::String("Paid Search".into()))
        );
    }

    #[test]
    fn lit_tagged_decimal_keeps_string_payload() {
        let v = yaml(r#"{ lit: { decimal: "1.23" } }"#);
        let out = deserialize_mapping_value(&v).expect("parse ok");
        assert_eq!(
            out,
            SemanticMappingValue::Literal(LiteralValue::Decimal("1.23".into()))
        );
    }

    // ── `expr:` arm — typed ExprSource<PhysicalLeaf> ──────────────────

    #[test]
    fn expr_block_function_call() {
        // Sugar form `upper:` from `parser::sugar::function_for_tag`.
        // There is no author-facing `function_call:` escape hatch — every
        // function tag must be in the closed sugar roster.
        let v = yaml(r#"{ expr: { upper: { col: amount } } }"#);
        let out = deserialize_mapping_value(&v).expect("parse ok");
        match out {
            SemanticMappingValue::Expr(ExprSource::Block(_)) => {}
            other => panic!("expected Expr(Block(_)), got {other:?}"),
        }
    }

    #[test]
    fn expr_bare_column_via_col_tag() {
        let v = yaml("{ expr: { col: amount } }");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        match out {
            SemanticMappingValue::Expr(ExprSource::Block(Expr::Leaf(PhysicalLeaf::Column(
                ColumnRef(ref name),
            )))) if name == "amount" => {}
            other => panic!("expected col leaf, got {other:?}"),
        }
    }

    #[test]
    fn expr_lit_inside_expr_arm() {
        let v = yaml("{ expr: { lit: 42 } }");
        let out = deserialize_mapping_value(&v).expect("parse ok");
        match out {
            SemanticMappingValue::Expr(ExprSource::Block(Expr::Leaf(PhysicalLeaf::Literal(
                IrLiteral::Integer(42),
            )))) => {}
            other => panic!("expected lit leaf, got {other:?}"),
        }
    }

    #[test]
    fn expr_rejects_semantic_tag_at_physical_site() {
        // `field:` is a semantic tag — rejected by PhysicalLeaf's
        // LeafResolver impl. The error surfaces as InvalidMappingValue
        // (we wrap the underlying ExprSource ParseError into the
        // mapping-level diagnostic).
        let v = yaml("{ expr: { field: revenue } }");
        let err = deserialize_mapping_value(&v).expect_err("should reject");
        match err {
            ParseError::InvalidMappingValue(msg) => {
                assert!(
                    msg.contains("physical-mapping") || msg.contains("field"),
                    "expected site-rejection message, got: {msg}"
                );
            }
            other => panic!("expected InvalidMappingValue, got {other:?}"),
        }
    }

    // ── unknown / mixed shapes reject ─────────────────────────────────

    #[test]
    fn unknown_top_key_rejects() {
        let v = yaml("{ wat: 1 }");
        let err = deserialize_mapping_value(&v).expect_err("should reject");
        assert!(matches!(err, ParseError::InvalidMappingValue(_)));
    }

    #[test]
    fn both_lit_and_expr_in_one_arm_rejects() {
        let v = yaml("{ lit: 0, expr: { col: x } }");
        let err = deserialize_mapping_value(&v).expect_err("should reject");
        assert!(matches!(err, ParseError::InvalidMappingValue(_)));
    }

    #[test]
    fn legacy_literal_keyword_rejects() {
        // The renamed-from `literal:` keyword must no longer be
        // recognized — Pass C updates fixtures.
        let v = yaml("{ literal: { string: USD } }");
        let err = deserialize_mapping_value(&v).expect_err("should reject");
        assert!(matches!(err, ParseError::InvalidMappingValue(_)));
    }

    #[test]
    fn sequence_at_top_level_rejects() {
        let v = yaml("[ 1, 2 ]");
        let err = deserialize_mapping_value(&v).expect_err("should reject");
        assert!(matches!(err, ParseError::InvalidMappingValue(_)));
    }

    // ── round-trip: Serialize emits `lit:` (not `literal:`) ───────────

    #[test]
    fn round_trip_literal_uses_lit_keyword() {
        // Construct a Literal arm and serialize through the standard
        // `SemanticMappingValue::Serialize` impl (in `entities/mapping.rs`).
        // The wire shape must use `lit:` not the legacy `literal:`.
        let v = SemanticMappingValue::Literal(LiteralValue::String("Paid Search".into()));
        let yaml_str = serde_yaml::to_string(&v).expect("serialise ok");
        assert!(
            yaml_str.contains("lit:"),
            "expected `lit:` in output, got: {yaml_str}"
        );
        assert!(
            !yaml_str.contains("literal:"),
            "should not emit legacy `literal:`, got: {yaml_str}"
        );
        // And it must round-trip.
        let parsed: SemanticMappingValue = serde_yaml::from_str(&yaml_str).expect("round-trip ok");
        assert_eq!(parsed, v);
    }

    #[test]
    fn round_trip_column_is_bare_scalar() {
        let v = SemanticMappingValue::Column("amount_cents".into());
        let yaml_str = serde_yaml::to_string(&v).expect("serialise ok");
        let parsed: SemanticMappingValue = serde_yaml::from_str(&yaml_str).expect("round-trip ok");
        assert_eq!(parsed, v);
    }
}
