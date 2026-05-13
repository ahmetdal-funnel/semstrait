//! `LiteralValue` bare-scalar widening — D-5 regression coverage.
//!
//! The author surface accepts both bare YAML scalars (string / integer
//! / float / bool — type derived from the YAML scalar shape) and the
//! externally-tagged single-key map form (`{string: USD}`, `{int: 5}`,
//! …). `Decimal` / `Date` / `Timestamp` must always use the tagged
//! form because YAML scalars cannot disambiguate them from strings.
//! Serialisation always emits the tagged form for round-trip stability.

use semstrait_model::LiteralValue;

// ── Bare-scalar parses ───────────────────────────────────────────────

#[test]
fn parses_bare_string_scalar_as_string_literal() {
    let lit: LiteralValue = serde_yaml::from_str("USD").expect("parse USD");
    assert_eq!(lit, LiteralValue::String("USD".into()));
}

#[test]
fn parses_bare_integer_scalar_as_int_literal() {
    let lit: LiteralValue = serde_yaml::from_str("42").expect("parse 42");
    assert_eq!(lit, LiteralValue::Int(42));
}

#[test]
fn parses_bare_float_scalar_as_float_literal() {
    let lit: LiteralValue = serde_yaml::from_str("2.5").expect("parse 2.5");
    assert_eq!(lit, LiteralValue::Float(2.5));
}

#[test]
fn parses_bare_bool_scalar_as_bool_literal() {
    let lit: LiteralValue = serde_yaml::from_str("true").expect("parse true");
    assert_eq!(lit, LiteralValue::Bool(true));
}

#[test]
fn parses_bare_null_as_null_literal() {
    // Both YAML `null` and the bare string `"null"` carry the body-less
    // Null meaning per the existing surface.
    let from_yaml_null: LiteralValue = serde_yaml::from_str("null").expect("parse null");
    assert_eq!(from_yaml_null, LiteralValue::Null);

    let from_string_null: LiteralValue =
        serde_yaml::from_str("\"null\"").expect("parse quoted null");
    assert_eq!(from_string_null, LiteralValue::Null);
}

// ── Tagged-form parses (existing baseline still works) ───────────────

#[test]
fn parses_tagged_string_as_string_literal() {
    // Tagged form must produce the same `LiteralValue::String("USD")`
    // as the bare-string shortcut.
    let lit: LiteralValue = serde_yaml::from_str("string: USD").expect("parse {string: USD}");
    assert_eq!(lit, LiteralValue::String("USD".into()));
}

#[test]
fn parses_tagged_decimal_as_decimal_literal() {
    // Decimal cannot be reached via the bare-scalar path — YAML cannot
    // tell a decimal-as-string from any other string.
    let lit: LiteralValue =
        serde_yaml::from_str("decimal: \"19.99\"").expect("parse {decimal: \"19.99\"}");
    assert_eq!(lit, LiteralValue::Decimal("19.99".into()));
}

// ── Round-trip stability ─────────────────────────────────────────────

#[test]
fn serialised_form_canonical_regardless_of_input_shape() {
    // Whether the input was authored as a bare scalar or as the tagged
    // single-key map form, the in-memory `LiteralValue` must be the
    // same value AND `Serialize` must emit the same canonical output
    // for both. P5 fixes the wire form to the single-key map shape
    // (`string: USD`) so it matches the form `Deserialize` accepts.
    let from_bare: LiteralValue = serde_yaml::from_str("USD").expect("parse bare");
    let from_tagged: LiteralValue =
        serde_yaml::from_str("string: USD").expect("parse tagged");
    assert_eq!(from_bare, from_tagged);

    let bare_serialised = serde_yaml::to_string(&from_bare).expect("serialize bare");
    let tagged_serialised = serde_yaml::to_string(&from_tagged).expect("serialize tagged");
    assert_eq!(bare_serialised, tagged_serialised);

    // The serialised text must be the single-key map form (`string: USD`),
    // not the YAML tag-scalar form (`!string USD`). The leading `!` is
    // the tell.
    assert!(
        !bare_serialised.trim_start().starts_with('!'),
        "serialised LiteralValue should be a mapping, not a tag-scalar; got {bare_serialised:?}"
    );
    assert!(
        bare_serialised.contains("string"),
        "serialised LiteralValue should carry the variant tag; got {bare_serialised:?}"
    );

    // Reciprocity: the serialised text must round-trip back to the same
    // value via the same `Deserialize` impl.
    let reparsed: LiteralValue =
        serde_yaml::from_str(&bare_serialised).expect("re-parse serialised form");
    assert_eq!(reparsed, from_bare);
}

#[test]
fn round_trip_holds_for_every_authorable_variant() {
    // Walk every variant `Serialize` is permitted to emit (`Metadata`
    // is excluded — it's compile-synthesized and has no author surface).
    // For each one, `from_str(to_string(v)) == v` must hold.
    let cases = vec![
        LiteralValue::Null,
        LiteralValue::Bool(true),
        LiteralValue::Int(42),
        LiteralValue::Float(2.5),
        LiteralValue::Decimal("19.99".into()),
        LiteralValue::String("USD".into()),
        LiteralValue::Date("2026-05-12".into()),
        LiteralValue::Timestamp("2026-05-12T18:30:00Z".into()),
    ];

    for original in cases {
        let serialised = serde_yaml::to_string(&original)
            .unwrap_or_else(|e| panic!("serialize {original:?}: {e}"));
        let reparsed: LiteralValue = serde_yaml::from_str(&serialised)
            .unwrap_or_else(|e| panic!("re-parse {serialised:?} ({original:?}): {e}"));
        assert_eq!(reparsed, original, "round-trip for {original:?}");
    }
}
