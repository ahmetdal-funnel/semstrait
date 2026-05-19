//! Single tokenizer shared by every leaf scalar in the YAML authoring
//! surface (and reused by the future inline-DSL parser).
//!
//! # Token rules (ratified Phase 8)
//!
//! | Author writes | Token | Outcome (semantic site) |
//! |---|---|---|
//! | `42`, `-7` | [`LeafToken::Integer`] | `Literal::Integer` |
//! | `1.5`, `1e3` | [`LeafToken::Float`] | `Literal::Float` |
//! | `true`, `false` | [`LeafToken::Boolean`] | `Literal::Boolean` |
//! | `null` | [`LeafToken::Null`] | `Literal::Null` |
//! | `'Germany'` | [`LeafToken::String`] | `Literal::String` |
//! | `"my.dotted.name"` | [`LeafToken::Name { dotted: true }`] | one identifier (no accessor split) |
//! | `country` | [`LeafToken::Name { dotted: false }`] | `Field` (semantic) / `Column` (physical) |
//! | `country.previous` | [`LeafToken::NameWithAccessor`] | accessor-bearing leaf |
//! | `shopify-shipping_country` | [`LeafToken::Name`] | one identifier (item 7) |
//!
//! Bare `null` / `true` / `false` / numerics self-identify. Any other
//! bare scalar is a name. DSL-level single quotes wrap string literals;
//! double quotes wrap names containing literal dots.
//!
//! Numerics, single-quoted strings and double-quoted names are
//! recognised by the tokenizer; YAML's own quote-style is irrelevant
//! (per the m09-domain "host syntax must not decide DSL semantics"
//! constraint — Phase 8).

use super::error::ParseError;

/// One leaf-position token. Output of [`tokenize_leaf`].
#[derive(Debug, Clone, PartialEq)]
pub enum LeafToken {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Null,
    /// Bare or double-quoted identifier with no accessor.
    Name(String),
    /// Bare identifier with one accessor segment after the first dot.
    /// Only valid at sites that admit an accessor (`dim:` / `measure:`
    /// / `metric:` / `key:` bodies).
    NameWithAccessor { name: String, accessor: String },
}

/// Tokenize one author-written scalar at a leaf position.
///
/// The input is the **DSL view** of the scalar, i.e. the string body
/// with YAML quoting already stripped by the YAML parser.
pub fn tokenize_leaf(raw: &str) -> Result<LeafToken, ParseError> {
    let s = raw;

    // Reserved literals first.
    if s == "null" {
        return Ok(LeafToken::Null);
    }
    if s == "true" {
        return Ok(LeafToken::Boolean(true));
    }
    if s == "false" {
        return Ok(LeafToken::Boolean(false));
    }

    // DSL single-quoted string literal: 'text'. Minimal escapes: \\ \'.
    if let Some(body) = strip_quoted(s, '\'') {
        return Ok(LeafToken::String(unescape_minimal(body)));
    }

    // DSL double-quoted dotted-identifier escape: "a.b.c".
    if let Some(body) = strip_quoted(s, '"') {
        if body.is_empty() {
            return Err(ParseError::InvalidToken {
                raw: raw.to_owned(),
                reason: "empty quoted identifier",
            });
        }
        return Ok(LeafToken::Name(unescape_minimal(body)));
    }

    // Numeric: integer first, then float fallback.
    if let Ok(i) = s.parse::<i64>() {
        return Ok(LeafToken::Integer(i));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(LeafToken::Float(f));
    }

    // Bare name with optional single accessor split on the first dot.
    if s.is_empty() {
        return Err(ParseError::InvalidToken {
            raw: raw.to_owned(),
            reason: "empty identifier",
        });
    }
    if let Some(idx) = s.find('.') {
        let (name, rest) = s.split_at(idx);
        let accessor = &rest[1..]; // drop the dot
        if name.is_empty() {
            return Err(ParseError::InvalidToken {
                raw: raw.to_owned(),
                reason: "accessor without name",
            });
        }
        if accessor.is_empty() {
            return Err(ParseError::InvalidToken {
                raw: raw.to_owned(),
                reason: "trailing dot",
            });
        }
        // We forbid further dots in the accessor for v1; if you need
        // dots in a name, double-quote it.
        if accessor.contains('.') {
            return Err(ParseError::InvalidToken {
                raw: raw.to_owned(),
                reason: "accessor must not contain a dot; double-quote the name to embed dots",
            });
        }
        return Ok(LeafToken::NameWithAccessor {
            name: name.to_owned(),
            accessor: accessor.to_owned(),
        });
    }

    Ok(LeafToken::Name(s.to_owned()))
}

fn strip_quoted(s: &str, q: char) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] as char == q && bytes[bytes.len() - 1] as char == q {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

fn unescape_minimal(s: &str) -> String {
    // Two recognised escapes: \\ → \ and \' → ' (or \" → "). Anything
    // else stays literal; we are intentionally minimal here so authors
    // don't trip over surprising escape sequences.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next @ ('\\' | '\'' | '"')) => out.push(next),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_and_booleans() {
        assert_eq!(tokenize_leaf("null").unwrap(), LeafToken::Null);
        assert_eq!(tokenize_leaf("true").unwrap(), LeafToken::Boolean(true));
        assert_eq!(tokenize_leaf("false").unwrap(), LeafToken::Boolean(false));
    }

    #[test]
    fn integer_and_float() {
        assert_eq!(tokenize_leaf("42").unwrap(), LeafToken::Integer(42));
        assert_eq!(tokenize_leaf("-7").unwrap(), LeafToken::Integer(-7));
        match tokenize_leaf("1.5").unwrap() {
            LeafToken::Float(f) => assert!((f - 1.5).abs() < f64::EPSILON),
            other => panic!("expected Float, got {other:?}"),
        }
        match tokenize_leaf("1e3").unwrap() {
            LeafToken::Float(f) => assert!((f - 1000.0).abs() < f64::EPSILON),
            other => panic!("expected Float, got {other:?}"),
        }
    }

    #[test]
    fn single_quoted_string_literal() {
        assert_eq!(
            tokenize_leaf("'Germany'").unwrap(),
            LeafToken::String("Germany".into())
        );
        assert_eq!(tokenize_leaf("''").unwrap(), LeafToken::String("".into()));
    }

    #[test]
    fn double_quoted_dotted_identifier() {
        assert_eq!(
            tokenize_leaf(r#""my.dotted.name""#).unwrap(),
            LeafToken::Name("my.dotted.name".into())
        );
    }

    #[test]
    fn double_quoted_empty_rejects() {
        let err = tokenize_leaf(r#""""#).unwrap_err();
        assert!(matches!(
            err,
            ParseError::InvalidToken { reason: "empty quoted identifier", .. }
        ));
    }

    #[test]
    fn bare_identifier_with_dash_is_one_atom() {
        assert_eq!(
            tokenize_leaf("shopify-shipping_country").unwrap(),
            LeafToken::Name("shopify-shipping_country".into())
        );
    }

    #[test]
    fn bare_identifier_with_dot_splits_accessor() {
        assert_eq!(
            tokenize_leaf("revenue.previous").unwrap(),
            LeafToken::NameWithAccessor {
                name: "revenue".into(),
                accessor: "previous".into(),
            }
        );
    }

    #[test]
    fn multi_dot_in_bare_form_rejects() {
        let err = tokenize_leaf("a.b.c").unwrap_err();
        assert!(matches!(err, ParseError::InvalidToken { .. }));
    }

    #[test]
    fn trailing_dot_rejects() {
        let err = tokenize_leaf("a.").unwrap_err();
        assert!(matches!(
            err,
            ParseError::InvalidToken { reason: "trailing dot", .. }
        ));
    }

    #[test]
    fn empty_input_rejects() {
        let err = tokenize_leaf("").unwrap_err();
        assert!(matches!(
            err,
            ParseError::InvalidToken { reason: "empty identifier", .. }
        ));
    }

    #[test]
    fn escape_sequences_in_string_literal() {
        assert_eq!(
            tokenize_leaf(r#"'it\'s'"#).unwrap(),
            LeafToken::String("it's".into())
        );
        assert_eq!(
            tokenize_leaf(r"'one\\two'").unwrap(),
            LeafToken::String("one\\two".into())
        );
    }
}
