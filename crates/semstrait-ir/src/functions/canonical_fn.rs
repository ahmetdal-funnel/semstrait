//! Canonical function identity newtype. Per `35 §8.2` / `14a §2`.

use crate::error::ValidateError;

/// Canonical function identity.
///
/// Newtype-over-stable per `30 §4.3` — explicitly NOT
/// `#[non_exhaustive]` per spec `35 §8.2`. The inner string is
/// crate-private; consumers construct via [`CanonicalFn::new`] and
/// borrow via [`CanonicalFn::as_str`]. Names are validated against the
/// `14 §6.5` identifier grammar `[A-Za-z_][A-Za-z0-9_]*` and normalized
/// to lowercase ASCII per `14a §2.3` at construction.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct CanonicalFn {
    name: String,
}

impl CanonicalFn {
    /// Validate per `14 §6.5` identifier grammar and normalize to
    /// lowercase ASCII per `14a §2.3`. Returns
    /// [`ValidateError::InvalidCanonicalFn`] for empty input,
    /// non-ASCII characters, leading digit, or any character outside
    /// `[A-Za-z0-9_]`.
    pub fn new(name: impl Into<String>) -> Result<Self, ValidateError> {
        let raw: String = name.into();
        let normalized = validate_and_normalize(&raw)?;
        Ok(Self { name: normalized })
    }

    /// Borrow the normalized canonical name (always lowercase ASCII).
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

fn validate_and_normalize(s: &str) -> Result<String, ValidateError> {
    if s.is_empty() {
        return Err(ValidateError::InvalidCanonicalFn {
            supplied: s.to_string(),
            reason: "empty",
        });
    }
    let mut iter = s.bytes();
    let first = iter.next().expect("non-empty checked above");
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return Err(ValidateError::InvalidCanonicalFn {
            supplied: s.to_string(),
            reason: "first character must be ASCII letter or underscore",
        });
    }
    for b in iter {
        if !(b.is_ascii_alphanumeric() || b == b'_') {
            return Err(ValidateError::InvalidCanonicalFn {
                supplied: s.to_string(),
                reason: "non-grammar character (allowed: ASCII letters, digits, underscore)",
            });
        }
    }
    Ok(s.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn new_accepts_valid_lowercase() {
        let f = CanonicalFn::new("upper").unwrap();
        assert_eq!(f.as_str(), "upper");
    }

    #[test]
    fn new_normalizes_uppercase_to_lowercase() {
        let f = CanonicalFn::new("UPPER").unwrap();
        assert_eq!(f.as_str(), "upper");
    }

    #[test]
    fn new_normalizes_mixed_case() {
        let f = CanonicalFn::new("DateAdd").unwrap();
        assert_eq!(f.as_str(), "dateadd");
    }

    #[test]
    fn new_accepts_underscore_leading() {
        let f = CanonicalFn::new("_priv").unwrap();
        assert_eq!(f.as_str(), "_priv");
    }

    #[test]
    fn new_accepts_digits_after_first() {
        let f = CanonicalFn::new("log10").unwrap();
        assert_eq!(f.as_str(), "log10");
    }

    #[test]
    fn new_rejects_empty() {
        let err = CanonicalFn::new("").unwrap_err();
        match err {
            ValidateError::InvalidCanonicalFn { supplied, reason } => {
                assert_eq!(supplied, "");
                assert_eq!(reason, "empty");
            }
            other => panic!("expected InvalidCanonicalFn, got {other:?}"),
        }
    }

    #[test]
    fn new_rejects_leading_digit() {
        let err = CanonicalFn::new("9func").unwrap_err();
        match err {
            ValidateError::InvalidCanonicalFn { supplied, .. } => assert_eq!(supplied, "9func"),
            other => panic!("expected InvalidCanonicalFn, got {other:?}"),
        }
    }

    #[test]
    fn new_rejects_non_ascii() {
        assert!(CanonicalFn::new("über").is_err());
        assert!(CanonicalFn::new("函数").is_err());
        assert!(CanonicalFn::new("café").is_err());
    }

    #[test]
    fn new_rejects_punctuation() {
        assert!(CanonicalFn::new("foo-bar").is_err());
        assert!(CanonicalFn::new("foo bar").is_err());
        assert!(CanonicalFn::new("foo.bar").is_err());
        assert!(CanonicalFn::new("foo(").is_err());
    }

    #[test]
    fn equality_after_case_normalization() {
        let a = CanonicalFn::new("FOO").unwrap();
        let b = CanonicalFn::new("foo").unwrap();
        let c = CanonicalFn::new("Foo").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn hash_consistent_with_equality() {
        let a = CanonicalFn::new("UPPER").unwrap();
        let b = CanonicalFn::new("upper").unwrap();
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn ord_lexicographic_on_normalized() {
        let a = CanonicalFn::new("alpha").unwrap();
        let b = CanonicalFn::new("BETA").unwrap();
        assert!(a < b, "lowercase normalization preserves ordering");
        let c = CanonicalFn::new("AlPhA").unwrap();
        assert_eq!(a, c);
    }
}
