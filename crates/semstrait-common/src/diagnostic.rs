//! Diagnostic primitives. Spec `31 §4`.
//!
//! Every consumer crate composes [`Diagnostic<K>`] around its own per-stage
//! typed-kind enum. Construction lives at the consumer; this crate exposes
//! the envelope shape, the [`Diagnose`] trait, and blanket [`Display`] /
//! [`std::error::Error`] impls.

use std::error::Error;
use std::fmt;

/// Per `31 §4.1`. Two variants only.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => f.write_str("error"),
            Severity::Warning => f.write_str("warning"),
        }
    }
}

/// Half-open byte-offset span. Per `31 §4.2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Opaque source identifier. Per `31 §4.2`. Constructors live on the
/// producing crate; this crate exposes only [`SourceId::unknown`] and
/// [`SourceId::as_str`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(SourceIdInner);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SourceIdInner {
    Unknown,
}

impl SourceId {
    pub const fn unknown() -> Self {
        Self(SourceIdInner::Unknown)
    }

    pub fn as_str(&self) -> &str {
        match &self.0 {
            SourceIdInner::Unknown => "<unknown>",
        }
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per `31 §4.2`. `span` is required (no `Option`), `path` removed —
/// per-stage kind variants carry pointer-style paths in their own fields.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}

/// Per `31 §4.4`. Open trait — third-party kind enums implement it and
/// slot into the [`Diagnostic<K>`] envelope.
pub trait Diagnose {
    fn message(&self) -> String;

    fn severity_default(&self) -> Severity;

    fn cause(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

/// Per `31 §4.3`. Consumers construct via per-crate helpers; this crate
/// exposes no [`Diagnostic::new`] or builder.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Diagnostic<K: Diagnose> {
    pub kind: K,
    pub severity: Severity,
    pub location: Option<Location>,
    pub notes: Vec<String>,
}

/// Transparent vector alias per `31 §4.3`.
pub type Diagnostics<K> = Vec<Diagnostic<K>>;

impl<K: Diagnose> fmt::Display for Diagnostic<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.kind.message())?;
        if let Some(loc) = &self.location {
            write!(
                f,
                " (at {} [{}..{}])",
                loc.source, loc.span.start, loc.span.end
            )?;
        }
        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }
        Ok(())
    }
}

impl<K: Diagnose + fmt::Debug> Error for Diagnostic<K> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.kind.cause()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    enum TestKind {
        Bad,
        Warn,
    }

    impl Diagnose for TestKind {
        fn message(&self) -> String {
            match self {
                Self::Bad => "bad".into(),
                Self::Warn => "warn".into(),
            }
        }

        fn severity_default(&self) -> Severity {
            match self {
                Self::Bad => Severity::Error,
                Self::Warn => Severity::Warning,
            }
        }
    }

    fn envelope<K: Diagnose>(kind: K) -> Diagnostic<K> {
        let severity = kind.severity_default();
        Diagnostic {
            kind,
            severity,
            location: None,
            notes: Vec::new(),
        }
    }

    #[test]
    fn severity_is_non_exhaustive_match_compiles() {
        let s = Severity::Warning;
        let label = match s {
            Severity::Error => "e",
            Severity::Warning => "w",
            _ => "?",
        };
        assert_eq!(label, "w");
    }

    #[test]
    fn severity_default_resolves_via_diagnose_trait() {
        assert_eq!(envelope(TestKind::Bad).severity, Severity::Error);
        assert_eq!(envelope(TestKind::Warn).severity, Severity::Warning);
    }

    #[test]
    fn source_id_unknown_is_const_constructible() {
        const ID: SourceId = SourceId::unknown();
        assert_eq!(ID.as_str(), "<unknown>");
    }

    #[test]
    fn source_id_unknown_eq_unknown() {
        assert_eq!(SourceId::unknown(), SourceId::unknown());
    }

    #[test]
    fn source_id_hashes_consistently_with_eq() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let a = SourceId::unknown();
        let b = SourceId::unknown();
        let mut ha = DefaultHasher::new();
        let mut hb = DefaultHasher::new();
        a.hash(&mut ha);
        b.hash(&mut hb);
        assert_eq!(ha.finish(), hb.finish());
    }

    #[test]
    fn span_is_half_open_byte_range() {
        let s = Span { start: 5, end: 10 };
        assert_eq!(s.start, 5);
        assert_eq!(s.end, 10);
    }

    #[test]
    fn location_carries_source_and_span() {
        let loc = Location {
            source: SourceId::unknown(),
            span: Span { start: 0, end: 0 },
        };
        assert_eq!(loc.source, SourceId::unknown());
        assert_eq!(loc.span.start, 0);
    }

    #[test]
    fn diagnostic_envelope_pub_fields() {
        let d = Diagnostic::<TestKind> {
            kind: TestKind::Bad,
            severity: Severity::Error,
            location: None,
            notes: vec!["hint".into()],
        };
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.notes.len(), 1);
    }

    #[test]
    fn display_delegates_to_diagnose_message() {
        let d = envelope(TestKind::Bad);
        assert_eq!(format!("{}", d), "error: bad");
    }

    #[test]
    fn display_does_not_require_kind_debug() {
        // TestKind has no Debug derive; this test compiling is the proof.
        let d = envelope(TestKind::Warn);
        let _ = format!("{}", d);
    }

    #[derive(Debug)]
    struct InnerCause;
    impl fmt::Display for InnerCause {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("inner")
        }
    }
    impl Error for InnerCause {}

    #[derive(Debug)]
    struct WrapKind {
        cause: InnerCause,
    }
    impl Diagnose for WrapKind {
        fn message(&self) -> String {
            "wrap".into()
        }
        fn severity_default(&self) -> Severity {
            Severity::Error
        }
        fn cause(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.cause)
        }
    }

    #[test]
    fn error_impl_chains_to_diagnose_cause() {
        let d = envelope(WrapKind { cause: InnerCause });
        let src = (&d as &dyn Error).source();
        assert!(src.is_some());
    }

    #[derive(Debug)]
    struct BareKind;
    impl Diagnose for BareKind {
        fn message(&self) -> String {
            "bare".into()
        }
        fn severity_default(&self) -> Severity {
            Severity::Error
        }
    }

    #[test]
    fn error_impl_returns_none_when_no_cause() {
        let d = envelope(BareKind);
        let src = (&d as &dyn Error).source();
        assert!(src.is_none());
    }

    #[test]
    fn diagnostics_alias_accepts_vec_methods() {
        let mut v: Diagnostics<TestKind> = Vec::new();
        v.push(envelope(TestKind::Warn));
        assert!(!v.is_empty());
        assert_eq!(v.iter().count(), 1);
    }

    #[test]
    fn diagnose_severity_default_is_overridable_by_caller() {
        let d = Diagnostic::<TestKind> {
            kind: TestKind::Bad,
            severity: Severity::Warning,
            location: None,
            notes: Vec::new(),
        };
        assert_eq!(d.severity, Severity::Warning);
    }
}
